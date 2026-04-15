use core::mem::size_of;

use axerrno::{AxError, AxResult, LinuxError};
use axnet::options::{Configurable, GetSocketOption, SetSocketOption};
use linux_raw_sys::{
    general::timeval,
    net::{socklen_t, tcp_info, IP_TTL, TCP_INFO, SOL_SOCKET, SO_RCVTIMEO, SO_SNDTIMEO},
};

use crate::{
    file::{FileLike, Socket},
    mm::{UserConstPtr, UserPtr},
    time::{read_timeval_user, write_timeval_user},
};

const PROTO_TCP: u32 = linux_raw_sys::net::IPPROTO_TCP as u32;

const PROTO_IP: u32 = linux_raw_sys::net::IPPROTO_IP as u32;

mod conv {
    use core::mem::size_of;

    use axerrno::{AxError, AxResult};
    use axnet::options::UnixCredentials;
    use linux_raw_sys::{general::timeval, net::socklen_t, net::ucred};

    use crate::mm::UserConstPtr;
    use crate::time::TimeValueLike;

    pub struct Int<T>(T);

    impl<T: TryFrom<i32> + TryInto<i32>> Int<T> {
        pub fn sys_to_rust(val: i32) -> AxResult<T> {
            T::try_from(val).map_err(|_| AxError::InvalidInput)
        }

        pub fn rust_to_sys(val: T) -> AxResult<i32> {
            val.try_into().map_err(|_| AxError::InvalidInput)
        }
    }

    pub struct IntBool;

    impl IntBool {
        pub fn sys_to_rust(val: i32) -> AxResult<bool> {
            Ok(val != 0)
        }

        pub fn rust_to_sys(val: bool) -> AxResult<i32> {
            Ok(val as _)
        }
    }

    pub struct Duration;

    impl Duration {
        pub fn sys_to_rust(val: timeval) -> AxResult<core::time::Duration> {
            val.try_into_time_value()
        }

        pub fn rust_to_sys(val: core::time::Duration) -> AxResult<timeval> {
            Ok(timeval::from_time_value(val))
        }
    }

    pub struct Ucred;

    impl Ucred {
        pub fn sys_to_rust(val: ucred) -> AxResult<UnixCredentials> {
            Ok(UnixCredentials {
                pid: val.pid,
                uid: val.uid,
                gid: val.gid,
            })
        }

        pub fn rust_to_sys(val: UnixCredentials) -> AxResult<ucred> {
            Ok(ucred {
                pid: val.pid,
                uid: val.uid,
                gid: val.gid,
            })
        }
    }

    /// Linux `IP_TTL` uses `int` / `sizeof(int)` for `optlen` (man 7 ip); values are 0–255.
    pub struct IpTtl;

    impl IpTtl {
        pub fn sys_to_rust(val: UserConstPtr<u8>, len: socklen_t) -> AxResult<u8> {
            if (len as usize) < size_of::<i32>() {
                return Err(AxError::InvalidInput);
            }
            let v = *val.cast::<i32>().get_as_ref()?;
            u8::try_from(v).map_err(|_| AxError::InvalidInput)
        }

        pub fn rust_to_sys(val: u8) -> AxResult<i32> {
            Ok(val as i32)
        }
    }
}

macro_rules! call_dispatch {
    ($dispatch:ident, $pat:expr) => {{
        use conv::*;
        use linux_raw_sys::net::*;

        call_dispatch! {
            $dispatch, $pat,
            (SOL_SOCKET, SO_REUSEADDR) => ReuseAddress as IntBool,
            (SOL_SOCKET, SO_ERROR) => Error,
            (SOL_SOCKET, SO_DONTROUTE) => DontRoute as IntBool,
            (SOL_SOCKET, SO_SNDBUF) => SendBuffer as Int<usize>,
            (SOL_SOCKET, SO_RCVBUF) => ReceiveBuffer as Int<usize>,
            (SOL_SOCKET, SO_KEEPALIVE) => KeepAlive as IntBool,
            (SOL_SOCKET, SO_PASSCRED) => PassCredentials as IntBool,
            (SOL_SOCKET, SO_PEERCRED) => PeerCredentials as Ucred,

            (PROTO_TCP, TCP_NODELAY) => NoDelay as IntBool,
            (PROTO_TCP, TCP_MAXSEG) => MaxSegment as Int<usize>,
        }
    }};
    ($dispatch:ident, $in:expr, $($pat:pat => $which:ident $(as $conv:ty)?),* $(,)?) => {
        match $in {
            $(
                $pat => {
                    dispatch!($which $(as $conv)?);
                }
            )*
            _ => return Err(AxError::from(LinuxError::ENOPROTOOPT)),
        }
    }
}

/// Like [`call_dispatch!`], but the fallback arm does `sockfd_lookup` before `ENOPROTOOPT` without
/// reading `optval` (issue-348); matched arms expect `dispatch!` to read user input before `from_fd`.
macro_rules! call_setsockopt_dispatch {
    ($dispatch:ident, $fd:expr, $pat:expr) => {{
        use conv::*;
        use linux_raw_sys::net::*;

        call_setsockopt_dispatch! {
            $dispatch, $fd, $pat,
            (SOL_SOCKET, SO_REUSEADDR) => ReuseAddress as IntBool,
            (SOL_SOCKET, SO_ERROR) => Error,
            (SOL_SOCKET, SO_DONTROUTE) => DontRoute as IntBool,
            (SOL_SOCKET, SO_SNDBUF) => SendBuffer as Int<usize>,
            (SOL_SOCKET, SO_RCVBUF) => ReceiveBuffer as Int<usize>,
            (SOL_SOCKET, SO_KEEPALIVE) => KeepAlive as IntBool,
            (SOL_SOCKET, SO_PASSCRED) => PassCredentials as IntBool,
            (SOL_SOCKET, SO_PEERCRED) => PeerCredentials as Ucred,

            (PROTO_TCP, TCP_NODELAY) => NoDelay as IntBool,
            (PROTO_TCP, TCP_MAXSEG) => MaxSegment as Int<usize>,
        }
    }};
    ($dispatch:ident, $fd:expr, $in:expr, $($pat:pat => $which:ident $(as $conv:ty)?),* $(,)?) => {
        match $in {
            $(
                $pat => {
                    dispatch!($which $(as $conv)?);
                }
            )*
            _ => {
                let _ = Socket::from_fd($fd)?;
                return Err(AxError::from(LinuxError::ENOPROTOOPT));
            }
        }
    }
}

pub fn sys_getsockopt(
    fd: i32,
    level: u32,
    optname: u32,
    optval: UserPtr<u8>,
    optlen: UserPtr<socklen_t>,
) -> AxResult<isize> {
    debug!(
        "sys_getsockopt <= fd: {}, level: {}, optname: {}, optval: {:?}, optlen: {:?}",
        fd,
        level,
        optname,
        optval.address(),
        optlen.address(),
    );

    fn get<'a, T: 'static>(val: UserPtr<u8>, len: &mut socklen_t) -> AxResult<&'a mut T> {
        if (*len as usize) < size_of::<T>() {
            return Err(AxError::InvalidInput);
        }
        *len = size_of::<T>() as socklen_t;
        val.cast().get_as_mut()
    }

    // Linux `do_getsockopt` often touches `optlen` before `sockfd_lookup`; bad `optlen` → EFAULT
    // before `EBADF` on bad `fd` (issue-347; supersedes from_fd-first ordering, issue-122).
    let optlen = optlen.get_as_mut()?;
    let socket = Socket::from_fd(fd)?;
    // `TCP_INFO`: validate length first; only set `*optlen` after a successful fill (issue-167).
    // Pass a real `&mut [u8]` so axnet cannot return Ok(0) without writing `struct tcp_info` (issue-191).
    if level == PROTO_TCP && optname == TCP_INFO {
        let need = size_of::<tcp_info>();
        if (*optlen as usize) < need {
            return Err(AxError::InvalidInput);
        }
        let buf = optval.get_as_mut_slice(need)?;
        let mut opt = GetSocketOption::TcpInfo(buf);
        if socket.get_option_inner(&mut opt)? {
            *optlen = need as socklen_t;
            return Ok(0);
        }
        return Err(AxError::from(LinuxError::ENOPROTOOPT));
    }
    if level == PROTO_IP && optname == IP_TTL {
        let mut val = 0u8;
        socket.get_option(GetSocketOption::Ttl(&mut val))?;
        *get(optval, optlen)? = conv::IpTtl::rust_to_sys(val)?;
        return Ok(0);
    }
    // issue-223: symmetric with `setsockopt` SO_RCVTIMEO/SO_SNDTIMEO (issue-217).
    if level == SOL_SOCKET && (optname == SO_RCVTIMEO || optname == SO_SNDTIMEO) {
        if (*optlen as usize) < size_of::<timeval>() {
            return Err(AxError::InvalidInput);
        }
        let mut d = core::time::Duration::ZERO;
        if optname == SO_RCVTIMEO {
            socket.get_option(GetSocketOption::ReceiveTimeout(&mut d))?;
        } else {
            socket.get_option(GetSocketOption::SendTimeout(&mut d))?;
        }
        let tv = conv::Duration::rust_to_sys(d)?;
        *optlen = size_of::<timeval>() as socklen_t;
        let p = optval.address().as_usize() as *mut timeval;
        write_timeval_user(p, tv)?;
        return Ok(0);
    }
    macro_rules! dispatch {
        ($which:ident) => {
            socket.get_option(GetSocketOption::$which(get(optval, optlen)?))?;
        };
        ($which:ident as $conv:ty) => {
            let mut val = Default::default();
            socket.get_option(GetSocketOption::$which(&mut val))?;
            *get(optval, optlen)? = <$conv>::rust_to_sys(val)?;
        };
    }
    call_dispatch!(dispatch, (level, optname));

    Ok(0)
}

pub fn sys_setsockopt(
    fd: i32,
    level: u32,
    optname: u32,
    optval: UserConstPtr<u8>,
    optlen: socklen_t,
) -> AxResult<isize> {
    debug!(
        "sys_setsockopt <= fd: {}, level: {}, optname: {}, optval: {:?}, optlen: {}",
        fd,
        level,
        optname,
        optval.address(),
        optlen
    );

    /// Linux accepts `optlen >= sizeof(value)` and uses only the first `sizeof(T)` bytes.
    fn get<'a, T: 'static>(val: UserConstPtr<u8>, len: socklen_t) -> AxResult<&'a T> {
        if (len as usize) < size_of::<T>() {
            return Err(AxError::InvalidInput);
        }
        val.cast().get_as_ref()
    }

    // Linux `do_setsockopt`: copy/validate user `optval` before `sockfd_lookup` on known paths
    // (issue-348; symmetric with issue-347 `getsockopt` / `optlen` before `from_fd`). Unknown
    // `(level, optname)` still does `sockfd_lookup` before `ENOPROTOOPT` without touching `optval`
    // (`call_setsockopt_dispatch` fallback).
    if level == PROTO_IP && optname == IP_TTL {
        let val = conv::IpTtl::sys_to_rust(optval, optlen)?;
        let socket = Socket::from_fd(fd)?;
        socket.set_option(SetSocketOption::Ttl(&val))?;
        return Ok(0);
    }
    // issue-217: `timeval` field-wise read (same as `read_timeval_user` / issue-204), not `get_as_ref` bulk load.
    if level == SOL_SOCKET && (optname == SO_RCVTIMEO || optname == SO_SNDTIMEO) {
        if (optlen as usize) < size_of::<timeval>() {
            return Err(AxError::InvalidInput);
        }
        let p = optval.address().as_usize() as *const timeval;
        let val = conv::Duration::sys_to_rust(read_timeval_user(p)?)?;
        let socket = Socket::from_fd(fd)?;
        if optname == SO_RCVTIMEO {
            socket.set_option(SetSocketOption::ReceiveTimeout(&val))?;
        } else {
            socket.set_option(SetSocketOption::SendTimeout(&val))?;
        }
        return Ok(0);
    }
    macro_rules! dispatch {
        ($which:ident) => {
            {
                let r = get(optval, optlen)?;
                let socket = Socket::from_fd(fd)?;
                socket.set_option(SetSocketOption::$which(r))?;
            }
        };
        ($which:ident as $conv:ty) => {
            {
                let mut val = <$conv>::sys_to_rust(*get(optval, optlen)?)?;
                let socket = Socket::from_fd(fd)?;
                socket.set_option(SetSocketOption::$which(&mut val))?;
            }
        };
    }
    call_setsockopt_dispatch!(dispatch, fd, (level, optname));

    Ok(0)
}
