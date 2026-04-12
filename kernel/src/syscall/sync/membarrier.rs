//! `membarrier(2)` — Linux command values are **single-bit masks** (see `uapi/linux/membarrier.h`).
//!
//! Non-`QUERY` commands that actually order memory use a **CPU** [`atomic::fence`] with
//! [`Ordering::SeqCst`], not `compiler_fence`, so the hardware enforces ordering on this hart.
//!
//! **SMP:** A full `MEMBARRIER_CMD_GLOBAL` on Linux runs barriers on **all** CPUs via IPI; this
//! kernel does not implement cross-CPU synchronization yet. The local fence is the best
//! available approximation until an IPI-based global barrier exists.
//!
//! **Registration:** `*_EXPEDITED` / `*_SYNC_CORE` execution commands require a prior matching
//! `REGISTER_*` on the calling process (**`EINVAL`** otherwise), matching Linux `membarrier.c`.

use core::sync::atomic::{self, Ordering};

use axerrno::{AxError, AxResult};
use axtask::current;

use crate::task::AsThread;

/// `MEMBARRIER_CMD_QUERY`
const MEMBARRIER_CMD_QUERY: i32 = 0;
/// `MEMBARRIER_CMD_GLOBAL` — `(1 << 0)`
const MEMBARRIER_CMD_GLOBAL: i32 = 1 << 0;
/// `MEMBARRIER_CMD_GLOBAL_EXPEDITED` — `(1 << 1)`
const MEMBARRIER_CMD_GLOBAL_EXPEDITED: i32 = 1 << 1;
/// `MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED` — `(1 << 2)`
const MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED: i32 = 1 << 2;
/// `MEMBARRIER_CMD_PRIVATE_EXPEDITED` — `(1 << 3)`
const MEMBARRIER_CMD_PRIVATE_EXPEDITED: i32 = 1 << 3;
/// `MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED` — `(1 << 4)`
const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: i32 = 1 << 4;
/// `MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE` — `(1 << 5)`
const MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE: i32 = 1 << 5;
/// `MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE` — `(1 << 6)`
const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE: i32 = 1 << 6;

/// Bitmask returned by `MEMBARRIER_CMD_QUERY` for commands implemented here.
const SUPPORTED_COMMANDS: i32 = MEMBARRIER_CMD_GLOBAL
    | MEMBARRIER_CMD_GLOBAL_EXPEDITED
    | MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED
    | MEMBARRIER_CMD_PRIVATE_EXPEDITED
    | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED
    | MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE
    | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE;

#[inline]
fn membarrier_cpu_local_fence() {
    atomic::fence(Ordering::SeqCst);
}

/// Extra core serialization for `PRIVATE_EXPEDITED_SYNC_CORE` (best-effort per architecture).
#[inline]
fn membarrier_sync_core() {
    membarrier_cpu_local_fence();
    #[cfg(target_arch = "riscv64")]
    unsafe {
        // Instruction-fetch barrier: flushes pipeline visibility for this hart (see Linux rseq/membarrier notes).
        core::arch::asm!("fence.i", options(nostack, preserves_flags));
    }
}

#[inline]
fn is_single_command_bit(cmd: i32) -> bool {
    let u = cmd as u32;
    u != 0 && (u & u.wrapping_sub(1)) == 0
}

pub fn sys_membarrier(cmd: i32, flags: u32, _cpu_id: i32) -> AxResult<isize> {
    if flags != 0 {
        return Err(AxError::InvalidInput);
    }

    if cmd == MEMBARRIER_CMD_QUERY {
        return Ok(SUPPORTED_COMMANDS as isize);
    }

    if !is_single_command_bit(cmd) {
        return Err(AxError::InvalidInput);
    }

    let task = current();
    let pd = &*task.as_thread().proc_data;

    match cmd {
        MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED => {
            pd.membarrier_reg_global_expedited
                .store(true, Ordering::Relaxed);
            Ok(0)
        }
        MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED => {
            pd.membarrier_reg_private_expedited
                .store(true, Ordering::Relaxed);
            Ok(0)
        }
        MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE => {
            pd.membarrier_reg_private_expedited_sync_core
                .store(true, Ordering::Relaxed);
            Ok(0)
        }

        MEMBARRIER_CMD_GLOBAL => {
            membarrier_cpu_local_fence();
            Ok(0)
        }

        MEMBARRIER_CMD_GLOBAL_EXPEDITED => {
            if !pd
                .membarrier_reg_global_expedited
                .load(Ordering::Relaxed)
            {
                return Err(AxError::InvalidInput);
            }
            membarrier_cpu_local_fence();
            Ok(0)
        }

        MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
            if !pd
                .membarrier_reg_private_expedited
                .load(Ordering::Relaxed)
            {
                return Err(AxError::InvalidInput);
            }
            membarrier_cpu_local_fence();
            Ok(0)
        }

        MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE => {
            if !pd
                .membarrier_reg_private_expedited_sync_core
                .load(Ordering::Relaxed)
            {
                return Err(AxError::InvalidInput);
            }
            membarrier_sync_core();
            Ok(0)
        }

        _ => Err(AxError::InvalidInput),
    }
}
