"""
Auto-Evolve Kernel: 调度内核
负责管理 debugger/executor agent 会话、消息队列、任务调度。
所有进程间通信走文件系统（JSON），GUI/daemon 通过读取状态文件获取信息。
"""

import json
import os
import re
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
ISSUE_ARCHIVE = BASE_DIR / "issue-archive"  # resolved/verified 归档
TESTS_DIR = BASE_DIR / "tests"
MEMORY_DIR = BASE_DIR / "memory"
STATE_FILE = BASE_DIR / "kernel-state.json"
MSG_QUEUE_DIR = BASE_DIR / "msg-queue"
SKILL_DEBUGGER = BASE_DIR / "skill-debugger"
SKILL_EXECUTOR = BASE_DIR / "skill-executor"
WORKSPACE = BASE_DIR.parent

AGENT_BIN = os.path.expanduser("~/.local/bin/agent")
AGENT_API_KEY = os.environ.get("CURSOR_API_KEY", "")
if not AGENT_API_KEY:
    for line in os.popen("env").readlines():
        if line.startswith("cursor-api-key="):
            AGENT_API_KEY = line.strip().split("=", 1)[1]
            break

for d in [ISSUE_POOL, ISSUE_ARCHIVE, TESTS_DIR, MEMORY_DIR, MSG_QUEUE_DIR,
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
    open_count = 0
    open_bug_count = 0
    resolved_issues = []
    for f in ISSUE_POOL.glob("issue-*.json"):
        try:
            d = json.loads(f.read_text())
            if d.get("status") == "resolved":
                resolved_issues.append(d.get("id", f.stem))
            elif d.get("status") == "open":
                open_count += 1
                if d.get("category") != "improvement":
                    open_bug_count += 1
        except Exception:
            pass

    # 也扫归档中的 resolved（可能归档延迟）
    for f in ISSUE_ARCHIVE.glob("issue-*.json"):
        try:
            d = json.loads(f.read_text())
            if d.get("status") == "resolved":
                resolved_issues.append(d.get("id", f.stem))
        except Exception:
            pass

    # 统计已完成总数（归档的）
    archive_total = sum(1 for _ in ISSUE_ARCHIVE.glob("issue-*.json"))

    parts = ["你当前处于自动巡检模式。\n"]

    step = 1
    if resolved_issues:
        parts.append(
            f"{step}. 【回归验证】以下 issue 被 executor 标记为 resolved，请重新运行测试验证：\n"
            f"   {', '.join(resolved_issues[:10])}\n"
            f"   验证通过改为 verified，不通过改回 open 并追加 verification_note。\n"
        )
        step += 1

    parts.append(
        f"{step}. 【发现高价值问题】（重要！请专注于大问题，不要再提交参数校验顺序之类的小修补）\n"
        f"\n"
        f"   当前状态：已修复 {archive_total} 个 issue，pool 剩余 {open_count} 个 open。\n"
        f"   参数校验、errno 精细化等小问题已经足够多了。现在请聚焦以下 **高价值方向**：\n"
        f"\n"
        f"   A. 【缺失的重要 syscall】以下常用 syscall 在 Starry 中完全缺失（返回 ENOSYS），实现任意一个都比修参数校验有价值：\n"
        f"      - waitid（更灵活的进程等待，systemd/init 使用）\n"
        f"      - execveat（从 fd 执行程序，fexecve 依赖）\n"
        f"      - ppoll 的 sigmask 正确处理\n"
        f"      - semget/semop/semctl（System V 信号量，PostgreSQL 使用）\n"
        f"      - mq_open/mq_send/mq_receive（POSIX 消息队列）\n"
        f"      - sched_get_priority_max/min\n"
        f"\n"
        f"   B. 【功能性增强】让更多真实程序能运行：\n"
        f"      - /proc/self/exe 符号链接（ldd、busybox applet 发现依赖它）\n"
        f"      - /proc/[pid]/maps 完善（调试工具、地址空间可视化）\n"
        f"      - /proc/meminfo 完善（free 命令）\n"
        f"      - /proc/cpuinfo（lscpu 命令）\n"
        f"      - Unix domain socket 的 SCM_CREDENTIALS/SO_PEERCRED 完善\n"
        f"      - pty/tty 的 TCSAFLUSH 等 termios 操作完善\n"
        f"\n"
        f"   C. 【架构级改进】高难度但影响深远：\n"
        f"      - 实现 SIGSTOP/SIGCONT 的多线程全进程暂停（当前只单线程）\n"
        f"      - CoW fork 的大页支持\n"
        f"      - mmap MAP_SHARED 的 msync 写回完整性\n"
        f"\n"
        f"   severity 标注规则：缺失 syscall = medium/high，/proc 完善 = medium，架构改进 = high。\n"
        f"   不要再提交 severity=low 的参数校验类 issue。\n"
    )
    step += 1

    if open_bug_count >= 5:
        parts.append(
            f"{step}. 【主动改进提案】可以额外提出 1-2 个 improvement 类 issue。\n"
            f"   方向：让更多 Alpine 包能直接运行、让开发者体验更好。\n"
        )
        step += 1

    parts.append(
        f"{step}. 【更新记忆】完成后更新 memory/debugger-memory.md。\n"
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
        self.log_file = BASE_DIR / f"logs/{self.name}.log"
        self.log_file.parent.mkdir(exist_ok=True)

    def _build_cmd(self, message: str) -> list[str]:
        cmd = [AGENT_BIN]

        if self.state.session_id:
            cmd += ["--resume", self.state.session_id]
            cmd.append(message)
        else:
            skill_content = self.skill_file.read_text()
            cmd.append(f"{skill_content}\n\n---\n\n{message}")

        cmd += [
            "--print",
            "--trust",
            "--yolo",
            "--output-format", "json",
            "--workspace", str(WORKSPACE),
        ]
        if AGENT_API_KEY:
            cmd += ["--api-key", AGENT_API_KEY]

        return cmd

    def _parse_session_id(self, output: str) -> Optional[str]:
        """从 agent JSON 输出中解析 session ID。"""
        for pattern in [
            r'"session_id"\s*:\s*"([^"]+)"',
            r'"chatId"\s*:\s*"([^"]+)"',
        ]:
            m = re.search(pattern, output)
            if m:
                return m.group(1)
        return None

    def _log(self, text: str):
        with open(self.log_file, "a") as f:
            f.write(text)

    def send_message(self, message: str) -> bool:
        """向 agent 发送一条消息并等待完成。"""
        with self._lock:
            self.state.status = AgentStatus.BUSY
            self.state.current_task = message[:120]
            self.state.last_active = datetime.now().isoformat()
            self.state.message_count += 1

        self._log(
            f"\n{'='*60}\n"
            f"[{datetime.now().isoformat()}] Message #{self.state.message_count}\n"
            f"{'='*60}\n"
            f"{message}\n"
        )

        cmd = self._build_cmd(message)

        try:
            print(f"[kernel] {self.name}: 发送消息 (#{self.state.message_count})...")
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=600,
                cwd=str(WORKSPACE),
                env={**os.environ, "CURSOR_API_KEY": AGENT_API_KEY},
            )

            self._log(f"\n--- STDOUT ({len(result.stdout)} chars) ---\n")
            self._log(result.stdout[-2000:] if len(result.stdout) > 2000 else result.stdout)
            if result.stderr:
                self._log(f"\n--- STDERR ---\n{result.stderr[-500:]}\n")

            session_id = self._parse_session_id(result.stdout)
            if session_id:
                self.state.session_id = session_id
                print(f"[kernel] {self.name}: session_id = {session_id}")

            if result.returncode != 0:
                with self._lock:
                    self.state.status = AgentStatus.ERROR
                    self.state.error = (result.stderr or result.stdout)[:200]
                print(f"[kernel] {self.name}: 执行失败 (exit={result.returncode})")
                return False

            print(f"[kernel] {self.name}: 执行完成")

        except subprocess.TimeoutExpired:
            with self._lock:
                self.state.status = AgentStatus.ERROR
                self.state.error = "执行超时(600s)"
            self._log("\n--- TIMEOUT ---\n")
            print(f"[kernel] {self.name}: 超时")
            return False
        except FileNotFoundError:
            with self._lock:
                self.state.status = AgentStatus.ERROR
                self.state.error = f"找不到 agent CLI: {AGENT_BIN}"
            print(f"[kernel] {self.name}: 找不到 agent CLI")
            return False

        with self._lock:
            self.state.status = AgentStatus.IDLE
            self.state.current_task = None
            self.state.error = None

        return True

    def is_idle(self) -> bool:
        return self.state.status == AgentStatus.IDLE

    def is_stopped(self) -> bool:
        return self.state.status == AgentStatus.STOPPED

    def stop(self):
        self.state.status = AgentStatus.STOPPED


# ── Issue lifecycle management ─────────────────────────────────

SEVERITY_ORDER = {"critical": 0, "high": 1, "medium": 2, "low": 3}

IN_PROGRESS_TIMEOUT = 20 * 60  # in-progress 超过 20 分钟自动回退到 open


def archive_resolved_issues():
    """将 resolved/verified 的 issue 从 issue-pool 移到 issue-archive。
    移动后 issue-pool 中不再有该文件，executor 不可能再次取到它。"""
    for f in list(ISSUE_POOL.glob("issue-*.json")):
        try:
            d = json.loads(f.read_text())
            status = d.get("status", "")
            if status in ("resolved", "verified"):
                dest = ISSUE_ARCHIVE / f.name
                shutil.move(str(f), str(dest))
                print(f"[kernel] 归档 {f.name} → issue-archive/ ({status})")
        except Exception:
            pass


def unstick_in_progress():
    """检测卡在 in-progress 超时的 issue，回退为 open。
    防止 agent 失败后 issue 永远卡住。"""
    now = time.time()
    for f in list(ISSUE_POOL.glob("issue-*.json")):
        try:
            d = json.loads(f.read_text())
            if d.get("status") != "in_progress":
                continue
            # 用文件修改时间判断是否超时
            mtime = f.stat().st_mtime
            if now - mtime > IN_PROGRESS_TIMEOUT:
                d["status"] = "open"
                d.setdefault("_retries", 0)
                d["_retries"] += 1
                d["_unstick_note"] = f"自动回退: in-progress 超过 {IN_PROGRESS_TIMEOUT//60} 分钟未完成"
                f.write_text(json.dumps(d, indent=2, ensure_ascii=False))
                print(f"[kernel] {f.name} 卡住，回退为 open（第 {d['_retries']} 次）")
        except Exception:
            pass


def pick_next_issue() -> Optional[dict]:
    """按 severity 排序取出下一个 open issue。
    只扫描 issue-pool（不扫 issue-archive），所以 resolved 的不会被再次处理。"""
    candidates = []
    for f in sorted(ISSUE_POOL.glob("issue-*.json")):
        try:
            d = json.loads(f.read_text())
            if d.get("status") == "open":
                candidates.append((f, d))
        except Exception:
            pass
    if not candidates:
        return None
    candidates.sort(key=lambda x: SEVERITY_ORDER.get(x[1].get("severity", "low"), 9))
    return candidates[0][1]


def count_open_issues() -> int:
    count = 0
    for f in ISSUE_POOL.glob("issue-*.json"):
        try:
            d = json.loads(f.read_text())
            if d.get("status") == "open":
                count += 1
        except Exception:
            pass
    return count


# ── Scheduler ─────────────────────────────────────────────────

class Scheduler:
    """调度内核：executor 按优先级连续修复，debugger 按需唤醒/自动转 executor。

    调度策略：
    - executor: 手动消息优先 → 否则自动取最高优先级 open issue
    - debugger:
      - 手动消息优先
      - open issue < DEBUGGER_MIN_ISSUES 时唤醒发现新问题
      - open issue > DEBUGGER_AS_EXECUTOR_THRESHOLD 时转为 executor 帮忙消化
      - 每 30 分钟定期唤醒
    """

    DEBUGGER_MIN_ISSUES = 10        # 低于此数唤醒 debugger 找新问题
    DEBUGGER_AS_EXECUTOR_THRESHOLD = 30  # 高于此数 debugger 转为 executor
    DEBUGGER_WAKE_INTERVAL = 30 * 60     # 30 分钟定期唤醒

    def __init__(self):
        self.kernel_state = KernelState.load()
        self.kernel_state.started_at = datetime.now().isoformat()

        self.debugger = AgentSession(
            "debugger", SKILL_DEBUGGER, self.kernel_state.debugger)
        self.executor = AgentSession(
            "executor", SKILL_EXECUTOR, self.kernel_state.executor)

        self._running = True
        self._tick_interval = 5
        self._last_debugger_wake = 0.0  # epoch
        self._executor_thread: Optional[threading.Thread] = None
        self._debugger_thread: Optional[threading.Thread] = None

    def _run_executor_once(self):
        """executor 单次执行：手动消息 > 自动取最高优先级 issue。"""
        msg = dequeue_message("executor")
        if msg:
            is_manual = msg["type"] in ("manual", "file_drop")
            prio_label = "手动" if is_manual else "自动"
            print(f"[kernel] executor: 处理{prio_label}消息")
            self.executor.send_message(msg["content"])
            return

        issue = pick_next_issue()
        if not issue:
            print("[kernel] executor: 问题池无 open issue，等待...")
            self.executor.state.status = AgentStatus.IDLE
            return

        issue_id = issue.get("id", "?")
        severity = issue.get("severity", "?")
        title = issue.get("title", "?")[:60]
        print(f"[kernel] executor: 取 {issue_id} [{severity}] {title}")

        issue_content = json.dumps(issue, indent=2, ensure_ascii=False)
        prompt = (
            f"请处理以下问题（优先级: {severity}）：\n\n"
            f"```json\n{issue_content}\n```\n\n"
            f"按照 skill-executor 工作流程执行：\n"
            f"1. 将 status 改为 in-progress\n"
            f"2. 阅读 source_context 定位代码\n"
            f"3. 实施修复（修改 kernel/src/ 下的源码）\n"
            f"4. 运行 cargo clippy --target riscv64gc-unknown-none-elf -F qemu\n"
            f"5. 编译通过后将 status 改为 resolved，填写 fix_summary 和 files_changed\n"
            f"6. 更新 auto-evolve/memory/executor-memory.md\n"
            f"7. git add && git commit 你的改动"
        )
        self.executor.send_message(prompt)

    def _debugger_mode(self) -> str:
        """决定 debugger 当前应该做什么。返回 'sleep'/'debug'/'execute'/'manual'。"""
        if queue_size("debugger") > 0:
            return "manual"

        open_count = count_open_issues()

        # 积压太多 → 转为 executor 帮忙消化
        if open_count > self.DEBUGGER_AS_EXECUTOR_THRESHOLD:
            return "execute"

        # 问题太少 → 找新问题
        if open_count < self.DEBUGGER_MIN_ISSUES:
            return "debug"

        # 定期唤醒
        now = time.time()
        if now - self._last_debugger_wake >= self.DEBUGGER_WAKE_INTERVAL:
            # 积压中等 → 优先帮忙消化
            if open_count >= self.DEBUGGER_MIN_ISSUES:
                return "execute"
            return "debug"

        return "sleep"

    def _run_debugger_as_executor(self):
        """debugger 临时转为 executor，帮忙消化积压 issue。"""
        issue = pick_next_issue()
        if not issue:
            return

        issue_id = issue.get("id", "?")
        severity = issue.get("severity", "?")
        title = issue.get("title", "?")[:60]
        open_count = count_open_issues()
        print(f"[kernel] debugger→executor: 积压 {open_count} 个，帮忙处理 {issue_id} [{severity}]")

        issue_content = json.dumps(issue, indent=2, ensure_ascii=False)
        prompt = (
            f"当前问题池积压较多（{open_count} 个 open），你暂时切换为 executor 角色帮忙消化。\n\n"
            f"请处理以下问题（优先级: {severity}）：\n\n"
            f"```json\n{issue_content}\n```\n\n"
            f"请按照 auto-evolve/skill-executor 中定义的工作流程执行：\n"
            f"1. 将 status 改为 in-progress\n"
            f"2. 阅读 source_context 定位代码\n"
            f"3. 实施修复（修改 kernel/src/ 下的源码）\n"
            f"4. 运行 cargo clippy --target riscv64gc-unknown-none-elf -F qemu\n"
            f"5. 编译通过后将 status 改为 resolved\n"
            f"6. git add && git commit 你的改动"
        )
        self.debugger.send_message(prompt)
        self._last_debugger_wake = time.time()

    def _run_debugger_once(self):
        """debugger 单次执行：手动消息 > 转 executor > 自动巡检。"""
        mode = self._debugger_mode()

        if mode == "sleep":
            return False  # 不执行，返回 False 让循环 sleep

        if mode == "manual":
            msg = dequeue_message("debugger")
            if msg:
                prio_label = "手动" if msg["type"] in ("manual", "file_drop") else "自动"
                print(f"[kernel] debugger: 处理{prio_label}消息")
                self.debugger.send_message(msg["content"])
                self._last_debugger_wake = time.time()
            return True

        if mode == "execute":
            self._run_debugger_as_executor()
            return True

        if mode == "debug":
            open_count = count_open_issues()
            print(f"[kernel] debugger: open={open_count}，自动巡检...")
            prompt = generate_auto_prompt_debugger()
            self.debugger.send_message(prompt)
            self._last_debugger_wake = time.time()
            return True

        return False

    def _executor_loop(self):
        """executor 工作线程：连续取 issue 并修复。"""
        print("[kernel] executor 工作线程启动")
        while self._running:
            if self.executor.is_stopped():
                time.sleep(5)
                continue
            try:
                self._run_executor_once()
            except Exception as e:
                print(f"[kernel] executor error: {e}")
                self.executor.state.error = str(e)[:200]
                time.sleep(10)
            self.kernel_state.issue_stats = scan_issues()
            self.kernel_state.save()
            time.sleep(3)

    def _debugger_loop(self):
        """debugger 工作线程：按需唤醒，积压时自动转 executor。"""
        print("[kernel] debugger 工作线程启动（自适应模式）")
        while self._running:
            if self.debugger.is_stopped():
                time.sleep(5)
                continue
            try:
                did_work = self._run_debugger_once()
            except Exception as e:
                print(f"[kernel] debugger error: {e}")
                self.debugger.state.error = str(e)[:200]
                did_work = False
                time.sleep(10)
            if did_work:
                self.kernel_state.issue_stats = scan_issues()
                self.kernel_state.save()
                time.sleep(3)
            else:
                time.sleep(10)

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
        """主循环：启动 executor 和 debugger 线程。"""
        print("[kernel] ═══════════════════════════════════════")
        print("[kernel] Auto-Evolve 调度内核启动")
        print(f"[kernel] Issue pool: {count_open_issues()} open issues")
        print(f"[kernel] Debugger 唤醒阈值: open < {self.DEBUGGER_MIN_ISSUES}")
        print(f"[kernel] Debugger 转 executor: open > {self.DEBUGGER_AS_EXECUTOR_THRESHOLD}")
        print(f"[kernel] Debugger 定期唤醒: 每 {self.DEBUGGER_WAKE_INTERVAL//60} 分钟")
        print("[kernel] ═══════════════════════════════════════")

        self.start_agent("executor")
        self.start_agent("debugger")

        self._executor_thread = threading.Thread(
            target=self._executor_loop, name="executor-loop", daemon=True)
        self._debugger_thread = threading.Thread(
            target=self._debugger_loop, name="debugger-loop", daemon=True)

        self._executor_thread.start()
        self._debugger_thread.start()

        while self._running:
            # 维护 issue 生命周期
            archive_resolved_issues()
            unstick_in_progress()

            self.kernel_state.last_tick = datetime.now().isoformat()
            self.kernel_state.issue_stats = scan_issues()
            self.kernel_state.save()

            stats = self.kernel_state.issue_stats
            dbg_mode = self._debugger_mode()
            pool_total = sum(1 for _ in ISSUE_POOL.glob("issue-*.json"))
            archive_total = sum(1 for _ in ISSUE_ARCHIVE.glob("issue-*.json"))
            print(
                f"[kernel] tick | "
                f"exe={self.executor.state.status.value} "
                f"dbg={self.debugger.state.status.value}({dbg_mode}) | "
                f"pool={pool_total} archive={archive_total} | "
                f"open={stats['open']} resolved={stats['resolved']} verified={stats['verified']}"
            )
            time.sleep(self._tick_interval)

    def shutdown(self):
        print("[kernel] 关闭中...")
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
