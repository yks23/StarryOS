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
    /// `event_f_flags` from `fanotify_init(2)` (validated `O_*` bits). Linux stores these on the
    /// fanotify group for opening/accessing paths when delivering events; future `fanotify_mark` /
    /// event paths should apply them (issue-328).
    #[allow(dead_code)]
    event_f_flags: u32,
}

impl FanotifyFd {
    pub fn new(nonblocking: bool, event_f_flags: u32) -> Arc<Self> {
        Arc::new(Self {
            non_blocking: AtomicBool::new(nonblocking),
            event_f_flags,
        })
    }

    /// Validated `O_*` bits from `fanotify_init`; for future mark/event fd creation.
    #[allow(dead_code)]
    #[inline]
    pub fn event_f_flags(&self) -> u32 {
        self.event_f_flags
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
