/*
 * Test: /proc/cpuinfo 与 /proc/self/maps 的 Linux 兼容内容（Starry maps 常为占位 vdso，无 [heap]）
 * Target: procfs
 * Expected: 可打开 /proc/cpuinfo；/proc/self/maps 含 [heap] 或栈等真实映射行
 * Build: riscv64-linux-musl-gcc -static -o test_proc_incomplete test_proc_incomplete.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

static char *slurp(const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) {
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    if (sz < 0 || sz > 1024 * 1024) {
        fclose(f);
        return NULL;
    }
    rewind(f);
    char *buf = malloc((size_t)sz + 1);
    if (!buf) {
        fclose(f);
        return NULL;
    }
    size_t n = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    buf[n] = '\0';
    return buf;
}

static void test_proc_cpuinfo(void) {
    TEST_BEGIN("open /proc/cpuinfo");
    FILE *f = fopen("/proc/cpuinfo", "r");
    if (!f) {
        printf("FAIL: /proc/cpuinfo missing (%s)\n", strerror(errno));
        failures++;
        return;
    }
    fclose(f);
    TEST_PASS();
}

static void test_proc_maps_meaningful(void) {
    TEST_BEGIN("/proc/self/maps contains heap or stack");
    char *s = slurp("/proc/self/maps");
    if (!s) {
        TEST_FAIL("read maps");
        return;
    }
    int ok = (strstr(s, "[heap]") != NULL) || (strstr(s, "[stack]") != NULL) ||
             (strstr(s, "rwxp") != NULL);
    free(s);
    if (!ok) {
        printf("FAIL: maps lacks typical entries (stub vdso-only maps)\n");
        failures++;
        return;
    }
    TEST_PASS();
}

int main(void) {
    test_proc_cpuinfo();
    test_proc_maps_meaningful();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
