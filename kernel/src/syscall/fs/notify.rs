use axerrno::{AxError, AxResult};
use linux_raw_sys::general::{
    IN_CLOEXEC, IN_NONBLOCK,
    O_ACCMODE, O_APPEND, O_CLOEXEC, O_DSYNC, O_LARGEFILE, O_NOATIME, O_NONBLOCK as O_NONBLOCK_OPEN,
    __O_SYNC,
};

use crate::file::{FanotifyFd, InotifyFd, add_file_like};

/// `linux/fanotify.h` — flags for `fanotify_init()` (through `FAN_REPORT_MNT`).
const FAN_CLOEXEC: u32 = 0x0000_0001;
const FAN_NONBLOCK: u32 = 0x0000_0002;
const FAN_CLASS_CONTENT: u32 = 0x0000_0004;
const FAN_CLASS_PRE_CONTENT: u32 = 0x0000_0008;
const FAN_UNLIMITED_QUEUE: u32 = 0x0000_0010;
const FAN_UNLIMITED_MARKS: u32 = 0x0000_0020;
const FAN_ENABLE_AUDIT: u32 = 0x0000_0040;
const FAN_REPORT_PIDFD: u32 = 0x0000_0080;
const FAN_REPORT_TID: u32 = 0x0000_0100;
const FAN_REPORT_FID: u32 = 0x0000_0200;
const FAN_REPORT_DIR_FID: u32 = 0x0000_0400;
const FAN_REPORT_NAME: u32 = 0x0000_0800;
const FAN_REPORT_TARGET_FID: u32 = 0x0000_1000;
const FAN_REPORT_FD_ERROR: u32 = 0x0000_2000;
const FAN_REPORT_MNT: u32 = 0x0000_4000;

/// Valid `flags` for `fanotify_init(2)` (current `linux/fanotify.h` uapi).
const VALID_FANOTIFY_INIT_FLAGS: u32 = FAN_CLOEXEC
    | FAN_NONBLOCK
    | FAN_CLASS_CONTENT
    | FAN_CLASS_PRE_CONTENT
    | FAN_UNLIMITED_QUEUE
    | FAN_UNLIMITED_MARKS
    | FAN_ENABLE_AUDIT
    | FAN_REPORT_PIDFD
    | FAN_REPORT_TID
    | FAN_REPORT_FID
    | FAN_REPORT_DIR_FID
    | FAN_REPORT_NAME
    | FAN_REPORT_TARGET_FID
    | FAN_REPORT_FD_ERROR
    | FAN_REPORT_MNT;

/// Matches Linux `FANOTIFY_INIT_ALL_EVENT_F_BITS` (`fs/notify/fanotify/fanotify_user.c`).
const VALID_FANOTIFY_EVENT_F_FLAGS: u32 = O_ACCMODE
    | O_APPEND
    | O_NONBLOCK_OPEN
    | __O_SYNC
    | O_DSYNC
    | O_CLOEXEC
    | O_LARGEFILE
    | O_NOATIME;

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

pub fn sys_fanotify_init(flags: u32, event_f_flags: u32) -> AxResult<isize> {
    if flags & !VALID_FANOTIFY_INIT_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    if event_f_flags & !VALID_FANOTIFY_EVENT_F_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    let cloexec = flags & FAN_CLOEXEC != 0;
    let nonblock = flags & FAN_NONBLOCK != 0;
    let fd = FanotifyFd::new(nonblock, event_f_flags);
    add_file_like(fd as _, cloexec).map(|fd| fd as isize)
}
