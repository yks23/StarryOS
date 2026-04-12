use core::ffi::{c_char, c_int};

use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;
use axfs_ng_vfs::{Location, NodePermission};
use linux_raw_sys::general::{
    __kernel_fsid_t, AT_EACCESS, AT_EMPTY_PATH, AT_NO_AUTOMOUNT, AT_STATX_SYNC_TYPE,
    AT_SYMLINK_NOFOLLOW, F_OK, R_OK, STATX__RESERVED, W_OK, X_OK, stat, statfs, statx,
};
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    file::{location_from_fd, resolve_at},
    mm::vm_load_string,
};

/// Get the file metadata by `path` and write into `statbuf`.
///
/// Return 0 if success.
#[cfg(target_arch = "x86_64")]
pub fn sys_stat(path: *const c_char, statbuf: *mut stat) -> AxResult<isize> {
    use linux_raw_sys::general::AT_FDCWD;

    sys_fstatat(AT_FDCWD, path, statbuf, 0)
}

/// Get file metadata by `fd` and write into `statbuf`.
///
/// Return 0 if success.
pub fn sys_fstat(fd: i32, statbuf: *mut stat) -> AxResult<isize> {
    sys_fstatat(fd, core::ptr::null(), statbuf, AT_EMPTY_PATH)
}

/// Get the metadata of the symbolic link and write into `buf`.
///
/// Return 0 if success.
#[cfg(target_arch = "x86_64")]
pub fn sys_lstat(path: *const c_char, statbuf: *mut stat) -> AxResult<isize> {
    use linux_raw_sys::general::AT_FDCWD;

    sys_fstatat(AT_FDCWD, path, statbuf, AT_SYMLINK_NOFOLLOW)
}

pub fn sys_fstatat(
    dirfd: i32,
    path: *const c_char,
    statbuf: *mut stat,
    flags: u32,
) -> AxResult<isize> {
    // Linux do_fstatat: reject unknown flags before copy_from_user(pathname) (EINVAL before EFAULT).
    // Same mask as Linux `VALID_NEWFSTATAT_FLAGS` (AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT |
    // AT_EMPTY_PATH | AT_STATX_SYNC_TYPE).
    const VALID_NEWFSTATAT_FLAGS: u32 =
        AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT | AT_STATX_SYNC_TYPE;
    if flags & !VALID_NEWFSTATAT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    if flags & AT_STATX_SYNC_TYPE == AT_STATX_SYNC_TYPE {
        return Err(AxError::InvalidInput);
    }

    let path = path.nullable().map(vm_load_string).transpose()?;

    debug!("sys_fstatat <= dirfd: {dirfd}, path: {path:?}, flags: {flags}");

    let loc = resolve_at(dirfd, path.as_deref(), flags)?;
    statbuf.vm_write(loc.stat()?.into())?;

    Ok(0)
}

pub fn sys_statx(
    dirfd: c_int,
    path: *const c_char,
    flags: u32,
    mask: u32,
    statxbuf: *mut statx,
) -> AxResult<isize> {
    // `statx()` uses pathname, dirfd, and flags to identify the target
    // file in one of the following ways:

    // An absolute pathname(situation 1)
    //        If pathname begins with a slash, then it is an absolute
    //        pathname that identifies the target file.  In this case,
    //        dirfd is ignored.

    // A relative pathname(situation 2)
    //        If pathname is a string that begins with a character other
    //        than a slash and dirfd is AT_FDCWD, then pathname is a
    //        relative pathname that is interpreted relative to the
    //        process's current working directory.

    // A directory-relative pathname(situation 3)
    //        If pathname is a string that begins with a character other
    //        than a slash and dirfd is a file descriptor that refers to
    //        a directory, then pathname is a relative pathname that is
    //        interpreted relative to the directory referred to by dirfd.
    //        (See openat(2) for an explanation of why this is useful.)

    // By file descriptor(situation 4)
    //        If pathname is an empty string (or NULL since Linux 6.11)
    //        and the AT_EMPTY_PATH flag is specified in flags (see
    //        below), then the target file is the one referred to by the
    //        file descriptor dirfd.

    // Linux vfs_statx: reject unknown flags before touching user `path` (EINVAL before EFAULT).
    const VALID_STATX_FLAGS: u32 = AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW | AT_STATX_SYNC_TYPE;
    if flags & !VALID_STATX_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    // Cannot set both AT_STATX_FORCE_SYNC and AT_STATX_DONT_SYNC (covers full sync-type mask).
    if flags & AT_STATX_SYNC_TYPE == AT_STATX_SYNC_TYPE {
        return Err(AxError::InvalidInput);
    }
    // Linux `vfs_statx`: reserved bits in `mask` → EINVAL (not silent strip).
    if mask & STATX__RESERVED != 0 {
        return Err(AxError::InvalidInput);
    }

    let path = path.nullable().map(vm_load_string).transpose()?;
    debug!("sys_statx <= dirfd: {dirfd}, path: {path:?}, flags: {flags}, mask: {mask}");

    let kstat = resolve_at(dirfd, path.as_deref(), flags)?.stat()?;
    statxbuf.vm_write(kstat.into_statx_with_mask(mask))?;

    Ok(0)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_access(path: *const c_char, mode: u32) -> AxResult<isize> {
    use linux_raw_sys::general::AT_FDCWD;

    sys_faccessat2(AT_FDCWD, path, mode, 0)
}

pub fn sys_faccessat2(dirfd: c_int, path: *const c_char, mode: u32, flags: u32) -> AxResult<isize> {
    // Linux do_faccessat: reject invalid flags/mode before copy_from_user(pathname) (EINVAL before EFAULT).
    // Linux VALID_FACCESSAT_FLAGS (see open.c): AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH | AT_EACCESS.
    const VALID_FACCESSAT_FLAGS: u32 = AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH | AT_EACCESS;
    if flags & !VALID_FACCESSAT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }

    // mode must be a subset of F_OK|R_OK|W_OK|X_OK (F_OK is 0 on Linux uapi).
    const VALID_ACCESS_MODE: u32 = F_OK | R_OK | W_OK | X_OK;
    if mode & !VALID_ACCESS_MODE != 0 {
        return Err(AxError::InvalidInput);
    }

    let path = path.nullable().map(vm_load_string).transpose()?;
    debug!("sys_faccessat2 <= dirfd: {dirfd}, path: {path:?}, mode: {mode}, flags: {flags}");

    let file = resolve_at(dirfd, path.as_deref(), flags)?;

    if mode == 0 {
        return Ok(0);
    }
    let mut required_mode = NodePermission::empty();
    if mode & R_OK != 0 {
        required_mode |= NodePermission::OWNER_READ;
    }
    if mode & W_OK != 0 {
        required_mode |= NodePermission::OWNER_WRITE;
    }
    if mode & X_OK != 0 {
        required_mode |= NodePermission::OWNER_EXEC;
    }
    let required_mode = required_mode.bits();
    if (file.stat()?.mode as u16 & required_mode) != required_mode {
        return Err(AxError::PermissionDenied);
    }

    Ok(0)
}

fn statfs(loc: &Location) -> AxResult<statfs> {
    let stat = loc.filesystem().stat()?;
    // `f_fsid` distinguishes filesystem / superblock instances. Encode mount `device` and
    // `f_type` into both words so small `device` ids are not stored only in `val[1]` with
    // `val[0] == 0` (unlike a naive `[0, dev as i32]` truncation).
    let dev = loc.mountpoint().device();
    let t = stat.fs_type as u64;
    let packed = dev ^ (t << 32);
    Ok(statfs {
        f_type: stat.fs_type as _,
        f_bsize: stat.block_size as _,
        f_blocks: stat.blocks as _,
        f_bfree: stat.blocks_free as _,
        f_bavail: stat.blocks_available as _,
        f_files: stat.file_count as _,
        f_ffree: stat.free_file_count as _,
        f_fsid: __kernel_fsid_t {
            val: [(packed >> 32) as i32, packed as i32],
        },
        f_namelen: stat.name_length as _,
        f_frsize: stat.fragment_size as _,
        f_flags: stat.mount_flags as _,
        // Linux uapi reserved tail; must be zero for ABI stability (issue-175).
        f_spare: [0; 4],
    })
}

pub fn sys_statfs(path: *const c_char, buf: *mut statfs) -> AxResult<isize> {
    // Linux: validate user `buf` before `path` resolution so EFAULT does not follow a successful
    // lookup (issue-281).
    if buf.is_null() {
        return Err(AxError::BadAddress);
    }
    let path = vm_load_string(path)?;
    debug!("sys_statfs <= path: {path:?}");

    buf.vm_write(statfs(
        &FS_CONTEXT
            .lock()
            .resolve(path)?
            .mountpoint()
            .root_location(),
    )?)?;
    Ok(0)
}

pub fn sys_fstatfs(fd: i32, buf: *mut statfs) -> AxResult<isize> {
    debug!("sys_fstatfs <= fd: {fd}");

    if buf.is_null() {
        return Err(AxError::BadAddress);
    }
    let loc = location_from_fd(fd)?;
    buf.vm_write(statfs(&loc)?)?;
    Ok(0)
}
