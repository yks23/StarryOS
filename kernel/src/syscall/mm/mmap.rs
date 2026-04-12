use alloc::sync::Arc;

use axerrno::{AxError, AxResult};
use axfs::FileBackend;
use axhal::paging::{MappingFlags, PageSize};
use axtask::current;
use linux_raw_sys::general::*;
use memory_addr::{MemoryAddr, VirtAddr, VirtAddrRange, align_up_4k};

use crate::{
    file::{File, FileLike},
    mm::{Backend, SharedPages},
    pseudofs::{Device, DeviceMmap},
    task::AsThread,
};

bitflags::bitflags! {
    /// `PROT_*` flags for use with [`sys_mmap`].
    ///
    /// For `PROT_NONE`, use `ProtFlags::empty()`.
    #[derive(Debug, Clone, Copy)]
    struct MmapProt: u32 {
        /// Page can be read.
        const READ = PROT_READ;
        /// Page can be written.
        const WRITE = PROT_WRITE;
        /// Page can be executed.
        const EXEC = PROT_EXEC;
        /// Extend change to start of growsdown vma (mprotect only).
        const GROWDOWN = PROT_GROWSDOWN;
        /// Extend change to start of growsup vma (mprotect only).
        const GROWSUP = PROT_GROWSUP;
    }
}

impl From<MmapProt> for MappingFlags {
    fn from(value: MmapProt) -> Self {
        let mut flags = MappingFlags::USER;
        if value.contains(MmapProt::READ) {
            flags |= MappingFlags::READ;
        }
        if value.contains(MmapProt::WRITE) {
            flags |= MappingFlags::WRITE;
        }
        if value.contains(MmapProt::EXEC) {
            flags |= MappingFlags::EXECUTE;
        }
        flags
    }
}

bitflags::bitflags! {
    /// flags for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct MmapFlags: u32 {
        /// Share changes
        const SHARED = MAP_SHARED;
        /// Share changes, but fail if mapping flags contain unknown
        const SHARED_VALIDATE = MAP_SHARED_VALIDATE;
        /// Changes private; copy pages on write.
        const PRIVATE = MAP_PRIVATE;
        /// Map address must be exactly as requested, no matter whether it is available.
        const FIXED = MAP_FIXED;
        /// Same as `FIXED`, but if the requested address overlaps an existing
        /// mapping, the call fails instead of replacing the existing mapping.
        const FIXED_NOREPLACE = MAP_FIXED_NOREPLACE;
        /// Don't use a file.
        const ANONYMOUS = MAP_ANONYMOUS;
        /// Populate the mapping.
        const POPULATE = MAP_POPULATE;
        /// Don't check for reservations.
        const NORESERVE = MAP_NORESERVE;
        /// Allocation is for a stack.
        const STACK = MAP_STACK;
        /// Huge page
        const HUGE = MAP_HUGETLB;
        /// Huge page 1g size
        const HUGE_1GB = MAP_HUGETLB | MAP_HUGE_1GB;
        /// Deprecated flag
        const DENYWRITE = MAP_DENYWRITE;
        /// Nonblocking map (best-effort populate).
        const NONBLOCK = MAP_NONBLOCK;
        /// Synchronous page faults.
        const SYNC = MAP_SYNC;
        /// Uninitialized mapping (MIPS etc.).
        const UNINITIALIZED = MAP_UNINITIALIZED;
        /// Grows down (stack-like).
        const GROWSDOWN = MAP_GROWSDOWN;
        /// Legacy executable mapping.
        const EXECUTABLE = MAP_EXECUTABLE;
        /// Lock mapped pages.
        const LOCKED = MAP_LOCKED;
        /// Droppable mapping (Linux 6+).
        const DROPPABLE = MAP_DROPPABLE;
        /// Huge page size encodings (`MAP_HUGE_*` from `linux/mman.h`).
        const HUGE_16KB = MAP_HUGE_16KB;
        const HUGE_64KB = MAP_HUGE_64KB;
        const HUGE_512KB = MAP_HUGE_512KB;
        const HUGE_1MB = MAP_HUGE_1MB;
        const HUGE_2MB = MAP_HUGE_2MB;
        const HUGE_8MB = MAP_HUGE_8MB;
        const HUGE_16MB = MAP_HUGE_16MB;
        const HUGE_32MB = MAP_HUGE_32MB;
        const HUGE_256MB = MAP_HUGE_256MB;
        const HUGE_512MB = MAP_HUGE_512MB;
        const HUGE_2GB = MAP_HUGE_2GB;
        const HUGE_16GB = MAP_HUGE_16GB;

        /// Mask for type of mapping
        const TYPE = MAP_TYPE;
    }
}

/// All `MAP_*` bits that may appear in `mmap(2)` `flags` (see `linux/mman.h` / uapi).
const ALLOWED_MAP_FLAGS: u32 = MAP_TYPE
    | MAP_FIXED
    | MAP_ANONYMOUS
    | MAP_GROWSDOWN
    | MAP_DENYWRITE
    | MAP_EXECUTABLE
    | MAP_LOCKED
    | MAP_NORESERVE
    | MAP_POPULATE
    | MAP_NONBLOCK
    | MAP_STACK
    | MAP_HUGETLB
    | MAP_SYNC
    | MAP_FIXED_NOREPLACE
    | MAP_UNINITIALIZED
    | MAP_DROPPABLE
    | (MAP_HUGE_MASK << MAP_HUGE_SHIFT);

pub fn sys_mmap(
    addr: usize,
    length: usize,
    prot: u32,
    flags: u32,
    fd: i32,
    offset: isize,
) -> AxResult<isize> {
    if length == 0 {
        return Err(AxError::InvalidInput);
    }

    let curr = current();
    let mut aspace = curr.as_thread().proc_data.aspace.write();
    let permission_flags = MmapProt::from_bits(prot).ok_or(AxError::InvalidInput)?;
    if permission_flags.intersects(MmapProt::GROWDOWN | MmapProt::GROWSUP) {
        return Err(AxError::InvalidInput);
    }
    if flags & !ALLOWED_MAP_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }
    let map_flags = match MmapFlags::from_bits(flags) {
        Some(flags) => flags,
        None => {
            if (flags & MmapFlags::TYPE.bits()) == MmapFlags::SHARED_VALIDATE.bits() {
                return Err(AxError::OperationNotSupported);
            }
            return Err(AxError::InvalidInput);
        }
    };
    let map_type = map_flags & MmapFlags::TYPE;
    if !matches!(
        map_type,
        MmapFlags::PRIVATE | MmapFlags::SHARED | MmapFlags::SHARED_VALIDATE
    ) {
        return Err(AxError::InvalidInput);
    }
    // Linux 2.6.12+: `MAP_ANONYMOUS` ignores `fd` (may be any value); still require `offset == 0`.
    // File-backed mmap without `MAP_ANONYMOUS` needs a positive fd (issue-264).
    if map_flags.contains(MmapFlags::ANONYMOUS) {
        if offset != 0 {
            return Err(AxError::InvalidInput);
        }
    } else if fd <= 0 {
        return Err(AxError::InvalidInput);
    }
    let offset: usize = offset.try_into().map_err(|_| AxError::InvalidInput)?;
    if !PageSize::Size4K.is_aligned(offset) {
        return Err(AxError::InvalidInput);
    }

    debug!(
        "sys_mmap <= addr: {addr:#x?}, length: {length:#x?}, prot: {permission_flags:?}, flags: \
         {map_flags:?}, fd: {fd:?}, offset: {offset:?}"
    );

    let page_size = if map_flags.contains(MmapFlags::HUGE_1GB) {
        PageSize::Size1G
    } else if map_flags.contains(MmapFlags::HUGE) {
        PageSize::Size2M
    } else {
        PageSize::Size4K
    };

    let start = addr.align_down(page_size);
    let end = (addr + length).align_up(page_size);
    let mut length = end - start;

    let start = if map_flags.intersects(MmapFlags::FIXED | MmapFlags::FIXED_NOREPLACE) {
        let dst_addr = VirtAddr::from(start);
        if !map_flags.contains(MmapFlags::FIXED_NOREPLACE) {
            aspace.unmap(dst_addr, length)?;
        }
        dst_addr
    } else {
        let align = page_size as usize;
        aspace
            .find_free_area(
                VirtAddr::from(start),
                length,
                VirtAddrRange::new(aspace.base(), aspace.end()),
                align,
            )
            .or(aspace.find_free_area(
                aspace.base(),
                length,
                VirtAddrRange::new(aspace.base(), aspace.end()),
                align,
            ))
            .ok_or(AxError::NoMemory)?
    };

    // Anonymous mappings never use `fd` as a backing file descriptor (match Linux).
    let file = if map_flags.contains(MmapFlags::ANONYMOUS) {
        None
    } else {
        Some(File::from_fd(fd)?)
    };

    let backend = match map_type {
        MmapFlags::SHARED | MmapFlags::SHARED_VALIDATE => {
            if let Some(file) = file {
                let file = file.inner();
                let backend = file.backend()?.clone();
                match file.backend()?.clone() {
                    FileBackend::Cached(cache) => {
                        // TODO(mivik): file mmap page size
                        Backend::new_file(
                            start,
                            cache,
                            file.flags(),
                            offset,
                            &curr.as_thread().proc_data.aspace,
                        )
                    }
                    FileBackend::Direct(loc) => {
                        let device = loc
                            .entry()
                            .downcast::<Device>()
                            .map_err(|_| AxError::NoSuchDevice)?;

                        match device.mmap() {
                            DeviceMmap::None => {
                                return Err(AxError::NoSuchDevice);
                            }
                            DeviceMmap::ReadOnly => {
                                Backend::new_cow(start, page_size, backend, offset as u64, None)
                            }
                            DeviceMmap::Physical(mut range) => {
                                range.start += offset;
                                if range.is_empty() {
                                    return Err(AxError::InvalidInput);
                                }
                                length = length.min(range.size().align_down(page_size));
                                Backend::new_linear(
                                    start.as_usize() as isize - range.start.as_usize() as isize,
                                )
                            }
                            DeviceMmap::Cache(cache) => Backend::new_file(
                                start,
                                cache,
                                file.flags(),
                                offset,
                                &curr.as_thread().proc_data.aspace,
                            ),
                        }
                    }
                }
            } else {
                Backend::new_shared(start, Arc::new(SharedPages::new(length, PageSize::Size4K)?))
            }
        }
        MmapFlags::PRIVATE => {
            if let Some(file) = file {
                // Private mapping from a file
                let backend = file.inner().backend()?.clone();
                Backend::new_cow(start, page_size, backend, offset as u64, None)
            } else {
                Backend::new_alloc(start, page_size)
            }
        }
        _ => return Err(AxError::InvalidInput),
    };

    let populate = map_flags.contains(MmapFlags::POPULATE);
    aspace.map(start, length, permission_flags.into(), populate, backend)?;

    Ok(start.as_usize() as _)
}

pub fn sys_munmap(addr: usize, length: usize) -> AxResult<isize> {
    debug!("sys_munmap <= addr: {addr:#x}, length: {length:x}");
    if length == 0 {
        return Err(AxError::InvalidInput);
    }
    let start_addr = VirtAddr::from(addr);
    // Linux/POSIX: `addr` must be page-aligned (issue-268).
    if !start_addr.is_aligned_4k() {
        return Err(AxError::InvalidInput);
    }
    let curr = current();
    let mut aspace = curr.as_thread().proc_data.aspace.write();
    let length = align_up_4k(length);
    aspace.unmap(start_addr, length)?;
    Ok(0)
}

pub fn sys_mprotect(addr: usize, length: usize, prot: u32) -> AxResult<isize> {
    // TODO: implement PROT_GROWSUP & PROT_GROWSDOWN
    let Some(permission_flags) = MmapProt::from_bits(prot) else {
        return Err(AxError::InvalidInput);
    };
    debug!("sys_mprotect <= addr: {addr:#x}, length: {length:x}, prot: {permission_flags:?}");

    if permission_flags.intersects(MmapProt::GROWDOWN | MmapProt::GROWSUP) {
        return Err(AxError::InvalidInput);
    }

    let start_addr = VirtAddr::from(addr);
    // Linux/POSIX: `addr` must be page-aligned (issue-270).
    if !start_addr.is_aligned_4k() {
        return Err(AxError::InvalidInput);
    }

    // Linux `do_mprotect_pkey`: `if (!len) return 0;` — zero-length no-op (issue-280). Unlike
    // `munmap` / `mmap` zero-length rules.
    if length == 0 {
        return Ok(0);
    }

    let curr = current();
    let mut aspace = curr.as_thread().proc_data.aspace.write();
    let length = align_up_4k(length);
    aspace.protect(start_addr, length, permission_flags.into())?;

    Ok(0)
}

pub fn sys_mremap(
    addr: usize,
    old_size: usize,
    new_size: usize,
    flags: u32,
    new_addr: usize,
) -> AxResult<isize> {
    debug!(
        "sys_mremap <= addr: {addr:#x}, old_size: {old_size:x}, new_size: {new_size:x}, flags: \
         {flags:#x}, new_addr: {new_addr:#x}"
    );

    if flags & !(MREMAP_MAYMOVE | MREMAP_FIXED | MREMAP_DONTUNMAP) != 0 {
        return Err(AxError::InvalidInput);
    }
    // Linux 4.17+ shrink-with-pages-retained; VMA model not implemented yet (issue-265).
    if flags & MREMAP_DONTUNMAP != 0 {
        return Err(AxError::OperationNotSupported);
    }

    if !addr.is_multiple_of(PageSize::Size4K as usize) {
        return Err(AxError::InvalidInput);
    }
    let addr = VirtAddr::from(addr);

    let curr = current();
    let proc_aspace = curr.as_thread().proc_data.aspace.clone();
    let old_size = align_up_4k(old_size);
    let new_size = align_up_4k(new_size);

    let mut aspace = proc_aspace.write();

    if flags & MREMAP_FIXED != 0 {
        // Linux: MREMAP_FIXED requires MREMAP_MAYMOVE; fifth argument is the target address.
        if flags & MREMAP_MAYMOVE == 0 {
            return Err(AxError::InvalidInput);
        }
        if !new_addr.is_multiple_of(PageSize::Size4K as usize) {
            return Err(AxError::InvalidInput);
        }
        let new_addr = VirtAddr::from(new_addr);
        let out = aspace.mremap_fixed(&proc_aspace, addr, old_size, new_size, new_addr)?;
        return Ok(out.as_usize() as isize);
    }

    let maymove = flags & MREMAP_MAYMOVE != 0;
    let out = aspace.mremap(&proc_aspace, addr, old_size, new_size, maymove)?;
    Ok(out.as_usize() as isize)
}

/// `linux/uapi` `MADV_*` values Linux accepts as valid `advice` to `madvise(2)`; other integers
/// (e.g. `0xdeadbeef`) return `EINVAL`.
const KNOWN_MADV_ADVICE: &[u32] = &[
    MADV_COLD,
    MADV_COLLAPSE,
    MADV_DODUMP,
    MADV_DOFORK,
    MADV_DONTDUMP,
    MADV_DONTFORK,
    MADV_DONTNEED,
    MADV_DONTNEED_LOCKED,
    MADV_FREE,
    MADV_GUARD_INSTALL,
    MADV_GUARD_REMOVE,
    MADV_HUGEPAGE,
    MADV_HWPOISON,
    MADV_KEEPONFORK,
    MADV_MERGEABLE,
    MADV_NOHUGEPAGE,
    MADV_NORMAL,
    MADV_PAGEOUT,
    MADV_POPULATE_READ,
    MADV_POPULATE_WRITE,
    MADV_RANDOM,
    MADV_REMOVE,
    MADV_SEQUENTIAL,
    MADV_SOFT_OFFLINE,
    MADV_UNMERGEABLE,
    MADV_WILLNEED,
    MADV_WIPEONFORK,
];

pub fn sys_madvise(addr: usize, length: usize, advice: i32) -> AxResult<isize> {
    debug!("sys_madvise <= addr: {addr:#x}, length: {length:x}, advice: {advice:#x}");
    let start = VirtAddr::from(addr);
    // Linux/POSIX: `addr` must be page-aligned (issue-270).
    if !start.is_aligned_4k() {
        return Err(AxError::InvalidInput);
    }
    let length = align_up_4k(length);

    let a = advice as u32;
    if !KNOWN_MADV_ADVICE.contains(&a) {
        return Err(AxError::InvalidInput);
    }

    match a {
        MADV_DONTNEED | MADV_FREE => {
            let curr = current();
            let mut aspace = curr.as_thread().proc_data.aspace.write();
            aspace.madvise_dontneed(start, length)?;
        }
        MADV_WILLNEED | MADV_POPULATE_READ => {
            let curr = current();
            let mut aspace = curr.as_thread().proc_data.aspace.write();
            aspace.populate_area(start, length, MappingFlags::READ)?;
        }
        MADV_POPULATE_WRITE => {
            let curr = current();
            let mut aspace = curr.as_thread().proc_data.aspace.write();
            aspace.populate_area(start, length, MappingFlags::READ | MappingFlags::WRITE)?;
        }
        // Pure locality hints; no Starry pager policy yet.
        MADV_NORMAL | MADV_RANDOM | MADV_SEQUENTIAL => {}
        _ => return Err(AxError::OperationNotSupported),
    }
    Ok(0)
}

pub fn sys_msync(addr: usize, length: usize, flags: u32) -> AxResult<isize> {
    debug!("sys_msync <= addr: {addr:#x}, length: {length:x}, flags: {flags:#x}");

    if length == 0 {
        return Err(AxError::InvalidInput);
    }
    let has_async = flags & MS_ASYNC != 0;
    let has_sync = flags & MS_SYNC != 0;
    if has_async == has_sync {
        return Err(AxError::InvalidInput);
    }
    if flags & !(MS_ASYNC | MS_SYNC | MS_INVALIDATE) != 0 {
        return Err(AxError::InvalidInput);
    }

    let start = VirtAddr::from(addr);
    // Linux/POSIX: `addr` must be page-aligned (issue-267).
    if !start.is_aligned_4k() {
        return Err(AxError::InvalidInput);
    }
    let length = align_up_4k(length);
    let curr = current();
    let aspace = curr.as_thread().proc_data.aspace.read();
    if !aspace.contains_range(start, length)
        || !aspace.can_access_range(start, length, MappingFlags::READ)
    {
        return Err(AxError::NoMemory);
    }
    let range = VirtAddrRange::from_start_size(start, length);
    aspace.msync_file_mappings(range)?;
    Ok(0)
}

pub fn sys_mlock(addr: usize, length: usize) -> AxResult<isize> {
    sys_mlock2(addr, length, 0)
}

pub fn sys_mlock2(addr: usize, length: usize, flags: u32) -> AxResult<isize> {
    const MLOCK_ONFAULT: u32 = 1;
    if flags & !MLOCK_ONFAULT != 0 {
        return Err(AxError::InvalidInput);
    }
    if length == 0 {
        return Err(AxError::InvalidInput);
    }
    let start = VirtAddr::from(addr);
    // Linux/POSIX: `addr` must be page-aligned (issue-271).
    if !start.is_aligned_4k() {
        return Err(AxError::InvalidInput);
    }
    let length = align_up_4k(length);
    let curr = current();
    let aspace = curr.as_thread().proc_data.aspace.read();
    if !aspace.contains_range(start, length)
        || !aspace.can_access_range(start, length, MappingFlags::READ)
    {
        return Err(AxError::NoMemory);
    }
    Ok(0)
}
