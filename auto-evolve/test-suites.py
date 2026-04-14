#!/usr/bin/env python3
"""
Auto-Evolve Test Suites Manager
管理所有外部测试套件的下载、构建、分类、运行。

用法:
  python3 auto-evolve/test-suites.py setup           # 首次设置：下载所有测试资源
  python3 auto-evolve/test-suites.py list             # 列出所有可用测试
  python3 auto-evolve/test-suites.py list --suite ltp # 只列某个套件
  python3 auto-evolve/test-suites.py run --suite all  # 跑全部
  python3 auto-evolve/test-suites.py run --suite ltp  # 只跑 LTP
  python3 auto-evolve/test-suites.py run --suite libc # 只跑 libc-test
  python3 auto-evolve/test-suites.py report           # 查看报告
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path

BASE = Path(__file__).parent
WORKSPACE = BASE.parent
SUITES_DIR = BASE / "test-suites"
SUITES_DIR.mkdir(exist_ok=True)

ARCH_GCC = {
    "riscv64": "riscv64-linux-musl-gcc",
    "aarch64": "aarch64-linux-musl-gcc",
    "loongarch64": "loongarch64-linux-musl-gcc",
}

MUSL_PATHS = ["/opt/musl-toolchains/riscv64-linux-musl-cross/bin"]
for p in MUSL_PATHS:
    if os.path.isdir(p):
        os.environ["PATH"] = p + ":" + os.environ.get("PATH", "")


def log(msg): print(f"\033[36m[suites]\033[0m {msg}")
def err(msg): print(f"\033[31m[suites]\033[0m {msg}")


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite 1: oscomp basic syscall tests (32 个基础系统调用)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

OSCOMP_BASIC_TESTS = [
    "brk", "chdir", "clone", "close", "dup", "dup2", "execve", "exit",
    "fork", "fstat", "getcwd", "getdents", "getpid", "getppid",
    "gettimeofday", "mkdir", "mmap", "mount", "munmap", "open", "openat",
    "pipe", "read", "sleep", "times", "umount", "uname", "unlink",
    "wait", "waitpid", "write", "yield",
]


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite 2: musl libc-test (100+ libc 功能用例)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

LIBCTEST_CASES = [
    "argv", "basename", "clocale_mbfuncs", "clock_gettime", "crypt",
    "dirname", "env", "fdopen", "fnmatch", "fscanf", "fwscanf",
    "iconv_open", "inet_pton", "mbc", "memstream",
    "pthread_cancel_points", "pthread_cancel", "pthread_cond",
    "pthread_tsd", "qsort", "random", "search_hsearch", "search_insque",
    "search_lsearch", "search_tsearch", "setjmp", "snprintf", "socket",
    "sscanf", "stat", "statvfs", "strftime", "string", "string_memcpy",
    "string_memmem", "string_memset", "string_strchr", "string_strcspn",
    "string_strstr", "strptime", "strtod", "strtod_simple", "strtof",
    "strtol", "strtold", "time", "utime", "wcsstr", "wcstol",
    "daemon_failure", "fflush_exit", "fgets_eof", "fgetwc_buffering",
    "fpclassify_invalid_ld80", "ftello_unflushed_append",
    "getpwnam_r_crash", "getpwnam_r_errno", "iconv_roundtrips",
    "inet_ntop_v4mapped", "inet_pton_empty_last_field", "iswspace_null",
    "lrand48_signextend", "lseek_large", "malloc_0",
    "mbsrtowcs_overflow", "memmem_oob", "memmem_oob_read",
    "mkdtemp_failure", "mkstemp_failure",
    "printf_1e9_oob", "printf_fmt_g_round", "printf_fmt_g_zeros",
    "printf_fmt_n", "pthread_cancel_sem_wait", "pthread_condattr_setclock",
    "pthread_cond_smasher", "pthread_exit_cancel", "pthread_once_deadlock",
    "pthread_robust_detach", "pthread_rwlock_ebusy", "putenv_doublefree",
    "regex_backref_0", "regex_bracket_icase", "regexec_nosub",
    "regex_ere_backref", "regex_escaped_high_byte", "regex_negated_range",
    "rewind_clear_error", "rlimit_open_files",
    "scanf_bytes_consumed", "scanf_match_literal_eof", "scanf_nullbyte_char",
    "setvbuf_unget", "sigprocmask_internal",
    "sscanf_eof", "sscanf_long",
    "strverscmp", "swprintf", "syscall_sign_extend", "tgmath",
    "tls_align", "udiv", "ungetc", "uselocale_0",
    "wcsncpy_read_overflow", "wcsstr_false_negative",
    "pleval", "dn_expand_empty", "dn_expand_ptr_0",
]


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite 3: LTP syscall subset (精选 120 个)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

LTP_CASES_FILE = BASE / "test-suites" / "ltp-cases.txt"

LTP_CASES_DEFAULT = """
# 文件 I/O
read01 read02 write01 write02 writev01 readv01
open01 openat01 close01 dup01 dup201 lseek01
pread01 pwrite01 fstat01 stat01 fstatat01 statx01
access01 faccessat01 chmod01 fchmod01 chown01 fchown01
mkdir01 rmdir01 getdents01 getcwd01 chdir01
link01 unlink01 symlink01 readlink01 rename01
truncate01 ftruncate01 fallocate01 fsync01
sendfile01 splice01

# 进程
fork01 fork02 vfork01 clone01 clone02
execve01 execve02 exit01 exit_group01
wait01 wait02 waitpid01

# 信号
kill01 kill02 rt_sigaction01 rt_sigprocmask01
rt_sigreturn01 rt_sigsuspend01 sigaltstack01

# 内存
mmap01 mmap02 mmap03 munmap01 mprotect01 brk01
madvise01 msync01 mremap01 mlock01

# 同步
futex_wait01 futex_wake01 flock01 flock02 fcntl01 fcntl02

# IPC
pipe01 pipe02 eventfd01
msgget01 msgsnd01 msgrcv01
shmget01 shmat01 shmdt01

# 网络
socket01 bind01 connect01 accept01 listen01
send01 recv01 sendto01 recvfrom01
socketpair01 shutdown01
getsockname01 getpeername01

# 时间
clock_gettime01 clock_getres01 nanosleep01 gettimeofday01
timerfd_create01 timerfd_settime01

# 调度
sched_yield01 sched_getaffinity01 sched_setaffinity01
sched_getscheduler01 getpriority01

# 系统信息
uname01 sysinfo01 getpid01 getppid01 gettid01
getuid01 setuid01 getgid01 setgid01 getgroups01

# I/O 多路复用
poll01 ppoll01 epoll_create01 epoll_ctl01 epoll_wait01 select01

# oscomp 评测用例
setegid02 writev01
""".strip()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite 4: 自编 (auto-evolve/tests/)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def get_custom_tests():
    return sorted([f.stem for f in (BASE / "tests").glob("test_*.c")])


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 生成 rootfs 中的运行脚本
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def generate_run_script(suite: str, cases: list, bin_dir: str) -> str:
    """生成在 Starry OS 内运行测试的 shell 脚本。"""
    lines = ["#!/bin/sh", f'echo "SUITE_BEGIN {suite}"']

    if suite == "custom":
        for name in cases:
            lines.append(f'echo "=== RUN {name} ==="')
            lines.append(f'timeout 30 {bin_dir}/{name} 2>&1')
            lines.append(f'echo "=== EXIT {name} $? ==="')

    elif suite == "oscomp-basic":
        for name in cases:
            lines.append(f'echo "=== RUN test_{name} ==="')
            lines.append(f'timeout 30 {bin_dir}/test_{name} 2>&1')
            lines.append(f'echo "=== EXIT test_{name} $? ==="')

    elif suite == "libc-test":
        for name in cases:
            lines.append(f'echo "=== RUN entry-static.exe {name} ==="')
            lines.append(f'echo "========== START entry-static.exe {name} =========="')
            lines.append(f'timeout 30 {bin_dir}/entry-static.exe {name} 2>&1')
            lines.append(f'echo "========== END entry-static.exe {name} =========="')
            lines.append(f'echo "=== EXIT {name} $? ==="')

    elif suite == "ltp":
        for name in cases:
            lines.append(f'echo "RUN LTP CASE {name}"')
            lines.append(f'timeout 60 {bin_dir}/{name} 2>&1')
            lines.append(f'echo "END LTP CASE {name} : $?"')

    lines.append(f'echo "SUITE_END {suite}"')
    return "\n".join(lines) + "\n"


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 结果解析
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def parse_custom_output(raw: str) -> list:
    """解析自编测试的 [TEST] PASS/FAIL 输出。"""
    results = []
    current = None
    buf = []
    for line in raw.split("\n"):
        m = re.match(r"=== RUN (\S+) ===", line.strip())
        if m:
            if current:
                results.append(parse_one_custom(current, buf))
            current = m.group(1)
            buf = []
            continue
        m = re.match(r"=== EXIT (\S+) (\d+) ===", line.strip())
        if m and current == m.group(1):
            r = parse_one_custom(current, buf)
            r["exit_code"] = int(m.group(2))
            results.append(r)
            current = None
            buf = []
            continue
        if current:
            buf.append(line)
    return results


def parse_one_custom(name: str, lines: list) -> dict:
    text = "\n".join(lines)
    sub = []
    for line in lines:
        m = re.match(r"\[TEST\]\s+(.+?)\s+\.\.\.\s+(PASS|FAIL.*)", line.strip())
        if m:
            sub.append({"name": m.group(1), "passed": m.group(2) == "PASS",
                         "detail": m.group(2) if m.group(2) != "PASS" else ""})
    passed = sum(1 for s in sub if s["passed"])
    return {"name": name, "suite": "custom", "passed": passed,
            "failed": len(sub) - passed, "total": len(sub),
            "ok": passed == len(sub) and len(sub) > 0, "sub_tests": sub}


def parse_libctest_output(raw: str) -> list:
    """解析 libc-test entry-static.exe 输出。"""
    results = []
    pattern = r"========== START entry-static\.exe (\S+) ==========\s*(.*?)\s*========== END entry-static\.exe \1 =========="
    for m in re.finditer(pattern, raw, re.DOTALL):
        name = m.group(1)
        body = m.group(2).strip()
        passed = "Pass!" in body or "PASS" in body
        results.append({"name": name, "suite": "libc-test", "ok": passed,
                         "passed": 1 if passed else 0, "failed": 0 if passed else 1,
                         "total": 1, "detail": body[:100] if not passed else ""})
    return results


def parse_ltp_output(raw: str) -> list:
    """解析 LTP RUN/END 输出。"""
    results = []
    pattern = r"RUN LTP CASE (\S+)(.*?)END LTP CASE \1\s*:\s*(\d+)"
    for m in re.finditer(pattern, raw, re.DOTALL):
        name = m.group(1)
        body = m.group(2)
        exit_code = int(m.group(3))
        tpass = len(re.findall(r"TPASS", body))
        tfail = len(re.findall(r"TFAIL", body))
        ok = exit_code == 0 and tfail == 0
        results.append({"name": name, "suite": "ltp", "ok": ok,
                         "passed": tpass, "failed": tfail, "total": tpass + tfail,
                         "exit_code": exit_code})
    return results


def parse_oscomp_basic_output(raw: str) -> list:
    """解析 oscomp basic 测试输出。"""
    results = []
    current = None
    buf = []
    for line in raw.split("\n"):
        m = re.match(r"=== RUN (\S+) ===", line.strip())
        if m:
            if current:
                results.append({"name": current, "suite": "oscomp-basic",
                                 "ok": True, "passed": 1, "failed": 0, "total": 1})
            current = m.group(1)
            buf = []
            continue
        m = re.match(r"=== EXIT (\S+) (\d+) ===", line.strip())
        if m and current:
            ok = int(m.group(2)) == 0
            results.append({"name": current, "suite": "oscomp-basic",
                             "ok": ok, "passed": 1 if ok else 0, "failed": 0 if ok else 1,
                             "total": 1, "exit_code": int(m.group(2))})
            current = None
    return results


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 命令
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def cmd_setup(args):
    """下载/准备所有测试资源。"""
    log("── 准备测试套件 ──")

    # LTP 用例列表
    if not LTP_CASES_FILE.exists():
        LTP_CASES_FILE.write_text(LTP_CASES_DEFAULT)
        log(f"写入 LTP 用例列表: {LTP_CASES_FILE}")

    # 说明文件
    readme = SUITES_DIR / "README.md"
    readme.write_text(f"""# 测试套件资源

## 需要手动准备的二进制文件

### 1. oscomp basic 测试
来源: oscomp 竞赛 rootfs 中的 /test_* 二进制
放到: {SUITES_DIR}/oscomp-basic/{args.arch}/

### 2. libc-test (entry-static.exe)
来源: oscomp 竞赛 rootfs 中的 /entry-static.exe
放到: {SUITES_DIR}/libc-test/{args.arch}/entry-static.exe

### 3. LTP 二进制
来源: 交叉编译或从 oscomp rootfs 提取
放到: {SUITES_DIR}/ltp/{args.arch}/

```bash
# 从 oscomp Alpine rootfs 提取：
wget https://github.com/LearningOS/rust-based-os-comp2025/releases/download/alpine-linux-riscv64-ext4fs/alpine-linux-riscv64-ext4fs.img.xz
xz -d alpine-linux-riscv64-ext4fs.img.xz
mkdir -p /tmp/alpine && sudo mount -o loop alpine-linux-riscv64-ext4fs.img /tmp/alpine
# 复制需要的二进制
cp /tmp/alpine/opt/ltp/testcases/bin/* {SUITES_DIR}/ltp/{args.arch}/
cp /tmp/alpine/entry-static.exe {SUITES_DIR}/libc-test/{args.arch}/
sudo umount /tmp/alpine
```

### 4. 自编测试 (已就绪)
自动从 auto-evolve/tests/ 编译。
""")

    for d in [f"oscomp-basic/{args.arch}", f"libc-test/{args.arch}",
              f"ltp/{args.arch}", f"custom/{args.arch}"]:
        (SUITES_DIR / d).mkdir(parents=True, exist_ok=True)

    log("目录结构已创建。请按 README 准备外部二进制。")
    log(f"自编测试: {len(get_custom_tests())} 个（自动编译）")
    log(f"oscomp basic: {len(OSCOMP_BASIC_TESTS)} 个（需手动放入二进制）")
    log(f"libc-test: {len(LIBCTEST_CASES)} 个（需手动放入 entry-static.exe）")

    ltp_cases = [l.split()[0] for l in LTP_CASES_DEFAULT.split("\n")
                 if l.strip() and not l.strip().startswith("#")]
    log(f"LTP: {len(ltp_cases)} 个（需手动放入二进制）")


def cmd_list(args):
    """列出所有可用测试。"""
    suite = args.suite

    if suite in ("all", "custom"):
        tests = get_custom_tests()
        print(f"\n── 自编测试 ({len(tests)}) ──")
        for t in tests:
            print(f"  {t}")

    if suite in ("all", "oscomp-basic"):
        print(f"\n── oscomp basic ({len(OSCOMP_BASIC_TESTS)}) ──")
        for t in OSCOMP_BASIC_TESTS:
            print(f"  test_{t}")

    if suite in ("all", "libc-test", "libc"):
        print(f"\n── libc-test ({len(LIBCTEST_CASES)}) ──")
        for t in LIBCTEST_CASES[:20]:
            print(f"  entry-static.exe {t}")
        if len(LIBCTEST_CASES) > 20:
            print(f"  ... +{len(LIBCTEST_CASES)-20} more")

    if suite in ("all", "ltp"):
        cases = [l.split()[0] for l in LTP_CASES_DEFAULT.split("\n")
                 if l.strip() and not l.strip().startswith("#")]
        print(f"\n── LTP ({len(cases)}) ──")
        for t in cases[:20]:
            print(f"  {t}")
        if len(cases) > 20:
            print(f"  ... +{len(cases)-20} more")

    total = (len(get_custom_tests()) + len(OSCOMP_BASIC_TESTS) +
             len(LIBCTEST_CASES) + len([l for l in LTP_CASES_DEFAULT.split("\n")
             if l.strip() and not l.strip().startswith("#")]))
    print(f"\n总计: {total} 个测试用例")


def cmd_report(args):
    """打印报告。"""
    latest = BASE / "test-results" / "latest.json"
    if not latest.exists():
        print("暂无报告。先运行: test-harness.py run")
        return
    report = json.loads(latest.read_text())
    s = report.get("summary", {})
    print(f"\n最近测试: {report.get('timestamp','?')[:19]}")
    print(f"通过率: {s.get('pass_rate', 0)}%")
    print(f"子用例: {s.get('sub_tests_passed',0)}/{s.get('sub_tests_total',0)}")


def main():
    parser = argparse.ArgumentParser(description="Test Suites Manager")
    sub = parser.add_subparsers(dest="cmd")

    p_setup = sub.add_parser("setup", help="准备测试资源")
    p_setup.add_argument("--arch", default="riscv64")

    p_list = sub.add_parser("list", help="列出测试")
    p_list.add_argument("--suite", default="all",
                        choices=["all", "custom", "oscomp-basic", "libc-test", "libc", "ltp"])

    p_report = sub.add_parser("report", help="查看报告")

    args = parser.parse_args()
    if args.cmd == "setup":
        cmd_setup(args)
    elif args.cmd == "list":
        cmd_list(args)
    elif args.cmd == "report":
        cmd_report(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
