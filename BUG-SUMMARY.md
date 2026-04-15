# Starry OS Bug 发现与修复总结

Auto-Evolve 系统在 3.5 小时运行中共发现 245 个问题，修复 244 个。以下是分类总结。

## 总览

| 指标 | 数值 |
|------|------|
| 总 Issue | 245 |
| 已修复 (resolved + verified) | 244 |
| 未解决 (open) | 1 |
| Git Commits | 426 |

### 按严重性

| 严重性 | 数量 | 修复率 |
|--------|------|--------|
| Critical | 8 | 100% |
| High | 14 | 100% |
| Medium | 41 | 100% |
| Low | 182 | 99.5% |

### 按类别

| 类别 | 数量 | 说明 |
|------|------|------|
| syscall-stub | 8 | Ok(0) 假成功或 dummy fd |
| syscall-missing | 3 | 完全缺失的 syscall |
| syscall-semantic | 220+ | 语义偏差（参数校验、返回值、行为） |
| concurrency | 5 | 多核竞态条件 |
| correctness | 5 | 逻辑错误 |
| improvement | 4 | 主动改进提案 |

---

## Critical 问题（8 个，全部修复）

### 1. membarrier 仅用 compiler_fence（issue-005）

**问题**：`sys_membarrier` 的非 QUERY 路径仅执行 `compiler_fence(SeqCst)`，不产生硬件内存屏障指令。多核下依赖 membarrier 的用户态同步库可能出现数据竞争。

**修复**：`compiler_fence` → `atomic::fence(SeqCst)`，产生 CPU 级屏障指令（riscv64: `fence iorw,iorw`）。

**文件**：`kernel/src/syscall/sync/membarrier.rs`

### 2. 信号检查全局 AtomicBool 竞态（issue-006）

**问题**：`BLOCK_NEXT_SIGNAL_CHECK` 是全局静态 `AtomicBool`，`rt_sigreturn` 设置的"跳过下一次信号检查"标志可能被其他 CPU 上的线程消费。

**修复**：在 `Thread` 结构体中新增 `skip_next_signal_check: AtomicBool`，替换全局变量。

**文件**：`kernel/src/task/mod.rs`, `kernel/src/task/signal.rs`

### 3. timerfd 返回 dummy fd（issue-011）

**问题**：`timerfd_create` 返回 `DummyFd`，`poll()` 永远返回空事件，所有 event loop 的定时器失效。

**修复**：实现完整的 `TimerFd`（216 行），支持 `CLOCK_MONOTONIC`/`CLOCK_REALTIME`，实现 `Pollable` trait，到期时唤醒 epoll。

**文件**：新增 `kernel/src/file/timerfd.rs`，修改 `kernel/src/syscall/mod.rs`

### 4. inotify/fanotify 返回 dummy fd（issue-012）

**问题**：`inotify_init1`/`fanotify_init` 返回通用 `DummyFd`，`/proc/self/fd/N` 的 readlink 显示 `anon_inode:[dummy]`，用户态无法区分 fd 类型。

**修复**：新增 `InotifyFd`/`FanotifyFd`，path 分别为 `anon_inode:[inotify]`/`anon_inode:[fanotify]`。

### 5. POSIX timer 假成功（issue-015）

**问题**：`timer_create`/`timer_settime`/`timer_gettime` 直接返回 `Ok(0)`，不创建任何定时器。musl 的 `sleep()` 在某些路径依赖它，可能永久阻塞。

**修复**：改为返回 `AxError::Unsupported`（ENOSYS），让用户态库 fallback 到其他实现。

### 6. flock 空操作（issue-016）

**问题**：`sys_flock` 直接返回 `Ok(0)`，不加任何锁。包管理器、数据库多进程并发写文件可能数据损坏。

**修复**：实现 BSD flock（224 行），按 inode 维护排他锁/共享锁状态，支持 `LOCK_NB`、关闭 fd 自动释放。

**文件**：新增 `kernel/src/file/flock.rs`

### 7. fcntl 记录锁空操作（issue-017）

**问题**：`F_SETLK`/`F_SETLKW`/`F_OFD_SETLK` 直接返回 `Ok(0)`，不做文件记录锁。SQLite 等数据库依赖它。

**修复**：新增 `kernel/src/file/record_lock.rs`，实现按字节区间的 POSIX 建议性记录锁。

### 8. SIGSTOP 杀死进程（issue-026）

**问题**：收到 `SIGSTOP`/`SIGTSTP` 时调用 `do_exit(1, true)` 直接杀死进程，Ctrl+Z 无法暂停前台进程。

**修复**：在 `ProcessData` 中新增 `JobCtl`（stop_sig、stop_wait_pending、continued_wait_pending），Stop 动作改为阻塞等待 SIGCONT，waitpid 支持 `WUNTRACED`/`WCONTINUED`。

**文件**：`kernel/src/task/mod.rs`, `kernel/src/task/signal.rs`, `kernel/src/syscall/task/wait.rs`

---

## High 问题（14 个，全部修复）

| Issue | 标题 | 修复要点 |
|-------|------|---------|
| 004 | AddrSpace 全局 Mutex 串行化 | 改为 `RwLock`，page fault 读锁并行 |
| 009 | SIGSTOP/SIGCONT 重复项 | 与 issue-026 同源修复 |
| 010 | timerfd DummyFd | 与 issue-011 同源修复 |
| 013 | bpf/io_uring/userfaultfd dummy fd | 改返回 ENOSYS/EPERM |
| 014 | fsopen/fspick/open_tree dummy fd | 改返回 ENODEV/Unsupported |
| 022 | setuid/getuid 恒 0 | 在 ProcessData 中实现 ruid/euid/suid 凭证 |
| 024 | membarrier cmd 编码错误 | 使用 Linux uapi 位标志对齐 |
| 025 | getgroups/setgroups stub | 实现补充组 + seccomp 返回 ENOSYS |
| 028 | 多线程 execve 直接拒绝 | 先 SIGKILL 兄弟线程再 exec |
| 034 | accept4 写 local 而非 peer 地址 | 改为 `peer_addr()` |
| 036 | prctl SECCOMP/MCE 假成功 | 非法参数返回 EINVAL |
| 178 | close_range UNSHARE 后未写回 scope | 持写锁下正确 clone FD_TABLE |
| 181 | fcntl 未知 cmd 假成功 Ok(0) | 改返回 EINVAL |
| 183 | recvmsg SCM_RIGHTS 静默丢弃 | 添加 remaining() 检查 |
| 227 | shmat 无效 shmid 内核 panic | unwrap() 改为 ok_or(EINVAL) |

---

## Medium 问题（41 个，代表性示例）

| Issue | 标题 |
|-------|------|
| 001 | sched_getaffinity 仅支持 pid=0 |
| 002 | sched_setscheduler 是 stub |
| 003 | getpriority 固定返回 20 |
| 007 | ELF 加载器全局 Mutex |
| 008 | 跨进程 futex 全局锁 |
| 019 | madvise/msync/mlock 空操作 |
| 020 | mremap 用 memcpy 而非页表重映射 |
| 021 | capget 返回全部 capability |
| 027 | sysinfo totalram/uptime 为 0 |
| 029 | getresuid/getresgid 缺失 |
| 030 | setpriority 缺失 |
| 035 | clock_gettime 无效 clockid 不报错 |
| 037 | splice/copy_file_range 不校验 flags |
| 038 | recvfrom/sendto 不校验 MSG_* flags |

---

## Low 问题（182 个）

主要是参数校验顺序优化和边界条件修复，例如：

- 各 `*at` syscall 在加载路径前先验证 dirfd
- 非法 flags 位返回 EINVAL 而非静默忽略
- NULL 指针参数的提前校验
- 返回值与 Linux 精确对齐

这些单个影响不大，但累积起来显著提升了 Starry 的 Linux 兼容性水平。

---

## 测试覆盖

31 个 C 语言测试文件，涵盖：

| 类别 | 测试文件数 | 示例 |
|------|-----------|------|
| Syscall stub 验证 | 8 | timerfd、flock、fcntl、posix_timer |
| 并发/多核 | 4 | membarrier、信号竞态、并发 mmap、futex |
| 语义正确性 | 9 | fork CoW、execve 多线程、accept 地址、SIGSTOP |
| 参数校验 | 8 | 各 syscall 非法 flags 返回 EINVAL |
| 系统信息 | 2 | sysinfo、getpriority |

所有测试使用 musl-libc 静态编译，输出统一的 `[TEST] ... PASS/FAIL` 格式。
