use axerrno::{AxError, AxResult};
use linux_raw_sys::general::{IN_CLOEXEC, IN_NONBLOCK};

use crate::file::{FanotifyFd, InotifyFd, add_file_like};

/// fanotify_init(2) flags (subset; see `linux/fanotify.h`).
const FAN_CLOEXEC: u32 = 0x0000_0001;
const FAN_NONBLOCK: u32 = 0x0000_0002;

pub fn sys_inotify_init1(flags: i32) -> AxResult<isize> {
    let f = flags as u32;
    if f & !(IN_NONBLOCK | IN_CLOEXEC) != 0 {
        return Err(AxError::InvalidInput);
    }
    let cloexec = f & IN_CLOEXEC != 0;
    let nonblock = f & IN_NONBLOCK != 0;
    let fd = InotifyFd::new(nonblock);
    add_file_like(fd as _, cloexec).map(|fd| fd as isize)
}

pub fn sys_fanotify_init(flags: u32, _event_f_flags: u32) -> AxResult<isize> {
    let cloexec = flags & FAN_CLOEXEC != 0;
    let nonblock = flags & FAN_NONBLOCK != 0;
    let fd = FanotifyFd::new(nonblock);
    add_file_like(fd as _, cloexec).map(|fd| fd as isize)
}
