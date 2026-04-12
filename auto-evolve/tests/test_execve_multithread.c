/*
 * Test: 多线程进程中 execve：Linux 成功替换映像；Starry 返回错误则子进程 exit!=0
 * Target syscall: execve, clone/pthread
 * Build: riscv64-linux-musl-gcc -static -pthread -o test_execve_multithread test_execve_multithread.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
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

static const char *find_true(void) {
    static const char *c[] = {"/bin/true", "/usr/bin/true"};
    size_t i;

    for (i = 0; i < sizeof(c) / sizeof(c[0]); i++) {
        if (access(c[i], X_OK) == 0)
            return c[i];
    }
    return NULL;
}

static void *block(void *x) {
    (void)x;
    for (;;)
        pause();
    return NULL;
}

static void test_execve_mt(void) {
    const char *path;
    pid_t pid;
    int st;

    TEST_BEGIN("execve in multithreaded process succeeds on Linux")
    path = find_true();
    TEST_ASSERT(path != NULL, "no /bin/true for test");

    pid = fork();
    TEST_ASSERT(pid >= 0, "fork: %s", strerror(errno));
    if (pid == 0) {
        pthread_t t;
        if (pthread_create(&t, NULL, block, NULL) != 0)
            _exit(2);
        {
            char *argv[] = {(char *)path, NULL};
            char *envp[] = {NULL};
            execve(path, argv, envp);
        }
        perror("execve");
        _exit(1);
    }

    TEST_ASSERT(waitpid(pid, &st, 0) == pid, "waitpid: %s", strerror(errno));
    TEST_ASSERT(WIFEXITED(st), "child status=%d", st);
    TEST_ASSERT(WEXITSTATUS(st) == 0,
                "execve failed in MT process (exit=%d, Starry returns error?)",
                WEXITSTATUS(st));
    TEST_PASS();
}

int main(void) {
    test_execve_mt();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
