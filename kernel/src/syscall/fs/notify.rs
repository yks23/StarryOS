use axerrno::{AxError, AxResult};
use axtask::current;
use linux_raw_sys::general::{
    IN_CLOEXEC, IN_NONBLOCK,
    O_ACCMODE, O_APPEND, O_CLOEXEC, O_DSYNC, O_LARGEFILE, O_NOATIME, O_NONBLOCK as O_NONBLOCK_OPEN,
    O_RDONLY, O_RDWR, O_WRONLY,
    __O_SYNC,
};

use crate::{
    file::{FanotifyFd, InotifyFd, add_file_like},
    task::AsThread,
};

/// `linux/fanotify.h` — flags for `fanotify_init()` (through `FAN_REPORT_MNT`).
const FAN_CLOEXEC: u32 = 0x0000_0001;
const FAN_NONBLOCK: u32 = 0x0000_0002;
const FAN_CLASS_CONTENT: u32 = 0x0000_0004;
const FAN_CLASS_PRE_CONTENT: u32 = 0x0000_0008;
const FAN_UNLIMITED_QUEUE: u32 = 0x0000_0010;
const FAN_UNLIMITED_MARKS: u32 = 0x0000_0020;
const FAN_ENABLE_AUDIT: u32 = 0x0000_0040;

/// Linux `CAP_AUDIT_WRITE` (`include/uapi/linux/capability.h`).
const CAP_AUDIT_WRITE: u32 = 30;
const FAN_REPORT_PIDFD: u32 = 0x0000_0080;
const FAN_REPORT_TID: u32 = 0x0000_0100;
const FAN_REPORT_FID: u32 = 0x0000_0200;
const FAN_REPORT_DIR_FID: u32 = 0x0000_0400;
const FAN_REPORT_NAME: u32 = 0x0000_0800;
const FAN_REPORT_TARGET_FID: u32 = 0x0000_1000;
const FAN_REPORT_FD_ERROR: u32 = 0x0000_2000;
const FAN_REPORT_MNT: u32 = 0x0000_4000;

/// `FAN_CLASS_NOTIF` — uapi value 0 (`linux/uapi/linux/fanotify.h`).
const FAN_CLASS_NOTIF: u32 = 0;

/// Matches Linux `FAN_REPORT_DFID_NAME_TARGET` / internal `FANOTIFY_FID_BITS`
/// (`include/linux/fanotify.h`).
const FANOTIFY_FID_BITS: u32 =
    FAN_REPORT_DIR_FID | FAN_REPORT_NAME | FAN_REPORT_FID | FAN_REPORT_TARGET_FID;

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

/// `fanotify_init` `flags` / `event_f_flags` combination rules aligned with Linux
/// `SYSCALL_DEFINE2(fanotify_init, …)` (`fanotify_user.c`; issue-409; `event_f_flags` storage issue-328).
fn validate_fanotify_init_combined(flags: u32, event_f_flags: u32) -> AxResult<()> {
    let class = flags & (FAN_CLASS_CONTENT | FAN_CLASS_PRE_CONTENT);

    if flags & FAN_REPORT_PIDFD != 0 && flags & FAN_REPORT_TID != 0 {
        return Err(AxError::InvalidInput);
    }

    if flags & FAN_REPORT_MNT != 0 {
        if class != FAN_CLASS_NOTIF {
            return Err(AxError::InvalidInput);
        }
        if flags & (FANOTIFY_FID_BITS | FAN_REPORT_FD_ERROR) != 0 {
            return Err(AxError::InvalidInput);
        }
    }

    if event_f_flags & !VALID_FANOTIFY_EVENT_F_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }

    let acc = event_f_flags & O_ACCMODE;
    if acc != O_RDONLY && acc != O_WRONLY && acc != O_RDWR {
        return Err(AxError::InvalidInput);
    }

    let fid_mode = flags & FANOTIFY_FID_BITS;
    if fid_mode != 0 && class != FAN_CLASS_NOTIF {
        return Err(AxError::InvalidInput);
    }

    if fid_mode & FAN_REPORT_NAME != 0 && fid_mode & FAN_REPORT_DIR_FID == 0 {
        return Err(AxError::InvalidInput);
    }

    if fid_mode & FAN_REPORT_TARGET_FID != 0
        && (fid_mode & FAN_REPORT_NAME == 0 || fid_mode & FAN_REPORT_FID == 0)
    {
        return Err(AxError::InvalidInput);
    }

    // Linux `switch (class)` default: only `FAN_CLASS_NOTIF` (0), `FAN_CLASS_CONTENT`, or
    // `FAN_CLASS_PRE_CONTENT` — not both class bits (`fanotify_user.c`).
    if class != FAN_CLASS_NOTIF && class != FAN_CLASS_CONTENT && class != FAN_CLASS_PRE_CONTENT {
        return Err(AxError::InvalidInput);
    }

    Ok(())
}

/// Linux `fanotify_init`: `FAN_ENABLE_AUDIT` requires `capable(CAP_AUDIT_WRITE)` → EPERM
/// (`fanotify_user.c`; issue-411; issue-409 `EINVAL` rules orthogonal).
fn require_cap_audit_write_if_fanotify_audit(flags: u32) -> AxResult<()> {
    if flags & FAN_ENABLE_AUDIT == 0 {
        return Ok(());
    }
    let eff = current()
        .as_thread()
        .proc_data
        .get_capabilities()
        .0;
    if eff & (1 << CAP_AUDIT_WRITE) == 0 {
        return Err(AxError::PermissionDenied);
    }
    Ok(())
}

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
    validate_fanotify_init_combined(flags, event_f_flags)?;
    require_cap_audit_write_if_fanotify_audit(flags)?;
    let cloexec = flags & FAN_CLOEXEC != 0;
    let nonblock = flags & FAN_NONBLOCK != 0;
    let fd = FanotifyFd::new(nonblock, event_f_flags);
    add_file_like(fd as _, cloexec).map(|fd| fd as isize)
}
