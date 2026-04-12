use alloc::sync::Arc;

use axerrno::{AxError, AxResult};
use axtask::current;
use starry_process::Pid;

use crate::task::{AsThread, get_process_data, get_process_group};

pub fn sys_getsid(pid: Pid) -> AxResult<isize> {
    Ok(get_process_data(pid)?.proc.group().session().sid() as _)
}

pub fn sys_setsid() -> AxResult<isize> {
    let curr = current();
    let proc = &curr.as_thread().proc_data.proc;
    if get_process_group(proc.pid()).is_ok() {
        return Err(AxError::OperationNotPermitted);
    }

    if let Some((session, _)) = proc.create_session() {
        Ok(session.sid() as _)
    } else {
        Ok(proc.pid() as _)
    }
}

pub fn sys_getpgid(pid: Pid) -> AxResult<isize> {
    Ok(get_process_data(pid)?.proc.group().pgid() as _)
}

pub fn sys_setpgid(pid: Pid, pgid: Pid) -> AxResult<isize> {
    // Linux: only the process itself or its parent may change its process group; a
    // non-self target must be in the same session as the caller (EPERM otherwise).
    let caller = current().as_thread().proc_data.proc.clone();
    let target_pd = get_process_data(pid)?;
    let target = &target_pd.proc;

    if target.is_zombie() {
        return Err(AxError::NoSuchProcess);
    }

    let is_self = Arc::ptr_eq(&caller, target);
    let is_child = target
        .parent()
        .is_some_and(|p| Arc::ptr_eq(&p, &caller));
    if !is_self && !is_child {
        return Err(AxError::OperationNotPermitted);
    }
    if is_child && caller.group().session().sid() != target.group().session().sid() {
        return Err(AxError::OperationNotPermitted);
    }

    if pgid == 0 {
        target.create_group();
    } else {
        // Linux `setpgid`: no such process group → EINVAL; ESRCH is for invalid `pid` (issue-346).
        let pg = match get_process_group(pgid) {
            Ok(pg) => pg,
            Err(AxError::NoSuchProcess) => return Err(AxError::InvalidInput),
            Err(e) => return Err(e),
        };
        if !target.move_to_group(&pg) {
            return Err(AxError::OperationNotPermitted);
        }
    }

    Ok(0)
}

// TODO: job control
