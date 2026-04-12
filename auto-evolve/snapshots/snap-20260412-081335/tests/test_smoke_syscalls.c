/*
 * Test: 基础系统调用冒烟（用于 CI/回归基线；改进点：仓库缺少自动化 syscall 套件）
 * Target syscall: getpid, write, read
 * Expected: 全部成功
 * Build: riscv64-linux-musl-gcc -static -o test_smoke_syscalls test_smoke_syscalls.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
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

static void test_getpid(void) {
    TEST_BEGIN("getpid returns positive");
    pid_t p = getpid();
    if (p <= 0) {
        TEST_FAIL("getpid=%d", (int)p);
        return;
    }
    TEST_PASS();
}

static void test_write_read_pipe(void) {
    TEST_BEGIN("pipe write/read roundtrip");
    int fd[2];
    if (pipe(fd) < 0) {
        TEST_FAIL("pipe: %s", strerror(errno));
        return;
    }
    const char *msg = "ok";
    ssize_t nw = write(fd[1], msg, strlen(msg));
    if (nw != (ssize_t)strlen(msg)) {
        TEST_FAIL("write");
        close(fd[0]);
        close(fd[1]);
        return;
    }
    char buf[8];
    memset(buf, 0, sizeof buf);
    ssize_t nr = read(fd[0], buf, sizeof buf - 1);
    close(fd[0]);
    close(fd[1]);
    if (nr < 0) {
        TEST_FAIL("read: %s", strerror(errno));
        return;
    }
    if (strcmp(buf, "ok") != 0) {
        TEST_FAIL("data mismatch");
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_getpid();
    test_write_read_pipe();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
