# Auto-OS: AI 驱动的操作系统内核自动迭代体系

Auto-OS 是一个基于 AI Agent 的操作系统内核自动缺陷发现与修复系统。以 [Starry OS](https://github.com/Starry-OS/StarryOS)（2025 年全国大学生 OS 比赛内核赛道一等奖）为目标内核，通过两个协作 AI Agent 形成闭环：**Debugger 发现问题 → Executor 修复代码**。

## 项目结构

```
Auto-OS/
├── README.md                 # 本文件
├── Question1.md              # 多核支持能力分析 + 最值得改进的 10 个点
├── Question2.md              # Linux Syscall 支持能力与缺陷的源码级分析
├── BUG-SUMMARY.md            # 发现与修复的 bug 总结
│
├── auto-evolve/              # AI 自动迭代体系（核心）
│   ├── WORKFLOW.md           # Agent Workflow 设计文档
│   ├── CASE-STUDIES.md       # 5 个端到端案例分析
│   ├── stats-chart.png       # 统计可视化图表
│   ├── README.md             # 快速开始指南
│   │
│   ├── kernel.py             # 调度内核（消息队列 + Agent 管理 + 自适应调度）
│   ├── daemon.py             # 守护进程（健康检查 + 自动重启）
│   ├── dashboard.py          # TUI 仪表盘（Textual）
│   ├── monitor.py            # 快照监控（每小时）
│   ├── evolve                # 统一入口脚本
│   │
│   ├── skill-debugger        # Debugger Agent 提示词
│   ├── skill-executor        # Executor Agent 提示词
│   │
│   └── tests/                # Debugger 生成的 31 个 C 测试用例
│       ├── test_membarrier.c
│       ├── test_flock_stub.c
│       ├── test_timerfd.c
│       └── ...
│
└── starry-os/                # Starry OS 内核（git submodule）
    └── (cursor/exam-analysis-bb8f 分支，含所有修复)
```

## 系统架构

```
         ┌─────────────┐
         │   Human     │  拖文件 / 发消息 / 查看仪表盘
         └──────┬──────┘
                │
       ┌────────▼────────┐
       │   Kernel.py     │  调度内核：消息队列 + 优先级策略
       │   (调度引擎)     │
       └───┬─────────┬───┘
           │         │
    ┌──────▼──┐  ┌───▼──────┐
    │Debugger │  │ Executor │    ← cursor agent CLI
    │  Agent  │  │  Agent   │
    └────┬────┘  └────┬─────┘
         │            │
    发现问题       修复代码
    写测试用例     git commit
         │            │
    ┌────▼────────────▼────┐
    │     issue-pool/      │    JSON 格式的问题生命周期
    │  open → resolved →   │
    │  verified → archive  │
    └──────────────────────┘
```

## 运行成果

在 3.5 小时的自动运行中：

| 指标 | 数值 |
|------|------|
| 发现 Issue 总数 | 245 |
| 修复 Issue 总数 | 244 |
| Git Commits | 426 |
| 测试用例 | 31 个 C 文件 |
| Critical bugs 修复 | 8 / 8（全部） |
| Executor 速率 | ~55 次/小时 |
| Debugger 速率 | ~44 次/小时 |

### 修复的 Critical 问题

1. **membarrier 仅用 compiler_fence** → 改用 `atomic::fence` 产生硬件屏障
2. **信号检查全局 AtomicBool 竞态** → 改为 per-thread 字段
3. **timerfd 返回 dummy fd** → 实现完整的 TimerFd（216 行）
4. **inotify/fanotify 返回 dummy fd** → 使用专用 anon_inode fd
5. **POSIX timer 假成功 Ok(0)** → 改返回 ENOSYS
6. **flock 空操作** → 实现 BSD flock 锁（224 行）
7. **fcntl 记录锁空操作** → 实现 POSIX 记录锁
8. **SIGSTOP 杀死进程** → 实现 JobCtl + waitpid WUNTRACED

## 快速开始

```bash
# 克隆（含子模块）
git clone --recursive https://github.com/yks23/Auto-OS.git
cd Auto-OS

# 安装依赖
pip3 install rich textual
curl https://cursor.com/install -fsS | bash

# 启动
./auto-evolve/evolve start

# 查看状态
./auto-evolve/evolve report

# TUI 仪表盘
./auto-evolve/evolve gui

# 拖文件给 debugger
./auto-evolve/evolve drop debugger Question1.md
```

## 调度策略

- **Executor**：持续取最高 severity 的 open issue 修复
- **Debugger**：
  - open < 10 → 找新问题
  - 10~30 → 休眠
  - open > 30 → 自动转为 executor 帮忙消化
- **手动操作**：priority=0 永远插队

详见 [auto-evolve/WORKFLOW.md](auto-evolve/WORKFLOW.md)。

## 文档

| 文档 | 内容 |
|------|------|
| [Question1.md](Question1.md) | 多核支持分析 + 5 个 SMP 问题 + 10 个改进点 |
| [Question2.md](Question2.md) | 160+ syscall 逐个分析 + 优先实现排序 |
| [BUG-SUMMARY.md](BUG-SUMMARY.md) | 发现与修复的 bug 分类总结 |
| [WORKFLOW.md](auto-evolve/WORKFLOW.md) | Agent 系统完整设计 |
| [CASE-STUDIES.md](auto-evolve/CASE-STUDIES.md) | 5 个端到端案例 |
