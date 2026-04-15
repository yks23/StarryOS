/*
 * Test: capset 清空后 capget 不应再报告全 CAP
 * Target syscall: capget, capset
 * Build: riscv64-linux-musl-gcc -static -o test_cap_stub test_cap_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <linux/capability.h>
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

static void test_capset_followed_by_capget(void) {
    struct __user_cap_header_struct h;
    struct __user_cap_data_struct d0;
    struct __user_cap_data_struct d1;
    int rc;

    TEST_BEGIN("capset zero then capget not all-ones")
    memset(&h, 0, sizeof(h));
    h.version = _LINUX_CAPABILITY_VERSION_3;
    h.pid = 0;
    memset(&d0, 0, sizeof(d0));
    rc = syscall(SYS_capset, &h, &d0);
    if (rc < 0 && errno == EPERM) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(rc == 0, "capset: %s", strerror(errno));

    memset(&d1, 0xff, sizeof(d1));
    rc = syscall(SYS_capget, &h, &d1);
    TEST_ASSERT(rc == 0, "capget: %s", strerror(errno));
    TEST_ASSERT(d1.effective == 0 && d1.permitted == 0 && d1.inheritable == 0,
                "capabilities still max after capset zero (stub?)");

    TEST_PASS();
}

int main(void) {
    test_capset_followed_by_capget();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
