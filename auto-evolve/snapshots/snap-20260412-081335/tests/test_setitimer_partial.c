/*
 * Test: setitimer(ITIMER_VIRTUAL) 可被查询；精度问题需结合内核抢占
 * Target syscall: setitimer, getitimer
 * Build: riscv64-linux-musl-gcc -static -o test_setitimer_partial test_setitimer_partial.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/time.h>

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

static void test_itimer_virtual_roundtrip(void) {
    struct itimerval n, o, g;

    TEST_BEGIN("setitimer ITIMER_VIRTUAL roundtrip via getitimer")
    memset(&n, 0, sizeof(n));
    n.it_value.tv_sec = 10;
    TEST_ASSERT(setitimer(ITIMER_VIRTUAL, &n, &o) == 0, "setitimer: %s",
                strerror(errno));
    memset(&g, 0, sizeof(g));
    TEST_ASSERT(getitimer(ITIMER_VIRTUAL, &g) == 0, "getitimer: %s",
                strerror(errno));
    TEST_ASSERT(g.it_value.tv_sec > 0 || g.it_value.tv_usec > 0,
                "virtual timer not armed (partial impl?)");
    memset(&n, 0, sizeof(n));
    setitimer(ITIMER_VIRTUAL, &n, NULL);
    TEST_PASS();
}

int main(void) {
    test_itimer_virtual_roundtrip();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
