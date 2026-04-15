use axerrno::{AxError, AxResult};
use axhal::time::TimeValue;
use axpoll::IoEvents;
use axtask::future::{self, block_on, interruptible, poll_io};
use bitflags::bitflags;
use linux_raw_sys::general::{
    EPOLL_CLOEXEC, EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD, EPOLLEXCLUSIVE, EPOLLWAKEUP,
    epoll_event, timespec,
};
use starry_signal::SignalSet;

use crate::{
    file::{
        FileLike,
        epoll::{Epoll, EpollEvent, EpollFlags},
        get_file_like,
    },
    mm::{UserConstPtr, UserPtr, nullable},
    syscall::signal::check_sigset_size,
    task::with_blocked_signals,
    time::{TimeValueLike, read_timespec_user},
};

bitflags! {
    /// Flags for the `epoll_create` syscall.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct EpollCreateFlags: u32 {
        const CLOEXEC = EPOLL_CLOEXEC;
    }
}

/// Linux `epoll_event.events` bits accepted by `epoll_ctl` (`IoEvents` ∪ `EpollFlags` ∪
/// `EPOLLEXCLUSIVE`/`EPOLLWAKEUP`). The latter two are stripped before registration (not modeled).
const KNOWN_EPOLL_EVENTS_MASK: u32 =
    IoEvents::all().bits() | EpollFlags::all().bits() | EPOLLEXCLUSIVE | EPOLLWAKEUP;

const STRIP_EPOLL_CTL_UNUSED: u32 = EPOLLEXCLUSIVE | EPOLLWAKEUP;

pub fn sys_epoll_create1(flags: u32) -> AxResult<isize> {
    let flags = EpollCreateFlags::from_bits(flags).ok_or(AxError::InvalidInput)?;
    debug!("sys_epoll_create1 <= flags: {flags:?}");
    Epoll::new()
        .add_to_fd_table(flags.contains(EpollCreateFlags::CLOEXEC))
        .map(|fd| fd as isize)
}

pub fn sys_epoll_ctl(
    epfd: i32,
    op: u32,
    fd: i32,
    event: UserConstPtr<epoll_event>,
) -> AxResult<isize> {
    let epoll = Epoll::from_fd(epfd)?;
    debug!("sys_epoll_ctl <= epfd: {epfd}, op: {op}, fd: {fd}");

    // Linux `epoll_ctl(2)` / `ep_insert`: `fd` must not be the epoll instance itself → EINVAL.
    if fd == epfd {
        return Err(AxError::InvalidInput);
    }

    let parse_event = || -> AxResult<(EpollEvent, EpollFlags)> {
        let event = event.get_as_ref()?;
        let raw = event.events;
        if raw & !KNOWN_EPOLL_EVENTS_MASK != 0 {
            return Err(AxError::InvalidInput);
        }
        let masked = raw & !STRIP_EPOLL_CTL_UNUSED;
        let events =
            IoEvents::from_bits(masked & IoEvents::all().bits()).ok_or(AxError::InvalidInput)?;
        let flags =
            EpollFlags::from_bits(masked & EpollFlags::all().bits()).ok_or(AxError::InvalidInput)?;
        Ok((
            EpollEvent {
                events,
                user_data: event.data,
            },
            flags,
        ))
    };
    match op {
        EPOLL_CTL_ADD => {
            // Linux epoll_ctl: validate target fd (EBADF) before copy_from_user(event) (EFAULT).
            let _ = get_file_like(fd)?;
            let (event, flags) = parse_event()?;
            epoll.add(fd, event, flags)?;
        }
        EPOLL_CTL_MOD => {
            let _ = get_file_like(fd)?;
            let (event, flags) = parse_event()?;
            epoll.modify(fd, event, flags)?;
        }
        EPOLL_CTL_DEL => {
            epoll.delete(fd)?;
        }
        _ => return Err(AxError::InvalidInput),
    }
    Ok(0)
}

fn do_epoll_wait(
    epfd: i32,
    events: UserPtr<epoll_event>,
    maxevents: i32,
    timeout: Option<TimeValue>,
    sigmask: UserConstPtr<SignalSet>,
    sigsetsize: usize,
) -> AxResult<isize> {
    debug!("sys_epoll_wait <= epfd: {epfd}, maxevents: {maxevents}, timeout: {timeout:?}");

    // Linux `do_epoll_pwait`: `fget(epfd)` (EBADF) before `copy_sigset_from_user` / `sigsetsize`
    // validation (EINVAL); matches issue-334 `ppoll` / issue-338 `rt_sigtimedwait` ordering theme.
    let epoll = Epoll::from_fd(epfd)?;

    check_sigset_size(sigsetsize)?;

    if maxevents <= 0 {
        return Err(AxError::InvalidInput);
    }
    let events = events.get_as_mut_slice(maxevents as usize)?;

    with_blocked_signals(
        nullable!(sigmask.get_as_ref())?.copied(),
        || {
            // Align with `do_poll` / `do_select` (issue-355 / issue-364): `interruptible`(`timeout_at`(`poll_io`));
            // `Elapsed` → timeout (0 events); `Interrupted` → `EINTR` (issue-365).
            match block_on(interruptible(future::timeout_at(
                timeout,
                poll_io(epoll.as_ref(), IoEvents::IN, false, || epoll.poll_events(events)),
            ))) {
                Ok(Ok(Ok(n))) => Ok(n as isize),
                Ok(Ok(Err(e))) => Err(e),
                Ok(Err(_elapsed)) => Ok(0),
                Err(_) => Err(AxError::Interrupted),
            }
        },
    )
}

pub fn sys_epoll_pwait(
    epfd: i32,
    events: UserPtr<epoll_event>,
    maxevents: i32,
    timeout: i32,
    sigmask: UserConstPtr<SignalSet>,
    sigsetsize: usize,
) -> AxResult<isize> {
    let timeout = match timeout {
        -1 => None,
        t if t >= 0 => Some(TimeValue::from_millis(t as u64)),
        _ => return Err(AxError::InvalidInput),
    };
    do_epoll_wait(epfd, events, maxevents, timeout, sigmask, sigsetsize)
}

pub fn sys_epoll_pwait2(
    epfd: i32,
    events: UserPtr<epoll_event>,
    maxevents: i32,
    timeout: UserConstPtr<timespec>,
    sigmask: UserConstPtr<SignalSet>,
    sigsetsize: usize,
) -> AxResult<isize> {
    // issue-216: field-wise read (same as pselect6/ppoll / issue-215).
    let timeout = if timeout.is_null() {
        None
    } else {
        Some(
            read_timespec_user(timeout.address().as_usize() as *const timespec)?.try_into_time_value()?,
        )
    };
    do_epoll_wait(epfd, events, maxevents, timeout, sigmask, sigsetsize)
}
