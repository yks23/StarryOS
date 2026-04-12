/*
 * Test: timerfd 到期后 poll 应可读，read 应返回到期次数
 * Target syscall: timerfd_create, timerfd_settime, poll, read
 * Expected: poll 在超时内返回 POLLIN
 * Build: riscv64-linux-musl-gcc -static -o test_timerfd test_timerfd.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/timerfd.h>
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

static void test_timerfd_fires(void) {
    TEST_BEGIN("timerfd poll returns POLLIN before timeout");
    int fd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK);
    if (fd < 0) {
        TEST_FAIL("timerfd_create: %s", strerror(errno));
        return;
    }
    struct itimerspec tm;
    memset(&tm, 0, sizeof tm);
    tm.it_value.tv_sec = 0;
    tm.it_value.tv_nsec = 200 * 1000000L;
    if (timerfd_settime(fd, 0, &tm, NULL) < 0) {
        TEST_FAIL("timerfd_settime: %s", strerror(errno));
        close(fd);
        return;
    }
    struct pollfd pfd;
    pfd.fd = fd;
    pfd.events = POLLIN;
    int pr = poll(&pfd, 1, 1500);
    if (pr <= 0) {
        printf("FAIL: poll returned %d (dummy fd never becomes readable)\n", pr);
        failures++;
        close(fd);
        return;
    }
    if (!(pfd.revents & POLLIN)) {
        TEST_FAIL("poll: no POLLIN");
        close(fd);
        return;
    }
    uint64_t exp;
    ssize_t n = read(fd, &exp, sizeof exp);
    if (n != (ssize_t)sizeof exp || exp < 1u) {
        TEST_FAIL("read timerfd: n=%zd exp=%llu", n, (unsigned long long)exp);
        close(fd);
        return;
    }
    close(fd);
    TEST_PASS();
}

int main(void) {
    test_timerfd_fires();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
