use axerrno::{AxError, AxResult};
use axtask::current;
use bitflags::bitflags;
use starry_signal::SignalInfo;

use crate::{
    file::{FD_TABLE, FileLike, PidFd, add_file_like},
    syscall::signal::make_queue_signal_info,
    task::{
        AsThread, get_process_data, get_task, may_peer_process_by_cred, send_signal_to_process,
    },
};

bitflags! {
    #[derive(Debug, Clone, Copy, Default)]
    pub struct PidFdFlags: u32 {
        const NONBLOCK = 2048;
        const THREAD = 128;
    }
}

pub fn sys_pidfd_open(pid: u32, flags: u32) -> AxResult<isize> {
    debug!("sys_pidfd_open <= pid: {pid}, flags: {flags}");

    let flags = PidFdFlags::from_bits(flags).ok_or(AxError::InvalidInput)?;

    // Linux `pidfd_open(2)`：`pid`/`tid` 须为真实目标 id，`0` 非法（**EINVAL**），
    // 勿复用 `get_process_data`/`get_task` 对 `0` → current 的约定。
    if pid == 0 {
        return Err(AxError::InvalidInput);
    }

    let caller_pd = current().as_thread().proc_data.clone();
    let fd = if flags.contains(PidFdFlags::THREAD) {
        let task = get_task(pid)?;
        let target_pd = task.as_thread().proc_data.clone();
        if !may_peer_process_by_cred(&caller_pd, &target_pd) {
            return Err(AxError::OperationNotPermitted);
        }
        PidFd::new_thread(task.as_thread())
    } else {
        let target_pd = get_process_data(pid)?;
        if !may_peer_process_by_cred(&caller_pd, &target_pd) {
            return Err(AxError::OperationNotPermitted);
        }
        PidFd::new_process(&target_pd)
    };
    if flags.contains(PidFdFlags::NONBLOCK) {
        fd.set_nonblocking(true)?;
    }

    fd.add_to_fd_table(true).map(|fd| fd as _)
}

pub fn sys_pidfd_getfd(pidfd: i32, target_fd: i32, flags: u32) -> AxResult<isize> {
    debug!("sys_pidfd_getfd <= pidfd: {pidfd}, target_fd: {target_fd}, flags: {flags}");

    if flags != 0 {
        return Err(AxError::InvalidInput);
    }

    let pidfd = PidFd::from_fd(pidfd)?;
    let proc_data = pidfd.process_data()?;
    FD_TABLE
        .scope(&proc_data.scope.read())
        .read()
        .get(target_fd as usize)
        .ok_or(AxError::BadFileDescriptor)
        .and_then(|fd| {
            let fd = add_file_like(fd.inner.clone(), true)?;
            Ok(fd as isize)
        })
}

pub fn sys_pidfd_send_signal(
    pidfd: i32,
    signo: u32,
    sig: *mut SignalInfo,
    flags: u32,
) -> AxResult<isize> {
    if flags != 0 {
        return Err(AxError::InvalidInput);
    }

    let pidfd = PidFd::from_fd(pidfd)?;
    let pid = pidfd.process_data()?.proc.pid();

    let sig = make_queue_signal_info(pid, signo, sig)?;
    send_signal_to_process(pid, sig)?;
    Ok(0)
}
