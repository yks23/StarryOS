/*
 * Test: Stop 类信号应使子进程停止而非退出；waitpid(WUNTRACED) 应观察到停止状态
 * Target: SIGSTOP / SIGTSTP 处理路径（SignalOSAction::Stop）
 * Expected: 子进程 raise(SIGSTOP) 后 waitpid 返回 WIFSTOPPED，且 WSTOPSIG==SIGSTOP
 * Build: riscv64-linux-musl-gcc -static -o test_sigstop_sigcont test_sigstop_sigcont.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures = 0;
static int total = 0;

#define TEST_BEGIN(name)                                                       \
    do {                                                                       \
        total++;                                                               \
        printf("[TEST] %s ... ", (name));                                      \
    } while (0)

#define TEST_FAIL(fmt, ...)                                                    \
    do {                                                                       \
        printf("FAIL: " fmt "\n", ##__VA_ARGS__);                              \
        failures++;                                                            \
    } while (0)

#define TEST_PASS() printf("PASS\n")

static void test_sigstop_stops_child(void) {
    TEST_BEGIN("SIGSTOP stops child (WUNTRACED)");
    pid_t pid = fork();
    if (pid < 0) {
        TEST_FAIL("fork: %s", strerror(errno));
        return;
    }
    if (pid == 0) {
        raise(SIGSTOP);
        _exit(0);
    }
    int st = 0;
    if (waitpid(pid, &st, WUNTRACED) < 0) {
        TEST_FAIL("waitpid: %s", strerror(errno));
        return;
    }
    if (WIFEXITED(st)) {
        printf("FAIL: child exited (%d) instead of stopping (Stop action kills "
               "process)\n",
               WEXITSTATUS(st));
        failures++;
        return;
    }
    if (!WIFSTOPPED(st) || WSTOPSIG(st) != SIGSTOP) {
        TEST_FAIL("unexpected status: stopped=%d sig=%d\n", WIFSTOPPED(st),
                  WSTOPSIG(st));
        return;
    }
    kill(pid, SIGKILL);
    waitpid(pid, NULL, 0);
    TEST_PASS();
}

int main(void) {
    test_sigstop_stops_child();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
