# Starry OS 多核支持能力分析与最值得改进的点

## 一、多核支持能力现状分析

Starry 的多核能力并非集中在 `starry-kernel` 一个 crate 中，而是**分布在 ArceOS 组件栈的各个层次**。理解这一点是分析的前提。

### 1.1 架构分层与多核职责分布

```
┌─────────────────────────────────────────┐
│  starry-kernel (syscall 层)             │  ← 本仓库代码
│  提供 sched_setaffinity, membarrier 等  │
├─────────────────────────────────────────┤
│  axtask (调度器)                        │  ← 外部 crate
│  per-CPU 运行队列, AxCpuMask, spawn     │
├─────────────────────────────────────────┤
│  axhal (硬件抽象层)                      │  ← 外部 crate
│  多核启动, IPI, percpu, 中断路由         │
├─────────────────────────────────────────┤
│  硬件 (riscv64/aarch64/loongarch64)     │
└─────────────────────────────────────────┘
```

**SMP 启动流程**完全由 `axhal` 完成：主核（BSP）完成内核初始化后，通过架构特定的机制唤醒从核（Secondary CPUs）。`Cargo.toml` 中可以看到 SMP 是可选特性：

```toml
# /workspace/Cargo.toml
smp = ["axfeat/smp", "axplat-riscv64-visionfive2?/smp"]
```

`starry-kernel` 本身不直接管理 CPU 启动，而是通过 `axhal::cpu_num()` 获取核心数、通过 `axtask` 提交任务到多核调度器。

### 1.2 调度器层面的多核支持

根据 `kernel/Cargo.toml`，Starry 启用的调度策略是 **Round-Robin**：

```toml
"sched-rr",     # Round-Robin 调度器
"multitask",    # 多任务支持
```

从 `syscall/task/schedule.rs` 的实际实现来看：

**已实现的多核调度接口：**

| Syscall | 实现状态 | 源码位置 |
|---------|---------|---------|
| `sched_yield` | ✅ 完整 | 调用 `axtask::yield_now()` |
| `sched_getaffinity` | ⚠️ 部分 | 仅支持 `pid=0`（当前线程），其他 PID 返回 EPERM |
| `sched_setaffinity` | ⚠️ 部分 | 同上，仅当前线程 |
| `sched_getscheduler` | 🔴 stub | 硬编码返回 `SCHED_RR` |
| `sched_setscheduler` | 🔴 stub | 直接返回 `Ok(0)`，不做任何操作 |
| `sched_getparam` | 🔴 stub | 直接返回 `Ok(0)` |
| `getpriority` | ⚠️ 部分 | 始终返回 20（默认 nice 值），不支持设置 |

看一下 affinity 的具体实现（`syscall/task/schedule.rs:91-127`）：

```rust
pub fn sys_sched_getaffinity(pid: i32, cpusetsize: usize, user_mask: *mut u8) -> AxResult<isize> {
    if cpusetsize * 8 < axhal::cpu_num() {
        return Err(AxError::InvalidInput);
    }
    // TODO: support other threads
    if pid != 0 {
        return Err(AxError::OperationNotPermitted);
    }
    let mask = current().cpumask();
    let mask_bytes = mask.as_bytes();
    vm_write_slice(user_mask, mask_bytes)?;
    Ok(mask_bytes.len() as _)
}
```

注意那行 `// TODO: support other threads`——这意味着一个进程无法查询或设置其子线程的 CPU 亲和性，这对多线程程序（如 OpenMP、线程池模型）是一个功能缺失。

### 1.3 内核中的并发同步机制

Starry 内核中使用了四类同步原语：

**（1）`SpinNoIrq` / `SpinNoPreempt`（kspin crate）**

这是裸机内核中最基础的锁。`SpinNoIrq` 在持锁期间关闭中断，防止中断处理函数在同一个 CPU 上重入导致死锁。在 Starry 中广泛用于短期保护：

```rust
// task/futex.rs — FutexTable 的等待队列
pub struct WaitQueue {
    queue: SpinNoIrq<VecDeque<(Waker, u32)>>,
}
```

```rust
// mm/aspace/backend/cow.rs — 全局物理帧引用计数表
static FRAME_TABLE: SpinNoIrq<FrameTableRefCount> = SpinNoIrq::new(FrameTableRefCount::new());
```

**（2）`axsync::Mutex`（可阻塞互斥锁）**

用于可能长时间持有的临界区。持锁线程被阻塞时会让出 CPU：

```rust
// task/mod.rs — 进程地址空间
pub aspace: Arc<Mutex<AddrSpace>>,
```

这是**全内核最关键的一把锁**——整个进程的所有虚拟内存操作（page fault、mmap、munmap、mprotect、fork/clone 时的 try_clone）都必须先获取它。

**（3）`spin::RwLock`（自旋读写锁）**

用于读多写少的全局表：

```rust
// task/ops.rs — 全局任务/进程/进程组/会话表
static TASK_TABLE: RwLock<WeakMap<Pid, WeakAxTaskRef>> = RwLock::new(WeakMap::new());
static PROCESS_TABLE: RwLock<WeakMap<Pid, Weak<ProcessData>>> = RwLock::new(WeakMap::new());
static PROCESS_GROUP_TABLE: RwLock<WeakMap<Pid, Weak<ProcessGroup>>> = ...;
static SESSION_TABLE: RwLock<WeakMap<Pid, Weak<Session>>> = ...;
```

```rust
// file/mod.rs — 文件描述符表
pub static FD_TABLE: Arc<RwLock<FlattenObjects<FileDescriptor, AX_FILE_LIMIT>>> = ...;
```

**（4）`core::sync::atomic`（原子操作）**

用于无锁的状态标志：

```rust
// task/mod.rs — Thread 结构中的原子字段
clear_child_tid: AtomicUsize,
robust_list_head: AtomicUsize,
exit: Arc<AtomicBool>,
accessing_user_memory: AtomicBool,
umask: AtomicU32,
heap_top: AtomicUsize,
```

### 1.4 关键多核问题深入分析

#### 问题 1：地址空间大锁 — 多线程程序的性能瓶颈

**代码位置**：`task/mod.rs:197`

```rust
pub aspace: Arc<Mutex<AddrSpace>>,
```

**问题本质**：同一进程的所有线程共享一个 `AddrSpace`，而 `AddrSpace` 被单一 `Mutex` 保护。以下所有操作都需要获取这把锁：

| 操作 | 频率 | 持锁时间 |
|------|------|---------|
| Page fault 处理 | 极高（每次缺页） | 中等（可能涉及磁盘 I/O） |
| `mmap` / `munmap` | 中等 | 中等 |
| `mprotect` | 低 | 短 |
| `fork` 中的 `try_clone` | 低 | **极长**（需要遍历并复制所有 VMA） |
| `brk` | 中等 | 短 |

当一个多线程程序（比如 web 服务器，几十个工作线程）同时触发 page fault 时，所有线程**串行等待这把锁**。这在多核系统上是严重的性能瓶颈。

从 `task/user.rs:33` 可以看到 page fault 路径上确实要锁整个 aspace：

```rust
ReturnReason::PageFault(addr, flags) => {
    if !thr.proc_data.aspace.lock().handle_page_fault(addr, flags) {
        // SIGSEGV
    }
}
```

**优化方案**：

1. **短期**：将 `Mutex` 改为 `RwLock`。page fault 处理中的 `handle_page_fault` 大部分场景只读查询 VMA 区域，可以用读锁并行；只有 CoW 写时复制需要写锁
2. **中期**：引入 per-VMA 细粒度锁。参考 Linux 6.4 的 per-VMA lock 设计，每个 `MemoryArea` 有独立的锁，page fault 只锁对应的 VMA
3. **长期**：使用 RCU（Read-Copy-Update）保护 VMA 列表的查找路径，完全消除读操作的锁竞争

#### 问题 2：`membarrier` 实现不正确 — 多核内存模型错误

**代码位置**：`syscall/sync/membarrier.rs:20-33`

```rust
pub fn sys_membarrier(cmd: i32, flags: u32, _cpu_id: i32) -> AxResult<isize> {
    if flags != 0 {
        return Err(AxError::InvalidInput);
    }
    match cmd {
        MEMBARRIER_CMD_QUERY => Ok(SUPPORTED_COMMANDS as isize),
        _ => {
            compiler_fence(Ordering::SeqCst);  // ← 问题在这里
            Ok(0)
        }
    }
}
```

**问题本质**：`compiler_fence` **仅阻止编译器重排指令**，不生成任何 CPU 内存屏障指令。在多核系统上，CPU 可能乱序执行内存访问，`compiler_fence` 无法保证一个 CPU 上的 store 对另一个 CPU 可见。

`membarrier` 的正确语义是让**其他所有 CPU**（或指定进程的线程所在的 CPU）执行一次完整的内存屏障。Linux 的实现方式是通过 IPI（Inter-Processor Interrupt）强制所有目标 CPU 执行 `fence` / `dmb` / `mfence` 指令。

**影响**：任何依赖 `membarrier` 的用户态同步库（如 crossbeam、jemalloc、folly 的 hazard pointer 实现）在多核 Starry 上可能出现**极难复现的数据竞争 bug**。

**优化方案**：

```rust
// 伪代码
match cmd {
    MEMBARRIER_CMD_GLOBAL | MEMBARRIER_CMD_GLOBAL_EXPEDITED => {
        // 向所有 CPU 发送 IPI，每个 CPU 在 IPI handler 中执行 fence
        axhal::send_ipi_all(IpiAction::MemoryBarrier);
        // 等待所有 CPU 完成
    }
    MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
        // 只向运行当前进程线程的 CPU 发送 IPI
        for cpu_id in cpus_running_current_process() {
            axhal::send_ipi(cpu_id, IpiAction::MemoryBarrier);
        }
    }
    // ...
}
```

这需要 `axhal` 层提供 `send_ipi()` 原语，当前不确定 ArceOS 是否已暴露此接口。

#### 问题 3：信号检查的全局标志 — 多核竞态条件

**代码位置**：`task/signal.rs:43-51`

```rust
static BLOCK_NEXT_SIGNAL_CHECK: AtomicBool = AtomicBool::new(false);

pub fn block_next_signal() {
    BLOCK_NEXT_SIGNAL_CHECK.store(true, Ordering::SeqCst);
}

pub fn unblock_next_signal() -> bool {
    BLOCK_NEXT_SIGNAL_CHECK.swap(false, Ordering::SeqCst)
}
```

**问题本质**：这是一个**全局静态变量**，但它的语义是"跳过当前线程的下一次信号检查"。在多核场景下：

1. CPU-0 上的线程 A 调用 `block_next_signal()` 设置为 `true`
2. CPU-1 上的线程 B 在用户态循环中调用 `unblock_next_signal()` 读到 `true`
3. 线程 B 错误地跳过了自己的信号检查
4. 线程 A 的信号检查没有被跳过（它本来要跳过的）

这是一个典型的**逻辑竞态**。虽然原子操作保证了内存安全，但语义是错误的。

**优化方案**：将 `BLOCK_NEXT_SIGNAL_CHECK` 改为 per-thread 字段，放进 `Thread` 结构体中：

```rust
pub struct Thread {
    // ...
    block_next_signal: AtomicBool,
}
```

#### 问题 4：ELF 加载器全局锁 — 多进程 execve 串行化

**代码位置**：`mm/loader.rs:252`

```rust
static ELF_LOADER: Mutex<ElfLoader> = Mutex::new(ElfLoader::new());
```

`ElfLoader` 内部维护了一个 32 项的 LRU 缓存。每次 `execve` 都需要锁住这个全局 `Mutex`，加载 ELF 文件、解析头部、映射段——这个过程可能涉及磁盘 I/O，持锁时间可能很长。

**影响**：如果多个进程同时 `execve`（比如 shell 脚本中并行启动多个子进程），它们必须串行等待 ELF 加载完成。

**优化方案**：

1. 将 ELF 缓存改为并发安全的数据结构（如 `DashMap` 或分段锁的 `HashMap`），允许不同文件的加载并行进行
2. 或者用 `RwLock`，cache hit 路径用读锁，cache miss 路径用写锁

#### 问题 5：全局 Futex 表的锁粒度

**代码位置**：`task/futex.rs:269`

```rust
static SHARED_FUTEX_TABLES: Mutex<FutexTables> = Mutex::new(FutexTables::new());
```

跨进程的 shared futex 使用全局的 `FutexTables`（内部是 `BTreeMap`），每次 futex 操作都要锁这个全局表来查找或创建 `FutexTable`。

每 100 次操作还会做一次 GC（`retain` 清理过期条目），期间持锁：

```rust
fn get_or_insert(&mut self, key: usize) -> Arc<FutexTable> {
    self.operations += 1;
    if self.operations == 100 {
        self.operations = 0;
        self.map.retain(|_, table| Arc::strong_count(table) > 1 || !table.is_empty());
    }
    // ...
}
```

**优化方案**：使用分段锁或无锁哈希表替代全局 `Mutex<BTreeMap>`。

### 1.5 多核优化总结路线图

| 优先级 | 优化项 | 复杂度 | 风险 | 改动范围 |
|--------|--------|-------|------|---------|
| **P0** | 修复 `BLOCK_NEXT_SIGNAL_CHECK` 竞态 | 低 | 低 | `task/signal.rs` + `task/mod.rs` |
| **P0** | 修复 `membarrier` 实现 | 中 | 低 | `syscall/sync/membarrier.rs`，需 `axhal` IPI 支持 |
| **P1** | `AddrSpace` 锁改为 `RwLock` | 低 | 中 | `task/mod.rs`, `task/user.rs`, 所有锁住 aspace 的调用点 |
| **P1** | 完善 affinity 支持任意 PID | 低 | 低 | `syscall/task/schedule.rs` |
| **P2** | `ELF_LOADER` 锁粒度优化 | 中 | 中 | `mm/loader.rs` |
| **P2** | `SHARED_FUTEX_TABLES` 分段锁 | 中 | 中 | `task/futex.rs` |
| **P3** | AddrSpace per-VMA 锁 | 高 | 高 | `mm/aspace/` 全局重构 |
| **P3** | 多调度策略支持（CFS） | 高 | 高 | 依赖 `axtask` 重构 |

---

## 二、最值得改进的 10 个点

### 改进 1：信号 SIGSTOP / SIGCONT 未实现 — 影响 Shell 交互

**现状**（`task/signal.rs:21-39`）：

```rust
match os_action {
    SignalOSAction::Terminate => {
        do_exit(signo as i32, true);
    }
    SignalOSAction::CoreDump => {
        // TODO: implement core dump
        do_exit(128 + signo as i32, true);
    }
    SignalOSAction::Stop => {
        // TODO: implement stop
        do_exit(1, true);        // ← 收到 SIGSTOP 直接杀死进程！
    }
    SignalOSAction::Continue => {
        // TODO: implement continue  // ← SIGCONT 什么都不做
    }
    // ...
}
```

**影响**：用户在 shell 中按 `Ctrl+Z` 发送 `SIGTSTP`（属于 Stop 类信号），预期行为是暂停前台进程、回到 shell；但在 Starry 中，进程直接被杀死。`bg`、`fg`、`jobs` 等 job control 命令完全无法使用。

**改进计划**：
1. 在 `axtask` 中为任务添加 `Stopped` 状态（或在 `Thread` 中加一个 `stopped: AtomicBool` 标志）
2. 收到 Stop 类信号时，设置标志并让线程阻塞在一个等待队列上
3. 收到 SIGCONT 时，唤醒该等待队列
4. 向父进程发送 `SIGCHLD`（带 `CLD_STOPPED` / `CLD_CONTINUED` 信息）
5. 支持 `waitpid` 的 `WUNTRACED` / `WCONTINUED` 选项

### 改进 2：地址空间大锁 — 前面已详细分析

核心改动：`Mutex<AddrSpace>` → `RwLock<AddrSpace>`，page fault 路径使用读锁。

### 改进 3：`timerfd` 系列完全未实现 — 影响大量事件驱动程序

**现状**（`syscall/fs/io.rs:35-43`）：

```rust
pub fn sys_dummy_fd(sysno: Sysno) -> AxResult<isize> {
    if current().name().starts_with("qemu-") {
        return Err(AxError::Unsupported);
    }
    warn!("Dummy fd created: {sysno}");
    DummyFd.add_to_fd_table(false).map(|fd| fd as isize)
}
```

`timerfd_create` 返回一个 `DummyFd`，其 `poll()` 永远返回空事件集：

```rust
impl Pollable for DummyFd {
    fn poll(&self) -> IoEvents {
        IoEvents::empty()  // 永远不可读
    }
    fn register(&self, _context: &mut Context<'_>, _events: IoEvents) {}
}
```

**影响**：几乎所有现代 event loop 框架（libuv、tokio、glib mainloop）都使用 `timerfd` 实现定时器。使用 `epoll_wait` + `timerfd` 的程序将永远等不到定时事件触发。

**改进计划**：
1. 实现 `TimerFd` 结构体，内含定时器状态（到期时间、间隔、是否 armed）
2. 实现 `timerfd_create`、`timerfd_settime`、`timerfd_gettime` 三个 syscall
3. `TimerFd` 实现 `Pollable` trait，在定时器到期时返回 `IoEvents::IN`
4. 读取 `TimerFd` 返回到期次数（u64）

### 改进 4：`mremap` 实现过于粗暴 — 性能差且语义不正确

**现状**（`syscall/mm/mmap.rs:282-318`）：

```rust
pub fn sys_mremap(addr: usize, old_size: usize, new_size: usize, flags: u32) -> AxResult<isize> {
    // TODO: full implementation
    // ...
    let new_addr = sys_mmap(...)?;         // 分配全新区域
    let data = vm_load(addr.as_ptr(), copy_len)?;  // 拷贝旧数据到内核
    vm_write_slice(new_addr as *mut u8, &data)?;   // 写到新区域
    sys_munmap(addr.as_usize(), old_size)?;         // 释放旧区域
    Ok(new_addr as isize)
}
```

**问题**：
1. 使用了数据拷贝（内核中转）而非页表重映射，O(n) 内存拷贝，对大区域极慢
2. 没有处理 `MREMAP_MAYMOVE` / `MREMAP_FIXED` 标志
3. 新分配时强制使用 `MAP_PRIVATE | MAP_ANONYMOUS`，丢失了原映射的后端类型（shared、file-backed 等）
4. 如果是缩小映射（`new_size < old_size`），也做了完整的 alloc+copy+unmap，应该直接截断

**改进计划**：
1. 缩小时直接 `unmap` 尾部
2. 扩大时先尝试原地扩展（检查相邻空间是否空闲）
3. 如果必须移动，通过页表重映射（移动 PTE）而非数据拷贝
4. 正确处理 `MREMAP_MAYMOVE` / `MREMAP_FIXED` 标志

### 改进 5：`madvise` / `msync` / `mlock` 全是空操作

**现状**（`syscall/mm/mmap.rs`）：

```rust
pub fn sys_madvise(addr: usize, length: usize, advice: i32) -> AxResult<isize> {
    debug!("sys_madvise <= ...");
    Ok(0)  // 完全不处理
}

pub fn sys_msync(addr: usize, length: usize, flags: u32) -> AxResult<isize> {
    debug!("sys_msync <= ...");
    Ok(0)  // 不同步到磁盘
}

pub fn sys_mlock2(_addr: usize, _length: usize, _flags: u32) -> AxResult<isize> {
    Ok(0)  // 不锁定页面
}
```

**影响**：
- `madvise(MADV_DONTNEED)` 应该释放物理页（减少内存占用），不处理会导致内存膨胀
- `msync` 不同步意味着 mmap 写入的文件数据可能丢失
- `mlock` 不锁页意味着实时应用的延迟保证不成立

**改进计划**：
- 至少实现 `MADV_DONTNEED`（最常用，释放物理页但保留 VMA）
- 实现 `msync(MS_SYNC)` 将脏页写回文件后端
- `mlock` 可以在当前实现中标记 VMA 为 populate-on-map，确保不被换出（虽然 Starry 目前没有换页机制，但语义上应该标记）

### 改进 6：POSIX 定时器 `timer_create/settime/gettime` 是空操作

**现状**（`syscall/mod.rs:630`）：

```rust
Sysno::timer_create | Sysno::timer_gettime | Sysno::timer_settime => Ok(0),
```

直接返回 0，不创建任何定时器，不设置任何超时。

**影响**：musl libc 的 `sleep()` 在某些配置下使用 `timer_create` + `sigsuspend` 实现。如果 `timer_create` 静默成功但不触发信号，`sleep()` 可能永久阻塞。

**改进计划**：
1. 在 `ProcessData` 中维护一个定时器表 `timers: Vec<PosixTimer>`
2. `timer_create` 分配定时器 ID，记录信号通知方式（SIGEV_SIGNAL / SIGEV_THREAD_ID）
3. `timer_settime` 将定时器注册到内核 alarm 系统（复用 `timer.rs` 中的 `ALARM_LIST`）
4. 定时器到期时向目标进程/线程发送信号

### 改进 7：测试基础设施几乎为零

**现状**：
- 没有任何 Rust `#[test]` 单元测试
- CI 的 `ci-test.py` 仅验证"QEMU 能启动到 shell 提示符"
- 没有 syscall 级别的功能测试和回归测试

**影响**：无法及时发现回归 bug；无法量化 Linux 兼容性水平；新贡献者不敢改动核心代码。

**改进计划**：
1. 建立 C 语言用户态 syscall 测试套件，静态链接 musl，放入 rootfs
2. 每个重要 syscall 至少一组 "正常路径 + 错误路径" 测试
3. 修改 `ci-test.py`，在启动后自动运行测试程序，解析输出判定 pass/fail
4. 考虑引入 LTP（Linux Test Project）子集作为兼容性基准

### 改进 8：`/proc` 文件系统内容有限

**现状**（`pseudofs/proc.rs`）：procfs 已挂载，但内容不够完整。

**影响**：许多工具依赖 `/proc`：
- `ps` 需要 `/proc/[pid]/stat`、`/proc/[pid]/status`
- `top` / `htop` 需要 `/proc/meminfo`、`/proc/cpuinfo`、`/proc/[pid]/statm`
- `ldd` 需要 `/proc/self/exe`
- BusyBox 的 `mount` 命令需要 `/proc/mounts`

**改进计划**：
1. 实现 `/proc/self` 符号链接（指向当前进程的 PID 目录）
2. 完善 `/proc/[pid]/maps`（内存映射，从 `AddrSpace.areas()` 生成）
3. 实现 `/proc/meminfo`（从 `axalloc` 获取内存统计）
4. 实现 `/proc/cpuinfo`（从 `axhal` 获取 CPU 信息）

### 改进 9：`execve` 不支持多线程进程

**现状**（`syscall/task/execve.rs:50-54`）：

```rust
if proc_data.proc.threads().len() > 1 {
    // TODO: handle multi-thread case
    error!("sys_execve: multi-thread not supported");
    return Err(AxError::WouldBlock);
}
```

**影响**：多线程程序中如果某个线程调用 `execve`，应该杀死同进程的所有其他线程然后替换地址空间。当前直接返回错误。

**改进计划**：
1. 向同进程的其他线程发送 `SIGKILL`
2. 等待它们退出
3. 然后执行正常的 `execve` 流程

### 改进 10：CoW 帧引用计数使用 `u8` — 存在溢出风险

**现状**（`mm/aspace/backend/cow.rs:18-35`）：

```rust
struct FrameRefCnt(u8);

// clone_map 中：
frame.0 += 1;
if frame.0 == u8::MAX {
    warn!("frame reference count overflow");
    return Err(AxError::BadAddress);
}
```

**影响**：一个物理页最多被 255 个进程共享。如果一个进程 fork 超过 254 次（如 fork bomb 或高并发 web 服务器模型），后续 fork 会失败。

Linux 使用 `atomic_t`（32 位）或 `mapcount`（带特殊处理）来追踪页引用。

**改进计划**：将 `u8` 改为 `u32` 或 `AtomicU32`。这是一个低风险、低成本但能消除潜在限制的改动。

---

## 三、改进优先级排序

| 序号 | 改进项 | 紧迫性 | 实现难度 | 影响面 |
|------|--------|--------|---------|--------|
| 1 | 信号 SIGSTOP/SIGCONT | 高 | 中 | Shell 交互、job control |
| 2 | 地址空间大锁优化 | 高 | 低-中 | 多线程程序性能 |
| 3 | timerfd 实现 | 高 | 中 | 几乎所有 event-driven 程序 |
| 4 | 测试基础设施 | 高 | 中 | 开发流程、质量保证 |
| 5 | POSIX timer 实现 | 中 | 中 | sleep/alarm 语义正确性 |
| 6 | mremap 正确实现 | 中 | 中 | 内存分配器(malloc)性能 |
| 7 | /proc 完善 | 中 | 低 | 系统工具可用性 |
| 8 | madvise/msync 实现 | 中 | 低-中 | 内存效率、数据持久性 |
| 9 | 多线程 execve | 低 | 中 | 边界场景正确性 |
| 10 | CoW 引用计数扩容 | 低 | 极低 | 消除 fork 上限 |
