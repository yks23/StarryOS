"""
Auto-Evolve Daemon
守护进程：管理 kernel 调度器和两个 agent 的生命周期。
负责健康检查、自动重启、状态报告。

启动方式:
  python3 auto-evolve/daemon.py start     # 前台启动
  python3 auto-evolve/daemon.py start -d  # 后台启动
  python3 auto-evolve/daemon.py stop      # 停止
  python3 auto-evolve/daemon.py status    # 查看状态
"""

import json
import os
import sys
import time
import signal
import subprocess
import threading
from pathlib import Path
from datetime import datetime, timedelta

BASE_DIR = Path(__file__).parent
STATE_FILE = BASE_DIR / "kernel-state.json"
DAEMON_PID_FILE = BASE_DIR / "daemon.pid"
DAEMON_LOG = BASE_DIR / "logs" / "daemon.log"

DAEMON_LOG.parent.mkdir(parents=True, exist_ok=True)


def log(msg: str):
    ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    line = f"[{ts}] {msg}"
    print(line)
    with open(DAEMON_LOG, "a") as f:
        f.write(line + "\n")


def load_state() -> dict:
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except Exception:
            return {}
    return {}


class HealthChecker:
    """检查各组件的健康状态。"""

    def __init__(self):
        self.last_debugger_active = None
        self.last_executor_active = None
        self.stall_threshold = timedelta(minutes=10)

    def check(self) -> list[str]:
        """返回异常列表（空=健康）。"""
        issues = []
        state = load_state()

        if not state:
            issues.append("kernel-state.json 不存在或损坏")
            return issues

        last_tick = state.get("last_tick")
        if last_tick:
            try:
                tick_time = datetime.fromisoformat(last_tick)
                if datetime.now() - tick_time > timedelta(seconds=30):
                    issues.append(f"调度内核可能卡住（上次 tick: {last_tick}）")
            except Exception:
                pass

        for name in ["debugger", "executor"]:
            agent = state.get(name, {})
            status = agent.get("status", "stopped")

            if status == "error":
                error = agent.get("error", "未知")
                issues.append(f"{name} 处于错误状态: {error}")

            if status == "busy":
                last_active = agent.get("last_active")
                if last_active:
                    try:
                        active_time = datetime.fromisoformat(last_active)
                        if datetime.now() - active_time > self.stall_threshold:
                            issues.append(
                                f"{name} 可能卡住（busy 超过 {self.stall_threshold}）"
                            )
                    except Exception:
                        pass

        return issues


class Daemon:
    """守护进程主体。"""

    def __init__(self):
        self.kernel_proc: subprocess.Popen | None = None
        self.checker = HealthChecker()
        self._running = True
        self.check_interval = 10  # 秒
        self.kernel_restart_count = 0
        self.max_restarts = 5

    def start_kernel(self):
        """启动调度内核子进程。"""
        if self.kernel_proc and self.kernel_proc.poll() is None:
            log("调度内核已在运行")
            return

        log("启动调度内核...")
        kernel_log = open(BASE_DIR / "logs" / "kernel-stdout.log", "a")
        self.kernel_proc = subprocess.Popen(
            [sys.executable, str(BASE_DIR / "kernel.py"), "start"],
            stdout=kernel_log,
            stderr=subprocess.STDOUT,
            cwd=str(BASE_DIR.parent),
        )
        log(f"调度内核 PID: {self.kernel_proc.pid}")

    def stop_kernel(self):
        if self.kernel_proc and self.kernel_proc.poll() is None:
            log("停止调度内核...")
            self.kernel_proc.send_signal(signal.SIGINT)
            try:
                self.kernel_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.kernel_proc.kill()
            log("调度内核已停止")

    def health_loop(self):
        """健康检查循环。"""
        while self._running:
            if self.kernel_proc and self.kernel_proc.poll() is not None:
                exit_code = self.kernel_proc.returncode
                log(f"调度内核退出（exit code: {exit_code}）")

                if self.kernel_restart_count < self.max_restarts:
                    self.kernel_restart_count += 1
                    log(f"自动重启（第 {self.kernel_restart_count} 次）...")
                    time.sleep(2)
                    self.start_kernel()
                else:
                    log(f"已达最大重启次数 ({self.max_restarts})，停止重启")
                    self._running = False
                    break

            issues = self.checker.check()
            if issues:
                for issue in issues:
                    log(f"[健康检查] ⚠ {issue}")

            # 写入守护进程状态
            daemon_state = {
                "pid": os.getpid(),
                "kernel_pid": self.kernel_proc.pid if self.kernel_proc else None,
                "kernel_alive": (self.kernel_proc.poll() is None) if self.kernel_proc else False,
                "restart_count": self.kernel_restart_count,
                "last_check": datetime.now().isoformat(),
                "health_issues": issues,
            }
            (BASE_DIR / "daemon-state.json").write_text(
                json.dumps(daemon_state, indent=2, ensure_ascii=False))

            time.sleep(self.check_interval)

    def run(self):
        """主入口。"""
        DAEMON_PID_FILE.write_text(str(os.getpid()))
        log(f"守护进程启动 (PID: {os.getpid()})")

        signal.signal(signal.SIGINT, lambda *_: self.shutdown())
        signal.signal(signal.SIGTERM, lambda *_: self.shutdown())

        self.start_kernel()
        self.health_loop()

    def shutdown(self):
        log("守护进程关闭中...")
        self._running = False
        self.stop_kernel()
        if DAEMON_PID_FILE.exists():
            DAEMON_PID_FILE.unlink()
        log("守护进程已关闭")
        sys.exit(0)


def get_running_pid() -> int | None:
    if DAEMON_PID_FILE.exists():
        try:
            pid = int(DAEMON_PID_FILE.read_text().strip())
            os.kill(pid, 0)
            return pid
        except (ValueError, ProcessLookupError, PermissionError):
            DAEMON_PID_FILE.unlink(missing_ok=True)
    return None


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Auto-Evolve Daemon")
    parser.add_argument("action", choices=["start", "stop", "status", "restart"])
    parser.add_argument("-d", "--daemonize", action="store_true",
                        help="后台运行")
    args = parser.parse_args()

    if args.action == "status":
        pid = get_running_pid()
        if pid:
            print(f"守护进程运行中 (PID: {pid})")
            daemon_state_file = BASE_DIR / "daemon-state.json"
            if daemon_state_file.exists():
                state = json.loads(daemon_state_file.read_text())
                print(json.dumps(state, indent=2, ensure_ascii=False))
        else:
            print("守护进程未运行")
        return

    if args.action == "stop":
        pid = get_running_pid()
        if pid:
            print(f"停止守护进程 (PID: {pid})...")
            os.kill(pid, signal.SIGTERM)
            time.sleep(2)
            print("已发送停止信号")
        else:
            print("守护进程未运行")
        return

    if args.action == "restart":
        pid = get_running_pid()
        if pid:
            os.kill(pid, signal.SIGTERM)
            time.sleep(3)

    if args.action in ("start", "restart"):
        existing = get_running_pid()
        if existing:
            print(f"守护进程已在运行 (PID: {existing})，请先 stop")
            return

        if args.daemonize:
            if os.fork() > 0:
                print("守护进程已在后台启动")
                sys.exit(0)
            os.setsid()
            if os.fork() > 0:
                sys.exit(0)
            sys.stdout = open(DAEMON_LOG, "a")
            sys.stderr = sys.stdout

        daemon = Daemon()
        daemon.run()


if __name__ == "__main__":
    main()
