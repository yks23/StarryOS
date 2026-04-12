// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2025 KylinSoft Co., Ltd. <https://www.kylinos.cn/>
// Copyright (C) 2025 Azure-stars <Azure_stars@126.com>
// Copyright (C) 2025 Yuekai Jia <equation618@gmail.com>
// See LICENSES for license details.
//
// This file has been modified by KylinSoft on 2025.

use alloc::vec;

use axerrno::{AxError, AxResult};
use axhal::paging::MappingFlags;
use axtask::current;
use memory_addr::{MemoryAddr, PAGE_SIZE_4K, VirtAddr};
use starry_vm::vm_write_slice;

use crate::{
    config::{USER_SPACE_BASE, USER_SPACE_SIZE},
    task::AsThread,
};

/// Check whether pages are resident in memory.
///
/// The mincore() system call determines whether pages of the calling process's
/// virtual memory are resident in RAM.
///
/// # Arguments
/// * `addr` - Starting address (must be a multiple of the page size)
/// * `length` - Length of the region in bytes (effectively rounded up to next page boundary)
/// * `vec` - Output array containing at least (length+PAGE_SIZE-1)/PAGE_SIZE bytes.
///
/// # Return Value
/// * `Ok(0)` on success
/// * `Err(EAGAIN)` - Kernel is temporarily out of resources (not implemented in StarryOS)
/// * `Err(EFAULT)` - vec points to an invalid address (handled by vm_write_slice)
/// * `Err(EINVAL)` - addr is not a multiple of the page size
/// * `Err(ENOMEM)` - length is greater than (TASK_SIZE - addr), or negative length, or `addr` to `addr`+`length` contained unmapped memory
///
/// # Notes from Linux man page
/// - The least significant bit (bit 0) is set if page is resident in memory
/// - Bits 1-7 are reserved and currently cleared
/// - Information is only a snapshot; pages can be swapped at any moment
///
/// # Linux Errors
/// - EAGAIN:  kernel temporarily out of resources
/// - EFAULT: vec points to invalid address
/// - EINVAL: addr not page-aligned
/// - ENOMEM: length > (TASK_SIZE - addr), negative length, or unmapped memory
pub fn sys_mincore(addr: usize, length: usize, vec: *mut u8) -> AxResult<isize> {
    let start_addr = VirtAddr::from(addr);

    // EINVAL: addr must be a multiple of the page size
    if !start_addr.is_aligned(PAGE_SIZE_4K) {
        return Err(AxError::InvalidInput);
    }

    // length=0: Linux mm/mincore.c returns0 without touching `vec` (NULL is ok).
    // POSIX: zero-length mincore is a no-op after addr alignment is checked.
    if length == 0 {
        return Ok(0);
    }

    // EFAULT: vec must not be null when any output is produced
    if vec.is_null() {
        return Err(AxError::BadAddress);
    }

    debug!("sys_mincore <= addr: {addr:#x}, length: {length:#x}, vec: {vec:?}");

    // Linux ENOMEM: `length > TASK_SIZE - addr` (overflow or past user AS end). Reject before
    // `vec![..; page_count]` so absurd `length` cannot force huge kernel allocations (issue-354).
    const USER_SPACE_END: usize = USER_SPACE_BASE + USER_SPACE_SIZE;
    let end = addr.checked_add(length).ok_or(AxError::NoMemory)?;
    if addr < USER_SPACE_BASE || end > USER_SPACE_END {
        return Err(AxError::NoMemory);
    }

    // Calculate number of pages to check
    let page_count = length.div_ceil(PAGE_SIZE_4K);

    // Get current address space
    let curr = current();
    let aspace = curr.as_thread().proc_data.aspace.read();

    let mut result = vec![0u8; page_count];
    let mut i = 0;

    while i < page_count {
        let addr = start_addr + i * PAGE_SIZE_4K;

        // ENOMEM: Check if this page is within a valid VMA
        let area = aspace.find_area(addr).ok_or(AxError::NoMemory)?;

        // Verify we have at least USER access permission
        if !area.flags().contains(MappingFlags::USER) {
            return Err(AxError::NoMemory);
        }

        // Query page table with batch awareness
        let (is_resident, size) = match aspace.page_table().query(addr) {
            Ok((_, _, size)) => {
                // Physical page exists and is resident
                // page_size tells us how many contiguous pages have the same status
                (true, size as _)
            }
            Err(_) => {
                // Page is mapped but not populated (lazy allocation)
                // We need to determine how many contiguous pages are also not populated
                // For safety, we check the next page or use PAGE_SIZE_4K as minimum step
                (false, PAGE_SIZE_4K)
            }
        };
        let n = size / PAGE_SIZE_4K;

        if is_resident {
            let end = (i + n).min(page_count);
            result[i..end].fill(1);
        }

        i += n;
    }

    // EFAULT: Write result to user space
    // vm_write_slice will return EFAULT if vec is invalid
    vm_write_slice(vec, result.as_slice())?;

    Ok(0)
}
