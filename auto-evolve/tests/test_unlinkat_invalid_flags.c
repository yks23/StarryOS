/*
 * Test: unlinkat(2) 对非法 AT_* flags 须返回 EINVAL，且不得删除目标（Linux 行为）
 * Target: unlinkat
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_unlinkat_invalid_flags test_unlinkat_invalid_flags.c
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

static void test_bad_flags_einval_and_file_remains(void) {
    TEST_BEGIN("unlinkat with unknown flags returns EINVAL; file not removed")
    char dir[] = "/tmp/uatXXXXXX";
    TEST_ASSERT(mkdtemp(dir) != NULL, "mkdtemp");
    char path[256];
    snprintf(path, sizeof(path), "%s/f", dir);
    FILE *f = fopen(path, "w");
    TEST_ASSERT(f != NULL, "fopen");
    fclose(f);

    int r = unlinkat(AT_FDCWD, path, 0x80000000u);
    int e = errno;

    TEST_ASSERT(r < 0, "expected failure, got r=%d", r);
    TEST_ASSERT(e == EINVAL, "expected EINVAL, got %s", strerror(e));
    TEST_ASSERT(access(path, F_OK) == 0, "file should still exist after failed unlinkat");

    unlink(path);
    rmdir(dir);
    TEST_PASS();
}

int main(void) {
    test_bad_flags_einval_and_file_remains();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
