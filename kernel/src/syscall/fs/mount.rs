use core::ffi::{c_char, c_void};

use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;
use linux_raw_sys::general::{
    AT_FDCWD, MNT_DETACH, MNT_EXPIRE, MNT_FORCE, O_CLOEXEC, UMOUNT_NOFOLLOW,
};

use crate::{
    file::{Directory, FileLike},
    mm::{UserConstPtr, vm_load_string},
    pseudofs::MemoryFs,
};

/// Kernel-supported `mount(2)` filesystem type names (see `fs_type` argument).
const SUPPORTED_MOUNT_FSTYPES: &[&str] = &["tmpfs"];

fn validate_fs_type(fs_type: &str) -> AxResult<()> {
    if fs_type.is_empty() {
        return Err(AxError::InvalidInput);
    }
    if !SUPPORTED_MOUNT_FSTYPES.contains(&fs_type) {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

/// `mount(2)` `source` must match what we implement for each `fs_type` (issue-187). Extend with a
/// new arm when adding filesystems to [`SUPPORTED_MOUNT_FSTYPES`].
fn validate_mount_source(source: &str, fs_type: &str) -> AxResult<()> {
    match fs_type {
        "tmpfs" => {
            if source.is_empty() || source == "tmpfs" || source == fs_type {
                Ok(())
            } else {
                Err(AxError::InvalidInput)
            }
        }
        _ => Err(AxError::InvalidInput),
    }
}

/// Supported `mount(2)` subset: no `MS_*` bits (no `MS_RDONLY`/`MS_BIND`/… until implemented).
fn validate_mount_flags(flags: i32) -> AxResult<()> {
    if flags != 0 {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

/// Until `tmpfs`/`MemoryFs` options are parsed, only `NULL` or an empty C string.
fn validate_mount_data(data: *const c_void) -> AxResult<()> {
    if data.is_null() {
        return Ok(());
    }
    let s = vm_load_string(data.cast::<c_char>())?;
    if s.is_empty() {
        Ok(())
    } else {
        Err(AxError::InvalidInput)
    }
}

pub fn sys_mount(
    source: *const c_char,
    target: *const c_char,
    fs_type: *const c_char,
    flags: i32,
    data: *const c_void,
) -> AxResult<isize> {
    // Copy user strings before rejecting invalid `flags`/`data` content, similar to Linux
    // `copy_mount_string` / `copy_mount_options` ordering: **EFAULT** / length-class errors from
    // paths or `data` surface before **EINVAL** from unsupported `MS_*` or non-empty `data`
    // (issue-303).
    let source = vm_load_string(source)?;
    let target = vm_load_string(target)?;
    let fs_type = vm_load_string(fs_type)?;
    validate_mount_data(data)?;

    validate_mount_flags(flags)?;

    debug!("sys_mount <= source: {source:?}, target: {target:?}, fs_type: {fs_type:?}, flags: {flags}");

    validate_fs_type(&fs_type)?;
    validate_mount_source(&source, &fs_type)?;

    let fs = MemoryFs::new();

    let target = FS_CONTEXT.lock().resolve(target)?;
    target.mount(&fs)?;

    Ok(0)
}

const UMOUNT_ALLOWED_FLAGS_U32: u32 = MNT_FORCE | MNT_DETACH | MNT_EXPIRE | UMOUNT_NOFOLLOW;
/// Linux `MNT_FORCE` / `MNT_DETACH` / `MNT_EXPIRE` need namespace/lazy-unmount semantics Starry does not
/// implement yet; return **`EOPNOTSUPP`** instead of accepting the bits and performing an immediate
/// `unmount` (misleading vs. `umount2(2)`).
const UMOUNT_UNSUPPORTED_FLAGS_U32: u32 = MNT_FORCE | MNT_DETACH | MNT_EXPIRE;

pub fn sys_umount2(target: *const c_char, flags: i32) -> AxResult<isize> {
    let f = flags as u32;
    if f & !UMOUNT_ALLOWED_FLAGS_U32 != 0 {
        return Err(AxError::InvalidInput);
    }
    if f & UMOUNT_UNSUPPORTED_FLAGS_U32 != 0 {
        return Err(AxError::OperationNotSupported);
    }
    let target = vm_load_string(target)?;
    debug!("sys_umount2 <= target: {target:?}, flags: {flags}");
    let fs = FS_CONTEXT.lock();
    let loc = if f & UMOUNT_NOFOLLOW != 0 {
        fs.resolve_no_follow(&target)?
    } else {
        fs.resolve(&target)?
    };
    loc.unmount()?;
    Ok(0)
}

/// Linux `FSOPEN_CLOEXEC` (`uapi/linux/mount.h`); `linux_raw_sys` may not export it on this target.
const FSOPEN_CLOEXEC: u32 = 0x0000_0001;
/// Allowed `FSOPEN_*` mask until more flags are implemented.
const FSOPEN_KNOWN_FLAGS: u32 = FSOPEN_CLOEXEC;

/// Linux mount API (`fsopen`): no fs-context layer yet; return **ENODEV** so userland does not get a
/// misleading `anon_inode:[dummy]` fd (tests accept EINVAL / ENODEV / ENOENT).
pub fn sys_fsopen(fsname: UserConstPtr<c_char>, flags: u32) -> AxResult<isize> {
    // Unknown `FSOPEN_*` bits → **EINVAL** before `fsname` (flags-first; issue-386; Linux `fsopen`).
    if flags & !FSOPEN_KNOWN_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    let name = fsname.get_as_str()?;
    if name.is_empty() {
        return Err(AxError::InvalidInput);
    }
    debug!("sys_fsopen <= fsname: {name:?} (unsupported)");
    Err(AxError::NoSuchDevice)
}

/// `fspick`: unimplemented; **ENOSYS**.
pub fn sys_fspick(dfd: i32, pathname: UserConstPtr<c_char>, _flags: u32) -> AxResult<isize> {
    // Linux fspick: fdget(dfd) before copy_from_user(pathname) → EBADF first (issue-166).
    if dfd != AT_FDCWD {
        let _ = <Directory as FileLike>::from_fd(dfd)?;
    }
    let path = pathname.get_as_str()?;
    debug!("sys_fspick <= dfd: {dfd}, path: {path:?} (unsupported)");
    Err(AxError::Unsupported)
}

/// Linux `OPEN_TREE_CLONE` / `OPEN_TREE_ITERATIVE` (`uapi/linux/mount.h`); `OPEN_TREE_CLOEXEC` is `O_CLOEXEC`.
const OPEN_TREE_CLONE: u32 = 0x0000_0001;
const OPEN_TREE_ITERATIVE: u32 = 0x0000_0002;
const OPEN_TREE_KNOWN_FLAGS: u32 = OPEN_TREE_CLONE | OPEN_TREE_ITERATIVE | O_CLOEXEC;

/// `open_tree`: unimplemented; **ENOSYS** (callers that probe the API treat any error as skip).
pub fn sys_open_tree(dfd: i32, filename: UserConstPtr<c_char>, flags: u32) -> AxResult<isize> {
    if dfd != AT_FDCWD {
        let _ = <Directory as FileLike>::from_fd(dfd)?;
    }
    // After `from_fd` (EBADF) but before `filename`: unknown `OPEN_TREE_*` → EINVAL (issue-387).
    if flags & !OPEN_TREE_KNOWN_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    let path = filename.get_as_str()?;
    debug!("sys_open_tree <= dfd: {dfd}, path: {path:?} (unsupported)");
    Err(AxError::Unsupported)
}
