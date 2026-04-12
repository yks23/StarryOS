use core::sync::atomic::{self, Ordering};

use axerrno::{AxError, AxResult};

/// Memory barrier commands
const MEMBARRIER_CMD_QUERY: i32 = 0;
const MEMBARRIER_CMD_GLOBAL: i32 = 1;
const MEMBARRIER_CMD_GLOBAL_EXPEDITED: i32 = 2;
const MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED: i32 = 3;
const MEMBARRIER_CMD_PRIVATE_EXPEDITED: i32 = 4;
const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: i32 = 5;

/// Supported command flags for query
const SUPPORTED_COMMANDS: i32 = (1 << MEMBARRIER_CMD_GLOBAL)
    | (1 << MEMBARRIER_CMD_GLOBAL_EXPEDITED)
    | (1 << MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED)
    | (1 << MEMBARRIER_CMD_PRIVATE_EXPEDITED)
    | (1 << MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED);

#[inline]
fn membarrier_cpu_fence() {
    atomic::fence(Ordering::SeqCst);
}

pub fn sys_membarrier(cmd: i32, flags: u32, _cpu_id: i32) -> AxResult<isize> {
    if flags != 0 {
        return Err(AxError::InvalidInput);
    }

    match cmd {
        MEMBARRIER_CMD_QUERY => Ok(SUPPORTED_COMMANDS as isize),
        MEMBARRIER_CMD_GLOBAL
        | MEMBARRIER_CMD_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_PRIVATE_EXPEDITED
        | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED => {
            membarrier_cpu_fence();
            Ok(0)
        }
        _ => Err(AxError::InvalidInput),
    }
}
