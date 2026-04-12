/*
 * Test: 对普通文件使用终端 ioctl 应失败 ENOTTY（非静默成功）
 * Target syscall: ioctl
 * Build: riscv64-linux-musl-gcc -static -o test_ioctl_partial test_ioctl_partial.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <termios.h>
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

static void test_tcgets_regular_file(void) {
    int fd;
    struct termios t;
    int rc;

    TEST_BEGIN("TCGETS on regular file returns ENOTTY")
    fd = open("/etc/passwd", O_RDONLY);
    if (fd < 0)
        fd = open(".", O_RDONLY);
    TEST_ASSERT(fd >= 0, "open: %s", strerror(errno));
    memset(&t, 0, sizeof(t));
    rc = ioctl(fd, TCGETS, &t);
    close(fd);
    TEST_ASSERT(rc < 0, "ioctl unexpectedly succeeded");
    TEST_ASSERT(errno == ENOTTY || errno == EINVAL,
                "expected ENOTTY/EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_tcgets_regular_file();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
