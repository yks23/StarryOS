/*
 * Test: setuid 后 getuid 应反映新值
 * Target syscall: setuid, getuid
 * Build: riscv64-linux-musl-gcc -static -o test_setuid_stub test_setuid_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
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

static void test_setuid(void) {
    uid_t u;

    TEST_BEGIN("setuid(1000) then getuid is 1000")
    TEST_ASSERT(setuid(1000) == 0, "setuid: %s", strerror(errno));
    u = getuid();
    TEST_ASSERT(u == 1000, "getuid=%u after setuid (stub returns 0?)", (unsigned)u);
    TEST_PASS();
}

int main(void) {
    test_setuid();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
