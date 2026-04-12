/*
 * Test: faccessat2(2) 对非法 AT_* flags 须返回 EINVAL（Linux 行为）
 * Target: faccessat2
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_faccessat2_invalid_flags test_faccessat2_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef __NR_faccessat2
#if defined(__x86_64__)
#define __NR_faccessat2 334
#elif defined(__riscv) && __riscv_xlen == 64
#define __NR_faccessat2 48
#elif defined(__aarch64__)
#define __NR_faccessat2 48
#else
#define __NR_faccessat2 334
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
    TEST_BEGIN("faccessat2 with unknown flags returns EINVAL")
    char dir[] = "/tmp/fa2XXXXXX";
    TEST_ASSERT(mkdtemp(dir) != NULL, "mkdtemp");
    char path[256];
    snprintf(path, sizeof(path), "%s/f", dir);
    FILE *f = fopen(path, "w");
    TEST_ASSERT(f != NULL, "fopen");
    fclose(f);

    long r = syscall(__NR_faccessat2, AT_FDCWD, path, R_OK, 0x80000000u);
    int e = errno;

    TEST_ASSERT(access(path, F_OK) == 0, "file should exist");

    unlink(path);
    rmdir(dir);

    TEST_ASSERT(r < 0, "expected failure, got r=%ld", r);
    TEST_ASSERT(e == EINVAL, "expected EINVAL, got %s", strerror(e));
    TEST_PASS();
}

int main(void) {
    test_unknown_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
