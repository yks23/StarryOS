use alloc::{sync::Arc, vec::Vec};
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
#[inline]
pub fn cmsg_align(len: usize) -> usize {
    let align = size_of::<usize>();
    (len + align - 1) & !(align - 1)
}

pub enum CMsg {
    Rights { fds: Vec<Arc<dyn FileLike>> },
}
impl CMsg {
    pub fn parse(hdr: &cmsghdr) -> AxResult<Self> {
        if hdr.cmsg_len < size_of::<cmsghdr>() {
            return Err(AxError::InvalidInput);
        }

        let data =
            UserConstPtr::<u8>::from((hdr as *const cmsghdr as usize) + size_of::<cmsghdr>())
                .get_as_slice(hdr.cmsg_len - size_of::<cmsghdr>())?;
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

pub struct CMsgBuilder<'a> {
    hdr: UserPtr<cmsghdr>,
    len: &'a mut usize,
    capacity: usize,
}
impl<'a> CMsgBuilder<'a> {
    pub fn new(msg: UserPtr<cmsghdr>, len: &'a mut usize) -> Self {
        let capacity = *len;
        *len = 0;
        Self {
            hdr: msg,
            len,
            capacity,
        }
    }

    pub fn push(
        &mut self,
        level: u32,
        ty: u32,
        body: impl FnOnce(&mut [u8]) -> AxResult<usize>,
    ) -> AxResult<bool> {
        let remaining = self.capacity.saturating_sub(*self.len);
        if remaining < size_of::<cmsghdr>() {
            return Ok(false);
        }
        let body_capacity = remaining - size_of::<cmsghdr>();
        let cmsg_base = self.hdr.address().as_usize();

        let hdr = self.hdr.get_as_mut()?;
        hdr.cmsg_level = level as _;
        hdr.cmsg_type = ty as _;

        let data = UserPtr::<u8>::from(cmsg_base + size_of::<cmsghdr>())
            .get_as_mut_slice(body_capacity)?;
        let body_len = body(data)?;

        let cmsg_len = size_of::<cmsghdr>() + body_len;
        let padded = cmsg_align(cmsg_len);
        if padded > remaining {
            return Err(AxError::InvalidInput);
        }
        hdr.cmsg_len = cmsg_len;
        if padded > cmsg_len {
            UserPtr::<u8>::from(cmsg_base + cmsg_len)
                .get_as_mut_slice(padded - cmsg_len)?
                .fill(0);
        }
        self.hdr = UserPtr::from(cmsg_base + padded);
        *self.len += padded;
        Ok(true)
    }
}
