use alloc::{boxed::Box, vec::Vec};
use core::mem::{offset_of, size_of};
use core::net::Ipv4Addr;

use axerrno::{AxError, AxResult};
use axio::prelude::*;
use axnet::{CMsgData, RecvFlags, RecvOptions, SendFlags, SendOptions, SocketAddrEx, SocketOps};
use linux_raw_sys::net::{
    MSG_CONFIRM, MSG_CMSG_CLOEXEC, MSG_CTRUNC, MSG_DONTROUTE, MSG_DONTWAIT, MSG_EOR,
    MSG_ERRQUEUE, MSG_FIN, MSG_MORE, MSG_NOSIGNAL, MSG_OOB, MSG_PEEK, MSG_PROBE, MSG_RST,
    MSG_SYN, MSG_TRUNC, MSG_WAITALL, SCM_RIGHTS, SOL_SOCKET, cmsghdr, msghdr, sockaddr,
    socklen_t,
};

/// Linux `recvmsg(2)` / `recvfrom(2)` 第三参 flags：仅允许内核接受的 `MSG_*` 组合（与 Linux
/// `recvmsg` 策略一致），未知位须 **`EINVAL`**。
const RECVMSG_FLAGS_MASK: u32 = MSG_OOB
    | MSG_PEEK
    | MSG_DONTROUTE
    | MSG_TRUNC
    | MSG_DONTWAIT
    | MSG_WAITALL
    | MSG_ERRQUEUE
    | MSG_CMSG_CLOEXEC;

/// `recvmsg` 掩码内、Starry 尚未实现的 `MSG_*`（显式 **`Unsupported`**，勿静默忽略）。
const RECVMSG_FLAGS_UNSUPPORTED: u32 =
    MSG_OOB | MSG_DONTROUTE | MSG_ERRQUEUE | MSG_CMSG_CLOEXEC;

/// Linux `sendmsg(2)` / `sendto(2)` flags：与 `recv` 掩码不同（不含 **`MSG_PEEK`** 等仅接收语义位）。
const SENDMSG_FLAGS_MASK: u32 = MSG_OOB
    | MSG_DONTROUTE
    | MSG_PROBE
    | MSG_TRUNC
    | MSG_DONTWAIT
    | MSG_EOR
    | MSG_WAITALL
    | MSG_FIN
    | MSG_SYN
    | MSG_CONFIRM
    | MSG_RST
    | MSG_ERRQUEUE
    | MSG_NOSIGNAL
    | MSG_MORE
    | MSG_CMSG_CLOEXEC;

use super::addr::SocketAddrExt;
use crate::{
    file::{FileLike, Socket, add_file_like},
    mm::{IoVec, IoVectorBuf, UserConstPtr, UserPtr, VmBytes, VmBytesMut},
    syscall::net::{CMsg, CMsgBuilder, cmsg_align},
};

/// After [`Socket::from_fd`] and `flags` validation: optional `to` address + [`Socket::send`].
fn send_on_socket(
    socket: &Socket,
    fd: i32,
    mut src: impl Read + IoBuf,
    flags: u32,
    addr: UserConstPtr<sockaddr>,
    addrlen: socklen_t,
    cmsg: Vec<CMsgData>,
) -> AxResult<isize> {
    let addr = if addr.is_null() || addrlen == 0 {
        None
    } else {
        Some(SocketAddrEx::read_from_user(addr, addrlen)?)
    };

    debug!("sys_send <= fd: {fd}, flags: {flags}, addr: {addr:?}");
    let sent = socket.send(
        &mut src,
        SendOptions {
            to: addr,
            flags: SendFlags::from_bits_truncate(flags),
            cmsg,
        },
    )?;

    Ok(sent as isize)
}

fn send_impl(
    fd: i32,
    src: impl Read + IoBuf,
    flags: u32,
    addr: UserConstPtr<sockaddr>,
    addrlen: socklen_t,
    cmsg: Vec<CMsgData>,
) -> AxResult<isize> {
    if flags & !SENDMSG_FLAGS_MASK != 0 {
        return Err(AxError::InvalidInput);
    }

    // Linux __sys_sendto / sendmsg: sockfd_lookup before copy sockaddr (EBADF before EFAULT).
    let socket = Socket::from_fd(fd)?;
    send_on_socket(&socket, fd, src, flags, addr, addrlen, cmsg)
}

pub fn sys_sendto(
    fd: i32,
    buf: *const u8,
    len: usize,
    flags: u32,
    addr: UserConstPtr<sockaddr>,
    addrlen: socklen_t,
) -> AxResult<isize> {
    send_impl(fd, VmBytes::new(buf, len), flags, addr, addrlen, Vec::new())
}

pub fn sys_sendmsg(fd: i32, msg: UserConstPtr<msghdr>, flags: u32) -> AxResult<isize> {
    // Linux __sys_sendmsg: sockfd_lookup before copy_msghdr_from_user (EBADF before EFAULT).
    let socket = Socket::from_fd(fd)?;
    if flags & !SENDMSG_FLAGS_MASK != 0 {
        return Err(AxError::InvalidInput);
    }

    // Whole-structure snapshot (like `copy_msghdr_from_user`): `msg_control` / `msg_controllen` /
    // `msg_iov` / … come from one load sequence, not independent re-reads of user `msghdr` (issue-197).
    let msg = *msg.get_as_ref()?;
    let mut cmsg = Vec::new();
    if !msg.msg_control.is_null() {
        let mut ptr = msg.msg_control as usize;
        let ptr_end = ptr
            .checked_add(msg.msg_controllen)
            .ok_or(AxError::InvalidInput)?;
        while let Some(hdr_end) = ptr.checked_add(size_of::<cmsghdr>()) {
            if hdr_end > ptr_end {
                break;
            }
            // Snapshot header in kernel: stepping and `CMsg::parse` use the same `cmsg_len`
            // (Linux copies ancillary headers before parsing; avoids TOCTOU on user `cmsg_len`).
            let hdr = *UserConstPtr::<cmsghdr>::from(ptr).get_as_ref()?;
            if hdr.cmsg_len < size_of::<cmsghdr>() {
                return Err(AxError::InvalidInput);
            }
            let step = cmsg_align(hdr.cmsg_len)?;
            let Some(next) = ptr.checked_add(step) else {
                return Err(AxError::InvalidInput);
            };
            if next > ptr_end {
                return Err(AxError::InvalidInput);
            }
            cmsg.push(Box::new(CMsg::parse(&hdr, ptr)?) as CMsgData);
            ptr += step;
        }
    }
    send_on_socket(
        &socket,
        fd,
        IoVectorBuf::new(msg.msg_iov.cast::<IoVec>(), msg.msg_iovlen)?.into_io(),
        flags,
        UserConstPtr::from(msg.msg_name as usize),
        msg.msg_namelen as socklen_t,
        cmsg,
    )
}

#[inline]
fn validate_recvmsg_flags(flags: u32) -> AxResult<()> {
    if flags & !RECVMSG_FLAGS_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    if flags & RECVMSG_FLAGS_UNSUPPORTED != 0 {
        return Err(AxError::OperationNotSupported);
    }
    Ok(())
}

/// `recv_on_socket` uses this as `RecvOptions::from` when the user supplies `msg_name`/`addr`.
/// Datagram transports overwrite it with the sender; stream transports (TCP, Unix stream, vsock)
/// leave it unchanged. Linux `recvfrom(2)` then returns the connected peer (same as `getpeername`).
#[inline]
fn recv_addr_is_kernel_placeholder(addr: &SocketAddrEx) -> bool {
    matches!(
        addr,
        SocketAddrEx::Ip(sa) if sa.ip().is_unspecified() && sa.port() == 0
    )
}

/// After [`validate_recvmsg_flags`], [`Socket::from_fd`], and (for `recvmsg`) user `msghdr` setup.
fn recv_on_socket(
    socket: &Socket,
    fd: i32,
    mut dst: impl Write + IoBufMut,
    flags: u32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
    cmsg_builder: Option<CMsgBuilder>,
    msg_flags_out: Option<UserPtr<u32>>,
) -> AxResult<isize> {
    debug!("sys_recv <= fd: {fd}, flags: {flags}");

    let recv_flags = RecvFlags::from_bits_truncate(flags);

    let mut cmsg = Vec::new();
    let mut msg_trunc = false;

    let mut remote_addr =
        (!addr.is_null()).then(|| SocketAddrEx::Ip((Ipv4Addr::UNSPECIFIED, 0).into()));
    let recv = socket.recv(
        &mut dst,
        RecvOptions {
            from: remote_addr.as_mut(),
            flags: recv_flags,
            cmsg: Some(&mut cmsg),
            msg_trunc: msg_flags_out
                .is_some()
                .then_some(core::ptr::addr_of_mut!(msg_trunc)),
        },
    )?;

    if let Some(mut remote_addr) = remote_addr {
        if recv_addr_is_kernel_placeholder(&remote_addr)
            && let Ok(peer) = socket.peer_addr()
        {
            remote_addr = peer;
        }
        remote_addr.write_to_user(addr, addrlen.get_as_mut()?)?;
    }

    let mut cmsg_trunc = false;
    if let Some(mut builder) = cmsg_builder {
        let mut iter = cmsg.into_iter();
        while let Some(cmsg) = iter.next() {
            let Ok(cmsg) = cmsg.downcast::<CMsg>() else {
                warn!("received unexpected cmsg");
                cmsg_trunc = true;
                continue;
            };

            let pushed = match *cmsg {
                CMsg::Rights { fds } => {
                    // `push` may return `Ok(false)` before invoking the closure; do not move `fds`
                    // into a `FnOnce` that could be dropped unrun (issue-183).
                    if builder.remaining() < size_of::<cmsghdr>() {
                        cmsg_trunc = true;
                        drop(fds);
                        continue;
                    }
                    let body_cap = builder.remaining() - size_of::<cmsghdr>();
                    let total = fds.len();
                    let pushed_inner = builder.push(SOL_SOCKET, SCM_RIGHTS, move |data| {
                        let cap_fds = data.len() / size_of::<i32>();
                        let to_install = total.min(cap_fds);
                        let mut it = fds.into_iter();
                        for (f, chunk) in it
                            .by_ref()
                            .take(to_install)
                            .zip(data.chunks_exact_mut(size_of::<i32>()))
                        {
                            let fd = add_file_like(f, false)?;
                            chunk.copy_from_slice(&fd.to_ne_bytes());
                        }
                        for f in it {
                            drop(f);
                        }
                        Ok(to_install * size_of::<i32>())
                    })?;
                    if total.saturating_mul(size_of::<i32>()) > body_cap {
                        cmsg_trunc = true;
                    }
                    pushed_inner
                }
            };
            if !pushed {
                cmsg_trunc = true;
                break;
            }
        }
        if iter.next().is_some() {
            cmsg_trunc = true;
        }
        builder.commit()?;
    } else if !cmsg.is_empty() {
        cmsg_trunc = true;
    }

    if let Some(out) = msg_flags_out {
        let mut mf = 0u32;
        if msg_trunc {
            mf |= MSG_TRUNC;
        }
        if cmsg_trunc {
            mf |= MSG_CTRUNC;
        }
        *out.get_as_mut()? = mf;
    }

    debug!("sys_recv => fd: {fd}, recv: {recv}");
    Ok(recv as isize)
}

fn recv_impl(
    fd: i32,
    dst: impl Write + IoBufMut,
    flags: u32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
    cmsg_builder: Option<CMsgBuilder>,
) -> AxResult<isize> {
    validate_recvmsg_flags(flags)?;
    let socket = Socket::from_fd(fd)?;
    recv_on_socket(&socket, fd, dst, flags, addr, addrlen, cmsg_builder, None)
}

pub fn sys_recvfrom(
    fd: i32,
    buf: *mut u8,
    len: usize,
    flags: u32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
) -> AxResult<isize> {
    recv_impl(fd, VmBytesMut::new(buf, len), flags, addr, addrlen, None)
}

pub fn sys_recvmsg(fd: i32, msg: UserPtr<msghdr>, flags: u32) -> AxResult<isize> {
    // Linux __sys_recvmsg: flags + sockfd_lookup before copy_msghdr_from_user (EINVAL/EBADF before EFAULT).
    validate_recvmsg_flags(flags)?;
    let socket = Socket::from_fd(fd)?;
    // Snapshot `msghdr` + field addresses: avoid holding `&mut msghdr` across `recv` (issue-198).
    let base = msg.address().as_usize();
    let m = *msg.get_as_mut()?;
    let cmsg_builder = if m.msg_control.is_null() {
        None
    } else {
        Some(CMsgBuilder::new(
            UserPtr::<cmsghdr>::from(m.msg_control as usize),
            UserPtr::<usize>::from(base + offset_of!(msghdr, msg_controllen)),
        )?)
    };
    recv_on_socket(
        &socket,
        fd,
        IoVectorBuf::new(m.msg_iov.cast::<IoVec>(), m.msg_iovlen)?.into_io(),
        flags,
        UserPtr::from(m.msg_name as usize),
        UserPtr::<socklen_t>::from(base + offset_of!(msghdr, msg_namelen)),
        cmsg_builder,
        Some(UserPtr::<u32>::from(base + offset_of!(msghdr, msg_flags))),
    )
}
