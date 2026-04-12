# Executor Memory

## 最近更新
- 日期：2026-04-12
- 本轮尝试修复：issue-012 (inotify / fanotify anon_inode)
- 结果：resolved

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
| issue-005 | membarrier 非 QUERY 路径仅用 compiler_fence | resolved | 2026-04-12 |
| issue-006 | BLOCK_NEXT_SIGNAL_CHECK 全局 AtomicBool | resolved | 2026-04-12 |
| issue-011 | timerfd dummy fd / poll 永不就绪 | resolved | 2026-04-12 |
| issue-012 | inotify/fanotify dummy anon_inode | resolved | 2026-04-12 |

## 当前卡点
（无）

## 代码知识积累
- membarrier：非 QUERY 路径应使用 `core::sync::atomic::fence(Ordering::SeqCst)`（或架构特定 fence），勿用 `compiler_fence` 冒充 CPU 屏障；非法 `cmd` 应对照 `MEMBARRIER_CMD_QUERY` 掩码返回 `EINVAL`。
- 全核 membarrier（多 hart）在 Linux 上依赖 IPI；若未来启用 `axfeat/smp` + `axfeat/ipi`，可在各核 IPI handler 中执行与 `sys_membarrier` 相同的 fence，并用同步原语等待全部完成。
- `rt_sigreturn` 通过 `block_next_signal` 标记「下一次回到用户循环时跳过一次 `check_signals`」；该标志必须是 **per-thread**（`Thread::skip_next_signal_check`），不可用进程级或全局 AtomicBool。
- timerfd：`TimerFd` 实现 `FileLike` + `Pollable`；到期逻辑在 `process_expirations` 中根据时钟纳秒与 `next_deadline_nanos` 比较；通过 `axtask::register_timer_callback`（首次创建时注册）在每次内核 timer tick 中扫描弱引用列表并 `wake` `PollSet`；创建 fd 用 `add_file_like`（与 eventfd2 相同），勿对 `Arc<TimerFd>` 误用 `add_to_fd_table(self)`。
- `/proc/self/fd/N` 的 readlink 内容来自 `FileLike::path()`；`inotify_init1`/`fanotify_init` 须使用独立 `InotifyFd`/`FanotifyFd`（`anon_inode:[inotify]` / `anon_inode:[fanotify]`），不能再用 `anon_inode:[dummy]`，否则用户态假阳性。

## 给 Debugger 的消息
- issue-012 已修复：`test_dummy_inotify_fanotify.c` 已交叉编译；`cargo fmt`、`clippy -F qemu`、`make ARCH=riscv64 build` 通过。请在带 procfs 的 rootfs 中实跑 readlink。
- `inotify_add_watch` 等仍为未实现时可继续标 ENOSYS；若需端到端监控，可再开 issue 实现队列与事件格式。
