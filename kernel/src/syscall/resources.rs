use alloc::sync::Arc;

use axerrno::{AxError, AxResult};
use axhal::time::TimeValue;
use axtask::current;
use linux_raw_sys::general::{__kernel_old_timeval, RLIM_NLIMITS, rlimit64, rusage};
use starry_process::Pid;
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    mm::AddrSpace,
    task::{AsThread, ProcessData, Thread, get_process_data, get_task, time_value_from_nanos},
    time::TimeValueLike,
};

/// Linux `prlimit(2)`: cross-process access requires same thread group, `CAP_SYS_RESOURCE`,
/// root effective uid, or matching real uid (`man 2 prlimit`; `do_prlimit` / LSM).
#[inline]
fn may_prlimit_peer(caller: &Arc<ProcessData>, target: &Arc<ProcessData>) -> bool {
    if Arc::ptr_eq(caller, target) {
        return true;
    }
    const CAP_SYS_RESOURCE: u32 = 24;
    if caller.geteuid() == 0 {
        return true;
    }
    let (eff, _, _) = caller.get_capabilities();
    if eff & (1 << CAP_SYS_RESOURCE) != 0 {
        return true;
    }
    caller.getuid() == target.getuid()
}

#[inline]
fn rss_kb_from_aspace(aspace: &AddrSpace) -> i64 {
    aspace.resident_set_size_kb().try_into().unwrap_or(i64::MAX)
}

pub fn sys_prlimit64(
    pid: Pid,
    resource: u32,
    new_limit: *const rlimit64,
    old_limit: *mut rlimit64,
) -> AxResult<isize> {
    if resource >= RLIM_NLIMITS {
        return Err(AxError::InvalidInput);
    }

    let proc_data = get_process_data(pid)?;
    let caller_pd = current().as_thread().proc_data.clone();
    if !may_prlimit_peer(&caller_pd, &proc_data) {
        return Err(AxError::OperationNotPermitted);
    }

    // Linux do_prlimit: copy_from_user(new) + validate + apply before copy_to_user(old), so
    // EFAULT/EINVAL/EPERM on new do not write old (issue-141).
    let old_snapshot = old_limit.nullable().map(|_| {
        let limit = &proc_data.rlim.read()[resource];
        rlimit64 {
            rlim_cur: limit.current,
            rlim_max: limit.max,
        }
    });

    if let Some(user_new) = new_limit.nullable() {
        // Linux `struct rlimit64` is two `__u64` fields; read by scalar to avoid
        // `AnyBitPattern` / whole-struct `assume_init` if uapi padding ever changes.
        let p = user_new.cast::<u64>();
        let rlim_cur = p.vm_read()?;
        let rlim_max = unsafe { p.add(1) }.vm_read()?;
        let new_limit = rlimit64 { rlim_cur, rlim_max };
        if new_limit.rlim_cur > new_limit.rlim_max {
            return Err(AxError::InvalidInput);
        }

        let limit = &mut proc_data.rlim.write()[resource];
        if new_limit.rlim_max > limit.max {
            // Linux do_prlimit: unprivileged raise of hard limit above current cap → EPERM.
            return Err(AxError::OperationNotPermitted);
        }
        limit.max = new_limit.rlim_max;
        limit.current = new_limit.rlim_cur;
    }

    if let (Some(old_out), Some(snapshot)) = (old_limit.nullable(), old_snapshot) {
        old_out.vm_write(snapshot)?;
    }

    Ok(0)
}

/// CPU and memory stats for `getrusage(2)`.
///
/// Linux also exposes `ru_minflt` / `ru_majflt` / `ru_nvcsw` / `ru_nivcsw` / etc.
/// Those are not wired in Starry yet; they remain zero in the returned `rusage`.
#[derive(Default)]
struct Rusage {
    utime: TimeValue,
    stime: TimeValue,
    /// `ru_maxrss` in kilobytes (Linux ABI). Zero means unset (e.g. `RUSAGE_CHILDREN`).
    ru_maxrss_kb: i64,
}

impl Rusage {
    fn from_thread(thread: &Thread) -> Self {
        let (utime, stime) = thread.time.borrow().output();
        Self {
            utime,
            stime,
            ru_maxrss_kb: 0,
        }
    }

    fn collate(mut self, other: Rusage) -> Self {
        self.utime += other.utime;
        self.stime += other.stime;
        self
    }
}

impl From<Rusage> for rusage {
    fn from(value: Rusage) -> Self {
        Self {
            ru_utime: __kernel_old_timeval::from_time_value(value.utime),
            ru_stime: __kernel_old_timeval::from_time_value(value.stime),
            ru_maxrss: value.ru_maxrss_kb as _,
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
}

pub fn sys_getrusage(who: i32, usage: *mut rusage) -> AxResult<isize> {
    const RUSAGE_SELF: i32 = linux_raw_sys::general::RUSAGE_SELF as i32;
    const RUSAGE_CHILDREN: i32 = linux_raw_sys::general::RUSAGE_CHILDREN;
    const RUSAGE_THREAD: i32 = linux_raw_sys::general::RUSAGE_THREAD as i32;

    match who {
        RUSAGE_SELF | RUSAGE_CHILDREN | RUSAGE_THREAD => {}
        _ => return Err(AxError::InvalidInput),
    }
    if usage.is_null() {
        // Linux `getrusage(2)`: `usage` must be writable; NULL → EFAULT (after `who` is valid).
        return Err(AxError::BadAddress);
    }

    let curr = current();
    let thr = curr.as_thread();

    let result = match who {
        RUSAGE_SELF => {
            let rss_kb = rss_kb_from_aspace(&thr.proc_data.aspace.read());
            let mut u = thr
                .proc_data
                .proc
                .threads()
                .into_iter()
                .fold(Rusage::default(), |acc, tid| {
                    if let Ok(task) = get_task(tid) {
                        acc.collate(Rusage::from_thread(task.as_thread()))
                    } else {
                        acc
                    }
                });
            u.ru_maxrss_kb = rss_kb;
            u
        }
        RUSAGE_CHILDREN => {
            // Linux: resources of terminated and waited-for children only — not sibling pthreads.
            let (cu_ns, cs_ns) = thr.proc_data.waited_children_cpu_nanos();
            Rusage {
                utime: time_value_from_nanos(cu_ns),
                stime: time_value_from_nanos(cs_ns),
                ..Default::default()
            }
        }
        RUSAGE_THREAD => {
            let rss_kb = rss_kb_from_aspace(&thr.proc_data.aspace.read());
            let mut u = Rusage::from_thread(thr);
            u.ru_maxrss_kb = rss_kb;
            u
        }
        _ => return Err(AxError::InvalidInput),
    };
    usage.vm_write(result.into())?;

    Ok(0)
}
