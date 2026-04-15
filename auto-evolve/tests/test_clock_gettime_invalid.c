/*
 * Test: clock_gettime 对无效 clockid 须失败（EINVAL），不得静默返回「墙钟」时间
 * Target syscall: clock_gettime
 * Expected: Linux 对未知 clock id 返回 -1 且 errno=EINVAL
 * Build: riscv64-linux-musl-gcc -static -o test_clock_gettime_invalid test_clock_gettime_invalid.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
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

static void test_invalid_clock_id(void) {
    TEST_BEGIN("clock_gettime invalid clock id returns EINVAL")
    struct timespec ts;
    memset(&ts, 0, sizeof(ts));
    long rc = syscall(SYS_clock_gettime, (int)0x7fffffff, &ts);
    TEST_ASSERT(rc < 0, "expected failure, got rc=%ld", rc);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_invalid_clock_id();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
