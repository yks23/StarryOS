#!/usr/bin/env python3
"""生成 oscomp basic syscall 测试的 C 源码并交叉编译。"""

import os
import subprocess
from pathlib import Path

ARCH = os.environ.get("ARCH", "riscv64")
GCC = f"{ARCH}-linux-musl-gcc"
OUT = Path(__file__).parent / ARCH
OUT.mkdir(parents=True, exist_ok=True)
SRC = Path(__file__).parent / "src"
SRC.mkdir(parents=True, exist_ok=True)

TESTS = {
"test_brk": r'''
#include <stdio.h>
#include <unistd.h>
int main(){void*p1=sbrk(0);sbrk(64);void*p2=sbrk(0);sbrk(64);void*p3=sbrk(0);
printf("Before alloc,heap pos: %ld\n",(long)p1);printf("After alloc,heap pos: %ld\n",(long)p2);printf("Alloc again,heap pos: %ld\n",(long)p3);return 0;}
''',
"test_chdir": r'''
#include <stdio.h>
#include <unistd.h>
int main(){char buf[256];chdir("/tmp");getcwd(buf,256);printf("%s\n",buf);chdir("/");getcwd(buf,256);printf("%s\n",buf);return 0;}
''',
"test_close": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_tc",O_CREAT|O_WRONLY,0644);if(fd<0){puts("open fail");return 1;}printf("close %d\n",close(fd));printf("close again %d\n",close(fd));unlink("/tmp/_tc");return 0;}
''',
"test_dup": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_td",O_CREAT|O_RDWR,0644);int fd2=dup(fd);write(fd,"hi",2);lseek(fd2,0,SEEK_SET);char b[8]={0};read(fd2,b,2);printf("read from dup: %s\n",b);close(fd);close(fd2);unlink("/tmp/_td");return 0;}
''',
"test_dup2": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_td2",O_CREAT|O_RDWR,0644);dup2(fd,100);write(100,"ok",2);lseek(fd,0,SEEK_SET);char b[8]={0};read(fd,b,2);printf("read: %s\n",b);close(fd);close(100);unlink("/tmp/_td2");return 0;}
''',
"test_execve": r'''
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
int main(){pid_t p=fork();if(p==0){char*a[]={"/bin/echo","execve_ok",NULL};execve(a[0],a,NULL);_exit(1);}int s;waitpid(p,&s,0);printf("child exit %d\n",WEXITSTATUS(s));return 0;}
''',
"test_exit": r'''
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
int main(){pid_t p=fork();if(p==0)exit(42);int s;waitpid(p,&s,0);printf("child exit code: %d\n",WEXITSTATUS(s));return 0;}
''',
"test_fork": r'''
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
int main(){pid_t p=fork();if(p<0){puts("fork fail");return 1;}if(p==0){puts("child");_exit(0);}int s;waitpid(p,&s,0);printf("parent: child exit %d\n",WEXITSTATUS(s));return 0;}
''',
"test_clone": r'''
#define _GNU_SOURCE
#include <stdio.h>
#include <sched.h>
#include <sys/wait.h>
#include <stdlib.h>
#include <unistd.h>
int child_fn(void*a){puts("clone child");return 0;}
int main(){char*stack=malloc(65536);pid_t p=clone(child_fn,stack+65536,SIGCHLD,NULL);if(p<0){puts("clone fail");return 1;}int s;waitpid(p,&s,0);printf("clone child exit %d\n",WEXITSTATUS(s));free(stack);return 0;}
''',
"test_fstat": r'''
#include <stdio.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp",O_RDONLY);struct stat st;fstat(fd,&st);printf("mode: %o\nsize: %ld\n",(unsigned)st.st_mode,(long)st.st_size);close(fd);return 0;}
''',
"test_getcwd": r'''
#include <stdio.h>
#include <unistd.h>
int main(){char buf[256];getcwd(buf,256);printf("%s\n",buf);return 0;}
''',
"test_getdents": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/syscall.h>
#include <dirent.h>
int main(){int fd=open("/",O_RDONLY|O_DIRECTORY);char buf[1024];int n=syscall(SYS_getdents64,fd,buf,sizeof(buf));printf("getdents64 returned %d\n",n);close(fd);return n>0?0:1;}
''',
"test_getpid": r'''
#include <stdio.h>
#include <unistd.h>
int main(){printf("pid: %d\n",getpid());return getpid()>0?0:1;}
''',
"test_getppid": r'''
#include <stdio.h>
#include <unistd.h>
int main(){printf("ppid: %d\n",getppid());return getppid()>0?0:1;}
''',
"test_gettimeofday": r'''
#include <stdio.h>
#include <sys/time.h>
int main(){struct timeval tv;gettimeofday(&tv,NULL);printf("sec: %ld\nusec: %ld\n",(long)tv.tv_sec,(long)tv.tv_usec);return tv.tv_sec>0?0:1;}
''',
"test_mkdir": r'''
#include <stdio.h>
#include <sys/stat.h>
#include <unistd.h>
int main(){int r=mkdir("/tmp/_tmkdir",0755);printf("mkdir: %d\n",r);rmdir("/tmp/_tmkdir");return r;}
''',
"test_mmap": r'''
#include <stdio.h>
#include <sys/mman.h>
#include <string.h>
int main(){void*p=mmap(NULL,4096,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(p==MAP_FAILED){puts("mmap fail");return 1;}memset(p,'A',4096);printf("mmap ok, first byte: %c\n",*(char*)p);munmap(p,4096);return 0;}
''',
"test_munmap": r'''
#include <stdio.h>
#include <sys/mman.h>
int main(){void*p=mmap(NULL,4096,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);int r=munmap(p,4096);printf("munmap: %d\n",r);return r;}
''',
"test_open": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_topen",O_CREAT|O_WRONLY,0644);printf("open fd: %d\n",fd);close(fd);unlink("/tmp/_topen");return fd>=0?0:1;}
''',
"test_openat": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=openat(AT_FDCWD,"/tmp/_topenat",O_CREAT|O_WRONLY,0644);printf("openat fd: %d\n",fd);close(fd);unlink("/tmp/_topenat");return fd>=0?0:1;}
''',
"test_pipe": r'''
#include <stdio.h>
#include <unistd.h>
int main(){int fds[2];pipe(fds);write(fds[1],"hi",2);char b[8]={0};read(fds[0],b,2);printf("pipe read: %s\n",b);close(fds[0]);close(fds[1]);return 0;}
''',
"test_read": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_tread",O_CREAT|O_RDWR,0644);write(fd,"hello",5);lseek(fd,0,SEEK_SET);char b[8]={0};int n=read(fd,b,5);printf("read %d bytes: %s\n",n,b);close(fd);unlink("/tmp/_tread");return n==5?0:1;}
''',
"test_sleep": r'''
#include <stdio.h>
#include <time.h>
int main(){struct timespec ts={0,10000000};nanosleep(&ts,NULL);puts("sleep ok");return 0;}
''',
"test_times": r'''
#include <stdio.h>
#include <sys/times.h>
int main(){struct tms t;clock_t c=times(&t);printf("times: %ld\nutime: %ld\nstime: %ld\n",(long)c,(long)t.tms_utime,(long)t.tms_stime);return 0;}
''',
"test_uname": r'''
#include <stdio.h>
#include <sys/utsname.h>
int main(){struct utsname u;uname(&u);printf("sysname: %s\nnodename: %s\nrelease: %s\nmachine: %s\n",u.sysname,u.nodename,u.release,u.machine);return 0;}
''',
"test_unlink": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_tunlink",O_CREAT|O_WRONLY,0644);close(fd);int r=unlink("/tmp/_tunlink");printf("unlink: %d\n",r);return r;}
''',
"test_wait": r'''
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
int main(){pid_t p=fork();if(p==0)_exit(7);int s;wait(&s);printf("wait: exit=%d\n",WEXITSTATUS(s));return WEXITSTATUS(s)==7?0:1;}
''',
"test_waitpid": r'''
#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
int main(){pid_t p=fork();if(p==0)_exit(13);int s;waitpid(p,&s,0);printf("waitpid: exit=%d\n",WEXITSTATUS(s));return WEXITSTATUS(s)==13?0:1;}
''',
"test_write": r'''
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
int main(){int fd=open("/tmp/_twrite",O_CREAT|O_WRONLY,0644);int n=write(fd,"hello\n",6);printf("write %d bytes\n",n);close(fd);unlink("/tmp/_twrite");return n==6?0:1;}
''',
"test_yield": r'''
#include <stdio.h>
#include <sched.h>
int main(){sched_yield();puts("yield ok");return 0;}
''',
"test_mount": r'''
#include <stdio.h>
int main(){puts("mount test: skip (needs root + device)");return 0;}
''',
"test_umount": r'''
#include <stdio.h>
int main(){puts("umount test: skip (needs root + device)");return 0;}
''',
}

ok = 0
fail = 0
for name, code in TESTS.items():
    src_file = SRC / f"{name}.c"
    src_file.write_text(code.strip() + "\n")
    binary = OUT / name
    r = subprocess.run([GCC, "-static", "-o", str(binary), str(src_file), "-lpthread"],
                       capture_output=True, text=True, timeout=30)
    if r.returncode == 0:
        ok += 1
    else:
        print(f"FAIL: {name}: {r.stderr[:100]}")
        fail += 1

print(f"\noscomp basic: {ok}/{ok+fail} 编译成功")
print(f"输出: {OUT}")
