use axerrno::{AxError, AxResult};
use linux_raw_sys::general::{
    CLOCK_MONOTONIC, CLOCK_REALTIME, TFD_CLOEXEC, TFD_CREATE_FLAGS, TFD_NONBLOCK,
    TFD_TIMER_ABSTIME, itimerspec,
};
use starry_vm::VmMutPtr;

use crate::file::{FileLike, TimerFd, add_file_like};
use crate::time::read_timespec_user;

pub fn sys_timerfd_create(clockid: i32, flags: i32) -> AxResult<isize> {
    let cid = clockid as u32;
    if cid != CLOCK_REALTIME && cid != CLOCK_MONOTONIC {
        return Err(AxError::InvalidInput);
    }
    let flags = flags as u32;
    if flags & !TFD_CREATE_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    let cloexec = flags & TFD_CLOEXEC != 0;
    let nonblock = flags & TFD_NONBLOCK != 0;
    let tfd = TimerFd::new(cid, nonblock);
    add_file_like(tfd as _, cloexec).map(|fd| fd as isize)
}

pub fn sys_timerfd_settime(
    fd: i32,
    flags: i32,
    new_value: *const itimerspec,
    old_value: *mut itimerspec,
) -> AxResult<isize> {
    let tfd = TimerFd::from_fd(fd)?;
    // Linux timerfd_settime: reject unknown `flags` bits before NULL `new_value` (EINVAL ordering;
    // issue-330; issue-110 mask). Unrelated to field-wise `read_timespec_user` (issue-209).
    if flags as u32 & !TFD_TIMER_ABSTIME != 0 {
        return Err(AxError::InvalidInput);
    }
    if new_value.is_null() {
        return Err(AxError::InvalidInput);
    }
    // issue-209: read each `timespec` field-by-field (same as `read_timespec_user` / issue-205).
    let spec = unsafe {
        itimerspec {
            it_interval: read_timespec_user(core::ptr::addr_of!((*new_value).it_interval))?,
            it_value: read_timespec_user(core::ptr::addr_of!((*new_value).it_value))?,
        }
    };
    if !old_value.is_null() {
        old_value.vm_write(tfd.gettime()?)?;
    }
    tfd.settime(flags, &spec)?;
    Ok(0)
}

pub fn sys_timerfd_gettime(fd: i32, curr_value: *mut itimerspec) -> AxResult<isize> {
    let tfd = TimerFd::from_fd(fd)?;
    if curr_value.is_null() {
        // Linux `timerfd_gettime(2)`：NULL `curr_value` → EFAULT；须在 `gettime`/`vm_write` 前拒绝。
        return Err(AxError::BadAddress);
    }
    curr_value.vm_write(tfd.gettime()?)?;
    Ok(0)
}
