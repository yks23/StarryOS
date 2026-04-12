use alloc::sync::Arc;

use axerrno::{AxError, AxResult};
use starry_vm::{VmMutPtr, VmPtr};

use crate::file::{IoUringFd, add_file_like};

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IoSqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    resv2: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IoCqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    resv: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: IoSqringOffsets,
    cq_off: IoCqringOffsets,
}

/// Minimal `io_uring_setup`: validates entries, copies back adjusted queue sizes, returns an
/// `anon_inode:[io_uring]` fd. Ring mmap and I/O are not supported.
pub fn sys_io_uring_setup(entries: u32, params: *mut IoUringParams) -> AxResult<isize> {
    if params.is_null() {
        return Err(AxError::InvalidInput);
    }
    if entries == 0 || entries > 4096 {
        return Err(AxError::InvalidInput);
    }
    let mut p = unsafe { params.vm_read_uninit()?.assume_init() };
    if p.flags != 0 {
        return Err(AxError::InvalidInput);
    }
    let sqe = entries.next_power_of_two();
    p.sq_entries = sqe;
    p.cq_entries = sqe.saturating_mul(2);
    p.features = 0;
    p.wq_fd = 0;
    params.vm_write(p)?;
    add_file_like(Arc::new(IoUringFd), false).map(|fd| fd as isize)
}
