use core::sync::atomic::{AtomicBool, Ordering};

use axerrno::{AxError, AxResult};
use axhal::uspace::UserContext;
use axtask::{TaskInner, current};
use starry_process::Pid;
use starry_signal::{SignalInfo, SignalOSAction, SignalSet};

use super::{AsThread, Thread, do_exit, get_process_data, get_process_group, get_task};

pub fn check_signals(
    thr: &Thread,
    uctx: &mut UserContext,
    restore_blocked: Option<SignalSet>,
) -> bool {
    let Some((sig, os_action)) = thr.signal.check_signals(uctx, restore_blocked) else {
        return false;
    };

    let signo = sig.signo();
    match os_action {
        SignalOSAction::Terminate => {
            do_exit(signo as i32, true);
        }
        SignalOSAction::CoreDump => {
            super::core_dump::write_min_elf_core(thr, uctx, signo as u32);
            do_exit(128 + signo as i32, true);
        }
        SignalOSAction::Stop => {
            let signo_u8 = signo as u8;
            {
                let mut jc = thr.proc_data.jobctl.lock();
                jc.stop_sig = Some(signo_u8);
                jc.stop_wait_pending = true;
            }
            wake_parent_for_jobctl(thr);
            let wq = thr.proc_data.jobctl_stop_wq.clone();
            loop {
                {
                    let jc = thr.proc_data.jobctl.lock();
                    if jc.stop_sig.is_none() {
                        break;
                    }
                }
                while check_signals(thr, uctx, restore_blocked) {}
                // Block on the jobctl wait queue instead of spinning on `yield_now`.
                let still_wait = match wq.wait_if(!0, None, || {
                    thr.proc_data.jobctl.lock().stop_sig.is_some()
                }) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !still_wait {
                    break;
                }
            }
        }
        SignalOSAction::Continue => {
            let mut jc = thr.proc_data.jobctl.lock();
            let was_stopped = jc.stop_sig.take().is_some();
            if was_stopped {
                jc.continued_wait_pending = true;
            }
            drop(jc);
            if was_stopped {
                thr.proc_data.jobctl_stop_wq.wake(usize::MAX, !0);
            }
            wake_parent_for_jobctl(thr);
        }
        SignalOSAction::Handler => {
            // do nothing
        }
    }
    true
}

fn wake_parent_for_jobctl(thr: &Thread) {
    if let Some(p) = thr.proc_data.proc.parent()
        && let Ok(pdata) = get_process_data(p.pid())
    {
        pdata.child_exit_event.wake();
    }
}

static BLOCK_NEXT_SIGNAL_CHECK: AtomicBool = AtomicBool::new(false);

pub fn block_next_signal() {
    BLOCK_NEXT_SIGNAL_CHECK.store(true, Ordering::SeqCst);
}

pub fn unblock_next_signal() -> bool {
    BLOCK_NEXT_SIGNAL_CHECK.swap(false, Ordering::SeqCst)
}

pub fn with_blocked_signals<R>(
    blocked: Option<SignalSet>,
    f: impl FnOnce() -> AxResult<R>,
) -> AxResult<R> {
    let curr = current();
    let sig = &curr.as_thread().signal;

    let old_blocked = blocked.map(|set| sig.set_blocked(set));
    let result = f();
    if let Some(old) = old_blocked {
        sig.set_blocked(old);
    }
    result
}

pub(super) fn send_signal_thread_inner(task: &TaskInner, thr: &Thread, sig: SignalInfo) {
    if thr.signal.send_signal(sig) {
        task.interrupt();
    }
}

/// Sends a signal to a thread.
pub fn send_signal_to_thread(tgid: Option<Pid>, tid: Pid, sig: Option<SignalInfo>) -> AxResult<()> {
    let task = get_task(tid)?;
    let thread = task.try_as_thread().ok_or(AxError::OperationNotPermitted)?;
    if tgid.is_some_and(|tgid| thread.proc_data.proc.pid() != tgid) {
        return Err(AxError::NoSuchProcess);
    }

    if let Some(sig) = sig {
        info!("Send signal {:?} to thread {}", sig.signo(), tid);
        send_signal_thread_inner(&task, thread, sig);
    }

    Ok(())
}

/// Sends a signal to a process.
pub fn send_signal_to_process(pid: Pid, sig: Option<SignalInfo>) -> AxResult<()> {
    let proc_data = get_process_data(pid)?;

    if let Some(sig) = sig {
        let signo = sig.signo();
        info!("Send signal {signo:?} to process {pid}");
        if let Some(tid) = proc_data.signal.send_signal(sig)
            && let Ok(task) = get_task(tid)
        {
            task.interrupt();
        }
    }

    Ok(())
}

/// Sends a signal to a process group.
pub fn send_signal_to_process_group(pgid: Pid, sig: Option<SignalInfo>) -> AxResult<()> {
    let pg = get_process_group(pgid)?;

    if let Some(sig) = sig {
        info!("Send signal {:?} to process group {}", sig.signo(), pgid);
        for proc in pg.processes() {
            send_signal_to_process(proc.pid(), Some(sig.clone()))?;
        }
    }

    Ok(())
}

/// Sends a fatal signal to the current process.
pub fn raise_signal_fatal(sig: SignalInfo) -> AxResult<()> {
    let curr = current();
    let proc_data = &curr.as_thread().proc_data;

    let signo = sig.signo();
    info!("Send fatal signal {signo:?} to the current process");
    if let Some(tid) = proc_data.signal.send_signal(sig)
        && let Ok(task) = get_task(tid)
    {
        task.interrupt();
    } else {
        // No task wants to handle the signal, abort the task
        do_exit(signo as i32, true);
    }

    Ok(())
}
