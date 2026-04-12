use alloc::sync::Arc;

use axerrno::{AxError, AxResult};
use axtask::current;
use linux_raw_sys::general::{__user_cap_data_struct, __user_cap_header_struct};
use starry_vm::{VmMutPtr, VmPtr, vm_write_slice};

use crate::{
    mm::UserConstPtr,
    task::{AsThread, CRED_NO_CHANGE, ProcessData, get_process_data},
};

const CAPABILITY_VERSION_3: u32 = 0x20080522;

fn read_cap_header(header_ptr: *mut __user_cap_header_struct) -> AxResult<__user_cap_header_struct> {
    // FIXME: AnyBitPattern
    let mut header = unsafe { header_ptr.vm_read_uninit()?.assume_init() };
    if header.version != CAPABILITY_VERSION_3 {
        header.version = CAPABILITY_VERSION_3;
        header_ptr.vm_write(header)?;
        return Err(AxError::InvalidInput);
    }
    Ok(header)
}

fn resolve_cap_target(header: &__user_cap_header_struct) -> AxResult<Arc<ProcessData>> {
    let pid = if header.pid == 0 {
        0u32
    } else if header.pid > 0 {
        header.pid as u32
    } else {
        return Err(AxError::InvalidInput);
    };
    get_process_data(pid)
}

fn ensure_same_process_for_cap(target: &Arc<ProcessData>) -> AxResult<()> {
    let curr = current().as_thread().proc_data.clone();
    if !Arc::ptr_eq(target, &curr) {
        return Err(AxError::PermissionDenied);
    }
    Ok(())
}

pub fn sys_capget(
    header: *mut __user_cap_header_struct,
    data: *mut __user_cap_data_struct,
) -> AxResult<isize> {
    let header = read_cap_header(header)?;
    let target = resolve_cap_target(&header)?;
    ensure_same_process_for_cap(&target)?;
    let (e, p, i) = target.get_capabilities();
    data.vm_write(__user_cap_data_struct {
        effective: e,
        permitted: p,
        inheritable: i,
    })?;
    Ok(0)
}

pub fn sys_capset(
    header: *mut __user_cap_header_struct,
    data: *mut __user_cap_data_struct,
) -> AxResult<isize> {
    let header = read_cap_header(header)?;
    let target = resolve_cap_target(&header)?;
    ensure_same_process_for_cap(&target)?;
    let cap_data = unsafe { data.vm_read_uninit()?.assume_init() };
    target.set_capabilities(cap_data.effective, cap_data.permitted, cap_data.inheritable)?;
    Ok(0)
}

pub fn sys_umask(mask: u32) -> AxResult<isize> {
    let curr = current();
    let old = curr.as_thread().proc_data.replace_umask(mask);
    Ok(old as isize)
}

pub fn sys_setreuid(ruid: u32, euid: u32) -> AxResult<isize> {
    current()
        .as_thread()
        .proc_data
        .setresuid(ruid, euid, CRED_NO_CHANGE)?;
    Ok(0)
}

pub fn sys_setresuid(ruid: u32, euid: u32, suid: u32) -> AxResult<isize> {
    current()
        .as_thread()
        .proc_data
        .setresuid(ruid, euid, suid)?;
    Ok(0)
}

pub fn sys_setresgid(rgid: u32, egid: u32, sgid: u32) -> AxResult<isize> {
    current()
        .as_thread()
        .proc_data
        .setresgid(rgid, egid, sgid)?;
    Ok(0)
}

pub fn sys_get_mempolicy(
    policy: *mut i32,
    nodemask: *mut usize,
    maxnode: usize,
    addr: usize,
    flags: usize,
) -> AxResult<isize> {
    debug!(
        "sys_get_mempolicy <= policy {policy:p}, nodemask {nodemask:p}, maxnode {maxnode}, addr {addr:#x}, flags {flags:#x}"
    );

    // Linux EINVAL: non-NULL nodemask requires a positive maxnode (valid bit length).
    if !nodemask.is_null() && maxnode == 0 {
        return Err(AxError::InvalidInput);
    }

    // No per-node NUMA policy in this kernel: report default policy (Linux `MPOL_DEFAULT` == 0).
    const MPOL_DEFAULT: i32 = 0;
    if let Some(p) = policy.nullable() {
        p.vm_write(MPOL_DEFAULT)?;
    }

    // Zero nodemask bits when a buffer is provided (`maxnode` is the number of bits).
    if !nodemask.is_null() && maxnode > 0 {
        let nbytes = maxnode.div_ceil(8).min(8192);
        vm_write_slice(nodemask as *mut u8, &alloc::vec![0u8; nbytes])?;
    }

    Ok(0)
}

/// prctl() is called with a first argument describing what to do, and further
/// arguments with a significance depending on the first one.
/// The first argument can be:
/// - PR_SET_NAME: set the name of the calling thread, using the value pointed to by `arg2`
/// - PR_GET_NAME: get the name of the calling
/// - PR_SET_SECCOMP: enable seccomp mode, with the mode specified in `arg2`
/// - PR_MCE_KILL: set the machine check exception policy
/// - PR_SET_MM options: set various memory management options (start/end code/data/brk/stack)
pub fn sys_prctl(
    option: u32,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> AxResult<isize> {
    use linux_raw_sys::prctl::*;

    debug!("sys_prctl <= option: {option}, args: {arg2}, {arg3}, {arg4}, {arg5}");

    match option {
        PR_SET_NAME => {
            // Linux: up to 16 bytes including terminating NUL (≤15 printable + '\0'); no unbounded scan.
            const PR_SET_NAME_MAX: usize = 16;
            let bytes = UserConstPtr::from(arg2 as *const u8).get_as_slice(PR_SET_NAME_MAX)?;
            let Some(nul_pos) = bytes.iter().position(|&b| b == 0) else {
                return Err(AxError::InvalidInput);
            };
            let name_bytes = &bytes[..nul_pos];
            let s = core::str::from_utf8(name_bytes).map_err(|_| AxError::InvalidInput)?;
            current().set_name(s);
        }
        PR_GET_NAME => {
            let name = current().name();
            let len = name.len().min(15);
            let mut buf = [0; 16];
            buf[..len].copy_from_slice(&name.as_bytes()[..len]);
            vm_write_slice(arg2 as _, &buf)?;
        }
        PR_SET_SECCOMP => {
            // Linux `SECCOMP_MODE_*`: 0 = disabled, 1 = strict, 2 = filter.
            const SECCOMP_MODE_FILTER: usize = 2;
            if arg2 > SECCOMP_MODE_FILTER {
                return Err(AxError::InvalidInput);
            }
            return Err(AxError::Unsupported);
        }
        PR_MCE_KILL => {
            use linux_raw_sys::prctl::{
                PR_MCE_KILL_CLEAR, PR_MCE_KILL_DEFAULT, PR_MCE_KILL_SET,
            };
            match arg2 {
                x if x == PR_MCE_KILL_CLEAR as usize => return Err(AxError::Unsupported),
                x if x == PR_MCE_KILL_SET as usize => {
                    if arg3 > PR_MCE_KILL_DEFAULT as usize {
                        return Err(AxError::InvalidInput);
                    }
                    return Err(AxError::Unsupported);
                }
                _ => return Err(AxError::InvalidInput),
            }
        }
        PR_SET_MM => {
            // not implemented; but avoid annoying warnings
            return Err(AxError::InvalidInput);
        }
        _ => {
            warn!("sys_prctl: unsupported option {option}");
            return Err(AxError::InvalidInput);
        }
    }

    Ok(0)
}
