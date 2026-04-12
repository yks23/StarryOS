use alloc::{format, string::ToString, sync::Arc};
use core::ffi::{c_char, c_int};

use axerrno::{AxError, AxResult};
use axfs::{FS_CONTEXT, FileBackend, FileFlags, OpenOptions, OpenResult};
use axfs_ng_vfs::{DirEntry, FileNode, Location, NodePermission, NodeType, Reference};
use axtask::current;
use bitflags::bitflags;
use linux_raw_sys::general::*;
use spin::RwLock;

use crate::{
    file::{
        Directory, FD_TABLE, File, FileLike, MemfdCreatedFile, Pipe, add_file_like,
        add_file_like_at_least, close_file_like, dirfd_for_path_resolution, get_file_like, with_fs,
    },
    mm::{UserConstPtr, UserPtr, vm_load_string},
    pseudofs::{Device, dev::tty},
    syscall::sys::{sys_getegid, sys_geteuid},
    task::AsThread,
};

/// Linux `fs/open.c` `O_PATH_FLAGS`: with `O_PATH`, only these `open(2)` bits may be set
/// (`build_open_flags`; rejects `O_CREAT`/`O_TRUNC`/`O_EXCL`/`O_APPEND`/… with `O_PATH`).
const OPEN_PATH_FLAG_MASK: u32 = O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC | O_PATH;

fn validate_open_flags(flags: u32) -> AxResult<()> {
    if flags & O_PATH == 0 {
        return Ok(());
    }
    if flags & !OPEN_PATH_FLAG_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

/// Convert open flags to [`OpenOptions`].
fn flags_to_options(flags: c_int, mode: __kernel_mode_t, (uid, gid): (u32, u32)) -> OpenOptions {
    let flags = flags as u32;
    let mut options = OpenOptions::new();
    options.mode(mode).user(uid, gid);
    match flags & 0b11 {
        O_RDONLY => options.read(true),
        O_WRONLY => options.write(true),
        _ => options.read(true).write(true),
    };
    if flags & O_APPEND != 0 {
        options.append(true);
    }
    if flags & O_TRUNC != 0 {
        options.truncate(true);
    }
    if flags & O_CREAT != 0 {
        options.create(true);
    }
    if flags & O_PATH != 0 {
        options.path(true);
    }
    if flags & O_EXCL != 0 {
        options.create_new(true);
    }
    if flags & O_DIRECTORY != 0 {
        options.directory(true);
    }
    if flags & O_NOFOLLOW != 0 {
        options.no_follow(true);
    }
    if flags & O_DIRECT != 0 {
        options.direct(true);
    }
    options
}

fn add_to_fd(result: OpenResult, flags: u32) -> AxResult<i32> {
    let f: Arc<dyn FileLike> = match result {
        OpenResult::File(mut file) => {
            // /dev/xx handling
            if let Ok(device) = file.location().entry().downcast::<Device>() {
                let inner = device.inner().as_any();
                if let Some(ptmx) = inner.downcast_ref::<tty::Ptmx>() {
                    // Opening /dev/ptmx creates a new pseudo-terminal
                    let (master, pty_number) = ptmx.create_pty()?;
                    // TODO: this is cursed
                    let pts = FS_CONTEXT.lock().resolve("/dev/pts")?;
                    let entry = DirEntry::new_file(
                        FileNode::new(master),
                        NodeType::CharacterDevice,
                        Reference::new(Some(pts.entry().clone()), pty_number.to_string()),
                    );
                    let loc = Location::new(file.location().mountpoint().clone(), entry);
                    file = axfs::File::new(FileBackend::Direct(loc), file.flags());
                } else if inner.is::<tty::CurrentTty>() {
                    let term = current()
                        .as_thread()
                        .proc_data
                        .proc
                        .group()
                        .session()
                        .terminal()
                        .ok_or(AxError::NotFound)?;
                    let path = if term.is::<tty::NTtyDriver>() {
                        "/dev/console".to_string()
                    } else if let Some(pts) = term.downcast_ref::<tty::PtyDriver>() {
                        format!("/dev/pts/{}", pts.pty_number())
                    } else {
                        panic!("unknown terminal type")
                    };
                    let loc = FS_CONTEXT.lock().resolve(&path)?;
                    file = axfs::File::new(FileBackend::Direct(loc), file.flags());
                }
            }
            Arc::new(File::new(file))
        }
        OpenResult::Dir(dir) => Arc::new(Directory::new(dir)),
    };
    if flags & O_NONBLOCK != 0 {
        f.set_nonblocking(true)?;
    }
    add_file_like(f, flags & O_CLOEXEC != 0)
}

/// Linux `fcntl(F_SETFL)` flags allowed in `arg` (see `man 2 fcntl` / kernel `SETFL_MASK`).
const F_SETFL_MASK: u32 = O_APPEND | O_NONBLOCK | O_DIRECT | O_NOATIME | FASYNC | O_DSYNC;

/// Maps [`axfs::FileFlags`] to Linux `open(2)`/`fcntl(F_GETFL)` access + `O_APPEND`/`O_PATH` bits.
fn axfs_flags_to_linux_open_bits(ff: FileFlags) -> c_int {
    let mut ret: c_int = 0;
    if ff.contains(FileFlags::PATH) {
        ret |= O_PATH as c_int;
    }
    let read = ff.contains(FileFlags::READ);
    let write = ff.contains(FileFlags::WRITE);
    if ff.contains(FileFlags::APPEND) {
        ret |= O_APPEND as c_int;
    }
    match (read, write) {
        (true, false) => ret |= O_RDONLY as c_int,
        (false, true) => ret |= O_WRONLY as c_int,
        (true, true) => ret |= O_RDWR as c_int,
        (false, false) => {
            if ff.contains(FileFlags::PATH) {
                ret |= O_RDONLY as c_int;
            }
        }
    }
    ret
}

fn f_getfl_for_file_like(f: &Arc<dyn FileLike>) -> AxResult<c_int> {
    let mut ret: c_int = if let Some(file) = f.downcast_ref::<File>() {
        axfs_flags_to_linux_open_bits(file.inner().flags())
    } else if let Some(m) = f.downcast_ref::<MemfdCreatedFile>() {
        axfs_flags_to_linux_open_bits(m.inner_file().inner().flags())
    } else {
        let mut r = 0;
        let perm = NodePermission::from_bits_truncate(f.stat()?.mode as _);
        let read = perm.contains(NodePermission::OWNER_READ);
        let write = perm.contains(NodePermission::OWNER_WRITE);
        match (read, write) {
            (true, true) => r |= O_RDWR as c_int,
            (true, false) => r |= O_RDONLY as c_int,
            (false, true) => r |= O_WRONLY as c_int,
            (false, false) => {}
        }
        r
    };
    if f.nonblocking() {
        ret |= O_NONBLOCK as c_int;
    }
    Ok(ret)
}

fn f_setfl_rest_for_axfs_file(file: &File, rest: u32) -> AxResult<()> {
    if rest & (O_DIRECT | O_NOATIME | FASYNC | O_DSYNC) != 0 {
        return Err(AxError::OperationNotSupported);
    }
    let append = file.inner().flags().contains(FileFlags::APPEND);
    let want_append = rest & O_APPEND != 0;
    if want_append != append {
        return Err(AxError::OperationNotSupported);
    }
    Ok(())
}

/// Open or create a file.
/// fd: file descriptor
/// filename: file path to be opened or created
/// flags: open flags
/// mode: see man 7 inode
/// return new file descriptor if succeed, or return -1.
pub fn sys_openat(
    dirfd: c_int,
    path: *const c_char,
    flags: i32,
    mode: __kernel_mode_t,
) -> AxResult<isize> {
    // Linux do_sys_open: build_open_flags before getname/copy of pathname (EINVAL for bad flag
    // combinations before EFAULT on bad path pointer; issue-323; same theme as issue-318/issue-305).
    validate_open_flags(flags as u32)?;

    let path = vm_load_string(path)?;
    // Linux rejects empty pathname with EINVAL before open lookup (issue-405; issue-393 theme;
    // bad flags rejected first, issue-323; O_PATH details issue-259).
    if path.is_empty() {
        return Err(AxError::InvalidInput);
    }
    debug!("sys_openat <= {dirfd} {path:?} {flags:#o} {mode:#o}");

    let mode = mode & !current().as_thread().proc_data.umask();

    let options = flags_to_options(flags, mode, (sys_geteuid()? as _, sys_getegid()? as _));
    let dirfd = dirfd_for_path_resolution(dirfd, path.as_str());
    with_fs(dirfd, |fs| options.open(fs, path))
        .and_then(|it| add_to_fd(it, flags as _))
        .map(|fd| fd as isize)
}

/// Open a file by `filename` and insert it into the file descriptor table.
///
/// Return its index in the file table (`fd`). Return `EMFILE` if it already
/// has the maximum number of files open.
#[cfg(target_arch = "x86_64")]
pub fn sys_open(path: *const c_char, flags: i32, mode: __kernel_mode_t) -> AxResult<isize> {
    sys_openat(AT_FDCWD as _, path, flags, mode)
}

pub fn sys_close(fd: c_int) -> AxResult<isize> {
    debug!("sys_close <= {fd}");
    close_file_like(fd)?;
    Ok(0)
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    struct CloseRangeFlags: u32 {
        const UNSHARE = 1 << 1;
        const CLOEXEC = 1 << 2;
    }
}

pub fn sys_close_range(first: i32, last: i32, flags: u32) -> AxResult<isize> {
    // Linux __sys_close_range: reject unknown flag bits before first/last range (EINVAL ordering;
    // issue-324; same theme as issue-323/issue-317).
    let flags = CloseRangeFlags::from_bits(flags).ok_or(AxError::InvalidInput)?;
    if first < 0 || last < first {
        return Err(AxError::InvalidInput);
    }
    debug!("sys_close_range <= fds: [{first}, {last}], flags: {flags:?}");
    if flags.contains(CloseRangeFlags::UNSHARE) {
        // Linux `CLOSE_RANGE_UNSHARE`: private FD table for this task group slot (see `clone` without
        // `CLONE_FILES`). Cannot use `scope_mut(..).write().clone_from(&FD_TABLE.read())` here:
        // that deadlocks when the active table is the same `Arc` as the scope slot. Snapshot under
        // a read lock, then replace the slot with a fresh `Arc` like `!CLONE_FILES` does for a child.
        let curr = current();
        let mut scope = curr.as_thread().proc_data.scope.write();
        let snapshot = FD_TABLE.read().clone();
        *FD_TABLE.scope_mut(&mut scope) = Arc::new(RwLock::new(snapshot));
    }

    let cloexec = flags.contains(CloseRangeFlags::CLOEXEC);
    let mut fd_table = FD_TABLE.write();
    if let Some(max_index) = fd_table.ids().next_back() {
        for fd in first..=last.min(max_index as i32) {
            if cloexec {
                if let Some(f) = fd_table.get_mut(fd as _) {
                    f.cloexec = true;
                }
            } else {
                fd_table.remove(fd as _);
            }
        }
    }

    Ok(0)
}

fn dup_fd(old_fd: c_int, cloexec: bool, min_fd: usize) -> AxResult<isize> {
    let f = get_file_like(old_fd)?;
    // Linux fd numbers are `int`; do not truncate `usize`→`c_int`→`usize` (e.g. `0x1_0000_0000` → 0,
    // bypassing `add_file_like_at_least` bounds). `add_file_like_at_least` enforces `AX_FILE_LIMIT`
    // (issue-410; `file/mod.rs`).
    if min_fd > c_int::MAX as usize {
        return Err(AxError::InvalidInput);
    }
    let new_fd = add_file_like_at_least(f, cloexec, min_fd)?;
    Ok(new_fd as _)
}

pub fn sys_dup(old_fd: c_int) -> AxResult<isize> {
    debug!("sys_dup <= {old_fd}");
    dup_fd(old_fd, false, 0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_dup2(old_fd: c_int, new_fd: c_int) -> AxResult<isize> {
    if old_fd == new_fd {
        get_file_like(new_fd)?;
        return Ok(new_fd as _);
    }
    sys_dup3(old_fd, new_fd, 0)
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Dup3Flags: c_int {
        const O_CLOEXEC = O_CLOEXEC as _; // Close on exec
    }
}

pub fn sys_dup3(old_fd: c_int, new_fd: c_int, flags: c_int) -> AxResult<isize> {
    // Linux `do_dup3`: `oldfd == newfd` → EINVAL before rejecting unknown `flags` bits (issue-341;
    // same theme as close_range issue-324 / openat issue-323).
    if old_fd == new_fd {
        return Err(AxError::InvalidInput);
    }

    let flags = Dup3Flags::from_bits(flags).ok_or(AxError::InvalidInput)?;
    debug!("sys_dup3 <= old_fd: {old_fd}, new_fd: {new_fd}, flags: {flags:?}");

    let mut fd_table = FD_TABLE.write();
    let mut f = fd_table
        .get(old_fd as _)
        .cloned()
        .ok_or(AxError::BadFileDescriptor)?;
    f.cloexec = flags.contains(Dup3Flags::O_CLOEXEC);

    fd_table.remove(new_fd as _);
    fd_table
        .add_at(new_fd as _, f)
        .map_err(|_| AxError::BadFileDescriptor)?;

    Ok(new_fd as _)
}

pub fn sys_fcntl(fd: c_int, cmd: c_int, arg: usize) -> AxResult<isize> {
    debug!("sys_fcntl <= fd: {fd} cmd: {cmd} arg: {arg}");

    match cmd as u32 {
        F_DUPFD => dup_fd(fd, false, arg),
        F_DUPFD_CLOEXEC => dup_fd(fd, true, arg),
        F_SETLK | F_OFD_SETLK => {
            // Linux do_fcntl: fget(fd) before copy_from_user(flock) (EBADF before EFAULT).
            get_file_like(fd)?;
            let fl = *UserConstPtr::<flock64>::from(arg).get_as_ref()?;
            crate::file::record_lock::sys_fcntl_setlk(fd, false, &fl)
        }
        F_SETLKW | F_OFD_SETLKW => {
            get_file_like(fd)?;
            let fl = *UserConstPtr::<flock64>::from(arg).get_as_ref()?;
            crate::file::record_lock::sys_fcntl_setlk(fd, true, &fl)
        }
        F_GETLK | F_OFD_GETLK => {
            get_file_like(fd)?;
            let ptr = UserPtr::<flock64>::from(arg);
            let fl = ptr.get_as_mut()?;
            crate::file::record_lock::sys_fcntl_getlk(fd, fl)
        }
        F_SETFL => {
            let arg = arg as u32;
            if arg & !F_SETFL_MASK != 0 {
                return Err(AxError::InvalidInput);
            }
            let f = get_file_like(fd)?;
            f.set_nonblocking(arg & O_NONBLOCK != 0)?;
            let rest = arg & !O_NONBLOCK;
            if rest == 0 {
                return Ok(0);
            }
            if let Some(file) = f.downcast_ref::<File>() {
                f_setfl_rest_for_axfs_file(file, rest)?;
            } else if let Some(m) = f.downcast_ref::<MemfdCreatedFile>() {
                f_setfl_rest_for_axfs_file(m.inner_file(), rest)?;
            } else {
                // Pipe/socket/etc.: Linux only allows `O_NONBLOCK` here (issue-252).
                return Err(AxError::InvalidInput);
            }
            Ok(0)
        }
        F_GETFL => {
            let f = get_file_like(fd)?;
            f_getfl_for_file_like(&f).map(|r| r as isize)
        }
        F_GETFD => {
            let cloexec = FD_TABLE
                .read()
                .get(fd as _)
                .ok_or(AxError::BadFileDescriptor)?
                .cloexec;
            Ok(if cloexec { FD_CLOEXEC as _ } else { 0 })
        }
        F_SETFD => {
            let cloexec = arg & FD_CLOEXEC as usize != 0;
            FD_TABLE
                .write()
                .get_mut(fd as _)
                .ok_or(AxError::BadFileDescriptor)?
                .cloexec = cloexec;
            Ok(0)
        }
        F_GETPIPE_SZ => {
            let pipe = Pipe::from_fd(fd)?;
            Ok(pipe.capacity() as _)
        }
        F_SETPIPE_SZ => {
            let pipe = Pipe::from_fd(fd)?;
            pipe.resize(arg)?;
            Ok(0)
        }
        _ => {
            warn!("unsupported fcntl parameters: cmd: {cmd}");
            Err(AxError::InvalidInput)
        }
    }
}

pub fn sys_flock(fd: c_int, operation: c_int) -> AxResult<isize> {
    debug!("flock <= fd: {fd}, operation: {operation}");
    crate::file::flock::sys_flock(fd, operation)
}
