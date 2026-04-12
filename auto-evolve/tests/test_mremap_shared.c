/*
 * Test: mremap 扩展 MAP_SHARED 文件映射后，第二页应写回文件
 * Target syscall: mremap, mmap, msync
 * Build: riscv64-linux-musl-gcc -static -o test_mremap_shared test_mremap_shared.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#ifndef MREMAP_MAYMOVE
#define MREMAP_MAYMOVE 1
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

static char template[] = "/tmp/starry_mremap_XXXXXX";

static void test_mremap_file_shared(void) {
    int fd;
    size_t ps;
    void *p;
    void *p2;
    char c;
    ssize_t n;

    TEST_BEGIN("mremap grows MAP_SHARED mapping; second page visible in file")
    ps = (size_t)sysconf(_SC_PAGESIZE);
    fd = mkstemp(template);
    TEST_ASSERT(fd >= 0, "mkstemp: %s", strerror(errno));
    TEST_ASSERT(ftruncate(fd, (off_t)ps) == 0, "ftruncate: %s", strerror(errno));

    p = mmap(NULL, ps, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    TEST_ASSERT(p != MAP_FAILED, "mmap: %s", strerror(errno));

    p2 = mremap(p, ps, ps * 2, MREMAP_MAYMOVE);
    TEST_ASSERT(p2 != MAP_FAILED, "mremap: %s", strerror(errno));

    ((unsigned char *)p2)[ps] = 'Y';
    TEST_ASSERT(msync(p2, ps * 2, MS_SYNC) == 0, "msync: %s", strerror(errno));

    n = pread(fd, &c, 1, (off_t)ps);
    TEST_ASSERT(n == 1, "pread: %s", strerror(errno));
    TEST_ASSERT(c == 'Y', "second page not wired to file (mremap lost backend?)");

    munmap(p2, ps * 2);
    close(fd);
    unlink(template);
    TEST_PASS();
}

int main(void) {
    test_mremap_file_shared();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
