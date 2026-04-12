# Debugger Memory

## 最近更新
- 日期：2026-04-12
- 本轮新发现问题数：1（**issue-171**：**`sys_readlinkat`** 未 **`dirfd_for_path_resolution`**，与 **issue-169** 已改 *at 不一致）
- 本轮验证已修复问题数：1（**issue-169** **`dirfd_for_path_resolution`** + **`resolve_at`/`openat`/`mkdirat` 等**；**clippy** PASS → **verified**）
- **issue-pool 下一可用 ID**：**issue-172**

## 扫描进度
- 已审计的 syscall 模块：依据 Question2.md 全文与 kernel 抽样核对（`syscall/mod.rs` dummy 分支、`fd_ops.rs` flock/fcntl、`mmap.rs`、`schedule.rs`、`sys.rs`、`ctl.rs`、`signal.rs`、`membarrier.rs`）；**本轮新增：`syscall/net/socket.rs`（accept4 与 peer 地址语义）**；**`file/fs.rs` `resolve_at`/`dirfd_for_path_resolution`（→ issue-169 verified）**；**`ctl.rs` `sys_readlinkat` 与 *at 族差异（→ issue-171 open）**
- 未审计的 syscall 模块：`syscall/net/io.rs`/`cmsg.rs` 边界、**`syscall/net/opt.rs` 已扫为宏分发（非本轮重点）**、ioctl 全表、prctl 剩余选项；**`axnet-ng/general.rs` `SO_ERROR`（issue-170 open）**、**`fs/stat.rs` `sys_statfs` buf/path 顺序**（示例）
- 已检查的 TODO/FIXME 位置：execve 多线程、mremap full、fd_ops flock、flock64、sysinfo Zeroable、timer 抢占相关注释

## 活跃问题摘要
- issue-011: timerfd dummy fd（critical, open）
- issue-012: inotify/fanotify dummy（critical, open）
- issue-013: bpf/io_uring/perf/userfault dummy（high, open）
- issue-014: fsopen/open_tree/memfd_secret dummy（high, open）
- issue-015: POSIX timer Ok(0) 假成功（critical, open）
- issue-016: flock no-op（critical, open）
- issue-017: fcntl 记录锁 no-op（critical, open）
- issue-018: sched 策略 stub（medium, open）
- issue-019: madvise/msync/mlock no-op（medium, open）
- issue-020: mremap 粗糙实现（medium, open）
- issue-021: capget/capset stub（medium, open）
- issue-022: setresuid/identity 固定 root（high, open）
- issue-023: get_mempolicy stub（low, open）
- issue-024: membarrier 仅 compiler_fence（high, open）
- issue-025: groups/seccomp stub（high, open）
- issue-026: SIGSTOP/SIGCONT 默认动作错误（critical, open）
- issue-027: sysinfo 内存/uptime 为 0（medium, open）
- issue-028: execve 多线程拒绝（high, open）
- issue-029: getresuid/getresgid 缺失 ENOSYS（medium, open；与 issue-001 亲和性问题不同）
- issue-030: setpriority 缺失（medium, open）
- issue-031: ioctl 非终端场景（low, open）
- issue-032: mount 类型受限/行为（medium, open）
- issue-033: setitimer VIRTUAL/PROF 精度（low, open）
- issue-034: accept 返回本端地址非对端（high, open）
- **issue-pool（高编号，与上表独立）open：issue-170**（**`getsockopt` `SO_ERROR`** 恒 0）、**issue-171**（**`readlinkat`** 未跟 **issue-169** **`dirfd_for_path_resolution`**）

## 给 Executor 的消息
- 建议仍按 Question2 第四部分「第一梯队」顺序：先假成功类（011–017、015），再 026 信号与 job control（依赖 axtask 能力）。
- issue-012/013/014 依赖 `/proc/self/fd` readlink；若 rootfs 无 proc，测试需在该环境标注或换检测方式。
- sys_dummy_fd 对进程名 `qemu-` 前缀会返回 ENOSYS（io.rs），与裸机行为不一致，修复时勿忽略该分支。
- issue-034 为单行语义错误（`local_addr`→`peer_addr`），修复成本低、影响所有依赖 accept 输出地址的网络服务。

## 待验证
- Q1 生成的 issue-001–010 未在本仓库中出现；若合并仓库后请避免编号冲突。
- 全部测试已在 `riscv64-linux-musl-gcc -static -pthread -Werror` 下通过交叉编译，尚未在 QEMU/Starry 真机跑通。
- **issue-169**：QEMU 绝对路径 + 无效 **`dirfd`**（**`fstatat`/`openat`**）；**issue-171**：同上场景 **`readlinkat`**；**issue-170**：挂起错误 + **`SO_ERROR`**。
