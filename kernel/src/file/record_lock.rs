//! POSIX advisory record locks for `fcntl` (`F_SETLK`, `F_GETLK`, …).

use alloc::vec::Vec;
use core::ffi::c_int;

use axerrno::{AxError, AxResult};
use axio::{Seek, SeekFrom};
use axsync::Mutex;
use axtask::current;
use lazy_static::lazy_static;
use linux_raw_sys::general::{F_RDLCK, F_UNLCK, F_WRLCK, SEEK_CUR, SEEK_END, SEEK_SET, flock64};
use starry_process::Pid;

use super::{File, FileLike, get_file_like};
use crate::task::AsThread;

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct InodeKey {
    dev: u64,
    ino: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PosixLockType {
    Read,
    Write,
}

struct LockEntry {
    inode: InodeKey,
    start: u64,
    end: u64,
    typ: PosixLockType,
    pid: Pid,
    fd: u32,
}

lazy_static! {
    static ref REC_LOCK: Mutex<Vec<LockEntry>> = Mutex::new(Vec::new());
}

fn inode_key(fd: c_int) -> AxResult<InodeKey> {
    let f = get_file_like(fd)?;
    if f.downcast_ref::<File>().is_none() {
        return Err(AxError::InvalidInput);
    }
    let m = f.stat()?;
    Ok(InodeKey {
        dev: m.dev,
        ino: m.ino,
    })
}

fn file_size_and_offset(fd: c_int) -> AxResult<(u64, u64)> {
    let f_arc = get_file_like(fd)?;
    let size = f_arc.stat()?.size;
    let f = File::from_fd(fd)?;
    let mut inner = f.inner();
    let cur = inner.seek(SeekFrom::Current(0))?;
    Ok((size, cur))
}

fn resolve_range(fl: &flock64, fsize: u64, cur: u64) -> AxResult<(u64, u64)> {
    let whence = fl.l_whence as u32;
    let start = if whence == SEEK_SET {
        fl.l_start
    } else if whence == SEEK_CUR {
        cur as i64 + fl.l_start
    } else if whence == SEEK_END {
        fsize as i64 + fl.l_start
    } else {
        return Err(AxError::InvalidInput);
    };
    if start < 0 {
        return Err(AxError::InvalidInput);
    }
    let start = start as u64;
    if fl.l_len == 0 {
        return Ok((start, u64::MAX));
    }
    if fl.l_len > 0 {
        let end = start
            .checked_add(fl.l_len as u64)
            .ok_or(AxError::InvalidInput)?;
        if end <= start {
            return Err(AxError::InvalidInput);
        }
        return Ok((start, end));
    }
    let end = start;
    let start = (start as i64 + fl.l_len) as u64;
    if start >= end {
        return Err(AxError::InvalidInput);
    }
    Ok((start, end))
}

fn range_overlaps(s1: u64, e1: u64, s2: u64, e2: u64) -> bool {
    s1 < e2 && s2 < e1
}

fn lock_types_conflict(a: PosixLockType, b: PosixLockType) -> bool {
    matches!(a, PosixLockType::Write) || matches!(b, PosixLockType::Write)
}

fn posix_type_from_fcntl(t: i16) -> AxResult<PosixLockType> {
    match t as u32 {
        x if x == F_RDLCK => Ok(PosixLockType::Read),
        x if x == F_WRLCK => Ok(PosixLockType::Write),
        _ => Err(AxError::InvalidInput),
    }
}

fn unlock_range_locked(locks: &mut Vec<LockEntry>, key: InodeKey, pid: Pid, r0: u64, r1: u64) {
    let mut i = 0;
    while i < locks.len() {
        let e = &locks[i];
        if e.inode != key || e.pid != pid {
            i += 1;
            continue;
        }
        if !range_overlaps(e.start, e.end, r0, r1) {
            i += 1;
            continue;
        }
        let e = locks.remove(i);
        let left_end = e.end.min(r0);
        if e.start < left_end {
            locks.push(LockEntry { end: left_end, ..e });
        }
        if r1 < e.end {
            let right_start = r1.max(e.start);
            if right_start < e.end {
                locks.push(LockEntry {
                    start: right_start,
                    ..e
                });
            }
        }
    }
}

pub fn release_fd(fd: c_int) {
    let fd_u = fd as u32;
    let mut g = REC_LOCK.lock();
    g.retain(|e| e.fd != fd_u);
}

pub fn sys_fcntl_setlk(fd: c_int, blocking: bool, fl: &flock64) -> AxResult<isize> {
    let key = inode_key(fd)?;
    let (fsize, cur) = file_size_and_offset(fd)?;
    let pid = current().as_thread().proc_data.proc.pid();
    let fd_u = fd as u32;
    let range = resolve_range(fl, fsize, cur)?;
    let l_type = fl.l_type as u32;

    if l_type == F_UNLCK {
        let mut g = REC_LOCK.lock();
        unlock_range_locked(&mut g, key, pid, range.0, range.1);
        return Ok(0);
    }

    let new_typ = posix_type_from_fcntl(fl.l_type)?;

    loop {
        let mut g = REC_LOCK.lock();
        let mut conflict = false;
        for e in g.iter() {
            if e.inode != key || e.pid == pid {
                continue;
            }
            if !range_overlaps(e.start, e.end, range.0, range.1) {
                continue;
            }
            if lock_types_conflict(new_typ, e.typ) {
                conflict = true;
                break;
            }
        }
        if conflict {
            drop(g);
            if blocking {
                axtask::yield_now();
                continue;
            }
            return Err(AxError::WouldBlock);
        }

        unlock_range_locked(&mut g, key, pid, range.0, range.1);
        g.push(LockEntry {
            inode: key,
            start: range.0,
            end: range.1,
            typ: new_typ,
            pid,
            fd: fd_u,
        });
        return Ok(0);
    }
}

pub fn sys_fcntl_getlk(fd: c_int, fl: &mut flock64) -> AxResult<isize> {
    let key = inode_key(fd)?;
    let (fsize, cur) = file_size_and_offset(fd)?;
    let pid = current().as_thread().proc_data.proc.pid();
    let want = posix_type_from_fcntl(fl.l_type)?;
    let range = resolve_range(fl, fsize, cur)?;

    let g = REC_LOCK.lock();
    for e in g.iter() {
        if e.inode != key || e.pid == pid {
            continue;
        }
        if !range_overlaps(e.start, e.end, range.0, range.1) {
            continue;
        }
        if !lock_types_conflict(want, e.typ) {
            continue;
        }
        fl.l_type = match e.typ {
            PosixLockType::Read => F_RDLCK as _,
            PosixLockType::Write => F_WRLCK as _,
        };
        fl.l_whence = SEEK_SET as _;
        fl.l_start = e.start as _;
        fl.l_len = if e.end == u64::MAX {
            0
        } else {
            (e.end - e.start) as _
        };
        fl.l_pid = e.pid as _;
        return Ok(0);
    }
    fl.l_type = F_UNLCK as _;
    Ok(0)
}
