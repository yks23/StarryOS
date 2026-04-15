use alloc::borrow::Cow;
use core::task::Context;

use axpoll::{IoEvents, Pollable};

use crate::file::FileLike;

/// Placeholder fd for `io_uring_setup`; reports the Linux anon_inode path so
/// `/proc/self/fd/N` is not `anon_inode:[dummy]`. Submission/completion rings are not implemented.
pub struct IoUringFd;

impl FileLike for IoUringFd {
    fn path(&self) -> Cow<'_, str> {
        "anon_inode:[io_uring]".into()
    }
}

impl Pollable for IoUringFd {
    fn poll(&self) -> IoEvents {
        IoEvents::empty()
    }

    fn register(&self, _context: &mut Context<'_>, _events: IoEvents) {}
}
