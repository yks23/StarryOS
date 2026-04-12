use alloc::{string::ToString, sync::Arc, vec::Vec};
use core::{ffi::c_char, future::poll_fn, task::Poll};

use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;
use axhal::uspace::UserContext;
use axtask::{
    current,
    future::{block_on, interruptible},
};
use starry_process::Pid;
use starry_signal::{SignalInfo, Signo};
use starry_vm::vm_load_until_nul;

use crate::{
    config::USER_HEAP_BASE,
    file::FD_TABLE,
    mm::{load_user_app, vm_load_string},
    task::{AsThread, send_signal_to_thread},
};

pub fn sys_execve(
    uctx: &mut UserContext,
    path: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> AxResult<isize> {
    // Linux bprm_execve/copy_strings: `argv` must be valid; NULL argv → EFAULT before copying
    // pathname from user (issue-325; same class as issue-323/issue-321).
    if argv.is_null() {
        return Err(AxError::BadAddress);
    }

    let path = vm_load_string(path)?;
    // Linux rejects empty pathname with EINVAL before replacing the image (issue-407; issue-393
    // theme; NULL `argv` first, issue-325).
    if path.is_empty() {
        return Err(AxError::InvalidInput);
    }

    let args = vm_load_until_nul(argv)?
        .into_iter()
        .map(vm_load_string)
        .collect::<Result<Vec<_>, _>>()?;

    let curr = current();
    let proc_data = &curr.as_thread().proc_data;

    // Linux: `envp == NULL` inherits the calling process's environment (not an empty env).
    let envs = if envp.is_null() {
        proc_data.environment.read().as_ref().clone()
    } else {
        vm_load_until_nul(envp)?
            .into_iter()
            .map(vm_load_string)
            .collect::<Result<Vec<_>, _>>()?
    };

    debug!("sys_execve <= path: {path:?}, args: {args:?}, envs: {envs:?}");
    let process = proc_data.proc.clone();
    let my_tid = curr.id().as_u64() as Pid;

    // Linux: execve terminates all other threads in the process before replacing the image.
    if process.threads().len() > 1 {
        let sig_kill = SignalInfo::new_kernel(Signo::SIGKILL);
        for tid in process.threads() {
            if tid != my_tid {
                let _ = send_signal_to_thread(None, tid, Some(sig_kill.clone()));
            }
        }
        if let Err(e) = block_on(interruptible(poll_fn(|cx| {
            if process.threads().len() <= 1 {
                return Poll::Ready(Ok::<(), AxError>(()));
            }
            proc_data.thread_group_exit_event.register(cx.waker());
            if process.threads().len() <= 1 {
                return Poll::Ready(Ok::<(), AxError>(()));
            }
            Poll::Pending
        }))) {
            return Err(e.into());
        }
    }

    let mut aspace = proc_data.aspace.write();
    let (entry_point, user_stack_base) =
        load_user_app(&mut aspace, Some(path.as_str()), &args, &envs)?;
    drop(aspace);

    let loc = FS_CONTEXT.lock().resolve(&path)?;
    curr.set_name(loc.name());

    *proc_data.exe_path.write() = loc.absolute_path()?.to_string();
    *proc_data.cmdline.write() = Arc::new(args);
    *proc_data.environment.write() = Arc::new(envs);

    proc_data.set_heap_top(USER_HEAP_BASE);

    *proc_data.signal.actions.lock() = Default::default();

    // Clear set_child_tid after exec since the original address is no longer valid
    curr.as_thread().set_clear_child_tid(0);

    // Close CLOEXEC file descriptors
    let mut fd_table = FD_TABLE.write();
    let cloexec_fds = fd_table
        .ids()
        .filter(|it| fd_table.get(*it).unwrap().cloexec)
        .collect::<Vec<_>>();
    for fd in cloexec_fds {
        fd_table.remove(fd);
    }
    drop(fd_table);

    uctx.set_ip(entry_point.as_usize());
    uctx.set_sp(user_stack_base.as_usize());
    Ok(0)
}
