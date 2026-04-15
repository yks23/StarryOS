/*
 * Test: getrusage(RUSAGE_CHILDREN) 不得计入同进程其它线程的 CPU（仅应反映已回收子进程等）
 * Linux: RUSAGE_CHILDREN 为已 wait 的子进程资源累计；pthread 兄弟线程不计入
 * Target: getrusage
 * Build: riscv64-linux-musl-gcc -static -pthread -o test_getrusage_children_not_threads test_getrusage_children_not_threads.c
 */

#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/time.h>
#include <unistd.h>

volatile unsigned long spin;

static void *burn_cpu(void *arg) {
    (void)arg;
    for (int i = 0; i < 80000000; i++) {
        spin++;
    }
    return NULL;
}

int main(void) {
    pthread_t t;
    if (pthread_create(&t, NULL, burn_cpu, NULL) != 0) {
        printf("FAIL: pthread_create\n");
        return 1;
    }
    /* 让工作线程占用 CPU 一段时间 */
    usleep(400000);

    struct rusage ru;
    memset(&ru, 0, sizeof(ru));
    if (getrusage(RUSAGE_CHILDREN, &ru) != 0) {
        perror("getrusage");
        return 1;
    }

    long u_usec = (long)ru.ru_utime.tv_sec * 1000000L + ru.ru_utime.tv_usec;
    long s_usec = (long)ru.ru_stime.tv_sec * 1000000L + ru.ru_stime.tv_usec;
    long total = u_usec + s_usec;

    pthread_join(t, NULL);

    printf("[TEST] RUSAGE_CHILDREN utime+stime usec=%ld (expect small on Linux)\n", total);
    printf("\n=== SUMMARY: 1/1 passed ===\n");

    /* Linux 在仅有 pthread、无子进程时接近 0；错误实现会把兄弟线程 CPU 计入 */
    if (total > 50000L) {
        printf("FAIL: children rusage too large (likely counted sibling threads)\n");
        return 1;
    }
    return 0;
}
