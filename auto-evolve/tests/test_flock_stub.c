/*
 * Test: flock 互斥 — 父进程持锁时子进程 LOCK_NB 应失败
 * Target syscall: flock
 * Build: riscv64-linux-musl-gcc -static -o test_flock_stub test_flock_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
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

static char template[] = "/tmp/starry_flock_XXXXXX";

static void test_flock_exclusion(void) {
    int fd;
    pid_t pid;
    int st;

    TEST_BEGIN("second flock LOCK_NB fails while first holds exclusive lock")
    fd = mkstemp(template);
    TEST_ASSERT(fd >= 0, "mkstemp: %s", strerror(errno));
    TEST_ASSERT(ftruncate(fd, 16) == 0, "ftruncate: %s", strerror(errno));
    TEST_ASSERT(flock(fd, LOCK_EX) == 0, "flock parent: %s", strerror(errno));

    pid = fork();
    TEST_ASSERT(pid >= 0, "fork: %s", strerror(errno));
    if (pid == 0) {
        int cfd = open(template, O_RDWR);
        int r;
        if (cfd < 0)
            _exit(3);
        r = flock(cfd, LOCK_EX | LOCK_NB);
        close(cfd);
        if (r == 0)
            _exit(2);
        if (r < 0 && (errno == EWOULDBLOCK || errno == EAGAIN))
            _exit(0);
        _exit(4);
    }
    waitpid(pid, &st, 0);
    TEST_ASSERT(WIFEXITED(st), "child killed");
    TEST_ASSERT(WEXITSTATUS(st) == 0,
                "child should get EWOULDBLOCK, exit=%d", WEXITSTATUS(st));
    flock(fd, LOCK_UN);
    close(fd);
    unlink(template);
    TEST_PASS();
}

int main(void) {
    test_flock_exclusion();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
