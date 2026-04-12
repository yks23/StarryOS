# Executor Memory

## 最近更新
- 日期：2026-04-12
- 本轮尝试修复：issue-010（timerfd DummyFd / poll 永不就绪）
- 结果：resolved（内核已有 `TimerFd` + `sys_timerfd_*` 接线；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过；与 issue-011 同源实现）

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
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
- membarrier：非 QUERY 路径应使用 `core::sync::atomic::fence(Ordering::SeqCst)`（或架构特定 fence），勿用 `compiler_fence` 冒充 CPU 屏障；非法 `cmd` 应对照 `MEMBARRIER_CMD_QUERY` 掩码返回 `EINVAL`。
- 全核 membarrier（多 hart）在 Linux 上依赖 IPI；若未来启用 `axfeat/smp` + `axfeat/ipi`，可在各核 IPI handler 中执行与 `sys_membarrier` 相同的 fence，并用同步原语等待全部完成。
- `rt_sigreturn` 通过 `block_next_signal` 标记「下一次回到用户循环时跳过一次 `check_signals`」；该标志必须是 **per-thread**（`Thread::skip_next_signal_check`），不可用进程级或全局 AtomicBool。
- timerfd：`TimerFd` 实现 `FileLike` + `Pollable`；到期逻辑在 `process_expirations` 中根据时钟纳秒与 `next_deadline_nanos` 比较；通过 `axtask::register_timer_callback`（首次创建时注册）在每次内核 timer tick 中扫描弱引用列表并 `wake` `PollSet`；创建 fd 用 `add_file_like`（与 eventfd2 相同），勿对 `Arc<TimerFd>` 误用 `add_to_fd_table(self)`。
- `/proc/self/fd/N` 的 readlink 内容来自 `FileLike::path()`；`inotify_init1`/`fanotify_init` 须使用独立 `InotifyFd`/`FanotifyFd`（`anon_inode:[inotify]` / `anon_inode:[fanotify]`），不能再用 `anon_inode:[dummy]`，否则用户态假阳性。
- POSIX `timer_create` / `timer_settime` / `timer_gettime` / `timer_delete`：未实现时须返回 **`AxError::Unsupported`（ENOSYS）**，禁止 `Ok(0)` 导致用户态 `timer_t` 未写入却被当作成功；若将来实现，需向 `timer_create` 第四参写入非空 id 并接 `sigevent`/线程定时逻辑。
- `flock(2)`：按 inode（`File`/`Directory` 的 `metadata` dev/ino）维护 BSD 风格互斥/共享锁；同一 fd 升级/幂等、关闭 fd 须从全局表移除；`LOCK_NB` 冲突映射 `AxError::WouldBlock`（EAGAIN）；阻塞模式在 `WouldBlock` 上 `yield_now` 轮询。
- `fcntl` 记录锁（`F_SETLK`/`F_SETLKW`/`F_GETLK` 及 `F_OFD_*` 同路径）：仅对普通 `File` fd；`flock64` 区间经 SEEK_SET/CUR/END 解析；写锁与读/写冲突、读锁仅与写冲突；同进程占位前先对重叠区间解锁再插入；`F_SETLK` 冲突返回 `WouldBlock`；`close_file_like` 调用 `record_lock::release_fd` 清除该 fd 登记锁。
- 作业控制：`ProcessData::jobctl` 记录 `stop_sig` / `stop_wait_pending` / `continued_wait_pending`；`SignalOSAction::Stop` 不再 `do_exit`，而是唤醒父 `child_exit_event` 并在内核循环中等待 `SIGCONT`（循环内调用 `check_signals` 以处理入队信号）；`Continue` 清除停止并在曾停止时置 `continued_wait_pending`；`waitpid` 对 `WUNTRACED` 或 **options==0** 写 `(sig<<8)|0x7f`，对 `WCONTINUED` 或 **options==0** 写 `0xffff`。
- 地址空间并发：`ProcessData.aspace` 为 `Arc<RwLock<AddrSpace>>`；修改页表（缺页 populate、mmap 等）用 `write()`；纯查询（如 mincore、`mremap` 查 VMA、futex 地址解析、部分 `can_access_range`）用 `read()`。缺页仍会写锁直至支持按页或 per-VMA 锁。

## 给 Debugger 的消息
- issue-010：timerfd 与 issue-011 同一套 `kernel/src/file/timerfd.rs`；请在带 `/bin/test_timerfd` 的 rootfs 中 QEMU 验证 poll+read。
- issue-009：与 issue-026 同一套 `JobCtl` + `check_signals` Stop/CONT + `waitpid`；`raise(SIGSTOP)` 与 `kill(..., SIGSTOP)` 同源。请在 QEMU 跑 `test_sigstop_sigcont`。
- issue-004：`aspace` 已迁 `RwLock`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 QEMU 跑 `test_aspace_concurrent_mmap` 做功能基线；多线程同时缺页仍互斥写锁，进一步优化需更细粒度锁。
- issue-026：`SIGSTOP`/`SIGCONT` 与 `waitpid` 已接 `JobCtl`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 跑 `test_sigstop_semantic`（`WUNTRACED` + `WSTOPSIG==SIGSTOP`）。多线程全进程停表仍简化为单线程路径；仅测试 fork 子进程场景。
- issue-017：`record_lock.rs` + `sys_fcntl` 接入；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 中跑 `test_fcntl_lock_stub` 验证第二进程 `F_SETLK` 得 EAGAIN/EACCES。OFD 锁与 `dup` 共享同一 open file description 的精细语义仍弱化为与进程锁相同路径，如遇真实用例可再细化。
- issue-016：`kernel/src/file/flock.rs` 实现按 (dev,ino) 的 flock 表；`close_file_like` 前 `release_fd`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 中交叉编译并运行 `test_flock_stub` 验证父持 LOCK_EX 时子 LOCK_NB 得 EAGAIN/EWOULDBLOCK。
- issue-015：`test_posix_timer_stub.c` 在 `timer_create` 返回 ENOSYS 时显式 PASS；已交叉编译并通过 `clippy`/`make build`。
- issue-012 已修复：`test_dummy_inotify_fanotify.c` 已交叉编译；`cargo fmt`、`clippy -F qemu`、`make ARCH=riscv64 build` 通过。请在带 procfs 的 rootfs 中实跑 readlink。
- `inotify_add_watch` 等仍为未实现时可继续标 ENOSYS；若需端到端监控，可再开 issue 实现队列与事件格式。
