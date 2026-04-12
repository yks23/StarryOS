use alloc::{
    borrow::Cow,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    task::Context,
};

use axerrno::{AxError, AxResult};
use axhal::time::{monotonic_time_nanos, wall_time_nanos};
use axpoll::{IoEvents, PollSet, Pollable};
use axsync::Mutex;
use axtask::future::{block_on, poll_io};
use lazy_static::lazy_static;
use linux_raw_sys::general::{CLOCK_MONOTONIC, TFD_TIMER_ABSTIME, itimerspec, timespec};

use crate::file::{FileLike, IoDst};

struct TimerState {
    next_deadline_nanos: Option<u64>,
    interval_nanos: u64,
    pending_expirations: u64,
}

pub struct TimerFd {
    clock_id: AtomicU32,
    non_blocking: AtomicBool,
    state: Mutex<TimerState>,
    poll_rx: PollSet,
}

impl TimerFd {
    pub fn new(clock_id: u32, nonblocking: bool) -> Arc<Self> {
        ensure_timerfd_tick();
        let this = Arc::new(Self {
            clock_id: AtomicU32::new(clock_id),
            non_blocking: AtomicBool::new(nonblocking),
            state: Mutex::new(TimerState {
                next_deadline_nanos: None,
                interval_nanos: 0,
                pending_expirations: 0,
            }),
            poll_rx: PollSet::new(),
        });
        TIMERFD_WEAKS.lock().push(Arc::downgrade(&this));
        this
    }

    fn now_nanos(&self) -> u64 {
        if self.clock_id.load(Ordering::Relaxed) == CLOCK_MONOTONIC {
            monotonic_time_nanos()
        } else {
            wall_time_nanos()
        }
    }

    pub fn process_expirations(&self) {
        let now = self.now_nanos();
        let mut guard = self.state.lock();
        let Some(deadline) = guard.next_deadline_nanos else {
            return;
        };
        if now < deadline {
            return;
        }
        guard.pending_expirations = guard.pending_expirations.saturating_add(1);
        if guard.interval_nanos > 0 {
            let mut d = deadline;
            while now >= d {
                d = d.saturating_add(guard.interval_nanos);
            }
            guard.next_deadline_nanos = Some(d);
        } else {
            guard.next_deadline_nanos = None;
        }
        drop(guard);
        self.poll_rx.wake();
    }

    fn timespec_to_nanos(ts: &timespec) -> AxResult<u64> {
        if ts.tv_sec < 0 || ts.tv_nsec < 0 || ts.tv_nsec > 999_999_999 {
            return Err(AxError::InvalidInput);
        }
        Ok((ts.tv_sec as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(ts.tv_nsec as u64))
    }

    fn nanos_to_timespec(n: u64) -> timespec {
        timespec {
            tv_sec: (n / 1_000_000_000) as _,
            tv_nsec: (n % 1_000_000_000) as _,
        }
    }

    pub fn settime(&self, flags: i32, new_value: &itimerspec) -> AxResult<()> {
        let interval = Self::timespec_to_nanos(&new_value.it_interval)?;
        let value = Self::timespec_to_nanos(&new_value.it_value)?;
        let now = self.now_nanos();
        let mut guard = self.state.lock();

        if value == 0 {
            guard.next_deadline_nanos = None;
            guard.interval_nanos = 0;
            drop(guard);
            self.poll_rx.wake();
            return Ok(());
        }

        guard.interval_nanos = interval;
        let next = if flags as u32 & TFD_TIMER_ABSTIME != 0 {
            value
        } else {
            now.saturating_add(value)
        };
        guard.next_deadline_nanos = Some(next);
        drop(guard);
        self.process_expirations();
        Ok(())
    }

    pub fn gettime(&self) -> AxResult<itimerspec> {
        self.process_expirations();
        let now = self.now_nanos();
        let guard = self.state.lock();
        let remain = match guard.next_deadline_nanos {
            None => 0,
            Some(d) if now >= d => 0,
            Some(d) => d - now,
        };
        Ok(itimerspec {
            it_interval: Self::nanos_to_timespec(guard.interval_nanos),
            it_value: Self::nanos_to_timespec(remain),
        })
    }
}

lazy_static! {
    static ref TIMERFD_WEAKS: Mutex<Vec<Weak<TimerFd>>> = Mutex::new(Vec::new());
}

static TIMERFD_CB_REGISTERED: AtomicBool = AtomicBool::new(false);

fn ensure_timerfd_tick() {
    if TIMERFD_CB_REGISTERED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    axtask::register_timer_callback(|_| {
        let mut v = TIMERFD_WEAKS.lock();
        v.retain(|w| {
            w.upgrade()
                .map(|t| {
                    t.process_expirations();
                    true
                })
                .unwrap_or(false)
        });
    });
}

impl FileLike for TimerFd {
    fn read(&self, dst: &mut IoDst) -> axio::Result<usize> {
        if dst.remaining_mut() < size_of::<u64>() {
            return Err(AxError::InvalidInput);
        }

        block_on(poll_io(self, IoEvents::IN, self.nonblocking(), || {
            self.process_expirations();
            let mut guard = self.state.lock();
            if guard.pending_expirations > 0 {
                let n = guard.pending_expirations;
                guard.pending_expirations = 0;
                drop(guard);
                dst.write(&n.to_ne_bytes())?;
                self.poll_rx.wake();
                Ok(size_of::<u64>())
            } else {
                Err(AxError::WouldBlock)
            }
        }))
    }

    fn nonblocking(&self) -> bool {
        self.non_blocking.load(Ordering::Acquire)
    }

    fn set_nonblocking(&self, nonblocking: bool) -> axio::Result {
        self.non_blocking.store(nonblocking, Ordering::Release);
        Ok(())
    }

    fn path(&self) -> Cow<'_, str> {
        "anon_inode:[timerfd]".into()
    }
}

impl Pollable for TimerFd {
    fn poll(&self) -> IoEvents {
        self.process_expirations();
        let mut events = IoEvents::empty();
        let pending = self.state.lock().pending_expirations;
        events.set(IoEvents::IN, pending > 0);
        events
    }

    fn register(&self, context: &mut Context<'_>, events: IoEvents) {
        if events.contains(IoEvents::IN) {
            self.poll_rx.register(context.waker());
        }
    }
}
