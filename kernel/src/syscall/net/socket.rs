use axerrno::{AxError, AxResult, LinuxError};
#[cfg(feature = "vsock")]
use axnet::vsock::{VsockSocket, VsockStreamTransport};
use axnet::{
    Shutdown, Socket as SocketInner, SocketAddrEx, SocketOps,
    tcp::TcpSocket,
    udp::UdpSocket,
    unix::{DgramTransport, StreamTransport, UnixSocket},
};
use axtask::current;
use linux_raw_sys::{
    general::{O_CLOEXEC, O_NONBLOCK},
    net::{
        AF_INET, AF_UNIX, AF_VSOCK, IPPROTO_TCP, IPPROTO_UDP, SHUT_RD, SHUT_RDWR, SHUT_WR,
        SOCK_DGRAM, SOCK_STREAM, sockaddr, socklen_t,
    },
};

use super::addr::SocketAddrExt;
use crate::{
    file::{FileLike, Socket, close_file_like},
    mm::{UserConstPtr, UserPtr},
    task::AsThread,
};

/// Linux `SOCK_TYPE_MASK` (`uapi/linux/net.h`): base `SOCK_*` kind in the low nibble.
const SOCK_TYPE_MASK: u32 = 0xf;

/// `socket(2)` / `socketpair(2)` `type` must not set bits outside the sock type mask and
/// `SOCK_CLOEXEC`/`SOCK_NONBLOCK` (same values as `O_CLOEXEC`/`O_NONBLOCK`).
#[inline]
fn validate_socket_type(raw_ty: u32) -> AxResult<()> {
    let allowed = SOCK_TYPE_MASK | O_CLOEXEC | O_NONBLOCK;
    if raw_ty & !allowed != 0 {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

pub fn sys_socket(domain: u32, raw_ty: u32, proto: u32) -> AxResult<isize> {
    debug!("sys_socket <= domain: {domain}, ty: {raw_ty}, proto: {proto}");
    validate_socket_type(raw_ty)?;
    let ty = raw_ty & SOCK_TYPE_MASK;

    // Linux `unix_create`: Unix domain sockets require `protocol == 0` → EPROTONOSUPPORT otherwise.
    if domain == AF_UNIX && proto != 0 {
        return Err(AxError::from(LinuxError::EPROTONOSUPPORT));
    }

    let pid = current().as_thread().proc_data.proc.pid();
    let socket = match (domain, ty) {
        (AF_INET, SOCK_STREAM) => {
            if proto != 0 && proto != IPPROTO_TCP as _ {
                return Err(AxError::from(LinuxError::EPROTONOSUPPORT));
            }
            SocketInner::Tcp(TcpSocket::new())
        }
        (AF_INET, SOCK_DGRAM) => {
            if proto != 0 && proto != IPPROTO_UDP as _ {
                return Err(AxError::from(LinuxError::EPROTONOSUPPORT));
            }
            SocketInner::Udp(UdpSocket::new())
        }
        (AF_UNIX, SOCK_STREAM) => SocketInner::Unix(UnixSocket::new(StreamTransport::new(pid))),
        (AF_UNIX, SOCK_DGRAM) => SocketInner::Unix(UnixSocket::new(DgramTransport::new(pid))),
        #[cfg(feature = "vsock")]
        (AF_VSOCK, SOCK_STREAM) => {
            SocketInner::Vsock(VsockSocket::new(VsockStreamTransport::new()))
        }
        (AF_INET, _) | (AF_UNIX, _) | (AF_VSOCK, _) => {
            warn!("Unsupported socket type: domain: {domain}, ty: {ty}");
            return Err(AxError::from(LinuxError::ESOCKTNOSUPPORT));
        }
        _ => {
            return Err(AxError::from(LinuxError::EAFNOSUPPORT));
        }
    };
    let socket = Socket::new(socket);

    if raw_ty & O_NONBLOCK != 0 {
        socket.set_nonblocking(true)?;
    }
    let cloexec = raw_ty & O_CLOEXEC != 0;

    socket.add_to_fd_table(cloexec).map(|fd| fd as isize)
}

pub fn sys_bind(fd: i32, addr: UserConstPtr<sockaddr>, addrlen: u32) -> AxResult<isize> {
    // Linux __sys_bind: sockfd_lookup before move_addr_to_kernel (EBADF before EFAULT).
    let socket = Socket::from_fd(fd)?;
    let addr = SocketAddrEx::read_from_user(addr, addrlen)?;
    debug!("sys_bind <= fd: {fd}, addr: {addr:?}");

    socket.bind(addr)?;

    Ok(0)
}

pub fn sys_connect(fd: i32, addr: UserConstPtr<sockaddr>, addrlen: u32) -> AxResult<isize> {
    // Linux __sys_connect: sockfd_lookup before copy sockaddr (EBADF before EFAULT).
    let socket = Socket::from_fd(fd)?;
    let addr = SocketAddrEx::read_from_user(addr, addrlen)?;
    debug!("sys_connect <= fd: {fd}, addr: {addr:?}");

    socket.connect(addr).map_err(|e| {
        if e == AxError::WouldBlock {
            AxError::InProgress
        } else {
            e
        }
    })?;

    Ok(0)
}

pub fn sys_listen(fd: i32, backlog: i32) -> AxResult<isize> {
    debug!("sys_listen <= fd: {fd}, backlog: {backlog}");

    Socket::from_fd(fd)?.listen(backlog)?;

    Ok(0)
}

pub fn sys_accept(
    fd: i32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
) -> AxResult<isize> {
    sys_accept4(fd, addr, addrlen, 0)
}

pub fn sys_accept4(
    fd: i32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
    flags: u32,
) -> AxResult<isize> {
    debug!("sys_accept <= fd: {fd}, flags: {flags}");

    // Linux __sys_accept4: sockfd_lookup before validating accept4-only flags (EBADF before EINVAL
    // when both bad fd and unknown flag bits; issue-322; same class as splice/pidfd_getfd issue-317/315/320).
    let socket = Socket::from_fd(fd)?;

    // Linux accept4(2): only SOCK_CLOEXEC/SOCK_NONBLOCK (same values as O_CLOEXEC/O_NONBLOCK).
    const VALID_ACCEPT4_FLAGS: u32 = O_CLOEXEC | O_NONBLOCK;
    if flags & !VALID_ACCEPT4_FLAGS != 0 {
        return Err(AxError::InvalidInput);
    }

    let cloexec = flags & O_CLOEXEC != 0;
    let socket = Socket::new(socket.accept()?);
    if flags & O_NONBLOCK != 0 {
        socket.set_nonblocking(true)?;
    }

    let remote_addr = socket.peer_addr()?;

    // Linux __sys_accept4: copy peer address before installing the new fd; if copy_to_user fails,
    // do not leave an orphan accepted socket in the fd table (issue-149).
    if !addr.is_null() {
        remote_addr.write_to_user(addr, addrlen.get_as_mut()?)?;
    }

    let fd = socket.add_to_fd_table(cloexec).map(|fd| fd as isize)?;
    debug!("sys_accept => fd: {fd}, addr: {remote_addr:?}");

    Ok(fd)
}

pub fn sys_shutdown(fd: i32, how: u32) -> AxResult<isize> {
    debug!("sys_shutdown <= fd: {fd}, how: {how:?}");

    let socket = Socket::from_fd(fd)?;
    let how = match how {
        SHUT_RD => Shutdown::Read,
        SHUT_WR => Shutdown::Write,
        SHUT_RDWR => Shutdown::Both,
        _ => return Err(AxError::InvalidInput),
    };
    socket.shutdown(how).map(|_| 0)
}

pub fn sys_socketpair(
    domain: u32,
    raw_ty: u32,
    proto: u32,
    fds: UserPtr<[i32; 2]>,
) -> AxResult<isize> {
    debug!("sys_socketpair <= domain: {domain}, ty: {raw_ty}, proto: {proto}");
    validate_socket_type(raw_ty)?;
    let ty = raw_ty & SOCK_TYPE_MASK;

    if domain != AF_UNIX {
        return Err(AxError::from(LinuxError::EAFNOSUPPORT));
    }
    if proto != 0 {
        return Err(AxError::from(LinuxError::EPROTONOSUPPORT));
    }

    // Validate user `fds` before allocating the pair (issue-286; same class as issue-281 output
    // buffer ordering).
    let out = fds.get_as_mut()?;

    let pid = current().as_thread().proc_data.proc.pid();
    let (sock1, sock2) = match ty {
        SOCK_STREAM => {
            let (sock1, sock2) = StreamTransport::new_pair(pid);
            (UnixSocket::new(sock1), UnixSocket::new(sock2))
        }
        // `AF_UNIX` `SOCK_SEQPACKET` is not implemented; fall through to `_` → `ESOCKTNOSUPPORT`
        // like `sys_socket` (issue-251). Do not build `DgramTransport` pairs for SEQPACKET.
        SOCK_DGRAM => {
            let (sock1, sock2) = DgramTransport::new_pair(pid);
            (UnixSocket::new(sock1), UnixSocket::new(sock2))
        }
        _ => {
            warn!("Unsupported socketpair type: {ty}");
            return Err(AxError::from(LinuxError::ESOCKTNOSUPPORT));
        }
    };
    let sock1 = Socket::new(SocketInner::Unix(sock1));
    let sock2 = Socket::new(SocketInner::Unix(sock2));

    if raw_ty & O_NONBLOCK != 0 {
        sock1.set_nonblocking(true)?;
        sock2.set_nonblocking(true)?;
    }
    let cloexec = raw_ty & O_CLOEXEC != 0;

    let fd1 = sock1.add_to_fd_table(cloexec)?;
    let fd2 = match sock2.add_to_fd_table(cloexec) {
        Ok(fd) => fd,
        Err(e) => {
            let _ = close_file_like(fd1);
            return Err(e);
        }
    };
    *out = [fd1, fd2];
    Ok(0)
}
