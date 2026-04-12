/*
 * shmat(2)：SHM_RND 将附着地址按 SHMLBA 向下对齐（Linux 基线；Starry 应对齐 issue-058 fix）
 * Build: gcc -Wall -o test_shmat_shm_rnd_remap_smoke test_shmat_shm_rnd_remap_smoke.c
 * 注：Linux 对 shmflg 未知高位常静默忽略，与 Starry `VALID_SHMAT_FLAGS` 的 EINVAL 策略不同，故不做跨 Linux 的「非法位」断言。
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/ipc.h>
#include <sys/shm.h>
#include <unistd.h>

#ifndef SHM_RND
#define SHM_RND 020000
#endif

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

static void test_shm_rnd_aligns_down(void) {
    TEST_BEGIN("shmat SHM_RND aligns attach address to SHMLBA")
    int id = shmget(IPC_PRIVATE, 4096, IPC_CREAT | 0600);
    TEST_ASSERT(id >= 0, "shmget: %s", strerror(errno));
    void *p = shmat(id, (void *)0x12340UL, SHM_RND);
    TEST_ASSERT(p != (void *)-1, "shmat: %s", strerror(errno));
    unsigned long a = (unsigned long)p;
    TEST_ASSERT((a & 0xfffUL) == 0, "expected page-aligned, got %#lx", a);
    shmdt(p);
    shmctl(id, IPC_RMID, NULL);
    TEST_PASS();
}

int main(void) {
    test_shm_rnd_aligns_down();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
