use alloc::vec::Vec;
use core::{future::poll_fn, mem::size_of, slice, task::Poll};

use axerrno::{AxError, AxResult, LinuxError};
use axtask::{
    current,
    future::{block_on, interruptible},
};
use bitflags::bitflags;
use linux_raw_sys::general::{
    __WALL, __WCLONE, __WNOTHREAD, WCONTINUED, WEXITED, WNOHANG, WNOWAIT, WUNTRACED,
    __kernel_old_timeval, rusage,
};
use starry_process::{Pid, Process};
use starry_vm::{VmMutPtr, VmPtr, vm_write_slice};

use crate::{
    task::{AsThread, get_process_data, remove_zombie_process_data, time_value_from_nanos},
    time::TimeValueLike,
};

bitflags! {
    #[derive(Debug, Clone, Copy)]
    struct WaitOptions: u32 {
        /// Do not block when there are no processes wishing to report status.
        const WNOHANG = WNOHANG;
        /// Report the status of selected processes which are stopped due to a
        /// `SIGTTIN`, `SIGTTOU`, `SIGTSTP`, or `SIGSTOP` signal.
        const WUNTRACED = WUNTRACED;
        /// Report the status of selected processes which have terminated.
        const WEXITED = WEXITED;
        /// Report the status of selected processes that have continued from a
        /// job control stop by receiving a `SIGCONT` signal.
        const WCONTINUED = WCONTINUED;
        /// Don't reap, just poll status.
        const WNOWAIT = WNOWAIT;

        /// Don't wait on children of other threads in this group (rejected at syscall entry until we track parent tid).
        const WNOTHREAD = __WNOTHREAD;
        /// Wait on all children, regardless of type
        const WALL = __WALL;
        /// Wait for "clone" children only.
        const WCLONE = __WCLONE;
    }
}

#[derive(Debug, Clone, Copy)]
enum WaitPid {
    /// Wait for any child process
    Any,
    /// Wait for the child whose process ID is equal to the value.
    Pid(Pid),
    /// Wait for any child process whose process group ID is equal to the value.
    Pgid(Pid),
}

impl WaitPid {
    fn apply(&self, child: &Process) -> bool {
        match self {
            WaitPid::Any => true,
            WaitPid::Pid(pid) => child.pid() == *pid,
            WaitPid::Pgid(pgid) => child.group().pgid() == *pgid,
        }
    }
}

/// Linux `waitpid(2)` / `wait4(2)` child-kind filtering (`__WALL` / `__WCLONE`).
///
/// - **`__WALL`**: wait for any matching child; `__WCLONE` is ignored (Linux).
/// - **`__WCLONE` only**: only "clone" children (`ProcessData::is_clone_child`).
/// - **Neither**: only traditional children (`SIGCHLD` delivery), i.e. not `is_clone_child`.
///
/// **`__WNOTHREAD`** is handled in [`sys_waitpid`] (issue-219): not implemented here.
/// Linux `wait4(2)`: resource usage of the waited-for child (thread-group CPU at wait time;
/// other `rusage` fields not modeled yet, same as `getrusage` stubs in `resources.rs`).
fn rusage_from_child_cpu(utime_ns: usize, stime_ns: usize) -> rusage {
    let utime = time_value_from_nanos(utime_ns);
    let stime = time_value_from_nanos(stime_ns);
    rusage {
        ru_utime: __kernel_old_timeval::from_time_value(utime),
        ru_stime: __kernel_old_timeval::from_time_value(stime),
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    }
}

#[inline]
fn write_wait4_rusage(ru: *mut rusage, utime_ns: usize, stime_ns: usize) -> AxResult<()> {
    if ru.is_null() {
        return Ok(());
    }
    let k = rusage_from_child_cpu(utime_ns, stime_ns);
    let bytes = unsafe { slice::from_raw_parts((&k as *const rusage).cast::<u8>(), size_of::<rusage>()) };
    vm_write_slice(ru.cast::<u8>(), bytes)?;
    Ok(())
}

fn wait_child_matches_kind(child: &Process, options: &WaitOptions) -> bool {
    let is_clone = get_process_data(child.pid())
        .map(|pd| pd.is_clone_child())
        .unwrap_or(false);

    if options.contains(WaitOptions::WALL) {
        true
    } else if options.contains(WaitOptions::WCLONE) {
        is_clone
    } else {
        !is_clone
    }
}

pub fn sys_waitpid(
    pid: i32,
    exit_code: *mut i32,
    options: u32,
    ru: *mut rusage,
) -> AxResult<isize> {
    let options = WaitOptions::from_bits(options).ok_or(AxError::InvalidInput)?;
    if options.contains(WaitOptions::WNOTHREAD) {
        // issue-219: Linux excludes children not created by the calling thread's clones;
        // we do not record fork/clone parent thread id — fail explicitly instead of ignoring the bit.
        return Err(AxError::Unsupported);
    }
    info!("sys_waitpid <= pid: {pid:?}, options: {options:?}");

    let curr = current();
    let proc_data = &curr.as_thread().proc_data;
    let proc = &proc_data.proc;

    let pid = if pid == -1 {
        WaitPid::Any
    } else if pid == 0 {
        WaitPid::Pgid(proc.group().pgid())
    } else if pid > 0 {
        WaitPid::Pid(pid as _)
    } else {
        WaitPid::Pgid(-pid as _)
    };

    let children = proc
        .children()
        .into_iter()
        .filter(|child| pid.apply(child) && wait_child_matches_kind(child, &options))
        .collect::<Vec<_>>();
    if children.is_empty() {
        return Err(AxError::from(LinuxError::ECHILD));
    }

    let check_children = || {
        if let Some(child) = children.iter().find(|c| c.is_zombie()) {
            let cpu = get_process_data(child.pid())
                .map(|pd| pd.thread_group_cpu_nanos())
                .unwrap_or((0, 0));
            if !options.contains(WaitOptions::WNOWAIT) {
                proc_data.accumulate_waited_child_cpu_ns(cpu.0, cpu.1);
                child.free();
                remove_zombie_process_data(child.pid());
            }
            if let Some(exit_code) = exit_code.nullable() {
                exit_code.vm_write(child.exit_code())?;
            }
            write_wait4_rusage(ru, cpu.0, cpu.1)?;
            return Ok(Some(child.pid() as _));
        }

        let report_stop = options.contains(WaitOptions::WUNTRACED) || options.is_empty();
        let report_continued = options.contains(WaitOptions::WCONTINUED) || options.is_empty();

        for child in &children {
            if child.is_zombie() {
                continue;
            }
            let Ok(data) = get_process_data(child.pid()) else {
                continue;
            };
            let mut jc = data.jobctl.lock();
            if report_stop && jc.stop_wait_pending {
                let (cu, cs) = data.thread_group_cpu_nanos();
                if let Some(ec) = exit_code.nullable() {
                    let sig = jc.stop_sig.unwrap_or(0) as i32;
                    let st = (sig << 8) | 0x7f;
                    ec.vm_write(st)?;
                }
                jc.stop_wait_pending = false;
                write_wait4_rusage(ru, cu, cs)?;
                return Ok(Some(child.pid() as _));
            }
            if report_continued && jc.continued_wait_pending {
                let (cu, cs) = data.thread_group_cpu_nanos();
                if let Some(ec) = exit_code.nullable() {
                    ec.vm_write(0xffff)?;
                }
                jc.continued_wait_pending = false;
                write_wait4_rusage(ru, cu, cs)?;
                return Ok(Some(child.pid() as _));
            }
        }

        if options.contains(WaitOptions::WNOHANG) {
            Ok(Some(0))
        } else {
            Ok(None)
        }
    };

    block_on(interruptible(poll_fn(|cx| {
        match check_children().transpose() {
            Some(res) => Poll::Ready(res),
            None => {
                proc_data.child_exit_event.register(cx.waker());
                Poll::Pending
            }
        }
    })))?
}
