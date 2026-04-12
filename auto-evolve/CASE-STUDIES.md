# Auto-Evolve 案例分析

以下是 5 个代表性案例，展示 Debugger 如何发现问题、Executor 如何修复、最终产生了什么代码变更。

---

## 案例 1：membarrier 仅用 compiler_fence（issue-005, critical）

### Debugger 发现

Debugger 审计 `kernel/src/syscall/sync/membarrier.rs` 时发现，所有非 QUERY 命令都走同一个分支：

```rust
// 修复前
_ => {
    compiler_fence(Ordering::SeqCst);  // ← 仅阻止编译器重排，不产生 CPU 屏障指令
    Ok(0)
}
```

`compiler_fence` 在多核场景下**完全没有硬件屏障效果**。依赖 `membarrier` 的用户态同步库（如 jemalloc 的 hazard pointer）可能出现数据竞争。

### Debugger 产出的测试

```c
// test_membarrier.c
TEST_BEGIN("membarrier QUERY returns supported commands") {
    int ret = syscall(SYS_membarrier, 0 /*QUERY*/, 0, 0);
    TEST_ASSERT(ret >= 0, "QUERY failed: %d", ret);
    // QUERY 应返回支持的命令位掩码
    TEST_ASSERT(ret != 0, "QUERY returned 0 (no commands supported)");
    TEST_PASS();
}

TEST_BEGIN("membarrier invalid command returns EINVAL") {
    int ret = syscall(SYS_membarrier, 0x80000000, 0, 0);
    TEST_ASSERT(ret == -1 && errno == EINVAL, "expected EINVAL, got ret=%d errno=%d", ret, errno);
    TEST_PASS();
}
```

### Executor 修复

```diff
-use core::sync::atomic::{Ordering, compiler_fence};
+use core::sync::atomic::{self, Ordering};

+#[inline]
+fn membarrier_cpu_fence() {
+    atomic::fence(Ordering::SeqCst);  // 产生真正的 CPU 内存屏障指令
+}

 match cmd {
     MEMBARRIER_CMD_QUERY => Ok(SUPPORTED_COMMANDS as isize),
-    _ => {
-        compiler_fence(Ordering::SeqCst);
+    MEMBARRIER_CMD_GLOBAL | ... | MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
+        membarrier_cpu_fence();
         Ok(0)
     }
+    _ => Err(AxError::InvalidInput),  // 未知命令返回 EINVAL
 }
```

**关键改动**：`compiler_fence` → `atomic::fence`，后者生成硬件级别的内存屏障指令（riscv64 上是 `fence iorw,iorw`，x86_64 上是 `mfence`），同时对未知命令返回 EINVAL 而非静默成功。

---

## 案例 2：信号检查全局竞态（issue-006, critical）

### Debugger 发现

`rt_sigreturn` 系统调用通过一个全局 `AtomicBool` 标记"跳过下一次信号检查"：

```rust
// 修复前 — task/signal.rs
static BLOCK_NEXT_SIGNAL_CHECK: AtomicBool = AtomicBool::new(false);

pub fn block_next_signal() {
    BLOCK_NEXT_SIGNAL_CHECK.store(true, Ordering::SeqCst);
}
```

在多核场景下，CPU-0 上的线程 A 设置的标志会被 CPU-1 上的线程 B 消费，导致 A 的信号检查没被跳过而 B 的被错误跳过。

### Executor 修复

在 `Thread` 结构体中增加 per-thread 字段，替换全局变量：

```diff
 pub struct Thread {
     // ...
+    /// Set by rt_sigreturn so the next return to user loop skips one check_signals pass.
+    skip_next_signal_check: AtomicBool,
 }

-static BLOCK_NEXT_SIGNAL_CHECK: AtomicBool = AtomicBool::new(false);
-
 pub fn block_next_signal() {
-    BLOCK_NEXT_SIGNAL_CHECK.store(true, Ordering::SeqCst);
+    current().as_thread().skip_next_signal_check.store(true, Ordering::SeqCst);
 }

 pub fn unblock_next_signal() -> bool {
-    BLOCK_NEXT_SIGNAL_CHECK.swap(false, Ordering::SeqCst)
+    current().as_thread().skip_next_signal_check.swap(false, Ordering::SeqCst)
 }
```

**改动量极小**（3 处引用点），但消除了一个在多核下可能导致信号丢失的竞态条件。

---

## 案例 3：timerfd 返回 dummy fd（issue-011, critical）

### Debugger 发现

`timerfd_create` 返回一个 `DummyFd`，其 `poll()` 永远返回空事件：

```rust
// 修复前 — syscall/fs/io.rs
struct DummyFd;
impl Pollable for DummyFd {
    fn poll(&self) -> IoEvents { IoEvents::empty() }  // 永远不可读
    fn register(&self, _: &mut Context<'_>, _: IoEvents) {}  // 不注册唤醒
}
```

所有 event loop 框架（libuv、tokio、glib）使用 `epoll_wait + timerfd` 实现定时器。dummy fd 导致定时器永远不触发，程序静默挂死。

### Debugger 产出的测试

```c
// test_timerfd.c
TEST_BEGIN("timerfd_create returns valid fd") {
    int fd = timerfd_create(CLOCK_MONOTONIC, 0);
    TEST_ASSERT(fd >= 0, "timerfd_create failed: %s", strerror(errno));
    close(fd);
    TEST_PASS();
}

TEST_BEGIN("timerfd fires after settime") {
    int fd = timerfd_create(CLOCK_MONOTONIC, 0);
    struct itimerspec ts = { .it_value = { .tv_nsec = 10000000 } }; // 10ms
    timerfd_settime(fd, 0, &ts, NULL);

    struct pollfd pfd = { .fd = fd, .events = POLLIN };
    int ret = poll(&pfd, 1, 1000);  // 等 1 秒
    TEST_ASSERT(ret == 1, "poll returned %d (expected 1)", ret);

    uint64_t expirations;
    read(fd, &expirations, sizeof(expirations));
    TEST_ASSERT(expirations >= 1, "expirations=%lu", expirations);
    close(fd);
    TEST_PASS();
}
```

### Executor 修复

创建了完整的 `kernel/src/file/timerfd.rs`（216 行）：

- `TimerFd` 结构体：维护时钟类型、截止时间、间隔、待处理到期次数
- 实现 `FileLike` trait：`read()` 返回 u64 到期次数，无到期时阻塞
- 实现 `Pollable` trait：到期时返回 `IoEvents::IN`，通过 `PollSet` 通知 epoll
- 后台定时器线程扫描所有 `TimerFd` 的弱引用，到期时唤醒 `PollSet`
- 在 `syscall/mod.rs` 中将 `timerfd_create` 从 `sys_dummy_fd` 改为真实实现

---

## 案例 4：flock 是空操作（issue-016, critical）

### Debugger 发现

```rust
// 修复前 — syscall/fs/fd_ops.rs
pub fn sys_flock(fd: c_int, operation: c_int) -> AxResult<isize> {
    debug!("flock <= fd: {fd}, operation: {operation}");
    // TODO: flock
    Ok(0)  // ← 假装加锁成功，实际什么都没做
}
```

包管理器（apk/dpkg）、数据库、日志系统用 `flock` 做文件互斥。假成功意味着**多进程可以同时写同一个文件，数据损坏**。

### Executor 修复

创建了 `kernel/src/file/flock.rs`（224 行），实现 BSD 风格的文件锁：

```rust
// 核心数据结构
struct FlockInodeKey { dev: u64, ino: u64 }  // 按 inode 标识锁

enum InodeLocks {
    Exclusive { task: u64, refs: u32 },       // 排他锁（一个持有者）
    Shared { map: BTreeMap<u64, u32> },       // 共享锁（多个持有者）
}

// 锁操作逻辑
LOCK_EX: 如果当前有其他进程持共享/排他锁 → LOCK_NB 时返回 EAGAIN，否则阻塞
LOCK_SH: 如果当前有排他锁（非自己）→ 同上
LOCK_UN: 释放锁
同一 fd 可以升级（SH→EX）/ 降级（EX→SH）
close(fd) 时自动释放该 fd 的锁
```

---

## 案例 5：SIGSTOP 杀死进程（issue-026, critical）

### Debugger 发现

用户按 Ctrl+Z 发送 `SIGTSTP` 时，预期行为是暂停前台进程并回到 shell。但 Starry 的默认处理是直接杀死进程：

```rust
// 修复前 — task/signal.rs
SignalOSAction::Stop => {
    // TODO: implement stop
    do_exit(1, true);        // ← 直接杀死进程！
}
SignalOSAction::Continue => {
    // TODO: implement continue  // ← 什么都不做
}
```

### Debugger 产出的测试

```c
// test_sigstop_sigcont.c
TEST_BEGIN("SIGSTOP suspends child, SIGCONT resumes") {
    pid_t pid = fork();
    if (pid == 0) {
        while(1) pause();  // 子进程等待信号
        _exit(0);
    }
    kill(pid, SIGSTOP);
    int status;
    waitpid(pid, &status, WUNTRACED);
    TEST_ASSERT(WIFSTOPPED(status), "child not stopped");
    TEST_ASSERT(WSTOPSIG(status) == SIGSTOP, "wrong stop signal");

    kill(pid, SIGCONT);
    // 子进程应该恢复运行
    kill(pid, SIGTERM);
    waitpid(pid, &status, 0);
    TEST_ASSERT(WIFSIGNALED(status), "child not terminated after CONT+TERM");
    TEST_PASS();
}
```

### Executor 修复

**三处联动改动**：

1. **`task/mod.rs`**：新增 `JobCtl` 结构体到 `ProcessData`

```rust
pub struct JobCtl {
    pub stop_sig: Option<u8>,
    pub stop_wait_pending: bool,
    pub continued_wait_pending: bool,
}
```

2. **`task/signal.rs`**：Stop 不再杀进程，而是设置标志并等待 SIGCONT

```diff
 SignalOSAction::Stop => {
-    do_exit(1, true);
+    // 设置停止状态，唤醒父进程的 waitpid
+    let mut jc = proc_data.jobctl.lock();
+    jc.stop_sig = Some(signo as u8);
+    jc.stop_wait_pending = true;
+    drop(jc);
+    // 通知父进程
+    parent.child_exit_event.wake();
+    // 阻塞等待 SIGCONT
+    while !received_sigcont() { check_signals(...); yield_now(); }
 }

 SignalOSAction::Continue => {
+    let mut jc = proc_data.jobctl.lock();
+    if jc.stop_sig.take().is_some() {
+        jc.continued_wait_pending = true;
+    }
 }
```

3. **`syscall/task/wait.rs`**：`waitpid` 支持 `WUNTRACED` 和 `WCONTINUED`

```diff
+let report_stop = options.contains(WaitOptions::WUNTRACED) || options.is_empty();
+let report_continued = options.contains(WaitOptions::WCONTINUED) || options.is_empty();
+
+for child in &children {
+    let Ok(data) = get_process_data(child.pid()) else { continue };
+    let mut jc = data.jobctl.lock();
+    if report_stop && jc.stop_wait_pending {
+        let sig = jc.stop_sig.unwrap_or(0) as i32;
+        exit_code.vm_write((sig << 8) | 0x7f)?;  // WIFSTOPPED 格式
+        jc.stop_wait_pending = false;
+        return Ok(Some(child.pid() as _));
+    }
+}
```

**效果**：Ctrl+Z 现在正确暂停进程，shell 的 `bg`/`fg`/`jobs` 命令可以正常工作。

---

## 总结

| 案例 | 问题类型 | 发现手段 | 修复规模 | 影响 |
|------|---------|---------|---------|------|
| membarrier | 语义错误 | 源码审计 | 改 1 文件，+15 -4 行 | 多核内存安全 |
| 信号竞态 | 并发 bug | 推理发现 | 改 2 文件，+10 -5 行 | 信号可靠性 |
| timerfd | dummy stub | 源码审计 | 新增 1 文件（216 行）| 所有 event loop |
| flock | no-op stub | 源码审计 | 新增 1 文件（224 行）| 文件并发安全 |
| SIGSTOP | 语义错误 | 对比分析 | 改 3 文件，+50 行 | Shell job control |
