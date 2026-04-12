/*
 * Test: membarrier 查询与全局屏障调用应成功；语义上需 CPU 级屏障（本测试仅验证接口可用）
 * Target syscall: membarrier
 * Expected: MEMBARRIER_CMD_QUERY 返回非负掩码；MEMBARRIER_CMD_GLOBAL 返回 0
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
    TEST_PASS();
}

static void test_global(void) {
    TEST_BEGIN("membarrier(MEMBARRIER_CMD_GLOBAL)");
    long r = syscall(__NR_membarrier, (long)MEMBARRIER_CMD_GLOBAL, 0, 0);
    if (r < 0) {
        TEST_FAIL("membarrier GLOBAL: %s", strerror(errno));
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_query();
    test_global();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
