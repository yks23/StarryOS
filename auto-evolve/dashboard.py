"""
Auto-Evolve TUI Dashboard
基于 Textual 的终端 GUI 仪表盘。
启动方式: python3 auto-evolve/dashboard.py
"""

import json
import os
import sys
from pathlib import Path
from datetime import datetime

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical, Container
from textual.widgets import (
    Header, Footer, Static, DataTable, Log, Input, Button, Label,
    RichLog, TabbedContent, TabPane,
)
from textual.reactive import reactive
from textual.timer import Timer
from textual import events, work

BASE_DIR = Path(__file__).parent
ISSUE_POOL = BASE_DIR / "issue-pool"
STATE_FILE = BASE_DIR / "kernel-state.json"
MSG_QUEUE_DIR = BASE_DIR / "msg-queue"


def load_state() -> dict:
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except Exception:
            pass
    return {}


def scan_issues() -> list[dict]:
    results = []
    for f in sorted(ISSUE_POOL.glob("issue-*.json")):
        try:
            data = json.loads(f.read_text())
            results.append({
                "id": data.get("id", f.stem),
                "title": data.get("title", "")[:50],
                "severity": data.get("severity", "?"),
                "status": data.get("status", "?"),
                "category": data.get("category", "?"),
            })
        except Exception:
            results.append({"id": f.stem, "title": "PARSE ERROR",
                            "severity": "?", "status": "?", "category": "?"})
    return results


def read_log(agent: str, lines: int = 30) -> str:
    log_file = BASE_DIR / f"logs/{agent}.log"
    if not log_file.exists():
        return f"（{agent} 暂无日志）"
    text = log_file.read_text()
    return "\n".join(text.split("\n")[-lines:])


def read_memory(agent: str) -> str:
    mem_file = BASE_DIR / f"memory/{agent}-memory.md"
    if mem_file.exists():
        return mem_file.read_text()
    return "（暂无记忆）"


STATUS_COLORS = {
    "idle": "green",
    "busy": "yellow",
    "manual": "cyan",
    "waiting": "blue",
    "stopped": "dim",
    "error": "red",
}

SEVERITY_COLORS = {
    "critical": "bold red",
    "high": "red",
    "medium": "yellow",
    "low": "dim",
}

ISSUE_STATUS_COLORS = {
    "open": "bold red",
    "in_progress": "yellow",
    "resolved": "green",
    "verified": "bold green",
}


class AgentPanel(Static):
    """单个 agent 的状态面板。"""

    def __init__(self, agent_name: str, **kwargs):
        super().__init__(**kwargs)
        self.agent_name = agent_name

    def compose(self) -> ComposeResult:
        yield Static(id=f"agent-{self.agent_name}-info")

    def update_info(self, data: dict):
        status = data.get("status", "stopped")
        color = STATUS_COLORS.get(status, "white")
        session = data.get("session_id", "无") or "无"
        task = data.get("current_task", "") or "无"
        if len(task) > 60:
            task = task[:60] + "..."
        msgs = data.get("message_count", 0)
        last = data.get("last_active", "从未") or "从未"
        if last != "从未":
            last = last[11:19]
        error = data.get("error", "")
        q_size = len(list((MSG_QUEUE_DIR / self.agent_name).glob("*.json")))

        lines = [
            f"[bold]{self.agent_name.upper()}[/bold]",
            f"  状态: [{color}]● {status}[/{color}]",
            f"  会话: {session}",
            f"  队列: {q_size} 条待处理",
            f"  消息: 已处理 {msgs} 条",
            f"  最后活跃: {last}",
        ]
        if task != "无":
            lines.append(f"  当前: {task}")
        if error:
            lines.append(f"  [red]错误: {error}[/red]")

        widget = self.query_one(f"#agent-{self.agent_name}-info", Static)
        widget.update("\n".join(lines))


class DashboardApp(App):
    """Auto-Evolve TUI 仪表盘。"""

    CSS = """
    Screen {
        layout: vertical;
    }
    #top-row {
        height: 12;
        layout: horizontal;
    }
    #agent-debugger-panel, #agent-executor-panel {
        width: 1fr;
        border: solid $primary;
        padding: 1;
    }
    #stats-panel {
        width: 20;
        border: solid $secondary;
        padding: 1;
    }
    #main-area {
        height: 1fr;
    }
    #issue-table {
        height: 1fr;
    }
    #bottom-bar {
        height: 3;
        layout: horizontal;
        padding: 0 1;
    }
    #cmd-input {
        width: 1fr;
    }
    #send-btn {
        width: 12;
    }
    .log-pane {
        height: 1fr;
    }
    """

    BINDINGS = [
        ("q", "quit", "退出"),
        ("d", "drop_file", "拖文件"),
        ("r", "refresh", "刷新"),
        ("1", "focus_debugger", "→ Debugger"),
        ("2", "focus_executor", "→ Executor"),
    ]

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)

        with Horizontal(id="top-row"):
            with Container(id="agent-debugger-panel"):
                yield AgentPanel("debugger")
            with Container(id="stats-panel"):
                yield Static(id="issue-stats")
            with Container(id="agent-executor-panel"):
                yield AgentPanel("executor")

        with TabbedContent(id="main-area"):
            with TabPane("问题池", id="tab-issues"):
                yield DataTable(id="issue-table")
            with TabPane("Debugger 日志", id="tab-dbg-log"):
                yield RichLog(id="debugger-log", classes="log-pane",
                              highlight=True, markup=True)
            with TabPane("Executor 日志", id="tab-exe-log"):
                yield RichLog(id="executor-log", classes="log-pane",
                              highlight=True, markup=True)
            with TabPane("Debugger 记忆", id="tab-dbg-mem"):
                yield RichLog(id="debugger-mem", classes="log-pane",
                              highlight=True, markup=True)
            with TabPane("Executor 记忆", id="tab-exe-mem"):
                yield RichLog(id="executor-mem", classes="log-pane",
                              highlight=True, markup=True)

        with Horizontal(id="bottom-bar"):
            yield Input(placeholder="命令: drop <agent> <file> | send <agent> <msg> | start/stop <agent>",
                        id="cmd-input")
            yield Button("执行", id="send-btn", variant="primary")

        yield Footer()

    def on_mount(self) -> None:
        table = self.query_one("#issue-table", DataTable)
        table.add_columns("ID", "标题", "严重性", "状态", "类别")
        self.refresh_all()
        self.set_interval(3, self.refresh_all)

    def refresh_all(self) -> None:
        state = load_state()

        dbg_panel = self.query_one("AgentPanel#", AgentPanel) if False else None
        for panel in self.query(AgentPanel):
            if panel.agent_name == "debugger":
                panel.update_info(state.get("debugger", {}))
            elif panel.agent_name == "executor":
                panel.update_info(state.get("executor", {}))

        stats = state.get("issue_stats", {})
        stats_text = (
            "[bold]问题统计[/bold]\n"
            f"  [red]Open: {stats.get('open', 0)}[/red]\n"
            f"  [yellow]进行中: {stats.get('in_progress', 0)}[/yellow]\n"
            f"  [green]已修复: {stats.get('resolved', 0)}[/green]\n"
            f"  [bold green]已验证: {stats.get('verified', 0)}[/bold green]"
        )
        self.query_one("#issue-stats", Static).update(stats_text)

        table = self.query_one("#issue-table", DataTable)
        table.clear()
        for issue in scan_issues():
            sev = issue["severity"]
            st = issue["status"]
            table.add_row(
                issue["id"],
                issue["title"],
                sev,
                st,
                issue["category"],
            )

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "send-btn":
            self.execute_command()

    def on_input_submitted(self, event: Input.Submitted) -> None:
        self.execute_command()

    def execute_command(self) -> None:
        inp = self.query_one("#cmd-input", Input)
        cmd = inp.value.strip()
        if not cmd:
            return
        inp.value = ""

        parts = cmd.split(maxsplit=2)
        verb = parts[0].lower()

        if verb == "drop" and len(parts) >= 3:
            agent, filepath = parts[1], parts[2]
            if agent in ("debugger", "executor"):
                from kernel import handle_file_drop
                handle_file_drop(agent, filepath)
                self.notify(f"已拖入 {filepath} → {agent}")
            else:
                self.notify("agent 必须是 debugger 或 executor", severity="error")

        elif verb == "send" and len(parts) >= 3:
            agent, msg = parts[1], parts[2]
            if agent in ("debugger", "executor"):
                from kernel import enqueue_message
                enqueue_message(agent, msg, priority=0, msg_type="manual")
                self.notify(f"已发送消息 → {agent}")
            else:
                self.notify("agent 必须是 debugger 或 executor", severity="error")

        elif verb in ("start", "stop") and len(parts) >= 2:
            agent = parts[1]
            if agent in ("debugger", "executor"):
                from kernel import KernelState, AgentStatus
                state = KernelState.load()
                ag = state.debugger if agent == "debugger" else state.executor
                ag.status = AgentStatus.IDLE if verb == "start" else AgentStatus.STOPPED
                state.save()
                self.notify(f"{agent} → {ag.status}")
            else:
                self.notify("agent 必须是 debugger 或 executor", severity="error")

        elif verb == "refresh":
            self.refresh_all()

        else:
            self.notify(f"未知命令: {cmd}", severity="warning")

        self.refresh_all()

    def action_refresh(self) -> None:
        self.refresh_all()
        self.notify("已刷新")

    def action_drop_file(self) -> None:
        self.query_one("#cmd-input", Input).focus()
        self.query_one("#cmd-input", Input).value = "drop "

    def action_focus_debugger(self) -> None:
        self.query_one("#cmd-input", Input).value = "send debugger "
        self.query_one("#cmd-input", Input).focus()

    def action_focus_executor(self) -> None:
        self.query_one("#cmd-input", Input).value = "send executor "
        self.query_one("#cmd-input", Input).focus()


if __name__ == "__main__":
    DashboardApp().run()
