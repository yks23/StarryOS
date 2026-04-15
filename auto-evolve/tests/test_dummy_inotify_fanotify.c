/*
 * Test: inotify_init1 / fanotify_init 返回真实 anon inode，而非 Starry dummy
 * Target syscall: inotify_init1, fanotify_init
 * Build: riscv64-linux-musl-gcc -static -o test_dummy_inotify_fanotify test_dummy_inotify_fanotify.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/inotify.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef FAN_CLOEXEC
#define FAN_CLOEXEC 0x00000001
#endif
#ifndef FAN_CLASS_NOTIF
#define FAN_CLASS_NOTIF 0x00000100
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

static void test_inotify_not_dummy(void) {
    int fd;
    char path[64];
    char buf[PATH_MAX];
    ssize_t lr;

    TEST_BEGIN("inotify_init1 fd path is not anon_inode:[dummy]")
    fd = syscall(SYS_inotify_init1, IN_NONBLOCK | IN_CLOEXEC);
    TEST_ASSERT(fd >= 0, "inotify_init1: %s", strerror(errno));

    snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
    lr = readlink(path, buf, sizeof(buf) - 1);
    TEST_ASSERT(lr >= 0, "readlink %s: %s", path, strerror(errno));
    buf[lr] = 0;
    TEST_ASSERT(strstr(buf, "dummy") == NULL, "kernel exposes dummy inode: %s",
                buf);

    close(fd);
    TEST_PASS();
}

static void test_fanotify_not_dummy(void) {
    int fd;
    char path[64];
    char buf[PATH_MAX];
    ssize_t lr;

    TEST_BEGIN("fanotify_init fd path is not anon_inode:[dummy]")
    fd = syscall(SYS_fanotify_init, FAN_CLASS_NOTIF | FAN_CLOEXEC, 0);
    if (fd < 0 && errno == ENOSYS) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(fd >= 0, "fanotify_init: %s", strerror(errno));
    snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
    lr = readlink(path, buf, sizeof(buf) - 1);
    TEST_ASSERT(lr >= 0, "readlink %s: %s", path, strerror(errno));
    buf[lr] = 0;
    TEST_ASSERT(strstr(buf, "dummy") == NULL, "kernel exposes dummy inode: %s",
                buf);
    close(fd);
    TEST_PASS();
}

int main(void) {
    test_inotify_not_dummy();
    test_fanotify_not_dummy();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
