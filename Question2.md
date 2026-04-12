# Starry OS Linux Syscall 支持能力与缺陷的源码级分析

## 一、Syscall 分发机制

### 1.1 入口与路由

所有 syscall 的入口是 `kernel/src/syscall/mod.rs` 中的 `handle_syscall` 函数。用户态通过架构 trap 指令（riscv64: `ecall`, x86_64: `syscall`）陷入内核后，经 `axhal` 的 trap handler 进入用户态主循环 `task/user.rs`，在 `ReturnReason::Syscall` 分支调用此函数。

```rust
// task/user.rs:30-31
match reason {
    ReturnReason::Syscall => handle_syscall(&mut uctx),
    // ...
}
```

`handle_syscall` 内部是一个约 600 行的 `match sysno { ... }` 巨型分发表，直接将 `syscalls::Sysno` 枚举匹配到对应的 `sys_*` 实现函数：

```rust
// syscall/mod.rs:22-640
pub fn handle_syscall(uctx: &mut UserContext) {
    let Some(sysno) = Sysno::new(uctx.sysno()) else {
        warn!("Invalid syscall number: {}", uctx.sysno());
        uctx.set_retval(-LinuxError::ENOSYS.code() as _);
        return;
    };
    let result = match sysno {
        Sysno::read => sys_read(uctx.arg0() as _, uctx.arg1() as _, uctx.arg2() as _),
        // ...约 160 个 match arm...
        _ => {
            warn!("Unimplemented syscall: {sysno}");
            Err(AxError::Unsupported)  // 未实现的返回 ENOSYS
        }
    };
    uctx.set_retval(result.unwrap_or_else(|err| -LinuxError::from(err).code() as _) as _);
}
```

### 1.2 ABI 兼容性保证

- **Syscall 编号**：使用 `syscalls` crate 的 `Sysno` 枚举，与 Linux 标准编号完全一致
- **参数传递**：通过 `uctx.arg0()` ~ `uctx.arg5()` 按架构约定取寄存器值
- **返回值**：成功返回 `Ok(isize)`，失败通过 `AxError` → `LinuxError` 映射为 Linux 标准负 errno
- **数据结构**：使用 `linux_raw_sys` crate 引入 Linux 原生结构体定义（`stat`, `timespec`, `sigaction` 等），确保二进制布局完全兼容
- **用户内存访问**：通过 `starry_vm` crate 的 `VmPtr` / `VmMutPtr` trait 安全读写用户态指针

### 1.3 架构特定处理

Starry 正确处理了不同架构上 Linux syscall 表的差异：

| 特性 | x86_64 | riscv64 / aarch64 / loongarch64 |
|------|--------|-------------------------------|
| `open` | ✅ 存在（映射到 `sys_openat`） | ❌ 不存在（只有 `openat`） |
| `stat` / `lstat` | ✅ 存在 | ❌ 不存在（只有 `fstatat`） |
| `fork` | ✅ 存在（映射到 `sys_clone`） | ❌ 不存在（只有 `clone`/`clone3`） |
| `pipe` | ✅ 存在（映射到 `sys_pipe2`） | ❌ 不存在（只有 `pipe2`） |
| `poll` / `select` | ✅ 存在 | ❌ 不存在（只有 `ppoll`/`pselect6`） |
| `arch_prctl` | ✅ x86_64 特有 | ❌ |
| `riscv_flush_icache` | ❌ | ✅ riscv64 特有 |

这些通过 `#[cfg(target_arch = "...")]` 条件编译控制，与 Linux 真实行为一致。

---

## 二、已实现 Syscall 全景分类

### 2.1 文件系统（~50 个）

**路径操作**（`syscall/fs/ctl.rs`）：

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `ioctl` | ⚠️ 部分 | 仅支持终端 ioctl（TCGETS/TCSETS/TIOCGPGRP/TIOCGWINSZ 等），其他设备 ioctl 缺失 |
| `chdir` / `fchdir` | ✅ 完整 | 使用 `FS_CONTEXT` 修改当前工作目录 |
| `chroot` | ✅ 完整 | |
| `mkdirat` | ✅ 完整 | |
| `getdents64` | ✅ 完整 | |
| `linkat` / `unlinkat` | ✅ 完整 | |
| `symlinkat` / `readlinkat` | ✅ 完整 | |
| `getcwd` | ✅ 完整 | |
| `renameat2` | ✅ 完整 | |
| `sync` / `syncfs` | ✅ 完整 | |
| `fchownat` / `fchmodat` | ✅ 完整 | 但无真实权限模型，不实际拒绝操作 |
| `utimensat` | ✅ 完整 | |

**文件描述符操作**（`syscall/fs/fd_ops.rs`）：

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `openat` | ✅ 完整 | 支持 O_CREAT, O_TRUNC, O_APPEND, O_NONBLOCK, O_CLOEXEC, O_DIRECTORY, O_NOFOLLOW, O_DIRECT, O_PATH |
| `close` / `close_range` | ✅ 完整 | `close_range` 支持 CLOEXEC 和 UNSHARE 标志 |
| `dup` / `dup3` | ✅ 完整 | |
| `fcntl` | ⚠️ 部分 | F_DUPFD, F_GETFL/SETFL, F_GETFD/SETFD, F_GETPIPE_SZ/SETPIPE_SZ 已实现；**F_SETLK/SETLKW 是 no-op**（不做文件锁） |
| `flock` | 🔴 stub | 直接返回 `Ok(0)`，不实际加锁 |

**I/O 读写**（`syscall/fs/io.rs`）：

| Syscall | 实现质量 |
|---------|---------|
| `read` / `write` | ✅ 完整 |
| `readv` / `writev` | ✅ 完整 |
| `pread64` / `pwrite64` | ✅ 完整 |
| `preadv` / `pwritev` / `preadv2` / `pwritev2` | ✅ 完整 |
| `lseek` | ✅ 完整 |
| `truncate` / `ftruncate` | ✅ 完整 |
| `fallocate` | ✅ 完整 |
| `fsync` / `fdatasync` | ✅ 完整 |
| `fadvise64` | ✅ 完整 |
| `sendfile` | ✅ 完整 |
| `copy_file_range` | ✅ 完整 |
| `splice` | ✅ 完整 |

**文件属性**（`syscall/fs/stat.rs`）：

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `fstat` / `fstatat` | ✅ 完整 | 返回完整 `struct stat`，字段逐一映射 |
| `statx` | ✅ 完整 | |
| `faccessat2` | ✅ 完整 | |
| `statfs` / `fstatfs` | ✅ 完整 | |

**特殊文件**（`syscall/fs/` 各文件）：

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `pipe2` | ✅ 完整 | |
| `eventfd2` | ✅ 完整 | |
| `memfd_create` | ✅ 完整 | |
| `signalfd4` | ✅ 完整 | |
| `pidfd_open` / `pidfd_getfd` / `pidfd_send_signal` | ✅ 完整 | |
| `mount` / `umount2` | ⚠️ 部分 | 支持的文件系统类型有限 |

### 2.2 内存管理（~10 个）

源码位于 `syscall/mm/`。

| Syscall | 实现质量 | 源码分析 |
|---------|---------|---------|
| `brk` | ✅ 完整 | `brk.rs` — 调整 heap_top，map/unmap 对应区域 |
| `mmap` | ✅ 基本完整 | `mmap.rs` — 支持 MAP_PRIVATE(CoW)/SHARED/ANONYMOUS/FIXED/POPULATE/HUGETLB；文件映射支持 Cached 和 Direct 后端 |
| `munmap` | ✅ 完整 | |
| `mprotect` | ✅ 完整 | 但 PROT_GROWSUP/PROT_GROWSDOWN 未实现 |
| `mremap` | 🔴 粗糙 | 通过 mmap+memcpy+munmap 实现，**不是页表重映射**；丢失原映射后端类型；不处理 MREMAP_MAYMOVE/MREMAP_FIXED |
| `madvise` | 🔴 no-op | 直接返回 `Ok(0)`，不处理任何 advice |
| `msync` | 🔴 no-op | 直接返回 `Ok(0)`，不同步脏页 |
| `mlock` / `mlock2` | 🔴 no-op | 直接返回 `Ok(0)` |
| `mincore` | ✅ 完整 | |

### 2.3 进程与线程管理（~20 个）

**创建与执行**（`syscall/task/`）：

| Syscall | 实现质量 | 源码分析 |
|---------|---------|---------|
| `clone` | ✅ 完整 | `clone.rs` — 支持 CLONE_VM/FILES/SIGHAND/THREAD/VFORK/PARENT/PIDFD/SETTLS/CHILD_CLEARTID/CHILD_SETTID/CLEAR_SIGHAND 等，namespace 标志仅 warn 但不拒绝 |
| `clone3` | ✅ 完整 | `clone3.rs` — 解析 `struct clone_args`，委托给 `do_clone` |
| `execve` | ⚠️ 部分 | `execve.rs` — ELF 加载完整（含动态链接器和 shebang），但**多线程进程中 execve 直接报错** |
| `exit` / `exit_group` | ✅ 完整 | `exit.rs` — `exit_group` 向同进程所有线程发 SIGKILL |
| `wait4` | ✅ 基本完整 | `wait.rs` — 支持 WNOHANG/WNOWAIT；**但 WUNTRACED/WCONTINUED 定义了但不会触发**（因为 SIGSTOP 未实现） |

**线程/进程信息**（`syscall/task/thread.rs`, `ctl.rs`）：

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `getpid` / `getppid` / `gettid` | ✅ 完整 | |
| `set_tid_address` | ✅ 完整 | |
| `prctl` | ⚠️ 部分 | 仅支持 PR_SET_NAME/PR_GET_NAME/PR_SET_SECCOMP(no-op)/PR_MCE_KILL(no-op) |
| `capget` / `capset` | 🔴 stub | `capget` 返回全部 capability = MAX，`capset` 直接 Ok |
| `umask` | ✅ 完整 | |
| `setreuid` / `setresuid` / `setresgid` | 🔴 stub | 直接返回 `Ok(0)` |
| `get_mempolicy` | 🔴 stub | 直接返回 `Ok(0)` |

**进程组与会话**（`syscall/task/job.rs`）：

| Syscall | 实现质量 |
|---------|---------|
| `getsid` / `setsid` | ✅ 完整 |
| `getpgid` / `setpgid` | ✅ 完整 |

### 2.4 信号（~12 个）

源码位于 `syscall/signal.rs`。

| Syscall | 实现质量 | 源码分析 |
|---------|---------|---------|
| `rt_sigaction` | ✅ 完整 | 正确拒绝 SIGKILL/SIGSTOP 的 handler 注册 |
| `rt_sigprocmask` | ✅ 完整 | 支持 SIG_BLOCK/SIG_UNBLOCK/SIG_SETMASK |
| `rt_sigpending` | ✅ 完整 | |
| `rt_sigreturn` | ✅ 完整 | 调用 `block_next_signal()` 后恢复用户上下文 |
| `rt_sigtimedwait` | ✅ 完整 | 可阻塞等待指定信号集，支持超时 |
| `rt_sigsuspend` | ✅ 完整 | 临时替换信号掩码并挂起，始终返回 EINTR |
| `kill` | ✅ 完整 | 支持 pid>0（指定进程）、pid=0（当前进程组）、pid=-1（所有进程）、pid<-1（指定进程组） |
| `tkill` / `tgkill` | ✅ 完整 | |
| `rt_sigqueueinfo` / `rt_tgsigqueueinfo` | ✅ 完整 | |
| `sigaltstack` | ✅ 完整 | |

**关键缺陷**：信号**投递后的 OS 默认动作**有问题（`task/signal.rs:21-39`）：
- `Stop` 动作（SIGSTOP/SIGTSTP/SIGTTIN/SIGTTOU）→ **直接 `do_exit(1, true)` 杀死进程**
- `Continue` 动作（SIGCONT）→ **什么都不做**
- `CoreDump` 动作 → 不生成 core dump，仅以 128+signo 退出

### 2.5 调度（~8 个）

源码位于 `syscall/task/schedule.rs`。

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `sched_yield` | ✅ 完整 | |
| `nanosleep` / `clock_nanosleep` | ✅ 完整 | 支持 CLOCK_REALTIME/CLOCK_MONOTONIC，支持 TIMER_ABSTIME |
| `sched_getaffinity` | ⚠️ 部分 | 仅支持 pid=0（当前线程） |
| `sched_setaffinity` | ⚠️ 部分 | 同上 |
| `sched_getscheduler` | 🔴 stub | 硬编码返回 SCHED_RR |
| `sched_setscheduler` | 🔴 stub | 直接 Ok(0) |
| `sched_getparam` | 🔴 stub | 直接 Ok(0) |
| `getpriority` | ⚠️ 部分 | 始终返回 20，不支持 `setpriority` |

### 2.6 网络（~15 个）

源码位于 `syscall/net/`。

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `socket` | ✅ 完整 | 支持 AF_INET(TCP/UDP) + AF_UNIX(STREAM/DGRAM) + AF_VSOCK(可选) |
| `socketpair` | ✅ 完整 | |
| `bind` / `connect` / `listen` | ✅ 完整 | |
| `accept` / `accept4` | ✅ 完整 | |
| `shutdown` | ✅ 完整 | |
| `sendto` / `recvfrom` | ✅ 完整 | |
| `sendmsg` / `recvmsg` | ✅ 完整 | |
| `getsockname` / `getpeername` | ✅ 完整 | |
| `getsockopt` / `setsockopt` | ⚠️ 部分 | 具体支持的选项取决于 `axnet` 实现 |

**缺失**：`recvmmsg` / `sendmmsg`（批量收发）、`AF_INET6`（IPv6）。

### 2.7 I/O 多路复用（~7 个）

| Syscall | 实现质量 |
|---------|---------|
| `ppoll` | ✅ 完整 |
| `pselect6` | ✅ 完整 |
| `epoll_create1` | ✅ 完整 |
| `epoll_ctl` | ✅ 完整 |
| `epoll_pwait` / `epoll_pwait2` | ✅ 完整 |

### 2.8 IPC（~8 个）

**System V 消息队列**（`syscall/ipc/msg.rs`）：`msgget` / `msgsnd` / `msgrcv` / `msgctl` — ✅ 完整实现，包含权限检查、消息优先级、IPC_RMID 等。

**System V 共享内存**（`syscall/ipc/shm.rs`）：`shmget` / `shmat` / `shmdt` / `shmctl` — ✅ 完整实现，包含物理页共享、引用计数、进程退出时的清理。

**缺失**：`semget` / `semop` / `semctl`（System V 信号量）完全未实现。

### 2.9 同步原语（~3 个）

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `futex` | ✅ 基本完整 | 支持 FUTEX_WAIT/WAKE/WAIT_BITSET/WAKE_BITSET/REQUEUE/CMP_REQUEUE；区分 private/shared futex；支持 robust list |
| `get_robust_list` / `set_robust_list` | ✅ 完整 | |
| `membarrier` | 🔴 语义错误 | 仅用 `compiler_fence`，多核场景不正确（详见 Question1） |

**futex 缺失的操作**：`FUTEX_LOCK_PI` / `FUTEX_UNLOCK_PI`（优先级继承）、`FUTEX_WAIT_REQUEUE_PI` 等高级变体。

### 2.10 时间（~6 个）

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `clock_gettime` | ✅ 完整 | |
| `clock_getres` | ✅ 完整 | |
| `gettimeofday` | ✅ 完整 | |
| `times` | ✅ 完整 | |
| `getitimer` / `setitimer` | ⚠️ 部分 | ITIMER_REAL 工作正常；ITIMER_VIRTUAL / ITIMER_PROF 依赖用户/内核态计时准确性（`timer.rs` 注释 `TODO: preempting does not change the timer state currently`） |

### 2.11 系统信息（~10 个）

| Syscall | 实现质量 | 备注 |
|---------|---------|------|
| `uname` | ✅ 完整 | 报告 `sysname="Linux"`, `release="10.0.0"`, `machine=ARCH` |
| `sysinfo` | ⚠️ 部分 | 仅填充 `procs` 和 `mem_unit`，其他字段（totalram/freeram/uptime 等）全为 0 |
| `getrandom` | ✅ 完整 | 通过读取 `/dev/urandom` 或 `/dev/random` 实现 |
| `getuid`/`geteuid`/`getgid`/`getegid` | 🔴 stub | 固定返回 0（root） |
| `setuid` / `setgid` | 🔴 stub | 直接 Ok(0)，不修改任何状态 |
| `getgroups` | 🔴 stub | 固定返回 `[0]` |
| `setgroups` | 🔴 stub | 直接 Ok(0) |
| `seccomp` | 🔴 stub | 直接 Ok(0)，不建立任何安全策略 |
| `prlimit64` | ✅ 完整 | 支持查询和设置资源限制 |
| `getrusage` | ✅ 完整 | 返回用户/系统时间 |

### 2.12 Dummy / Stub 入口

以下 syscall 有入口但**返回假数据或什么都不做**：

```rust
// syscall/mod.rs:617-631
// 返回一个永远不触发事件的 dummy fd
Sysno::timerfd_create | Sysno::fanotify_init | Sysno::inotify_init1
| Sysno::userfaultfd | Sysno::perf_event_open | Sysno::io_uring_setup
| Sysno::bpf | Sysno::fsopen | Sysno::fspick | Sysno::open_tree
| Sysno::memfd_secret => sys_dummy_fd(sysno),

// 直接返回 0，不做任何操作
Sysno::timer_create | Sysno::timer_gettime | Sysno::timer_settime => Ok(0),
```

---

## 三、完全缺失的重要 Syscall

通过对比 Linux syscall 全表与 `handle_syscall` 的 match arms，以下是**落入 `_` default arm（返回 ENOSYS）的重要 syscall**：

| 缺失 Syscall | 类别 | 影响评估 |
|-------------|------|---------|
| `waitid` | 进程管理 | 比 `wait4` 更灵活的等待接口，部分程序使用 |
| `execveat` | 进程管理 | `fexecve()` 的内核支持 |
| `setpriority` | 调度 | `nice` / `renice` 命令 |
| `timerfd_settime` / `timerfd_gettime` | 定时器 | 配合 timerfd_create 使用，当前 timerfd 完全不可用 |
| `timer_delete` / `timer_getoverrun` | 定时器 | POSIX timer 管理 |
| `inotify_add_watch` / `inotify_rm_watch` | 文件监控 | 文件变化通知 |
| `semget` / `semop` / `semctl` | IPC | System V 信号量 |
| `ptrace` | 调试 | strace/gdb 的基础 |
| `clock_settime` | 时间 | NTP 等时间同步 |
| `recvmmsg` / `sendmmsg` | 网络 | 批量收发优化 |
| `io_uring_enter` / `io_uring_register` | 异步I/O | 现代高性能 I/O |
| `personality` | 兼容性 | 执行域设置 |
| `mq_open` / `mq_send` 等 | IPC | POSIX 消息队列 |
| `sched_get_priority_max` / `min` | 调度 | 查询优先级范围 |
| `getresuid` / `getresgid` | 权限 | 查询真实/有效/保存 UID |

---

## 四、Syscall 优先实现排序

### 排序原则

1. **依赖链底层优先**：被其他 syscall 或用户态库间接依赖的优先
2. **实际程序触发频率优先**：BusyBox/Alpine 包管理器/shell 脚本中高频使用的优先
3. **当前实现的危害性**：dummy/no-op 实现比 ENOSYS 更危险（程序误以为成功但行为错误），修复优先
4. **实现成本与收益比**：低成本高收益优先

### 排序结果

#### 第一梯队：修复「假成功」—— 静默 no-op 比 ENOSYS 更有害

| 优先级 | Syscall | 当前状态 | 修复理由 |
|--------|---------|---------|---------|
| **1** | `timerfd_create` + `timerfd_settime` + `timerfd_gettime` | dummy fd + 缺失 | 几乎所有 event loop 框架依赖 timerfd。当前返回一个永不触发的 fd，程序不报错但定时器静默失效——**这比返回 ENOSYS 更糟糕**，因为程序无法 fallback。需要实现 `TimerFd` 结构体+三个 syscall |
| **2** | `timer_create` + `timer_settime` + `timer_gettime` + `timer_delete` | 直接 Ok(0) | POSIX 定时器。`timer_create` 返回 0 但不创建任何东西；后续 `timer_settime` 设置的定时器**永远不会触发信号**。musl libc 的某些 `sleep()` 路径依赖它 |
| **3** | `flock` | 直接 Ok(0) | 文件锁。包管理器（apk/dpkg）、数据库、日志系统用它做互斥。当前假装加锁成功，多进程并发时**可能导致数据损坏** |
| **4** | `fcntl(F_SETLK/F_SETLKW)` | 直接 Ok(0) | 记录锁。与 `flock` 同理，SQLite 等数据库依赖它 |
| **5** | `sched_setscheduler` / `sched_getparam` | 直接 Ok(0) | 调度策略设置。程序可能基于 `sched_setscheduler` 成功的假设来做实时调度，但实际没有任何效果 |

**理由**：这些 syscall 当前返回成功但不做实际操作，用户程序误以为操作已完成，后续行为基于错误假设。这比坦诚地返回 ENOSYS（让程序有机会处理错误或 fallback）更加危险。

#### 第二梯队：补全核心功能 —— 解锁重要用户场景

| 优先级 | Syscall | 修复理由 |
|--------|---------|---------|
| **6** | `waitid` | 比 `wait4` 更灵活（可等待进程组、支持更多选项）。systemd-like 初始化进程使用。实现难度低：复用现有 `wait4` 逻辑，增加 `idtype` 参数解析 |
| **7** | `setpriority` | 与已有的 `getpriority` 配对。`nice` 命令依赖它。实现难度极低：在 `ProcessData` 中加 `nice` 字段 |
| **8** | `execveat` | `fexecve()` 的内核支持。某些安全场景下需要从 fd 执行程序。实现难度低：在 `sys_execve` 基础上增加 dirfd 参数 |
| **9** | `sysinfo` 完善 | 当前 `totalram`/`freeram`/`uptime` 全为 0。`free` / `top` 命令显示全错。修复：从 `axalloc` 获取内存统计，从 `axhal::time` 计算 uptime |
| **10** | `getresuid` / `getresgid` | 某些程序（如 sudo、su）查询三组 UID/GID。当前完全缺失。实现难度极低 |

#### 第三梯队：信号与进程控制完善 —— 需要跨模块改动

| 优先级 | Syscall / 功能 | 修复理由 |
|--------|---------------|---------|
| **11** | SIGSTOP / SIGCONT 语义修复 | 不是新增 syscall，而是修复 `check_signals` 中 Stop/Continue 的默认动作。影响 shell job control（Ctrl+Z、bg、fg）。需要在 `axtask` 层增加任务挂起/恢复机制 |
| **12** | `wait4` 的 WUNTRACED / WCONTINUED | 依赖 SIGSTOP/SIGCONT 的实现。`waitpid` 需要能报告子进程的 stopped/continued 状态 |
| **13** | `clock_settime` | NTP 和时间同步工具需要。实现：调用 `axhal::time::set_wall_time()` |
| **14** | `setitimer` 完善（ITIMER_VIRTUAL / ITIMER_PROF） | 性能分析工具（gprof）依赖 ITIMER_PROF。`timer.rs` 注释说抢占不改变定时器状态，ITIMER_VIRTUAL 在被抢占时可能计时不准 |

#### 第四梯队：文件系统与监控 —— 增强用户态工具体验

| 优先级 | Syscall | 修复理由 |
|--------|---------|---------|
| **15** | `inotify_init1` + `inotify_add_watch` + `inotify_rm_watch` | 文件变化监控。`make` 的增量构建、编辑器的自动重载、包管理器。当前 `inotify_init1` 返回 dummy fd，`add_watch` 完全缺失 |
| **16** | `semget` / `semop` / `semctl` | System V 信号量。PostgreSQL 等数据库使用。实现参考已有的 `msgget` / `shmget` 模式 |
| **17** | `/proc` 完善（不是 syscall，但影响 syscall 可用性） | 多个工具通过 `openat` + `read` 读取 `/proc` 下的文件。`/proc/self/exe`、`/proc/meminfo`、`/proc/cpuinfo` 的缺失直接影响 `ldd`、`free`、`lscpu` 等命令 |

#### 第五梯队：高级特性 —— 长期目标

| 优先级 | Syscall | 修复理由 |
|--------|---------|---------|
| **18** | `ptrace` | 调试器基础。strace 用它追踪 syscall，gdb 用它设断点。实现极其复杂（需要在调度器中支持 traced 状态），但对内核开发者价值极大 |
| **19** | `io_uring` 系列 | 现代高性能 I/O。Rust 的 tokio-uring、C 的 liburing。实现非常复杂（共享内存环形队列+内核轮询线程） |
| **20** | `bpf` | eBPF 运行时。安全监控、网络过滤、性能分析。实现极复杂，可考虑 uBPF 等用户态 BPF 引擎作为轻量替代 |
| **21** | `clone` namespace 支持 | 容器化基础（CLONE_NEWNS/NEWPID/NEWNET 等）。当前仅 warn 不拒绝，但实际不创建命名空间 |

### 排序理由总结

```
   高                                          低
   ←── 修复紧迫性 ──→
   
   「假成功」修复       核心功能补全      信号完善      文件监控/IPC      高级特性
   timerfd             waitid           SIGSTOP       inotify          ptrace
   timer_create        setpriority      WCONTINUED    semaphore        io_uring
   flock               execveat         clock_set     /proc完善        bpf
   fcntl锁             sysinfo完善                                    namespace
   sched_set*          getresuid
   
   ← 对用户态程序的危害更大                      实现复杂度更高 →
```

核心原则：**「返回假成功」比「返回错误码」危害大得多**。一个程序收到 ENOSYS 可以 fallback 或报错退出；但收到假成功后，它会继续运行在错误假设之上，最终在不可预测的时刻以不可预测的方式失败。所以第一梯队全部是修复 no-op/dummy 实现。

---

## 五、Syscall 兼容性强度评估

### 5.1 定量统计

| 分类 | ✅ 完整实现 | ⚠️ 部分实现 | 🔴 Stub/No-op | 完全缺失 |
|------|-----------|------------|--------------|---------|
| 文件系统 | ~40 | ~3 | ~2 | ~3 |
| 内存管理 | ~5 | ~1 | ~4 | 0 |
| 进程/线程 | ~12 | ~3 | ~5 | ~3 |
| 信号 | ~12 | 0 | 0 | 0 |
| 调度 | ~3 | ~2 | ~3 | ~2 |
| 网络 | ~14 | ~1 | 0 | ~2 |
| I/O多路复用 | ~7 | 0 | 0 | 0 |
| IPC | ~8 | 0 | 0 | ~3 |
| 同步 | ~2 | 0 | ~1 | 0 |
| 时间 | ~5 | ~1 | 0 | ~1 |
| 系统信息 | ~4 | ~1 | ~5 | ~2 |
| **合计** | **~112** | **~12** | **~20** | **~16** |

### 5.2 兼容性结论

Starry 对 Linux syscall 的覆盖率在**教学/比赛内核中属于很高水平**（约 160 个入口点）。核心子系统（文件 I/O、网络、信号、epoll、fork/exec/wait、mmap）的主要路径实现质量好，足以运行 BusyBox + musl-libc + Alpine 软件生态。

**最大风险区域**是那 20 个 stub/no-op 实现——它们不报错但不工作，是最容易引发隐蔽 bug 的地方。优先修复它们比增加新 syscall 更有价值。
