# Starry OS 机考分析报告

## 目录

1. [多核支持能力分析与优化方案](#1-多核支持能力分析与优化方案)
2. [最值得改进的10个点及改进计划](#2-最值得改进的10个点及改进计划)
3. [Linux Syscall 支持能力与缺陷分析](#3-linux-syscall-支持能力与缺陷分析)
4. [Syscall 优先实现排序](#4-syscall-优先实现排序)
5. [AI 自动迭代编程方法设计](#5-ai-自动迭代编程方法设计)

---

## 1. 多核支持能力分析与优化方案

### 1.1 当前多核支持现状

Starry 的多核支持分布在多个层次：

**HAL 层 (`axhal`)**：
- 多核启动（secondary CPU bring-up）由 `axhal` 在各架构的汇编入口处完成
- 提供 `cpu_num()` 获取 CPU 核心数，`cpu_id()` 获取当前 CPU ID
- 通过 `percpu` crate 实现 per-CPU 数据存储

**调度层 (`axtask`)**：
- `AxCpuMask` 实现 CPU 亲和性位图
- `sched_getaffinity` / `sched_setaffinity` 已实现，但仅支持当前线程（`pid != 0` 返回错误）
- 调度器使用 Round-Robin 策略（`axfeat` feature 中可见 `sched-rr`）
- `sched_setscheduler` 实际上是空实现，直接返回 `Ok(0)`

**内核锁机制**：
- `SpinNoIrq` / `SpinNoPreempt`：自旋锁变体，关中断/关抢占
- `axsync::Mutex`：内核互斥锁（基于 futex 风格的阻塞）
- `spin::RwLock`：读写锁
- `core::sync::atomic`：原子操作广泛使用

**关键并发数据结构**：
- `TASK_TABLE`, `PROCESS_TABLE` 等全局表使用 `spin::RwLock`
- `ProcessData.aspace` 使用 `Arc<Mutex<AddrSpace>>`——这是一个**关键瓶颈**
- `FutexTable` 使用 `Mutex<HashMap<...>>`
- `SHARED_FUTEX_TABLES` 全局静态，使用 `Mutex`

### 1.2 多核支持的关键问题

#### 问题 1：地址空间锁粒度过粗

```
pub aspace: Arc<Mutex<AddrSpace>>,
```

整个进程的地址空间被一把大锁保护。所有页表操作（page fault 处理、mmap、munmap、mprotect 等）都需要获取这把锁。在多线程程序中，多个线程同时触发 page fault 时会严重竞争。

**优化方案**：
- 引入 per-VMA 的细粒度锁或 `RwLock`，读操作（page fault 查询）可以并行
- 参考 Linux 的 `mmap_lock` + per-VMA lock 设计（Linux 6.4+）
- 短期可将 `Mutex` 改为 `RwLock`，page fault 处理使用读锁

#### 问题 2：membarrier 实现不完整

```rust
// membarrier.rs
_ => {
    compiler_fence(Ordering::SeqCst);
    Ok(0)
}
```

`membarrier` 系统调用仅使用 `compiler_fence` 而非真正的 IPI（Inter-Processor Interrupt）。在多核场景下，`compiler_fence` 仅阻止编译器重排序，**不保证硬件层面的内存可见性**。正确实现需要向其他 CPU 发送 IPI 来执行内存屏障。

**优化方案**：
- 实现 `MEMBARRIER_CMD_GLOBAL` 时，通过 IPI 让所有 CPU 执行 `fence` 指令
- 实现 `MEMBARRIER_CMD_PRIVATE_EXPEDITED` 时，仅向运行当前进程线程的 CPU 发送 IPI
- 需要 `axhal` 层提供 `send_ipi_to(cpu_id)` 原语

#### 问题 3：调度器功能有限

- 仅支持 Round-Robin，不支持 CFS、FIFO 等策略
- `sched_setscheduler` 是空操作
- `getpriority` 始终返回 20（默认 nice 值），不支持设置优先级
- `sched_getaffinity` / `sched_setaffinity` 仅支持当前线程

**优化方案**：
- 实现多级反馈队列或类 CFS 调度器
- 支持 `nice` 值影响调度权重
- 支持指定任意 PID 的 affinity 操作

#### 问题 4：`BLOCK_NEXT_SIGNAL_CHECK` 使用全局静态变量

```rust
static BLOCK_NEXT_SIGNAL_CHECK: AtomicBool = AtomicBool::new(false);
```

信号检查阻塞标志使用全局变量而非 per-CPU 或 per-task 变量，这在多核场景下是一个竞态条件——一个 CPU 上的操作可能影响另一个 CPU 上的信号处理流程。

**优化方案**：将该标志移入 `Thread` 结构体或使用 per-CPU 存储。

### 1.3 多核优化总结路线图

| 优先级 | 优化项 | 难度 | 影响范围 |
|--------|--------|------|----------|
| P0 | 修复 BLOCK_NEXT_SIGNAL_CHECK 竞态 | 低 | task/signal.rs |
| P0 | 修复 membarrier 实现 | 中 | 需要 axhal IPI 支持 |
| P1 | AddrSpace 锁粒度优化 | 高 | mm 子系统全局 |
| P1 | 完善 affinity 支持（任意 PID） | 低 | syscall/task/schedule.rs |
| P2 | 多调度策略支持 | 高 | 依赖 axtask 重构 |
| P2 | per-CPU 运行队列优化 | 高 | axtask 核心 |

---

## 2. 最值得改进的10个点及改进计划

### 改进 1：信号处理机制不完整

**现状**：
- `SIGSTOP` / `SIGCONT` 未实现（`check_signals` 中对 `Stop` 动作直接 `do_exit`）
- Core dump 未实现（仅以 exit code 128+signo 退出）
- 信号处理中的竞态条件（全局 `BLOCK_NEXT_SIGNAL_CHECK`）

**计划**：
1. 实现 `SIGSTOP` 使线程挂起（引入 `Stopped` 状态）
2. 实现 `SIGCONT` 唤醒被 stop 的线程
3. 为 Core dump 生成 ELF core 文件（可简化为仅记录寄存器状态）
4. 将全局信号标志改为 per-thread

### 改进 2：`/proc` 文件系统过于简陋

**现状**：`pseudofs/proc.rs` 实现了基础的 procfs，但许多关键条目缺失或内容不完整。

**计划**：
1. 完善 `/proc/[pid]/status`、`/proc/[pid]/maps`、`/proc/[pid]/stat`
2. 实现 `/proc/[pid]/fd/` 目录（符号链接到打开的文件）
3. 实现 `/proc/meminfo`、`/proc/cpuinfo`
4. 实现 `/proc/self` 符号链接

### 改进 3：FS 功能缺失

**现状**：
- `timerfd_create` 是 dummy fd（返回一个永远不会触发的 fd）
- `inotify_init1` 是 dummy fd
- `io_uring` 系列完全未实现
- `flock` 可能是 no-op

**计划**：
1. 实现 `timerfd`（最常用，glibc/musl 的 timer 机制依赖它）
2. 实现 `inotify`（许多用户态程序依赖文件监控）
3. `io_uring` 可作为长期目标

### 改进 4：网络子系统完善

**现状**：
- socket 基本功能已实现
- 但 `setsockopt` / `getsockopt` 的选项支持可能不完整
- Unix domain socket 支持程度不明确
- `recvmmsg` / `sendmmsg` 未实现

**计划**：
1. 完善 socket option 处理（SO_REUSEADDR、SO_KEEPALIVE 等）
2. 完善 Unix domain socket（SOCK_STREAM + SOCK_DGRAM）
3. 添加 `recvmmsg` / `sendmmsg` 支持

### 改进 5：ELF 加载器增强

**现状**：
- `ELF_LOADER` 使用全局 `Mutex`，同一时刻只能加载一个 ELF——多进程 `execve` 串行化
- 不支持 `execveat`（AT_EMPTY_PATH 等）
- 解释器（dynamic linker）支持情况需要验证

**计划**：
1. 消除 `ELF_LOADER` 全局锁，改为局部变量
2. 添加 `execveat` 系统调用
3. 完善 ELF 辅助向量（auxv）的所有条目

### 改进 6：wait 系统调用族不完整

**现状**：
- 仅实现 `wait4`（映射 `Sysno::wait4`）
- 缺少 `waitid`

**计划**：
1. 实现 `waitid`，支持更丰富的等待选项
2. 完善 `WUNTRACED`、`WCONTINUED` 标志（依赖信号 STOP/CONT 的实现）

### 改进 7：用户/权限模型不真实

**现状**：
- `getuid` 等固定返回 0（root）
- `setuid` / `setgid` 等是 no-op（直接返回 Ok）
- `capget` / `capset` 直接返回 Ok
- 没有真正的权限检查

**计划**：
1. 在 `ProcessData` 中添加 uid/gid/groups 字段
2. 实现文件权限检查（open 时检查 rwx）
3. 实现 capability 子集

### 改进 8：测试基础设施薄弱

**现状**：
- 没有任何 Rust `#[test]` 单元测试
- CI 测试仅验证"能否启动到 shell 提示符"
- 没有 syscall 级别的回归测试

**计划**：
1. 建立 syscall 测试框架（C 语言用户态测试程序集）
2. 引入 LTP（Linux Test Project）子集作为兼容性测试
3. 添加内核关键路径的单元测试（如 futex、信号）

### 改进 9：缺少 POSIX 定时器

**现状**：
- `timer_create` / `timer_gettime` / `timer_settime` 直接返回 `Ok(0)`（no-op）
- `timerfd_create` 返回 dummy fd
- `setitimer` / `getitimer` 仅支持 `ITIMER_REAL`

**计划**：
1. 实现 `timer_create` 基于进程级定时器表
2. 实现 `timer_settime` 注册到内核定时器系统
3. 实现 `timerfd_create` / `timerfd_settime` / `timerfd_gettime`

### 改进 10：共享内存与 IPC 健壮性

**现状**：
- `shmget` / `shmat` / `shmdt` / `shmctl` 已实现
- `msgget` / `msgsnd` / `msgrcv` / `msgctl` 已实现
- 但缺少 `semget` / `semop` / `semctl`（POSIX 信号量）
- IPC 命名空间隔离不存在

**计划**：
1. 实现 System V 信号量
2. 考虑添加 POSIX 信号量（`sem_open` 等基于文件的信号量）
3. 完善 IPC 资源限制

---

## 3. Linux Syscall 支持能力与缺陷分析

### 3.1 已实现 Syscall 分类统计

通过对 `kernel/src/syscall/mod.rs` 中 `handle_syscall` 的 match arms 进行源码分析，Starry 目前实现了约 **160+ 个 syscall 入口点**（包括架构特定的变体）。按类别统计：

| 类别 | 已实现数量 | 代表性 Syscall |
|------|-----------|---------------|
| 文件系统控制 | ~25 | ioctl, chdir, mkdir, unlinkat, getcwd, rename, sync |
| 文件操作 | ~15 | open, close, dup, fcntl, flock |
| I/O 读写 | ~20 | read, write, pread64, sendfile, splice, copy_file_range |
| I/O 多路复用 | ~7 | poll, ppoll, select, pselect6, epoll_* |
| 内存管理 | ~10 | brk, mmap, munmap, mprotect, mremap, madvise, msync |
| 进程/线程 | ~15 | clone, clone3, execve, exit, wait4, getpid, set_tid_address |
| 信号 | ~12 | rt_sigaction, rt_sigprocmask, kill, tkill, rt_sigreturn |
| 调度 | ~8 | sched_yield, nanosleep, sched_getaffinity |
| 网络 | ~15 | socket, bind, connect, listen, accept4, sendto, recvfrom |
| 时间 | ~6 | clock_gettime, gettimeofday, setitimer |
| IPC | ~8 | msgget, msgsnd, shmget, shmat, futex |
| 同步 | ~3 | futex, membarrier, get/set_robust_list |
| 系统信息 | ~10 | uname, sysinfo, getrandom, getuid, prctl |
| 特殊文件 | ~7 | pipe2, eventfd2, pidfd_open, memfd_create, signalfd4 |

### 3.2 关键缺陷分析

#### 缺陷 1：Dummy/Stub 实现

以下 syscall 有入口但实际是 dummy 或 stub：

```rust
// 返回 dummy fd（永远不触发事件的文件描述符）
Sysno::timerfd_create | Sysno::fanotify_init | Sysno::inotify_init1
| Sysno::userfaultfd | Sysno::perf_event_open | Sysno::io_uring_setup
| Sysno::bpf | Sysno::fsopen | Sysno::fspick | Sysno::open_tree
| Sysno::memfd_secret => sys_dummy_fd(sysno),

// 直接返回 Ok(0)，不做任何操作
Sysno::timer_create | Sysno::timer_gettime | Sysno::timer_settime => Ok(0),
```

- `sched_setscheduler` / `sched_getparam` 直接返回 Ok，不改变任何状态
- `seccomp` 直接返回 Ok，不实施任何安全策略
- `flock` 可能仅记录状态但不实际阻塞
- `capget` / `capset` 直接返回 Ok

#### 缺陷 2：完全缺失的重要 Syscall

| Syscall | 重要性 | 影响 |
|---------|--------|------|
| `waitid` | 高 | 部分程序使用更灵活的 wait 接口 |
| `execveat` | 中 | `fexecve()` 依赖它 |
| `timerfd_settime/gettime` | 高 | event loop 框架普遍使用 |
| `inotify_add_watch/rm_watch` | 中 | 文件监控 |
| `semget/semop/semctl` | 中 | System V 信号量 |
| `setpriority` | 低 | `nice` 命令 |
| `recvmmsg/sendmmsg` | 低 | 高性能网络 |
| `io_uring_enter/register` | 低 | 新一代异步 I/O |
| `ptrace` | 低 | 调试器（strace/gdb） |
| `personality` | 低 | 运行兼容模式 |
| `clock_settime` | 低 | 时钟设置 |
| `mq_open/mq_send/mq_receive` | 低 | POSIX 消息队列 |

#### 缺陷 3：语义不完整的已实现 Syscall

1. **`clone`**：不支持所有 clone flags（如 `CLONE_NEWNS`, `CLONE_NEWPID` 等 namespace 相关标志）
2. **`ioctl`**：仅支持终端相关的 ioctl 和部分通用 ioctl，大量设备 ioctl 缺失
3. **`mmap`**：`MAP_HUGETLB` 未支持，大页映射缺失
4. **`fcntl`**：`F_SETLK` / `F_GETLK`（文件锁）可能不完整
5. **`prctl`**：仅实现部分选项
6. **`setsockopt`**：支持的选项集可能很有限
7. **`mount`**：支持的文件系统类型有限
8. **`getdents64`**：对特殊文件系统可能有兼容性问题

### 3.3 架构特定 Syscall 差异

Starry 对不同架构维护了不同的 syscall 集合：

- **x86_64 专有**：`open`（vs. `openat`），`stat`/`lstat`（vs. `fstatat`），`fork`（vs. `clone`），`poll`（vs. `ppoll`），`arch_prctl` 等旧式接口
- **riscv64 专有**：`riscv_flush_icache`
- **通用**：`openat`、`fstatat`、`ppoll`、`clone3` 等新式接口在所有架构可用

这体现了 Linux ABI 的历史演进——新架构（如 riscv64）不再提供旧接口。

---

## 4. Syscall 优先实现排序

### 排序原则

1. **依赖链底层优先**：基础 syscall 被更多上层功能依赖
2. **用户态库依赖优先**：musl libc / glibc 内部依赖的 syscall
3. **测试套件通过率优先**：能解锁更多 LTP / busybox 测试
4. **实现复杂度与收益比**：低成本高收益优先

### 排序结果

#### 第一梯队：基础补全（影响最广，解锁最多用户程序）

| 优先级 | Syscall | 理由 |
|--------|---------|------|
| 1 | `timerfd_create/settime/gettime` | 几乎所有 event loop（libuv, tokio, epoll-based 服务器）依赖 timerfd；当前 dummy 实现导致定时器永不触发 |
| 2 | `timer_create/settime/gettime/delete` | POSIX 定时器，musl 的 `sleep()` 在某些情况下使用；当前 no-op 可能导致死等 |
| 3 | `waitid` | systemd-like init 进程和一些程序使用 `waitid` 替代 `wait4`；实现简单（复用 wait 逻辑） |
| 4 | `setpriority` | 与已有的 `getpriority` 配对；`nice` 命令/`renice` 依赖它 |
| 5 | `execveat` | `fexecve()` 的内核支持；某些安全框架需要 |

#### 第二梯队：信号与进程控制完善

| 优先级 | Syscall | 理由 |
|--------|---------|------|
| 6 | 完善 `SIGSTOP/SIGCONT` | shell 的 job control（Ctrl+Z、`bg`、`fg`）依赖它；当前 stop 直接导致进程退出 |
| 7 | `clock_settime` | NTP 和时间同步工具需要 |
| 8 | `semget/semop/semctl` | System V 信号量，数据库（PostgreSQL）和部分服务端程序使用 |
| 9 | `setitimer` 完善 | 当前仅支持 `ITIMER_REAL`，`ITIMER_VIRTUAL` / `ITIMER_PROF` 是性能分析工具的基础 |

#### 第三梯队：文件系统与监控

| 优先级 | Syscall | 理由 |
|--------|---------|------|
| 10 | `inotify_init1/add_watch/rm_watch` | 文件变化监控，build 系统（make）、编辑器、包管理器广泛使用 |
| 11 | `fcntl` 文件锁完善（`F_SETLK/F_GETLK`） | 数据库和需要并发文件访问的程序 |
| 12 | `flock` 真正实现 | 包管理器、日志系统依赖文件锁 |
| 13 | `fanotify_init/mark` | 安全监控和杀毒软件框架 |

#### 第四梯队：高级特性

| 优先级 | Syscall | 理由 |
|--------|---------|------|
| 14 | `ptrace` | 调试器（gdb/strace）的基础；实现复杂但对开发极有价值 |
| 15 | `io_uring` 系列 | 现代高性能 I/O 框架；实现非常复杂，可作为长期目标 |
| 16 | `bpf` | eBPF 程序运行；极其复杂，可考虑子集 |
| 17 | `clone` namespace 支持 | 容器化的基础；需要大量 kernel 改动 |

### 排序理由总结

这个排序遵循"**自底向上、先宽后深**"的原则：
- **timerfd/timer** 排第一是因为大量现实世界程序（web 服务器、数据库、shell 脚本中的 sleep）会因为 dummy 实现而静默失败或行为异常
- **waitid** 和 **setpriority** 排在前面因为实现成本低、影响面适中
- **信号完善** 排在第二梯队因为它虽然重要（影响 shell 交互），但实现涉及较多状态机改动
- **ptrace/io_uring/bpf** 放在最后因为它们实现复杂度极高，投入产出比较低

---

## 5. AI 自动迭代编程方法设计

### 5.1 核心思想

设计一个 **"测试驱动的 AI 内核改进循环"**（Test-Driven AI Kernel Improvement Loop, TDAKIL），核心流程为：

```
发现缺陷 → 编写测试 → 实现修复 → 验证通过 → 回归测试 → 寻找下一个缺陷
```

### 5.2 系统架构

```
┌──────────────────────────────────────────────────────────┐
│                   AI 编程主控制器                          │
│  (Prompt + 上下文管理 + 迭代决策)                         │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────┐  ┌───────────┐  ┌───────────┐             │
│  │ 缺陷探测器│  │ 测试生成器 │  │ 代码修复器 │             │
│  │ (Probe)  │  │ (TestGen) │  │ (Fixer)   │             │
│  └─────┬────┘  └─────┬─────┘  └─────┬─────┘             │
│        │             │              │                    │
│        v             v              v                    │
│  ┌─────────────────────────────────────────┐             │
│  │          QEMU 测试运行环境               │             │
│  │  (rootfs + 编译好的测试程序 + 串口输出)   │             │
│  └─────────────────────────────────────────┘             │
│                      │                                   │
│                      v                                   │
│  ┌─────────────────────────────────────────┐             │
│  │          结果分析器 (Analyzer)            │             │
│  │  (解析串口日志, 判断 pass/fail/crash)     │             │
│  └─────────────────────────────────────────┘             │
└──────────────────────────────────────────────────────────┘
```

### 5.3 详细方法设计

#### Phase 1: 缺陷探测（Probe Phase）

**方法 A: Syscall 覆盖率扫描**

```python
# AI Prompt 模板
"""
分析 Starry OS 的 syscall/mod.rs，找出所有 match arms 中：
1. 返回 sys_dummy_fd 的 syscall
2. 直接返回 Ok(0) 不做实际操作的 syscall  
3. 匹配到 _ (default arm) 的 syscall（通过与 Linux syscall 全表对比）
4. 内部包含 TODO/FIXME/unimplemented 注释的实现

对每个找到的缺陷，评估：
- 哪些实际用户程序会触发它
- 触发后会导致什么后果（静默失败 vs 崩溃 vs 功能异常）
- 实现它的难度（需要改动哪些模块）

输出格式：
{ "syscall": "timerfd_create", "status": "dummy", "impact": "high",
  "affected_programs": ["nginx", "redis", "libuv-based apps"],
  "fix_difficulty": "medium", "modules": ["file/", "syscall/fs/"] }
"""
```

**方法 B: 动态测试探测**

```c
// 自动生成的探测程序模板
#include <sys/syscall.h>
#include <stdio.h>

int main() {
    // 系统性地调用每个 syscall，观察返回值
    long ret;
    
    // Test: timerfd_create
    ret = syscall(SYS_timerfd_create, CLOCK_MONOTONIC, 0);
    printf("PROBE timerfd_create: ret=%ld errno=%d\n", ret, errno);
    
    // Test: inotify_init1
    ret = syscall(SYS_inotify_init1, 0);
    printf("PROBE inotify_init1: ret=%ld errno=%d\n", ret, errno);
    
    // ... 对每个可能缺失的 syscall 重复
    return 0;
}
```

#### Phase 2: 测试用例生成（TestGen Phase）

**系统提示词（System Prompt for Test Generation）**：

```
你是一个 Linux 内核系统调用测试专家。你的任务是为实验性 OS 内核 Starry 编写用户态测试程序。

规则：
1. 测试程序用 C 语言编写，使用 musl-libc 静态链接编译
2. 每个测试程序专注测试一个 syscall 或一组相关 syscall
3. 使用 assert() 或自定义 CHECK() 宏验证返回值和副作用
4. 输出格式必须为: "TEST <name>: PASS" 或 "TEST <name>: FAIL <reason>"
5. 测试应覆盖: 正常路径、边界条件、错误路径、并发场景
6. 测试程序必须能在 Starry OS 上运行（无 glibc 特有依赖）

当前已知 Starry 的限制：
- UID 固定为 0
- 不支持 namespace
- 部分 ioctl 未实现
- timerfd/inotify 是 dummy 实现

模板：
```c
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <sys/syscall.h>

#define CHECK(cond, msg) do { \
    if (!(cond)) { \
        printf("TEST %s: FAIL %s (line %d, errno=%d)\n", \
               __func__, msg, __LINE__, errno); \
        return 1; \
    } \
} while(0)

#define PASS() printf("TEST %s: PASS\n", __func__)

// ... test functions ...

int main() {
    int failures = 0;
    failures += test_xxx();
    // ...
    printf("SUMMARY: %d failures\n", failures);
    return failures ? 1 : 0;
}
```
```

#### Phase 3: 编译与执行（Build & Run Phase）

```bash
#!/bin/bash
# auto-test.sh - 自动编译和运行测试

ARCH=${1:-riscv64}
TEST_DIR="tests/userspace"
ROOTFS="rootfs-${ARCH}.img"

# 1. 交叉编译所有测试程序
for src in ${TEST_DIR}/*.c; do
    name=$(basename "$src" .c)
    ${ARCH}-linux-musl-gcc -static -o "${TEST_DIR}/${name}" "$src" -lpthread
done

# 2. 将测试程序注入 rootfs
mkdir -p /tmp/mnt
mount -o loop "$ROOTFS" /tmp/mnt
cp ${TEST_DIR}/test_* /tmp/mnt/bin/
# 修改 init.sh 来运行测试
cat > /tmp/mnt/run_tests.sh << 'EOF'
#!/bin/sh
for test in /bin/test_*; do
    echo "=== Running $test ==="
    "$test"
    echo "=== Exit code: $? ==="
done
EOF
chmod +x /tmp/mnt/run_tests.sh
umount /tmp/mnt

# 3. 编译内核并在 QEMU 中运行，捕获串口输出
make ARCH=${ARCH} build
timeout 120 make ARCH=${ARCH} ACCEL=n justrun \
    QEMU_ARGS="-monitor none -serial file:test_output.log" &
wait

# 4. 解析输出
python3 parse_results.py test_output.log
```

#### Phase 4: 结果分析与迭代决策

```python
# parse_results.py 的 AI 分析提示词
"""
分析以下 Starry OS 测试输出日志。

对于每个失败的测试：
1. 确定失败的 syscall 和具体参数
2. 在 kernel/src/syscall/ 中定位相关实现代码
3. 分析失败原因（未实现 / 参数处理错误 / 返回值错误 / 边界条件）
4. 生成修复方案（具体的代码改动建议）
5. 评估修复的风险（是否可能影响其他 syscall）

输出修复优先级列表和具体的代码 patch。
"""
```

### 5.4 自动迭代 Prompt Chain

```
迭代循环 Prompt:

Round N:
1. [探测] "基于上一轮的测试结果和修复记录，分析当前 Starry 的下一个最值得修复的 syscall 缺陷是什么？给出理由。"

2. [测试] "为 {目标syscall} 编写一组全面的用户态测试程序，覆盖正常/异常/并发场景。输出 C 源代码。"

3. [运行] (自动编译、注入 rootfs、QEMU 运行、捕获输出)

4. [分析] "分析测试输出，{目标syscall} 的哪些测试用例失败了？原因是什么？"

5. [修复] "基于分析结果，给出 kernel/src/ 中需要修改的文件和具体代码改动。确保不破坏现有功能。"

6. [验证] (重新编译内核、运行同一组测试、确认全部通过)

7. [回归] (运行之前所有轮次的测试，确认没有退化)

8. [记录] "总结本轮修复的内容、改动的文件、新增的测试用例数量。更新进度追踪表。"

→ 回到步骤 1
```

### 5.5 MCP/Hook 集成设计

可以设计一个 Cursor MCP (Model Context Protocol) server 来自动化这个流程：

```typescript
// starry-dev-mcp/index.ts 
// MCP Tools 定义

tools: [
  {
    name: "probe_syscalls",
    description: "扫描 Starry 源码，找出未实现/stub 的 syscall",
    // 执行: grep + AST 分析 syscall/mod.rs
  },
  {
    name: "generate_test",
    description: "为指定 syscall 生成 C 语言测试程序",
    parameters: { syscall_name: string, test_type: "basic"|"edge"|"concurrent" }
  },
  {
    name: "build_and_run_test",
    description: "交叉编译测试程序，注入 rootfs，在 QEMU 中运行，返回测试输出",
    parameters: { test_files: string[], arch: string }
  },
  {
    name: "analyze_failure",
    description: "分析测试失败原因并定位内核源码",
    parameters: { test_output: string, syscall_name: string }
  },
  {
    name: "apply_fix",
    description: "应用代码修复并验证编译通过",
    parameters: { file_path: string, old_code: string, new_code: string }
  },
  {
    name: "run_regression",
    description: "运行所有历史测试用例，检查回归",
  }
]
```

### 5.6 关键 Skill（可复用的 AI 能力模块）

```yaml
skills:
  - name: "syscall_analyzer"
    trigger: "分析 syscall 实现"
    context: |
      你熟悉 Linux syscall ABI（参数传递、返回值约定、errno 语义）。
      你知道 Starry 使用 `syscalls::Sysno` 进行 syscall 编号匹配。
      返回值通过 `AxResult<isize>` 传递，`AxError` 映射到 `LinuxError`。
    
  - name: "test_writer"  
    trigger: "编写用户态测试"
    context: |
      目标平台: riscv64/aarch64/loongarch64 + musl-libc 静态链接。
      不能使用 glibc 特有功能。不能依赖 /proc 的完整实现。
      输出格式: "TEST <name>: PASS/FAIL"
      
  - name: "kernel_fixer"
    trigger: "修复内核代码"
    context: |
      Starry 使用 Rust no_std 环境。
      错误处理用 AxError/AxResult。
      用户内存访问用 VmPtr/VmMutPtr。
      所有修改必须通过 `cargo clippy --target <target> -F qemu`。
```

### 5.7 预期效果

通过这个自动迭代系统：
- **每轮迭代**可以发现 1-3 个 syscall 级别的 bug 或缺失
- **自动生成**可复用的测试用例，持续积累回归测试库
- **AI 驱动**的修复建议可以覆盖 70-80% 的简单 stub/dummy 实现补全
- 复杂问题（如信号竞态、内存管理 bug）需要人工介入分析，但 AI 可以提供初始定位

---

## 附录 A：快速上手指南

### 环境搭建
```bash
# 安装依赖（已在环境中完成）
sudo apt install -y build-essential cmake clang qemu-system
# Musl 工具链已安装到 /opt/musl-toolchains/

# 验证编译
make ARCH=riscv64 build

# 下载 rootfs 并运行
make rootfs
make run
```

### 关键代码路径
```
kernel/src/syscall/mod.rs    # syscall 分发入口
kernel/src/task/user.rs      # 用户态主循环
kernel/src/task/signal.rs    # 信号处理
kernel/src/mm/aspace/mod.rs  # 地址空间管理
kernel/src/entry.rs          # 内核初始化
```

## 附录 B：面试准备要点

### 架构设计问题
- Starry 的分层架构：axhal（硬件抽象）→ axtask（调度）→ axruntime → starry-kernel（syscall 层）
- 与 Linux 宏内核的对比：Starry 基于 ArceOS unikernel 扩展成宏内核
- 可插拔组件设计：通过 Cargo features 控制功能开关

### 核心技术问题
- 页表管理与 page fault 处理流程
- CoW（Copy-on-Write）fork 的实现
- 信号从发送到处理的完整路径
- Futex 的等待/唤醒机制
- ELF 加载与用户空间初始化

### 并发/并行问题
- 多核启动流程
- 中断处理与锁的交互（SpinNoIrq 的必要性）
- 信号与系统调用的交互（EINTR 语义）
- 外设驱动中断对调度的影响
