/*
 * Test: getresuid/getresgid 应存在（sudo 等依赖三组 id）
 * Target syscall: getresuid, getresgid
 * Build: riscv64-linux-musl-gcc -static -o test_getresuid_enosys test_getresuid_enosys.c
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

static void test_getresuid(void) {
    unsigned ruid, euid, suid;
    int rc;

    TEST_BEGIN("getresuid syscall exists (not ENOSYS)")
    ruid = euid = suid = (unsigned)-1;
    rc = syscall(SYS_getresuid, &ruid, &euid, &suid);
    TEST_ASSERT(rc == 0, "getresuid: %s", strerror(errno));
    TEST_PASS();
}

static void test_getresgid(void) {
    unsigned rgid, egid, sgid;
    int rc;

    TEST_BEGIN("getresgid syscall exists (not ENOSYS)")
    rgid = egid = sgid = (unsigned)-1;
    rc = syscall(SYS_getresgid, &rgid, &egid, &sgid);
    TEST_ASSERT(rc == 0, "getresgid: %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_getresuid();
    test_getresgid();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
