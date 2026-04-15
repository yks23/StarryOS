# Auto-Evolve Agent Workflow 设计文档

## 系统总览

Auto-Evolve 是一个 AI 驱动的内核缺陷发现与修复闭环系统。两个 AI agent（Debugger 和 Executor）通过文件系统协作，在调度内核的编排下自动迭代改进 Starry OS 内核。

```
                     ┌──────────────────┐
                     │   用户 (Human)    │
                     │  拖拽文件 / 发消息 │
                     └────────┬─────────┘
                              │ priority=0（最高）
                              ▼
┌─────────────────────────────────────────────────────────┐
│                    Kernel (kernel.py)                    │
│              调度内核 — 消息队列 + 策略引擎               │
│                                                         │
│  ┌─────────────┐    消息队列     ┌─────────────┐        │
│  │  msg-queue/  │◄──────────────│  msg-queue/  │        │
│  │  debugger/   │               │  executor/   │        │
│  └──────┬──────┘               └──────┬──────┘        │
│         │                              │                │
│    ┌────▼────┐                   ┌─────▼────┐          │
│    │Debugger │                   │ Executor │          │
│    │ Thread  │                   │  Thread  │          │
│    └────┬────┘                   └─────┬────┘          │
│         │                              │                │
└─────────┼──────────────────────────────┼────────────────┘
          │                              │
          ▼                              ▼
   ┌─────────────┐                ┌─────────────┐
   │ cursor agent│                │ cursor agent│
   │   CLI       │                │   CLI       │
   │ (debugger)  │                │ (executor)  │
   └──────┬──────┘                └──────┬──────┘
          │                              │
          │ 写入                          │ 修改
          ▼                              ▼
   ┌─────────────┐                ┌─────────────┐
   │ issue-pool/ │───────────────▶│ kernel/src/ │
   │ tests/      │    executor    │  (内核源码)  │
   │             │    读取 issue  │             │
   └─────────────┘    修复代码    └─────────────┘
          │                              │
          │ resolved                     │ git commit
          ▼                              ▼
   ┌─────────────┐                ┌─────────────┐
   │issue-archive│                │  git history │
   │ (已完成)    │                │  (修复记录)  │
   └─────────────┘                └─────────────┘
```

## Agent 角色定义

### Debugger（缺陷猎手）

**职责**：发现问题 → 写测试 → 写 issue JSON → 验证修复

**输入**：
- 内核源码（只读）
- executor-memory.md（了解修复进展）
- 用户拖入的分析文件

**输出**：
- `issue-pool/issue-NNN.json`（问题描述 + 代码定位）
- `tests/test_xxx.c`（可编译运行的测试用例）
- `memory/debugger-memory.md`（扫描进度 + 消息）

**问题发现手段**：
| 手段 | 描述 | 产出 severity |
|------|------|--------------|
| A. 源码审计 | 搜索 TODO/FIXME、stub/dummy 实现 | critical~high |
| B. 对比分析 | 与 Linux man page 对比语义偏差 | high~medium |
| C. 推理发现 | 推测并发 bug、子系统交互问题 | high~medium |
| D. 主动改进 | 提出易用性/性能/兼容性增强 | medium~low |

### Executor（修复工匠）

**职责**：取 issue → 读源码 → 修复 → 验证 → 标记完成

**输入**：
- `issue-pool/issue-NNN.json`（问题 + 测试 + 代码定位）
- 内核源码（读写）
- debugger-memory.md（了解 debugger 的建议）

**输出**：
- 修改后的 `kernel/src/` 源码
- 更新后的 issue JSON（status: resolved）
- `memory/executor-memory.md`（修复记录 + 代码知识）
- git commit（格式：`fix(module): description (issue-NNN)`）

## 数据协议

### Issue JSON Schema

```json
{
  "id": "issue-NNN",
  "title": "简短标题",
  "status": "open | in-progress | resolved | verified",
  "severity": "critical | high | medium | low",
  "category": "syscall-stub | syscall-missing | syscall-semantic | concurrency | correctness | improvement",
  "created_at": "ISO 8601",
  "description": "详细描述",
  "affected_syscalls": ["syscall_name"],
  "source_context": {
    "primary_files": ["kernel/src/path:line-range"],
    "related_files": ["..."],
    "key_insight": "问题根因"
  },
  "test": {
    "source_file": "auto-evolve/tests/test_xxx.c",
    "build_command": "riscv64-linux-musl-gcc -static ...",
    "pass_criteria": "所有 [TEST] 行 PASS，退出码 0"
  },
  "suggested_fix": "修复建议",
  "resolved_at": "ISO 8601 (executor 填)",
  "fix_summary": "修复摘要 (executor 填)",
  "files_changed": ["..."]
}
```

### Issue 生命周期

```
                   debugger 创建
                        │
                        ▼
    ┌──────────────────────────────────────┐
    │              open                     │ ◄── 卡住超时自动回退
    └──────────────────┬───────────────────┘
                       │ executor 取走
                       ▼
    ┌──────────────────────────────────────┐
    │           in-progress                 │ ── 超过 20 分钟自动回退 open
    └──────────────────┬───────────────────┘
                       │ executor 修复完成
                       ▼
    ┌──────────────────────────────────────┐
    │            resolved                   │
    └──────────────────┬───────────────────┘
                       │ debugger 回归验证通过
                       ▼
    ┌──────────────────────────────────────┐
    │            verified                   │
    └──────────────────┬───────────────────┘
                       │ kernel 自动归档
                       ▼
    ┌──────────────────────────────────────┐
    │    issue-archive/ (永久归档)          │
    └──────────────────────────────────────┘
```

## 调度策略

### Executor 调度（连续工作模式）

```
每个 tick:
  1. 检查 msg-queue/executor/ 有无手动消息 → 有则立即处理（priority=0）
  2. 否则 pick_next_issue() 按 severity 排序取最高优先级 open issue
  3. 发送修复指令给 agent CLI
  4. 等待完成 → 归档 resolved issue → 取下一个
```

### Debugger 调度（自适应模式）

```
每个 tick:
  1. 检查 msg-queue/debugger/ 有无手动消息 → 有则立即处理
  2. 计算 open issue 数量:
     - open < 10  → 「debug 模式」找新问题
     - 10~30      → 「sleep 模式」等待
     - open > 30  → 「executor 模式」帮忙消化积压
  3. 每 30 分钟定期唤醒:
     - open >= 10 → 帮忙消化
     - open < 10  → 找新问题
```

```
          open issues
  0 ──────── 10 ──────── 30 ──────── ∞
  │  debug   │   sleep   │  executor │
  │  找问题  │   等待    │  帮修bug  │
  └──────────┴───────────┴───────────┘
```

### 优先级体系

```
消息优先级:
  0  ← 用户手动操作（drop/send）    ← 永远插队
  10 ← 自动生成的任务               ← 正常排队

Issue severity 排序:
  critical > high > medium > low

Debugger 问题发现优先级:
  1. 假成功 stub（Ok(0) 但应该有逻辑）
  2. dummy fd（永不触发的文件描述符）
  3. 语义偏差（行为与 Linux 不一致）
  4. 并发问题（多核竞态条件）
  5. 边界条件（参数校验不足）
  6. 功能缺失（返回 ENOSYS）
  7. 主动改进（性能/易用性/兼容性增强）
```

## 进程架构

```
daemon.py (守护进程, 可选)
  │  健康检查每 10s / 自动重启 / 最多 5 次
  │
  ├── kernel.py (调度内核)
  │     │  主循环: 每 5s tick
  │     │  归档 resolved/verified issue
  │     │  检测卡住的 in-progress
  │     │
  │     ├── executor-loop (线程)
  │     │     调用 cursor agent CLI
  │     │     --print --trust --yolo --output-format json
  │     │     --resume <session_id>  (维持上下文)
  │     │
  │     └── debugger-loop (线程)
  │           同上，自适应模式切换
  │
  └── monitor.py (快照监控, 可选)
        每 1h 拍快照 + 生成 progress-report.json
```

## Agent CLI 调用方式

```bash
# 首次调用（发送 skill prompt + 任务）
agent --print --trust --yolo \
  --output-format json \
  --workspace /workspace \
  --api-key $KEY \
  "<skill 内容>\n---\n<任务消息>"

# 后续调用（resume 会话 + 新任务）
agent --resume <session_id> \
  --print --trust --yolo \
  --output-format json \
  --workspace /workspace \
  --api-key $KEY \
  "<任务消息>"
```

返回 JSON 包含 `session_id`，用于下次 `--resume` 维持对话上下文。

## 共享记忆区

两个 agent 通过 `memory/` 目录异步通信：

```
memory/
├── debugger-memory.md    ← debugger 写，executor 读
│   - 扫描进度（已审计/未审计模块）
│   - 活跃问题摘要
│   - 给 executor 的建议
│   - 待验证列表
│
└── executor-memory.md    ← executor 写，debugger 读
    - 修复历史表
    - 当前卡点
    - 代码知识积累
    - 给 debugger 的消息（新发现的线索）
```

## 用户交互

```bash
./auto-evolve/evolve start [-d]    # 启动系统（-d 后台）
./auto-evolve/evolve stop          # 停止
./auto-evolve/evolve status        # 查看状态
./auto-evolve/evolve gui           # TUI 仪表盘

# 手动操作（插队到最高优先级）
./auto-evolve/evolve drop debugger Question1.md   # 拖文件 → 批量生成 issue
./auto-evolve/evolve drop executor issue-001.json  # 指定修复某个 issue
./auto-evolve/evolve send debugger "检查 mmap"     # 发消息
./auto-evolve/evolve send executor "修复 timerfd"   # 发消息

# 监控
./auto-evolve/evolve report        # 打印进度报告
./auto-evolve/evolve snap          # 立即拍快照
./auto-evolve/evolve history       # 查看历史曲线
./auto-evolve/evolve monitor       # 启动每小时定时快照
```

## 文件结构

```
auto-evolve/
├── evolve              # 统一入口脚本
├── kernel.py           # 调度内核
├── daemon.py           # 守护进程
├── dashboard.py        # TUI 仪表盘
├── monitor.py          # 快照监控
├── skill-debugger      # Debugger 角色提示词
├── skill-executor      # Executor 角色提示词
├── WORKFLOW.md         # 本文件
├── README.md           # 快速开始指南
├── stats-chart.png     # 统计可视化图表
├── .gitignore          # 排除运行时文件
│
├── issue-pool/         # 待处理问题 (open/in-progress)
├── issue-archive/      # 已完成问题 (resolved/verified)
├── tests/              # C 测试源码
├── memory/             # 共享记忆区
├── msg-queue/          # 消息队列
│   ├── debugger/
│   └── executor/
├── logs/               # 运行日志
└── snapshots/          # 定时快照
```

## 性能数据（实测）

基于 3.5 小时实际运行数据：

| 指标 | 数值 |
|------|------|
| 总 issue 发现 | 230+ |
| 总 issue 修复 | 200+ |
| 总 git commit | 250+ |
| Executor 平均每次 | 65 秒 |
| Debugger 平均每次 | 81 秒 |
| Executor 吞吐 | ~55 次/小时 |
| Debugger 吞吐 | ~44 次/小时 |
| 8 个 critical 全部修复 | ✅ |
| 峰值 commit 速率 | 68 commit/小时 (09:00) |
