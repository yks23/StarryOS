/*
 * Test: membarrier QUERY与 Starry 能力位（无 SMP 全局屏障时 GLOBAL 未宣称且调用 EINVAL）
 * Target syscall: membarrier
 * Expected: QUERY 成功；掩码不含 GLOBAL；MEMBARRIER_CMD_GLOBAL 返回 EINVAL
 * Build: riscv64-linux-musl-gcc -static -o test_membarrier test_membarrier.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

/* Linux uapi */
#ifndef __NR_membarrier
#if defined(__riscv) && __riscv_xlen == 64
#define __NR_membarrier 283
#elif defined(__x86_64__)
#define __NR_membarrier 324
#else
#define __NR_membarrier 283
#endif
#endif

#define MEMBARRIER_CMD_QUERY 0
#define MEMBARRIER_CMD_GLOBAL 1

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

static void test_query(void) {
    TEST_BEGIN("membarrier(MEMBARRIER_CMD_QUERY)");
    long r = syscall(__NR_membarrier, (long)MEMBARRIER_CMD_QUERY, 0, 0);
    if (r < 0) {
        TEST_FAIL("membarrier QUERY: %s", strerror(errno));
        return;
    }
    if ((r & MEMBARRIER_CMD_GLOBAL) != 0) {
        TEST_FAIL("QUERY mask must not advertise MEMBARRIER_CMD_GLOBAL (issue-203)");
        return;
    }
    TEST_PASS();
}

static void test_global_unsupported(void) {
    TEST_BEGIN("membarrier(MEMBARRIER_CMD_GLOBAL) EINVAL (no cross-hart barrier)");
    long r = syscall(__NR_membarrier, (long)MEMBARRIER_CMD_GLOBAL, 0, 0);
    if (r == 0) {
        TEST_FAIL("GLOBAL should not succeed without SMP global membarrier");
        return;
    }
    if (errno != EINVAL) {
        TEST_FAIL("expected EINVAL, got %s", strerror(errno));
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_query();
    test_global_unsupported();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
