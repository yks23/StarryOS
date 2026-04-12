/*
 * Test: SIGSTOP 默认动作应停止进程，waitpid WUNTRACED 可见
 * Target: 默认信号动作（非独立 syscall）
 * Build: riscv64-linux-musl-gcc -static -o test_sigstop_semantic test_sigstop_semantic.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures;
static int total;

#define TEST_BEGIN(name)                                                       \
    do {                                                                       \
        total++;                                                               \
        printf("[TEST] %s ... ", (name));

#define TEST_ASSERT(cond, fmt, ...)                                            \
    if (!(cond)) {                                                             \
        printf("FAIL: " fmt "\n", ##__VA_ARGS__);                              \
        failures++;                                                            \
        break;                                                                 \
    }

#define TEST_PASS()                                                            \
    printf("PASS\n");                                                          \
    } while (0)

static void test_sigstop_stops_child(void) {
    pid_t p;
    int st;

    TEST_BEGIN("SIGSTOP stops child; waitpid reports WIFSTOPPED")
    p = fork();
    TEST_ASSERT(p >= 0, "fork: %s", strerror(errno));
    if (p == 0) {
        pause();
        _exit(0);
    }
    TEST_ASSERT(kill(p, SIGSTOP) == 0, "kill SIGSTOP: %s", strerror(errno));
    TEST_ASSERT(waitpid(p, &st, WUNTRACED) == p, "waitpid: %s", strerror(errno));
    TEST_ASSERT(WIFSTOPPED(st), "expected stopped, got status 0x%x", st);
    TEST_ASSERT(WSTOPSIG(st) == SIGSTOP, "stop sig %d", WSTOPSIG(st));
    kill(p, SIGKILL);
    waitpid(p, NULL, 0);
    TEST_PASS();
}

int main(void) {
    test_sigstop_stops_child();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
