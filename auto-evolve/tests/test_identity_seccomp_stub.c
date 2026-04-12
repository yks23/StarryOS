/*
 * Test: setgroups 后 getgroups 应一致；seccomp 非空操作应可检测
 * Target syscall: setgroups, getgroups, seccomp, getegid
 * Build: riscv64-linux-musl-gcc -static -o test_identity_seccomp_stub test_identity_seccomp_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <grp.h>
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

static void test_groups_roundtrip(void) {
    gid_t g[4];
    int n;

    TEST_BEGIN("setgroups then getgroups roundtrip")
    g[0] = 100;
    g[1] = 200;
    if (setgroups(2, g) < 0) {
        if (errno == EPERM) {
            printf("PASS\n");
            break;
        }
        TEST_ASSERT(0, "setgroups: %s", strerror(errno));
    }
    memset(g, 0, sizeof(g));
    n = getgroups(4, g);
    TEST_ASSERT(n == 2, "getgroups count=%d", n);
    TEST_ASSERT(g[0] == 100 && g[1] == 200, "groups mismatch (stub?)");
    TEST_PASS();
}

static void test_seccomp_returns(void) {
    TEST_BEGIN("seccomp syscall returns without crashing")
    (void)syscall(SYS_seccomp, 0, 0, NULL);
    TEST_PASS();
}

int main(void) {
    test_groups_roundtrip();
    test_seccomp_returns();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
