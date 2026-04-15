use alloc::{sync::Arc, vec, vec::Vec};
use core::mem::size_of;

use axerrno::{AxError, AxResult};
use linux_raw_sys::net::{
    SCM_CREDENTIALS, SCM_RIGHTS, SCM_SECURITY, SCM_TIMESTAMP, SCM_TIMESTAMPING, SCM_TIMESTAMPNS,
    SOL_SOCKET, cmsghdr,
};

use crate::{
    file::{FileLike, get_file_like},
    mm::{UserConstPtr, UserPtr},
};

/// Linux `CMSG_ALIGN(len)`: round up so the next `cmsghdr` is aligned to `sizeof(size_t)`.
///
/// Returns [`AxError::InvalidInput`] when `len + align - 1` would overflow `usize` (no wrapping).
#[inline]
pub fn cmsg_align(len: usize) -> AxResult<usize> {
    let align = size_of::<usize>();
    len.checked_add(align - 1)
        .map(|x| x & !(align - 1))
        .ok_or(AxError::InvalidInput)
}

pub enum CMsg {
    Rights { fds: Vec<Arc<dyn FileLike>> },
}
impl CMsg {
    /// Parse one ancillary message. `hdr` must be a **kernel snapshot** of the user `cmsghdr`
    /// (see `sendmsg`); `cmsg_user_ptr` is the user address of that header so payload bytes are
    /// read from the correct mapping (do not derive payload pointer from `hdr`'s stack address).
    pub fn parse(hdr: &cmsghdr, cmsg_user_ptr: usize) -> AxResult<Self> {
        if hdr.cmsg_len < size_of::<cmsghdr>() {
            return Err(AxError::InvalidInput);
        }

        let payload_len = hdr.cmsg_len - size_of::<cmsghdr>();
        let data = UserConstPtr::<u8>::from(cmsg_user_ptr + size_of::<cmsghdr>())
            .get_as_slice(payload_len)?;
        match (hdr.cmsg_level as u32, hdr.cmsg_type as u32) {
            (SOL_SOCKET, SCM_RIGHTS) => {
                if data.len() % size_of::<i32>() != 0 {
                    return Err(AxError::InvalidInput);
                }
                let mut fds = Vec::new();
                for fd in data.chunks_exact(size_of::<i32>()) {
                    let fd = i32::from_ne_bytes(fd.try_into().unwrap());
                    if fd < 0 {
                        return Err(AxError::BadFileDescriptor);
                    }
                    let f = get_file_like(fd)?;
                    fds.push(f);
                }
                Ok(Self::Rights { fds })
            }
            // Known `SOL_SOCKET` control messages (`unix(7)`) not implemented; do not use EINVAL.
            (SOL_SOCKET, SCM_CREDENTIALS)
            | (SOL_SOCKET, SCM_TIMESTAMP)
            | (SOL_SOCKET, SCM_TIMESTAMPNS)
            | (SOL_SOCKET, SCM_TIMESTAMPING)
            | (SOL_SOCKET, SCM_SECURITY) => Err(AxError::Unsupported),
            _ => Err(AxError::InvalidInput),
        }
    }
}

pub struct CMsgBuilder {
    hdr: UserPtr<cmsghdr>,
    controllen: UserPtr<usize>,
    capacity: usize,
    /// Ancillary bytes written; user `msg_controllen` is updated only in [`Self::commit`] after `recv` succeeds.
    written: usize,
}

impl CMsgBuilder {
    pub fn new(msg: UserPtr<cmsghdr>, controllen: UserPtr<usize>) -> AxResult<Self> {
        let capacity = *controllen.get_as_mut()?;
        Ok(Self {
            hdr: msg,
            controllen,
            capacity,
            written: 0,
        })
    }

    /// After a successful `recvmsg`, write the final ancillary length to user `msg_controllen`.
    #[inline]
    pub fn commit(self) -> AxResult<()> {
        *self.controllen.get_as_mut()? = self.written;
        Ok(())
    }

    /// Bytes still available in the user control buffer (from `msg_controllen` capacity).
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.written)
    }

    /// Build one ancillary message. The `body` closure fills the **payload** slice and returns the
    /// byte length written (≤ slice length). Payload is staged in a kernel buffer first; the user
    /// `cmsghdr` (`cmsg_len` / `cmsg_level` / `cmsg_type`) and payload are committed only after
    /// `body` succeeds, so a failing `body` (e.g. `SCM_RIGHTS` install) does not leave a partially
    /// filled `cmsghdr` in user memory (issue-362; Linux-style atomicity for this path).
    pub fn push(
        &mut self,
        level: u32,
        ty: u32,
        body: impl FnOnce(&mut [u8]) -> AxResult<usize>,
    ) -> AxResult<bool> {
        let remaining = self.capacity.saturating_sub(self.written);
        if remaining < size_of::<cmsghdr>() {
            return Ok(false);
        }
        let body_capacity = remaining - size_of::<cmsghdr>();
        let cmsg_base = self.hdr.address().as_usize();

        let mut kbuf = vec![0u8; body_capacity];
        let body_len = body(&mut kbuf)?;

        let cmsg_len = size_of::<cmsghdr>() + body_len;
        let padded = cmsg_align(cmsg_len)?;
        if padded > remaining {
            return Err(AxError::InvalidInput);
        }

        let hdr = self.hdr.get_as_mut()?;
        hdr.cmsg_len = cmsg_len;
        hdr.cmsg_level = level as _;
        hdr.cmsg_type = ty as _;

        UserPtr::<u8>::from(cmsg_base + size_of::<cmsghdr>())
            .get_as_mut_slice(body_len)?
            .copy_from_slice(&kbuf[..body_len]);

        if padded > cmsg_len {
            UserPtr::<u8>::from(cmsg_base + cmsg_len)
                .get_as_mut_slice(padded - cmsg_len)?
                .fill(0);
        }
        self.hdr = UserPtr::from(cmsg_base + padded);
        self.written += padded;
        Ok(true)
    }
}
