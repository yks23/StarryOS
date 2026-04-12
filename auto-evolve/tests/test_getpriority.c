/*
 * Test: setpriority 后 getpriority 应返回新的 nice 值
 * Target syscall: setpriority, getpriority
 * Expected: setpriority(PRIO_PROCESS,0,5) 后 getpriority 返回 5（Linux nice 语义）
 * Build: riscv64-linux-musl-gcc -static -o test_getpriority test_getpriority.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <unistd.h>

static int failures = 0;
static int total = 0;

#define TEST_BEGIN(name)                                                       \
    do {                                                                       \
        total++;                                                               \
        printf("[TEST] %s ... ", (name));                                      \
    } while (0)

#define TEST_FAIL(fmt, ...)                                                    \
    do {                                                                       \
        printf("FAIL: " fmt "\n", ##__VA_ARGS__);                              \
        failures++;                                                            \
    } while (0)

#define TEST_PASS() printf("PASS\n")

static void test_nice_roundtrip(void) {
    TEST_BEGIN("setpriority then getpriority reflects nice");
    int target = 5;
    if (setpriority(PRIO_PROCESS, 0, target) < 0) {
        TEST_FAIL("setpriority: %s", strerror(errno));
        return;
    }
    int g = getpriority(PRIO_PROCESS, 0);
    if (g < 0 && errno != 0) {
        TEST_FAIL("getpriority: %s", strerror(errno));
        return;
    }
    if (g != target) {
        printf("FAIL: getpriority returned %d (expected %d; kernel always "
               "returns fixed value)\n",
               g, target);
        failures++;
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_nice_roundtrip();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
