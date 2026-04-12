use core::ffi::{c_char, c_void};

use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;

use crate::{
    mm::{UserConstPtr, vm_load_string},
    pseudofs::MemoryFs,
};

pub fn sys_mount(
    source: *const c_char,
    target: *const c_char,
    fs_type: *const c_char,
    _flags: i32,
    _data: *const c_void,
) -> AxResult<isize> {
    let source = vm_load_string(source)?;
    let target = vm_load_string(target)?;
    let fs_type = vm_load_string(fs_type)?;
    debug!("sys_mount <= source: {source:?}, target: {target:?}, fs_type: {fs_type:?}");

    if fs_type != "tmpfs" {
        return Err(AxError::NoSuchDevice);
    }

    let fs = MemoryFs::new();

    let target = FS_CONTEXT.lock().resolve(target)?;
    target.mount(&fs)?;

    Ok(0)
}

pub fn sys_umount2(target: *const c_char, _flags: i32) -> AxResult<isize> {
    let target = vm_load_string(target)?;
    debug!("sys_umount2 <= target: {target:?}");
    let target = FS_CONTEXT.lock().resolve(target)?;
    target.unmount()?;
    Ok(0)
}

/// Linux mount API (`fsopen`): no fs-context layer yet; return **ENODEV** so userland does not get a
/// misleading `anon_inode:[dummy]` fd (tests accept EINVAL / ENODEV / ENOENT).
pub fn sys_fsopen(fsname: UserConstPtr<c_char>, _flags: u32) -> AxResult<isize> {
    let name = fsname.get_as_str()?;
    if name.is_empty() {
        return Err(AxError::InvalidInput);
    }
    debug!("sys_fsopen <= fsname: {name:?} (unsupported)");
    Err(AxError::NoSuchDevice)
}

/// `fspick`: unimplemented; **ENOSYS**.
pub fn sys_fspick(dfd: i32, pathname: UserConstPtr<c_char>, _flags: u32) -> AxResult<isize> {
    let path = pathname.get_as_str()?;
    debug!("sys_fspick <= dfd: {dfd}, path: {path:?} (unsupported)");
    Err(AxError::Unsupported)
}

/// `open_tree`: unimplemented; **ENOSYS** (callers that probe the API treat any error as skip).
pub fn sys_open_tree(dfd: i32, filename: UserConstPtr<c_char>, _flags: u32) -> AxResult<isize> {
    let path = filename.get_as_str()?;
    debug!("sys_open_tree <= dfd: {dfd}, path: {path:?} (unsupported)");
    Err(AxError::Unsupported)
}
