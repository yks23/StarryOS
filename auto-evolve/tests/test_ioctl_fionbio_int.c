/*
 * Test: ioctl(FIONBIO) 须按 Linux 语义读取用户态 int，任意非零均表示 O_NONBLOCK
 * Target: ioctl + FIONBIO
 * Linux: *(int *)arg != 0 即非阻塞；不限制值仅为 0/1，且按整型解释（非单字节）
 * Build: riscv64-linux-musl-gcc -static -o test_ioctl_fionbio_int test_ioctl_fionbio_int.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
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

static void test_fionbio_value_2(void) {
    TEST_BEGIN("FIONBIO with *arg=2 succeeds (non-zero int)")
    int p[2];
    TEST_ASSERT(pipe(p) == 0, "pipe: %s", strerror(errno));
    int v = 2;
    int r = ioctl(p[0], FIONBIO, &v);
    close(p[0]);
    close(p[1]);
    TEST_ASSERT(r == 0, "expected r=0, got %d errno=%s", r, strerror(errno));
    TEST_PASS();
}

static void test_fionbio_value_256(void) {
    TEST_BEGIN("FIONBIO with *arg=256 succeeds (non-zero, low byte 0 on LE)")
    int p[2];
    TEST_ASSERT(pipe(p) == 0, "pipe: %s", strerror(errno));
    int v = 256;
    int r = ioctl(p[0], FIONBIO, &v);
    close(p[0]);
    close(p[1]);
    TEST_ASSERT(r == 0, "expected r=0, got %d errno=%s", r, strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_fionbio_value_2();
    test_fionbio_value_256();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
