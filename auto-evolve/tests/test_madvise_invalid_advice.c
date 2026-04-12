/*
 * Test: madvise(2) 对未知 advice 须返回 EINVAL（Linux 行为）
 * Target: madvise
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_madvise_invalid_advice test_madvise_invalid_advice.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
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

static void test_unknown_advice(void) {
    TEST_BEGIN("madvise with unknown advice returns EINVAL")
    void *p =
        mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    TEST_ASSERT(p != MAP_FAILED, "mmap: %s", strerror(errno));
    int r = madvise(p, 4096, (int)0xdeadbeef);
    munmap(p, 4096);
    TEST_ASSERT(r < 0, "expected failure, got r=%d", r);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_unknown_advice();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
