/*
 * Test: times(2) 的 tms_cutime 在子进程消耗用户态 CPU 并经 wait 回收后应增长（Linux 将子进程 user time 计入父 cutime）
 * Target syscall: times
 * Expected: wait 子进程后父进程 tms_cutime 相对 wait 前增加（在子进程有用户态耗时的情况下）
 * Build: riscv64-linux-musl-gcc -static -o test_times_cutime test_times_cutime.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/times.h>
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

static void burn_user(void) {
    volatile unsigned long x = 0;
    for (int i = 0; i < 80000000; i++)
        x += (unsigned long)i;
    (void)x;
}

static void test_cutime_includes_child_after_wait(void) {
    TEST_BEGIN("times: tms_cutime increases after child user CPU and wait")
    struct tms t1, t2;
    memset(&t1, 0, sizeof(t1));
    memset(&t2, 0, sizeof(t2));
    TEST_ASSERT(times(&t1) != (clock_t)-1, "times: %s", strerror(errno));

    pid_t pid = fork();
    TEST_ASSERT(pid >= 0, "fork: %s", strerror(errno));
    if (pid == 0) {
        burn_user();
        _exit(0);
    }
    int st;
    TEST_ASSERT(waitpid(pid, &st, 0) == pid, "waitpid: %s", strerror(errno));

    TEST_ASSERT(times(&t2) != (clock_t)-1, "times: %s", strerror(errno));

    clock_t d_cut = t2.tms_cutime - t1.tms_cutime;
    TEST_ASSERT(d_cut > 0,
                "tms_cutime did not increase (got delta cutime=%ld; stub may copy utime only)",
                (long)d_cut);

    TEST_PASS();
}

int main(void) {
    test_cutime_includes_child_after_wait();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
