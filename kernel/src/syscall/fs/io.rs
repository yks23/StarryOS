use alloc::{borrow::Cow, sync::Arc, vec};
use core::{
    ffi::{c_char, c_int},
    task::Context,
};

use axerrno::{AxError, AxResult, LinuxError};
use axfs::{FS_CONTEXT, FileFlags, OpenOptions};
use axfs_ng_vfs::NodeType;
use axio::{Seek, SeekFrom};
use axpoll::{IoEvents, Pollable};
use linux_raw_sys::general::{
    __kernel_off_t, FALLOC_FL_COLLAPSE_RANGE, FALLOC_FL_INSERT_RANGE, FALLOC_FL_KEEP_SIZE,
    FALLOC_FL_NO_HIDE_STALE, FALLOC_FL_PUNCH_HOLE, FALLOC_FL_UNSHARE_RANGE,
    FALLOC_FL_WRITE_ZEROES, FALLOC_FL_ZERO_RANGE, RWF_APPEND, RWF_DSYNC, RWF_HIPRI, RWF_NOWAIT,
    RWF_SYNC, SPLICE_F_GIFT, SPLICE_F_MORE, SPLICE_F_MOVE, SPLICE_F_NONBLOCK,
};

/// Linux `splice(2)` flags; unknown bits must be rejected with EINVAL.
const SPLICE_F_MASK: u32 = SPLICE_F_MOVE | SPLICE_F_NONBLOCK | SPLICE_F_MORE | SPLICE_F_GIFT;

/// Linux `copy_file_range(2)` flags (`uapi/linux/fs.h`); unknown bits → EINVAL.
const COPY_FILE_RANGE_COMPRESS: u32 = 1 << 0;
const COPY_FILE_RANGE_DEDUPE: u32 = 1 << 2;
const COPY_FILE_RANGE_MASK: u32 = COPY_FILE_RANGE_COMPRESS | COPY_FILE_RANGE_DEDUPE;

/// Linux `preadv2(2)` / `pwritev2(2)` `RWF_*` (`uapi/linux/fs.h`, via `linux_raw_sys::general`);
/// unknown bits → EINVAL; defined bits not yet honored → EOPNOTSUPP (issue-248).
const RWF_MASK: u32 = RWF_HIPRI | RWF_DSYNC | RWF_SYNC | RWF_NOWAIT | RWF_APPEND;

/// Linux `POSIX_FADV_*` through `POSIX_FADV_WIPEONFORK` (`uapi/linux/fadvise.h`, values 0..=7).
/// Larger `advice` values are EINVAL until extended in the uapi.
const POSIX_FADV_MAX: u32 = 7;

/// Linux `fallocate(2)` `FALLOC_FL_*` bits (`uapi/linux/fs.h` / `linux_raw_sys`); unknown bits → EINVAL.
/// `FALLOC_FL_ALLOCATE_RANGE` is **0** (default); non-zero modes are not implemented yet (issue-225).
const FALLOC_FL_KNOWN_MASK: u32 = FALLOC_FL_KEEP_SIZE
    | FALLOC_FL_PUNCH_HOLE
    | FALLOC_FL_NO_HIDE_STALE
    | FALLOC_FL_COLLAPSE_RANGE
    | FALLOC_FL_ZERO_RANGE
    | FALLOC_FL_INSERT_RANGE
    | FALLOC_FL_UNSHARE_RANGE
    | FALLOC_FL_WRITE_ZEROES;

/// `lseek(2)` / `llseek` — not always exposed next to `SEEK_SET` in all libc headers.
const SEEK_SET: c_int = 0;
const SEEK_DATA: c_int = 3;
const SEEK_HOLE: c_int = 4;

/// Linux `SEEK_DATA` / `SEEK_HOLE` for a **dense** file (no tracked internal holes).
///
/// Starry does not yet expose per-file extent/hole maps (e.g. after `FALLOC_FL_PUNCH_HOLE`);
/// we treat `[0, size)` as data and `[size, ∞)` as hole, matching Linux for non-sparse files.
fn lseek_data_hole_dense(f: &File, offset: __kernel_off_t, seek_data: bool) -> AxResult<u64> {
    let size = f.inner().location().metadata()?.size;
    let off = offset;
    if off < 0 {
        return Err(AxError::InvalidInput);
    }
    let off_u = off as u64;
    if seek_data {
        if off_u < size {
            Ok(off_u)
        } else if off_u == size {
            Err(LinuxError::ENXIO.into())
        } else {
            Err(AxError::InvalidInput)
        }
    } else if off_u < size {
        Ok(size)
    } else {
        Ok(off_u)
    }
}
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    file::{File, FileLike, MemfdCreatedFile, Pipe, get_file_like},
    mm::{IoVec, IoVectorBuf, UserConstPtr, VmBytes, VmBytesMut},
};

struct DummyFd;
impl FileLike for DummyFd {
    fn path(&self) -> Cow<'_, str> {
        "anon_inode:[dummy]".into()
    }
}
impl Pollable for DummyFd {
    fn poll(&self) -> IoEvents {
        IoEvents::empty()
    }

    fn register(&self, _context: &mut Context<'_>, _events: IoEvents) {}
}

/// Read data from the file indicated by `fd`.
///
/// Return the read size if success.
pub fn sys_read(fd: i32, buf: *mut u8, len: usize) -> AxResult<isize> {
    debug!("sys_read <= fd: {fd}, buf: {buf:p}, len: {len}");
    Ok(get_file_like(fd)?.read(&mut VmBytesMut::new(buf, len))? as _)
}

pub fn sys_readv(fd: i32, iov: *const IoVec, iovcnt: usize) -> AxResult<isize> {
    debug!("sys_readv <= fd: {fd}, iovcnt: {iovcnt}");
    let f = get_file_like(fd)?;
    f.read(&mut IoVectorBuf::new(iov, iovcnt)?.into_io())
        .map(|n| n as _)
}

/// Write data to the file indicated by `fd`.
///
/// Return the written size if success.
pub fn sys_write(fd: i32, buf: *mut u8, len: usize) -> AxResult<isize> {
    debug!("sys_write <= fd: {fd}, buf: {buf:p}, len: {len}");
    Ok(get_file_like(fd)?.write(&mut VmBytes::new(buf, len))? as _)
}

pub fn sys_writev(fd: i32, iov: *const IoVec, iovcnt: usize) -> AxResult<isize> {
    debug!("sys_writev <= fd: {fd}, iovcnt: {iovcnt}");
    let f = get_file_like(fd)?;
    f.write(&mut IoVectorBuf::new(iov, iovcnt)?.into_io())
        .map(|n| n as _)
}

pub fn sys_lseek(fd: c_int, offset: __kernel_off_t, whence: c_int) -> AxResult<isize> {
    debug!("sys_lseek <= {fd} {offset} {whence}");
    let f = File::from_fd(fd)?;
    if whence == SEEK_DATA || whence == SEEK_HOLE {
        let pos = lseek_data_hole_dense(&f, offset, whence == SEEK_DATA)?;
        let off = f.inner().seek(SeekFrom::Start(pos))?;
        return Ok(off as _);
    }
    // Linux: SEEK_SET with negative offset → EINVAL (avoid `offset as u64` wrap, issue-272).
    if whence == SEEK_SET && offset < 0 {
        return Err(AxError::InvalidInput);
    }
    let pos = match whence {
        SEEK_SET => SeekFrom::Start(offset as _),
        1 => SeekFrom::Current(offset as _),
        2 => SeekFrom::End(offset as _),
        _ => return Err(AxError::InvalidInput),
    };
    let off = f.inner().seek(pos)?;
    Ok(off as _)
}

pub fn sys_truncate(path: UserConstPtr<c_char>, length: __kernel_off_t) -> AxResult<isize> {
    if length < 0 {
        return Err(AxError::InvalidInput);
    }
    let path = path.get_as_str()?;
    debug!("sys_truncate <= {path:?} {length}");
    let file = OpenOptions::new()
        .write(true)
        .open(&FS_CONTEXT.lock(), path)?
        .into_file()?;
    file.access(FileFlags::WRITE)?.set_len(length as _)?;
    Ok(0)
}

pub fn sys_ftruncate(fd: c_int, length: __kernel_off_t) -> AxResult<isize> {
    debug!("sys_ftruncate <= {fd} {length}");
    if length < 0 {
        return Err(AxError::InvalidInput);
    }
    let f = File::from_fd(fd)?;
    f.inner().access(FileFlags::WRITE)?.set_len(length as _)?;
    Ok(0)
}

/// Default `mode == 0` extends file length to `max(current, offset + len)` (space reservation).
/// Other `FALLOC_FL_*` combinations (punch hole, zero range, …) need sparse/VFS support → `EOPNOTSUPP`.
pub fn sys_fallocate(
    fd: c_int,
    mode: u32,
    offset: __kernel_off_t,
    len: __kernel_off_t,
) -> AxResult<isize> {
    debug!("sys_fallocate <= fd: {fd}, mode: {mode}, offset: {offset}, len: {len}");
    // Resolve `fd` before `mode`/`offset`/`len` so **EBADF** precedes **EINVAL**/**EOPNOTSUPP** when
    // both an invalid fd and bad parameters are present (Linux `fdget`/`__sys_fallocate`; issue-313).
    let f = File::from_fd(fd)?;
    if mode & !FALLOC_FL_KNOWN_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    if mode != 0 {
        return Err(AxError::OperationNotSupported);
    }
    if offset < 0 || len < 0 {
        return Err(AxError::InvalidInput);
    }
    let offset_u = offset as u64;
    let len_u = len as u64;
    let Some(end) = offset_u.checked_add(len_u) else {
        return Err(AxError::InvalidInput);
    };

    let inner = f.inner();
    let file = inner.access(FileFlags::WRITE)?;
    file.set_len(file.location().len()?.max(end))?;
    Ok(0)
}

/// Linux `vfs_fsync`: only regular-file-like descriptors; pipe/socket/etc. → `EINVAL`.
fn fsync_fd(fd: c_int, data_only: bool) -> AxResult<isize> {
    let f = get_file_like(fd)?;
    if let Some(file) = f.downcast_ref::<File>() {
        file.inner().sync(data_only)?;
        return Ok(0);
    }
    if let Some(m) = f.downcast_ref::<MemfdCreatedFile>() {
        m.inner_file().inner().sync(data_only)?;
        return Ok(0);
    }
    Err(AxError::InvalidInput)
}

pub fn sys_fsync(fd: c_int) -> AxResult<isize> {
    debug!("sys_fsync <= {fd}");
    fsync_fd(fd, false)
}

pub fn sys_fdatasync(fd: c_int) -> AxResult<isize> {
    debug!("sys_fdatasync <= {fd}");
    fsync_fd(fd, true)
}

pub fn sys_fadvise64(
    fd: c_int,
    offset: __kernel_off_t,
    len: __kernel_off_t,
    advice: u32,
) -> AxResult<isize> {
    debug!("sys_fadvise64 <= fd: {fd}, offset: {offset}, len: {len}, advice: {advice}");
    let f = get_file_like(fd)?;
    if f.downcast_ref::<Pipe>().is_some() {
        // Linux: fadvise on non-seekable fd (pipe, etc.) → ESPIPE, not EPIPE (broken pipe).
        return Err(LinuxError::ESPIPE.into());
    }
    if f.downcast_ref::<File>().is_none() && f.downcast_ref::<MemfdCreatedFile>().is_none() {
        // Linux `vfs_fadvise`: regular-file-like only; socket/timerfd/epoll/… → EINVAL (issue-290).
        return Err(AxError::InvalidInput);
    }
    if advice > POSIX_FADV_MAX {
        return Err(AxError::InvalidInput);
    }
    // Stub: no VFS hook yet, but reject invalid intervals like Linux `vfs_fadvise` (EINVAL).
    if offset < 0 || len < 0 {
        return Err(AxError::InvalidInput);
    }
    let offset_u = offset as u64;
    let len_u = len as u64;
    if offset_u.checked_add(len_u).is_none() {
        return Err(AxError::InvalidInput);
    }
    Ok(0)
}

pub fn sys_pread64(fd: c_int, buf: *mut u8, len: usize, offset: __kernel_off_t) -> AxResult<isize> {
    let f = File::from_fd(fd)?;
    if offset < 0 {
        return Err(AxError::InvalidInput);
    }
    let read = f.inner().read_at(VmBytesMut::new(buf, len), offset as _)?;
    Ok(read as _)
}

pub fn sys_pwrite64(
    fd: c_int,
    buf: *const u8,
    len: usize,
    offset: __kernel_off_t,
) -> AxResult<isize> {
    if offset < 0 {
        return Err(AxError::InvalidInput);
    }
    let f = File::from_fd(fd)?;
    if len == 0 {
        return Ok(0);
    }
    let write = f.inner().write_at(VmBytes::new(buf, len), offset as _)?;
    Ok(write as _)
}

pub fn sys_preadv(
    fd: c_int,
    iov: *const IoVec,
    iovcnt: usize,
    offset: __kernel_off_t,
) -> AxResult<isize> {
    sys_preadv2(fd, iov, iovcnt, offset, 0)
}

pub fn sys_pwritev(
    fd: c_int,
    iov: *const IoVec,
    iovcnt: usize,
    offset: __kernel_off_t,
) -> AxResult<isize> {
    sys_pwritev2(fd, iov, iovcnt, offset, 0)
}

/// `preadv2`/`pwritev2` `flags` (`RWF_*`): reject unknown bits; defined semantics not implemented yet.
fn check_rwf_flags(flags: u32) -> AxResult<()> {
    if flags & !RWF_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    if flags != 0 {
        return Err(AxError::OperationNotSupported);
    }
    Ok(())
}

pub fn sys_preadv2(
    fd: c_int,
    iov: *const IoVec,
    iovcnt: usize,
    offset: __kernel_off_t,
    flags: u32,
) -> AxResult<isize> {
    debug!("sys_preadv2 <= fd: {fd}, iovcnt: {iovcnt}, offset: {offset}, flags: {flags}");
    check_rwf_flags(flags)?;
    let f = File::from_fd(fd)?;
    if offset < 0 {
        return Err(AxError::InvalidInput);
    }
    f.inner()
        .read_at(IoVectorBuf::new(iov, iovcnt)?.into_io(), offset as _)
        .map(|n| n as _)
}

pub fn sys_pwritev2(
    fd: c_int,
    iov: *const IoVec,
    iovcnt: usize,
    offset: __kernel_off_t,
    flags: u32,
) -> AxResult<isize> {
    debug!("sys_pwritev2 <= fd: {fd}, iovcnt: {iovcnt}, offset: {offset}, flags: {flags}");
    check_rwf_flags(flags)?;
    let f = File::from_fd(fd)?;
    if offset < 0 {
        return Err(AxError::InvalidInput);
    }
    f.inner()
        .write_at(IoVectorBuf::new(iov, iovcnt)?.into_io(), offset as _)
        .map(|n| n as _)
}

enum SendFile {
    Direct(Arc<dyn FileLike>),
    Offset(Arc<File>, *mut u64),
}

impl SendFile {
    fn has_data(&self) -> bool {
        match self {
            SendFile::Direct(file) => file.poll(),
            SendFile::Offset(file, ..) => file.poll(),
        }
        .contains(IoEvents::IN)
    }

    fn read(&mut self, mut buf: &mut [u8]) -> AxResult<usize> {
        match self {
            SendFile::Direct(file) => file.read(&mut buf),
            SendFile::Offset(file, offset) => {
                let off = offset.vm_read()?;
                let bytes_read = file.inner().read_at(&mut buf, off)?;
                offset.vm_write(off + bytes_read as u64)?;
                Ok(bytes_read)
            }
        }
    }

    fn write(&mut self, mut buf: &[u8]) -> AxResult<usize> {
        match self {
            SendFile::Direct(file) => file.write(&mut buf),
            SendFile::Offset(file, offset) => {
                let off = offset.vm_read()?;
                let bytes_written = file.inner().write_at(buf, off)?;
                offset.vm_write(off + bytes_written as u64)?;
                Ok(bytes_written)
            }
        }
    }
}

fn do_send(mut src: SendFile, mut dst: SendFile, len: usize) -> AxResult<usize> {
    let mut buf = vec![0; 0x1000];
    let mut total_written = 0;
    let mut remaining = len;

    while remaining > 0 {
        if total_written > 0 && !src.has_data() {
            break;
        }
        let to_read = buf.len().min(remaining);
        let bytes_read = match src.read(&mut buf[..to_read]) {
            Ok(n) => n,
            Err(AxError::WouldBlock) if total_written > 0 => break,
            Err(e) => return Err(e),
        };
        if bytes_read == 0 {
            break;
        }

        let bytes_written = dst.write(&buf[..bytes_read])?;
        if bytes_written < bytes_read {
            break;
        }

        total_written += bytes_written;
        remaining -= bytes_written;
    }

    Ok(total_written)
}

pub fn sys_sendfile(out_fd: c_int, in_fd: c_int, offset: *mut u64, len: usize) -> AxResult<isize> {
    debug!(
        "sys_sendfile <= out_fd: {}, in_fd: {}, offset: {}, len: {}",
        out_fd,
        in_fd,
        !offset.is_null(),
        len
    );

    // Linux `sendfile(2)`: `in_fd` and `out_fd` must not refer to the same file description; same
    // descriptor → EINVAL.
    if in_fd == out_fd {
        return Err(AxError::InvalidInput);
    }

    let src = if !offset.is_null() {
        // LP64: user `offset` is `loff_t*` / updated `u64` — no 4GiB cap (issue-220); `read_at`/`write_at`
        // enforce any file-size limits.
        let file = File::from_fd(in_fd)?;
        SendFile::Offset(file, offset)
    } else {
        SendFile::Direct(get_file_like(in_fd)?)
    };

    let dst = SendFile::Direct(get_file_like(out_fd)?);

    do_send(src, dst, len).map(|n| n as _)
}

pub fn sys_copy_file_range(
    fd_in: c_int,
    off_in: *mut u64,
    fd_out: c_int,
    off_out: *mut u64,
    len: usize,
    flags: u32,
) -> AxResult<isize> {
    debug!(
        "sys_copy_file_range <= fd_in: {}, off_in: {}, fd_out: {}, off_out: {}, len: {}, flags: {}",
        fd_in,
        !off_in.is_null(),
        fd_out,
        !off_out.is_null(),
        len,
        flags
    );

    if flags & !COPY_FILE_RANGE_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    // `COPY_FILE_RANGE_COMPRESS` / `COPY_FILE_RANGE_DEDUPE` require fs support; do not fall back to
    // plain read/write and pretend success (issue-246).
    if flags != 0 {
        return Err(AxError::OperationNotSupported);
    }

    let f_in = File::from_fd(fd_in)?;
    let f_out = File::from_fd(fd_out)?;
    let mi = f_in.inner().location().metadata()?;
    let mo = f_out.inner().location().metadata()?;
    if mi.node_type != NodeType::RegularFile || mo.node_type != NodeType::RegularFile {
        return Err(AxError::InvalidInput);
    }
    if (fd_in == fd_out || (mi.inode == mo.inode && mi.device == mo.device)) && len > 0 {
        let in_start = if off_in.is_null() {
            f_in.inner().seek(SeekFrom::Current(0))?
        } else {
            off_in.vm_read()?
        };
        let out_start = if off_out.is_null() {
            f_out.inner().seek(SeekFrom::Current(0))?
        } else {
            off_out.vm_read()?
        };
        let in_end = in_start
            .checked_add(len as u64)
            .ok_or(AxError::InvalidInput)?;
        let out_end = out_start
            .checked_add(len as u64)
            .ok_or(AxError::InvalidInput)?;
        if in_start < out_end && out_start < in_end {
            return Err(AxError::InvalidInput);
        }
    }

    let src = if !off_in.is_null() {
        SendFile::Offset(File::from_fd(fd_in)?, off_in)
    } else {
        SendFile::Direct(get_file_like(fd_in)?)
    };

    let dst = if !off_out.is_null() {
        SendFile::Offset(File::from_fd(fd_out)?, off_out)
    } else {
        SendFile::Direct(get_file_like(fd_out)?)
    };

    do_send(src, dst, len).map(|n| n as _)
}

pub fn sys_splice(
    fd_in: c_int,
    off_in: *mut i64,
    fd_out: c_int,
    off_out: *mut i64,
    len: usize,
    flags: u32,
) -> AxResult<isize> {
    debug!(
        "sys_splice <= fd_in: {}, off_in: {}, fd_out: {}, off_out: {}, len: {}, flags: {}",
        fd_in,
        !off_in.is_null(),
        fd_out,
        !off_out.is_null(),
        len,
        flags
    );

    if flags & !SPLICE_F_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    // `SPLICE_F_NONBLOCK`/`MOVE`/`MORE`/`GIFT` are not honored by `do_send` (no EAGAIN from flags
    // alone); reject non-zero flags until splice implements them (issue-247), like optional
    // `copy_file_range` flags (issue-246).
    if flags != 0 {
        return Err(AxError::OperationNotSupported);
    }

    // Linux `splice(2)` / `do_splice`: input and output must not be the same file descriptor → EINVAL.
    if fd_in == fd_out {
        return Err(AxError::InvalidInput);
    }

    let mut has_pipe = false;

    if DummyFd::from_fd(fd_in).is_ok() || DummyFd::from_fd(fd_out).is_ok() {
        return Err(AxError::BadFileDescriptor);
    }

    let src = if !off_in.is_null() {
        let file = File::from_fd(fd_in)?;
        if off_in.vm_read()? < 0 {
            return Err(AxError::InvalidInput);
        }
        SendFile::Offset(file, off_in.cast())
    } else {
        if let Ok(src) = Pipe::from_fd(fd_in) {
            if !src.is_read() {
                return Err(AxError::BadFileDescriptor);
            }
            has_pipe = true;
        }
        if let Ok(file) = File::from_fd(fd_in)
            && file.inner().is_path()
        {
            return Err(AxError::InvalidInput);
        }
        SendFile::Direct(get_file_like(fd_in)?)
    };

    let dst = if !off_out.is_null() {
        let file = File::from_fd(fd_out)?;
        if off_out.vm_read()? < 0 {
            return Err(AxError::InvalidInput);
        }
        SendFile::Offset(file, off_out.cast())
    } else {
        if let Ok(dst) = Pipe::from_fd(fd_out) {
            if !dst.is_write() {
                return Err(AxError::BadFileDescriptor);
            }
            has_pipe = true;
        }
        if let Ok(file) = File::from_fd(fd_out)
            && file.inner().access(FileFlags::APPEND).is_ok()
        {
            return Err(AxError::InvalidInput);
        }
        let f = get_file_like(fd_out)?;
        f.write(&mut b"".as_slice())?;
        SendFile::Direct(f)
    };

    if !has_pipe {
        return Err(AxError::InvalidInput);
    }

    do_send(src, dst, len).map(|n| n as _)
}
