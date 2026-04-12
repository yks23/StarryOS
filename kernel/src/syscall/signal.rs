use core::{future::poll_fn, mem::MaybeUninit, slice, task::Poll};

use axerrno::{AxError, AxResult, LinuxError};
use axhal::uspace::UserContext;
use axtask::{
    current,
    future::{self, block_on},
};
use linux_raw_sys::general::{
    MINSIGSTKSZ, SI_TKILL, SI_USER, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK, __kernel_sighandler_t,
    __sifields, kernel_sigaction, kernel_sigset_t, siginfo, siginfo__bindgen_ty_1,
    siginfo__bindgen_ty_1__bindgen_ty_1, timespec,
};
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "s390x",
    target_arch = "arm",
    target_arch = "aarch64",
))]
use linux_raw_sys::general::__sigrestore_t;
use starry_process::Pid;
use starry_signal::{SignalInfo, SignalSet, SignalStack, Signo};
use starry_vm::{VmMutPtr, VmPtr, vm_read_slice};

use crate::{
    task::{
        AsThread, block_next_signal, check_signals, processes, send_signal_to_process,
        send_signal_to_process_group, send_signal_to_thread,
    },
    time::{TimeValueLike, read_timespec_user},
};

/// Matches `starry-signal` `build.rs` / `cfg(sa_restorer)` for `kernel_sigaction` layout.
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "s390x",
    target_arch = "arm",
    target_arch = "aarch64",
))]
fn read_kernel_sigaction_user(p: *const kernel_sigaction) -> AxResult<kernel_sigaction> {
    // `sa_handler` / `sa_restorer` are `Option<fn>` (not `AnyBitPattern`); Linux stores them as
    // pointer-sized words — read `usize` and preserve bit pattern (issue-206).
    let sa_handler_kernel = {
        let bits = unsafe {
            core::ptr::addr_of!((*p).sa_handler_kernel)
                .cast::<usize>()
                .vm_read()?
        };
        unsafe { core::mem::transmute::<usize, __kernel_sighandler_t>(bits) }
    };
    unsafe {
        Ok(kernel_sigaction {
            sa_handler_kernel,
            sa_flags: core::ptr::addr_of!((*p).sa_flags).vm_read()?,
            sa_restorer: {
                let bits = core::ptr::addr_of!((*p).sa_restorer)
                    .cast::<usize>()
                    .vm_read()?;
                core::mem::transmute::<usize, __sigrestore_t>(bits)
            },
            sa_mask: kernel_sigset_t {
                sig: [core::ptr::addr_of!((*p).sa_mask.sig[0]).vm_read()?],
            },
        })
    }
}

#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "s390x",
    target_arch = "arm",
    target_arch = "aarch64",
)))]
fn read_kernel_sigaction_user(p: *const kernel_sigaction) -> AxResult<kernel_sigaction> {
    let sa_handler_kernel = {
        let bits = unsafe {
            core::ptr::addr_of!((*p).sa_handler_kernel)
                .cast::<usize>()
                .vm_read()?
        };
        unsafe { core::mem::transmute::<usize, __kernel_sighandler_t>(bits) }
    };
    unsafe {
        Ok(kernel_sigaction {
            sa_handler_kernel,
            sa_flags: core::ptr::addr_of!((*p).sa_flags).vm_read()?,
            sa_mask: kernel_sigset_t {
                sig: [core::ptr::addr_of!((*p).sa_mask.sig[0]).vm_read()?],
            },
        })
    }
}

pub(crate) fn read_signal_set_user(p: *const SignalSet) -> AxResult<SignalSet> {
    let p = p.cast::<kernel_sigset_t>();
    let word = unsafe { core::ptr::addr_of!((*p).sig[0]).vm_read()? };
    Ok(kernel_sigset_t { sig: [word] }.into())
}

/// User `siginfo_t` for queue paths (issue-212): `si_*` scalars + `_sifields` blob — no bulk
/// `assume_init` over the full `SignalInfo`.
fn read_signal_info_user(p: *const SignalInfo) -> AxResult<SignalInfo> {
    let p = p.cast::<siginfo>();
    let si_signo = unsafe { core::ptr::addr_of!((*p).__bindgen_anon_1.__bindgen_anon_1.si_signo).vm_read()? };
    let si_errno = unsafe { core::ptr::addr_of!((*p).__bindgen_anon_1.__bindgen_anon_1.si_errno).vm_read()? };
    let si_code = unsafe { core::ptr::addr_of!((*p).__bindgen_anon_1.__bindgen_anon_1.si_code).vm_read()? };
    let mut sifields = MaybeUninit::<__sifields>::uninit();
    unsafe {
        vm_read_slice(
            core::ptr::addr_of!((*p).__bindgen_anon_1.__bindgen_anon_1._sifields),
            slice::from_mut(&mut sifields),
        )?;
    }
    let inner = siginfo__bindgen_ty_1__bindgen_ty_1 {
        si_signo,
        si_errno,
        si_code,
        _sifields: unsafe { sifields.assume_init() },
    };
    Ok(SignalInfo(siginfo {
        __bindgen_anon_1: siginfo__bindgen_ty_1 {
            __bindgen_anon_1: inner,
        },
    }))
}

fn read_signal_stack_user(p: *const SignalStack) -> AxResult<SignalStack> {
    unsafe {
        Ok(SignalStack {
            sp: core::ptr::addr_of!((*p).sp).vm_read()?,
            flags: core::ptr::addr_of!((*p).flags).vm_read()?,
            size: core::ptr::addr_of!((*p).size).vm_read()?,
        })
    }
}

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
            let set = read_signal_set_user(set_ptr)?;
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
    let old = actions[signo].clone();

    match act.nullable() {
        None => {
            if let Some(oldact) = oldact.nullable() {
                oldact.vm_write(old.into())?;
            }
        }
        Some(act_ptr) => {
            // Linux do_rt_sigaction: copy_from_user(act) before copy_to_user(oldact); EFAULT on
            // `act` must not clobber `oldact` (issue-147).
            let act = read_kernel_sigaction_user(act_ptr)?.into();
            debug!("sys_rt_sigaction <= signo: {signo:?}, act: {act:?}");
            actions[signo] = act;
            if let Some(oldact) = oldact.nullable() {
                oldact.vm_write(old.into())?;
            }
        }
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
    // Linux `tkill(2)`: `tid` must be a valid thread ID; `0` is EINVAL (not "current thread";
    // `get_task(0)` maps to `current()` for other syscalls only).
    if tid == 0 {
        return Err(AxError::InvalidInput);
    }
    let sig = make_siginfo(signo, SI_TKILL)?;
    send_signal_to_thread(None, tid, sig)?;
    Ok(0)
}

pub fn sys_tgkill(tgid: Pid, tid: Pid, signo: u32) -> AxResult<isize> {
    // Linux `tgkill(2)`: `tid` must be non-zero; `0` is EINVAL (issue-249).
    if tid == 0 {
        return Err(AxError::InvalidInput);
    }
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
    let mut sig = read_signal_info_user(sig)?;
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
    // Linux `do_rt_sigtimedwait` / `copy_sigset_from_user`: NULL `set` → EFAULT before bad
    // `sigsetsize` → EINVAL (issue-338; same theme as signalfd issue-321 / ppoll issue-334).
    if set.is_null() {
        return Err(AxError::BadAddress);
    }

    check_sigset_size(sigsetsize)?;

    let set = read_signal_set_user(set)?;

    let timeout = if let Some(ts) = timeout.nullable() {
        let ts = read_timespec_user(ts)?;
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

    let set = read_signal_set_user(set)?;
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

    // Linux `do_sigaltstack`: validate/read new `stack_t` before `copy_to_user(old)` so EFAULT/EINVAL
    // on the new `ss` does not expose a partially updated `old_ss` (issue-337; issue-244/098 semantics).
    if let Some(ss) = ss.nullable() {
        let ss = read_signal_stack_user(ss)?;
        // Linux EINVAL for ss_size below MINSIGSTKSZ (illegal stack_t), not ENOMEM.
        if ss.size < MINSIGSTKSZ as usize {
            return Err(AxError::InvalidInput);
        }
        if let Some(old_ss) = old_ss.nullable() {
            old_ss.vm_write(sig.stack())?;
        }
        sig.set_stack(ss);
    } else {
        // Linux `sigaltstack(2)`: `ss == NULL` disables the alternate stack (equivalent to `SS_DISABLE`).
        if let Some(old_ss) = old_ss.nullable() {
            old_ss.vm_write(sig.stack())?;
        }
        sig.set_stack(SignalStack::default());
    }
    Ok(0)
}
