use alloc::vec::Vec;
use core::{fmt, mem, ptr, time::Duration};

use axerrno::{AxError, AxResult};
use axpoll::IoEvents;
use axtask::future::{self, block_on, poll_io};
use bitmaps::Bitmap;
use linux_raw_sys::{
    general::*,
    select_macros::{FD_ISSET, FD_SET, FD_ZERO},
};
use starry_signal::SignalSet;

use super::FdPollSet;
use crate::{
    file::FD_TABLE,
    mm::{UserConstPtr, UserPtr, nullable},
    syscall::signal::check_sigset_size,
    task::with_blocked_signals,
    time::{TimeValueLike, read_timespec_user},
};
#[cfg(target_arch = "x86_64")]
use crate::time::read_timeval_user;

struct FdSet(Bitmap<{ __FD_SETSIZE as usize }>);

impl FdSet {
    fn new(nfds: usize, fds: Option<&__kernel_fd_set>) -> Self {
        let mut bitmap = Bitmap::new();
        if let Some(fds) = fds {
            for i in 0..nfds {
                if unsafe { FD_ISSET(i as _, fds) } {
                    bitmap.set(i, true);
                }
            }
        }
        Self(bitmap)
    }
}

impl fmt::Debug for FdSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.0).finish()
    }
}

#[inline]
fn empty_kernel_fd_set() -> __kernel_fd_set {
    let mut s: __kernel_fd_set = unsafe { mem::zeroed() };
    unsafe { FD_ZERO(&mut s) };
    s
}

/// Copy a kernel-side output `fd_set` to the user buffer. Linux keeps user `fd_set` unchanged
/// until `select` returns; we only touch user memory here.
fn copy_fd_set_to_user(user: &mut __kernel_fd_set, kern: &__kernel_fd_set) {
    unsafe {
        ptr::copy_nonoverlapping(
            kern as *const __kernel_fd_set as *const u8,
            user as *mut __kernel_fd_set as *mut u8,
            mem::size_of::<__kernel_fd_set>(),
        );
    }
}

fn flush_output_fd_sets(
    readfds: Option<&mut __kernel_fd_set>,
    writefds: Option<&mut __kernel_fd_set>,
    exceptfds: Option<&mut __kernel_fd_set>,
    read_kern: Option<&__kernel_fd_set>,
    write_kern: Option<&__kernel_fd_set>,
    except_kern: Option<&__kernel_fd_set>,
) {
    if let (Some(u), Some(k)) = (readfds, read_kern) {
        copy_fd_set_to_user(u, k);
    }
    if let (Some(u), Some(k)) = (writefds, write_kern) {
        copy_fd_set_to_user(u, k);
    }
    if let (Some(u), Some(k)) = (exceptfds, except_kern) {
        copy_fd_set_to_user(u, k);
    }
}

fn do_select(
    nfds: u32,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: Option<Duration>,
    sigmask: UserConstPtr<SignalSetWithSize>,
) -> AxResult<isize> {
    // Also enforced in `sys_select`/`sys_pselect6` before `timeout` read (issue-333).
    if nfds > __FD_SETSIZE {
        return Err(AxError::InvalidInput);
    }
    let sigmask = if let Some(sigmask) = nullable!(sigmask.get_as_ref())? {
        check_sigset_size(sigmask.sigsetsize)?;
        let set = sigmask.set;
        nullable!(set.get_as_ref())?
    } else {
        None
    };

    let readfds = nullable!(readfds.get_as_mut())?;
    let writefds = nullable!(writefds.get_as_mut())?;
    let exceptfds = nullable!(exceptfds.get_as_mut())?;

    let read_set = FdSet::new(nfds as _, readfds.as_deref());
    let write_set = FdSet::new(nfds as _, writefds.as_deref());
    let except_set = FdSet::new(nfds as _, exceptfds.as_deref());

    let mut read_kernel = readfds.is_some().then_some(empty_kernel_fd_set());
    let mut write_kernel = writefds.is_some().then_some(empty_kernel_fd_set());
    let mut except_kernel = exceptfds.is_some().then_some(empty_kernel_fd_set());

    debug!(
        "sys_select <= nfds: {nfds} sets: [read: {read_set:?}, write: {write_set:?}, except: \
         {except_set:?}] timeout: {timeout:?}"
    );

    let fd_table = FD_TABLE.read();
    let fd_bitmap = read_set.0 | write_set.0 | except_set.0;
    let fd_count = fd_bitmap.len();
    let mut fds = Vec::with_capacity(fd_count);
    let mut fd_indices = Vec::with_capacity(fd_count);
    for fd in fd_bitmap.into_iter() {
        let f = fd_table
            .get(fd)
            .ok_or(AxError::BadFileDescriptor)?
            .inner
            .clone();
        let mut events = IoEvents::empty();
        events.set(IoEvents::IN, read_set.0.get(fd));
        events.set(IoEvents::OUT, write_set.0.get(fd));
        events.set(IoEvents::ERR, except_set.0.get(fd));
        if !events.is_empty() {
            fds.push((f, events));
            fd_indices.push(fd);
        }
    }

    drop(fd_table);
    let fds = FdPollSet(fds);

    if fds.0.is_empty() {
        flush_output_fd_sets(
            readfds,
            writefds,
            exceptfds,
            read_kernel.as_ref(),
            write_kernel.as_ref(),
            except_kernel.as_ref(),
        );
        return Ok(0);
    }

    let poll_result = with_blocked_signals(sigmask.copied(), || {
        match block_on(future::timeout(
            timeout,
            poll_io(&fds, IoEvents::empty(), false, || {
                let mut res = 0usize;
                for ((fd, interested), index) in fds.0.iter().zip(fd_indices.iter().copied()) {
                    let events = fd.poll() & *interested;
                    if events.contains(IoEvents::IN) {
                        if let Some(set) = read_kernel.as_mut() {
                            res += 1;
                            unsafe { FD_SET(index as _, set) };
                        }
                    }
                    if events.contains(IoEvents::OUT) {
                        if let Some(set) = write_kernel.as_mut() {
                            res += 1;
                            unsafe { FD_SET(index as _, set) };
                        }
                    }
                    if events.contains(IoEvents::ERR) {
                        if let Some(set) = except_kernel.as_mut() {
                            res += 1;
                            unsafe { FD_SET(index as _, set) };
                        }
                    }
                }
                if res > 0 {
                    return Ok(res as _);
                }

                Err(AxError::WouldBlock)
            }),
        )) {
            Ok(r) => r,
            Err(_) => Ok(0),
        }
    });

    if poll_result.is_ok() {
        flush_output_fd_sets(
            readfds,
            writefds,
            exceptfds,
            read_kernel.as_ref(),
            write_kernel.as_ref(),
            except_kernel.as_ref(),
        );
    }

    poll_result
}

#[inline]
fn check_select_nfds(nfds: u32) -> AxResult<()> {
    if nfds > __FD_SETSIZE {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
pub fn sys_select(
    nfds: u32,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: UserConstPtr<timeval>,
) -> AxResult<isize> {
    // Linux core_sys_select: reject oversized `nfds` before copying `timeout` (EINVAL before EFAULT
    // on bad `timeout`; issue-333).
    check_select_nfds(nfds)?;
    let timeout = if timeout.is_null() {
        None
    } else {
        Some(
            read_timeval_user(timeout.address().as_usize() as *const timeval)?.try_into_time_value()?,
        )
    };
    do_select(nfds, readfds, writefds, exceptfds, timeout, 0.into())
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SignalSetWithSize {
    set: UserConstPtr<SignalSet>,
    sigsetsize: usize,
}

pub fn sys_pselect6(
    nfds: u32,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: UserConstPtr<timespec>,
    sigmask: UserConstPtr<SignalSetWithSize>,
) -> AxResult<isize> {
    // Linux: `nfds` bound before `timeout` copy (issue-333; same as `sys_select`).
    check_select_nfds(nfds)?;
    let timeout = if timeout.is_null() {
        None
    } else {
        Some(
            read_timespec_user(timeout.address().as_usize() as *const timespec)?.try_into_time_value()?,
        )
    };
    do_select(nfds, readfds, writefds, exceptfds, timeout, sigmask)
}
