/*
 * Test: 多线程互斥锁应力（与跨进程 shared futex 全局表争用相关；本测试验证用户态锁路径稳定）
 * Target: futex 相关内核路径（进程内 pthread 使用 futex）
 * Expected: 大量加锁无死锁、计数正确
 * Build: riscv64-linux-musl-gcc -static -o test_futex_shared_stress test_futex_shared_stress.c -lpthread
 */

#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

#define N_THREADS 16
#define ITERS 5000

static pthread_mutex_t m = PTHREAD_MUTEX_INITIALIZER;
static unsigned long acc;
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

static void *worker(void *x) {
    (void)x;
    for (int i = 0; i < ITERS; i++) {
        pthread_mutex_lock(&m);
        acc++;
        pthread_mutex_unlock(&m);
    }
    return NULL;
}

static void test_mutex_stress(void) {
    TEST_BEGIN("pthread mutex stress (uses futex under musl)");
    acc = 0;
    pthread_t th[N_THREADS];
    for (int i = 0; i < N_THREADS; i++) {
        if (pthread_create(&th[i], NULL, worker, NULL) != 0) {
            TEST_FAIL("pthread_create");
            return;
        }
    }
    for (int i = 0; i < N_THREADS; i++) {
        pthread_join(th[i], NULL);
    }
    unsigned long expect = (unsigned long)N_THREADS * (unsigned long)ITERS;
    if (acc != expect) {
        TEST_FAIL("acc=%lu expect=%lu", acc, expect);
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_mutex_stress();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
