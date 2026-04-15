# Auto-Evolve: Starry OS 自动迭代体系

AI 驱动的内核缺陷发现与修复闭环系统。

## 架构

```
┌──────────────────────────────────────────────────────────────┐
│                          daemon.py                           │
│                     守护进程（健康检查）                        │
│                           │                                  │
│                      ┌────┴────┐                             │
│                      │kernel.py│                             │
│                      │ 调度内核 │                             │
│                      └────┬────┘                             │
│              ┌────────────┴────────────┐                     │
│         ┌────┴─────┐            ┌──────┴────┐                │
│         │ Debugger  │            │ Executor  │                │
│         │(agent CLI)│            │(agent CLI)│                │
│         └────┬─────┘            └──────┬────┘                │
│              │                         │                     │
│    ┌─────────┴──────────┐    ┌─────────┴──────────┐          │
│    │ 发现问题            │    │ 修复代码            │          │
│    │ 写测试 → issue-pool │───→│ issue-pool → 验证  │          │
│    │ 验证修复            │←───│ 标记 resolved       │          │
│    └────────────────────┘    └────────────────────┘          │
│                                                              │
│    ┌─────────────────────────────────────────────┐           │
│    │              memory/ (共享记忆区)             │           │
│    │  debugger-memory.md  ←→  executor-memory.md  │           │
│    └─────────────────────────────────────────────┘           │
│                                                              │
│    ┌──────────────────┐                                      │
│    │   dashboard.py    │    TUI 仪表盘（状态 + 操作）         │
│    └──────────────────┘                                      │
└──────────────────────────────────────────────────────────────┘
```

## 快速开始

```bash
# 安装依赖
pip3 install rich textual

# 启动（前台，可看日志）
./auto-evolve/evolve start

# 或后台启动
./auto-evolve/evolve start -d

# 打开 TUI 仪表盘（另一个终端）
./auto-evolve/evolve gui

# 查看状态
./auto-evolve/evolve status
```

## 手动操作

```bash
# 拖拽分析文件给 debugger → 自动转化为 issue + 测试用例
./auto-evolve/evolve drop debugger Question1.md

# 指定一个 issue 给 executor → 立即修复
./auto-evolve/evolve drop executor auto-evolve/issue-pool/issue-001.json

# 手动发消息
./auto-evolve/evolve send debugger "请检查 mmap 相关的 syscall"
./auto-evolve/evolve send executor "请优先修复 timerfd"

# 停止
./auto-evolve/evolve stop
```

## TUI 仪表盘快捷键

| 按键 | 功能 |
|------|------|
| `q` | 退出 |
| `r` | 刷新 |
| `d` | 输入 drop 命令 |
| `1` | 快速发消息给 debugger |
| `2` | 快速发消息给 executor |

命令栏支持：
- `drop debugger <file>` — 拖文件给 debugger
- `drop executor <file>` — 拖文件给 executor  
- `send debugger <msg>` — 发消息给 debugger
- `send executor <msg>` — 发消息给 executor
- `start/stop debugger/executor` — 启停 agent

## 文件结构

```
auto-evolve/
├── evolve              # 统一入口脚本
├── kernel.py           # 调度内核（消息队列 + agent 管理 + 自动调度）
├── daemon.py           # 守护进程（健康检查 + 自动重启）
├── dashboard.py        # TUI 仪表盘
├── skill-debugger      # Debugger agent 提示词
├── skill-executor      # Executor agent 提示词
├── issue-pool/         # 问题池（JSON 文件）
├── tests/              # 测试源码（C 文件）
├── memory/             # 共享记忆区
│   ├── debugger-memory.md
│   └── executor-memory.md
├── msg-queue/          # 消息队列
│   ├── debugger/
│   └── executor/
├── logs/               # 日志目录
├── kernel-state.json   # 内核运行状态
└── daemon-state.json   # 守护进程状态
```

## 调度逻辑

1. **手动操作优先**：用户 `drop`/`send` 的消息优先级为 0，自动消息优先级为 10
2. **空闲自动调度**：agent 空闲时 kernel 自动生成任务
   - debugger: 自动巡检 syscall → 发现问题 → 写测试
   - executor: 自动取最高优先级 open issue → 修复
3. **手动操作打断**：手动消息进入队列头部，下一个调度周期立即执行
4. **状态可见**：所有状态通过 JSON 文件暴露，TUI 每 3 秒刷新

## 接入 Cursor Agent CLI

`kernel.py` 中的 `AgentSession.send_message()` 方法预留了 cursor agent CLI 调用点。
当 `cursor-agent` CLI 可用时，取消注释即可接入真实的 AI agent：

```python
cmd = self._build_cmd(message, resume)
result = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
```

会话通过 `session_id` 维持上下文，支持 resume 继续对话。
