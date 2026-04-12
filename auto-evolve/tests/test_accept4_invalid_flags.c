/*
 * Test: accept4(2) 对非法 flags 须返回 EINVAL（Linux 仅允许 SOCK_CLOEXEC/SOCK_NONBLOCK 等价位）
 * Target: accept4
 * Build: riscv64-linux-musl-gcc -static -Wall -o test_accept4_invalid_flags test_accept4_invalid_flags.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
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

static void test_unknown_flags(void) {
    TEST_BEGIN("accept4 with unknown flags returns EINVAL")
    int ls = socket(AF_UNIX, SOCK_STREAM, 0);
    TEST_ASSERT(ls >= 0, "socket: %s", strerror(errno));
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    snprintf(addr.sun_path, sizeof(addr.sun_path), "/tmp/acc4_test_%d", (int)getpid());
    unlink(addr.sun_path);

    TEST_ASSERT(bind(ls, (struct sockaddr *)&addr, sizeof(addr)) == 0,
                "bind: %s", strerror(errno));
    TEST_ASSERT(listen(ls, 1) == 0, "listen: %s", strerror(errno));

    int cli = socket(AF_UNIX, SOCK_STREAM, 0);
    TEST_ASSERT(cli >= 0, "socket cli: %s", strerror(errno));
    TEST_ASSERT(connect(cli, (struct sockaddr *)&addr, sizeof(addr)) == 0,
                "connect: %s", strerror(errno));

    int a = accept4(ls, NULL, NULL, 0x80000000u);
    int e = errno;
    if (a >= 0) {
        close(a);
    }
    close(cli);
    close(ls);
    unlink(addr.sun_path);

    TEST_ASSERT(a < 0, "expected failure, got fd=%d", a);
    TEST_ASSERT(e == EINVAL, "expected EINVAL, got %s", strerror(e));
    TEST_PASS();
}

int main(void) {
    test_unknown_flags();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
