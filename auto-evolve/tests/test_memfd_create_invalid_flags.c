/*
 * Test: memfd_create(2) 对未知 flags 须返回 EINVAL（Linux 行为）
 * Target: memfd_create
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_memfd_create_invalid_flags test_memfd_create_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef __NR_memfd_create
#if defined(__x86_64__)
#define __NR_memfd_create 319
#elif defined(__riscv) && __riscv_xlen == 64
#define __NR_memfd_create 279
#elif defined(__aarch64__)
#define __NR_memfd_create 279
#else
#define __NR_memfd_create 0
#endif
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

static void test_unknown_flags(void) {
#if __NR_memfd_create == 0
    printf("[SKIP] __NR_memfd_create unknown\n");
    total--;
    return;
#endif
    TEST_BEGIN("memfd_create with unknown flags returns EINVAL")
    int fd = syscall(__NR_memfd_create, "x", 0x80000000u);
    if (fd >= 0) {
        close(fd);
    }
    TEST_ASSERT(fd < 0, "expected failure, got fd=%d", fd);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_unknown_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
