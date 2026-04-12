/*
 * Test: accept(2) 返回的对端地址应与 getpeername(accepted_fd) 一致
 * Target syscall: accept / accept4
 * Expected: Linux 在 accept 中填充的是远端（peer）地址，而非本端 local 地址
 * Build: riscv64-linux-musl-gcc -static -o test_accept_peer_addr test_accept_peer_addr.c
 */

#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
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

static int sockaddr_in_equal(const struct sockaddr_in *a, const struct sockaddr_in *b) {
    return a->sin_family == b->sin_family && a->sin_port == b->sin_port &&
           a->sin_addr.s_addr == b->sin_addr.s_addr;
}

static void test_accept_addr_matches_getpeername(void) {
    TEST_BEGIN("accept peer addr equals getpeername on new socket")
    int srv = socket(AF_INET, SOCK_STREAM, 0);
    TEST_ASSERT(srv >= 0, "socket srv: %s", strerror(errno));

    int reuse = 1;
    if (setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse)) < 0) {
        close(srv);
        printf("FAIL: setsockopt: %s\n", strerror(errno));
        failures++;
        break;
    }

    struct sockaddr_in bind_addr;
    memset(&bind_addr, 0, sizeof(bind_addr));
    bind_addr.sin_family = AF_INET;
    bind_addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    bind_addr.sin_port = 0;

    if (bind(srv, (struct sockaddr *)&bind_addr, sizeof(bind_addr)) < 0) {
        close(srv);
        printf("FAIL: bind: %s\n", strerror(errno));
        failures++;
        break;
    }
    if (listen(srv, 8) < 0) {
        close(srv);
        printf("FAIL: listen: %s\n", strerror(errno));
        failures++;
        break;
    }

    socklen_t blen = sizeof(bind_addr);
    if (getsockname(srv, (struct sockaddr *)&bind_addr, &blen) < 0) {
        close(srv);
        printf("FAIL: getsockname: %s\n", strerror(errno));
        failures++;
        break;
    }
    uint16_t port = ntohs(bind_addr.sin_port);

    pid_t pid = fork();
    if (pid < 0) {
        close(srv);
        printf("FAIL: fork: %s\n", strerror(errno));
        failures++;
        break;
    }
    if (pid == 0) {
        int c = socket(AF_INET, SOCK_STREAM, 0);
        if (c < 0)
            _exit(10);
        struct sockaddr_in peer;
        memset(&peer, 0, sizeof(peer));
        peer.sin_family = AF_INET;
        inet_pton(AF_INET, "127.0.0.1", &peer.sin_addr);
        peer.sin_port = htons(port);
        if (connect(c, (struct sockaddr *)&peer, sizeof(peer)) < 0) {
            close(c);
            _exit(11);
        }
        close(c);
        _exit(0);
    }

    struct sockaddr_storage from_accept;
    socklen_t alen = sizeof(from_accept);
    memset(&from_accept, 0, sizeof(from_accept));
    int newfd = accept(srv, (struct sockaddr *)&from_accept, &alen);
    close(srv);

    int st;
    waitpid(pid, &st, 0);
    TEST_ASSERT(WIFEXITED(st) && WEXITSTATUS(st) == 0, "child connect failed, status %d", st);

    TEST_ASSERT(newfd >= 0, "accept: %s", strerror(errno));

    struct sockaddr_storage from_gp;
    socklen_t glen = sizeof(from_gp);
    memset(&from_gp, 0, sizeof(from_gp));
    TEST_ASSERT(getpeername(newfd, (struct sockaddr *)&from_gp, &glen) == 0,
                "getpeername: %s", strerror(errno));

    TEST_ASSERT(from_accept.ss_family == AF_INET && from_gp.ss_family == AF_INET,
                "expected AF_INET, got %d / %d", (int)from_accept.ss_family,
                (int)from_gp.ss_family);

    TEST_ASSERT(sockaddr_in_equal((const struct sockaddr_in *)&from_accept,
                                  (const struct sockaddr_in *)&from_gp),
                "accept addr != getpeername (peer address mismatch — possible local_addr bug)");

    close(newfd);
    TEST_PASS();
}

int main(void) {
    test_accept_addr_matches_getpeername();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
