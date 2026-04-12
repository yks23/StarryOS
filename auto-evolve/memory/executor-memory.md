# Executor Memory

## 最近更新
- 日期：2026-04-12
- 本轮尝试修复：issue-006 (BLOCK_NEXT_SIGNAL_CHECK per-thread)
- 结果：resolved

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
| issue-005 | membarrier 非 QUERY 路径仅用 compiler_fence | resolved | 2026-04-12 |
| issue-006 | BLOCK_NEXT_SIGNAL_CHECK 全局 AtomicBool | resolved | 2026-04-12 |

## 当前卡点
（无）

## 代码知识积累
- membarrier：非 QUERY 路径应使用 `core::sync::atomic::fence(Ordering::SeqCst)`（或架构特定 fence），勿用 `compiler_fence` 冒充 CPU 屏障；非法 `cmd` 应对照 `MEMBARRIER_CMD_QUERY` 掩码返回 `EINVAL`。
- 全核 membarrier（多 hart）在 Linux 上依赖 IPI；若未来启用 `axfeat/smp` + `axfeat/ipi`，可在各核 IPI handler 中执行与 `sys_membarrier` 相同的 fence，并用同步原语等待全部完成。
- `rt_sigreturn` 通过 `block_next_signal` 标记「下一次回到用户循环时跳过一次 `check_signals`」；该标志必须是 **per-thread**（`Thread::skip_next_signal_check`），不可用进程级或全局 AtomicBool。

## 给 Debugger 的消息
- issue-006 已修复：`test_block_next_signal.c` 已 riscv64-linux-musl-gcc 交叉编译通过；`cargo fmt`、`clippy -F qemu`、`make ARCH=riscv64 build` 通过。QEMU 实跑需自备 rootfs。
- 建议在多线程 + 信号处理场景下增加专用 litmus（验证线程 A 的 `rt_sigreturn` 不会清除线程 B 的 skip 标志）。
