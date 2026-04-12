use alloc::vec;
use core::ffi::c_char;

use axconfig::ARCH;
use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;
use axtask::current;
use bytemuck::cast_slice;
use linux_raw_sys::{
    general::{GRND_INSECURE, GRND_NONBLOCK, GRND_RANDOM},
    system::{new_utsname, sysinfo},
};
use starry_vm::{VmMutPtr, vm_write_slice};

use crate::{
    mm::UserConstPtr,
    task::{AsThread, processes},
};

pub fn sys_getuid() -> AxResult<isize> {
    Ok(current().as_thread().proc_data.getuid() as isize)
}

pub fn sys_geteuid() -> AxResult<isize> {
    Ok(current().as_thread().proc_data.geteuid() as isize)
}

pub fn sys_getgid() -> AxResult<isize> {
    Ok(current().as_thread().proc_data.getgid() as isize)
}

pub fn sys_getegid() -> AxResult<isize> {
    Ok(current().as_thread().proc_data.getegid() as isize)
}

pub fn sys_setuid(uid: u32) -> AxResult<isize> {
    debug!("sys_setuid <= uid: {uid}");
    current().as_thread().proc_data.setresuid(uid, uid, uid)?;
    Ok(0)
}

pub fn sys_setgid(gid: u32) -> AxResult<isize> {
    debug!("sys_setgid <= gid: {gid}");
    current().as_thread().proc_data.setresgid(gid, gid, gid)?;
    Ok(0)
}

pub fn sys_getgroups(size: isize, list: *mut u32) -> AxResult<isize> {
    debug!("sys_getgroups <= size: {size}");
    if size < 0 {
        return Err(AxError::InvalidInput);
    }
    let groups = current().as_thread().proc_data.get_supplementary_groups();
    let ngroups = groups.len();

    let sz = size as usize;
    if sz == 0 {
        return Ok(ngroups as isize);
    }
    if sz < ngroups {
        return Err(AxError::InvalidInput);
    }
    if list.is_null() {
        return Err(AxError::BadAddress);
    }
    if ngroups > 0 {
        vm_write_slice(list as *mut u8, cast_slice(groups.as_slice()))?;
    }
    Ok(ngroups as isize)
}

pub fn sys_setgroups(size: isize, list: *const u32) -> AxResult<isize> {
    debug!("sys_setgroups <= size: {size}");
    if size < 0 {
        return Err(AxError::InvalidInput);
    }
    let sz = size as usize;
    if sz > crate::task::SUPP_GROUPS_MAX {
        return Err(AxError::InvalidInput);
    }
    if sz == 0 {
        current()
            .as_thread()
            .proc_data
            .set_supplementary_groups(&[])?;
        return Ok(0);
    }
    if list.is_null() {
        return Err(AxError::BadAddress);
    }
    let slice = UserConstPtr::from(list).get_as_slice(sz)?;
    current()
        .as_thread()
        .proc_data
        .set_supplementary_groups(slice)?;
    Ok(0)
}

const fn pad_str(info: &str) -> [c_char; 65] {
    let mut data: [c_char; 65] = [0; 65];
    // this needs #![feature(const_copy_from_slice)]
    // data[..info.len()].copy_from_slice(info.as_bytes());
    unsafe {
        core::ptr::copy_nonoverlapping(info.as_ptr().cast(), data.as_mut_ptr(), info.len());
    }
    data
}

const UTSNAME: new_utsname = new_utsname {
    sysname: pad_str("Linux"),
    nodename: pad_str("starry"),
    release: pad_str("10.0.0"),
    version: pad_str("10.0.0"),
    machine: pad_str(ARCH),
    domainname: pad_str("https://github.com/Starry-OS/StarryOS"),
};

pub fn sys_uname(name: *mut new_utsname) -> AxResult<isize> {
    name.vm_write(UTSNAME)?;
    Ok(0)
}

pub fn sys_sysinfo(info: *mut sysinfo) -> AxResult<isize> {
    // FIXME: Zeroable
    let mut kinfo: sysinfo = unsafe { core::mem::zeroed() };
    kinfo.procs = processes().len() as _;
    kinfo.mem_unit = 1;
    info.vm_write(kinfo)?;
    Ok(0)
}

pub fn sys_syslog(_type: i32, _buf: *mut c_char, _len: usize) -> AxResult<isize> {
    Ok(0)
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct GetRandomFlags: u32 {
        const NONBLOCK = GRND_NONBLOCK;
        const RANDOM = GRND_RANDOM;
        const INSECURE = GRND_INSECURE;
    }
}

pub fn sys_getrandom(buf: *mut u8, len: usize, flags: u32) -> AxResult<isize> {
    if len == 0 {
        return Ok(0);
    }
    let flags = GetRandomFlags::from_bits_retain(flags);

    debug!("sys_getrandom <= buf: {buf:p}, len: {len}, flags: {flags:?}");

    let path = if flags.contains(GetRandomFlags::RANDOM) {
        "/dev/random"
    } else {
        "/dev/urandom"
    };

    let f = FS_CONTEXT.lock().resolve(path)?;
    let mut kbuf = vec![0; len];
    let len = f.entry().as_file()?.read_at(&mut kbuf, 0)?;

    vm_write_slice(buf, &kbuf)?;

    Ok(len as _)
}

pub fn sys_seccomp(_op: u32, _flags: u32, _args: *const ()) -> AxResult<isize> {
    Err(AxError::Unsupported)
}

#[cfg(target_arch = "riscv64")]
pub fn sys_riscv_flush_icache() -> AxResult<isize> {
    riscv::asm::fence_i();
    Ok(0)
}
