use axerrno::AxResult;
use axnet::SocketOps;
use linux_raw_sys::net::{sockaddr, socklen_t};

use super::addr::SocketAddrExt;
use crate::{
    file::{FileLike, Socket},
    mm::UserPtr,
};

pub fn sys_getsockname(
    fd: i32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
) -> AxResult<isize> {
    let socket = Socket::from_fd(fd)?;
    // Resolve `addrlen` before `local_addr` so NULL/unwritable `addrlen` fails like Linux
    // (move_addr_to_user) before any address lookup with potential side effects.
    let alen = addrlen.get_as_mut()?;
    let local_addr = socket.local_addr()?;
    debug!("sys_getsockname <= fd: {fd}, addr: {local_addr:?}");

    local_addr.write_to_user(addr, alen)?;
    Ok(0)
}

pub fn sys_getpeername(
    fd: i32,
    addr: UserPtr<sockaddr>,
    addrlen: UserPtr<socklen_t>,
) -> AxResult<isize> {
    let socket = Socket::from_fd(fd)?;
    let alen = addrlen.get_as_mut()?;
    let peer_addr = socket.peer_addr()?;
    debug!("sys_getpeername <= fd: {fd}, addr: {peer_addr:?}");

    peer_addr.write_to_user(addr, alen)?;
    Ok(0)
}
