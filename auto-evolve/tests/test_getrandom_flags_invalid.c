/*
 * Test: getrandom(2) 对非法 flags 须返回 EINVAL（Linux 行为）
 * Target syscall: getrandom
 * Build: riscv64-linux-musl-gcc -static -o test_getrandom_flags_invalid test_getrandom_flags_invalid.c
 */

#define _GNU_SOURCE
#include <errno.h>
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

static void test_invalid_flags(void) {
    TEST_BEGIN("getrandom with unknown flags returns EINVAL")
    char buf[8];
    long rc = syscall(SYS_getrandom, buf, sizeof(buf), 0xdeadbeefu);
    TEST_ASSERT(rc < 0, "expected failure, got rc=%ld", rc);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_invalid_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
