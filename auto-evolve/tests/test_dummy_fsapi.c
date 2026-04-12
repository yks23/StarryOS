/*
 * Test: fsopen / fspick / open_tree 返回真实 fd，而非 dummy
 * Target syscall: fsopen, fspick, open_tree
 * Build: riscv64-linux-musl-gcc -static -o test_dummy_fsapi test_dummy_fsapi.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef __NR_memfd_secret
#define __NR_memfd_secret 447
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

/* 0 = not dummy, 1 = dummy, -1 = readlink failed */
static int path_has_dummy(int fd) {
    char path[64];
    char buf[PATH_MAX];
    ssize_t lr;

    snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
    lr = readlink(path, buf, sizeof(buf) - 1);
    if (lr < 0)
        return -1;
    buf[lr] = 0;
    return strstr(buf, "dummy") != NULL ? 1 : 0;
}

static void test_fsopen(void) {
    int fd;
    int d;

    TEST_BEGIN("fsopen returns non-dummy fd or EINVAL")
    fd = syscall(__NR_fsopen, "ext4", 0);
    if (fd < 0) {
        TEST_ASSERT(errno == EINVAL || errno == ENODEV || errno == ENOENT,
                    "unexpected errno %s", strerror(errno));
        printf("PASS\n");
        break;
    }
    d = path_has_dummy(fd);
    TEST_ASSERT(d >= 0, "readlink proc");
    TEST_ASSERT(d == 0, "dummy fd from fsopen");
    close(fd);
    TEST_PASS();
}

static void test_open_tree(void) {
    int fd;
    int d;

    TEST_BEGIN("open_tree returns non-dummy fd or error")
    fd = syscall(__NR_open_tree, AT_FDCWD, "/", 0);
    if (fd < 0) {
        printf("PASS\n");
        break;
    }
    d = path_has_dummy(fd);
    TEST_ASSERT(d >= 0, "readlink proc");
    TEST_ASSERT(d == 0, "dummy fd from open_tree");
    close(fd);
    TEST_PASS();
}

static void test_memfd_secret(void) {
    int fd;
    int d;

    TEST_BEGIN("memfd_secret returns non-dummy or ENOSYS")
    fd = syscall(__NR_memfd_secret, "x", 0);
    if (fd < 0 && errno == ENOSYS) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(fd >= 0, "memfd_secret: %s", strerror(errno));
    d = path_has_dummy(fd);
    TEST_ASSERT(d >= 0, "readlink proc");
    TEST_ASSERT(d == 0, "dummy memfd_secret");
    close(fd);
    TEST_PASS();
}

int main(void) {
    test_fsopen();
    test_open_tree();
    test_memfd_secret();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
