use axerrno::{AxError, AxResult};
use axhal::time::{TimeValue, monotonic_time, monotonic_time_nanos, wall_time};
use axtask::current;
use linux_raw_sys::general::{
    __kernel_clock_t, __kernel_clockid_t, CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_MONOTONIC_COARSE,
    CLOCK_MONOTONIC_RAW, CLOCK_PROCESS_CPUTIME_ID, CLOCK_REALTIME, CLOCK_REALTIME_COARSE,
    CLOCK_THREAD_CPUTIME_ID, itimerval, timespec, timeval, timezone,
};
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    task::{AsThread, ITimerType, time_value_from_nanos},
    time::{TimeValueLike, read_timeval_user},
};

/// Linux `USER_HZ` / `_SC_CLK_TCK` for `times(2)`: `struct tms` fields and the syscall return value
/// are `clock_t` jiffies, not microseconds (issue-224).
const USER_HZ: u64 = 100;

#[inline]
fn cpu_nanos_to_clock_t(ns: usize) -> __kernel_clock_t {
    let v = (ns as u128)
        .saturating_mul(USER_HZ as u128)
        / 1_000_000_000u128;
    // Linux `struct tms` uses signed `clock_t`; jiffies are non-negative. Clamp to signed range
    // (issue-287; follow-up to issue-224 `usize` → uapi alignment).
    let v = v.min(i64::MAX as u128) as i64;
    v as __kernel_clock_t
}

#[inline]
fn monotonic_nanos_to_jiffies(nanos: u64) -> u64 {
    nanos.saturating_mul(USER_HZ) / 1_000_000_000
}

fn clock_id_supported(clock_id: u32) -> bool {
    matches!(
        clock_id,
        CLOCK_REALTIME
            | CLOCK_REALTIME_COARSE
            | CLOCK_MONOTONIC
            | CLOCK_MONOTONIC_RAW
            | CLOCK_MONOTONIC_COARSE
            | CLOCK_BOOTTIME
            | CLOCK_PROCESS_CPUTIME_ID
            | CLOCK_THREAD_CPUTIME_ID
    )
}

/// `CLOCK_MONOTONIC` / `CLOCK_MONOTONIC_COARSE` — HAL monotonic counter (`axhal::time::monotonic_time`).
#[inline]
fn clock_read_monotonic() -> TimeValue {
    monotonic_time()
}

/// `CLOCK_MONOTONIC_RAW` — Linux exposes hardware time without NTP/frequency adjustment; Starry has no
/// separate skew layer on top of the HAL counter, so this matches [`clock_read_monotonic`] (issue-250).
#[inline]
fn clock_read_monotonic_raw() -> TimeValue {
    monotonic_time()
}

/// `CLOCK_BOOTTIME` — Linux includes suspend time; Starry does not track S-state duration, so this
/// matches [`clock_read_monotonic`] (issue-250).
#[inline]
fn clock_read_boottime() -> TimeValue {
    monotonic_time()
}

/// Resolution reported by [`sys_clock_getres`], aligned with Linux conventions:
/// `*_COARSE` clocks follow jiffies / HZ-scale granularity (here **1 ms**); others match
/// typical high-resolution / hrtimer **`1 ns`** reports.
fn clock_get_resolution(clock_id: u32) -> TimeValue {
    match clock_id {
        CLOCK_REALTIME_COARSE | CLOCK_MONOTONIC_COARSE => TimeValue::from_millis(1),
        CLOCK_REALTIME
        | CLOCK_MONOTONIC
        | CLOCK_MONOTONIC_RAW
        | CLOCK_BOOTTIME
        | CLOCK_PROCESS_CPUTIME_ID
        | CLOCK_THREAD_CPUTIME_ID => TimeValue::new(0, 1),
        _ => TimeValue::from_micros(1),
    }
}

pub fn sys_clock_gettime(clock_id: __kernel_clockid_t, ts: *mut timespec) -> AxResult<isize> {
    // Linux `common_clock_get` / `put_user`: reject NULL `tp` → EFAULT before `clock_id` rejection
    // (EINVAL) and before reading the clock (issue-342). Same `clock_id` vs user-buffer ordering
    // theme as `sys_clock_nanosleep` (issue-332: `clock_id` before `read_timespec_user(req)` there
    // is the inverse shape — gettime outputs `tp`, so validate output pointer first).
    if ts.is_null() {
        return Err(AxError::BadAddress);
    }

    let cid = clock_id as u32;
    let now = match cid {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => wall_time(),
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_COARSE => clock_read_monotonic(),
        CLOCK_MONOTONIC_RAW => clock_read_monotonic_raw(),
        CLOCK_BOOTTIME => clock_read_boottime(),
        CLOCK_THREAD_CPUTIME_ID => {
            let (utime, stime) = current().as_thread().time.borrow().output();
            utime + stime
        }
        CLOCK_PROCESS_CPUTIME_ID => {
            let (ut_ns, st_ns) = current().as_thread().proc_data.thread_group_cpu_nanos();
            time_value_from_nanos(ut_ns.saturating_add(st_ns))
        }
        _ => {
            warn!("sys_clock_gettime: unsupported clock_id {clock_id}");
            return Err(AxError::InvalidInput);
        }
    };
    ts.vm_write(timespec::from_time_value(now))?;
    Ok(0)
}

/// Linux `gettimeofday(2)`: legacy `struct timezone *tz` is obsolete; when non-NULL the kernel
/// still writes zeros for glibc compatibility (issue-288).
pub fn sys_gettimeofday(ts: *mut timeval, tz: *mut timezone) -> AxResult<isize> {
    if let Some(ts) = ts.nullable() {
        ts.vm_write(timeval::from_time_value(wall_time()))?;
    }
    if let Some(tz) = tz.nullable() {
        tz.vm_write(timezone {
            tz_minuteswest: 0,
            tz_dsttime: 0,
        })?;
    }
    Ok(0)
}

pub fn sys_clock_getres(clock_id: __kernel_clockid_t, res: *mut timespec) -> AxResult<isize> {
    let cid = clock_id as u32;
    if !clock_id_supported(cid) {
        warn!("sys_clock_getres: unsupported clock_id {clock_id}");
        return Err(AxError::InvalidInput);
    }
    if let Some(res) = res.nullable() {
        res.vm_write(timespec::from_time_value(clock_get_resolution(cid)))?;
    }
    Ok(0)
}

/// Matches Linux `include/uapi/linux/times.h` `struct tms`: fields are `clock_t` (`__kernel_clock_t`
/// in uapi) jiffies at `USER_HZ` (not microseconds).
#[repr(C)]
pub struct Tms {
    /// user CPU time (jiffies)
    tms_utime: __kernel_clock_t,
    /// system CPU time (jiffies)
    tms_stime: __kernel_clock_t,
    /// user CPU time of waited children (jiffies)
    tms_cutime: __kernel_clock_t,
    /// system CPU time of waited children (jiffies)
    tms_cstime: __kernel_clock_t,
}

pub fn sys_times(tms: *mut Tms) -> AxResult<isize> {
    let proc_data = current().as_thread().proc_data.clone();
    let (ut_ns, st_ns) = proc_data.thread_group_cpu_nanos();
    let (cu_ns, cs_ns) = proc_data.waited_children_cpu_nanos();

    let utime = cpu_nanos_to_clock_t(ut_ns);
    let stime = cpu_nanos_to_clock_t(st_ns);
    let cutime = cpu_nanos_to_clock_t(cu_ns);
    let cstime = cpu_nanos_to_clock_t(cs_ns);

    if let Some(tms) = tms.nullable() {
        tms.vm_write(Tms {
            tms_utime: utime,
            tms_stime: stime,
            tms_cutime: cutime,
            tms_cstime: cstime,
        })?;
    }
    Ok(monotonic_nanos_to_jiffies(monotonic_time_nanos()) as _)
}

fn read_itimerval_user(p: *const itimerval) -> AxResult<itimerval> {
    unsafe {
        Ok(itimerval {
            it_interval: read_timeval_user(core::ptr::addr_of!((*p).it_interval))?,
            it_value: read_timeval_user(core::ptr::addr_of!((*p).it_value))?,
        })
    }
}

pub fn sys_getitimer(which: i32, value: *mut itimerval) -> AxResult<isize> {
    let ty = ITimerType::from_repr(which).ok_or(AxError::InvalidInput)?;
    // Linux `getitimer(2)`: `curr_value` must be writable; NULL → EFAULT.
    if value.is_null() {
        return Err(AxError::BadAddress);
    }
    let (it_interval, it_value) = current().as_thread().time.borrow().get_itimer(ty);

    value.vm_write(itimerval {
        it_interval: timeval::from_time_value(it_interval),
        it_value: timeval::from_time_value(it_value),
    })?;
    Ok(0)
}

pub fn sys_setitimer(
    which: i32,
    new_value: *const itimerval,
    old_value: *mut itimerval,
) -> AxResult<isize> {
    let ty = ITimerType::from_repr(which).ok_or(AxError::InvalidInput)?;
    let curr = current();

    let old = match new_value.nullable() {
        Some(new_value) => {
            let new_value = read_itimerval_user(new_value)?;
            let interval = new_value.it_interval.try_into_time_value()?.as_nanos() as usize;
            let remained = new_value.it_value.try_into_time_value()?.as_nanos() as usize;
            debug!("sys_setitimer <= type: {ty:?}, interval: {interval:?}, remained: {remained:?}");
            curr
                .as_thread()
                .time
                .borrow_mut()
                .set_itimer(ty, interval, remained)
        }
        None => {
            // Linux `setitimer(2)` (man-pages VERSIONS): `new_value == NULL` disarms the timer,
            // equivalent to zero `it_interval`/`it_value` — not a no-op (issue-279).
            debug!("sys_setitimer <= type: {ty:?}, new_value: NULL (disarm)");
            curr
                .as_thread()
                .time
                .borrow_mut()
                .set_itimer(ty, 0, 0)
        }
    };

    if let Some(old_value) = old_value.nullable() {
        old_value.vm_write(itimerval {
            it_interval: timeval::from_time_value(old.0),
            it_value: timeval::from_time_value(old.1),
        })?;
    }
    Ok(0)
}
