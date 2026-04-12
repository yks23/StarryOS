/*
 * Test: pipe2(2) 对未知 flags 须返回 EINVAL（Linux 行为）
 * Target: pipe2
 * Build: riscv64-linux-musl-gcc -static -o test_pipe2_invalid_flags test_pipe2_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
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

static void test_pipe2_unknown_flags(void) {
    TEST_BEGIN("pipe2 with unknown flags returns EINVAL")
    int p[2];
    int r = pipe2(p, 0x80000000u);
    if (r == 0) {
        close(p[0]);
        close(p[1]);
    }
    TEST_ASSERT(r < 0, "expected failure, got r=%d", r);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_pipe2_unknown_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
