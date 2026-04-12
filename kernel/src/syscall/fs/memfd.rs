use alloc::format;
use core::ffi::c_char;

use axerrno::{AxError, AxResult};
use axfs::{FS_CONTEXT, OpenOptions};
use linux_raw_sys::general::{
    MFD_ALLOW_SEALING, MFD_CLOEXEC, MFD_EXEC, MFD_HUGE_16GB, MFD_HUGE_16MB, MFD_HUGE_1GB,
    MFD_HUGE_1MB, MFD_HUGE_256MB, MFD_HUGE_2GB, MFD_HUGE_2MB, MFD_HUGE_32MB, MFD_HUGE_512KB,
    MFD_HUGE_512MB, MFD_HUGE_64KB, MFD_HUGE_8MB, MFD_HUGE_MASK, MFD_HUGE_SHIFT, MFD_HUGETLB,
    MFD_NOEXEC_SEAL,
};

use crate::{
    file::{File, FileLike},
    mm::UserConstPtr,
};

// TODO: correct memfd implementation

/// Linux `memfd_create(2)`: base flags plus at most one valid `MFD_HUGE_*` encoding in the
/// `MFD_HUGE_MASK << MFD_HUGE_SHIFT` field (see `linux/uapi/linux/memfd.h`). A full huge-tlb
/// field mask would incorrectly accept invalid encodings such as `0x80000000`.
fn validate_memfd_flags(flags: u32) -> AxResult<()> {
    const BASE: u32 = MFD_CLOEXEC | MFD_ALLOW_SEALING | MFD_HUGETLB | MFD_EXEC | MFD_NOEXEC_SEAL;
    let huge_field = MFD_HUGE_MASK << MFD_HUGE_SHIFT;
    let base = flags & !huge_field;
    let huge = flags & huge_field;
    if base & !BASE != 0 {
        return Err(AxError::InvalidInput);
    }
    if huge == 0 {
        return Ok(());
    }
    const VALID_HUGE: &[u32] = &[
        MFD_HUGE_64KB,
        MFD_HUGE_512KB,
        MFD_HUGE_1MB,
        MFD_HUGE_2MB,
        MFD_HUGE_8MB,
        MFD_HUGE_16MB,
        MFD_HUGE_32MB,
        MFD_HUGE_256MB,
        MFD_HUGE_512MB,
        MFD_HUGE_1GB,
        MFD_HUGE_2GB,
        MFD_HUGE_16GB,
    ];
    if !VALID_HUGE.contains(&huge) {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

pub fn sys_memfd_create(_name: UserConstPtr<c_char>, flags: u32) -> AxResult<isize> {
    validate_memfd_flags(flags)?;
    // This is cursed
    for id in 0..0xffff {
        let name = format!("/tmp/memfd-{id:04x}");
        let fs = FS_CONTEXT.lock().clone();
        if fs.resolve(&name).is_err() {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&fs, &name)?
                .into_file()?;
            let cloexec = flags & MFD_CLOEXEC != 0;
            return File::new(file).add_to_fd_table(cloexec).map(|fd| fd as _);
        }
    }
    Err(AxError::TooManyOpenFiles)
}

/// `memfd_secret(2)`: same backing as `memfd_create` here so `/proc/self/fd/N` is a real file path, not dummy.
pub fn sys_memfd_secret(name: UserConstPtr<c_char>, flags: u32) -> AxResult<isize> {
    sys_memfd_create(name, flags)
}
