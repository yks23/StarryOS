/*
 * Test: 同一匿名可写页被大量 fork 子进程共享（仅读）时，CoW 帧引用计数 u8 上限可导致后续 fork 失败
 * Target: mm/aspace/backend/cow.rs FrameRefCnt(u8)
 * Expected: 至少 256 个存活子进程仍可持续 fork（Linux）；u8 溢出时更早失败并返回错误
 * Build: riscv64-linux-musl-gcc -static -o test_fork_cow_refcount test_fork_cow_refcount.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

#define TARGET_ALIVE 260

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

static void test_many_forks_shared_page(void) {
    TEST_BEGIN("fork many children holding one shared COW anon page");
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
    *(volatile char *)p = 1;

    pid_t ch[TARGET_ALIVE];
    int n = 0;
    for (; n < TARGET_ALIVE; n++) {
        pid_t pid = fork();
        if (pid < 0) {
            for (int i = 0; i < n; i++) {
                kill(ch[i], SIGKILL);
                waitpid(ch[i], NULL, 0);
            }
            printf("FAIL: fork failed at n=%d errno=%d (%s)\n", n, errno,
                   strerror(errno));
            failures++;
            break;
        }
        if (pid == 0) {
            (void)*(volatile char *)p;
            for (;;)
                pause();
        }
        ch[n] = pid;
    }

    for (int i = 0; i < n; i++) {
        kill(ch[i], SIGKILL);
        waitpid(ch[i], NULL, 0);
    }
    munmap(p, (size_t)ps);

    if (failures) {
        return;
    }
    if (n < 256) {
        printf("FAIL: stopped at %d children (<256; possible u8 refcount cap)\n",
               n);
        failures++;
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_many_forks_shared_page();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
