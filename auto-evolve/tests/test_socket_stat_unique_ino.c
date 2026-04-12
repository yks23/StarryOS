/*
 * Test: 不同 socket fd 上 fstat 应得到不同 st_ino（Linux 套接字 inode 唯一）
 * Target syscall: fstat（经 FileLike::stat → Socket）
 * Expected: 两个独立 socket 的 st_ino 不相等
 * Build: riscv64-linux-musl-gcc -static -o test_socket_stat_unique_ino test_socket_stat_unique_ino.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
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

static void test_two_sockets_distinct_ino(void) {
    TEST_BEGIN("fstat: two TCP sockets have distinct st_ino")
    int a = socket(AF_INET, SOCK_STREAM, 0);
    int b = socket(AF_INET, SOCK_STREAM, 0);
    TEST_ASSERT(a >= 0 && b >= 0, "socket: %s", strerror(errno));
    struct stat sa, sb;
    memset(&sa, 0, sizeof(sa));
    memset(&sb, 0, sizeof(sb));
    TEST_ASSERT(fstat(a, &sa) == 0 && fstat(b, &sb) == 0, "fstat: %s",
                strerror(errno));
    close(a);
    close(b);
    TEST_ASSERT(S_ISSOCK(sa.st_mode) && S_ISSOCK(sb.st_mode),
                "expected S_IFSOCK (got mode %o / %o)", sa.st_mode, sb.st_mode);
    TEST_ASSERT(sa.st_ino != sb.st_ino,
                "st_ino not unique across sockets (stub ino=%lu == %lu)",
                (unsigned long)sa.st_ino, (unsigned long)sb.st_ino);
    TEST_PASS();
}

int main(void) {
    test_two_sockets_distinct_ino();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
