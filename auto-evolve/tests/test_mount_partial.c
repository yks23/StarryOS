/*
 * Test: mount 非法 fstype 应 EINVAL，不应假成功
 * Target syscall: mount
 * Build: riscv64-linux-musl-gcc -static -o test_mount_partial test_mount_partial.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/mount.h>

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

static void test_mount_bad_type(void) {
    int rc;

    TEST_BEGIN("mount with bogus fstype fails")
    rc = mount("none", "/tmp", "__not_a_real_fs_type__", 0, NULL);
    TEST_ASSERT(rc < 0, "mount unexpectedly succeeded");
    if (errno == EPERM) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(errno == EINVAL || errno == ENODEV || errno == ENOENT,
                "unexpected errno %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_mount_bad_type();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
