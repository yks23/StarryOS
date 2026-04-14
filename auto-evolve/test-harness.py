#!/usr/bin/env python3
"""
Auto-Evolve Test Harness
一条命令完成：编译全部测试 → 注入 rootfs → 启动 QEMU → 收集串口输出 → 解析结果 → 生成报告

用法:
  python3 auto-evolve/test-harness.py run                # 编译+注入+QEMU+解析 全流程
  python3 auto-evolve/test-harness.py run --test test_flock_stub  # 只跑一个
  python3 auto-evolve/test-harness.py build              # 只编译
  python3 auto-evolve/test-harness.py inject              # 编译+注入 rootfs
  python3 auto-evolve/test-harness.py report             # 查看最近结果
  python3 auto-evolve/test-harness.py list               # 列出所有测试
"""

import argparse
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime
from pathlib import Path

BASE = Path(__file__).parent
WORKSPACE = BASE.parent
TESTS_SRC = BASE / "tests"
BUILD_DIR = BASE / "test-build"
RESULTS_DIR = BASE / "test-results"
ROOTFS_IMG = None  # 动态确定

MUSL_PATHS = [
    "/opt/musl-toolchains/riscv64-linux-musl-cross/bin",
    os.path.expanduser("~/.local/musl/bin"),
]

ARCH_GCC = {
    "riscv64": "riscv64-linux-musl-gcc",
    "aarch64": "aarch64-linux-musl-gcc",
    "loongarch64": "loongarch64-linux-musl-gcc",
}

for p in MUSL_PATHS:
    if os.path.isdir(p):
        os.environ["PATH"] = p + ":" + os.environ.get("PATH", "")


def log(msg: str):
    print(f"\033[36m[harness]\033[0m {msg}")


def err(msg: str):
    print(f"\033[31m[harness]\033[0m {msg}")


# ── 编译 ──────────────────────────────────────────────────

def compile_tests(arch: str, only: str = None) -> dict:
    """编译测试，返回 {name: path} 或 {name: error}。"""
    gcc = ARCH_GCC.get(arch, f"{arch}-linux-musl-gcc")
    out = BUILD_DIR / arch
    out.mkdir(parents=True, exist_ok=True)

    results = {}
    sources = sorted(TESTS_SRC.glob("test_*.c"))
    if only:
        sources = [s for s in sources if s.stem == only or only in s.stem]

    for src in sources:
        name = src.stem
        binary = out / name
        cmd = [gcc, "-static", "-o", str(binary), str(src), "-lpthread"]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            if r.returncode == 0:
                results[name] = {"status": "ok", "path": str(binary)}
            else:
                results[name] = {"status": "compile_error", "error": r.stderr[:200]}
                err(f"  编译失败: {name}")
        except FileNotFoundError:
            results[name] = {"status": "no_gcc", "error": f"找不到 {gcc}"}
            err(f"  找不到编译器: {gcc}")
            break
        except Exception as e:
            results[name] = {"status": "error", "error": str(e)}

    ok = sum(1 for v in results.values() if v["status"] == "ok")
    log(f"编译: {ok}/{len(results)} 成功 ({arch})")
    return results


# ── 注入 rootfs ──────────────────────────────────────────

def find_rootfs(arch: str) -> Path:
    candidates = [
        WORKSPACE / f"rootfs-{arch}.img",
        WORKSPACE / f"make/disk.img",
    ]
    for c in candidates:
        if c.exists():
            return c
    return None


def inject_into_rootfs(arch: str, binaries: dict) -> bool:
    """把编译好的测试二进制注入 rootfs 镜像。"""
    rootfs = find_rootfs(arch)
    if not rootfs:
        err(f"rootfs 不存在，先运行: cd {WORKSPACE} && make ARCH={arch} rootfs")
        return False

    # 复制一份避免污染原镜像
    test_rootfs = BUILD_DIR / arch / "test-disk.img"
    shutil.copy2(rootfs, test_rootfs)

    mnt = tempfile.mkdtemp(prefix="harness_mnt_")
    try:
        subprocess.run(["sudo", "mount", "-o", "loop", str(test_rootfs), mnt],
                       check=True, capture_output=True)

        test_dir = os.path.join(mnt, "bin", "tests")
        subprocess.run(["sudo", "mkdir", "-p", test_dir], check=True, capture_output=True)

        count = 0
        test_names = []
        for name, info in binaries.items():
            if info["status"] != "ok":
                continue
            subprocess.run(["sudo", "cp", info["path"], test_dir],
                           check=True, capture_output=True)
            test_names.append(name)
            count += 1

        # 生成自动运行脚本
        run_script = os.path.join(mnt, "bin", "run-all-tests.sh")
        script_content = "#!/bin/sh\n"
        script_content += 'echo "AUTO_TEST_BEGIN"\n'
        for name in sorted(test_names):
            script_content += f'echo "=== RUN {name} ==="\n'
            script_content += f'timeout 30 /bin/tests/{name} 2>&1\n'
            script_content += f'echo "=== EXIT {name} $? ==="\n'
        script_content += 'echo "AUTO_TEST_END"\n'

        with tempfile.NamedTemporaryFile(mode='w', suffix='.sh', delete=False) as f:
            f.write(script_content)
            tmp_script = f.name
        subprocess.run(["sudo", "cp", tmp_script, run_script], check=True, capture_output=True)
        subprocess.run(["sudo", "chmod", "+x", run_script], check=True, capture_output=True)
        os.unlink(tmp_script)

        subprocess.run(["sudo", "umount", mnt], check=True, capture_output=True)
        log(f"注入: {count} 个测试 → {test_rootfs}")
        return True

    except Exception as e:
        err(f"注入失败: {e}")
        subprocess.run(["sudo", "umount", mnt], capture_output=True)
        return False
    finally:
        os.rmdir(mnt)


# ── QEMU 运行 + 串口收集 ────────────────────────────────

def run_qemu_and_collect(arch: str, timeout_sec: int = 120) -> str:
    """启动 QEMU，通过串口 TCP 收集输出，返回全部输出文本。"""
    test_rootfs = BUILD_DIR / arch / "test-disk.img"
    if not test_rootfs.exists():
        err("测试 rootfs 不存在，先运行 inject")
        return ""

    # 复制到 make/disk.img（Starry 的 Makefile 从这里读）
    disk_img = WORKSPACE / "make" / "disk.img"
    shutil.copy2(test_rootfs, disk_img)

    port = 14444
    log(f"启动 QEMU ({arch})，串口 TCP :{port}，超时 {timeout_sec}s")

    # 修改 init.sh 让它自动运行测试脚本然后关机
    # 通过环境变量传递（Starry 的 init.sh 会执行 sh --login）
    qemu_cmd = [
        "make", "-C", str(WORKSPACE), f"ARCH={arch}", "ACCEL=n", "justrun",
        f'QEMU_ARGS=-monitor none -serial tcp::{port},server=on,wait=off -nographic',
    ]

    output_lines = []
    qemu_proc = None
    collected = threading.Event()

    def collect_serial():
        nonlocal output_lines
        time.sleep(2)  # 等 QEMU 启动
        retries = 0
        while retries < 10:
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(5)
                sock.connect(("127.0.0.1", port))
                break
            except (ConnectionRefusedError, socket.timeout):
                retries += 1
                time.sleep(1)
        else:
            err("无法连接 QEMU 串口")
            collected.set()
            return

        sock.settimeout(2)
        buf = ""
        start = time.time()

        # 等待 shell 提示符
        while time.time() - start < 30:
            try:
                data = sock.recv(4096).decode("utf-8", errors="replace")
                buf += data
                if "starry" in buf.lower() or "#" in buf or "$" in buf:
                    break
            except socket.timeout:
                pass

        # 发送测试命令
        sock.sendall(b"/bin/run-all-tests.sh\n")

        # 收集输出直到看到 AUTO_TEST_END 或超时
        start = time.time()
        while time.time() - start < timeout_sec:
            try:
                data = sock.recv(4096).decode("utf-8", errors="replace")
                buf += data
                if "AUTO_TEST_END" in buf:
                    break
            except socket.timeout:
                pass

        output_lines.append(buf)
        sock.close()
        collected.set()

    try:
        qemu_proc = subprocess.Popen(
            qemu_cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            cwd=str(WORKSPACE),
        )

        t = threading.Thread(target=collect_serial, daemon=True)
        t.start()
        collected.wait(timeout=timeout_sec + 30)

    finally:
        if qemu_proc:
            qemu_proc.kill()
            qemu_proc.wait()

    full_output = "\n".join(output_lines)
    log(f"收集到 {len(full_output)} 字符输出")
    return full_output


# ── 输出解析 ──────────────────────────────────────────────

def parse_output(raw: str) -> dict:
    """解析 QEMU 串口输出，提取每个测试的结果。"""
    results = {}
    current_test = None
    current_output = []

    for line in raw.split("\n"):
        line = line.strip()

        # === RUN test_xxx ===
        m = re.match(r"=== RUN (\S+) ===", line)
        if m:
            if current_test:
                results[current_test] = parse_single_test("\n".join(current_output))
            current_test = m.group(1)
            current_output = []
            continue

        # === EXIT test_xxx N ===
        m = re.match(r"=== EXIT (\S+) (\d+) ===", line)
        if m:
            name = m.group(1)
            exit_code = int(m.group(2))
            if current_test == name:
                result = parse_single_test("\n".join(current_output))
                result["exit_code"] = exit_code
                results[name] = result
                current_test = None
                current_output = []
            continue

        if current_test:
            current_output.append(line)

    if current_test:
        results[current_test] = parse_single_test("\n".join(current_output))

    return results


def parse_single_test(output: str) -> dict:
    """解析单个测试的 [TEST] ... PASS/FAIL 输出。"""
    sub_tests = []
    for line in output.split("\n"):
        m = re.match(r"\[TEST\]\s+(.+?)\s+\.\.\.\s+(PASS|FAIL.*)", line.strip())
        if m:
            sub_tests.append({
                "name": m.group(1),
                "passed": m.group(2) == "PASS",
                "detail": "" if m.group(2) == "PASS" else m.group(2),
            })

    summary = re.search(r"SUMMARY:\s*(\d+)/(\d+)\s*passed", output)
    passed = sum(1 for t in sub_tests if t["passed"])
    total = len(sub_tests)

    return {
        "sub_tests": sub_tests,
        "passed": passed,
        "failed": total - passed,
        "total": total,
        "all_passed": passed == total and total > 0,
        "raw_output": output[-500:] if len(output) > 500 else output,
    }


# ── 报告 ──────────────────────────────────────────────────

def generate_report(compile_results: dict, test_results: dict, arch: str) -> dict:
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)

    compiled = sum(1 for v in compile_results.values() if v["status"] == "ok")
    tested = len(test_results)
    all_passed = sum(1 for v in test_results.values() if v.get("all_passed"))
    any_failed = sum(1 for v in test_results.values() if not v.get("all_passed"))

    total_sub = sum(v.get("total", 0) for v in test_results.values())
    passed_sub = sum(v.get("passed", 0) for v in test_results.values())

    report = {
        "timestamp": datetime.now().isoformat(),
        "arch": arch,
        "summary": {
            "compiled": compiled,
            "tested": tested,
            "test_files_passed": all_passed,
            "test_files_failed": any_failed,
            "sub_tests_total": total_sub,
            "sub_tests_passed": passed_sub,
            "pass_rate": round(passed_sub / max(total_sub, 1) * 100, 1),
        },
        "tests": {},
    }

    for name in sorted(set(list(compile_results.keys()) + list(test_results.keys()))):
        entry = {"compile": "ok"}
        if name in compile_results and compile_results[name]["status"] != "ok":
            entry["compile"] = compile_results[name]["status"]
        if name in test_results:
            tr = test_results[name]
            entry["passed"] = tr.get("all_passed", False)
            entry["sub_passed"] = tr.get("passed", 0)
            entry["sub_total"] = tr.get("total", 0)
            entry["exit_code"] = tr.get("exit_code", -1)
            if not tr.get("all_passed"):
                failed = [s for s in tr.get("sub_tests", []) if not s["passed"]]
                entry["failures"] = [f["detail"] for f in failed[:5]]
        else:
            entry["passed"] = None  # 未运行

        report["tests"][name] = entry

    # 保存
    report_path = RESULTS_DIR / "latest.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False))
    ts = datetime.now().strftime("%Y%m%d-%H%M%S")
    history_dir = RESULTS_DIR / "history"
    history_dir.mkdir(exist_ok=True)
    (history_dir / f"{ts}.json").write_text(json.dumps(report, indent=2, ensure_ascii=False))

    return report


def print_report(report: dict):
    s = report["summary"]
    print()
    print("=" * 65)
    print(f"  测试报告  {report['timestamp'][:19]}  arch={report['arch']}")
    print("=" * 65)
    print(f"  编译:     {s['compiled']} 个")
    print(f"  运行:     {s['tested']} 个")
    print(f"  文件通过: {s['test_files_passed']}/{s['tested']}")
    print(f"  子用例:   {s['sub_tests_passed']}/{s['sub_tests_total']} ({s['pass_rate']}%)")
    print()

    passed_bar = int(40 * s["sub_tests_passed"] / max(s["sub_tests_total"], 1))
    print(f"  {'█' * passed_bar}{'░' * (40 - passed_bar)} {s['pass_rate']}%")
    print()

    # 失败列表
    failed = [(k, v) for k, v in report["tests"].items()
              if v.get("passed") is False]
    if failed:
        print("  ── 失败 ──")
        for name, info in failed:
            detail = "; ".join(info.get("failures", [])[:2])
            if detail:
                detail = f" ({detail[:60]})"
            print(f"  ✗ {name}: {info.get('sub_passed',0)}/{info.get('sub_total',0)}{detail}")
        print()

    # 通过列表
    passed = [(k, v) for k, v in report["tests"].items()
              if v.get("passed") is True]
    if passed:
        print(f"  ── 通过 ({len(passed)}) ──")
        for name, info in passed:
            print(f"  ✓ {name}: {info.get('sub_passed',0)}/{info.get('sub_total',0)}")
        print()

    # 未运行
    not_run = [(k, v) for k, v in report["tests"].items()
               if v.get("passed") is None]
    if not_run:
        print(f"  ── 未运行 ({len(not_run)}) ──")
        for name, info in not_run[:5]:
            print(f"  - {name}: {info.get('compile', '?')}")
        if len(not_run) > 5:
            print(f"    ... +{len(not_run)-5}")
    print()


# ── 主入口 ────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Auto-Evolve Test Harness: 一条命令跑全部测试")
    parser.add_argument("action",
                        choices=["run", "build", "inject", "report", "list"],
                        help="run=全流程, build=只编译, inject=编译+注入, report=看报告, list=列出测试")
    parser.add_argument("--arch", default="riscv64")
    parser.add_argument("--test", help="只运行指定测试（名称或部分匹配）")
    parser.add_argument("--timeout", type=int, default=120, help="QEMU 运行超时（秒）")
    parser.add_argument("--no-qemu", action="store_true",
                        help="跳过 QEMU，只编译+注入（agent 可用此模式）")
    args = parser.parse_args()

    if args.action == "list":
        tests = sorted(TESTS_SRC.glob("test_*.c"))
        print(f"共 {len(tests)} 个测试文件:\n")
        for t in tests:
            lines = t.read_text().split("\n")
            desc = ""
            for l in lines[:5]:
                if "Test:" in l:
                    desc = l.split("Test:")[1].strip()
                    break
            print(f"  {t.stem:40s} {desc}")
        return

    if args.action == "report":
        latest = RESULTS_DIR / "latest.json"
        if latest.exists():
            print_report(json.loads(latest.read_text()))
        else:
            print("暂无报告，先运行: python3 auto-evolve/test-harness.py run")
        return

    # build
    log(f"── 编译测试 ({args.arch}) ──")
    compile_results = compile_tests(args.arch, args.test)

    if args.action == "build":
        ok = sum(1 for v in compile_results.values() if v["status"] == "ok")
        log(f"编译完成: {ok}/{len(compile_results)}")
        log(f"二进制在: {BUILD_DIR / args.arch}")
        return

    # inject
    log(f"── 注入 rootfs ──")
    ok_binaries = {k: v for k, v in compile_results.items() if v["status"] == "ok"}
    if not inject_into_rootfs(args.arch, ok_binaries):
        err("注入失败，检查 rootfs 是否存在")
        if args.action == "inject":
            return
        log("跳过 QEMU 运行")
        report = generate_report(compile_results, {}, args.arch)
        print_report(report)
        return

    if args.action == "inject":
        log("注入完成（未运行 QEMU）")
        return

    # run with QEMU
    if args.no_qemu:
        log("--no-qemu: 跳过 QEMU 运行")
        report = generate_report(compile_results, {}, args.arch)
        print_report(report)
        return

    log(f"── 编译内核 ──")
    r = subprocess.run(
        ["make", f"ARCH={args.arch}", "build"],
        capture_output=True, text=True, cwd=str(WORKSPACE), timeout=300,
    )
    if r.returncode != 0:
        err(f"内核编译失败:\n{r.stderr[-500:]}")
        return

    log(f"── 启动 QEMU + 运行测试 ──")
    raw_output = run_qemu_and_collect(args.arch, args.timeout)

    if not raw_output:
        err("未收集到输出")
        report = generate_report(compile_results, {}, args.arch)
        print_report(report)
        return

    # 保存原始输出
    raw_path = RESULTS_DIR / "raw-output.txt"
    raw_path.write_text(raw_output)
    log(f"原始输出: {raw_path}")

    # 解析
    log(f"── 解析结果 ──")
    test_results = parse_output(raw_output)
    log(f"解析到 {len(test_results)} 个测试结果")

    # 报告
    report = generate_report(compile_results, test_results, args.arch)
    print_report(report)
    log(f"报告: {RESULTS_DIR / 'latest.json'}")


if __name__ == "__main__":
    main()
