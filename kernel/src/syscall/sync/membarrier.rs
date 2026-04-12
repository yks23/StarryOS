//! `membarrier(2)` — Linux command values are **single-bit masks** (see `uapi/linux/membarrier.h`).
//!
//! Non-`QUERY` commands that actually order memory use a **CPU** [`atomic::fence`] with
//! [`Ordering::SeqCst`], not `compiler_fence`, so the hardware enforces ordering on this hart.
//!
//! **SMP:** A full `MEMBARRIER_CMD_GLOBAL` on Linux runs barriers on **all** CPUs via IPI; this
//! kernel does not implement cross-CPU synchronization yet. The local fence is the best
//! available approximation until an IPI-based global barrier exists.

use core::sync::atomic::{self, Ordering};

use axerrno::{AxError, AxResult};

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

    match cmd {
        MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED
        | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE => Ok(0),

        MEMBARRIER_CMD_GLOBAL
        | MEMBARRIER_CMD_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
            membarrier_cpu_local_fence();
            Ok(0)
        }

        MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE => {
            membarrier_sync_core();
            Ok(0)
        }

        _ => Err(AxError::InvalidInput),
    }
}
