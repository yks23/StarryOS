/*
 * Test: sendmsg 应在一条控制缓冲内正确传递多个 SCM_RIGHTS（或合并为单条多 fd）
 * Target syscall: sendmsg / recvmsg（控制消息解析与内核递送）
 * Expected: 父进程 recvmsg 在 SOL_SOCKET/SCM_RIGHTS 中得到至少 2 个有效 fd（Linux 常合并为一条 cmsg）
 * Build: riscv64-linux-musl-gcc -static -o test_sendmsg_cmsg_align test_sendmsg_cmsg_align.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
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

static int count_rights_fds(struct msghdr *msg) {
    int n = 0;
    for (struct cmsghdr *c = CMSG_FIRSTHDR(msg); c != NULL; c = CMSG_NXTHDR(msg, c)) {
        if (c->cmsg_level != SOL_SOCKET || c->cmsg_type != SCM_RIGHTS)
            continue;
        size_t pay = c->cmsg_len - CMSG_LEN(0);
        if (pay % sizeof(int) != 0)
            continue;
        int *fp = (int *)CMSG_DATA(c);
        size_t ni = pay / sizeof(int);
        for (size_t i = 0; i < ni; i++) {
            int fd = fp[i];
            if (fd >= 0 && fcntl(fd, F_GETFD) != -1)
                n++;
        }
    }
    return n;
}

static void test_two_pipe_fds_via_sendmsg(void) {
    TEST_BEGIN("sendmsg passes two pipe fds, recvmsg sees >=2 valid fds")
    int sp[2];
    TEST_ASSERT(socketpair(AF_UNIX, SOCK_STREAM, 0, sp) == 0, "socketpair: %s",
                strerror(errno));

    pid_t pid = fork();
    TEST_ASSERT(pid >= 0, "fork: %s", strerror(errno));

    if (pid == 0) {
        close(sp[0]);
        int p1[2], p2[2];
        if (pipe(p1) != 0 || pipe(p2) != 0)
            _exit(20);

        char control[CMSG_SPACE(sizeof(int)) + CMSG_SPACE(sizeof(int))];
        memset(control, 0, sizeof(control));
        struct msghdr msg = {0};
        char byte = 'x';
        struct iovec iov = {.iov_base = &byte, .iov_len = 1};
        msg.msg_iov = &iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control;
        msg.msg_controllen = sizeof(control);

        struct cmsghdr *c = CMSG_FIRSTHDR(&msg);
        c->cmsg_level = SOL_SOCKET;
        c->cmsg_type = SCM_RIGHTS;
        c->cmsg_len = CMSG_LEN(sizeof(int));
        *(int *)CMSG_DATA(c) = p1[1];

        struct cmsghdr *c2 = CMSG_NXTHDR(&msg, c);
        if (!c2)
            _exit(21);
        c2->cmsg_level = SOL_SOCKET;
        c2->cmsg_type = SCM_RIGHTS;
        c2->cmsg_len = CMSG_LEN(sizeof(int));
        *(int *)CMSG_DATA(c2) = p2[1];

        msg.msg_controllen =
            (unsigned char *)c2 + CMSG_ALIGN(c2->cmsg_len) - (unsigned char *)control;

        if (sendmsg(sp[1], &msg, 0) < 0)
            _exit(22);
        close(p1[1]);
        close(p2[1]);
        close(p1[0]);
        close(p2[0]);
        close(sp[1]);
        _exit(0);
    }

    close(sp[1]);
    char rbuf[8];
    struct iovec riov = {.iov_base = rbuf, .iov_len = sizeof(rbuf)};
    char rcontrol[256];
    struct msghdr rmsg = {0};
    rmsg.msg_iov = &riov;
    rmsg.msg_iovlen = 1;
    rmsg.msg_control = rcontrol;
    rmsg.msg_controllen = sizeof(rcontrol);

    ssize_t nr = recvmsg(sp[0], &rmsg, 0);
    int st;
    waitpid(pid, &st, 0);

    TEST_ASSERT(WIFEXITED(st) && WEXITSTATUS(st) == 0, "child failed status %d", st);
    TEST_ASSERT(nr >= 0, "recvmsg: %s", strerror(errno));

    int nfd = count_rights_fds(&rmsg);
    TEST_ASSERT(nfd >= 2, "expected at least 2 SCM_RIGHTS fds, got %d (send path may drop 2nd cmsg)",
                nfd);

    for (struct cmsghdr *c = CMSG_FIRSTHDR(&rmsg); c != NULL; c = CMSG_NXTHDR(&rmsg, c)) {
        if (c->cmsg_level != SOL_SOCKET || c->cmsg_type != SCM_RIGHTS)
            continue;
        size_t pay = c->cmsg_len - CMSG_LEN(0);
        if (pay % sizeof(int) != 0)
            continue;
        int *fp = (int *)CMSG_DATA(c);
        size_t ni = pay / sizeof(int);
        for (size_t i = 0; i < ni; i++)
            close(fp[i]);
    }

    close(sp[0]);
    TEST_PASS();
}

int main(void) {
    test_two_pipe_fds_via_sendmsg();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
