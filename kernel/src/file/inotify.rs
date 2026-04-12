use alloc::{borrow::Cow, sync::Arc};
use core::{
    sync::atomic::{AtomicBool, Ordering},
    task::Context,
};

use axerrno::AxResult;
use axpoll::{IoEvents, Pollable};

use crate::file::FileLike;

/// Placeholder inotify instance (no watch/event delivery yet); exposes a stable anon_inode path.
pub struct InotifyFd {
    non_blocking: AtomicBool,
}

impl InotifyFd {
    pub fn new(nonblocking: bool) -> Arc<Self> {
        Arc::new(Self {
            non_blocking: AtomicBool::new(nonblocking),
        })
    }
}

impl FileLike for InotifyFd {
    fn path(&self) -> Cow<'_, str> {
        "anon_inode:[inotify]".into()
    }

    fn nonblocking(&self) -> bool {
        self.non_blocking.load(Ordering::Acquire)
    }

    fn set_nonblocking(&self, nonblocking: bool) -> AxResult {
        self.non_blocking.store(nonblocking, Ordering::Release);
        Ok(())
    }
}

impl Pollable for InotifyFd {
    fn poll(&self) -> IoEvents {
        IoEvents::empty()
    }

    fn register(&self, _context: &mut Context<'_>, _events: IoEvents) {}
}

/// Placeholder fanotify instance (no marks/events yet); exposes a stable anon_inode path.
pub struct FanotifyFd {
    non_blocking: AtomicBool,
}

impl FanotifyFd {
    pub fn new(nonblocking: bool) -> Arc<Self> {
        Arc::new(Self {
            non_blocking: AtomicBool::new(nonblocking),
        })
    }
}

impl FileLike for FanotifyFd {
    fn path(&self) -> Cow<'_, str> {
        "anon_inode:[fanotify]".into()
    }

    fn nonblocking(&self) -> bool {
        self.non_blocking.load(Ordering::Acquire)
    }

    fn set_nonblocking(&self, nonblocking: bool) -> AxResult {
        self.non_blocking.store(nonblocking, Ordering::Release);
        Ok(())
    }
}

impl Pollable for FanotifyFd {
    fn poll(&self) -> IoEvents {
        IoEvents::empty()
    }

    fn register(&self, _context: &mut Context<'_>, _events: IoEvents) {}
}
