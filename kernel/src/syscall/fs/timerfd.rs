use axerrno::{AxError, AxResult};
use linux_raw_sys::general::{
    CLOCK_MONOTONIC, CLOCK_REALTIME, TFD_CLOEXEC, TFD_CREATE_FLAGS, TFD_NONBLOCK,
    TFD_TIMER_ABSTIME, itimerspec,
};
use starry_vm::{VmMutPtr, VmPtr};

use crate::file::{FileLike, TimerFd, add_file_like};

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
    if new_value.is_null() {
        return Err(AxError::InvalidInput);
    }
    // Linux `timerfd_settime(2)`：仅允许 `TFD_TIMER_ABSTIME`，未知位 → EINVAL。
    if flags as u32 & !TFD_TIMER_ABSTIME != 0 {
        return Err(AxError::InvalidInput);
    }
    let new_value = unsafe { new_value.vm_read_uninit()?.assume_init() };
    if !old_value.is_null() {
        old_value.vm_write(tfd.gettime()?)?;
    }
    tfd.settime(flags, &new_value)?;
    Ok(0)
}

pub fn sys_timerfd_gettime(fd: i32, curr_value: *mut itimerspec) -> AxResult<isize> {
    let tfd = TimerFd::from_fd(fd)?;
    curr_value.vm_write(tfd.gettime()?)?;
    Ok(0)
}
