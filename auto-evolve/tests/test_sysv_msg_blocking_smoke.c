/*
 * SysV 消息队列：阻塞 msgrcv / 延迟 msgsnd 烟测（Linux 基线；Starry QEMU 可复跑）
 * Build: gcc -pthread -o test_sysv_msg_blocking_smoke test_sysv_msg_blocking_smoke.c
 */

#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/ipc.h>
#include <sys/msg.h>
#include <unistd.h>

struct mymsg {
    long mtype;
    char mtext[64];
};

static int msqid;
static volatile int recv_ok;

static void *receiver(void *arg) {
    struct mymsg buf;
    ssize_t n = msgrcv(msqid, &buf, sizeof(buf.mtext), 1, 0);
    if (n < 0) {
        printf("FAIL msgrcv: %s\n", strerror(errno));
        return (void *)1;
    }
    if (n < 1 || buf.mtext[0] != 'x') {
        printf("FAIL bad payload\n");
        return (void *)2;
    }
    recv_ok = 1;
    return NULL;
}

static void *sender(void *arg) {
    usleep(200000);
    struct mymsg buf = {.mtype = 1};
    buf.mtext[0] = 'x';
    if (msgsnd(msqid, &buf, 1, 0) != 0) {
        printf("FAIL msgsnd: %s\n", strerror(errno));
        return (void *)3;
    }
    return NULL;
}

int main(void) {
    msqid = msgget(IPC_PRIVATE, 0600 | IPC_CREAT);
    if (msqid < 0) {
        perror("msgget");
        return 1;
    }
    pthread_t tr, ts;
    if (pthread_create(&tr, NULL, receiver, NULL) != 0) {
        perror("pthread_create receiver");
        return 1;
    }
    if (pthread_create(&ts, NULL, sender, NULL) != 0) {
        perror("pthread_create sender");
        return 1;
    }
    void *r1, *r2;
    pthread_join(tr, &r1);
    pthread_join(ts, &r2);
    msgctl(msqid, IPC_RMID, NULL);
    if (r1 || r2 || !recv_ok) {
        printf("FAIL r1=%p r2=%p recv_ok=%d\n", r1, r2, recv_ok);
        return 1;
    }
    printf("PASS\n");
    printf("\n=== SUMMARY: 1/1 passed ===\n");
    return 0;
}
