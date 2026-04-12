/*
 * Test: 多线程基础并发（BLOCK_NEXT_SIGNAL_CHECK 为全局 AtomicBool 的语义问题需内核 per-thread 修复）
 * Target: 与 rt_sigreturn 路径 block_next_signal 相关（用户态无法直接调用；本测试作为并发基线）
 * Expected: 多线程原子递增无丢失（正确性基线）
 * Build: riscv64-linux-musl-gcc -static -o test_block_next_signal test_block_next_signal.c -lpthread
 */

#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

#define ITERS 100000
#define N_THREADS 4

static pthread_mutex_t mtx = PTHREAD_MUTEX_INITIALIZER;
static unsigned counter;
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

static void *inc(void *a) {
    (void)a;
    for (unsigned i = 0; i < ITERS; i++) {
        pthread_mutex_lock(&mtx);
        counter++;
        pthread_mutex_unlock(&mtx);
    }
    return NULL;
}

static void test_atomic_baseline(void) {
    TEST_BEGIN("concurrent atomic increments (baseline)");
    counter = 0u;
    pthread_t th[N_THREADS];
    for (int i = 0; i < N_THREADS; i++) {
        if (pthread_create(&th[i], NULL, inc, NULL) != 0) {
            TEST_FAIL("pthread_create");
            return;
        }
    }
    for (int i = 0; i < N_THREADS; i++) {
        pthread_join(th[i], NULL);
    }
    unsigned expect = (unsigned)N_THREADS * ITERS;
    if (counter != expect) {
        TEST_FAIL("counter %u != %u", counter, expect);
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_atomic_baseline();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
