/*
 * Test: sysinfo.totalram 应反映物理内存量级
 * Target syscall: sysinfo
 * Build: riscv64-linux-musl-gcc -static -o test_sysinfo_partial test_sysinfo_partial.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/sysinfo.h>

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

static void test_totalram_nonzero(void) {
    struct sysinfo si;

    TEST_BEGIN("sysinfo.totalram is non-zero on real system")
    memset(&si, 0, sizeof(si));
    TEST_ASSERT(sysinfo(&si) == 0, "sysinfo: %s", strerror(errno));
    TEST_ASSERT(si.totalram > 0, "totalram==0 (partial sysinfo stub?)");
    TEST_PASS();
}

int main(void) {
    test_totalram_nonzero();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
