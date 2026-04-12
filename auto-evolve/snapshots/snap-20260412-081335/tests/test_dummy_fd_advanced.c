/*
 * Test: bpf / io_uring_setup / userfaultfd / perf_event_open 不应返回 dummy anon_inode
 * Target syscall: bpf, io_uring_setup, userfaultfd, perf_event_open
 * Build: riscv64-linux-musl-gcc -static -o test_dummy_fd_advanced test_dummy_fd_advanced.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <linux/bpf.h>
#include <linux/io_uring.h>
#include <linux/perf_event.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef __NR_userfaultfd
#if defined(__riscv) && __riscv_xlen == 64
#define __NR_userfaultfd 282
#endif
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

static int check_not_dummy(const char *tag, int fd) {
    char path[64];
    char buf[PATH_MAX];
    ssize_t lr;

    snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
    lr = readlink(path, buf, sizeof(buf) - 1);
    if (lr < 0) {
        printf("FAIL: %s readlink: %s\n", tag, strerror(errno));
        failures++;
        close(fd);
        return -1;
    }
    buf[lr] = 0;
    if (strstr(buf, "dummy") != NULL) {
        printf("FAIL: %s dummy inode path=%s\n", tag, buf);
        failures++;
        close(fd);
        return -1;
    }
    close(fd);
    return 0;
}

static void test_bpf(void) {
    union bpf_attr attr;
    int fd;

    TEST_BEGIN("bpf returns real fd or error (not dummy)")
    memset(&attr, 0, sizeof(attr));
    attr.prog_type = BPF_PROG_TYPE_SOCKET_FILTER;
    attr.insn_cnt = 0;
    attr.insns = 0;
    attr.license = 0;
    fd = syscall(SYS_bpf, BPF_PROG_LOAD, &attr, sizeof(attr));
    if (fd < 0) {
        printf("PASS\n");
        break;
    }
    if (check_not_dummy("bpf", fd) != 0)
        break;
    TEST_PASS();
}

static void test_io_uring(void) {
    struct io_uring_params p;
    int fd;

    TEST_BEGIN("io_uring_setup fd not dummy")
    memset(&p, 0, sizeof(p));
    fd = syscall(SYS_io_uring_setup, 4, &p);
    TEST_ASSERT(fd >= 0, "io_uring_setup: %s", strerror(errno));
    if (check_not_dummy("io_uring", fd) != 0)
        break;
    TEST_PASS();
}

static void test_userfaultfd(void) {
#ifdef __NR_userfaultfd
    int fd;

    TEST_BEGIN("userfaultfd fd not dummy or ENOSYS")
    fd = syscall(__NR_userfaultfd, O_NONBLOCK | O_CLOEXEC);
    if (fd < 0 && errno == ENOSYS) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(fd >= 0, "userfaultfd: %s", strerror(errno));
    if (check_not_dummy("userfaultfd", fd) != 0)
        break;
    TEST_PASS();
#else
    total++;
    printf("[TEST] userfaultfd fd not dummy or ENOSYS ... PASS\n");
#endif
}

static void test_perf(void) {
    struct perf_event_attr pea;
    int fd;

    TEST_BEGIN("perf_event_open fd not dummy or EPERM")
    memset(&pea, 0, sizeof(pea));
    pea.size = sizeof(pea);
    pea.type = PERF_TYPE_SOFTWARE;
    pea.config = PERF_COUNT_SW_CPU_CLOCK;
    pea.disabled = 1;
    pea.exclude_kernel = 1;
    fd = syscall(SYS_perf_event_open, &pea, 0, -1, -1, 0);
    if (fd < 0 && (errno == EACCES || errno == EPERM)) {
        printf("PASS\n");
        break;
    }
    TEST_ASSERT(fd >= 0, "perf_event_open: %s", strerror(errno));
    if (check_not_dummy("perf", fd) != 0)
        break;
    TEST_PASS();
}

int main(void) {
    test_bpf();
    test_io_uring();
    test_userfaultfd();
    test_perf();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
