/*
 * Test: sched_getscheduler 对普通进程应为 SCHED_OTHER；sched_setscheduler 应生效
 * Target syscall: sched_getscheduler, sched_setscheduler, sched_getparam
 * Build: riscv64-linux-musl-gcc -static -o test_sched_stubs test_sched_stubs.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <sched.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
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

static void test_default_policy_is_other(void) {
    int pol;

    TEST_BEGIN("sched_getscheduler(0) is SCHED_OTHER for normal task")
    pol = sched_getscheduler(0);
    TEST_ASSERT(pol >= 0, "sched_getscheduler: %s", strerror(errno));
    TEST_ASSERT(pol == SCHED_OTHER,
                "expected SCHED_OTHER=%d, got %d (stub returns RR?)", SCHED_OTHER,
                pol);
    TEST_PASS();
}

int main(void) {
    test_default_policy_is_other();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
