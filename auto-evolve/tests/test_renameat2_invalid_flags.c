/*
 * Test: renameat2(2) 对未知 flags 须返回 EINVAL（Linux 行为）
 * Target: renameat2
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_renameat2_invalid_flags test_renameat2_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef __NR_renameat2
#if defined(__x86_64__)
#define __NR_renameat2 316
#elif defined(__riscv) && __riscv_xlen == 64
#define __NR_renameat2 56
#elif defined(__aarch64__)
#define __NR_renameat2 276
#else
#define __NR_renameat2 316
#endif
#endif

static long do_renameat2(int odfd, const char *op, int ndfd, const char *np, unsigned int f) {
    return syscall(__NR_renameat2, odfd, op, ndfd, np, f);
}

static int failures;
static int total;

#define TEST_BEGIN(name)                                                       \
    do {                                                                       \
        total++;                                                               \
        printf("[TEST] %s ... ", (name));

#define TEST_ASSERT(cond, fmt, ...)                                            \
    if (!(cond)) {                                                             \
        printf("FAIL: " fmt "\n", ##__VA_ARGS__);                             \
        failures++;                                                            \
        break;                                                                 \
    }

#define TEST_PASS()                                                            \
    printf("PASS\n");                                                          \
    } while (0)

static void test_unknown_flags(void) {
    TEST_BEGIN("renameat2 with unknown flags returns EINVAL")
    char dir[] = "/tmp/rn2XXXXXX";
    TEST_ASSERT(mkdtemp(dir) != NULL, "mkdtemp");
    char a[256], b[256];
    snprintf(a, sizeof(a), "%s/a", dir);
    snprintf(b, sizeof(b), "%s/b", dir);
    FILE *fa = fopen(a, "w");
    FILE *fb = fopen(b, "w");
    TEST_ASSERT(fa && fb, "fopen");
    fclose(fa);
    fclose(fb);

    long r = do_renameat2(AT_FDCWD, a, AT_FDCWD, b, 0x80000000u);
    int e = errno;

    TEST_ASSERT(r < 0, "expected failure, got r=%ld", r);
    TEST_ASSERT(e == EINVAL, "expected EINVAL, got %s", strerror(e));
    TEST_ASSERT(access(a, F_OK) == 0 && access(b, F_OK) == 0,
                "both files must still exist (no rename)");

    unlink(a);
    unlink(b);
    rmdir(dir);
    TEST_PASS();
}

int main(void) {
    test_unknown_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
