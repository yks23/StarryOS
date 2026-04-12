//! User address space management.

use alloc::{borrow::ToOwned, string::String, vec, vec::Vec};
use core::{ffi::CStr, iter};

use axerrno::{AxError, AxResult};
use axfs::{CachedFile, FS_CONTEXT, FileBackend};
use axfs_ng_vfs::Location;
use axhal::{
    mem::virt_to_phys,
    paging::{MappingFlags, PageSize},
};
use kernel_elf_parser::{AuxEntry, ELFHeaders, ELFHeadersBuilder, ELFParser, app_stack_region};
use memory_addr::{MemoryAddr, PAGE_SIZE_4K, VirtAddr};
use ouroboros::self_referencing;
use spin::RwLock;
use uluru::LRUCache;

use crate::{
    config::{USER_SPACE_BASE, USER_SPACE_SIZE},
    mm::aspace::{AddrSpace, Backend},
};

/// Creates a new empty user address space.
pub fn new_user_aspace_empty() -> AxResult<AddrSpace> {
    AddrSpace::new_empty(VirtAddr::from_usize(USER_SPACE_BASE), USER_SPACE_SIZE)
}

/// If the target architecture requires it, the kernel portion of the address
/// space will be copied to the user address space.
pub fn copy_from_kernel(_aspace: &mut AddrSpace) -> AxResult {
    #[cfg(not(any(target_arch = "aarch64", target_arch = "loongarch64")))]
    {
        // ARMv8 (aarch64) and LoongArch64 use separate page tables for user space
        // (aarch64: TTBR0_EL1, LoongArch64: PGDL), so there is no need to copy the
        // kernel portion to the user page table.
        let kspace = axmm::kernel_aspace().lock();
        _aspace.page_table_mut().cursor().copy_from(
            kspace.page_table(),
            kspace.base(),
            kspace.size(),
        );
    }
    Ok(())
}

/// Map the signal trampoline to the user address space.
pub fn map_trampoline(aspace: &mut AddrSpace) -> AxResult {
    let signal_trampoline_paddr =
        virt_to_phys(starry_signal::arch::signal_trampoline_address().into());
    aspace.map_linear(
        crate::config::SIGNAL_TRAMPOLINE.into(),
        signal_trampoline_paddr,
        PAGE_SIZE_4K,
        MappingFlags::READ | MappingFlags::EXECUTE | MappingFlags::USER,
    )?;
    Ok(())
}

fn mapping_flags(flags: xmas_elf::program::Flags) -> MappingFlags {
    let mut mapping_flags = MappingFlags::USER;
    if flags.is_read() {
        mapping_flags |= MappingFlags::READ;
    }
    if flags.is_write() {
        mapping_flags |= MappingFlags::WRITE;
    }
    if flags.is_execute() {
        mapping_flags |= MappingFlags::EXECUTE;
    }
    mapping_flags
}

/// Map the elf file to the user address space.
///
/// # Arguments
/// - `uspace`: The address space of the user app.
/// - `elf`: The elf file.
///
/// # Returns
/// - The entry point of the user app.
fn map_elf<'a>(
    uspace: &mut AddrSpace,
    base: usize,
    entry: &'a ElfCacheEntry,
) -> AxResult<ELFParser<'a>> {
    let elf_parser = ELFParser::new(entry.borrow_elf(), base).map_err(|_| AxError::InvalidData)?;
    let cache = entry.borrow_cache();

    for ph in elf_parser
        .headers()
        .ph
        .iter()
        .filter(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Load))
    {
        let vaddr = ph.virtual_addr as usize + elf_parser.base();
        debug!(
            "Mapping ELF segment: [{:#x?}, {:#x?}) flags: {}",
            vaddr,
            vaddr + ph.mem_size as usize,
            ph.flags
        );
        let seg_pad = vaddr.align_offset_4k();
        assert_eq!(seg_pad, ph.offset as usize % PAGE_SIZE_4K);

        let seg_align_size =
            (ph.mem_size as usize + seg_pad + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1);
        let seg_start = VirtAddr::from_usize(vaddr);

        // Note that `offset` might not be aligned to 4K here, and it's
        // backend's responsibility to properly handle it.
        let backend = Backend::new_cow(
            seg_start,
            PageSize::Size4K,
            FileBackend::Cached(cache.clone()),
            ph.offset,
            Some(ph.offset + ph.file_size),
        );
        uspace.map(
            seg_start.align_down_4k(),
            seg_align_size,
            mapping_flags(ph.flags),
            false,
            backend,
        )?;

        // TDOO: flush the I-cache
    }

    Ok(elf_parser)
}

fn map_elf_error(err: &'static str) -> AxError {
    debug!("Failed to parse ELF file: {err}");
    AxError::InvalidExecutable
}

#[self_referencing]
struct ElfCacheEntry {
    cache: CachedFile,
    data: Vec<u8>,
    #[borrows(data)]
    #[covariant]
    elf: ELFHeaders<'this>,
}

impl ElfCacheEntry {
    fn load(loc: Location) -> AxResult<Result<Self, Vec<u8>>> {
        let cache = CachedFile::get_or_create(loc);

        let mut data = vec![0; 4096];
        let read = cache.read_at(&mut data[..], 0)?;
        data.truncate(read);
        match ElfCacheEntry::try_new_or_recover::<AxError>(cache.clone(), data, |data| {
            let builder = ELFHeadersBuilder::new(data).map_err(map_elf_error)?;
            let range = builder.ph_range();
            if range.end as usize <= data.len() {
                builder.build(&data[range.start as usize..range.end as usize])
            } else {
                let mut buf = vec![0; (range.end - range.start) as usize];
                cache.read_at(&mut buf[..], range.start)?;
                builder.build(&buf)
            }
            .map_err(map_elf_error)
        }) {
            Ok(e) => Ok(Ok(e)),
            Err((_, heads)) => Ok(Err(heads.data)),
        }
    }
}

struct ElfLoader(LRUCache<ElfCacheEntry, 32>);

impl ElfLoader {
    const fn new() -> Self {
        Self(LRUCache::new())
    }

    fn lookup_entry(&self, loc: &Location) -> Option<&ElfCacheEntry> {
        self.0
            .iter()
            .find(|e| e.borrow_cache().location().ptr_eq(loc))
    }
}

/// Ensure `loc` is present in the ELF LRU. File read happens **without** holding the ELF lock.
fn ensure_elf_cached(loc: Location) -> AxResult<Result<(), Vec<u8>>> {
    {
        let cache = ELF_LOADER.read();
        if cache.lookup_entry(&loc).is_some() {
            return Ok(Ok(()));
        }
    }
    match ElfCacheEntry::load(loc.clone())? {
        Ok(entry) => {
            let mut cache = ELF_LOADER.write();
            if cache.lookup_entry(&loc).is_none() {
                cache.0.insert(entry);
            }
            Ok(Ok(()))
        }
        Err(data) => Ok(Err(data)),
    }
}

fn ensure_elf_cached_executable(loc: Location) -> AxResult<()> {
    match ensure_elf_cached(loc)? {
        Ok(()) => Ok(()),
        Err(_) => Err(AxError::InvalidInput),
    }
}

fn map_cached_elf_into_uspace(
    uspace: &mut AddrSpace,
    loc: &Location,
) -> AxResult<(VirtAddr, Vec<AuxEntry>)> {
    let ldso_loc_opt: Option<Location> = {
        let cache = ELF_LOADER.read();
        let main_entry = cache.lookup_entry(loc).ok_or(AxError::InvalidData)?;
        if let Some(header) = main_entry
            .borrow_elf()
            .ph
            .iter()
            .find(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Interp))
        {
            let cache_file = main_entry.borrow_cache();
            let mut data = vec![0; header.file_size as usize];
            let read = cache_file.read_at(&mut data[..], header.offset)?;
            assert_eq!(data.len(), read);

            let ldso = CStr::from_bytes_with_nul(&data)
                .ok()
                .and_then(|cstr| cstr.to_str().ok())
                .ok_or(AxError::InvalidInput)?;
            debug!("Loading dynamic linker: {ldso}");
            Some(FS_CONTEXT.lock().resolve(ldso)?)
        } else {
            None
        }
    };

    if let Some(ref ldso_loc) = ldso_loc_opt {
        ensure_elf_cached_executable(ldso_loc.clone())?;
    }

    let cache = ELF_LOADER.read();
    let main_entry = cache.lookup_entry(loc).ok_or(AxError::InvalidData)?;
    let ldso_entry = ldso_loc_opt.as_ref().and_then(|l| cache.lookup_entry(l));

    uspace.clear();
    map_trampoline(uspace)?;

    let elf = map_elf(uspace, crate::config::USER_SPACE_BASE, main_entry)?;
    let ldso = ldso_entry
        .map(|e| map_elf(uspace, crate::config::USER_INTERP_BASE, e))
        .transpose()?;

    let entry = VirtAddr::from_usize(
        ldso.as_ref()
            .map_or_else(|| elf.entry(), |ldso| ldso.entry()),
    );
    let auxv = elf
        .aux_vector(PAGE_SIZE_4K, ldso.map(|elf| elf.base()))
        .collect::<Vec<_>>();

    Ok((entry, auxv))
}

static ELF_LOADER: RwLock<ElfLoader> = RwLock::new(ElfLoader::new());

/// Clear the ELF cache.
///
/// Useful for removing noises during memory leak detect.
pub fn clear_elf_cache() {
    ELF_LOADER.write().0.clear();
}

/// Load the user app to the user address space.
///
/// # Arguments
/// - `uspace`: The address space of the user app.
/// - `args`: The arguments of the user app. The first argument is the path of
///   the user app.
/// - `envs`: The environment variables of the user app.
///
/// # Returns
/// - The entry point of the user app.
/// - The stack pointer of the user app.
pub fn load_user_app(
    uspace: &mut AddrSpace,
    path: Option<&str>,
    args: &[String],
    envs: &[String],
) -> AxResult<(VirtAddr, VirtAddr)> {
    let path = path
        .or_else(|| args.first().map(String::as_str))
        .ok_or(AxError::InvalidInput)?;

    // FIXME: impl `/proc/self/exe` to let busybox retry running
    if path.ends_with(".sh") {
        let new_args: Vec<String> = iter::once("/bin/sh".to_owned())
            .chain(args.iter().cloned())
            .collect();
        return load_user_app(uspace, None, &new_args, envs);
    }

    let loc = FS_CONTEXT.lock().resolve(path)?;
    let (entry, auxv) = match ensure_elf_cached(loc.clone())? {
        Ok(()) => map_cached_elf_into_uspace(uspace, &loc)?,
        Err(data) => {
            if data.starts_with(b"#!") {
                let head = &data[2..data.len().min(256)];
                let pos = head.iter().position(|c| *c == b'\n').unwrap_or(head.len());
                let line = core::str::from_utf8(&head[..pos]).map_err(|_| AxError::InvalidInput)?;

                let new_args: Vec<String> = line
                    .trim()
                    .splitn(2, |c: char| c.is_ascii_whitespace())
                    .map(|s| s.trim_ascii().to_owned())
                    .chain(iter::once(path.to_owned()))
                    .chain(args.iter().skip(1).cloned())
                    .collect();
                return load_user_app(uspace, None, &new_args, envs);
            }
            return Err(AxError::InvalidExecutable);
        }
    };

    let ustack_top = VirtAddr::from_usize(crate::config::USER_STACK_TOP);
    let ustack_size = crate::config::USER_STACK_SIZE;
    let ustack_start = ustack_top - ustack_size;
    debug!("Mapping user stack: {ustack_start:#x?} -> {ustack_top:#x?}");

    uspace.map(
        ustack_start,
        ustack_size,
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
        false,
        Backend::new_alloc(ustack_start, PageSize::Size4K),
    )?;

    let stack_data = app_stack_region(args, envs, &auxv, ustack_top.into());
    let user_sp = ustack_top - stack_data.len();
    let user_sp_aligned = user_sp.align_down_4k();
    uspace.populate_area(
        user_sp_aligned,
        (ustack_top - user_sp_aligned).align_up_4k(),
        MappingFlags::READ | MappingFlags::WRITE,
    )?;
    uspace.write(user_sp, stack_data.as_slice())?;

    let heap_start = VirtAddr::from_usize(crate::config::USER_HEAP_BASE);
    let heap_size = crate::config::USER_HEAP_SIZE;
    uspace.map(
        heap_start,
        heap_size,
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
        true,
        Backend::new_alloc(heap_start, PageSize::Size4K),
    )?;

    Ok((entry, user_sp))
}
