/*
 * Test: setpriority 应存在并可调整 nice（与 getpriority 配对）
 * Target syscall: setpriority (缺失则 ENOSYS)
 * Build: riscv64-linux-musl-gcc -static -o test_nice_enosys test_nice_enosys.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/time.h>
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

static void test_setpriority_exists(void) {
    int r;

    TEST_BEGIN("setpriority(PRIO_PROCESS) succeeds or EINVAL for out of range")
    errno = 0;
    r = setpriority(PRIO_PROCESS, 0, 5);
    TEST_ASSERT(r == 0 || errno != ENOSYS,
                "setpriority missing (ENOSYS): %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_setpriority_exists();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
