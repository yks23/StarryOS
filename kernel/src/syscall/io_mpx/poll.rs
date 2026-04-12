use alloc::vec::Vec;

use axerrno::{AxError, AxResult};
use axhal::time::TimeValue;
use axpoll::IoEvents;
use axtask::future::{self, block_on, poll_io};
use linux_raw_sys::general::{POLLNVAL, pollfd, timespec};
use starry_signal::SignalSet;

use super::FdPollSet;
use crate::{
    file::get_file_like,
    mm::{UserConstPtr, UserPtr, nullable},
    syscall::signal::check_sigset_size,
    task::with_blocked_signals,
    time::TimeValueLike,
};

fn do_poll(
    poll_fds: &mut [pollfd],
    timeout: Option<TimeValue>,
    sigmask: Option<SignalSet>,
) -> AxResult<isize> {
    debug!("do_poll fds={poll_fds:?} timeout={timeout:?}");

    let mut nval_ready = 0isize;
    let mut fds = Vec::with_capacity(poll_fds.len());
    let mut fd_indices = Vec::with_capacity(poll_fds.len());
    for (idx, pfd) in poll_fds.iter_mut().enumerate() {
        if pfd.fd < 0 {
            // Linux: entries with negative fd are ignored; revents must be cleared.
            pfd.revents = 0;
            continue;
        }
        match get_file_like(pfd.fd) {
            Ok(f) => {
                fds.push((
                    f,
                    IoEvents::from_bits(pfd.events as _).ok_or(AxError::InvalidInput)?
                        | IoEvents::ALWAYS_POLL,
                ));
                fd_indices.push(idx);
            }
            Err(_) => {
                // If the fd is invalid, set revents to POLLNVAL
                pfd.revents = POLLNVAL as _;
                nval_ready += 1;
            }
        }
    }
    let fds = FdPollSet(fds);

    if fds.0.is_empty() {
        return Ok(nval_ready);
    }

    with_blocked_signals(sigmask, || {
        match block_on(future::timeout(
            timeout,
            poll_io(&fds, IoEvents::empty(), false, || {
                let mut res = 0usize;
                for ((fd, events), &idx) in fds.0.iter().zip(fd_indices.iter()) {
                    let mut result = fd.poll();
                    if result.contains(IoEvents::IN) {
                        result |= IoEvents::RDNORM;
                    }
                    if result.contains(IoEvents::OUT) {
                        result |= IoEvents::WRNORM;
                    }
                    result &= *events;

                    poll_fds[idx].revents = result.bits() as _;
                    if poll_fds[idx].revents != 0 {
                        res += 1;
                    }
                }
                if res > 0 {
                    Ok(res as isize)
                } else {
                    Err(AxError::WouldBlock)
                }
            }),
        )) {
            Ok(r) => r.map(|c: isize| c + nval_ready),
            Err(_) => {
                // Timeout: Linux counts every pollfd with non-zero revents (includes POLLNVAL).
                let mut total = 0isize;
                for fd in poll_fds.iter() {
                    if fd.revents != 0 {
                        total += 1;
                    }
                }
                Ok(total)
            }
        }
    })
}

#[cfg(target_arch = "x86_64")]
pub fn sys_poll(fds: UserPtr<pollfd>, nfds: u32, timeout: i32) -> AxResult<isize> {
    let fds = fds.get_as_mut_slice(nfds as usize)?;
    let timeout = if timeout < 0 {
        None
    } else {
        Some(TimeValue::from_millis(timeout as u64))
    };
    do_poll(fds, timeout, None)
}

pub fn sys_ppoll(
    fds: UserPtr<pollfd>,
    nfds: i32,
    timeout: UserConstPtr<timespec>,
    sigmask: UserConstPtr<SignalSet>,
    sigsetsize: usize,
) -> AxResult<isize> {
    check_sigset_size(sigsetsize)?;
    let fds = fds.get_as_mut_slice(nfds.try_into().map_err(|_| AxError::InvalidInput)?)?;
    let timeout = nullable!(timeout.get_as_ref())?
        .map(|ts| ts.try_into_time_value())
        .transpose()?;
    // TODO: handle signal
    do_poll(fds, timeout, nullable!(sigmask.get_as_ref())?.copied())
}
