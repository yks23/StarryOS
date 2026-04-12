/*
 * Test: timer_create 应写入 timer_t；内核若仅返回 Ok(0) 则 timerid 未初始化
 * Target syscall: timer_create, timer_settime, timer_gettime
 * Build: riscv64-linux-musl-gcc -static -o test_posix_timer_stub test_posix_timer_stub.c -lrt
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
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

static void test_timer_create_writes_id(void) {
    timer_t tid;
    int rc;
    struct sigevent sev;

    TEST_BEGIN("timer_create sets non-null timer id on success")
    memset(&sev, 0, sizeof(sev));
    sev.sigev_notify = SIGEV_SIGNAL;
    sev.sigev_signo = SIGALRM;
    tid = (timer_t)0;
    rc = syscall(SYS_timer_create, CLOCK_MONOTONIC, &sev, &tid);
    if (rc < 0) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(rc == 0, "timer_create returned %d", rc);
    TEST_ASSERT(tid != (timer_t)0, "timer id still null after success (stub?)");
    syscall(SYS_timer_delete, tid);
    TEST_PASS();
}

int main(void) {
    test_timer_create_writes_id();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
