use axerrno::AxResult;

use crate::task::do_exit;

/// Linux `SYSCALL_DEFINE1(exit, ...)` passes `(error_code & 0xff) << 8` to `do_exit`.
fn kernel_exit_status(exit_code: i32) -> i32 {
    (((exit_code as u32) & 0xff) as i32) << 8
}

pub fn sys_exit(exit_code: i32) -> AxResult<isize> {
    do_exit(kernel_exit_status(exit_code), false);
    Ok(0)
}

pub fn sys_exit_group(exit_code: i32) -> AxResult<isize> {
    do_exit(kernel_exit_status(exit_code), true);
    Ok(0)
}
