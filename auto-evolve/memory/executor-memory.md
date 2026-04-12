# Executor Memory

## 最近更新
- 日期：2026-04-13：issue-029 resolved（**`getresuid`/`getresgid`** syscall + **`ProcessData::get_resuid`/`get_resgid`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-036 resolved（**`prctl`** **`PR_SET_SECCOMP`**/**`PR_MCE_KILL`**：非法参数 **`EINVAL`**，未实现 **`Unsupported`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-027 resolved（**`sys_sysinfo`**：**`totalram`**/**`freeram`**/**`uptime`** 来自 **`axhal`/`axalloc`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-034 resolved（**`sendmsg`/`recvmsg`** **`CMSG_ALIGN`**；**`cmsg_align`** + **`CMsgBuilder`** 填充）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-021 resolved（**`capget`/`capset`** → **`ProcessData`** 三域 **`AtomicU32`**，**`copy_credentials_from`** 继承）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-020 resolved（**`mremap`** 保留 File/COW；**`msync`** → **`CachedFile::sync`**）

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
| issue-029 | getresuid/getresgid 缺失 ENOSYS | resolved | 2026-04-13 |
| issue-036 | prctl SECCOMP/MCE 假成功 | resolved | 2026-04-13 |
| issue-027 | sysinfo totalram/uptime 等为零 | resolved | 2026-04-13 |
| issue-034 | sendmsg/recvmsg CMSG 对齐（多段 SCM_RIGHTS） | resolved | 2026-04-13 |
| issue-021 | capget 全 CAP / capset 空操作 | resolved | 2026-04-13 |
| issue-020 | mremap 丢失 MAP_SHARED 文件后端 | resolved | 2026-04-13 |
| issue-019 | madvise/msync/mlock 桩 | resolved | 2026-04-13 |
| issue-018 | sched_getscheduler RR 桩（同 issue-002） | resolved | 2026-04-13 |
| issue-008 | shared futex 全局 FutexTables Mutex | resolved | 2026-04-13 |
| issue-007 | ELF 加载器 Mutex 串行 execve | resolved | 2026-04-13 |
| issue-003 | getpriority 固定 nice / setpriority | resolved | 2026-04-13 |
| issue-002 | sched_get/setscheduler/getparam 桩 | resolved | 2026-04-13 |
| issue-001 | sched_get/setaffinity 仅当前任务 | resolved | 2026-04-13 |
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
- 进程凭证：`ProcessData` 中 **`ruid/euid/suid`** 与 **`rgid/egid/sgid`**（`AtomicU32`）；`getuid`→`ruid`，`geteuid`→`euid`，gid 同理；**`get_resuid`/`get_resgid`** 供 **`getresuid(2)`/`getresgid(2)`** 一次读三组；`setuid`/`setgid` 为三 ID 同步设置；`setresuid`/`setresgid` 中 **`CRED_NO_CHANGE`=`u32::MAX`** 表示不改；**`euid==0`（或 `egid==0`）** 视为特权可任意改，否则新值须属于当前 `{r,e,s}` 之一否则 **`PermissionDenied`**。`clone` 新建进程在 `ProcessData::new` 后 **`copy_credentials_from`** 父 `ProcessData`。完整 Linux 能力集与 setfsuid 等仍未建模。
- 补充组：**`supplementary_gids`**（`Mutex<Vec<u32>>`，上限 **`SUPP_GROUPS_MAX`**）；`getgroups` 仅列补充组不含主 `rgid`；`setgroups` 需 **`euid==0`**；`getgroups(0,…)` 返回个数。**`seccomp(2)`** 未实现时 **`Unsupported`（ENOSYS）**。**`prctl(PR_SET_SECCOMP)`**：**`arg2`>2**（非 **`SECCOMP_MODE_*`**）→ **`EINVAL`**；mode **0/1/2** 未实现 → **`Unsupported`**。**`prctl(PR_MCE_KILL)`**：**`arg2`** 须 **`CLEAR`/`SET`**；**`SET`** 时 **`arg3`**≤**`DEFAULT`**，否则 **`EINVAL`**；合法组合未实现 → **`Unsupported`**。
- **`execve` 多线程**：在替换映像前若 **`proc.threads().len() > 1`**，对其余 tid **`SIGKILL`** 并 **`yield_now`** 直至仅剩当前线程（对齐 Linux 先杀线程组再 exec）；长时间未收敛则 **`WouldBlock`**。非 vfork/线程本地存储析构等细语义仍弱于 Linux。
- **`accept` / `accept4`**：向用户写入的 sockaddr 必须是 **`peer_addr()`**（远端），勿用 **`local_addr()`**（本端监听地址）；与 **`getpeername(accepted_fd)`** 一致。
- **`sendmsg` / `recvmsg` 与 ancillary**：控制缓冲区须与 Linux 一致使用 **`CMSG_ALIGN(sizeof(cmsghdr)+payload)`** 作为**占用步长**；**`cmsg_len`** 仍为含头的逻辑长度。遍历下一条头用 **`ptr += CMSG_ALIGN(cmsg_len)`**；**`CMsgBuilder::push`** 在 **`msg_controllen`** 与下一 **`cmsghdr`** 指针上前移对齐后长度，**`cmsg_len` 至对齐边界**建议填 **0**。
- **`sched_getaffinity` / `sched_setaffinity`**：`pid==0` 为当前任务；非零先 **`get_task(pid)`**，失败再 **`get_process_data(pid)`** 取 **`proc.threads()` 最小 tid** 定位线程组代表线程。set 时当前任务走 **`set_current_affinity`**（SMP 迁移），其它任务仅 **`set_cpumask`**。未完整建模 CAP、僵尸 **`ESRCH`** 等。
- **`sched_getscheduler` / `sched_setscheduler` / `sched_getparam`**：每线程在 **`Thread`** 上存 **`sched_policy`**（默认0，即 `SCHED_NORMAL`/`SCHED_OTHER`）与 **`sched_priority`**（默认 0）。`setscheduler` 从用户读 **`sched_param`** 并校验策略与优先级范围后写入；`getscheduler`/`getparam` 返回已存值。策略未接入 axtask 真实 RT 调度，仅保证与用户态查询一致。**issue-018** 与 **issue-002** 描述同一修复；验收可用 **`test_sched_stubs.c`**（默认 **`sched_getscheduler(0)==SCHED_OTHER`**）或 **`test_sched_policy_stubs.c`**。
- **`getpriority` / `setpriority`**：每进程 **`ProcessData::nice`**（**-20..=19**，默认 **0**）；`fork` 经 **`copy_credentials_from`** 继承。**`setpriority`** 为新 syscall 分发。**`PRIO_PGRP`/`PRIO_USER`** 在 **`processes()`** 上取匹配进程的 **最小 nice**（最高调度优先级）。未建模 **`CAP_SYS_NICE`** 与特权 **`nice`** 下限等 **`EPERM`**。
- **ELF `execve` 缓存**：全局 **`ELF_LOADER`** 为 **`spin::RwLock<ElfLoader>`**（LRU 32）。**`ensure_elf_cached`**：先读锁命中则返回；未命中则在**无 ELF 锁**下 **`ElfCacheEntry::load`**，再写锁 **去重插入**。**`map_cached_elf_into_uspace`** 持**读锁**做 **`lookup_entry`**、**`uspace.clear`**、**`map_elf`**（多进程可并发读同一缓存项）。写锁仅覆盖 LRU 变更；高并发 + 满缓存时仍存在 **LRU 驱逐** 与「装入后、映射前被挤掉」的极小理论窗口（可后续改为 `Arc` 条目或分片）。
- **跨进程 shared futex**：**`futex_table_for(Shared)`** 使用 **`SHARED_FUTEX_TABLES[shard]`**（**16** 个 **`Mutex<FutexTables>`**），**`shard = (ptr ^ ptr>>12 ^ ptr>>24) % 16`**，`ptr` 为 **`Weak::as_ptr(region)`**。每片内仍为 **`BTreeMap` + 约每 100 次 `retain` GC**；不同共享 region 多数走不同分片。私有 futex 仍 **`ProcessData::futex_table`**。
- **`madvise` / `msync` / `mlock`**：**`MADV_DONTNEED`/`MADV_FREE`** 对 **`CowBackend`** 调用 **`BackendOps::unmap`** 后由缺页 **`populate`** 再分配零页；**`Shared`/`File`/`Linear`** 不丢页（跳过）。**`msync`** 校验 **`MS_ASYNC`与 `MS_SYNC` 二选一**、允许标志位及区间已映射可读；对重叠的 **`File`** VMA 经 **`AddrSpace::msync_file_mappings`** 去重后 **`CachedFile::sync`** 回写页缓存（**`MS_ASYNC`/`MS_SYNC`** 当前均走此路径；**`MS_INVALIDATE`** 未实现丢弃语义）。**`mlock`/`mlock2`** 仅校验 **`MLOCK_ONFAULT`**、**`len>0`**、映射可读；未接 **`RLIMIT_MEMLOCK`** 与物理钉页。
- **`mremap`**：**`AddrSpace::mremap`** 要求 **`addr` 为 VMA 起点且 `old_size` 等于该 VMA 长度**。**缩小**：`unmap` 尾部。**原地放大**：尾部虚拟区间无其它映射时 **`unmap` 全段 + `map` 新尺寸**，**`FileBackend::remap_at`** / **`CowBackend::with_virt_start`** 保持同一 **`CachedFile`/文件偏移或 COW 状态**。**`MREMAP_MAYMOVE`**：尾部冲突时 **`read`→`find_free_area`→搬迁 `map`→`write`**。**`Shared`** 变长（缺页框）与 **`Linear`**：**`OperationNotSupported`**。**`MREMAP_FIXED`/`DONTUNMAP`**：**`EINVAL`**。
- **`capget` / `capset`**：**`ProcessData`** 存 **`cap_effective`/`cap_permitted`/`cap_inheritable`**（**`AtomicU32`**，v3 低 32 位；默认 **`u32::MAX`**）。**`sys_capget`**/**`sys_capset`** 仅允许操作**当前进程**（**`header.pid==0` 或正 pid 解析到同一 `ProcessData`**，否则 **`PermissionDenied`**）。**`capset`** 要求 **`effective`/`inheritable` ⊆ `permitted`**；仅 **`euid==0`** 或当前 **`effective`** 含 **`CAP_SETPCAP`（1<<8）** 可改，否则 **`EPERM`**。未实现 64 位第二组 **`__user_cap_data_struct`**。
- **`sysinfo(2)`**：**`totalram`** ← **`axhal::mem::total_ram_size()`**；**`freeram`** ← **`min(available_pages * PAGE_SIZE_4K, totalram)`**（**`axalloc::global_allocator()`** 空闲页池，近似值）；**`uptime`** ← **`monotonic_time_nanos / NANOS_PER_SEC`**；**`loads`/swap/buffer/high** 仍为 **0**；**`mem_unit=1`**。与 Linux **MemAvailable** 级统计仍有差距。

## 给 Debugger 的消息
- issue-029：请跑 **`/bin/test_getresuid_enosys`**（**`getresuid`/`getresgid`** 返回 0 并写入三组 id）。
- issue-036：请跑 **`/bin/test_prctl_seccomp_stub`**（**`PR_SET_SECCOMP` + 非法 mode → EINVAL**）。
- issue-027：请跑 **`/bin/test_sysinfo_partial`**（**`sysinfo.totalram > 0`**）。
- issue-021：请跑 **`/bin/test_cap_stub`**（**`capset` 清零后 `capget` 全 0**；若 **`capset` EPERM** 则测试会跳过断言）。
- issue-020：请跑 **`/bin/test_mremap_shared`**（**`MAP_SHARED`扩展 + **`msync`** +文件第二页）。
- issue-019：请跑 **`/bin/test_mm_noop`**（**`MADV_DONTNEED`** 后读零）。
- issue-018 / issue-002：可在 rootfs 跑 **`/bin/test_sched_stubs`** 或 **`/bin/test_sched_policy_stubs`**。
- issue-008：请在 rootfs 跑 `/bin/test_futex_shared_stress`（pthread/musl futex）。
- issue-007：请在 rootfs 放 `/bin/true`，跑 `/bin/test_elf_parallel_exec`（双进程并行 `execve`）。
- issue-003：请在 rootfs 跑 `/bin/test_getpriority`（`setpriority`/`getpriority` 对 `PRIO_PROCESS`）。
- issue-002：请在 rootfs 跑 `/bin/test_sched_policy_stubs`（`SCHED_OTHER` 往返与 `sched_getparam` 写缓冲区）。
- issue-001：请在 rootfs 跑 `/bin/test_sched_affinity`（对存活子进程 `sched_getaffinity`）；多线程非 leader PID 行为弱于 Linux。
- issue-034：请在 rootfs 跑 **`/bin/test_sendmsg_cmsg_align`**（多段 **`SCM_RIGHTS`**，依赖 **`CMSG_ALIGN`**）。
- **`accept`/`accept4` peer**：请跑 **`/bin/test_accept_peer_addr`**（与上条 issue 编号无关）。
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
