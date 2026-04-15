/*
 * Test: timerfd 设置一次性定时后 poll 应在超时内收到 POLLIN
 * Target syscall: timerfd_create, timerfd_settime, poll
 * Expected: Linux 上定时到期后 poll 返回 POLLIN，read 可读到期次数
 * Build: riscv64-linux-musl-gcc -static -o test_dummy_timerfd test_dummy_timerfd.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/timerfd.h>
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

static void test_timerfd_fires(void) {
    int fd;
    struct itimerspec its;
    struct pollfd pfd;
    int pr;
    uint64_t exp;
    ssize_t n;

    TEST_BEGIN("timerfd interval then poll sees POLLIN")
    fd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK | TFD_CLOEXEC);
    TEST_ASSERT(fd >= 0, "timerfd_create: %s", strerror(errno));

    memset(&its, 0, sizeof(its));
    its.it_value.tv_sec = 0;
    its.it_value.tv_nsec = 200000000;
    its.it_interval.tv_sec = 0;
    its.it_interval.tv_nsec = 0;

    TEST_ASSERT(timerfd_settime(fd, 0, &its, NULL) == 0, "timerfd_settime: %s",
                strerror(errno));

    pfd.fd = fd;
    pfd.events = POLLIN;
    pr = poll(&pfd, 1, 800);
    TEST_ASSERT(pr > 0, "poll expected event within 800ms, got %d errno=%s", pr,
                strerror(errno));
    TEST_ASSERT((pfd.revents & POLLIN) != 0, "POLLIN not set");

    n = read(fd, &exp, sizeof(exp));
    TEST_ASSERT(n == (ssize_t)sizeof(exp), "read timerfd: %s", strerror(errno));

    close(fd);
    TEST_PASS();
}

int main(void) {
    test_timerfd_fires();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
