#!/usr/bin/env python3
"""
Auto-Evolve Monitor
每小时自动拍快照 + 生成进度总结 JSON。
可独立运行，也可被 daemon 集成。

用法:
  python3 auto-evolve/monitor.py run          # 启动定时循环（每小时）
  python3 auto-evolve/monitor.py snap         # 立即拍一次快照
  python3 auto-evolve/monitor.py report       # 打印最新报告（不拍快照）
  python3 auto-evolve/monitor.py history      # 查看所有快照的进度曲线
"""

import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

BASE_DIR = Path(__file__).parent
WORKSPACE = BASE_DIR.parent
ISSUE_POOL = BASE_DIR / "issue-pool"
TESTS_DIR = BASE_DIR / "tests"
MEMORY_DIR = BASE_DIR / "memory"
SNAPSHOTS_DIR = BASE_DIR / "snapshots"
STATE_FILE = BASE_DIR / "kernel-state.json"
REPORT_FILE = BASE_DIR / "progress-report.json"

SNAPSHOTS_DIR.mkdir(parents=True, exist_ok=True)

SNAP_INTERVAL = 3600  # 1 小时


def scan_all_issues() -> list[dict]:
    issues = []
    for f in sorted(ISSUE_POOL.glob("issue-*.json")):
        try:
            d = json.loads(f.read_text())
            issues.append({
                "id": d.get("id", f.stem),
                "title": d.get("title", ""),
                "severity": d.get("severity", "unknown"),
                "status": d.get("status", "unknown"),
                "category": d.get("category", "unknown"),
                "affected_syscalls": d.get("affected_syscalls", []),
                "fix_summary": d.get("fix_summary", None),
                "files_changed": d.get("files_changed", None),
                "resolved_at": d.get("resolved_at", None),
            })
        except Exception as e:
            issues.append({"id": f.stem, "error": str(e)})
    return issues


def count_by(issues: list[dict], key: str) -> dict:
    counts = {}
    for i in issues:
        v = i.get(key, "unknown")
        counts[v] = counts.get(v, 0) + 1
    return counts


def git_log_since(base_commit: str = "b7a897d", limit: int = 50) -> list[dict]:
    """获取自 base_commit 以来的 git 提交。"""
    try:
        result = subprocess.run(
            ["git", "log", f"{base_commit}..HEAD", f"--max-count={limit}",
             "--pretty=format:%H|%h|%ai|%s"],
            capture_output=True, text=True, cwd=str(WORKSPACE),
        )
        commits = []
        for line in result.stdout.strip().split("\n"):
            if not line:
                continue
            parts = line.split("|", 3)
            if len(parts) == 4:
                commits.append({
                    "hash": parts[0],
                    "short": parts[1],
                    "date": parts[2],
                    "message": parts[3],
                })
        return commits
    except Exception:
        return []


def load_kernel_state() -> dict:
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except Exception:
            pass
    return {}


def build_report() -> dict:
    """构建完整的进度报告。"""
    now = datetime.now()
    issues = scan_all_issues()
    kernel = load_kernel_state()
    commits = git_log_since()

    status_counts = count_by(issues, "status")
    severity_counts = count_by(issues, "severity")
    category_counts = count_by(issues, "category")

    resolved = [i for i in issues if i["status"] == "resolved"]
    in_progress = [i for i in issues if i["status"] == "in_progress"]
    open_issues = [i for i in issues if i["status"] == "open"]

    resolved_by_severity = count_by(resolved, "severity")
    open_by_severity = count_by(open_issues, "severity")

    test_files = sorted([f.name for f in TESTS_DIR.glob("test_*.c")])

    all_affected = set()
    for i in issues:
        for sc in i.get("affected_syscalls", []):
            all_affected.add(sc)
    resolved_syscalls = set()
    for i in resolved:
        for sc in i.get("affected_syscalls", []):
            resolved_syscalls.add(sc)

    report = {
        "timestamp": now.isoformat(),
        "snapshot_id": now.strftime("snap-%Y%m%d-%H%M%S"),

        "summary": {
            "total_issues": len(issues),
            "open": status_counts.get("open", 0),
            "in_progress": status_counts.get("in_progress", 0),
            "resolved": status_counts.get("resolved", 0),
            "verified": status_counts.get("verified", 0),
            "completion_pct": round(
                (status_counts.get("resolved", 0) + status_counts.get("verified", 0))
                / max(len(issues), 1) * 100, 1
            ),
            "total_commits": len(commits),
            "total_test_files": len(test_files),
            "total_affected_syscalls": len(all_affected),
            "resolved_syscalls": len(resolved_syscalls),
        },

        "by_status": status_counts,
        "by_severity": severity_counts,
        "by_category": category_counts,

        "resolved_by_severity": resolved_by_severity,
        "open_by_severity": open_by_severity,

        "agents": {
            "executor": {
                "status": kernel.get("executor", {}).get("status", "unknown"),
                "message_count": kernel.get("executor", {}).get("message_count", 0),
                "last_active": kernel.get("executor", {}).get("last_active"),
                "current_task": kernel.get("executor", {}).get("current_task", "")[:80] or None,
                "error": kernel.get("executor", {}).get("error"),
            },
            "debugger": {
                "status": kernel.get("debugger", {}).get("status", "unknown"),
                "message_count": kernel.get("debugger", {}).get("message_count", 0),
                "last_active": kernel.get("debugger", {}).get("last_active"),
                "error": kernel.get("debugger", {}).get("error"),
            },
        },

        "resolved_issues": [
            {
                "id": i["id"],
                "title": i["title"][:80],
                "severity": i["severity"],
                "fix_summary": (i.get("fix_summary") or "")[:120],
                "files_changed": i.get("files_changed"),
                "resolved_at": i.get("resolved_at"),
            }
            for i in resolved
        ],

        "in_progress_issues": [
            {"id": i["id"], "title": i["title"][:80], "severity": i["severity"]}
            for i in in_progress
        ],

        "open_issues_preview": [
            {"id": i["id"], "title": i["title"][:60], "severity": i["severity"]}
            for i in open_issues[:10]
        ],

        "recent_commits": commits[:15],
    }

    return report


def take_snapshot():
    """拍快照：复制 issue-pool + tests + memory，生成报告 JSON。"""
    report = build_report()
    snap_id = report["snapshot_id"]
    snap_dir = SNAPSHOTS_DIR / snap_id

    snap_dir.mkdir(parents=True, exist_ok=True)

    shutil.copytree(ISSUE_POOL, snap_dir / "issue-pool", dirs_exist_ok=True)
    shutil.copytree(TESTS_DIR, snap_dir / "tests", dirs_exist_ok=True)
    shutil.copytree(MEMORY_DIR, snap_dir / "memory", dirs_exist_ok=True)

    report_path = snap_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False))

    REPORT_FILE.write_text(json.dumps(report, indent=2, ensure_ascii=False))

    print(f"[monitor] 快照 {snap_id} 已保存")
    print(f"  Issues: {report['summary']['total_issues']} total, "
          f"{report['summary']['resolved']} resolved, "
          f"{report['summary']['open']} open")
    print(f"  Completion: {report['summary']['completion_pct']}%")
    print(f"  Commits: {report['summary']['total_commits']}")

    return report


def print_report():
    """打印当前报告。"""
    report = build_report()
    s = report["summary"]

    print("=" * 60)
    print(f"  Auto-Evolve 进度报告  {report['timestamp'][:19]}")
    print("=" * 60)
    print()
    print(f"  总 Issues:     {s['total_issues']}")
    print(f"  ├─ Open:       {s['open']}")
    print(f"  ├─ In Progress:{s['in_progress']}")
    print(f"  ├─ Resolved:   {s['resolved']}")
    print(f"  └─ Verified:   {s['verified']}")
    print(f"  完成率:        {s['completion_pct']}%")
    print(f"  Git Commits:   {s['total_commits']}")
    print(f"  测试文件:      {s['total_test_files']}")
    print(f"  涉及 Syscall:  {s['resolved_syscalls']}/{s['total_affected_syscalls']} 已修复")
    print()

    print("─── 按严重性统计 ───")
    for sev in ["critical", "high", "medium", "low"]:
        resolved = report["resolved_by_severity"].get(sev, 0)
        still_open = report["open_by_severity"].get(sev, 0)
        total = resolved + still_open + (1 if sev in [i["severity"] for i in report["in_progress_issues"]] else 0)
        if total > 0:
            print(f"  {sev:10s}: {resolved} resolved / {still_open} open")
    print()

    if report["resolved_issues"]:
        print("─── 已修复 ───")
        for i in report["resolved_issues"]:
            fix = i["fix_summary"] or ""
            print(f"  ✓ {i['id']} [{i['severity']}] {i['title'][:50]}")
            if fix:
                print(f"    → {fix[:80]}")
        print()

    if report["in_progress_issues"]:
        print("─── 进行中 ───")
        for i in report["in_progress_issues"]:
            print(f"  ⟳ {i['id']} [{i['severity']}] {i['title'][:50]}")
        print()

    ea = report["agents"]["executor"]
    da = report["agents"]["debugger"]
    print("─── Agent 状态 ───")
    print(f"  Executor:  {ea['status']} (msgs={ea['message_count']})")
    print(f"  Debugger:  {da['status']} (msgs={da['message_count']})")
    print()


def print_history():
    """打印所有快照的进度曲线。"""
    snapshots = sorted(SNAPSHOTS_DIR.iterdir())
    if not snapshots:
        print("暂无快照")
        return

    print(f"{'快照时间':22s} {'Total':>6s} {'Open':>6s} {'Resolved':>9s} {'%':>6s} {'Commits':>8s}")
    print("─" * 65)

    for snap_dir in snapshots:
        report_file = snap_dir / "report.json"
        if not report_file.exists():
            continue
        try:
            r = json.loads(report_file.read_text())
            s = r["summary"]
            ts = r["timestamp"][:19]
            print(f"{ts:22s} {s['total_issues']:>6d} {s['open']:>6d} "
                  f"{s['resolved']:>9d} {s['completion_pct']:>5.1f}% {s['total_commits']:>8d}")
        except Exception:
            pass


def run_loop():
    """主循环：每小时拍一次快照。"""
    print(f"[monitor] 启动定时监控（间隔 {SNAP_INTERVAL}s = {SNAP_INTERVAL//3600}h）")
    print(f"[monitor] 快照保存到 {SNAPSHOTS_DIR}")

    take_snapshot()

    while True:
        time.sleep(SNAP_INTERVAL)
        try:
            take_snapshot()
        except Exception as e:
            print(f"[monitor] 快照失败: {e}")


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Auto-Evolve Monitor")
    parser.add_argument("action", choices=["run", "snap", "report", "history"],
                        help="run=定时循环, snap=立即拍快照, report=打印报告, history=查看历史")
    args = parser.parse_args()

    if args.action == "run":
        run_loop()
    elif args.action == "snap":
        take_snapshot()
    elif args.action == "report":
        print_report()
    elif args.action == "history":
        print_history()


if __name__ == "__main__":
    main()
