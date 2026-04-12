/*
 * Test: fcntl F_SETLK 非阻塞写锁 — 第二 fd 应失败
 * Target syscall: fcntl
 * Build: riscv64-linux-musl-gcc -static -o test_fcntl_lock_stub test_fcntl_lock_stub.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
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

static char template[] = "/tmp/starry_fcntl_XXXXXX";

static void test_fcntl_setlk(void) {
    int fd;
    pid_t pid;
    int st;

    TEST_BEGIN("fcntl F_SETLK second writer fails with EAGAIN/EACCES")
    fd = mkstemp(template);
    TEST_ASSERT(fd >= 0, "mkstemp: %s", strerror(errno));
    TEST_ASSERT(ftruncate(fd, 16) == 0, "ftruncate: %s", strerror(errno));

    struct flock fl0;
    memset(&fl0, 0, sizeof(fl0));
    fl0.l_type = F_WRLCK;
    fl0.l_whence = SEEK_SET;
    fl0.l_start = 0;
    fl0.l_len = 0;
    TEST_ASSERT(fcntl(fd, F_SETLK, &fl0) == 0, "F_SETLK parent: %s",
                strerror(errno));

    pid = fork();
    TEST_ASSERT(pid >= 0, "fork: %s", strerror(errno));
    if (pid == 0) {
        int cfd = open(template, O_RDWR);
        struct flock fl1;
        int r;
        if (cfd < 0)
            _exit(3);
        memset(&fl1, 0, sizeof(fl1));
        fl1.l_type = F_WRLCK;
        fl1.l_whence = SEEK_SET;
        fl1.l_start = 0;
        fl1.l_len = 0;
        r = fcntl(cfd, F_SETLK, &fl1);
        close(cfd);
        if (r == 0)
            _exit(2);
        if (r < 0 && (errno == EAGAIN || errno == EACCES))
            _exit(0);
        _exit(5);
    }
    waitpid(pid, &st, 0);
    TEST_ASSERT(WIFEXITED(st), "child killed");
    TEST_ASSERT(WEXITSTATUS(st) == 0,
                "child should fail lock, exit=%d", WEXITSTATUS(st));
    close(fd);
    unlink(template);
    TEST_PASS();
}

int main(void) {
    test_fcntl_setlk();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
