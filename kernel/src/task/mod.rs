//! User task management.

mod futex;
mod ops;
mod resources;
mod signal;
mod stat;
mod timer;
mod user;

pub(crate) use self::timer::time_value_from_nanos;

use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{
    cell::RefCell,
    ops::Deref,
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering},
};

use axerrno::{AxError, AxResult};
use axpoll::PollSet;
use axsync::{Mutex, spin::SpinNoIrq};
use axtask::{TaskExt, TaskInner};
use extern_trait::extern_trait;
use scope_local::{ActiveScope, Scope};
use spin::RwLock;
use starry_process::Process;
use starry_signal::{
    Signo,
    api::{ProcessSignalManager, SignalActions, ThreadSignalManager},
};

pub use self::{futex::*, ops::*, resources::*, signal::*, stat::*, timer::*, user::*};
use crate::mm::AddrSpace;

/// Sentinel for `setresuid` / `setresgid` / `setreuid` unused slots: Linux `(uid_t)-1` / `(gid_t)-1`.
pub const CRED_NO_CHANGE: u32 = u32::MAX;

/// Upper bound for `setgroups` / supplementary GIDs (see `NGROUPS_MAX` on Linux).
pub const SUPP_GROUPS_MAX: usize = 4096;

/// Job-control state shared by all threads in a process (`SIGSTOP` / `SIGCONT`).
#[derive(Default)]
pub struct JobCtl {
    /// Present while the process is job-stopped (still alive, not a zombie).
    pub stop_sig: Option<u8>,
    /// A `waitpid(WUNTRACED)` has not yet consumed this stop event.
    pub stop_wait_pending: bool,
    /// A `waitpid(WCONTINUED)` has not yet consumed this continue event.
    pub continued_wait_pending: bool,
}

///  A wrapper type that assumes the inner type is `Sync`.
#[repr(transparent)]
pub struct AssumeSync<T>(pub T);

unsafe impl<T> Sync for AssumeSync<T> {}

impl<T> Deref for AssumeSync<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The inner data of a thread.
pub struct Thread {
    /// The process data shared by all threads in the process.
    pub proc_data: Arc<ProcessData>,

    /// The clear thread tid field
    ///
    /// See <https://manpages.debian.org/unstable/manpages-dev/set_tid_address.2.en.html#clear_child_tid>
    ///
    /// When the thread exits, the kernel clears the word at this address if it
    /// is not NULL.
    clear_child_tid: AtomicUsize,

    /// The head of the robust list
    robust_list_head: AtomicUsize,

    /// The thread-level signal manager
    pub signal: Arc<ThreadSignalManager>,

    /// Time manager
    ///
    /// This is assumed to be `Sync` because it's only borrowed mutably during
    /// context switches, which is exclusive to the current thread.
    pub time: AssumeSync<RefCell<TimeManager>>,

    /// The OOM score adjustment value.
    oom_score_adj: AtomicI32,

    /// Ready to exit
    pub exit: Arc<AtomicBool>,

    /// Set by `rt_sigreturn` so the next return to the user loop skips one `check_signals` pass.
    skip_next_signal_check: AtomicBool,

    /// Indicates whether the thread is currently accessing user memory.
    accessing_user_memory: AtomicBool,

    /// Self exit event
    pub exit_event: Arc<PollSet>,

    /// Linux-visible scheduling policy (`SCHED_*`, e.g. `SCHED_OTHER`/`SCHED_NORMAL` == 0).
    sched_policy: AtomicI32,
    /// `sched_param.sched_priority` for this thread (persisted; real-time scheduling not wired).
    sched_priority: AtomicI32,
}

impl Thread {
    /// Create a new [`Thread`].
    pub fn new(tid: u32, proc_data: Arc<ProcessData>) -> Box<Self> {
        Box::new(Thread {
            signal: ThreadSignalManager::new(tid, proc_data.signal.clone()),
            proc_data,
            clear_child_tid: AtomicUsize::new(0),
            robust_list_head: AtomicUsize::new(0),
            time: AssumeSync(RefCell::new(TimeManager::new())),
            exit: Arc::new(AtomicBool::new(false)),
            skip_next_signal_check: AtomicBool::new(false),
            oom_score_adj: AtomicI32::new(200),
            accessing_user_memory: AtomicBool::new(false),
            exit_event: Arc::default(),
            sched_policy: AtomicI32::new(0),
            sched_priority: AtomicI32::new(0),
        })
    }

    /// Get the clear child tid field.
    pub fn clear_child_tid(&self) -> usize {
        self.clear_child_tid.load(Ordering::Relaxed)
    }

    /// Set the clear child tid field.
    pub fn set_clear_child_tid(&self, clear_child_tid: usize) {
        self.clear_child_tid
            .store(clear_child_tid, Ordering::Relaxed);
    }

    /// Get the robust list head.
    pub fn robust_list_head(&self) -> usize {
        self.robust_list_head.load(Ordering::SeqCst)
    }

    /// Set the robust list head.
    pub fn set_robust_list_head(&self, robust_list_head: usize) {
        self.robust_list_head
            .store(robust_list_head, Ordering::SeqCst);
    }

    /// Get the oom score adjustment value.
    pub fn oom_score_adj(&self) -> i32 {
        self.oom_score_adj.load(Ordering::SeqCst)
    }

    /// Set the oom score adjustment value.
    pub fn set_oom_score_adj(&self, value: i32) {
        self.oom_score_adj.store(value, Ordering::SeqCst);
    }

    /// Check if the thread is ready to exit.
    pub fn pending_exit(&self) -> bool {
        self.exit.load(Ordering::Acquire)
    }

    /// Set the thread to exit.
    pub fn set_exit(&self) {
        self.exit.store(true, Ordering::Release);
    }

    /// Check if the thread is accessing user memory.
    pub fn is_accessing_user_memory(&self) -> bool {
        self.accessing_user_memory.load(Ordering::Acquire)
    }

    /// Set the accessing user memory flag.
    pub fn set_accessing_user_memory(&self, accessing: bool) {
        self.accessing_user_memory
            .store(accessing, Ordering::Release);
    }

    /// Current `sched_getscheduler` policy (`SCHED_*`).
    pub fn sched_policy(&self) -> i32 {
        self.sched_policy.load(Ordering::Relaxed)
    }

    /// Current `sched_param.sched_priority`.
    pub fn sched_priority_value(&self) -> i32 {
        self.sched_priority.load(Ordering::Relaxed)
    }

    /// Set policy and priority from `sched_setscheduler` (stored only; axtask RR is unchanged).
    pub fn set_sched_policy_param(&self, policy: i32, priority: i32) {
        self.sched_policy.store(policy, Ordering::Relaxed);
        self.sched_priority.store(priority, Ordering::Relaxed);
    }
}

#[extern_trait]
impl TaskExt for Box<Thread> {
    fn on_enter(&self) {
        let scope = self.proc_data.scope.read();
        unsafe { ActiveScope::set(&scope) };
        core::mem::forget(scope);
    }

    fn on_leave(&self) {
        ActiveScope::set_global();
        unsafe { self.proc_data.scope.force_read_decrement() };
    }
}

/// Helper trait to access the thread from a task.
pub trait AsThread {
    /// Try to get the thread from the task.
    fn try_as_thread(&self) -> Option<&Thread>;

    /// Get the thread from the task, panicking if it is a kernel task.
    fn as_thread(&self) -> &Thread {
        self.try_as_thread().expect("kernel task")
    }
}

impl AsThread for TaskInner {
    fn try_as_thread(&self) -> Option<&Thread> {
        self.task_ext()
            .map(|ext| ext.downcast_ref::<Box<Thread>>().as_ref())
    }
}

/// [`Process`]-shared data.
pub struct ProcessData {
    /// The process.
    pub proc: Arc<Process>,
    /// The executable path
    pub exe_path: RwLock<String>,
    /// The command line arguments
    pub cmdline: RwLock<Arc<Vec<String>>>,
    /// The virtual memory address space.
    // TODO: scopify
    pub aspace: Arc<RwLock<AddrSpace>>,
    /// The resource scope
    pub scope: RwLock<Scope>,
    /// The user heap top
    heap_top: AtomicUsize,

    /// The resource limits
    pub rlim: RwLock<Rlimits>,

    /// The child exit wait event
    pub child_exit_event: Arc<PollSet>,
    /// Self exit event
    pub exit_event: Arc<PollSet>,
    /// The exit signal of the thread
    pub exit_signal: Option<Signo>,

    /// Job control (`waitpid` stopped / continued).
    pub jobctl: Mutex<JobCtl>,

    /// The process signal manager
    pub signal: Arc<ProcessSignalManager>,

    /// The futex table.
    futex_table: Arc<FutexTable>,

    /// The default mask for file permissions.
    umask: AtomicU32,

    /// Real / effective / saved user-IDs (`setresuid` / `getuid` family).
    ruid: AtomicU32,
    euid: AtomicU32,
    suid: AtomicU32,
    /// Real / effective / saved group-IDs (`setresgid` / `getgid` family).
    rgid: AtomicU32,
    egid: AtomicU32,
    sgid: AtomicU32,

    /// Supplementary group IDs (`getgroups` / `setgroups`); excludes primary `rgid`.
    supplementary_gids: Mutex<Vec<u32>>,

    /// Linux capability sets (`capget` / `capset`), version-3 lower 32 bits each.
    /// Default all bits set to match prior stub behavior; `capset` can clear.
    cap_effective: AtomicU32,
    cap_permitted: AtomicU32,
    cap_inheritable: AtomicU32,

    /// Process nice (`getpriority` / `setpriority`, range -20..=19 on Linux; default 0).
    nice: AtomicI32,

    /// Sum of exited threads' user/system CPU time (nanoseconds).
    exited_threads_utime_ns: AtomicUsize,
    exited_threads_stime_ns: AtomicUsize,
    /// Cumulative user/system CPU of children reaped via `wait` (nanoseconds).
    child_utime_ns: AtomicUsize,
    child_stime_ns: AtomicUsize,
}

impl ProcessData {
    /// Create a new [`ProcessData`].
    pub fn new(
        proc: Arc<Process>,
        exe_path: String,
        cmdline: Arc<Vec<String>>,
        aspace: Arc<RwLock<AddrSpace>>,
        signal_actions: Arc<SpinNoIrq<SignalActions>>,
        exit_signal: Option<Signo>,
    ) -> Arc<Self> {
        Arc::new(Self {
            proc,
            exe_path: RwLock::new(exe_path),
            cmdline: RwLock::new(cmdline),
            aspace,
            scope: RwLock::new(Scope::new()),
            heap_top: AtomicUsize::new(crate::config::USER_HEAP_BASE),

            rlim: RwLock::default(),

            child_exit_event: Arc::default(),
            exit_event: Arc::default(),
            exit_signal,

            jobctl: Mutex::new(JobCtl::default()),

            signal: Arc::new(ProcessSignalManager::new(
                signal_actions,
                crate::config::SIGNAL_TRAMPOLINE,
            )),

            futex_table: Arc::new(FutexTable::new()),

            umask: AtomicU32::new(0o022),

            ruid: AtomicU32::new(0),
            euid: AtomicU32::new(0),
            suid: AtomicU32::new(0),
            rgid: AtomicU32::new(0),
            egid: AtomicU32::new(0),
            sgid: AtomicU32::new(0),

            supplementary_gids: Mutex::new(Vec::new()),

            cap_effective: AtomicU32::new(u32::MAX),
            cap_permitted: AtomicU32::new(u32::MAX),
            cap_inheritable: AtomicU32::new(u32::MAX),

            nice: AtomicI32::new(0),

            exited_threads_utime_ns: AtomicUsize::new(0),
            exited_threads_stime_ns: AtomicUsize::new(0),
            child_utime_ns: AtomicUsize::new(0),
            child_stime_ns: AtomicUsize::new(0),
        })
    }

    /// Copy r/e/s uid and gid from a parent process (fork / vfork child).
    pub fn copy_credentials_from(&self, parent: &ProcessData) {
        let o = Ordering::SeqCst;
        self.ruid.store(parent.ruid.load(o), o);
        self.euid.store(parent.euid.load(o), o);
        self.suid.store(parent.suid.load(o), o);
        self.rgid.store(parent.rgid.load(o), o);
        self.egid.store(parent.egid.load(o), o);
        self.sgid.store(parent.sgid.load(o), o);

        let pg = parent.supplementary_gids.lock();
        let mut cg = self.supplementary_gids.lock();
        cg.clear();
        cg.extend_from_slice(&pg);

        self.nice
            .store(parent.nice.load(Ordering::SeqCst), Ordering::SeqCst);

        self.cap_effective
            .store(parent.cap_effective.load(o), o);
        self.cap_permitted
            .store(parent.cap_permitted.load(o), o);
        self.cap_inheritable
            .store(parent.cap_inheritable.load(o), o);

        self.exited_threads_utime_ns.store(0, o);
        self.exited_threads_stime_ns.store(0, o);
        self.child_utime_ns.store(0, o);
        self.child_stime_ns.store(0, o);
    }

    #[inline]
    pub fn get_nice(&self) -> i32 {
        self.nice.load(Ordering::Relaxed)
    }

    pub fn set_nice(&self, value: i32) {
        self.nice.store(value, Ordering::Relaxed);
    }

    #[inline]
    pub fn getuid(&self) -> u32 {
        self.ruid.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn geteuid(&self) -> u32 {
        self.euid.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn getgid(&self) -> u32 {
        self.rgid.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn getegid(&self) -> u32 {
        self.egid.load(Ordering::SeqCst)
    }

    /// Real, effective, and saved user-IDs (`getresuid(2)`).
    #[inline]
    pub fn get_resuid(&self) -> (u32, u32, u32) {
        let o = Ordering::SeqCst;
        (
            self.ruid.load(o),
            self.euid.load(o),
            self.suid.load(o),
        )
    }

    /// Real, effective, and saved group-IDs (`getresgid(2)`).
    #[inline]
    pub fn get_resgid(&self) -> (u32, u32, u32) {
        let o = Ordering::SeqCst;
        (
            self.rgid.load(o),
            self.egid.load(o),
            self.sgid.load(o),
        )
    }

    fn cred_change_allowed(new: u32, r: u32, e: u32, s: u32) -> bool {
        new == r || new == e || new == s
    }

    /// Linux-like `setresuid`: `CRED_NO_CHANGE` leaves that component unchanged.
    pub fn setresuid(&self, req_r: u32, req_e: u32, req_s: u32) -> AxResult<()> {
        let old_r = self.ruid.load(Ordering::SeqCst);
        let old_e = self.euid.load(Ordering::SeqCst);
        let old_s = self.suid.load(Ordering::SeqCst);

        if old_e != 0 {
            if req_r != CRED_NO_CHANGE && !Self::cred_change_allowed(req_r, old_r, old_e, old_s) {
                return Err(AxError::PermissionDenied);
            }
            if req_e != CRED_NO_CHANGE && !Self::cred_change_allowed(req_e, old_r, old_e, old_s) {
                return Err(AxError::PermissionDenied);
            }
            if req_s != CRED_NO_CHANGE && !Self::cred_change_allowed(req_s, old_r, old_e, old_s) {
                return Err(AxError::PermissionDenied);
            }
        }

        let o = Ordering::SeqCst;
        if req_r != CRED_NO_CHANGE {
            self.ruid.store(req_r, o);
        }
        if req_e != CRED_NO_CHANGE {
            self.euid.store(req_e, o);
        }
        if req_s != CRED_NO_CHANGE {
            self.suid.store(req_s, o);
        }
        Ok(())
    }

    /// Linux-like `setresgid`.
    pub fn setresgid(&self, req_r: u32, req_e: u32, req_s: u32) -> AxResult<()> {
        let old_r = self.rgid.load(Ordering::SeqCst);
        let old_e = self.egid.load(Ordering::SeqCst);
        let old_s = self.sgid.load(Ordering::SeqCst);

        if old_e != 0 {
            if req_r != CRED_NO_CHANGE && !Self::cred_change_allowed(req_r, old_r, old_e, old_s) {
                return Err(AxError::PermissionDenied);
            }
            if req_e != CRED_NO_CHANGE && !Self::cred_change_allowed(req_e, old_r, old_e, old_s) {
                return Err(AxError::PermissionDenied);
            }
            if req_s != CRED_NO_CHANGE && !Self::cred_change_allowed(req_s, old_r, old_e, old_s) {
                return Err(AxError::PermissionDenied);
            }
        }

        let o = Ordering::SeqCst;
        if req_r != CRED_NO_CHANGE {
            self.rgid.store(req_r, o);
        }
        if req_e != CRED_NO_CHANGE {
            self.egid.store(req_e, o);
        }
        if req_s != CRED_NO_CHANGE {
            self.sgid.store(req_s, o);
        }
        Ok(())
    }

    /// Supplementary groups only (not including primary real GID).
    pub fn get_supplementary_groups(&self) -> Vec<u32> {
        self.supplementary_gids.lock().clone()
    }

    /// Replace supplementary groups. Requires effective uid0 (CAP_SETGID not modeled).
    pub fn set_supplementary_groups(&self, gids: &[u32]) -> AxResult<()> {
        if self.geteuid() != 0 {
            return Err(AxError::PermissionDenied);
        }
        if gids.len() > SUPP_GROUPS_MAX {
            return Err(AxError::InvalidInput);
        }
        let mut g = self.supplementary_gids.lock();
        g.clear();
        g.extend_from_slice(gids);
        Ok(())
    }

    /// Get the top address of the user heap.
    pub fn get_heap_top(&self) -> usize {
        self.heap_top.load(Ordering::Acquire)
    }

    /// Set the top address of the user heap.
    pub fn set_heap_top(&self, top: usize) {
        self.heap_top.store(top, Ordering::Release)
    }

    /// Linux manual: A "clone" child is one which delivers no signal, or a
    /// signal other than SIGCHLD to its parent upon termination.
    pub fn is_clone_child(&self) -> bool {
        self.exit_signal != Some(Signo::SIGCHLD)
    }

    /// Get the umask.
    pub fn umask(&self) -> u32 {
        self.umask.load(Ordering::SeqCst)
    }

    /// Set the umask.
    pub fn set_umask(&self, umask: u32) {
        self.umask.store(umask, Ordering::SeqCst);
    }

    /// Set the umask and return the old value.
    pub fn replace_umask(&self, umask: u32) -> u32 {
        self.umask.swap(umask, Ordering::SeqCst)
    }

    /// Returns `(effective, permitted, inheritable)` capability masks (lower 32 bits).
    pub fn get_capabilities(&self) -> (u32, u32, u32) {
        let o = Ordering::SeqCst;
        (
            self.cap_effective.load(o),
            self.cap_permitted.load(o),
            self.cap_inheritable.load(o),
        )
    }

    /// Applies `capset(2)` data for this process. Enforces `effective`/`inheritable` ⊆ `permitted`.
    /// Requires effective uid 0 or `CAP_SETPCAP` in the current effective set.
    pub fn set_capabilities(&self, eff: u32, perm: u32, inh: u32) -> AxResult<()> {
        const CAP_SETPCAP: u32 = 1 << 8;

        if eff & !perm != 0 || inh & !perm != 0 {
            return Err(AxError::InvalidInput);
        }

        let euid = self.euid.load(Ordering::SeqCst);
        let cur_eff = self.cap_effective.load(Ordering::SeqCst);
        if euid != 0 && (cur_eff & CAP_SETPCAP) == 0 {
            return Err(AxError::PermissionDenied);
        }

        let o = Ordering::SeqCst;
        self.cap_effective.store(eff, o);
        self.cap_permitted.store(perm, o);
        self.cap_inheritable.store(inh, o);
        Ok(())
    }

    /// Called when a thread exits: fold its CPU time into process-wide exited totals.
    pub fn accumulate_exited_thread_cpu_ns(&self, utime_ns: usize, stime_ns: usize) {
        self.exited_threads_utime_ns
            .fetch_add(utime_ns, Ordering::SeqCst);
        self.exited_threads_stime_ns
            .fetch_add(stime_ns, Ordering::SeqCst);
    }

    /// Linux `times` / `wait4` child CPU: waited-for zombie's thread-group time.
    pub fn accumulate_waited_child_cpu_ns(&self, utime_ns: usize, stime_ns: usize) {
        self.child_utime_ns.fetch_add(utime_ns, Ordering::SeqCst);
        self.child_stime_ns.fetch_add(stime_ns, Ordering::SeqCst);
    }

    /// Cumulative waited-children CPU (nanoseconds), for `times` / `getrusage` child fields.
    pub fn waited_children_cpu_nanos(&self) -> (usize, usize) {
        let o = Ordering::Relaxed;
        (
            self.child_utime_ns.load(o),
            self.child_stime_ns.load(o),
        )
    }

    /// Thread-group CPU (nanoseconds): exited threads plus all live threads in this process.
    pub fn thread_group_cpu_nanos(&self) -> (usize, usize) {
        let mut ut = self.exited_threads_utime_ns.load(Ordering::Relaxed);
        let mut st = self.exited_threads_stime_ns.load(Ordering::Relaxed);
        for tid in self.proc.threads() {
            if let Ok(task) = get_task(tid) {
                if let Some(thr) = task.try_as_thread() {
                    if thr.proc_data.proc.pid() == self.proc.pid() {
                        let (u, s) = thr.time.borrow().cpu_nanos();
                        ut += u;
                        st += s;
                    }
                }
            }
        }
        (ut, st)
    }
}
