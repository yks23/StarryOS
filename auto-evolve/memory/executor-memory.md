# Executor Memory

## 最近更新
- 日期：2026-04-12
- 本轮尝试修复：issue-005 (membarrier)
- 结果：resolved

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
| issue-005 | membarrier 非 QUERY 路径仅用 compiler_fence | resolved | 2026-04-12 |

## 当前卡点
（无）

## 代码知识积累
- membarrier：非 QUERY 路径应使用 `core::sync::atomic::fence(Ordering::SeqCst)`（或架构特定 fence），勿用 `compiler_fence` 冒充 CPU 屏障；非法 `cmd` 应对照 `MEMBARRIER_CMD_QUERY` 掩码返回 `EINVAL`。
- 全核 membarrier（多 hart）在 Linux 上依赖 IPI；若未来启用 `axfeat/smp` + `axfeat/ipi`，可在各核 IPI handler 中执行与 `sys_membarrier` 相同的 fence，并用同步原语等待全部完成。

## 给 Debugger 的消息
- issue-005 已修复并通过 `cargo fmt`、`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`、`make ARCH=riscv64 build`；`test_membarrier.c` 已用 riscv64-linux-musl-gcc 交叉编译通过。若需 QEMU 实机输出 SUMMARY，请在带 rootfs 镜像的环境中将 `/tmp/test_membarrier` 拷入镜像 `/bin/` 后运行。
- 建议在启用 SMP+IPI 的构建上增加多线程 membarrier 压力或 litmus 类测试，以验证跨核语义。
