/*
 * Test: sched_getaffinity 对非零 pid（存活子进程）在 Linux 上应成功；Starry 对 pid!=0 返回 EPERM
 * Target syscall: sched_getaffinity
 * Expected: sched_getaffinity(child_pid, ...) == 0
 * Build: riscv64-linux-musl-gcc -static -o test_sched_affinity test_sched_affinity.c -lpthread
 */

#define _GNU_SOURCE
#include <errno.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures = 0;
static int total = 0;

#define TEST_BEGIN(name)                                                       \
    do {                                                                       \
        total++;                                                               \
        const char *_test_name = (name);                                       \
        (void)_test_name;                                                      \
        printf("[TEST] %s ... ", _test_name);                                  \
    } while (0)

#define TEST_FAIL(fmt, ...)                                                    \
    do {                                                                       \
        printf("FAIL: " fmt "\n", ##__VA_ARGS__);                              \
        failures++;                                                            \
    } while (0)

#define TEST_PASS() printf("PASS\n")

static void test_getaffinity_live_child(void) {
    TEST_BEGIN("sched_getaffinity(live child pid) succeeds (Linux)");
    pid_t cpid = fork();
    if (cpid < 0) {
        TEST_FAIL("fork: %s", strerror(errno));
        return;
    }
    if (cpid == 0) {
        for (;;)
            pause();
    }

    unsigned char buf[512];
    memset(buf, 0, sizeof buf);
    int r = sched_getaffinity(cpid, sizeof buf, (cpu_set_t *)buf);
    kill(cpid, SIGKILL);
    waitpid(cpid, NULL, 0);

    if (r < 0) {
        if (errno == EPERM) {
            printf("FAIL: errno=EPERM (kernel only supports pid==0 / current task)\n");
            failures++;
        } else {
            TEST_FAIL("sched_getaffinity: %s", strerror(errno));
        }
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_getaffinity_live_child();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
