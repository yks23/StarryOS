use alloc::vec;
use core::{
    ffi::c_char,
    mem::size_of,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use axalloc::global_allocator;
use axconfig::ARCH;
use axerrno::{AxError, AxResult};
use axfs::FS_CONTEXT;
use axhal::mem::{PAGE_SIZE_4K, total_ram_size};
use axhal::time::{NANOS_PER_SEC, monotonic_time_nanos};
use axtask::{TaskState, current};
use bytemuck::cast_slice;
use linux_raw_sys::{
    general::{GRND_INSECURE, GRND_NONBLOCK, GRND_RANDOM},
    system::{SI_LOAD_SHIFT, new_utsname, sysinfo},
};
use starry_vm::{VmMutPtr, vm_write_slice};

use crate::{
    mm::{UserConstPtr, UserPtr, check_access},
    task::{AsThread, processes, tasks},
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
    // Linux getgroups: with gidsetsize > 0, NULL grouplist → EFAULT before EINVAL for too-small
    // buffer (issue-329; same theme as issue-304/issue-306).
    if list.is_null() {
        return Err(AxError::BadAddress);
    }
    if sz < ngroups {
        return Err(AxError::InvalidInput);
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
    // NIS/YP domain — `(none)` matches typical Linux when unset; do not use an HTTP URL here
    // (misread as DNS/NIS or build metadata; issue-312).
    domainname: pad_str("(none)"),
};

pub fn sys_uname(name: *mut new_utsname) -> AxResult<isize> {
    // Linux `uname(2)`: `buf` must be writable; NULL → EFAULT.
    if name.is_null() {
        return Err(AxError::BadAddress);
    }
    name.vm_write(UTSNAME)?;
    Ok(0)
}

/// Linux `sysinfo.loads` fixed-point: `floor(load * (1 << SI_LOAD_SHIFT))`.
static SYSINFO_LOAD_LAST_NS: AtomicU64 = AtomicU64::new(0);
static SYSINFO_LOAD_EMA_1: AtomicU64 = AtomicU64::new(0);
static SYSINFO_LOAD_EMA_5: AtomicU64 = AtomicU64::new(0);
static SYSINFO_LOAD_EMA_15: AtomicU64 = AtomicU64::new(0);

/// UP：尚无 SMP 在线 CPU 导出时按 1 处理。
const SYSINFO_LOAD_NCPUS: u64 = 1;

const LOAD_TAU_1_NS: u64 = 60 * NANOS_PER_SEC;
const LOAD_TAU_5_NS: u64 = 5 * 60 * NANOS_PER_SEC;
const LOAD_TAU_15_NS: u64 = 15 * 60 * NANOS_PER_SEC;

fn count_runnable_tasks() -> usize {
    tasks()
        .iter()
        .filter(|t| matches!(t.state(), TaskState::Running | TaskState::Ready))
        .count()
}

fn ema_load_u64(old: u64, sample: u64, tau_ns: u64, delta_ns: u64) -> u64 {
    if delta_ns == 0 {
        return old;
    }
    let den = (tau_ns as u128).saturating_add(delta_ns as u128);
    let diff = sample as i128 - old as i128;
    let adj = (diff * delta_ns as i128 / den as i128) as i64;
    let v = (old as i128).saturating_add(adj as i128);
    v.clamp(0, u64::MAX as i128) as u64
}

/// issue-218: 近似 1/5/15 分钟平均负载（runqueue 上 Ready/Running 计数 + EWMA），与 Linux 定点格式一致。
fn sysinfo_load_avg_fixed() -> [u64; 3] {
    let scale = 1u64
        .checked_shl(SI_LOAD_SHIFT)
        .expect("SI_LOAD_SHIFT must fit in u64");
    let runnable = count_runnable_tasks() as u64;
    let sample = (runnable as u128 * scale as u128 / SYSINFO_LOAD_NCPUS as u128)
        .min(u64::MAX as u128) as u64;

    let now = monotonic_time_nanos();
    let last = SYSINFO_LOAD_LAST_NS.load(Ordering::Relaxed);
    let delta = now.saturating_sub(last);
    SYSINFO_LOAD_LAST_NS.store(now, Ordering::Relaxed);

    if last == 0 {
        SYSINFO_LOAD_EMA_1.store(sample, Ordering::Relaxed);
        SYSINFO_LOAD_EMA_5.store(sample, Ordering::Relaxed);
        SYSINFO_LOAD_EMA_15.store(sample, Ordering::Relaxed);
        return [sample, sample, sample];
    }

    let step = |slot: &AtomicU64, tau: u64| {
        let old = slot.load(Ordering::Relaxed);
        let new = ema_load_u64(old, sample, tau, delta);
        slot.store(new, Ordering::Relaxed);
        new
    };

    [
        step(&SYSINFO_LOAD_EMA_1, LOAD_TAU_1_NS),
        step(&SYSINFO_LOAD_EMA_5, LOAD_TAU_5_NS),
        step(&SYSINFO_LOAD_EMA_15, LOAD_TAU_15_NS),
    ]
}

pub fn sys_sysinfo(info: *mut sysinfo) -> AxResult<isize> {
    // Linux `sysinfo(2)`: `info` must be writable; NULL → EFAULT.
    if info.is_null() {
        return Err(AxError::BadAddress);
    }

    let total = total_ram_size();
    let free_pool = global_allocator()
        .available_pages()
        .saturating_mul(PAGE_SIZE_4K);
    let freeram = free_pool.min(total);

    let mut kinfo: sysinfo = unsafe { core::mem::zeroed() };
    kinfo.uptime = (monotonic_time_nanos() / NANOS_PER_SEC) as _;
    let loads = sysinfo_load_avg_fixed();
    kinfo.loads = [loads[0] as _, loads[1] as _, loads[2] as _];
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

#[allow(unused_variables)] // `len` only meaningful for READ* once klog exists; ABI always passes it.
pub fn sys_syslog(action: i32, buf: *mut c_char, len: usize) -> AxResult<isize> {
    if !(SYSLOG_ACTION_CLOSE..=SYSLOG_ACTION_SIZE_BUFFER).contains(&action) {
        return Err(AxError::InvalidInput);
    }

    match action {
        SYSLOG_ACTION_READ | SYSLOG_ACTION_READ_ALL | SYSLOG_ACTION_READ_CLEAR => {
            // Linux `do_syslog`: validate user buffer before discovering no printk ring (EFAULT vs
            // ENOTSUP order; issue-343). Starry still has no klog → `Unsupported` after a non-NULL `buf`.
            if buf.is_null() {
                return Err(AxError::BadAddress);
            }
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
    // Output buffer before `resolve`/`read_at` (issue-285; same class as issue-281).
    if buf.is_null() {
        return Err(AxError::BadAddress);
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

/// Linux `seccomp(2)` / `uapi/linux/seccomp.h` operation codes (subset; new ops need extending).
const SECCOMP_SET_MODE_STRICT: u32 = 0;
const SECCOMP_SET_MODE_FILTER: u32 = 1;
const SECCOMP_GET_ACTION_AVAIL: u32 = 2;
const SECCOMP_GET_NOTIF_SIZES: u32 = 3;
const SECCOMP_GET_NOTIF_FD: u32 = 4;

/// `SECCOMP_FILTER_FLAG_*` bits accepted for `SECCOMP_SET_MODE_FILTER` (Linux 6.x; extend when uapi adds flags).
const SECCOMP_FILTER_FLAGS_MASK: u32 = (1 << 0)
    | (1 << 1)
    | (1 << 2)
    | (1 << 3)
    | (1 << 4)
    | (1 << 5);

/// `struct sock_fprog` size on 64-bit Linux (`unsigned short` + pad + `struct sock_filter *`).
const SOCK_FPROG_BYTES: usize = 16;

/// Linux `struct seccomp_notif_sizes` (`3` x `__u16` + padding).
const SECCOMP_NOTIF_SIZES_BYTES: usize = 8;

/// `seccomp(2)`: validate `op`/`flags`/`args` like `kernel/seccomp.c` before reporting no seccomp
/// policy (issue-352; `PR_SET_SECCOMP` in `task/ctl.rs` remains a separate entry).
pub fn sys_seccomp(op: u32, flags: u32, args: usize) -> AxResult<isize> {
    match op {
        SECCOMP_SET_MODE_STRICT => {
            // Linux: `flags` and `args` must be zero / NULL.
            if flags != 0 || args != 0 {
                return Err(AxError::InvalidInput);
            }
        }
        SECCOMP_SET_MODE_FILTER => {
            if flags & !SECCOMP_FILTER_FLAGS_MASK != 0 {
                return Err(AxError::InvalidInput);
            }
            if args == 0 {
                return Err(AxError::InvalidInput);
            }
            check_access(args, SOCK_FPROG_BYTES).map_err(|_| AxError::BadAddress)?;
        }
        SECCOMP_GET_ACTION_AVAIL => {
            if flags != 0 {
                return Err(AxError::InvalidInput);
            }
            if args == 0 {
                return Err(AxError::InvalidInput);
            }
            check_access(args, size_of::<u32>()).map_err(|_| AxError::BadAddress)?;
        }
        SECCOMP_GET_NOTIF_SIZES => {
            if flags != 0 {
                return Err(AxError::InvalidInput);
            }
            if args == 0 {
                return Err(AxError::InvalidInput);
            }
            check_access(args, SECCOMP_NOTIF_SIZES_BYTES).map_err(|_| AxError::BadAddress)?;
        }
        SECCOMP_GET_NOTIF_FD => {
            if flags != 0 || args != 0 {
                return Err(AxError::InvalidInput);
            }
        }
        _ => return Err(AxError::InvalidInput),
    }

    Err(AxError::Unsupported)
}

/// Linux `SYS_RISCV_FLUSH_ICACHE_LOCAL` (`uapi/asm-riscv/cacheflush.h`).
#[cfg(target_arch = "riscv64")]
const SYS_RISCV_FLUSH_ICACHE_LOCAL: usize = 1;

/// Linux `riscv_flush_icache(2)` / `__riscv_flush_icache(start, end, flags)`.
#[cfg(target_arch = "riscv64")]
pub fn sys_riscv_flush_icache(start: usize, end: usize, flags: usize) -> AxResult<isize> {
    if flags & !SYS_RISCV_FLUSH_ICACHE_LOCAL != 0 {
        return Err(AxError::InvalidInput);
    }
    let local = flags & SYS_RISCV_FLUSH_ICACHE_LOCAL != 0;
    if local {
        if start > end {
            return Err(AxError::InvalidInput);
        }
        if start == end {
            return Ok(0);
        }
        let len = end - start;
        crate::mm::check_access(start, len).map_err(|_| AxError::InvalidInput)?;
    }
    // Linux: `flags==0` → `flush_icache_mm` (process-wide); `LOCAL` → `flush_icache_range`.
    // StarryOS: no `flush_icache_mm` walk / cross-hart IPI; `fence.i` serializes this hart's
    // instruction stream after stores to executable pages (issue-353; Zifencei).
    riscv::asm::fence_i();
    Ok(0)
}
