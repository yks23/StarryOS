use alloc::{boxed::Box, vec::Vec};
use core::mem::size_of;
use core::net::Ipv4Addr;

use axerrno::{AxError, AxResult};
use axio::prelude::*;
use axnet::{CMsgData, RecvFlags, RecvOptions, SendFlags, SendOptions, SocketAddrEx, SocketOps};
use linux_raw_sys::net::{
    MSG_CONFIRM, MSG_CMSG_CLOEXEC, MSG_DONTROUTE, MSG_DONTWAIT, MSG_EOR, MSG_ERRQUEUE,
    MSG_FIN, MSG_MORE, MSG_NOSIGNAL, MSG_OOB, MSG_PEEK, MSG_PROBE, MSG_RST, MSG_SYN,
    MSG_TRUNC, MSG_WAITALL, SCM_RIGHTS, SOL_SOCKET, cmsghdr, msghdr, sockaddr, socklen_t,
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

    let msg = msg.get_as_ref()?;
    let mut cmsg = Vec::new();
    if !msg.msg_control.is_null() {
        let mut ptr = msg.msg_control as usize;
        let ptr_end = ptr + msg.msg_controllen;
        while ptr + size_of::<cmsghdr>() <= ptr_end {
            let hdr = UserConstPtr::<cmsghdr>::from(ptr).get_as_ref()?;
            if hdr.cmsg_len < size_of::<cmsghdr>() {
                return Err(AxError::InvalidInput);
            }
            let step = cmsg_align(hdr.cmsg_len);
            let Some(next) = ptr.checked_add(step) else {
                return Err(AxError::InvalidInput);
            };
            if next > ptr_end {
                return Err(AxError::InvalidInput);
            }
            cmsg.push(Box::new(CMsg::parse(hdr)?) as CMsgData);
            ptr += step;
        }
    }
    send_on_socket(
        &socket,
        fd,
        IoVectorBuf::new(msg.msg_iov as *const IoVec, msg.msg_iovlen)?.into_io(),
        flags,
        UserConstPtr::from(msg.msg_name as usize),
        msg.msg_namelen as socklen_t,
        cmsg,
    )
}

fn recv_impl(
    fd: i32,
    mut dst: impl Write + IoBufMut,
    flags: u32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
    cmsg_builder: Option<CMsgBuilder>,
) -> AxResult<isize> {
    if flags & !RECVMSG_FLAGS_MASK != 0 {
        return Err(AxError::InvalidInput);
    }
    if flags & RECVMSG_FLAGS_UNSUPPORTED != 0 {
        return Err(AxError::OperationNotSupported);
    }

    debug!("sys_recv <= fd: {fd}, flags: {flags}");

    let socket = Socket::from_fd(fd)?;
    let recv_flags = RecvFlags::from_bits_truncate(flags);

    let mut cmsg = Vec::new();

    let mut remote_addr =
        (!addr.is_null()).then(|| SocketAddrEx::Ip((Ipv4Addr::UNSPECIFIED, 0).into()));
    let recv = socket.recv(
        &mut dst,
        RecvOptions {
            from: remote_addr.as_mut(),
            flags: recv_flags,
            cmsg: Some(&mut cmsg),
        },
    )?;

    if let Some(remote_addr) = remote_addr {
        remote_addr.write_to_user(addr, addrlen.get_as_mut()?)?;
    }

    if let Some(mut builder) = cmsg_builder {
        for cmsg in cmsg {
            let Ok(cmsg) = cmsg.downcast::<CMsg>() else {
                warn!("received unexpected cmsg");
                continue;
            };

            let pushed = match *cmsg {
                CMsg::Rights { fds } => builder.push(SOL_SOCKET, SCM_RIGHTS, |data| {
                    let mut written = 0;
                    for (f, chunk) in fds.into_iter().zip(data.chunks_exact_mut(size_of::<i32>())) {
                        let fd = add_file_like(f, false)?;
                        chunk.copy_from_slice(&fd.to_ne_bytes());
                        written += size_of::<i32>();
                    }
                    Ok(written)
                })?,
            };
            if !pushed {
                break;
            }
        }
    }

    debug!("sys_recv => fd: {fd}, recv: {recv}");
    Ok(recv as isize)
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
    let msg = msg.get_as_mut()?;
    recv_impl(
        fd,
        IoVectorBuf::new(msg.msg_iov as *mut IoVec, msg.msg_iovlen)?.into_io(),
        flags,
        UserPtr::from(msg.msg_name as usize),
        UserPtr::from(&mut msg.msg_namelen as *mut _ as *mut socklen_t),
        (!msg.msg_control.is_null()).then(|| {
            CMsgBuilder::new(
                UserPtr::from(msg.msg_control as *mut cmsghdr),
                &mut msg.msg_controllen,
            )
        }),
    )
}
