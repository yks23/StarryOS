/*
 * Test: sched_setscheduler 应改变调度策略；sched_getscheduler 应反映设置；sched_getparam 应写入参数
 * Target syscall: sched_setscheduler, sched_getscheduler, sched_getparam
 * Expected: set SCHED_OTHER 后 getscheduler 返回 SCHED_OTHER；getparam 覆盖用户缓冲区
 * Build: riscv64-linux-musl-gcc -static -o test_sched_policy_stubs test_sched_policy_stubs.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <sched.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
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

#ifndef SCHED_OTHER
#define SCHED_OTHER 0
#endif
#ifndef SCHED_RR
#define SCHED_RR 2
#endif

static void test_policy_roundtrip(void) {
    TEST_BEGIN("sched_setscheduler(SCHED_OTHER) then sched_getscheduler matches");
    struct sched_param p;
    memset(&p, 0, sizeof p);
    p.sched_priority = 0;

    long sr = syscall(SYS_sched_setscheduler, (long)0, (long)SCHED_OTHER, &p);
    if (sr < 0) {
        TEST_FAIL("sched_setscheduler: %s", strerror(errno));
        return;
    }

    long pol = syscall(SYS_sched_getscheduler, (long)0);
    if (pol < 0) {
        TEST_FAIL("sched_getscheduler: %s", strerror(errno));
        return;
    }
    if ((int)pol != SCHED_OTHER) {
        printf("FAIL: getscheduler returned %ld (expected SCHED_OTHER=%d; stub "
               "returns SCHED_RR)\n",
               pol, SCHED_OTHER);
        failures++;
        return;
    }
    TEST_PASS();
}

static void test_getparam_writes(void) {
    TEST_BEGIN("sched_getparam fills sched_param");
    struct sched_param p;
    memset(&p, 0xab, sizeof p);
    long r = syscall(SYS_sched_getparam, (long)0, &p);
    if (r < 0) {
        TEST_FAIL("sched_getparam: %s", strerror(errno));
        return;
    }
    /* 魔数填充后，内核应写入 sched_priority（Linux 通常为 0 等合法值） */
    if (p.sched_priority == (int)0xabababab) {
        printf("FAIL: sched_priority not written (stub returns Ok without "
               "filling)\n");
        failures++;
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_policy_roundtrip();
    test_getparam_writes();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
