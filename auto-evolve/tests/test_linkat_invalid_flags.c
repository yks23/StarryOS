/*
 * Test: linkat(2) 对非法 AT_* flags 须返回 EINVAL（Linux 行为）
 * Target: linkat
 * Build: riscv64-linux-musl-gcc -static -o test_linkat_invalid_flags test_linkat_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
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

static void test_unknown_at_flags(void) {
    TEST_BEGIN("linkat with unknown AT_* flags returns EINVAL")
    char dir[] = "/tmp/latXXXXXX";
    if (!mkdtemp(dir)) {
        printf("FAIL: mkdtemp\n");
        failures++;
        total--;
        return;
    }
    char src[256], dst[256];
    snprintf(src, sizeof(src), "%s/a", dir);
    snprintf(dst, sizeof(dst), "%s/b", dir);
    FILE *f = fopen(src, "w");
    TEST_ASSERT(f != NULL, "fopen src");
    fclose(f);

    int r = linkat(AT_FDCWD, src, AT_FDCWD, dst, 0x80000000u);
    if (r == 0) {
        unlink(dst);
    }
    unlink(src);
    rmdir(dir);
    TEST_ASSERT(r < 0, "expected failure, got r=%d", r);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_unknown_at_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
