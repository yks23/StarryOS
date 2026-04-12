use alloc::collections::BTreeMap;
use core::ffi::c_int;

use axerrno::{AxError, AxResult};
use axsync::Mutex;
use axtask::current;
use hashbrown::HashMap;
use lazy_static::lazy_static;
use linux_raw_sys::general::{LOCK_EX, LOCK_NB, LOCK_SH, LOCK_UN};

use crate::file::{Directory, File, get_file_like};

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct FlockInodeKey {
    dev: u64,
    ino: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FlockKind {
    Ex,
    Sh,
}

struct FlockFdEntry {
    key: FlockInodeKey,
    kind: FlockKind,
    task: u64,
}

enum InodeLocks {
    Exclusive { task: u64, refs: u32 },
    Shared { map: BTreeMap<u64, u32> },
}

struct FlockTables {
    by_fd: HashMap<u32, FlockFdEntry>,
    by_inode: HashMap<FlockInodeKey, InodeLocks>,
}

lazy_static! {
    static ref FLOCK: Mutex<FlockTables> = Mutex::new(FlockTables {
        by_fd: HashMap::new(),
        by_inode: HashMap::new(),
    });
}

fn flock_inode_key(fd: i32) -> AxResult<FlockInodeKey> {
    let f = get_file_like(fd)?;
    if let Some(file) = f.downcast_ref::<File>() {
        let m = file.inner().location().metadata()?;
        Ok(FlockInodeKey {
            dev: m.device,
            ino: m.inode,
        })
    } else if let Some(dir) = f.downcast_ref::<Directory>() {
        let m = dir.inner().metadata()?;
        Ok(FlockInodeKey {
            dev: m.device,
            ino: m.inode,
        })
    } else {
        Err(AxError::InvalidInput)
    }
}

fn release_fd_locked(g: &mut FlockTables, fd: u32) {
    let Some(entry) = g.by_fd.remove(&fd) else {
        return;
    };
    match g.by_inode.get_mut(&entry.key) {
        Some(InodeLocks::Exclusive { task, refs }) if *task == entry.task => {
            *refs = refs.saturating_sub(1);
            if *refs == 0 {
                g.by_inode.remove(&entry.key);
            }
        }
        Some(InodeLocks::Shared { map }) => {
            if let Some(n) = map.get_mut(&entry.task) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    map.remove(&entry.task);
                }
            }
            if map.is_empty() {
                g.by_inode.remove(&entry.key);
            }
        }
        _ => {}
    }
}

fn try_acquire_exclusive(
    g: &mut FlockTables,
    fd: u32,
    key: FlockInodeKey,
    tid: u64,
) -> Result<(), AxError> {
    match g.by_inode.get_mut(&key) {
        None => {
            g.by_inode
                .insert(key, InodeLocks::Exclusive { task: tid, refs: 1 });
            g.by_fd.insert(
                fd,
                FlockFdEntry {
                    key,
                    kind: FlockKind::Ex,
                    task: tid,
                },
            );
            Ok(())
        }
        Some(InodeLocks::Exclusive { task, refs }) if *task == tid => {
            *refs += 1;
            g.by_fd.insert(
                fd,
                FlockFdEntry {
                    key,
                    kind: FlockKind::Ex,
                    task: tid,
                },
            );
            Ok(())
        }
        Some(InodeLocks::Exclusive { .. }) | Some(InodeLocks::Shared { .. }) => {
            Err(AxError::WouldBlock)
        }
    }
}

fn try_acquire_shared(
    g: &mut FlockTables,
    fd: u32,
    key: FlockInodeKey,
    tid: u64,
) -> Result<(), AxError> {
    match g.by_inode.get_mut(&key) {
        None => {
            let mut map = BTreeMap::new();
            map.insert(tid, 1);
            g.by_inode.insert(key, InodeLocks::Shared { map });
            g.by_fd.insert(
                fd,
                FlockFdEntry {
                    key,
                    kind: FlockKind::Sh,
                    task: tid,
                },
            );
            Ok(())
        }
        Some(InodeLocks::Exclusive { .. }) => Err(AxError::WouldBlock),
        Some(InodeLocks::Shared { map }) => {
            *map.entry(tid).or_insert(0) += 1;
            g.by_fd.insert(
                fd,
                FlockFdEntry {
                    key,
                    kind: FlockKind::Sh,
                    task: tid,
                },
            );
            Ok(())
        }
    }
}

/// Called when an fd is closed; drops any BSD flock held by that fd.
pub fn release_fd(fd: c_int) {
    let mut g = FLOCK.lock();
    release_fd_locked(&mut g, fd as u32);
}

pub fn sys_flock(fd: c_int, operation: c_int) -> AxResult<isize> {
    let key = flock_inode_key(fd)?;
    let tid = current().id().as_u64();
    let fdu = fd as u32;
    let op = operation as u32;

    const ALLOWED: u32 = LOCK_NB | LOCK_SH | LOCK_EX | LOCK_UN;
    if op & !ALLOWED != 0 {
        return Err(AxError::InvalidInput);
    }
    let cmd = op & !LOCK_NB;
    if cmd != LOCK_SH && cmd != LOCK_EX && cmd != LOCK_UN {
        return Err(AxError::InvalidInput);
    }

    let nb = op & LOCK_NB != 0;

    if cmd == LOCK_UN {
        let mut g = FLOCK.lock();
        release_fd_locked(&mut g, fdu);
        return Ok(0);
    }

    loop {
        let mut g = FLOCK.lock();
        if let Some(e) = g.by_fd.get(&fdu) {
            if cmd == LOCK_EX && e.kind == FlockKind::Ex {
                return Ok(0);
            }
            if cmd == LOCK_SH && e.kind == FlockKind::Sh {
                return Ok(0);
            }
            release_fd_locked(&mut g, fdu);
        }

        let r = if cmd == LOCK_EX {
            try_acquire_exclusive(&mut g, fdu, key, tid)
        } else {
            try_acquire_shared(&mut g, fdu, key, tid)
        };

        match r {
            Ok(()) => return Ok(0),
            Err(AxError::WouldBlock) if !nb => {
                drop(g);
                axtask::yield_now();
            }
            Err(e) => return Err(e),
        }
    }
}
