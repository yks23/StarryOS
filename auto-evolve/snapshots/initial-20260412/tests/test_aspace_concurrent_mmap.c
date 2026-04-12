/*
 * Test: 多线程并发 mmap/munmap 应力（暴露 AddrSpace 全局 Mutex 时的争用；功能上应全部成功）
 * Target: mmap/munmap 路径与 ProcessData.aspace 锁
 * Expected: 多线程并发映射不同区域无死锁、无失败（正确性）
 * Build: riscv64-linux-musl-gcc -static -o test_aspace_concurrent_mmap test_aspace_concurrent_mmap.c -lpthread
 */

#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#define N_THREADS 8
#define ROUNDS 32
#define CHUNK (64 * 1024)

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

static void *worker(void *arg) {
    unsigned id = (unsigned)(uintptr_t)arg;
    for (int r = 0; r < ROUNDS; r++) {
        uintptr_t hint = (id + 1) * 0x1000000u + (unsigned)r * 0x10000u;
        void *p =
            mmap((void *)hint, CHUNK, PROT_READ | PROT_WRITE,
                 MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (p == MAP_FAILED) {
            /* 提示地址冲突时回退 */
            p = mmap(NULL, CHUNK, PROT_READ | PROT_WRITE,
                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        }
        if (p == MAP_FAILED) {
            return (void *)(uintptr_t)errno;
        }
        memset(p, (int)(id + r), CHUNK);
        if (munmap(p, CHUNK) < 0) {
            return (void *)(uintptr_t)errno;
        }
    }
    return NULL;
}

static void test_concurrent_mmap(void) {
    TEST_BEGIN("concurrent mmap/munmap from pthreads");
    pthread_t th[N_THREADS];
    for (unsigned i = 0; i < N_THREADS; i++) {
        if (pthread_create(&th[i], NULL, worker, (void *)(uintptr_t)i) != 0) {
            TEST_FAIL("pthread_create");
            return;
        }
    }
    for (unsigned i = 0; i < N_THREADS; i++) {
        void *ret = NULL;
        pthread_join(th[i], &ret);
        if (ret != NULL) {
            TEST_FAIL("worker %u failed errno=%u", i, (unsigned)(uintptr_t)ret);
            return;
        }
    }
    TEST_PASS();
}

int main(void) {
    test_concurrent_mmap();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
