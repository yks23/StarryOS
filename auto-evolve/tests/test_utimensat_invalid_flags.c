/*
 * Test: utimensat(2) 对非法 AT_* flags 须返回 EINVAL（Linux 行为）
 * Target: utimensat
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_utimensat_invalid_flags test_utimensat_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#ifndef UTIME_NOW
#define UTIME_NOW ((1l << 30) - 1l)
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

static void test_bad_flags(void) {
    TEST_BEGIN("utimensat with unknown flags returns EINVAL")
    char dir[] = "/tmp/utaXXXXXX";
    TEST_ASSERT(mkdtemp(dir) != NULL, "mkdtemp");
    char path[256];
    snprintf(path, sizeof(path), "%s/f", dir);
    FILE *f = fopen(path, "w");
    TEST_ASSERT(f != NULL, "fopen");
    fclose(f);

    struct timespec ts[2];
    ts[0].tv_sec = 0;
    ts[0].tv_nsec = UTIME_NOW;
    ts[1].tv_sec = 0;
    ts[1].tv_nsec = UTIME_NOW;

    struct stat st_before;
    TEST_ASSERT(stat(path, &st_before) == 0, "stat");

    int r = utimensat(AT_FDCWD, path, ts, 0x80000000u);
    int e = errno;

    struct stat st_after;
    TEST_ASSERT(stat(path, &st_after) == 0, "stat after");

    unlink(path);
    rmdir(dir);

    TEST_ASSERT(r < 0, "expected failure, got r=%d", r);
    TEST_ASSERT(e == EINVAL, "expected EINVAL, got %s", strerror(e));
    TEST_ASSERT(st_after.st_mtime == st_before.st_mtime,
                "mtime must not change on EINVAL");
    TEST_PASS();
}

int main(void) {
    test_bad_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
