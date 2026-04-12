use core::{future::poll_fn, task::Poll};

use axerrno::{AxError, AxResult, LinuxError};
use axhal::uspace::UserContext;
use axtask::{
    current,
    future::{self, block_on},
};
use linux_raw_sys::general::{
    MINSIGSTKSZ, SI_TKILL, SI_USER, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK, kernel_sigaction, siginfo,
    timespec,
};
use starry_process::Pid;
use starry_signal::{SignalInfo, SignalSet, SignalStack, Signo};
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    task::{
        AsThread, block_next_signal, check_signals, processes, send_signal_to_process,
        send_signal_to_process_group, send_signal_to_thread,
    },
    time::TimeValueLike,
};

pub(crate) fn check_sigset_size(size: usize) -> AxResult<()> {
    // Linux `copy_sigset_from_user` / `rt_sigprocmask` 等：`sigsetsize` 须严格等于
    // `sizeof(sigset_t)`，`0` 与过大/过小均为 **EINVAL**（与畸形 libc /直接 syscall 对齐）。
    if size != size_of::<SignalSet>() {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

fn parse_signo(signo: u32) -> AxResult<Signo> {
    Signo::from_repr(signo as u8).ok_or(AxError::InvalidInput)
}

pub fn sys_rt_sigprocmask(
    how: i32,
    set: *const SignalSet,
    oldset: *mut SignalSet,
    sigsetsize: usize,
) -> AxResult<isize> {
    check_sigset_size(sigsetsize)?;

    let curr = current();
    let sig = &curr.as_thread().signal;
    let old = sig.blocked();

    match set.nullable() {
        None => {
            // Linux: when `set` is NULL, `how` is ignored; only copy out old mask.
            if let Some(oldset) = oldset.nullable() {
                oldset.vm_write(old)?;
            }
        }
        Some(set_ptr) => {
            // Linux do_sigprocmask: validate `how` and read `set` before copy_to_user(old);
            // invalid `how` must not clobber `oldset` (issue-146); bad `set` pointer likewise.
            match how as u32 {
                SIG_BLOCK | SIG_UNBLOCK | SIG_SETMASK => {}
                _ => return Err(AxError::InvalidInput),
            }
            let set = unsafe { set_ptr.vm_read_uninit()?.assume_init() };
            let new_mask = match how as u32 {
                SIG_BLOCK => old | set,
                SIG_UNBLOCK => old & !set,
                SIG_SETMASK => set,
                _ => unreachable!(),
            };
            if let Some(oldset) = oldset.nullable() {
                oldset.vm_write(old)?;
            }
            debug!("sys_rt_sigprocmask <= {new_mask:?}");
            sig.set_blocked(new_mask);
        }
    }

    Ok(0)
}

pub fn sys_rt_sigaction(
    signo: u32,
    act: *const kernel_sigaction,
    oldact: *mut kernel_sigaction,
    sigsetsize: usize,
) -> AxResult<isize> {
    check_sigset_size(sigsetsize)?;

    let signo = parse_signo(signo)?;
    if matches!(signo, Signo::SIGKILL | Signo::SIGSTOP) {
        return Err(AxError::InvalidInput);
    }

    let curr = current();
    let mut actions = curr.as_thread().proc_data.signal.actions.lock();
    if let Some(oldact) = oldact.nullable() {
        oldact.vm_write(actions[signo].clone().into())?;
    }
    if let Some(act) = act.nullable() {
        let act = unsafe { act.vm_read_uninit()?.assume_init() }.into();
        debug!("sys_rt_sigaction <= signo: {signo:?}, act: {act:?}");
        actions[signo] = act;
    }
    Ok(0)
}

pub fn sys_rt_sigpending(set: *mut SignalSet, sigsetsize: usize) -> AxResult<isize> {
    check_sigset_size(sigsetsize)?;
    if set.is_null() {
        // Linux `rt_sigpending(2)`：`set` must be a writable user buffer; NULL → EFAULT.
        return Err(AxError::BadAddress);
    }
    set.vm_write(current().as_thread().signal.pending())?;
    Ok(0)
}

fn make_siginfo(signo: u32, code: i32) -> AxResult<Option<SignalInfo>> {
    if signo == 0 {
        return Ok(None);
    }
    let signo = parse_signo(signo)?;
    Ok(Some(SignalInfo::new_user(
        signo,
        code,
        current().as_thread().proc_data.proc.pid(),
    )))
}

pub fn sys_kill(pid: i32, signo: u32) -> AxResult<isize> {
    debug!("sys_kill: pid = {pid}, signo = {signo}");
    let sig = make_siginfo(signo, SI_USER as _)?;

    match pid {
        1.. => {
            send_signal_to_process(pid as _, sig)?;
        }
        0 => {
            let pgid = current().as_thread().proc_data.proc.group().pgid();
            send_signal_to_process_group(pgid, sig)?;
        }
        -1 => {
            let curr_pid = current().as_thread().proc_data.proc.pid();
            if let Some(sig) = sig {
                for proc_data in processes() {
                    // POSIX.1 requires that kill(-1,sig) send sig to all processes that
                    //    the calling process may send signals to, except possibly for some
                    //    implementation-defined system processes.  Linux allows a process
                    //    to signal itself, but on Linux the call kill(-1,sig) does not
                    //    signal the calling process.
                    if proc_data.proc.is_init() || proc_data.proc.pid() == curr_pid {
                        continue;
                    }
                    let _ = send_signal_to_process(proc_data.proc.pid(), Some(sig.clone()));
                }
            }
        }
        ..-1 => {
            send_signal_to_process_group((-pid) as Pid, sig)?;
        }
    }
    Ok(0)
}

pub fn sys_tkill(tid: Pid, signo: u32) -> AxResult<isize> {
    let sig = make_siginfo(signo, SI_TKILL)?;
    send_signal_to_thread(None, tid, sig)?;
    Ok(0)
}

pub fn sys_tgkill(tgid: Pid, tid: Pid, signo: u32) -> AxResult<isize> {
    let sig = make_siginfo(signo, SI_TKILL)?;
    send_signal_to_thread(Some(tgid), tid, sig)?;
    Ok(0)
}

pub(crate) fn make_queue_signal_info(
    tgid: Pid,
    signo: u32,
    sig: *const SignalInfo,
) -> AxResult<Option<SignalInfo>> {
    if signo == 0 {
        return Ok(None);
    }

    let signo = parse_signo(signo)?;
    if sig.is_null() {
        // Linux queue-siginfo paths: non-zero signal requires readable `siginfo_t`; NULL → EFAULT.
        return Err(AxError::BadAddress);
    }
    let mut sig = unsafe { sig.vm_read_uninit()?.assume_init() };
    sig.set_signo(signo);
    if current().as_thread().proc_data.proc.pid() != tgid
        && (sig.code() >= 0 || sig.code() == SI_TKILL)
    {
        return Err(AxError::OperationNotPermitted);
    }
    Ok(Some(sig))
}

/// Linux `rt_sigqueueinfo` is 3-arg (`pid`, `sig`, `uinfo`); there is no `sigsetsize` slot.
pub fn sys_rt_sigqueueinfo(
    tgid: Pid,
    signo: u32,
    sig: *const SignalInfo,
) -> AxResult<isize> {
    let sig = make_queue_signal_info(tgid, signo, sig)?;
    send_signal_to_process(tgid, sig)?;
    Ok(0)
}

/// Linux `rt_tgsigqueueinfo` is 4-arg (`tgid`, `tid`, `sig`, `uinfo`); there is no `sigsetsize` slot.
pub fn sys_rt_tgsigqueueinfo(
    tgid: Pid,
    tid: Pid,
    signo: u32,
    sig: *const SignalInfo,
) -> AxResult<isize> {
    let sig = make_queue_signal_info(tgid, signo, sig)?;
    send_signal_to_thread(Some(tgid), tid, sig)?;
    Ok(0)
}

pub fn sys_rt_sigreturn(uctx: &mut UserContext) -> AxResult<isize> {
    block_next_signal();
    current().as_thread().signal.restore(uctx);
    Ok(uctx.retval() as isize)
}

pub fn sys_rt_sigtimedwait(
    uctx: &mut UserContext,
    set: *const SignalSet,
    info: *mut siginfo,
    timeout: *const timespec,
    sigsetsize: usize,
) -> AxResult<isize> {
    check_sigset_size(sigsetsize)?;

    if set.is_null() {
        // Linux `rt_sigtimedwait(2)`: `set` must be readable; NULL → EFAULT.
        return Err(AxError::BadAddress);
    }

    let set = unsafe { set.vm_read_uninit()?.assume_init() };

    let timeout = if let Some(ts) = timeout.nullable() {
        let ts = unsafe { ts.vm_read_uninit()?.assume_init() };
        Some(ts.try_into_time_value()?)
    } else {
        None
    };

    debug!("sys_rt_sigtimedwait => set = {set:?}, timeout = {timeout:?}");

    let curr = current();
    let thr = curr.as_thread();
    let signal = &thr.signal;

    let old_blocked = signal.blocked();
    signal.set_blocked(old_blocked & !set);

    uctx.set_retval(-LinuxError::EINTR.code() as usize);
    let fut = poll_fn(|cx| {
        if let Some(sig) = signal.dequeue_signal(&set) {
            signal.set_blocked(old_blocked);
            Poll::Ready(Some(sig))
        } else if check_signals(thr, uctx, Some(old_blocked)) {
            Poll::Ready(None)
        } else {
            let _ = curr.poll_interrupt(cx);
            Poll::Pending
        }
    });

    let Ok(sig) = block_on(future::timeout(timeout, fut)) else {
        // Timeout
        signal.set_blocked(old_blocked);
        return Err(AxError::WouldBlock);
    };
    let Some(sig) = sig else {
        // Interrupted
        return Ok(0);
    };

    if let Some(info) = info.nullable() {
        info.vm_write(sig.0)?;
    }

    Ok(sig.signo() as _)
}

pub fn sys_rt_sigsuspend(
    uctx: &mut UserContext,
    set: *const SignalSet,
    sigsetsize: usize,
) -> AxResult<isize> {
    check_sigset_size(sigsetsize)?;

    if set.is_null() {
        // Linux `rt_sigsuspend(2)` / `sigsuspend`: `set` must be readable; NULL → EFAULT.
        return Err(AxError::BadAddress);
    }

    let curr = current();
    let thr = curr.as_thread();

    let set = unsafe { set.vm_read_uninit()?.assume_init() };
    let old_blocked = thr.signal.set_blocked(set);

    // sigsuspend always returns -EINTR when a signal is caught
    // We set this in uctx before check_signals so it's saved in SignalFrame
    uctx.set_retval(-LinuxError::EINTR.code() as usize);

    block_on(poll_fn(|cx| {
        if check_signals(thr, uctx, Some(old_blocked)) {
            return Poll::Ready(());
        }
        let _ = curr.poll_interrupt(cx);
        Poll::Pending
    }));

    // sigsuspend always returns -EINTR
    Err(AxError::Interrupted)
}

pub fn sys_sigaltstack(ss: *const SignalStack, old_ss: *mut SignalStack) -> AxResult<isize> {
    let curr = current();
    let sig = &curr.as_thread().signal;

    if let Some(old_ss) = old_ss.nullable() {
        old_ss.vm_write(sig.stack())?;
    }

    if let Some(ss) = ss.nullable() {
        let ss = unsafe { ss.vm_read_uninit()?.assume_init() };
        // Linux EINVAL for ss_size below MINSIGSTKSZ (illegal stack_t), not ENOMEM.
        if ss.size < MINSIGSTKSZ as usize {
            return Err(AxError::InvalidInput);
        }
        sig.set_stack(ss);
    }
    Ok(0)
}
