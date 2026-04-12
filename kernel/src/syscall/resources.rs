use axerrno::{AxError, AxResult};
use axhal::time::TimeValue;
use axtask::current;
use linux_raw_sys::general::{__kernel_old_timeval, RLIM_NLIMITS, rlimit64, rusage};
use starry_process::Pid;
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    mm::AddrSpace,
    task::{AsThread, Thread, get_process_data, get_task, time_value_from_nanos},
    time::TimeValueLike,
};

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

    // Linux do_prlimit: copy_from_user(new) + validate + apply before copy_to_user(old), so
    // EFAULT/EINVAL/EPERM on new do not write old (issue-141).
    let old_snapshot = old_limit.nullable().map(|_| {
        let limit = &proc_data.rlim.read()[resource];
        rlimit64 {
            rlim_cur: limit.current,
            rlim_max: limit.max,
        }
    });

    if let Some(new_limit) = new_limit.nullable() {
        // FIXME: AnyBitPattern
        let new_limit = unsafe { new_limit.vm_read_uninit()?.assume_init() };
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
        // FIXME: Zeroable
        let mut usage: rusage = unsafe { core::mem::zeroed() };
        usage.ru_utime = __kernel_old_timeval::from_time_value(value.utime);
        usage.ru_stime = __kernel_old_timeval::from_time_value(value.stime);
        usage.ru_maxrss = value.ru_maxrss_kb as _;
        usage
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
