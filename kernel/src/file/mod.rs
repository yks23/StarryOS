pub mod epoll;
pub mod event;
pub(crate) mod flock;
mod fs;
pub(crate) mod memfd;
mod inotify;
pub mod io_uring;
mod net;
mod pidfd;
mod pipe;
pub(crate) mod record_lock;
pub mod signalfd;
pub mod timerfd;

use alloc::{borrow::Cow, sync::Arc};
use core::{ffi::c_int, time::Duration};

use axerrno::{AxError, AxResult};
use axfs::{FS_CONTEXT, OpenOptions};
use axfs_ng_vfs::DeviceId;
use axio::prelude::*;
use axpoll::Pollable;
use axtask::current;
use downcast_rs::{DowncastSync, impl_downcast};
use flatten_objects::FlattenObjects;
use linux_raw_sys::general::{
    RLIMIT_NOFILE, stat, statx, statx_timestamp, STATX_ATIME, STATX_BLOCKS, STATX_BASIC_STATS,
    STATX_CTIME, STATX_GID, STATX_INO, STATX_MODE, STATX_MTIME, STATX_NLINK, STATX_SIZE,
    STATX_TYPE, STATX_UID, STATX__RESERVED,
};
use spin::RwLock;

pub use self::{
    fs::{Directory, File, dirfd_for_path_resolution, location_from_fd, resolve_at, with_fs},
    memfd::MemfdCreatedFile,
    inotify::{FanotifyFd, InotifyFd},
    io_uring::IoUringFd,
    net::Socket,
    pidfd::PidFd,
    pipe::Pipe,
    timerfd::TimerFd,
};
use crate::task::{AX_FILE_LIMIT, AsThread};

#[derive(Debug, Clone, Copy)]
pub struct Kstat {
    pub dev: u64,
    pub ino: u64,
    pub nlink: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub blksize: u32,
    pub blocks: u64,
    pub rdev: DeviceId,
    pub atime: Duration,
    pub mtime: Duration,
    pub ctime: Duration,
}

impl Default for Kstat {
    fn default() -> Self {
        Self {
            dev: 0,
            ino: 1,
            nlink: 1,
            mode: 0,
            uid: 1,
            gid: 1,
            size: 0,
            blksize: 4096,
            blocks: 0,
            rdev: DeviceId::default(),
            atime: Duration::default(),
            mtime: Duration::default(),
            ctime: Duration::default(),
        }
    }
}

impl From<Kstat> for stat {
    fn from(value: Kstat) -> Self {
        // SAFETY: valid for stat
        let mut stat: stat = unsafe { core::mem::zeroed() };
        stat.st_dev = value.dev as _;
        stat.st_ino = value.ino as _;
        stat.st_nlink = value.nlink as _;
        stat.st_mode = value.mode as _;
        stat.st_uid = value.uid as _;
        stat.st_gid = value.gid as _;
        stat.st_size = value.size as _;
        stat.st_blksize = value.blksize as _;
        stat.st_blocks = value.blocks as _;
        stat.st_rdev = value.rdev.0 as _;

        stat.st_atime = value.atime.as_secs() as _;
        stat.st_atime_nsec = value.atime.subsec_nanos() as _;
        stat.st_mtime = value.mtime.as_secs() as _;
        stat.st_mtime_nsec = value.mtime.subsec_nanos() as _;
        stat.st_ctime = value.ctime.as_secs() as _;
        stat.st_ctime_nsec = value.ctime.subsec_nanos() as _;

        stat
    }
}

impl Kstat {
    /// Builds a `statx` honoring Linux `mask` (`STATX_*`): only requested fields are written and
    /// `stx_mask` reports what was filled. Unsupported request bits (e.g. `STATX_BTIME`) are
    /// ignored and omitted from `stx_mask`.
    pub fn into_statx_with_mask(self, mask: u32) -> statx {
        fn time_to_statx(time: &Duration) -> statx_timestamp {
            statx_timestamp {
                tv_sec: time.as_secs() as _,
                tv_nsec: time.subsec_nanos() as _,
                __reserved: 0,
            }
        }

        let mut stx: statx = unsafe { core::mem::zeroed() };
        let want = mask & !STATX__RESERVED;
        let mut filled = 0u32;

        if want & (STATX_TYPE | STATX_MODE) != 0 {
            stx.stx_mode = self.mode as u16;
            stx.stx_attributes = self.mode as u64;
            filled |= want & (STATX_TYPE | STATX_MODE);
        }
        if want & STATX_NLINK != 0 {
            stx.stx_nlink = self.nlink;
            filled |= STATX_NLINK;
        }
        if want & STATX_UID != 0 {
            stx.stx_uid = self.uid;
            filled |= STATX_UID;
        }
        if want & STATX_GID != 0 {
            stx.stx_gid = self.gid;
            filled |= STATX_GID;
        }
        if want & STATX_ATIME != 0 {
            stx.stx_atime = time_to_statx(&self.atime);
            filled |= STATX_ATIME;
        }
        if want & STATX_MTIME != 0 {
            stx.stx_mtime = time_to_statx(&self.mtime);
            filled |= STATX_MTIME;
        }
        if want & STATX_CTIME != 0 {
            stx.stx_ctime = time_to_statx(&self.ctime);
            filled |= STATX_CTIME;
        }
        if want & STATX_INO != 0 {
            stx.stx_ino = self.ino;
            filled |= STATX_INO;
        }
        if want & STATX_SIZE != 0 {
            stx.stx_size = self.size;
            filled |= STATX_SIZE;
        }
        if want & STATX_BLOCKS != 0 {
            stx.stx_blocks = self.blocks;
            filled |= STATX_BLOCKS;
        }
        if want & (STATX_SIZE | STATX_BLOCKS) != 0 {
            stx.stx_blksize = self.blksize;
        }
        if want & STATX_INO != 0 {
            stx.stx_dev_major = (self.dev >> 32) as u32;
            stx.stx_dev_minor = self.dev as u32;
        }
        if want & (STATX_TYPE | STATX_MODE) != 0 {
            stx.stx_rdev_major = self.rdev.major();
            stx.stx_rdev_minor = self.rdev.minor();
        }

        stx.stx_mask = filled;
        stx
    }
}

impl From<Kstat> for statx {
    fn from(value: Kstat) -> Self {
        value.into_statx_with_mask(STATX_BASIC_STATS)
    }
}

pub trait WriteBuf: Write + IoBufMut {}
impl<T: Write + IoBufMut> WriteBuf for T {}
pub type IoDst<'a> = dyn WriteBuf + 'a;

pub trait ReadBuf: Read + IoBuf {}
impl<T: Read + IoBuf> ReadBuf for T {}
pub type IoSrc<'a> = dyn ReadBuf + 'a;

#[allow(dead_code)]
pub trait FileLike: Pollable + DowncastSync {
    fn read(&self, _dst: &mut IoDst) -> AxResult<usize> {
        Err(AxError::InvalidInput)
    }

    fn write(&self, _src: &mut IoSrc) -> AxResult<usize> {
        Err(AxError::InvalidInput)
    }

    fn stat(&self) -> AxResult<Kstat> {
        Ok(Kstat::default())
    }

    fn path(&self) -> Cow<'_, str>;

    fn ioctl(&self, _cmd: u32, _arg: usize) -> AxResult<usize> {
        Err(AxError::NotATty)
    }

    fn nonblocking(&self) -> bool {
        false
    }

    fn set_nonblocking(&self, _nonblocking: bool) -> AxResult {
        Ok(())
    }

    fn from_fd(fd: c_int) -> AxResult<Arc<Self>>
    where
        Self: Sized + 'static,
    {
        get_file_like(fd)?
            .downcast_arc()
            .map_err(|_| AxError::InvalidInput)
    }

    fn add_to_fd_table(self, cloexec: bool) -> AxResult<c_int>
    where
        Self: Sized + 'static,
    {
        add_file_like(Arc::new(self), cloexec)
    }
}
impl_downcast!(sync FileLike);

#[derive(Clone)]
pub struct FileDescriptor {
    pub inner: Arc<dyn FileLike>,
    pub cloexec: bool,
}

scope_local::scope_local! {
    /// The current file descriptor table.
    pub static FD_TABLE: Arc<RwLock<FlattenObjects<FileDescriptor, AX_FILE_LIMIT>>> = Arc::default();
}

/// Get a file-like object by `fd`.
pub fn get_file_like(fd: c_int) -> AxResult<Arc<dyn FileLike>> {
    FD_TABLE
        .read()
        .get(fd as usize)
        .map(|fd| fd.inner.clone())
        .ok_or(AxError::BadFileDescriptor)
}

/// Add a file to the file descriptor table.
pub fn add_file_like(f: Arc<dyn FileLike>, cloexec: bool) -> AxResult<c_int> {
    let max_nofile = current().as_thread().proc_data.rlim.read()[RLIMIT_NOFILE].current;
    let mut table = FD_TABLE.write();
    if table.count() as u64 >= max_nofile {
        return Err(AxError::TooManyOpenFiles);
    }
    let fd = FileDescriptor { inner: f, cloexec };
    Ok(table.add(fd).map_err(|_| AxError::TooManyOpenFiles)? as c_int)
}

/// Like [`add_file_like`], but the new fd is the smallest free number **`>= min_fd`**
/// (`F_DUPFD` / `F_DUPFD_CLOEXEC`, Linux `__get_unused_fd_flags`).
pub fn add_file_like_at_least(f: Arc<dyn FileLike>, cloexec: bool, min_fd: usize) -> AxResult<c_int> {
    if min_fd >= AX_FILE_LIMIT {
        return Err(AxError::InvalidInput);
    }
    let max_nofile = current().as_thread().proc_data.rlim.read()[RLIMIT_NOFILE].current;
    let mut table = FD_TABLE.write();
    if table.count() as u64 >= max_nofile {
        return Err(AxError::TooManyOpenFiles);
    }
    let fd = FileDescriptor { inner: f, cloexec };
    for id in min_fd..AX_FILE_LIMIT {
        if !table.is_assigned(id) {
            return table
                .add_at(id, fd)
                .map_err(|_| AxError::TooManyOpenFiles)
                .map(|i| i as c_int);
        }
    }
    Err(AxError::TooManyOpenFiles)
}

/// Close a file by `fd`.
pub fn close_file_like(fd: c_int) -> AxResult {
    flock::release_fd(fd);
    record_lock::release_fd(fd);
    let f = FD_TABLE
        .write()
        .remove(fd as usize)
        .ok_or(AxError::BadFileDescriptor)?;
    debug!("close_file_like <= count: {}", Arc::strong_count(&f.inner));
    Ok(())
}

pub fn add_stdio(fd_table: &mut FlattenObjects<FileDescriptor, AX_FILE_LIMIT>) -> AxResult<()> {
    assert_eq!(fd_table.count(), 0);
    let cx = FS_CONTEXT.lock();
    let open = |options: &mut OpenOptions| {
        AxResult::Ok(Arc::new(File::new(
            options.open(&cx, "/dev/console")?.into_file()?,
        )))
    };

    let tty_in = open(OpenOptions::new().read(true).write(false))?;
    let tty_out = open(OpenOptions::new().read(false).write(true))?;
    fd_table
        .add(FileDescriptor {
            inner: tty_in,
            cloexec: false,
        })
        .map_err(|_| AxError::TooManyOpenFiles)?;
    fd_table
        .add(FileDescriptor {
            inner: tty_out.clone(),
            cloexec: false,
        })
        .map_err(|_| AxError::TooManyOpenFiles)?;
    fd_table
        .add(FileDescriptor {
            inner: tty_out,
            cloexec: false,
        })
        .map_err(|_| AxError::TooManyOpenFiles)?;

    Ok(())
}
