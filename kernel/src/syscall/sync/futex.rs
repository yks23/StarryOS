use core::sync::atomic::Ordering;

use axerrno::{AxError, AxResult, LinuxError};
use axtask::current;
use linux_raw_sys::general::{
    FUTEX_CMD_MASK, FUTEX_CMP_REQUEUE, FUTEX_REQUEUE, FUTEX_WAIT, FUTEX_WAIT_BITSET, FUTEX_WAKE,
    FUTEX_WAKE_BITSET, robust_list_head, timespec,
};
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    task::{AsThread, FutexKey, futex_table_for, get_task},
    time::{TimeValueLike, read_timespec_user},
};

fn assert_unsigned(value: u32) -> AxResult<u32> {
    if (value as i32) < 0 {
        Err(AxError::InvalidInput)
    } else {
        Ok(value)
    }
}

pub fn sys_futex(
    uaddr: *const u32,
    futex_op: u32,
    value: u32,
    timeout: *const timespec,
    uaddr2: *mut u32,
    value3: u32,
) -> AxResult<isize> {
    debug!(
        "sys_futex <= uaddr: {uaddr:?}, futex_op: {futex_op}, value: {value}, uaddr2: {uaddr2:?}, \
         value3: {value3}",
    );

    let key = FutexKey::new_current(uaddr.addr());

    let futex_table = futex_table_for(&key);

    let command = futex_op & (FUTEX_CMD_MASK as u32);
    match command {
        FUTEX_WAIT | FUTEX_WAIT_BITSET => {
            // Fast path
            if uaddr.vm_read()? != value {
                return Err(AxError::WouldBlock);
            }

            let timeout = if let Some(ts) = timeout.nullable() {
                let ts = read_timespec_user(ts)?.try_into_time_value()?;
                Some(ts)
            } else {
                None
            };

            let futex = futex_table.get_or_insert(&key);

            let bitset = if command == FUTEX_WAIT_BITSET {
                value3
            } else {
                u32::MAX
            };

            if !futex
                .wq
                .wait_if(bitset, timeout, || uaddr.vm_read() == Ok(value))?
            {
                return Err(AxError::WouldBlock);
            }

            if futex.owner_dead.swap(false, Ordering::SeqCst) {
                Err(AxError::from(LinuxError::EOWNERDEAD))
            } else {
                Ok(0)
            }
        }
        FUTEX_WAKE | FUTEX_WAKE_BITSET => {
            let futex = futex_table.get(&key);
            let mut count = 0;
            if let Some(futex) = futex {
                let bitset = if command == FUTEX_WAKE_BITSET {
                    value3
                } else {
                    u32::MAX
                };
                count = futex.wq.wake(value as _, bitset);
            }
            axtask::yield_now();
            Ok(count as _)
        }
        FUTEX_REQUEUE | FUTEX_CMP_REQUEUE => {
            assert_unsigned(value)?;
            if command == FUTEX_CMP_REQUEUE && uaddr.vm_read()? != value3 {
                return Err(AxError::WouldBlock);
            }
            // Linux ABI: `timeout` argument slot carries `nr_requeue` (`val2`) as `u32`, not a timespec.
            let nr_requeue = assert_unsigned(timeout.addr() as u32)?;

            let futex = futex_table.get(&key);
            let key2 = FutexKey::new_current(uaddr2.addr());
            let table2 = futex_table_for(&key2);
            let futex2 = table2.get_or_insert(&key2);

            if let Some(futex) = futex {
                // Match kernel/futex/requeue.c: wake up to `nr_wake` waiters, then requeue up to
                // `nr_requeue` of the *remaining* blocked waiters—unconditional on whether `nr_wake`
                // was fully consumed.
                let woke = futex.wq.wake(value as _, u32::MAX);
                let requeued = futex.wq.requeue(nr_requeue as _, &futex2.wq);
                Ok((woke + requeued) as _)
            } else {
                Ok(0)
            }
        }
        _ => Err(AxError::Unsupported),
    }
}

pub fn sys_get_robust_list(
    tid: u32,
    head: *mut *const robust_list_head,
    size: *mut usize,
) -> AxResult<isize> {
    let task = get_task(tid)?;
    head.vm_write(task.as_thread().robust_list_head() as _)?;
    size.vm_write(size_of::<robust_list_head>())?;

    Ok(0)
}

pub fn sys_set_robust_list(head: *const robust_list_head, size: usize) -> AxResult<isize> {
    if size != size_of::<robust_list_head>() {
        return Err(AxError::InvalidInput);
    }
    current().as_thread().set_robust_list_head(head.addr());

    Ok(0)
}
