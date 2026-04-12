/*
 * Test: recvmsg/recvfrom 对非法 MSG_* 标志须失败（EINVAL），不得静默忽略未知位
 * Target syscall: recvmsg（及内核 recvfrom 路径）
 * Expected: Linux 对未知/非法 flags 返回 -1 EINVAL（见 man 2 recvmsg）
 * Build: riscv64-linux-musl-gcc -static -o test_recvmsg_invalid_flags test_recvmsg_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
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

static void test_recvmsg_unknown_flag_bits(void) {
    TEST_BEGIN("recvmsg with unknown flag bits returns EINVAL")
    int sp[2];
    TEST_ASSERT(socketpair(AF_UNIX, SOCK_STREAM, 0, sp) == 0, "socketpair: %s",
                strerror(errno));
    TEST_ASSERT(write(sp[1], "x", 1) == 1, "write: %s", strerror(errno));

    char buf[8];
    struct iovec iov = {.iov_base = buf, .iov_len = sizeof(buf)};
    struct msghdr msg = {0};
    msg.msg_iov = &iov;
    msg.msg_iovlen = 1;

    ssize_t r = recvmsg(sp[0], &msg, 0x80000000u);
    close(sp[0]);
    close(sp[1]);
    TEST_ASSERT(r < 0, "expected failure, got r=%zd", r);
    TEST_ASSERT(errno == EINVAL, "expected EINVAL, got %s", strerror(errno));
    TEST_PASS();
}

int main(void) {
    test_recvmsg_unknown_flag_bits();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
