"""
Auto-Evolve Kernel: 调度内核
负责管理 debugger/executor agent 会话、消息队列、任务调度。
所有进程间通信走文件系统（JSON），GUI/daemon 通过读取状态文件获取信息。
"""

import json
import os
import time
import subprocess
import threading
import signal
import sys
import glob
import shutil
from pathlib import Path
from datetime import datetime
from enum import Enum
from dataclasses import dataclass, field, asdict
from typing import Optional

BASE_DIR = Path(__file__).parent
ISSUE_POOL = BASE_DIR / "issue-pool"
TESTS_DIR = BASE_DIR / "tests"
MEMORY_DIR = BASE_DIR / "memory"
STATE_FILE = BASE_DIR / "kernel-state.json"
MSG_QUEUE_DIR = BASE_DIR / "msg-queue"
SKILL_DEBUGGER = BASE_DIR / "skill-debugger"
SKILL_EXECUTOR = BASE_DIR / "skill-executor"
WORKSPACE = BASE_DIR.parent

for d in [ISSUE_POOL, TESTS_DIR, MEMORY_DIR, MSG_QUEUE_DIR,
          MSG_QUEUE_DIR / "debugger", MSG_QUEUE_DIR / "executor"]:
    d.mkdir(parents=True, exist_ok=True)


class AgentStatus(str, Enum):
    IDLE = "idle"
    BUSY = "busy"
    MANUAL = "manual"          # 用户手动操作中
    WAITING_RESUME = "waiting"  # 等待 resume
    STOPPED = "stopped"
    ERROR = "error"


@dataclass
class AgentState:
    name: str
    status: AgentStatus = AgentStatus.STOPPED
    session_id: Optional[str] = None
    current_task: Optional[str] = None
    last_active: Optional[str] = None
    message_count: int = 0
    error: Optional[str] = None


@dataclass
class KernelState:
    debugger: AgentState = field(default_factory=lambda: AgentState("debugger"))
    executor: AgentState = field(default_factory=lambda: AgentState("executor"))
    issue_stats: dict = field(default_factory=lambda: {
        "open": 0, "in_progress": 0, "resolved": 0, "verified": 0
    })
    started_at: Optional[str] = None
    last_tick: Optional[str] = None

    def to_dict(self):
        return {
            "debugger": asdict(self.debugger),
            "executor": asdict(self.executor),
            "issue_stats": self.issue_stats,
            "started_at": self.started_at,
            "last_tick": self.last_tick,
        }

    def save(self):
        STATE_FILE.write_text(json.dumps(self.to_dict(), indent=2, ensure_ascii=False))

    @classmethod
    def load(cls):
        if STATE_FILE.exists():
            try:
                data = json.loads(STATE_FILE.read_text())
                state = cls()
                for k in ["name", "status", "session_id", "current_task",
                           "last_active", "message_count", "error"]:
                    if k in data.get("debugger", {}):
                        setattr(state.debugger, k, data["debugger"][k])
                    if k in data.get("executor", {}):
                        setattr(state.executor, k, data["executor"][k])
                state.issue_stats = data.get("issue_stats", state.issue_stats)
                state.started_at = data.get("started_at")
                state.last_tick = data.get("last_tick")
                return state
            except Exception:
                pass
        return cls()


def scan_issues() -> dict:
    stats = {"open": 0, "in_progress": 0, "resolved": 0, "verified": 0}
    for f in ISSUE_POOL.glob("issue-*.json"):
        try:
            data = json.loads(f.read_text())
            s = data.get("status", "open")
            if s in stats:
                stats[s] += 1
        except Exception:
            pass
    return stats


def next_issue_id() -> str:
    existing = sorted(ISSUE_POOL.glob("issue-*.json"))
    if not existing:
        return "issue-001"
    last = existing[-1].stem
    num = int(last.split("-")[1]) + 1
    return f"issue-{num:03d}"


# ── Message Queue ──────────────────────────────────────────────

def enqueue_message(agent: str, content: str, priority: int = 10,
                    msg_type: str = "auto"):
    """往 agent 的消息队列中放一条消息。"""
    q_dir = MSG_QUEUE_DIR / agent
    ts = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
    msg = {
        "id": f"{ts}_{priority}",
        "type": msg_type,       # auto / manual / file_drop
        "content": content,
        "priority": priority,   # 0=最高（手动操作），10=普通自动
        "created_at": datetime.now().isoformat(),
    }
    (q_dir / f"{priority:02d}_{ts}.json").write_text(
        json.dumps(msg, indent=2, ensure_ascii=False))


def dequeue_message(agent: str) -> Optional[dict]:
    """从 agent 的消息队列中取出优先级最高（数字最小）的消息。"""
    q_dir = MSG_QUEUE_DIR / agent
    files = sorted(q_dir.glob("*.json"))
    if not files:
        return None
    f = files[0]
    msg = json.loads(f.read_text())
    f.unlink()
    return msg


def queue_size(agent: str) -> int:
    return len(list((MSG_QUEUE_DIR / agent).glob("*.json")))


# ── File Drop Handler ──────────────────────────────────────────

def handle_file_drop(agent: str, filepath: str):
    """处理用户拖拽文件到 agent 的操作。"""
    fp = Path(filepath)
    if not fp.exists():
        print(f"[kernel] 文件不存在: {filepath}")
        return

    content = fp.read_text()

    if agent == "debugger":
        prompt = (
            f"用户拖入了一个分析文件，请将其中发现的问题逐个转化为 issue-pool 中的 JSON 文件，"
            f"并为每个问题编写对应的 C 语言测试用例放入 tests/ 目录。\n\n"
            f"文件名：{fp.name}\n"
            f"文件内容：\n```\n{content[:8000]}\n```\n\n"
            f"请严格按照 skill-debugger 中定义的 issue JSON schema 和测试程序规范执行。"
            f"下一个可用的 issue ID 是 {next_issue_id()}。"
        )
        enqueue_message("debugger", prompt, priority=0, msg_type="file_drop")

    elif agent == "executor":
        if fp.suffix == ".json" and fp.parent == ISSUE_POOL:
            issue_data = json.loads(content)
            issue_id = issue_data.get("id", fp.stem)
            prompt = (
                f"用户指定你立即处理这个问题：\n\n"
                f"Issue ID: {issue_id}\n"
                f"内容：\n```json\n{content}\n```\n\n"
                f"请按照 skill-executor 的工作流程修复此问题。"
            )
            enqueue_message("executor", prompt, priority=0, msg_type="file_drop")
        else:
            prompt = (
                f"用户拖入了一个文件要求你处理：\n\n"
                f"文件名：{fp.name}\n"
                f"文件内容：\n```\n{content[:8000]}\n```\n\n"
                f"请分析文件内容并按照 skill-executor 工作流程执行。"
            )
            enqueue_message("executor", prompt, priority=0, msg_type="file_drop")

    print(f"[kernel] 已将 {fp.name} 加入 {agent} 的消息队列（优先级 0）")


# ── Auto Prompt Generator ─────────────────────────────────────

def generate_auto_prompt_debugger() -> str:
    stats = scan_issues()
    open_count = stats["open"]
    resolved_issues = []
    for f in ISSUE_POOL.glob("issue-*.json"):
        try:
            d = json.loads(f.read_text())
            if d.get("status") == "resolved":
                resolved_issues.append(d.get("id", f.stem))
        except Exception:
            pass

    parts = ["你当前处于自动巡检模式。请执行以下操作：\n"]

    if resolved_issues:
        parts.append(
            f"1. 【回归验证】以下 issue 被 executor 标记为 resolved，请重新运行测试验证：\n"
            f"   {', '.join(resolved_issues)}\n"
            f"   验证通过改为 verified，不通过改回 open 并追加 verification_note。\n"
        )

    parts.append(
        f"2. 【发现新问题】当前问题池有 {open_count} 个 open issue。\n"
        f"   请审计一个尚未检查的 syscall 模块，发现问题并写入 issue-pool。\n"
        f"   参考 memory/debugger-memory.md 中的扫描进度，选择未审计的模块。\n"
    )

    parts.append(
        "3. 【更新记忆】完成后更新 memory/debugger-memory.md。\n"
    )

    parts.append(f"下一个可用的 issue ID 是 {next_issue_id()}。")
    return "\n".join(parts)


def generate_auto_prompt_executor() -> str:
    open_issues = []
    for f in sorted(ISSUE_POOL.glob("issue-*.json")):
        try:
            d = json.loads(f.read_text())
            if d.get("status") == "open":
                open_issues.append({
                    "id": d.get("id", f.stem),
                    "title": d.get("title", ""),
                    "severity": d.get("severity", "medium"),
                })
        except Exception:
            pass

    if not open_issues:
        return (
            "当前问题池中没有 open 的 issue。请执行以下操作：\n"
            "1. 读取 memory/executor-memory.md 回顾进展\n"
            "2. 读取 memory/debugger-memory.md 看看 debugger 有没有新消息\n"
            "3. 更新你的记忆文件\n"
            "4. 等待 debugger 发现新问题"
        )

    severity_order = {"critical": 0, "high": 1, "medium": 2, "low": 3}
    open_issues.sort(key=lambda x: severity_order.get(x["severity"], 9))
    target = open_issues[0]

    issue_file = ISSUE_POOL / f"{target['id']}.json"
    issue_content = issue_file.read_text() if issue_file.exists() else "{}"

    return (
        f"当前问题池有 {len(open_issues)} 个 open issue。\n"
        f"请处理最高优先级的问题：\n\n"
        f"```json\n{issue_content}\n```\n\n"
        f"请按照 skill-executor 工作流程修复此问题：\n"
        f"1. 先将 status 改为 in-progress\n"
        f"2. 阅读 source_context 定位代码\n"
        f"3. 实施修复\n"
        f"4. 编译验证\n"
        f"5. 修复成功则标记 resolved\n"
        f"6. 更新 memory/executor-memory.md"
    )


# ── Agent Session Manager ─────────────────────────────────────

class AgentSession:
    """管理一个 cursor agent CLI 会话。"""

    def __init__(self, name: str, skill_file: Path, state: AgentState):
        self.name = name
        self.skill_file = skill_file
        self.state = state
        self.process: Optional[subprocess.Popen] = None
        self._lock = threading.Lock()

    def _build_cmd(self, message: str, resume: bool = False) -> list[str]:
        cmd = ["cursor-agent"]
        if resume and self.state.session_id:
            cmd += ["--resume", self.state.session_id]
        else:
            cmd += ["--skill", str(self.skill_file)]
        cmd += ["--message", message]
        cmd += ["--workspace", str(WORKSPACE)]
        return cmd

    def send_message(self, message: str, resume: bool = True) -> bool:
        """向 agent 发送一条消息。实际中这里会调用 cursor agent CLI。"""
        with self._lock:
            self.state.status = AgentStatus.BUSY
            self.state.current_task = message[:100]
            self.state.last_active = datetime.now().isoformat()
            self.state.message_count += 1

        # 记录消息到日志
        log_file = BASE_DIR / f"logs/{self.name}.log"
        log_file.parent.mkdir(exist_ok=True)
        with open(log_file, "a") as f:
            f.write(f"\n{'='*60}\n")
            f.write(f"[{datetime.now().isoformat()}] Message #{self.state.message_count}\n")
            f.write(f"{'='*60}\n")
            f.write(message + "\n")

        # ── 实际的 agent CLI 调用点 ──
        # 当 cursor-agent CLI 可用时，取消下面的注释：
        #
        # cmd = self._build_cmd(message, resume)
        # try:
        #     result = subprocess.run(
        #         cmd, capture_output=True, text=True, timeout=600,
        #         cwd=str(WORKSPACE)
        #     )
        #     with open(log_file, "a") as f:
        #         f.write(f"\n--- STDOUT ---\n{result.stdout}\n")
        #         f.write(f"\n--- STDERR ---\n{result.stderr}\n")
        #     if result.returncode != 0:
        #         self.state.status = AgentStatus.ERROR
        #         self.state.error = result.stderr[:200]
        #         return False
        #     # 解析 session_id（cursor agent CLI 输出中应包含）
        #     # self.state.session_id = parse_session_id(result.stdout)
        # except subprocess.TimeoutExpired:
        #     self.state.status = AgentStatus.ERROR
        #     self.state.error = "timeout"
        #     return False

        with self._lock:
            self.state.status = AgentStatus.IDLE
            self.state.current_task = None

        return True

    def is_idle(self) -> bool:
        return self.state.status == AgentStatus.IDLE

    def is_stopped(self) -> bool:
        return self.state.status == AgentStatus.STOPPED

    def stop(self):
        self.state.status = AgentStatus.STOPPED


# ── Scheduler ─────────────────────────────────────────────────

class Scheduler:
    """调度内核：管理两个 agent 的生命周期和消息分发。"""

    def __init__(self):
        self.kernel_state = KernelState.load()
        self.kernel_state.started_at = datetime.now().isoformat()

        self.debugger = AgentSession(
            "debugger", SKILL_DEBUGGER, self.kernel_state.debugger)
        self.executor = AgentSession(
            "executor", SKILL_EXECUTOR, self.kernel_state.executor)

        self._running = True
        self._tick_interval = 5  # 秒

    def tick(self):
        """一次调度循环。"""
        now = datetime.now().isoformat()
        self.kernel_state.last_tick = now
        self.kernel_state.issue_stats = scan_issues()

        for name, agent, gen_prompt in [
            ("debugger", self.debugger, generate_auto_prompt_debugger),
            ("executor", self.executor, generate_auto_prompt_executor),
        ]:
            if agent.is_stopped():
                continue

            msg = dequeue_message(name)

            if msg:
                is_manual = msg["type"] in ("manual", "file_drop")
                if is_manual:
                    agent.state.status = AgentStatus.MANUAL
                agent.send_message(msg["content"])
            elif agent.is_idle():
                auto_prompt = gen_prompt()
                agent.send_message(auto_prompt)

        self.kernel_state.save()

    def start_agent(self, name: str):
        agent = self.debugger if name == "debugger" else self.executor
        agent.state.status = AgentStatus.IDLE
        agent.state.error = None
        print(f"[kernel] {name} 已启动")

    def stop_agent(self, name: str):
        agent = self.debugger if name == "debugger" else self.executor
        agent.stop()
        print(f"[kernel] {name} 已停止")

    def run(self):
        """主循环。"""
        print("[kernel] 调度内核启动")
        self.start_agent("debugger")
        self.start_agent("executor")

        while self._running:
            try:
                self.tick()
            except Exception as e:
                print(f"[kernel] tick error: {e}")
            time.sleep(self._tick_interval)

    def shutdown(self):
        self._running = False
        self.stop_agent("debugger")
        self.stop_agent("executor")
        self.kernel_state.save()
        print("[kernel] 已关闭")


# ── CLI 入口 ──────────────────────────────────────────────────

def cli_main():
    import argparse
    parser = argparse.ArgumentParser(description="Auto-Evolve Kernel")
    sub = parser.add_subparsers(dest="command")

    sub.add_parser("start", help="启动调度内核（前台）")
    sub.add_parser("status", help="查看当前状态")

    p_drop = sub.add_parser("drop", help="拖拽文件给 agent")
    p_drop.add_argument("agent", choices=["debugger", "executor"])
    p_drop.add_argument("file", help="文件路径")

    p_send = sub.add_parser("send", help="手动发送消息给 agent")
    p_send.add_argument("agent", choices=["debugger", "executor"])
    p_send.add_argument("message", help="消息内容")

    p_ctl = sub.add_parser("agent", help="控制 agent")
    p_ctl.add_argument("action", choices=["start", "stop"])
    p_ctl.add_argument("name", choices=["debugger", "executor"])

    sub.add_parser("gui", help="启动 TUI 仪表盘")

    args = parser.parse_args()

    if args.command == "start":
        scheduler = Scheduler()
        signal.signal(signal.SIGINT, lambda *_: scheduler.shutdown())
        scheduler.run()

    elif args.command == "status":
        state = KernelState.load()
        print(json.dumps(state.to_dict(), indent=2, ensure_ascii=False))

    elif args.command == "drop":
        handle_file_drop(args.agent, args.file)

    elif args.command == "send":
        enqueue_message(args.agent, args.message, priority=0, msg_type="manual")
        print(f"[kernel] 消息已加入 {args.agent} 队列（手动优先级）")

    elif args.command == "agent":
        state = KernelState.load()
        ag = state.debugger if args.name == "debugger" else state.executor
        if args.action == "start":
            ag.status = AgentStatus.IDLE
        else:
            ag.status = AgentStatus.STOPPED
        state.save()
        print(f"[kernel] {args.name} → {ag.status}")

    elif args.command == "gui":
        from dashboard import DashboardApp
        DashboardApp().run()

    else:
        parser.print_help()


if __name__ == "__main__":
    cli_main()
