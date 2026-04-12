use alloc::{borrow::Cow, string::ToString, sync::Arc};
use core::{
    ffi::c_int,
    hint::likely,
    sync::atomic::{AtomicBool, Ordering},
    task::Context,
};

use axerrno::{AxError, AxResult};
use axfs::{FS_CONTEXT, FsContext};
use axfs_ng_vfs::{Location, Metadata, NodeFlags, NodeType};
use axpoll::{IoEvents, Pollable};
use axsync::Mutex;
use axtask::future::{block_on, poll_io};
use linux_raw_sys::{
    general::{AT_EMPTY_PATH, AT_FDCWD, AT_SYMLINK_NOFOLLOW},
    ioctl::{
        TCGETS, TCGETS2, TCSETS, TCSETS2, TCSETSF, TCSETSF2, TCSETSW, TCSETSW2, TIOCGPTN, TIOCGPGRP,
        TIOCGWINSZ, TIOCNOTTY, TIOCSPGRP, TIOCSPTLCK, TIOCSWINSZ, TIOCSCTTY,
    },
};

use super::{FileLike, Kstat, get_file_like, memfd::MemfdCreatedFile};
use crate::file::{IoDst, IoSrc};

pub fn with_fs<R>(dirfd: c_int, f: impl FnOnce(&mut FsContext) -> AxResult<R>) -> AxResult<R> {
    let mut fs = FS_CONTEXT.lock();
    if dirfd == AT_FDCWD {
        f(&mut fs)
    } else {
        let dir = Directory::from_fd(dirfd)?.inner.clone();
        f(&mut fs.with_current_dir(dir)?)
    }
}

/// Effective `dirfd` for pathname resolution. Linux ignores `dirfd` when `path` is absolute
/// (starts with `/`); see statx(2) situation 1 and openat(2).
#[inline]
pub fn dirfd_for_path_resolution(dirfd: c_int, path: &str) -> c_int {
    if path.starts_with('/') {
        AT_FDCWD
    } else {
        dirfd
    }
}

pub enum ResolveAtResult {
    File(Location),
    Other(Arc<dyn FileLike>),
}

impl ResolveAtResult {
    pub fn into_file(self) -> Option<Location> {
        match self {
            Self::File(file) => Some(file),
            Self::Other(_) => None,
        }
    }

    pub fn stat(&self) -> AxResult<Kstat> {
        match self {
            Self::File(file) => file.metadata().map(|it| metadata_to_kstat(&it)),
            Self::Other(file_like) => file_like.stat(),
        }
    }
}

/// Resolve an open fd to a VFS [`Location`] for `fstatfs(2)` and similar syscalls.
///
/// Returns [`AxError::InvalidInput`] for fds that are not backed by a path in the VFS
/// (e.g. sockets, pipes), matching Linux `EINVAL` for those cases.
pub fn location_from_fd(fd: c_int) -> AxResult<Location> {
    let file_like = get_file_like(fd)?;
    let f = file_like.as_ref();
    if let Some(file) = f.downcast_ref::<File>() {
        Ok(file.inner().backend()?.location().clone())
    } else if let Some(m) = f.downcast_ref::<MemfdCreatedFile>() {
        Ok(m.inner_file().inner().backend()?.location().clone())
    } else if let Some(dir) = f.downcast_ref::<Directory>() {
        Ok(dir.inner().clone())
    } else {
        Err(AxError::InvalidInput)
    }
}

pub fn resolve_at(dirfd: c_int, path: Option<&str>, flags: u32) -> AxResult<ResolveAtResult> {
    match path {
        Some("") | None => {
            if flags & AT_EMPTY_PATH == 0 {
                return Err(AxError::NotFound);
            }
            let file_like = get_file_like(dirfd)?;
            let f = file_like.clone();
            Ok(if let Some(file) = f.downcast_ref::<File>() {
                ResolveAtResult::File(file.inner().backend()?.location().clone())
            } else if let Some(m) = f.downcast_ref::<MemfdCreatedFile>() {
                ResolveAtResult::File(m.inner_file().inner().backend()?.location().clone())
            } else if let Some(dir) = f.downcast_ref::<Directory>() {
                ResolveAtResult::File(dir.inner().clone())
            } else {
                ResolveAtResult::Other(file_like)
            })
        }
        Some(path) => {
            let dirfd = dirfd_for_path_resolution(dirfd, path);
            with_fs(dirfd, |fs| {
                if flags & AT_SYMLINK_NOFOLLOW != 0 {
                    fs.resolve_no_follow(path)
                } else {
                    fs.resolve(path)
                }
                .map(ResolveAtResult::File)
            })
        }
    }
}

pub fn metadata_to_kstat(metadata: &Metadata) -> Kstat {
    let ty = metadata.node_type as u8;
    let perm = metadata.mode.bits() as u32;
    let mode = ((ty as u32) << 12) | perm;
    Kstat {
        dev: metadata.device,
        ino: metadata.inode,
        mode,
        nlink: metadata.nlink as _,
        uid: metadata.uid,
        gid: metadata.gid,
        size: metadata.size,
        blksize: metadata.block_size as _,
        blocks: metadata.blocks,
        rdev: metadata.rdev,
        atime: metadata.atime,
        mtime: metadata.mtime,
        ctime: metadata.ctime,
    }
}

/// File wrapper for `axfs::fops::File`.
pub struct File {
    inner: axfs::File,
    nonblock: AtomicBool,
}

impl File {
    pub fn new(inner: axfs::File) -> Self {
        Self {
            inner,
            nonblock: AtomicBool::new(false),
        }
    }

    pub fn inner(&self) -> &axfs::File {
        &self.inner
    }

    fn is_blocking(&self) -> bool {
        self.inner.location().flags().contains(NodeFlags::BLOCKING)
    }
}

fn path_for(loc: &Location) -> Cow<'static, str> {
    loc.absolute_path()
        .map_or_else(|_| "<error>".into(), |f| Cow::Owned(f.to_string()))
}

/// Terminal/PTY ioctls handled by [`crate::pseudofs::dev::tty::Tty`]; only valid on character devices.
fn is_tty_driver_ioctl(cmd: u32) -> bool {
    matches!(
        cmd,
        TCGETS
            | TCGETS2
            | TCSETS
            | TCSETSF
            | TCSETSW
            | TCSETS2
            | TCSETSF2
            | TCSETSW2
            | TIOCGPGRP
            | TIOCSPGRP
            | TIOCGWINSZ
            | TIOCSWINSZ
            | TIOCSPTLCK
            | TIOCGPTN
            | TIOCSCTTY
            | TIOCNOTTY
    )
}

impl FileLike for File {
    fn read(&self, dst: &mut IoDst) -> AxResult<usize> {
        let inner = self.inner();
        if likely(self.is_blocking()) {
            inner.read(dst)
        } else {
            block_on(poll_io(self, IoEvents::IN, self.nonblocking(), || {
                inner.read(&mut *dst)
            }))
        }
    }

    fn write(&self, src: &mut IoSrc) -> AxResult<usize> {
        let inner = self.inner();
        if likely(self.is_blocking()) {
            inner.write(src)
        } else {
            block_on(poll_io(self, IoEvents::OUT, self.nonblocking(), || {
                inner.write(&mut *src)
            }))
        }
    }

    fn stat(&self) -> AxResult<Kstat> {
        Ok(metadata_to_kstat(&self.inner().location().metadata()?))
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> AxResult<usize> {
        let loc = self.inner().backend()?.location();
        if loc.node_type() != NodeType::CharacterDevice && is_tty_driver_ioctl(cmd) {
            return Err(AxError::NotATty);
        }
        loc.ioctl(cmd, arg)
    }

    fn set_nonblocking(&self, flag: bool) -> AxResult {
        self.nonblock.store(flag, Ordering::Release);
        Ok(())
    }

    fn nonblocking(&self) -> bool {
        self.nonblock.load(Ordering::Acquire)
    }

    fn path(&self) -> Cow<'_, str> {
        path_for(self.inner.location())
    }

    fn from_fd(fd: c_int) -> AxResult<Arc<Self>>
    where
        Self: Sized + 'static,
    {
        get_file_like(fd)?.downcast_arc().map_err(|any| {
            if any.is::<Directory>() {
                AxError::IsADirectory
            } else {
                AxError::BrokenPipe
            }
        })
    }
}
impl Pollable for File {
    fn poll(&self) -> IoEvents {
        self.inner().location().poll()
    }

    fn register(&self, context: &mut Context<'_>, events: IoEvents) {
        self.inner().location().register(context, events);
    }
}

/// Directory wrapper for `axfs::fops::Directory`.
pub struct Directory {
    inner: Location,
    pub offset: Mutex<u64>,
}

impl Directory {
    pub fn new(inner: Location) -> Self {
        Self {
            inner,
            offset: Mutex::new(0),
        }
    }

    /// Get the inner node of the directory.
    pub fn inner(&self) -> &Location {
        &self.inner
    }
}

impl FileLike for Directory {
    fn read(&self, _dst: &mut IoDst) -> AxResult<usize> {
        Err(AxError::IsADirectory)
    }

    fn write(&self, _src: &mut IoSrc) -> AxResult<usize> {
        Err(AxError::IsADirectory)
    }

    fn stat(&self) -> AxResult<Kstat> {
        Ok(metadata_to_kstat(&self.inner.metadata()?))
    }

    fn path(&self) -> Cow<'_, str> {
        path_for(&self.inner)
    }

    fn from_fd(fd: c_int) -> AxResult<Arc<Self>> {
        get_file_like(fd)?
            .downcast_arc()
            .map_err(|_| AxError::NotADirectory)
    }
}
impl Pollable for Directory {
    fn poll(&self) -> IoEvents {
        IoEvents::IN | IoEvents::OUT
    }

    fn register(&self, _context: &mut Context<'_>, _events: IoEvents) {}
}
