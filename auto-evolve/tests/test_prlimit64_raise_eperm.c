/*
 * Test: prlimit64 无法将硬上限提高到超过当前硬上限；须失败且 errno=EPERM
 * Target: prlimit64 / RLIMIT_NOFILE
 * Linux: 见 kernel/sys.c capable 与 do_prlimit
 * Build: riscv64-linux-musl-gcc -static -o test_prlimit64_raise_eperm test_prlimit64_raise_eperm.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>

static int failures;
static int total;

#define TEST_PASS()                                                            \
    printf("PASS\n");                                                          \
    } while (0)

static void test_raise_hard_beyond_cap(void) {
    total++;
    printf("[TEST] prlimit: rlim_max > current hard max returns EPERM ... ");
    struct rlimit cur;
    if (getrlimit(RLIMIT_NOFILE, &cur) != 0) {
        printf("FAIL: getrlimit: %s\n", strerror(errno));
        failures++;
        return;
    }
    if (cur.rlim_max == RLIM_INFINITY) {
        printf("SKIP (hard=RLIM_INFINITY)\n");
        total--;
        return;
    }
    struct rlimit newl;
    newl.rlim_cur = cur.rlim_cur;
    newl.rlim_max = cur.rlim_max + 1;
    int r = prlimit(0, RLIMIT_NOFILE, &newl, NULL);
    if (r != -1) {
        printf("FAIL: expected failure, got r=%d\n", r);
        failures++;
        return;
    }
    if (errno != EPERM) {
        printf("FAIL: expected EPERM, got %s\n", strerror(errno));
        failures++;
        return;
    }
    printf("PASS\n");
}

int main(void) {
    test_raise_hard_beyond_cap();
    printf("\n=== SUMMARY: %d/%d passed ===\n", total - failures, total);
    return failures ? 1 : 0;
}
