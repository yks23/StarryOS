/*
 * Test: splice(2) 对非法 SPLICE_F_* 组合须返回 EINVAL，不得忽略 flags
 * Target syscall: splice
 * Expected: Linux 对未知/非法 flags 返回 -1 且 errno=EINVAL
 * Build: riscv64-linux-musl-gcc -static -o test_splice_flags_invalid test_splice_flags_invalid.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
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

static void test_splice_invalid_flags(void) {
    TEST_BEGIN("splice with invalid flags returns EINVAL")
    int p[2];
    TEST_ASSERT(pipe(p) == 0, "pipe: %s", strerror(errno));
    long rc = syscall(SYS_splice, p[0], NULL, p[1], NULL, (size_t)1, 0xdeadbeefu);
    close(p[0]);
    close(p[1]);
    TEST_ASSERT(rc < 0, "expected failure, got rc=%ld", rc);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_splice_invalid_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
