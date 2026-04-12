use axerrno::{AxError, AxResult};
use axhal::time::{TimeValue, monotonic_time, monotonic_time_nanos, nanos_to_ticks, wall_time};
use axtask::current;
use linux_raw_sys::general::{
    __kernel_clockid_t, CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_MONOTONIC_COARSE,
    CLOCK_MONOTONIC_RAW, CLOCK_PROCESS_CPUTIME_ID, CLOCK_REALTIME, CLOCK_REALTIME_COARSE,
    CLOCK_THREAD_CPUTIME_ID, itimerval, timespec, timeval,
};
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    task::{AsThread, ITimerType, time_value_from_nanos},
    time::TimeValueLike,
};

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
    let cid = clock_id as u32;
    let now = match cid {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => wall_time(),
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_MONOTONIC_COARSE | CLOCK_BOOTTIME => {
            monotonic_time()
        }
        CLOCK_PROCESS_CPUTIME_ID | CLOCK_THREAD_CPUTIME_ID => {
            let (utime, stime) = current().as_thread().time.borrow().output();
            utime + stime
        }
        _ => {
            warn!("sys_clock_gettime: unsupported clock_id {clock_id}");
            return Err(AxError::InvalidInput);
        }
    };
    ts.vm_write(timespec::from_time_value(now))?;
    Ok(0)
}

pub fn sys_gettimeofday(ts: *mut timeval) -> AxResult<isize> {
    if let Some(ts) = ts.nullable() {
        ts.vm_write(timeval::from_time_value(wall_time()))?;
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

#[repr(C)]
pub struct Tms {
    /// user time
    tms_utime: usize,
    /// system time
    tms_stime: usize,
    /// user time of children
    tms_cutime: usize,
    /// system time of children
    tms_cstime: usize,
}

pub fn sys_times(tms: *mut Tms) -> AxResult<isize> {
    let proc_data = current().as_thread().proc_data.clone();
    let (ut_ns, st_ns) = proc_data.thread_group_cpu_nanos();
    let (cu_ns, cs_ns) = proc_data.waited_children_cpu_nanos();

    let utime = time_value_from_nanos(ut_ns).as_micros() as usize;
    let stime = time_value_from_nanos(st_ns).as_micros() as usize;
    let cutime = time_value_from_nanos(cu_ns).as_micros() as usize;
    let cstime = time_value_from_nanos(cs_ns).as_micros() as usize;

    if let Some(tms) = tms.nullable() {
        tms.vm_write(Tms {
            tms_utime: utime,
            tms_stime: stime,
            tms_cutime: cutime,
            tms_cstime: cstime,
        })?;
    }
    Ok(nanos_to_ticks(monotonic_time_nanos()) as _)
}

pub fn sys_getitimer(which: i32, value: *mut itimerval) -> AxResult<isize> {
    let ty = ITimerType::from_repr(which).ok_or(AxError::InvalidInput)?;
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
            // FIXME: AnyBitPattern
            let new_value = unsafe { new_value.vm_read_uninit()?.assume_init() };
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
            if old_value.nullable().is_none() {
                return Ok(0);
            }
            debug!("sys_setitimer <= type: {ty:?}, new_value: NULL (no change)");
            curr.as_thread().time.borrow().get_itimer(ty)
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
