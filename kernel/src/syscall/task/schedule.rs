use axerrno::{AxError, AxResult};
use axhal::time::TimeValue;
use axtask::{
    AxCpuMask, AxTaskRef, current,
    future::{block_on, interruptible, sleep},
};
use bytemuck::{Pod, Zeroable};
use linux_raw_sys::general::{
    __kernel_clockid_t, CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_MONOTONIC_COARSE,
    CLOCK_MONOTONIC_RAW, CLOCK_REALTIME, PRIO_PGRP, PRIO_PROCESS, PRIO_USER, SCHED_BATCH,
    SCHED_FIFO, SCHED_IDLE, SCHED_NORMAL, SCHED_RR, TIMER_ABSTIME, timespec,
};
use starry_process::Pid;
use starry_vm::{VmMutPtr, VmPtr, vm_load, vm_write_slice};

use crate::{
    task::{AsThread, ProcessData, get_process_data, get_process_group, get_task, processes},
    time::{TimeValueLike, read_timespec_user},
};

/// Linux `struct sched_param` (user ABI): single `sched_priority` field.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SchedParam {
    sched_priority: i32,
}

/// User `sched_param`: only `sched_priority` is ABI (issue-211); avoid bulk `assume_init` on the struct.
fn read_sched_param_user(p: *const SchedParam) -> AxResult<SchedParam> {
    unsafe {
        Ok(SchedParam {
            sched_priority: core::ptr::addr_of!((*p).sched_priority).vm_read()?,
        })
    }
}

fn validate_sched_user_param(policy: i32, priority: i32) -> AxResult<()> {
    match policy as u32 {
        SCHED_NORMAL | SCHED_BATCH | SCHED_IDLE => {
            if priority != 0 {
                return Err(AxError::InvalidInput);
            }
            Ok(())
        }
        SCHED_FIFO | SCHED_RR => {
            if !(1..=99).contains(&priority) {
                return Err(AxError::InvalidInput);
            }
            Ok(())
        }
        _ => Err(AxError::InvalidInput),
    }
}

/// Linux `sched_*affinity` `pid`: `0` is the calling task; otherwise a TID, or a thread-group PID
/// (we fall back to the smallest member TID, typically the leader).
fn sched_resolve_task(pid: i32) -> AxResult<AxTaskRef> {
    if pid < 0 {
        return Err(AxError::InvalidInput);
    }
    if pid == 0 {
        return get_task(0);
    }
    let pid_u = pid as Pid;
    match get_task(pid_u) {
        Ok(t) => Ok(t),
        Err(AxError::NoSuchProcess) => {
            let pdata = get_process_data(pid_u)?;
            let tid = pdata
                .proc
                .threads()
                .into_iter()
                .min()
                .ok_or(AxError::NoSuchProcess)?;
            get_task(tid)
        }
        Err(e) => Err(e),
    }
}

pub fn sys_sched_yield() -> AxResult<isize> {
    axtask::yield_now();
    Ok(0)
}

#[inline]
fn time_value_is_zero(tv: TimeValue) -> bool {
    tv.as_secs() == 0 && tv.subsec_nanos() == 0
}

/// Waits for `dur` using [`axtask::future::sleep`], which advances on the **monotonic** timeline.
/// `clock` must be [`axhal::time::monotonic_time`] so elapsed/remainder match that sleep (see
/// [`sys_clock_nanosleep`] for `CLOCK_REALTIME`, which is not driven by this primitive).
fn sleep_impl(clock: impl Fn() -> TimeValue, dur: TimeValue) -> TimeValue {
    debug!("sleep_impl <= {dur:?}");

    if time_value_is_zero(dur) {
        return TimeValue::new(0, 0);
    }

    let start = clock();
    // EINTR is detected below if slept time falls short of `dur`.
    let _ = block_on(interruptible(sleep(dur)));

    clock() - start
}

/// Sleep some nanoseconds (POSIX/Linux: interval on the monotonic clock).
pub fn sys_nanosleep(req: *const timespec, rem: *mut timespec) -> AxResult<isize> {
    let req = read_timespec_user(req)?.try_into_time_value()?;
    debug!("sys_nanosleep <= req: {req:?}");

    let actual = sleep_impl(axhal::time::monotonic_time, req);

    if let Some(diff) = req.checked_sub(actual) {
        debug!("sys_nanosleep => rem: {diff:?}");
        if let Some(rem) = rem.nullable() {
            rem.vm_write(timespec::from_time_value(diff))?;
        }
        Err(AxError::Interrupted)
    } else {
        Ok(0)
    }
}

pub fn sys_clock_nanosleep(
    clock_id: __kernel_clockid_t,
    flags: u32,
    req: *const timespec,
    rem: *mut timespec,
) -> AxResult<isize> {
    // Linux `clock_nanosleep(2)`：仅允许 0 或 `TIMER_ABSTIME`（与 `timerfd_settime` 策略一致，issue-180）。
    if flags & !TIMER_ABSTIME != 0 {
        return Err(AxError::InvalidInput);
    }
    let req = read_timespec_user(req)?.try_into_time_value()?;
    debug!("sys_clock_nanosleep <= clock_id: {clock_id}, flags: {flags}, req: {req:?}");

    let id = clock_id as u32;
    // Align with `sys_clock_gettime` (issue-250, issue-263): HAL has one monotonic counter, so
    // BOOTTIME / MONOTONIC_* / RAW / COARSE share `monotonic_time` for sleep timelines.
    let clock = match id {
        CLOCK_REALTIME => axhal::time::wall_time,
        CLOCK_MONOTONIC
        | CLOCK_MONOTONIC_RAW
        | CLOCK_MONOTONIC_COARSE
        | CLOCK_BOOTTIME => axhal::time::monotonic_time,
        _ => {
            warn!("Unsupported clock_id: {clock_id}");
            return Err(AxError::InvalidInput);
        }
    };

    let dur = if flags & TIMER_ABSTIME != 0 {
        req.saturating_sub(clock())
    } else {
        req
    };

    // Wall-clock sleeps are not implemented: `sleep(dur)` is monotonic-based. Reject non-trivial
    // `CLOCK_REALTIME` waits (issue-071); zero-duration / already-expired absolute waits return Ok(0).
    if id == CLOCK_REALTIME {
        if time_value_is_zero(dur) {
            return Ok(0);
        }
        return Err(AxError::Unsupported);
    }

    let actual = sleep_impl(clock, dur);

    if let Some(diff) = dur.checked_sub(actual) {
        debug!("sys_clock_nanosleep => rem: {diff:?}");
        if let Some(rem) = rem.nullable() {
            rem.vm_write(timespec::from_time_value(diff))?;
        }
        Err(AxError::Interrupted)
    } else {
        Ok(0)
    }
}

pub fn sys_sched_getaffinity(pid: i32, cpusetsize: usize, user_mask: *mut u8) -> AxResult<isize> {
    if cpusetsize * 8 < axhal::cpu_num() {
        return Err(AxError::InvalidInput);
    }

    let task = sched_resolve_task(pid)?;
    let mask = task.cpumask();
    let mask_bytes = mask.as_bytes();

    // NULL output buffer → **EFAULT** (`BadAddress`), same as `fstatat`/`capget` (issue-283, issue-304,
    // issue-306); after `sched_resolve_task` so **ESRCH** still wins for bad `pid`.
    if user_mask.is_null() {
        return Err(AxError::BadAddress);
    }
    vm_write_slice(user_mask, mask_bytes)?;

    // Linux `sched_getaffinity(2)`: success returns 0; the mask is only in user memory.
    Ok(0)
}

pub fn sys_sched_setaffinity(pid: i32, cpusetsize: usize, user_mask: *const u8) -> AxResult<isize> {
    // Linux sched_setaffinity: resolve pid (ESRCH) before copy_from_user(mask) (EFAULT).
    let task = sched_resolve_task(pid)?;
    let size = cpusetsize.min(axhal::cpu_num().div_ceil(8));
    let user_mask = vm_load(user_mask, size)?;
    let mut cpu_mask = AxCpuMask::new();

    for i in 0..(size * 8).min(axhal::cpu_num()) {
        if user_mask[i / 8] & (1 << (i % 8)) != 0 {
            cpu_mask.set(i, true);
        }
    }
    if task.id() == current().id() {
        if !axtask::set_current_affinity(cpu_mask) {
            return Err(AxError::InvalidInput);
        }
    } else {
        if cpu_mask.is_empty() {
            return Err(AxError::InvalidInput);
        }
        task.set_cpumask(cpu_mask);
    }

    Ok(0)
}

pub fn sys_sched_getscheduler(pid: i32) -> AxResult<isize> {
    let task = sched_resolve_task(pid)?;
    let thr = task.try_as_thread().ok_or(AxError::InvalidInput)?;
    Ok(thr.sched_policy() as isize)
}

pub fn sys_sched_setscheduler(pid: i32, policy: i32, param: *const ()) -> AxResult<isize> {
    let param = param.cast::<SchedParam>();
    let Some(param_ptr) = param.nullable() else {
        return Err(AxError::InvalidInput);
    };
    // Linux: find task (ESRCH) before copy_from_user(sched_param) (EFAULT); NULL param → EINVAL above.
    let task = sched_resolve_task(pid)?;
    let thr = task.try_as_thread().ok_or(AxError::InvalidInput)?;
    let user_param = read_sched_param_user(param_ptr)?;
    validate_sched_user_param(policy, user_param.sched_priority)?;
    thr.set_sched_policy_param(policy, user_param.sched_priority);
    Ok(0)
}

pub fn sys_sched_getparam(pid: i32, param: *mut ()) -> AxResult<isize> {
    let param = param.cast::<SchedParam>();
    let Some(param_ptr) = param.nullable() else {
        return Err(AxError::InvalidInput);
    };
    let task = sched_resolve_task(pid)?;
    let thr = task.try_as_thread().ok_or(AxError::InvalidInput)?;
    param_ptr.vm_write(SchedParam {
        sched_priority: thr.sched_priority_value(),
    })?;
    Ok(0)
}

fn min_nice_among<'a>(it: impl Iterator<Item = &'a ProcessData>) -> Option<i32> {
    it.map(ProcessData::get_nice).reduce(|a, b| a.min(b))
}

/// Linux `nice_to_rlimit()` for [`getpriority(2)`]: maps nice in **-20..=19** to **1..=40**.
#[inline]
fn linux_getpriority_ret(nice: i32) -> isize {
    (20 - nice) as isize
}

pub fn sys_getpriority(which: u32, who: u32) -> AxResult<isize> {
    debug!("sys_getpriority <= which: {which}, who: {who}");

    match which {
        PRIO_PROCESS => {
            let pdata = if who == 0 {
                current().as_thread().proc_data.clone()
            } else {
                get_process_data(who)?
            };
            Ok(linux_getpriority_ret(pdata.get_nice()))
        }
        PRIO_PGRP => {
            let pgid = if who == 0 {
                current().as_thread().proc_data.proc.group().pgid()
            } else {
                who
            };
            let _pg = get_process_group(pgid)?;
            let n = min_nice_among(
                processes()
                    .iter()
                    .filter_map(|p| (p.proc.group().pgid() == pgid).then_some(p.as_ref())),
            )
            .unwrap_or(0);
            Ok(linux_getpriority_ret(n))
        }
        PRIO_USER => {
            let uid = if who == 0 {
                current().as_thread().proc_data.geteuid()
            } else {
                who
            };
            let Some(n) = min_nice_among(
                processes()
                    .iter()
                    .filter_map(|p| (p.geteuid() == uid).then_some(p.as_ref())),
            ) else {
                return Err(AxError::NoSuchProcess);
            };
            Ok(linux_getpriority_ret(n))
        }
        _ => Err(AxError::InvalidInput),
    }
}

pub fn sys_setpriority(which: u32, who: u32, nice: i32) -> AxResult<isize> {
    debug!("sys_setpriority <= which: {which}, who: {who}, nice: {nice}");
    if !(-20..=19).contains(&nice) {
        return Err(AxError::InvalidInput);
    }

    match which {
        PRIO_PROCESS => {
            let pdata = if who == 0 {
                current().as_thread().proc_data.clone()
            } else {
                get_process_data(who)?
            };
            pdata.set_nice(nice);
            Ok(0)
        }
        PRIO_PGRP => {
            let pgid = if who == 0 {
                current().as_thread().proc_data.proc.group().pgid()
            } else {
                who
            };
            let _pg = get_process_group(pgid)?;
            for p in processes() {
                if p.proc.group().pgid() == pgid {
                    p.set_nice(nice);
                }
            }
            Ok(0)
        }
        PRIO_USER => {
            let uid = if who == 0 {
                current().as_thread().proc_data.geteuid()
            } else {
                who
            };
            let mut any = false;
            for p in processes() {
                if p.geteuid() == uid {
                    p.set_nice(nice);
                    any = true;
                }
            }
            if !any {
                return Err(AxError::NoSuchProcess);
            }
            Ok(0)
        }
        _ => Err(AxError::InvalidInput),
    }
}
