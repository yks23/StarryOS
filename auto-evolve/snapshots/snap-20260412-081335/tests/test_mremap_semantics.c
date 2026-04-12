/*
 * Test: mremap 扩大匿名映射时，无 MREMAP_MAYMOVE 时应在原地址原地扩展（Linux）；实现若通过新 mmap+拷贝+unmap 常得到不同地址
 * Target syscall: mremap
 * Expected: mremap(old, old_len, new_len, 0) 返回地址等于 old（在可原地扩展时）
 * Build: riscv64-linux-musl-gcc -static -o test_mremap_semantics test_mremap_semantics.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
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

static void test_expand_inplace(void) {
    TEST_BEGIN("mremap grow anonymous mapping in place (no MAYMOVE)");
    long ps = sysconf(_SC_PAGESIZE);
    if (ps < 1) {
        TEST_FAIL("sysconf pagesize");
        return;
    }
    size_t old_len = (size_t)ps;
    size_t new_len = old_len * 2u;
    void *p =
        mmap(NULL, old_len, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS,
             -1, 0);
    if (p == MAP_FAILED) {
        TEST_FAIL("mmap: %s", strerror(errno));
        return;
    }
    *(volatile unsigned int *)p = 0xAABBCCDDu;

    void *q = mremap(p, old_len, new_len, 0);
    if (q == MAP_FAILED) {
        TEST_FAIL("mremap: %s", strerror(errno));
        munmap(p, old_len);
        return;
    }
    if (*(volatile unsigned int *)q != 0xAABBCCDDu) {
        TEST_FAIL("data not preserved after mremap");
        munmap(q, new_len);
        return;
    }
    if (q != p) {
        printf("FAIL: mremap returned new address %p (old %p); kernel likely "
               "alloc+copy+unmap instead of remap\n",
               q, p);
        failures++;
        munmap(q, new_len);
        return;
    }
    munmap(q, new_len);
    TEST_PASS();
}

int main(void) {
    test_expand_inplace();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
