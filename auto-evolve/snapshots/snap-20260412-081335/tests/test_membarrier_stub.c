/*
 * Test: membarrier CMD_QUERY 应成功（完整 SMP 语义需另行验证）
 * Target syscall: membarrier
 * Build: riscv64-linux-musl-gcc -static -o test_membarrier_stub test_membarrier_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef MEMBARRIER_CMD_QUERY
#define MEMBARRIER_CMD_QUERY 0
#endif

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

static void test_membarrier_query(void) {
    long rc;

    TEST_BEGIN("membarrier(MEMBARRIER_CMD_QUERY) succeeds")
    rc = syscall(SYS_membarrier, MEMBARRIER_CMD_QUERY, 0, 0);
    TEST_ASSERT(rc >= 0, "membarrier: %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_membarrier_query();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
