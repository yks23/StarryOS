/*
 * Test: prctl(PR_SET_SECCOMP, 非法 mode) 须失败（EINVAL），不得静默成功
 * Target syscall: prctl
 * Expected: Linux 对无效 SECCOMP mode 返回 -1 EINVAL
 * Build: riscv64-linux-musl-gcc -static -o test_prctl_seccomp_stub test_prctl_seccomp_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <linux/prctl.h>
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

static void test_prctl_set_seccomp_invalid_mode(void) {
    TEST_BEGIN("PR_SET_SECCOMP with invalid mode returns EINVAL")
    long rc = syscall(SYS_prctl, PR_SET_SECCOMP, 999999u, 0, 0, 0);
    TEST_ASSERT(rc < 0, "expected failure, got rc=%ld", rc);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_prctl_set_seccomp_invalid_mode();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
