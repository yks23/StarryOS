/*
 * Test: fadvise64 在管道/ FIFO 类不可 seek 的 fd 上应失败且 errno=ESPIPE（Linux）
 * Target syscall: fadvise64
 * Expected: 对 pipe 读端 fadvise 返回 -1，errno=ESPIPE（非 EPIPE）
 * Build: riscv64-linux-musl-gcc -static -o test_fadvise64_pipe_errno test_fadvise64_pipe_errno.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
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

#ifndef __NR_fadvise64
#if defined(__x86_64__)
#define __NR_fadvise64 221
#elif defined(__riscv) && __riscv_xlen == 64
#define __NR_fadvise64 223
#else
#define __NR_fadvise64 0
#endif
#endif

static void test_fadvise_pipe_espipe(void) {
    TEST_BEGIN("fadvise64 on pipe read fd returns ESPIPE not EPIPE")
    int p[2];
    TEST_ASSERT(pipe(p) == 0, "pipe: %s", strerror(errno));
    errno = 0;
    long rc = syscall(__NR_fadvise64, p[0], (long)0, (long)0, 0);
    close(p[0]);
    close(p[1]);
    TEST_ASSERT(rc < 0, "expected failure, got rc=%ld", rc);
    TEST_ASSERT(errno == ESPIPE, "expected ESPIPE, got errno=%d %s", errno,
                strerror(errno));
    TEST_PASS();
}

int main(void) {
#if __NR_fadvise64 == 0
    printf("[SKIP] __NR_fadvise64 unknown for this arch\n");
    return 0;
#else
    test_fadvise_pipe_espipe();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
#endif
}
