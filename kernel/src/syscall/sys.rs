use alloc::vec;
use core::{
    ffi::c_char,
    sync::atomic::{AtomicBool, Ordering},
};

use axalloc::global_allocator;
use axconfig::ARCH;
use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;
use axhal::mem::{PAGE_SIZE_4K, total_ram_size};
use axhal::time::{NANOS_PER_SEC, monotonic_time_nanos};
use axtask::current;
use bytemuck::cast_slice;
use linux_raw_sys::{
    general::{GRND_INSECURE, GRND_NONBLOCK, GRND_RANDOM},
    system::{new_utsname, sysinfo},
};
use starry_vm::{VmMutPtr, vm_write_slice};

use crate::{
    mm::{UserConstPtr, UserPtr},
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

pub fn sys_getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> AxResult<isize> {
    // Linux: each pointer may be NULL to skip that field.
    let (r, e, s) = current().as_thread().proc_data.get_resuid();
    if !ruid.is_null() {
        *UserPtr::from(ruid).get_as_mut()? = r;
    }
    if !euid.is_null() {
        *UserPtr::from(euid).get_as_mut()? = e;
    }
    if !suid.is_null() {
        *UserPtr::from(suid).get_as_mut()? = s;
    }
    Ok(0)
}

pub fn sys_getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> AxResult<isize> {
    // Linux: each pointer may be NULL to skip that field.
    let (r, e, s) = current().as_thread().proc_data.get_resgid();
    if !rgid.is_null() {
        *UserPtr::from(rgid).get_as_mut()? = r;
    }
    if !egid.is_null() {
        *UserPtr::from(egid).get_as_mut()? = e;
    }
    if !sgid.is_null() {
        *UserPtr::from(sgid).get_as_mut()? = s;
    }
    Ok(0)
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
    let total = total_ram_size();
    let free_pool = global_allocator()
        .available_pages()
        .saturating_mul(PAGE_SIZE_4K);
    let freeram = free_pool.min(total);

    let mut kinfo: sysinfo = unsafe { core::mem::zeroed() };
    kinfo.uptime = (monotonic_time_nanos() / NANOS_PER_SEC) as _;
    kinfo.loads = [0, 0, 0];
    kinfo.totalram = total as _;
    kinfo.freeram = freeram as _;
    kinfo.sharedram = 0;
    kinfo.bufferram = 0;
    kinfo.totalswap = 0;
    kinfo.freeswap = 0;
    kinfo.procs = processes().len() as _;
    kinfo.pad = 0;
    kinfo.totalhigh = 0;
    kinfo.freehigh = 0;
    kinfo.mem_unit = 1;
    info.vm_write(kinfo)?;
    Ok(0)
}

/// Linux `SYSLOG_ACTION_*` (`uapi/linux/sys/syslog.h`). Starry has no printk ring buffer;
/// read/clear/console actions return **`Unsupported`**; size queries return **0**.
const SYSLOG_ACTION_CLOSE: i32 = 0;
const SYSLOG_ACTION_OPEN: i32 = 1;
const SYSLOG_ACTION_READ: i32 = 2;
const SYSLOG_ACTION_READ_ALL: i32 = 3;
const SYSLOG_ACTION_READ_CLEAR: i32 = 4;
const SYSLOG_ACTION_CLEAR: i32 = 5;
const SYSLOG_ACTION_CONSOLE_OFF: i32 = 6;
const SYSLOG_ACTION_CONSOLE_ON: i32 = 7;
const SYSLOG_ACTION_CONSOLE_LEVEL: i32 = 8;
const SYSLOG_ACTION_SIZE_UNREAD: i32 = 9;
const SYSLOG_ACTION_SIZE_BUFFER: i32 = 10;

pub fn sys_syslog(action: i32, _buf: *mut c_char, _len: usize) -> AxResult<isize> {
    if !(SYSLOG_ACTION_CLOSE..=SYSLOG_ACTION_SIZE_BUFFER).contains(&action) {
        return Err(AxError::InvalidInput);
    }

    match action {
        SYSLOG_ACTION_READ | SYSLOG_ACTION_READ_ALL | SYSLOG_ACTION_READ_CLEAR => {
            Err(AxError::Unsupported)
        }
        SYSLOG_ACTION_CLEAR
        | SYSLOG_ACTION_CONSOLE_OFF
        | SYSLOG_ACTION_CONSOLE_ON
        | SYSLOG_ACTION_CONSOLE_LEVEL => Err(AxError::Unsupported),
        SYSLOG_ACTION_SIZE_UNREAD | SYSLOG_ACTION_SIZE_BUFFER => Ok(0),
        SYSLOG_ACTION_OPEN | SYSLOG_ACTION_CLOSE => Ok(0),
        // `action` already restricted to `CLOSE..=SIZE_BUFFER`.
        _ => unreachable!(),
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct GetRandomFlags: u32 {
        const NONBLOCK = GRND_NONBLOCK;
        const RANDOM = GRND_RANDOM;
        const INSECURE = GRND_INSECURE;
    }
}

/// Linux `getrandom(2)` only allows `GRND_*` bits defined in `uapi/linux/random.h`.
const GRND_FLAGS_MASK: u32 = GRND_NONBLOCK | GRND_RANDOM | GRND_INSECURE;

/// Linux-style CRNG readiness for **`GRND_RANDOM` | `GRND_NONBLOCK`**: return **`EAGAIN`** until at
/// least one prior **`getrandom`** completed successfully (Starry has no true blocking entropy pool).
static CRNG_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn sys_getrandom(buf: *mut u8, len: usize, flags: u32) -> AxResult<isize> {
    // Linux getrandom: validate GRND_* before count==0 short-circuit (issue-151).
    if flags & !GRND_FLAGS_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    if len == 0 {
        return Ok(0);
    }
    let flags = GetRandomFlags::from_bits_truncate(flags);

    debug!("sys_getrandom <= buf: {buf:p}, len: {len}, flags: {flags:?}");

    // `GRND_INSECURE`: allow reads before CRNG init with weaker semantics → match **`/dev/urandom`**
    // (man 2 getrandom / issue-179).
    let use_urandom =
        !flags.contains(GetRandomFlags::RANDOM) || flags.contains(GetRandomFlags::INSECURE);

    // `GRND_RANDOM` + `GRND_NONBLOCK`: Linux returns **`EAGAIN`** while the blocking pool would wait.
    if !use_urandom
        && flags.contains(GetRandomFlags::NONBLOCK)
        && !CRNG_INITIALIZED.load(Ordering::Relaxed)
    {
        return Err(AxError::WouldBlock);
    }

    let path = if use_urandom {
        "/dev/urandom"
    } else {
        "/dev/random"
    };

    let f = FS_CONTEXT.lock().resolve(path)?;
    let mut kbuf = vec![0; len];
    let len = f.entry().as_file()?.read_at(&mut kbuf, 0)?;

    vm_write_slice(buf, &kbuf)?;

    CRNG_INITIALIZED.store(true, Ordering::Relaxed);

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
