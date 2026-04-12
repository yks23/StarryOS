/*
 * Test: 两个子进程并行 execve，功能上应均成功（ELF 全局锁导致性能串行化，本测试验证正确性）
 * Target syscall: execve
 * Expected: 两次 exec 均成功退出 0
 * Build: riscv64-linux-musl-gcc -static -o test_elf_parallel_exec test_elf_parallel_exec.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures = 0;
static int total = 0;

#define TEST_BEGIN(name)                                                       \
    do {                                                                       \
        total++;                                                               \
        printf("[TEST] %s ... ", (name));                                      \
    } while (0)

#define TEST_FAIL(fmt, ...)                                                    \
    do {                                                                       \
        printf("FAIL: " fmt "\n", ##__VA_ARGS__);                              \
        failures++;                                                            \
    } while (0)

#define TEST_PASS() printf("PASS\n")

static const char *pick_helper(void) {
    static const char *candidates[] = {
        "/bin/true",
        "/usr/bin/true",
    };
    for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]); i++) {
        if (access(candidates[i], X_OK) == 0) {
            return candidates[i];
        }
    }
    return NULL;
}

static void test_parallel_exec(void) {
    TEST_BEGIN("two children execve same helper");
    const char *path = pick_helper();
    if (!path) {
        TEST_FAIL("no /bin/true or /usr/bin/true in rootfs");
        return;
    }
    pid_t a = fork();
    if (a < 0) {
        TEST_FAIL("fork a: %s", strerror(errno));
        return;
    }
    if (a == 0) {
        char *argv[] = {(char *)path, NULL};
        char *env[] = {NULL};
        execve(path, argv, env);
        _exit(127);
    }
    pid_t b = fork();
    if (b < 0) {
        TEST_FAIL("fork b: %s", strerror(errno));
        waitpid(a, NULL, 0);
        return;
    }
    if (b == 0) {
        char *argv[] = {(char *)path, NULL};
        char *env[] = {NULL};
        execve(path, argv, env);
        _exit(127);
    }
    int sa = 0, sb = 0;
    waitpid(a, &sa, 0);
    waitpid(b, &sb, 0);
    if (!WIFEXITED(sa) || WEXITSTATUS(sa) != 0) {
        TEST_FAIL("child a status %d", sa);
        return;
    }
    if (!WIFEXITED(sb) || WEXITSTATUS(sb) != 0) {
        TEST_FAIL("child b status %d", sb);
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_parallel_exec();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
