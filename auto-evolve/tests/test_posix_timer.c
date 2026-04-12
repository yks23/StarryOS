/*
 * Test: POSIX 定时器 timer_settime 后应在到期时投递 SIGEV_SIGNAL 指定的信号
 * Target syscall: timer_create, timer_settime, timer_gettime (内核 mod.rs 对三者直接 Ok(0))
 * Expected: sigtimedwait 在超时内收到 SIGUSR1
 * Build: riscv64-linux-musl-gcc -static -o test_posix_timer test_posix_timer.c -lrt
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
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

static void test_timer_fires_signal(void) {
    TEST_BEGIN("POSIX timer delivers SIGUSR1 via sigtimedwait");
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = SIG_IGN;
    if (sigaction(SIGUSR1, &sa, NULL) != 0) {
        TEST_FAIL("sigaction: %s", strerror(errno));
        return;
    }

    timer_t tid;
    struct sigevent sev;
    memset(&sev, 0, sizeof sev);
    sev.sigev_notify = SIGEV_SIGNAL;
    sev.sigev_signo = SIGUSR1;

    if (timer_create(CLOCK_REALTIME, &sev, &tid) != 0) {
        TEST_FAIL("timer_create: %s", strerror(errno));
        return;
    }

    struct itimerspec its;
    memset(&its, 0, sizeof its);
    its.it_value.tv_sec = 0;
    its.it_value.tv_nsec = 150 * 1000000L;

    if (timer_settime(tid, 0, &its, NULL) != 0) {
        TEST_FAIL("timer_settime: %s", strerror(errno));
        timer_delete(tid);
        return;
    }

    sigset_t mask, oldmask;
    sigemptyset(&mask);
    sigaddset(&mask, SIGUSR1);
    if (sigprocmask(SIG_BLOCK, &mask, &oldmask) != 0) {
        TEST_FAIL("sigprocmask: %s", strerror(errno));
        timer_delete(tid);
        return;
    }

    struct timespec timeout = {.tv_sec = 2, .tv_nsec = 0};
    siginfo_t info;
    memset(&info, 0, sizeof info);

    int r = sigtimedwait(&mask, &info, &timeout);
    sigprocmask(SIG_SETMASK, &oldmask, NULL);
    timer_delete(tid);
    if (r < 0) {
        printf("FAIL: sigtimedwait: %s (POSIX timer syscalls may be no-ops)\n",
               strerror(errno));
        failures++;
        return;
    }
    if (r != SIGUSR1) {
        TEST_FAIL("unexpected signal %d", r);
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_timer_fires_signal();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
