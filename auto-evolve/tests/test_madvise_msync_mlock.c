/*
 * Test: madvise(MADV_DONTNEED) 后匿名页应可丢弃；本测试通过写入后 DONTNEED 再读观察是否仍保留旧数据（全为 noop 时常仍保留）
 * Target syscall: madvise, msync, mlock
 * Expected: Linux 上 DONTNEED 后读可能为 0；若始终保留 0x5A 则 madvise 未生效
 * Build: riscv64-linux-musl-gcc -static -o test_madvise_msync_mlock test_madvise_msync_mlock.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#ifndef MADV_DONTNEED
#define MADV_DONTNEED 4
#endif

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

static void test_madvise_dontneed(void) {
    TEST_BEGIN("madvise(MADV_DONTNEED) drops anonymous page content");
    long ps = sysconf(_SC_PAGESIZE);
    if (ps < 1) {
        TEST_FAIL("sysconf");
        return;
    }
    void *p =
        mmap(NULL, (size_t)ps, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS,
             -1, 0);
    if (p == MAP_FAILED) {
        TEST_FAIL("mmap");
        return;
    }
    memset(p, 0x5A, (size_t)ps);
    if (madvise(p, (size_t)ps, MADV_DONTNEED) != 0) {
        TEST_FAIL("madvise: %s", strerror(errno));
        munmap(p, (size_t)ps);
        return;
    }
    volatile unsigned char *vp = (volatile unsigned char *)p;
    unsigned char v = *vp;
    /*
     * Linux: page may be zero-filled or unspecified after DONTNEED; a kernel
     * that never discards pages will still read 0x5A.
     */
    if (v == 0x5A) {
        printf("FAIL: byte still 0x5A after MADV_DONTNEED (likely no-op "
               "madvise)\n");
        failures++;
        munmap(p, (size_t)ps);
        return;
    }
    munmap(p, (size_t)ps);
    TEST_PASS();
}

static void test_mlock_return_ok(void) {
    TEST_BEGIN("mlock on anonymous mapping returns success");
    long ps = sysconf(_SC_PAGESIZE);
    void *p =
        mmap(NULL, (size_t)ps, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS,
             -1, 0);
    if (p == MAP_FAILED) {
        TEST_FAIL("mmap");
        return;
    }
    memset(p, 1, (size_t)ps);
    if (mlock(p, (size_t)ps) != 0) {
        TEST_FAIL("mlock: %s", strerror(errno));
        munmap(p, (size_t)ps);
        return;
    }
    munlock(p, (size_t)ps);
    munmap(p, (size_t)ps);
    TEST_PASS();
}

int main(void) {
    test_madvise_dontneed();
    test_mlock_return_ok();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
