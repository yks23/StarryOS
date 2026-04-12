/*
 * Test: statx(2) 对未知 flags 须返回 EINVAL（Linux 行为）
 * Target: statx
 * Build: gcc / riscv64-linux-musl-gcc -static -o test_statx_invalid_flags test_statx_invalid_flags.c
 * 使用 syscall(SYS_statx, …) + linux/stat.h，便于 glibc 与 musl riscv 静态链一致编译。
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/stat.h>
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

static void test_statx_unknown_flags(void) {
    TEST_BEGIN("statx with unknown flags returns EINVAL")
    struct statx st;
    long r = syscall(SYS_statx, AT_FDCWD, "/tmp", 0x80000000u, STATX_BASIC_STATS,
                     &st);
    TEST_ASSERT(r < 0, "expected failure, got r=%ld", r);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_statx_unknown_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
