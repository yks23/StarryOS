use alloc::{borrow::Cow, format, sync::Arc};
use core::{
    ffi::c_int,
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
    task::Context,
};

use axerrno::{AxError, AxResult};
use axnet::{
    RecvOptions, SendOptions, Socket as SocketInner, SocketOps,
    options::{Configurable, GetSocketOption, SetSocketOption},
};
use axpoll::{IoEvents, Pollable};
use linux_raw_sys::general::S_IFSOCK;

use super::{FileLike, Kstat};
use crate::file::{IoDst, IoSrc, get_file_like};

/// Monotonic inode numbers for `stat` / `socket:[ino]` paths (sockfs-like).
static NEXT_SOCK_INO: AtomicU64 = AtomicU64::new(2);

/// Distinct from the old `Kstat::default()` stub `st_dev == 0`.
const SOCKFS_STAT_DEV: u64 = 0x0100_0000_0000_0001;

pub struct Socket {
    pub inner: SocketInner,
    sock_ino: u64,
    sock_dev: u64,
}

impl Socket {
    pub fn new(inner: SocketInner) -> Self {
        Self {
            inner,
            sock_ino: NEXT_SOCK_INO.fetch_add(1, Ordering::Relaxed),
            sock_dev: SOCKFS_STAT_DEV,
        }
    }
}

impl Deref for Socket {
    type Target = SocketInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl FileLike for Socket {
    fn read(&self, dst: &mut IoDst) -> AxResult<usize> {
        self.recv(dst, RecvOptions::default())
    }

    fn write(&self, src: &mut IoSrc) -> AxResult<usize> {
        self.send(src, SendOptions::default())
    }

    fn stat(&self) -> AxResult<Kstat> {
        Ok(Kstat {
            dev: self.sock_dev,
            ino: self.sock_ino,
            nlink: 1,
            mode: S_IFSOCK | 0o777u32, // rwxrwxrwx
            blksize: 4096,
            ..Default::default()
        })
    }

    fn nonblocking(&self) -> bool {
        let mut result = false;
        self.get_option(GetSocketOption::NonBlocking(&mut result))
            .unwrap();
        result
    }

    fn set_nonblocking(&self, nonblocking: bool) -> AxResult<()> {
        self.inner
            .set_option(SetSocketOption::NonBlocking(&nonblocking))
    }

    fn path(&self) -> Cow<'_, str> {
        format!("socket:[{}]", self.sock_ino).into()
    }

    fn from_fd(fd: c_int) -> AxResult<Arc<Self>>
    where
        Self: Sized + 'static,
    {
        get_file_like(fd)?
            .downcast_arc()
            .map_err(|_| AxError::NotASocket)
    }
}
impl Pollable for Socket {
    fn poll(&self) -> IoEvents {
        self.inner.poll()
    }

    fn register(&self, context: &mut Context<'_>, events: IoEvents) {
        self.inner.register(context, events);
    }
}
