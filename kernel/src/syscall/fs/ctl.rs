use alloc::{
    collections::VecDeque,
    ffi::CString,
    format, string::String, sync::Arc,
    vec, vec::Vec,
};
use core::{
    ffi::{c_char, c_int},
    mem::offset_of,
    time::Duration,
};

use axerrno::{AxError, AxResult};
use axfs::{FS_CONTEXT, FsContext};
use axfs_ng_vfs::{
    Location, MetadataUpdate, NodePermission, NodeType,
    path::{Path, DOT, DOTDOT},
};
use axhal::time::{monotonic_time_nanos, wall_time};
use axtask::current;
use linux_raw_sys::{
    general::*,
    ioctl::{FIONBIO, TIOCGWINSZ},
};
use starry_vm::{VmPtr, vm_write_slice};

use crate::{
    file::{
        Directory, FileLike, dirfd_for_path_resolution, get_file_like, location_from_fd,
        resolve_at, with_fs,
    },
    mm::vm_load_string,
    task::AsThread,
    time::TimeValueLike,
};

/// The ioctl() system call manipulates the underlying device parameters
/// of special files.
pub fn sys_ioctl(fd: i32, cmd: u32, arg: usize) -> AxResult<isize> {
    debug!("sys_ioctl <= fd: {fd}, cmd: {cmd}, arg: {arg}");
    let f = get_file_like(fd)?;
    if cmd == FIONBIO {
        // Linux: third arg is `int *`; any non-zero value enables O_NONBLOCK (not limited to 0/1).
        let val = (arg as *const c_int).vm_read()?;
        f.set_nonblocking(val != 0)?;
        return Ok(0);
    }
    f.ioctl(cmd, arg)
        .map(|result| result as isize)
        .inspect_err(|err| {
            if *err == AxError::NotATty {
                // glibc probes TIOCGWINSZ on many fds; on a non-tty this still
                // fails with ENOTTY (we surface `NotATty`) — same as Linux.
                // Here we only skip the warn below to avoid log spam; `inspect_err`
                // does not alter the `Result`, so the syscall still returns failure.
                if cmd == TIOCGWINSZ {
                    return;
                }
                warn!("Unsupported ioctl command: {cmd} for fd: {fd}");
            }
        })
}

pub fn sys_chdir(path: *const c_char) -> AxResult<isize> {
    let path = vm_load_string(path)?;
    debug!("sys_chdir <= path: {path}");

    let mut fs = FS_CONTEXT.lock();
    let entry = fs.resolve(path)?;
    fs.set_current_dir(entry)?;
    Ok(0)
}

pub fn sys_fchdir(dirfd: i32) -> AxResult<isize> {
    debug!("sys_fchdir <= dirfd: {dirfd}");

    let entry = with_fs(dirfd, |fs| Ok(fs.current_dir().clone()))?;
    FS_CONTEXT.lock().set_current_dir(entry)?;
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_mkdir(path: *const c_char, mode: u32) -> AxResult<isize> {
    sys_mkdirat(AT_FDCWD, path, mode)
}

pub fn sys_chroot(path: *const c_char) -> AxResult<isize> {
    let path = vm_load_string(path)?;
    debug!("sys_chroot <= path: {path}");

    let mut fs = FS_CONTEXT.lock();
    let loc = fs.resolve(path)?;
    if loc.node_type() != NodeType::Directory {
        return Err(AxError::NotADirectory);
    }
    *fs = FsContext::new(loc);
    Ok(0)
}

pub fn sys_mkdirat(dirfd: i32, path: *const c_char, mode: u32) -> AxResult<isize> {
    let path = vm_load_string(path)?;
    debug!("sys_mkdirat <= dirfd: {dirfd}, path: {path}, mode: {mode}");

    if dirfd != AT_FDCWD && !path.starts_with('/') {
        let _ = Directory::from_fd(dirfd)?;
    }

    let mode = mode & !current().as_thread().proc_data.umask();
    let mode = NodePermission::from_bits_truncate(mode as u16);

    let dirfd = dirfd_for_path_resolution(dirfd, path.as_str());
    with_fs(dirfd, |fs| {
        fs.create_dir(path, mode)?;
        Ok(0)
    })
}

// Directory buffer for getdents64 syscall
struct DirBuffer {
    buf: Vec<u8>,
    offset: usize,
}

impl DirBuffer {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0; len],
            offset: 0,
        }
    }

    fn remaining_space(&self) -> usize {
        self.buf.len().saturating_sub(self.offset)
    }

    fn write_entry(&mut self, d_ino: u64, d_off: i64, d_type: NodeType, name: &[u8]) -> bool {
        const NAME_OFFSET: usize = offset_of!(linux_dirent64, d_name);

        let len = NAME_OFFSET + name.len() + 1;
        // alignment
        let len = len.next_multiple_of(align_of::<linux_dirent64>());
        if self.remaining_space() < len {
            return false;
        }

        // FIXME: safety
        unsafe {
            let entry_ptr = self.buf.as_mut_ptr().add(self.offset);
            entry_ptr.cast::<linux_dirent64>().write(linux_dirent64 {
                d_ino,
                d_off,
                d_reclen: len as _,
                d_type: d_type as _,
                d_name: Default::default(),
            });

            let name_ptr = entry_ptr.add(NAME_OFFSET);
            name_ptr.copy_from_nonoverlapping(name.as_ptr(), name.len());
            name_ptr.add(name.len()).write(0);
        }

        self.offset += len;
        true
    }
}

pub fn sys_getdents64(fd: i32, buf: *mut u8, len: usize) -> AxResult<isize> {
    debug!("sys_getdents64 <= fd: {fd}, buf: {buf:?}, len: {len}");

    let mut buffer = DirBuffer::new(len);

    let dir = Directory::from_fd(fd)?;
    let mut dir_offset = dir.offset.lock();

    let mut has_remaining = false;

    dir.inner()
        .read_dir(*dir_offset, &mut |name: &str, ino, node_type, offset| {
            has_remaining = true;
            if !buffer.write_entry(ino, offset as _, node_type, name.as_bytes()) {
                return false;
            }
            *dir_offset = offset;
            true
        })?;

    if has_remaining && buffer.offset == 0 {
        return Err(AxError::InvalidInput);
    }

    vm_write_slice(buf, &buffer.buf)?;

    Ok(buffer.offset as _)
}

/// create a link from new_path to old_path
/// old_path: old file path
/// new_path: new file path
/// flags: link flags
/// return value: return 0 when success, else return -1.
pub fn sys_linkat(
    old_dirfd: c_int,
    old_path: *const c_char,
    new_dirfd: c_int,
    new_path: *const c_char,
    flags: u32,
) -> AxResult<isize> {
    // Linux linkat: reject unknown flags before copy_from_user paths (EINVAL before EFAULT).
    // Linux VALID_LINKAT_FLAGS: AT_EMPTY_PATH | AT_SYMLINK_FOLLOW (see namei.c).
    const VALID_LINKAT_FLAGS: u32 = AT_EMPTY_PATH | AT_SYMLINK_FOLLOW;
    if flags & !VALID_LINKAT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    // resolve_at uses AT_SYMLINK_NOFOLLOW; linkat(2) uses AT_SYMLINK_FOLLOW (inverse sense).
    let mut resolve_flags = flags & AT_EMPTY_PATH;
    if flags & AT_SYMLINK_FOLLOW == 0 {
        resolve_flags |= AT_SYMLINK_NOFOLLOW;
    }

    let old_path = old_path.nullable().map(vm_load_string).transpose()?;

    if old_dirfd != AT_FDCWD {
        let need_old_dir = match old_path.as_deref() {
            None => false,
            Some(p) => !p.starts_with('/'),
        };
        if need_old_dir {
            let _ = Directory::from_fd(old_dirfd)?;
        }
    }

    let new_path = vm_load_string(new_path)?;
    if new_dirfd != AT_FDCWD && !new_path.starts_with('/') {
        let _ = Directory::from_fd(new_dirfd)?;
    }
    debug!(
        "sys_linkat <= old_dirfd: {old_dirfd}, old_path: {old_path:?}, new_dirfd: {new_dirfd}, \
         new_path: {new_path}, flags: {flags}"
    );

    let old = resolve_at(old_dirfd, old_path.as_deref(), resolve_flags)?
        .into_file()
        .ok_or(AxError::BadFileDescriptor)?;
    if old.is_dir() {
        return Err(AxError::OperationNotPermitted);
    }
    let new_dirfd_eff = dirfd_for_path_resolution(new_dirfd, new_path.as_str());
    let (new_dir, new_name) =
        with_fs(new_dirfd_eff, |fs| fs.resolve_nonexistent(Path::new(&new_path)))?;

    new_dir.link(new_name, &old)?;
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_link(old_path: *const c_char, new_path: *const c_char) -> AxResult<isize> {
    sys_linkat(AT_FDCWD, old_path, AT_FDCWD, new_path, 0)
}

/// remove link of specific file (can be used to delete file)
/// dir_fd: the directory of link to be removed
/// path: the name of link to be removed
/// flags: can be 0 or AT_REMOVEDIR
/// return 0 when success, else return -1
pub fn sys_unlinkat(dirfd: i32, path: *const c_char, flags: usize) -> AxResult<isize> {
    // Linux unlinkat(2): reject invalid flags before copy_from_user(pathname) (EINVAL before EFAULT).
    // flags must be 0 or AT_REMOVEDIR only.
    if flags != 0 && flags != AT_REMOVEDIR as usize {
        return Err(AxError::InvalidInput);
    }

    let path = vm_load_string(path)?;

    debug!("sys_unlinkat <= dirfd: {dirfd}, path: {path:?}, flags: {flags}");

    if dirfd != AT_FDCWD && !path.starts_with('/') {
        let _ = Directory::from_fd(dirfd)?;
    }

    let dirfd = dirfd_for_path_resolution(dirfd, path.as_str());
    with_fs(dirfd, |fs| {
        if flags == AT_REMOVEDIR as _ {
            fs.remove_dir(path)?;
        } else {
            fs.remove_file(path)?;
        }
        Ok(0)
    })
}

#[cfg(target_arch = "x86_64")]
pub fn sys_rmdir(path: *const c_char) -> AxResult<isize> {
    sys_unlinkat(AT_FDCWD, path, AT_REMOVEDIR as _)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_unlink(path: *const c_char) -> AxResult<isize> {
    sys_unlinkat(AT_FDCWD, path, 0)
}

pub fn sys_getcwd(buf: *mut u8, size: isize) -> AxResult<isize> {
    // Linux getcwd(2): negative or zero `size` is EINVAL (not ERANGE). ERANGE is for a non-zero
    // buffer that is too small for the path + NUL (issue-194).
    let size: usize = size.try_into().map_err(|_| AxError::InvalidInput)?;
    if size == 0 {
        return Err(AxError::InvalidInput);
    }
    if buf.is_null() {
        return Err(AxError::BadAddress);
    }

    let cwd = FS_CONTEXT.lock().current_dir().absolute_path()?;
    debug!("sys_getcwd => cwd: {cwd}");

    let cwd = CString::new(cwd.as_str()).map_err(|_| AxError::InvalidInput)?;
    let cwd = cwd.as_bytes_with_nul();

    if cwd.len() <= size {
        vm_write_slice(buf, cwd)?;
        // Linux SYS_getcwd returns the length written (including terminating NUL).
        Ok(cwd.len() as isize)
    } else {
        Err(AxError::OutOfRange)
    }
}

#[cfg(target_arch = "x86_64")]
pub fn sys_symlink(target: *const c_char, linkpath: *const c_char) -> AxResult<isize> {
    sys_symlinkat(target, AT_FDCWD, linkpath)
}

pub fn sys_symlinkat(
    target: *const c_char,
    new_dirfd: i32,
    linkpath: *const c_char,
) -> AxResult<isize> {
    let target = vm_load_string(target)?;
    let linkpath = vm_load_string(linkpath)?;
    debug!("sys_symlinkat <= target: {target:?}, new_dirfd: {new_dirfd}, linkpath: {linkpath:?}");

    if new_dirfd != AT_FDCWD && !linkpath.starts_with('/') {
        let _ = Directory::from_fd(new_dirfd)?;
    }

    let new_dirfd = dirfd_for_path_resolution(new_dirfd, linkpath.as_str());
    with_fs(new_dirfd, |fs| {
        fs.symlink(target, linkpath)?;
        Ok(0)
    })
}

#[cfg(target_arch = "x86_64")]
pub fn sys_readlink(path: *const c_char, buf: *mut u8, size: usize) -> AxResult<isize> {
    sys_readlinkat(AT_FDCWD, path, buf, size)
}

pub fn sys_readlinkat(
    dirfd: i32,
    path: *const c_char,
    buf: *mut u8,
    size: usize,
) -> AxResult<isize> {
    // Linux readlinkat(2): bufsiz must be positive; EINVAL before path resolution.
    if size == 0 {
        return Err(AxError::InvalidInput);
    }

    let path = vm_load_string(path)?;

    debug!("sys_readlinkat <= dirfd: {dirfd}, path: {path:?}");

    if dirfd != AT_FDCWD && !path.starts_with('/') {
        let _ = Directory::from_fd(dirfd)?;
    }

    let dirfd = dirfd_for_path_resolution(dirfd, path.as_str());
    with_fs(dirfd, |fs| {
        let entry = fs.resolve_no_follow(path)?;
        let link = entry.read_link()?;
        let read = size.min(link.len());
        vm_write_slice(buf, &link.as_bytes()[..read])?;
        Ok(read as isize)
    })
}

#[cfg(target_arch = "x86_64")]
pub fn sys_chown(path: *const c_char, uid: i32, gid: i32) -> AxResult<isize> {
    sys_fchownat(AT_FDCWD, path, uid, gid, 0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_lchown(path: *const c_char, uid: i32, gid: i32) -> AxResult<isize> {
    use linux_raw_sys::general::AT_SYMLINK_NOFOLLOW;
    sys_fchownat(AT_FDCWD, path, uid, gid, AT_SYMLINK_NOFOLLOW)
}

pub fn sys_fchown(fd: i32, uid: i32, gid: i32) -> AxResult<isize> {
    sys_fchownat(fd, core::ptr::null(), uid, gid, AT_EMPTY_PATH)
}

pub fn sys_fchownat(
    dirfd: i32,
    path: *const c_char,
    uid: i32,
    gid: i32,
    flags: u32,
) -> AxResult<isize> {
    // Linux fchownat: reject unknown flags before copy_from_user(pathname) (EINVAL before EFAULT).
    const VALID_FCHOWNAT_FLAGS: u32 = AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW;
    if flags & !VALID_FCHOWNAT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }

    let path = path.nullable().map(vm_load_string).transpose()?;
    if dirfd != AT_FDCWD {
        let needs_dir = match path.as_deref() {
            None => false,
            Some(p) => !p.starts_with('/'),
        };
        if needs_dir {
            let _ = Directory::from_fd(dirfd)?;
        }
    }

    let loc = resolve_at(dirfd, path.as_deref(), flags)?
        .into_file()
        .ok_or(AxError::BadFileDescriptor)?;
    let meta = loc.metadata()?;

    let mut mode = meta.mode;
    // chown always clears the setuid bits
    mode.remove(NodePermission::SET_UID);
    // chown also removes the setgid bits if group-executable
    if mode.contains(NodePermission::GROUP_EXEC) {
        mode.remove(NodePermission::SET_GID);
    }

    let uid = if uid == -1 { meta.uid } else { uid as _ };
    let gid = if gid == -1 { meta.gid } else { gid as _ };
    loc.update_metadata(MetadataUpdate {
        owner: Some((uid, gid)),
        mode: Some(mode),
        ..Default::default()
    })?;
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_chmod(path: *const c_char, mode: u32) -> AxResult<isize> {
    sys_fchmodat(AT_FDCWD, path, mode, 0)
}

pub fn sys_fchmod(fd: i32, mode: u32) -> AxResult<isize> {
    sys_fchmodat(fd, core::ptr::null(), mode, AT_EMPTY_PATH)
}

pub fn sys_fchmodat(dirfd: i32, path: *const c_char, mode: u32, flags: u32) -> AxResult<isize> {
    // Linux fchmodat: reject unknown flags before copy_from_user(pathname) (EINVAL before EFAULT).
    const VALID_FCHMODAT_FLAGS: u32 = AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW;
    if flags & !VALID_FCHMODAT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }

    let path = path.nullable().map(vm_load_string).transpose()?;
    if dirfd != AT_FDCWD {
        let needs_dir = match path.as_deref() {
            None => false,
            Some(p) => !p.starts_with('/'),
        };
        if needs_dir {
            let _ = Directory::from_fd(dirfd)?;
        }
    }

    resolve_at(dirfd, path.as_deref(), flags)?
        .into_file()
        .ok_or(AxError::BadFileDescriptor)?
        .update_metadata(MetadataUpdate {
            mode: Some(NodePermission::from_bits_truncate(mode as u16)),
            ..Default::default()
        })?;
    Ok(0)
}

fn update_times(
    dirfd: i32,
    path: *const c_char,
    atime: Option<Duration>,
    mtime: Option<Duration>,
    flags: u32,
) -> AxResult<()> {
    let path = path.nullable().map(vm_load_string).transpose()?;
    if dirfd != AT_FDCWD {
        let needs_dir = match path.as_deref() {
            None => false,
            Some(p) => !p.starts_with('/'),
        };
        if needs_dir {
            let _ = Directory::from_fd(dirfd)?;
        }
    }

    resolve_at(dirfd, path.as_deref(), flags)?
        .into_file()
        .ok_or(AxError::BadFileDescriptor)?
        .update_metadata(MetadataUpdate {
            atime,
            mtime,
            ..Default::default()
        })?;
    Ok(())
}

#[cfg(target_arch = "x86_64")]
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct utimbuf {
    actime: linux_raw_sys::general::__kernel_old_time_t,
    modtime: linux_raw_sys::general::__kernel_old_time_t,
}

#[cfg(target_arch = "x86_64")]
pub fn sys_utime(path: *const c_char, times: *const utimbuf) -> AxResult<isize> {
    let (atime, mtime) = if let Some(times) = times.nullable() {
        // FIXME: AnyBitPattern
        let times = unsafe { times.vm_read_uninit()?.assume_init() };
        (
            Duration::from_secs(times.actime as _),
            Duration::from_secs(times.modtime as _),
        )
    } else {
        let time = wall_time();
        (time, time)
    };
    update_times(AT_FDCWD, path, Some(atime), Some(mtime), 0)?;
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_utimes(
    path: *const c_char,
    times: *const [linux_raw_sys::general::timeval; 2],
) -> AxResult<isize> {
    let (atime, mtime) = if let Some(times) = times.nullable() {
        // FIXME: AnyBitPattern
        let [atime, mtime] = unsafe { times.vm_read_uninit()?.assume_init() };
        (atime.try_into_time_value()?, mtime.try_into_time_value()?)
    } else {
        let time = wall_time();
        (time, time)
    };
    update_times(AT_FDCWD, path, Some(atime), Some(mtime), 0)?;
    Ok(0)
}

pub fn sys_utimensat(
    dirfd: i32,
    path: *const c_char,
    times: *const [timespec; 2],
    mut flags: u32,
) -> AxResult<isize> {
    if path.is_null() {
        flags |= AT_EMPTY_PATH;
    }
    // Linux VALID_UTIMENSAT_FLAGS (see utimes.c): AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH.
    const VALID_UTIMENSAT_FLAGS: u32 = AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH;
    if flags & !VALID_UTIMENSAT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    fn utime_to_duration(time: &timespec) -> Option<AxResult<Duration>> {
        match time.tv_nsec {
            val if val == UTIME_OMIT as _ => None,
            val if val == UTIME_NOW as _ => Some(Ok(wall_time())),
            _ => Some(time.try_into_time_value()),
        }
    }

    let (atime, mtime) = if let Some(times) = times.nullable() {
        // FIXME: AnyBitPattern
        let [atime, mtime] = unsafe { times.vm_read_uninit()?.assume_init() };
        (
            utime_to_duration(&atime).transpose()?,
            utime_to_duration(&mtime).transpose()?,
        )
    } else {
        let time = wall_time();
        (Some(time), Some(time))
    };
    if atime.is_none() && mtime.is_none() {
        return Ok(0);
    }

    update_times(dirfd, path, atime, mtime, flags)?;
    Ok(0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_rename(old_path: *const c_char, new_path: *const c_char) -> AxResult<isize> {
    sys_renameat(AT_FDCWD, old_path, AT_FDCWD, new_path)
}

#[cfg(not(target_arch = "riscv64"))]
pub fn sys_renameat(
    old_dirfd: i32,
    old_path: *const c_char,
    new_dirfd: i32,
    new_path: *const c_char,
) -> AxResult<isize> {
    sys_renameat2(old_dirfd, old_path, new_dirfd, new_path, 0)
}

pub fn sys_renameat2(
    old_dirfd: i32,
    old_path: *const c_char,
    new_dirfd: i32,
    new_path: *const c_char,
    flags: u32,
) -> AxResult<isize> {
    // Linux renameat2: validate flags before copy_from_user paths (EINVAL before EFAULT).
    const RENAME_FLAGS_MASK: u32 = RENAME_NOREPLACE | RENAME_EXCHANGE | RENAME_WHITEOUT;
    if flags & !RENAME_FLAGS_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    if flags & RENAME_NOREPLACE != 0 && flags & RENAME_EXCHANGE != 0 {
        return Err(AxError::InvalidInput);
    }
    if flags & RENAME_WHITEOUT != 0 {
        // Overlay whiteout; not modeled in this VFS.
        return Err(AxError::InvalidInput);
    }

    let old_path = vm_load_string(old_path)?;
    if old_dirfd != AT_FDCWD && !old_path.starts_with('/') {
        let _ = Directory::from_fd(old_dirfd)?;
    }

    let new_path = vm_load_string(new_path)?;
    if new_dirfd != AT_FDCWD && !new_path.starts_with('/') {
        let _ = Directory::from_fd(new_dirfd)?;
    }
    debug!(
        "sys_renameat2 <= old_dirfd: {old_dirfd}, old_path: {old_path:?}, new_dirfd: {new_dirfd}, \
         new_path: {new_path}, flags: {flags}"
    );

    let old_dirfd_eff = dirfd_for_path_resolution(old_dirfd, old_path.as_str());
    let new_dirfd_eff = dirfd_for_path_resolution(new_dirfd, new_path.as_str());
    let (old_dir, old_name) =
        with_fs(old_dirfd_eff, |fs| fs.resolve_parent(Path::new(&old_path)))?;
    let (new_dir, new_name) =
        with_fs(new_dirfd_eff, |fs| fs.resolve_parent(Path::new(&new_path)))?;
    let old_name = old_name.as_ref();
    let new_name = new_name.as_ref();

    if flags & RENAME_EXCHANGE != 0 {
        // Success-path naming matches Linux, but exchange is not crash-atomic here (issue-196).
        if old_dir.ptr_eq(&new_dir) && old_name == new_name {
            return Err(AxError::InvalidInput);
        }
        return renameat2_exchange(&old_dir, old_name, &new_dir, new_name).map(|_| 0);
    }

    if flags & RENAME_NOREPLACE != 0 && new_dir.lookup_no_follow(new_name).is_ok() {
        return Err(AxError::AlreadyExists);
    }

    old_dir.rename(old_name, &new_dir, new_name)?;
    Ok(0)
}

/// `RENAME_EXCHANGE`: swap two directory entries (`old_name` ↔ `new_name`).
///
/// **Successful return:** The name→inode mapping should match Linux `renameat2(..., RENAME_EXCHANGE)`.
///
/// **Crash / power-loss:** Implemented as up to three [`Location::rename`] calls using a temporary
/// name under `old_dir` (`.starry_exchange_*`). Unlike Linux’s single VFS-level exchange on
/// typical paths, interruption between steps can leave a visible temporary entry or a half-done
/// swap. Do not assume the same crash-atomicity as Linux for package managers / editors that rely
/// on exchange for “atomic replace”.
fn renameat2_exchange(
    old_dir: &Location,
    old_name: &str,
    new_dir: &Location,
    new_name: &str,
) -> AxResult<()> {
    old_dir.lookup_no_follow(old_name)?;
    new_dir.lookup_no_follow(new_name)?;

    let mut tmp = format!(".starry_exchange_{:x}", monotonic_time_nanos());
    for _ in 0..16u32 {
        if old_dir.lookup_no_follow(&tmp).is_err() {
            break;
        }
        tmp = format!(
            ".starry_exchange_{:x}_{}",
            monotonic_time_nanos(),
            monotonic_time_nanos()
        );
    }
    if old_dir.lookup_no_follow(&tmp).is_ok() {
        return Err(AxError::ResourceBusy);
    }

    if old_dir.ptr_eq(new_dir) {
        old_dir.rename(old_name, old_dir, &tmp)?;
        old_dir.rename(new_name, new_dir, old_name)?;
        old_dir.rename(&tmp, new_dir, new_name)?;
    } else {
        old_dir.rename(old_name, old_dir, &tmp)?;
        new_dir.rename(new_name, old_dir, old_name)?;
        old_dir.rename(&tmp, new_dir, new_name)?;
    }
    Ok(())
}

/// Matches [`FsContext::ReadDir::BUF_SIZE`] in axfs-ng: one `read_dir` batch at most this many names.
const SYNC_READ_DIR_BUF: usize = 128;

/// Flushes nested mounts depth-first (same order as `unmount_all`), then this mount's
/// [`FilesystemOps::flush`].
fn flush_mount_subtree(root: &Location) -> AxResult<()> {
    if !root.is_root_of_mount() {
        return Err(AxError::InvalidInput);
    }

    let mut pos = 0u64;
    loop {
        let mut batch = VecDeque::new();
        let mut next_pos = pos;
        root.read_dir(pos, &mut |name: &str, _ino: u64, node_type: NodeType, off: u64| {
            batch.push_back((String::from(name), node_type));
            next_pos = off;
            batch.len() < SYNC_READ_DIR_BUF
        })?;

        if batch.is_empty() {
            break;
        }
        pos = next_pos;

        for (name, node_type) in batch {
            if name == DOT || name == DOTDOT {
                continue;
            }
            if node_type != NodeType::Directory {
                continue;
            }
            let child = root.lookup_no_follow(&name)?;
            if !Arc::ptr_eq(child.mountpoint(), root.mountpoint()) {
                flush_mount_subtree(&child)?;
            }
        }
    }

    root.filesystem().flush()
}

pub fn sys_sync() -> AxResult<isize> {
    debug!("sys_sync");
    let fs = FS_CONTEXT.lock();
    flush_mount_subtree(fs.root_dir())?;
    Ok(0)
}

pub fn sys_syncfs(fd: i32) -> AxResult<isize> {
    debug!("sys_syncfs <= fd: {fd}");
    let loc = location_from_fd(fd)?;
    loc.filesystem().flush()?;
    Ok(0)
}
