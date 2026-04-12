/*
 * Test: madvise(MADV_DONTNEED) 应丢弃页内容，再次读取常为 0
 * Target syscall: madvise
 * Build: riscv64-linux-musl-gcc -static -o test_mm_noop test_mm_noop.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

static int failures;
static int total;
static size_t page_size;

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

static void test_madvise_dontneed(void) {
    unsigned char *p;
    int i;
    int saw_nonzero;

    TEST_BEGIN("madvise MADV_DONTNEED clears anonymous page pattern")
    page_size = (size_t)sysconf(_SC_PAGESIZE);
    p = mmap(NULL, page_size, PROT_READ | PROT_WRITE,
             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    TEST_ASSERT(p != MAP_FAILED, "mmap: %s", strerror(errno));
    memset(p, 0xAB, page_size);
    TEST_ASSERT(madvise(p, page_size, MADV_DONTNEED) == 0, "madvise: %s",
                strerror(errno));

    saw_nonzero = 0;
    for (i = 0; i < (int)page_size; i++) {
        if (p[i] != 0) {
            saw_nonzero = 1;
            break;
        }
    }
    TEST_ASSERT(saw_nonzero == 0,
                "page still non-zero after MADV_DONTNEED (madvise no-op?)");
    munmap(p, page_size);
    TEST_PASS();
}

int main(void) {
    test_madvise_dontneed();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
