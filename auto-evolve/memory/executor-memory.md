# Executor Memory

## 最近更新
- 日期：2026-04-13
- 本轮尝试修复：issue-001（sched_*affinity 非零 pid）
- 结果：resolved（`sched_resolve_task` + 对目标任务 cpumask 读/写；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过）

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
| issue-001 | sched_get/setaffinity 仅当前任务 | resolved | 2026-04-13 |
| issue-034 | accept4 写 local非 peer | resolved | 2026-04-12 |
| issue-028 | 多线程 execve WouldBlock | resolved | 2026-04-12 |
| issue-025 | 补充组 stub / seccomp 空成功 | resolved | 2026-04-12 |
| issue-024 | membarrier compiler_fence / cmd 编码 | resolved | 2026-04-12 |
| issue-022 | setuid/getuid 恒0 / setresuid 空操作 | resolved | 2026-04-12 |
| issue-014 | 挂载 API / memfd_secret dummy fd | resolved | 2026-04-12 |
| issue-013 | bpf/io_uring 等 dummy fd 误导 | resolved | 2026-04-12 |
| issue-010 | timerfd DummyFd / poll 永不触发 | resolved | 2026-04-12 |
| issue-005 | membarrier 非 QUERY 路径仅用 compiler_fence | resolved | 2026-04-12 |
| issue-006 | BLOCK_NEXT_SIGNAL_CHECK 全局 AtomicBool | resolved | 2026-04-12 |
| issue-011 | timerfd dummy fd / poll 永不就绪 | resolved | 2026-04-12 |
| issue-012 | inotify/fanotify dummy anon_inode | resolved | 2026-04-12 |
| issue-015 | POSIX timer_create 假成功 Ok(0) | resolved | 2026-04-12 |
| issue-016 | flock 恒 Ok(0) 无互斥 | resolved | 2026-04-12 |
| issue-017 | fcntl F_SETLK 记录锁 no-op | resolved | 2026-04-12 |
| issue-026 | SIGSTOP 误 exit / SIGCONT 空操作 | resolved | 2026-04-12 |
| issue-004 | aspace 全局 Mutex 串行化 | resolved | 2026-04-12 |
| issue-009 | Stop/CONT 作业控制（重复单） | resolved | 2026-04-12 |

## 当前卡点
（无）

## 代码知识积累
- membarrier：Linux `cmd`（除 `QUERY=0`）为 **单位掩码**（`GLOBAL=1<<0`、`GLOBAL_EXPEDITED=1<<1`、`REGISTER_GLOBAL_EXPEDITED=1<<2`…），非单 bit 组合须 **`EINVAL`**；`QUERY` 返回 **已实现命令的按位或**。执行类命令用 **`atomic::fence(SeqCst)`**（勿用 `compiler_fence`）；`PRIVATE_EXPEDITED_SYNC_CORE` 在 **riscv64** 上额外 **`fence.i`**。`REGISTER_*` 仅占位返回0。真 **`GLOBAL` 全系统** 语义需 IPI（当前仅本 hart 最强屏障）。
- 全核 membarrier（多 hart）在 Linux 上依赖 IPI；若未来启用 `axfeat/smp` + `axfeat/ipi`，可在各核 IPI handler 中执行与 `sys_membarrier` 相同的 fence，并用同步原语等待全部完成。
- `rt_sigreturn` 通过 `block_next_signal` 标记「下一次回到用户循环时跳过一次 `check_signals`」；该标志必须是 **per-thread**（`Thread::skip_next_signal_check`），不可用进程级或全局 AtomicBool。
- timerfd：`TimerFd` 实现 `FileLike` + `Pollable`；到期逻辑在 `process_expirations` 中根据时钟纳秒与 `next_deadline_nanos` 比较；通过 `axtask::register_timer_callback`（首次创建时注册）在每次内核 timer tick 中扫描弱引用列表并 `wake` `PollSet`；创建 fd 用 `add_file_like`（与 eventfd2 相同），勿对 `Arc<TimerFd>` 误用 `add_to_fd_table(self)`。
- `bpf` / `userfaultfd`：未实现时返回 **`AxError::Unsupported`（ENOSYS）**，勿再 `sys_dummy_fd`；`perf_event_open` 可返回 **`PermissionDenied`（EPERM）** 以匹配测试与常见无能力场景。`io_uring_setup` 若仅消除 dummy 路径：最小桩返回 `anon_inode:[io_uring]` 的 `IoUringFd`，写回 `sq_entries`/`cq_entries`；真 io_uring 需 ring mmap 与提交队列。
- `fsopen`：无 fs-context 实现时返回 **`NoSuchDevice`（ENODEV）**（或 EINVAL），勿发 `anon_inode:[dummy]`；`fspick`/`open_tree` 可 **`Unsupported`**。`memfd_secret` 在用户态常以两参探测（与 `memfd_create` 同形）时，可 **`sys_memfd_create` 复用** 以获得真实 memfd 路径。已移除 **`sys_dummy_fd`** 分配假 fd 的路径。
- `/proc/self/fd/N` 的 readlink 内容来自 `FileLike::path()`；`inotify_init1`/`fanotify_init` 须使用独立 `InotifyFd`/`FanotifyFd`（`anon_inode:[inotify]` / `anon_inode:[fanotify]`），不能再用 `anon_inode:[dummy]`，否则用户态假阳性。
- POSIX `timer_create` / `timer_settime` / `timer_gettime` / `timer_delete`：未实现时须返回 **`AxError::Unsupported`（ENOSYS）**，禁止 `Ok(0)` 导致用户态 `timer_t` 未写入却被当作成功；若将来实现，需向 `timer_create` 第四参写入非空 id 并接 `sigevent`/线程定时逻辑。
- `flock(2)`：按 inode（`File`/`Directory` 的 `metadata` dev/ino）维护 BSD 风格互斥/共享锁；同一 fd 升级/幂等、关闭 fd 须从全局表移除；`LOCK_NB` 冲突映射 `AxError::WouldBlock`（EAGAIN）；阻塞模式在 `WouldBlock` 上 `yield_now` 轮询。
- `fcntl` 记录锁（`F_SETLK`/`F_SETLKW`/`F_GETLK` 及 `F_OFD_*` 同路径）：仅对普通 `File` fd；`flock64` 区间经 SEEK_SET/CUR/END 解析；写锁与读/写冲突、读锁仅与写冲突；同进程占位前先对重叠区间解锁再插入；`F_SETLK` 冲突返回 `WouldBlock`；`close_file_like` 调用 `record_lock::release_fd` 清除该 fd 登记锁。
- 作业控制：`ProcessData::jobctl` 记录 `stop_sig` / `stop_wait_pending` / `continued_wait_pending`；`SignalOSAction::Stop` 不再 `do_exit`，而是唤醒父 `child_exit_event` 并在内核循环中等待 `SIGCONT`（循环内调用 `check_signals` 以处理入队信号）；`Continue` 清除停止并在曾停止时置 `continued_wait_pending`；`waitpid` 对 `WUNTRACED` 或 **options==0** 写 `(sig<<8)|0x7f`，对 `WCONTINUED` 或 **options==0** 写 `0xffff`。
- 地址空间并发：`ProcessData.aspace` 为 `Arc<RwLock<AddrSpace>>`；修改页表（缺页 populate、mmap 等）用 `write()`；纯查询（如 mincore、`mremap` 查 VMA、futex 地址解析、部分 `can_access_range`）用 `read()`。缺页仍会写锁直至支持按页或 per-VMA 锁。
- 进程凭证：`ProcessData` 中 **`ruid/euid/suid`** 与 **`rgid/egid/sgid`**（`AtomicU32`）；`getuid`→`ruid`，`geteuid`→`euid`，gid 同理；`setuid`/`setgid` 为三 ID 同步设置；`setresuid`/`setresgid` 中 **`CRED_NO_CHANGE`=`u32::MAX`** 表示不改；**`euid==0`（或 `egid==0`）** 视为特权可任意改，否则新值须属于当前 `{r,e,s}` 之一否则 **`PermissionDenied`**。`clone` 新建进程在 `ProcessData::new` 后 **`copy_credentials_from`** 父 `ProcessData`。完整 Linux 能力集与 setfsuid 等仍未建模。
- 补充组：**`supplementary_gids`**（`Mutex<Vec<u32>>`，上限 **`SUPP_GROUPS_MAX`**）；`getgroups` 仅列补充组不含主 `rgid`；`setgroups` 需 **`euid==0`**；`getgroups(0,…)` 返回个数。**`seccomp(2)`** 未实现时 **`Unsupported`（ENOSYS）**；`prctl(PR_SET_SECCOMP)` 仍为占位。
- **`execve` 多线程**：在替换映像前若 **`proc.threads().len() > 1`**，对其余 tid **`SIGKILL`** 并 **`yield_now`** 直至仅剩当前线程（对齐 Linux 先杀线程组再 exec）；长时间未收敛则 **`WouldBlock`**。非 vfork/线程本地存储析构等细语义仍弱于 Linux。
- **`accept` / `accept4`**：向用户写入的 sockaddr 必须是 **`peer_addr()`**（远端），勿用 **`local_addr()`**（本端监听地址）；与 **`getpeername(accepted_fd)`** 一致。
- **`sched_getaffinity` / `sched_setaffinity`**：`pid==0` 为当前任务；非零先 **`get_task(pid)`**，失败再 **`get_process_data(pid)`** 取 **`proc.threads()` 最小 tid** 定位线程组代表线程。set 时当前任务走 **`set_current_affinity`**（SMP 迁移），其它任务仅 **`set_cpumask`**。未完整建模 CAP、僵尸 **`ESRCH`** 等。

## 给 Debugger 的消息
- issue-001：请在 rootfs 跑 `/bin/test_sched_affinity`（对存活子进程 `sched_getaffinity`）；多线程非 leader PID 行为弱于 Linux。
- issue-034：请在 rootfs 跑 `/bin/test_accept_peer_addr`（IPv4 accept 与 getpeername 一致性）。
- issue-028：rootfs 需 `/bin/true`，跑 `/bin/test_execve_multithread`；若 SIGKILL 路径未调度退出可再查 `check_signals`/pthread 阻塞点。
- issue-025：请在 rootfs 跑 `/bin/test_identity_seccomp_stub`；真 seccomp-bpf 未实现；`PR_SET_SECCOMP` 与 `seccomp` syscall 行为不一致属已知简化。
- issue-024：`/bin/test_membarrier_stub` 仅测 `QUERY`；RSEQ/`GET_REGISTRATIONS` 等返回 `EINVAL`；多核全局屏障需后续 IPI。
- issue-022：请在 rootfs 跑 `/bin/test_setuid_stub`；未接 `setfsuid`/文件置位 exec 等。
- issue-014：请在 rootfs 跑 `/bin/test_dummy_fsapi`；真 Linux `memfd_secret` 常为单参 flags，本内核按测试与 `memfd_create` 同形两参接入。
- issue-013：请在 rootfs 跑 `/bin/test_dummy_fd_advanced`；`io_uring_setup` 仅为路径与 params 桩，真实 liburing 仍可能因缺 ring失败。
- issue-010：timerfd 与 issue-011 同一套 `kernel/src/file/timerfd.rs`；请在带 `/bin/test_timerfd` 的 rootfs 中 QEMU 验证 poll+read。
- issue-009：与 issue-026 同一套 `JobCtl` + `check_signals` Stop/CONT + `waitpid`；`raise(SIGSTOP)` 与 `kill(..., SIGSTOP)` 同源。请在 QEMU 跑 `test_sigstop_sigcont`。
- issue-004：`aspace` 已迁 `RwLock`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 QEMU 跑 `test_aspace_concurrent_mmap` 做功能基线；多线程同时缺页仍互斥写锁，进一步优化需更细粒度锁。
- issue-026：`SIGSTOP`/`SIGCONT` 与 `waitpid` 已接 `JobCtl`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 跑 `test_sigstop_semantic`（`WUNTRACED` + `WSTOPSIG==SIGSTOP`）。多线程全进程停表仍简化为单线程路径；仅测试 fork 子进程场景。
- issue-017：`record_lock.rs` + `sys_fcntl` 接入；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 中跑 `test_fcntl_lock_stub` 验证第二进程 `F_SETLK` 得 EAGAIN/EACCES。OFD 锁与 `dup` 共享同一 open file description 的精细语义仍弱化为与进程锁相同路径，如遇真实用例可再细化。
- issue-016：`kernel/src/file/flock.rs` 实现按 (dev,ino) 的 flock 表；`close_file_like` 前 `release_fd`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 中交叉编译并运行 `test_flock_stub` 验证父持 LOCK_EX 时子 LOCK_NB 得 EAGAIN/EWOULDBLOCK。
- issue-015：`test_posix_timer_stub.c` 在 `timer_create` 返回 ENOSYS 时显式 PASS；已交叉编译并通过 `clippy`/`make build`。
- issue-012 已修复：`test_dummy_inotify_fanotify.c` 已交叉编译；`cargo fmt`、`clippy -F qemu`、`make ARCH=riscv64 build` 通过。请在带 procfs 的 rootfs 中实跑 readlink。
- `inotify_add_watch` 等仍为未实现时可继续标 ENOSYS；若需端到端监控，可再开 issue 实现队列与事件格式。
