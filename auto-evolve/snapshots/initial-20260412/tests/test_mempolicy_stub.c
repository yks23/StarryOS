/*
 * Test: get_mempolicy 应写入 mode；stub 可能不写入
 * Target syscall: get_mempolicy
 * Build: riscv64-linux-musl-gcc -static -o test_mempolicy_stub test_mempolicy_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef MPOL_DEFAULT
#define MPOL_DEFAULT 0
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

static void test_get_mempolicy_writes_mode(void) {
    int mode;
    int rc;
    unsigned long mask[16];
    unsigned long maxnode;

    TEST_BEGIN("get_mempolicy sets mode for current thread")
    memset(&mask, 0, sizeof(mask));
    mode = -1;
    maxnode = sizeof(mask) * 8;
    rc = syscall(SYS_get_mempolicy, &mode, mask, maxnode, 0, 0);
    TEST_ASSERT(rc == 0, "get_mempolicy: %s", strerror(errno));
    TEST_ASSERT(mode != -1, "mode not written (stub?)");
    TEST_PASS();
}

int main(void) {
    test_get_mempolicy_writes_mode();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
