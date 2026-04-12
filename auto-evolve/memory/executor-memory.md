# Executor Memory

## 最近更新
- 日期：2026-04-12：issue-215 resolved（**`pselect6`/`ppoll`/`select` 超时**：**`read_timespec_user`**/**`read_timeval_user`**（**`crate::time`** 新增 **`read_timeval_user`**）；**`UserConstPtr::address()`** 转裸指针；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-213 resolved（**`msgctl` `IPC_SET`**：**`read_msqid_ds_ipc_set_user`** 仅 **`msg_perm.uid`/`gid`/`mode`** + **`msg_qbytes`** 标量读；去整 **`msqid_ds` `vm_read`**/**`AnyBitPattern`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-212 resolved（**`rt_sigqueueinfo`/`rt_tgsigqueueinfo`**：**`read_signal_info_user`** — **`si_signo`/`si_errno`/`si_code`** 字段读 + **`vm_read_slice`** 填 **`_sifields`**；去掉整 **`SignalInfo` `assume_init`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-211 resolved（**`sched_setscheduler`**：**`read_sched_param_user`**（**`sched_priority`** **`addr_of!` + `vm_read`**）；去掉 **`SchedParam` `assume_init`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-210 resolved（**`signalfd4`**：**`read_signal_set_user`** **`pub(crate)`**；**`mask`** 与 **`rt_sigprocmask`** 同字级读；去掉 **`assume_init`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-214 resolved（**`shmctl` `IPC_SET`**：仅合并用户 **`shm_perm.uid`/`gid`/`mode & 0o777`**，不再整颗 **`ShmidDs`** 覆盖内核；对齐 Linux **`shmctl(2)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-209 resolved（**`timerfd_settime`**：用户 **`itimerspec`** 由 **`read_timespec_user`** 读 **`it_interval`/`it_value`**（**`addr_of!`**）；去掉整结构 **`assume_init`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-208 resolved（**`io_uring_setup`**：用户侧仅 **`addr_of!((*params).flags).vm_read()`**；**`IoUringParams`** 成功写回前内核栈字面量构造；去掉整结构 **`vm_read_uninit`/`assume_init`**（嵌套 **`sq_off`/`cq_off`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-207 resolved（**`futex` `FUTEX_WAIT`/`FUTEX_WAIT_BITSET`**：用户 **`timespec` 超时** 经 **`crate::time::read_timespec_user`**（**`tv_sec`/`tv_nsec` 字段 `vm_read`**）；**`ctl`/`schedule`/`signal`** 重复实现并入 **`time.rs`**；去掉 **`assume_init`/`FIXME`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-206 resolved（**`rt_sigprocmask`/`rt_sigaction`/`rt_sigtimedwait`/`rt_sigsuspend`/`sigaltstack`**：**`SignalSet`/`kernel_sigaction`/`SignalStack`/`timespec`** 字段或 **`usize` 位型读**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-205 resolved（**`nanosleep`/`clock_nanosleep`**：用户 **`timespec` `req`** 字段级 **`vm_read`**；去掉 **`assume_init`/`FIXME`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-204 resolved（**`setitimer`**：**`itimerval`**/**`timeval`** 字段级 **`vm_read`**；去掉 **`assume_init`/`FIXME`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-202 resolved（**`capget`/`capset`**：**`__user_cap_header_struct`**/**`__user_cap_data_struct`** 字段级 **`vm_read`**；去掉 **`assume_init`/`FIXME`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-201 resolved（**`utimensat`/`utime`/`utimes`**：**`timespec`/`timeval`/`utimbuf`** 字段级 **`vm_read`**，去掉 **`assume_init`/`FIXME`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-194 resolved（**`getcwd`**：**`size==0`**/**负 `size`** → **`EINVAL`**；**`NULL` `buf`**+正 **`size`** → **`EFAULT`**；缓冲过短仍 **`ERANGE`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-192 resolved（**`ioctl` `TIOCGWINSZ`+`NotATty`**：注释澄清 **`inspect_err`** 仅减 **`warn`**，**syscall 仍失败**/**`ENOTTY`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-203 resolved（**`membarrier`**：**`QUERY` `SUPPORTED_COMMANDS`** 去掉 **`GLOBAL*`**/**`REGISTER_GLOBAL_EXPEDITED`**；仅宣称 **`PRIVATE_*` + 对应 `REGISTER_*`**；**`GLOBAL*`** → **`EINVAL`**；删 **`membarrier_reg_global_expedited`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-200 resolved（**`get_mempolicy`**：**`linux-raw-sys` `mempolicy`** **`MPOL_F_*`** 掩码 + **`addr`/`flags`** 组合校验；**`MPOL_F_ADDR`** 要求 **`AddrSpace::find_area`**；非法 → **`EINVAL`/`EFAULT`**；仍为 **`MPOL_DEFAULT` stub**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-199 resolved（**`IoVectorBuf`**：**`Vec<IoVec>`** 快照用户 **`iovec`**；**`read_with`/`IoVectorBufIo`** 不再对用户表二次 **`vm_read`**；对齐 **`import_iovec`** 固定描述；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-198 resolved（**`recvmsg`**：**`msghdr`** 快照 + **`offset_of!` `UserPtr`** 写 **`msg_namelen`/`msg_controllen`/`msg_flags`**；**`CMsgBuilder`**用 **`UserPtr<usize>`**替代 **`&mut msg.msg_controllen`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-197 resolved（**`sendmsg`**：**`*msg.get_as_ref()?`** 整 **`msghdr`** 拷内核栈后再解析 **`msg_control`**与组 **`IoVectorBuf`/地址**；缓解 **`msghdr` 字段级 TOCTOU**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-196 resolved（**`renameat2` `RENAME_EXCHANGE`**：**`ctl.rs`** 文档化 **三次 `rename` + 临时名** 实现 **非 crash-atomic**、与 Linux **VFS 交换** 故障语义可能不同；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-195 resolved（**`sendmsg` `msg_control`**：**`cmsghdr`** 内核栈快照 + **`CMsg::parse(&hdr, ptr)`** 用快照 **`cmsg_len`** 与用户 **`ptr` 取 payload**；消除 **`cmsg_len` TOCTOU**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-191 resolved（**`getsockopt` `TCP_INFO`**：**`GetSocketOption::TcpInfo(&mut [u8])`** + **`sys_getsockopt`** 按 **`sizeof(tcp_info)`** 填 **`optval`**/**`*optlen`** 仅在 **`get_option_inner` 成功**；**`axnet` `TcpSocket`** 仍 **`ENOPROTOOPT`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-193 resolved（**`umask`**：**`ProcessData::UMASK_PERM_MASK`（`0o777`）**；**`replace_umask`**存 **`mask & 0o777`**、返 **`old & 0o777`**；**`set_umask`** 同截断；对齐 Linux **`open`/`mkdir`** 的 **`mode & !umask`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-190 resolved（**`prlimit64`**：用户 **`new_limit`** 两次 **`u64::vm_read()`** + **`rlimit64 { .. }`**，去掉 **`assume_init`/`FIXME: AnyBitPattern`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-188 resolved（**`cmsg_align`**：**`checked_add(align - 1)`** 溢出 → **`InvalidInput`**；**`CMsgBuilder::push` `remaining`** 改为 **`capacity - written`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-189 resolved（**`recvmsg` `msg_controllen`**：**`CMsgBuilder`** 内 **`written`** 累计，**`commit()`** 仅在 **`recv` 成功**且 cmsg 处理完后写回用户；失败路径不再提前 **` *len = 0`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-185 resolved（**`setsockopt`/`getsockopt` `IP_TTL`**：**`conv::IpTtl`** 要求 **`optlen >= sizeof(i32)`**，读 **`i32`** 后 **`u8::try_from`**；**`IP_TTL`** 显式分支（不经 **`call_dispatch` `$conv:ty`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-184 resolved（**`getrusage`**：**`From<Rusage> for rusage`** 全字段字面量，去掉 **`mem::zeroed`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-187 resolved（**`mount`**：**`validate_mount_source`**约束 **`tmpfs`** 的 **`source`**；非法路径 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-186 resolved（**`sendmsg`**：**`msg_control` 区间** **`ptr_end`**与循环上界均 **`checked_add`**，防 **`usize` 溢出**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-180 resolved（**`clock_nanosleep`**：**`flags & !TIMER_ABSTIME`** → **`EINVAL`**，读 **`req`** 前校验；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-176 resolved（**`prctl` `PR_SET_MM`**：已知 **`PR_SET_MM_*`** → **`EPERM`**，否则 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-183 resolved（**`recvmsg` `SCM_RIGHTS`**：按缓冲区容量安装 fd，溢出部分显式 **`drop`**，**`MSG_CTRUNC`** 当 **`fds` 未全写入**；**`CMsgBuilder::remaining`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-182 resolved（**`fcntl` `F_DUPFD`/`F_DUPFD_CLOEXEC`**：**`add_file_like_at_least`** 尊重 **`arg`** 为最小 fd；负/`>= AX_FILE_LIMIT` **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-175 resolved（**`statfs`/`fstatfs`**：辅助函数 **`statfs { … }`** 显式初始化含 **`f_spare`**，去掉 **`mem::zeroed`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-181 resolved（**`fcntl`**：未实现 **`cmd`** 默认臂 **`InvalidInput`（`EINVAL`）** 替代 **`Ok(0)`**，避免假成功；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-179 resolved（**`getrandom`**：**`GRND_INSECURE`** → 强制 **`/dev/urandom`**；**`GRND_RANDOM`|`GRND_NONBLOCK`** 在 **`CRNG_INITIALIZED`** 前 **`EAGAIN`**，成功读后置位；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-178 resolved（**`close_range` `CLOSE_RANGE_UNSHARE`**：**`FD_TABLE.read().clone()`** 快照 + **`Arc::new(RwLock::new(..))`** 写回 **`scope`**，修复 **`mem::take`** 丢表；避免同 **`Arc` 上 `write`+`read` 死锁；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-177 resolved（**`umount2`**：**`UMOUNT_NOFOLLOW`** → **`resolve_no_follow`**；**`MNT_FORCE`/`MNT_DETACH`/`MNT_EXPIRE`** → **`EOPNOTSUPP`**，避免静默忽略；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-174 resolved（**`recvmsg`**：**`msg_flags`** **`MSG_TRUNC`**/**`MSG_CTRUNC`**；**`axnet`** **UDP/dgram** **`RecvOptions.msg_trunc`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-173 resolved（**`madvise`**：**`WILLNEED`/`POPULATE_*` → `populate_area`**；**`NORMAL`/`RANDOM`/`SEQUENTIAL`** no-op；其余白名单 → **`EOPNOTSUPP`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-172 resolved（**`clock_getres`**：**`*_COARSE` → `1 ms`**，其余已支持 clock → **`1 ns`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-171 resolved（**`readlinkat`**：**`dirfd_for_path_resolution`** +相对路径才 **`from_fd`**，与 **issue-169/**`mkdirat` 一致；实现见于 **`72a8e16`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-170 resolved（**`SO_ERROR`**：**`GeneralOptions` `AtomicI32` + **`swap` 读清**；**`TcpSocket`** 连接失败 **`record_so_error(ECONNREFUSED)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-169 resolved（**绝对路径忽略 `dirfd`**：**`dirfd_for_path_resolution`** + **`resolve_at`/`openat`**；**`*at`** 路径 syscall 相对路径才 **`Directory::from_fd`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-168 resolved（**Unix `SO_PASSCRED`**：**`axnet-ng`** **`StreamTransport`/`DgramTransport`** 增加 **`pass_cred`**，`get`/`set` **`PassCredentials`** 读写；**`accept`** 继承监听端；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-167 resolved（**`getsockopt` `TCP_INFO`**：**`TcpSocket`** **`TcpInfo` → `Ok(false)`/`ENOPROTOOPT`**；**`sys_getsockopt`** 提前分支避免 **`()` 选项把 `*optlen` 置 0**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-166 resolved（**`fspick`/`open_tree`** stub：**`dfd != AT_FDCWD`** 先 **`<Directory as FileLike>::from_fd`** 再 **`get_as_str`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-165 resolved（**`renameat2`**/**`renameat`**：**`flags`** 后 **`old_dirfd`** → **oldname**，再 **`new_dirfd`** → **newname**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-164 resolved（**`linkat`**：先 **`old_dirfd`**（**`old_path` 非 NULL**）再读 **oldname**，再 **`new_dirfd`** 后读 **newname**；**`AT_EMPTY_PATH` + NULL old_path** 不预检 **olddfd**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-163 resolved（**`fchownat`/`fchmodat`**/**`update_times`**：**`dirfd != AT_FDCWD` 且 `path` 非 NULL** 时先 **`Directory::from_fd`** 再读路径；**`AT_EMPTY_PATH` + NULL path** 不预检；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-162 resolved（**`readlinkat`**：**`size==0`** **`EINVAL`** 后、**`vm_load_string`** 前 **`dirfd != AT_FDCWD`** → **`Directory::from_fd`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-161 resolved（**`unlinkat`**：**`flags`** 校验后、**`vm_load_string`** 前 **`dirfd != AT_FDCWD`** → **`Directory::from_fd`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-160 resolved（**`mkdirat`**：**`dirfd != AT_FDCWD`** 时先 **`Directory::from_fd`** 再 **`vm_load_string(path)`**，**`EBADF`** 先于路径读；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-159 resolved（**`syncfs`**：复用 **`location_from_fd`**，**`MemfdCreatedFile`**/**memfd** **fd** 可 **`filesystem().flush()`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-158 resolved（**`symlinkat`**：**`new_dirfd != AT_FDCWD`** 时先 **`Directory::from_fd`** 再 **`vm_load_string`**，**`EBADF`** 先于路径读；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-157 resolved（**`fstatfs`**：**`location_from_fd`** 支持 **`Directory`**/**`MemfdCreatedFile`**/**`File`** 取 **`Location`**再 **`statfs`**，对齐 Linux目录 **fd**；非 VFS **fd** → **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-156 resolved（**`listen`**：**`SocketOps::listen(backlog)`**；TCP **`LISTEN_TABLE`** 按 **`min(backlog, SOMAXCONN)`** 限制 **`syn_queue`**；**`backlog < 0`** → **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-155 resolved（**`statx`**：**`Kstat::into_statx_with_mask`**按 **`STATX_*`** 填 **`struct statx`** 与 **`stx_mask`**；**`sys_statx`** 使用用户 **`mask`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-154 resolved（**`msgsnd`**：去掉 **`msg_qnum+1` vs `msg_qbytes`** 错误比较；满队列仅按 **`total_bytes + msgsz` vs `msg_qbytes`**，与 **`enqueue_message`**/**Linux** 字节配额一致；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-153 resolved（**`setitimer`**：**`new_value==NULL`** 不调用 **`set_itimer`**，仅 **`old_value`** 非空时 **`get_itimer`** 写回；两参皆 **`NULL`** 为 no-op；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-152 resolved（**`shmctl` `IPC_STAT`**：**`buf`** 必填可写 **`shmid_ds`**，**`NULL`** → **`BadAddress`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-151 resolved（**`getrandom`**：**`GRND_FLAGS_MASK`** 校验先于 **`len==0`** **`Ok(0)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-150 resolved（**`socketpair`**：第二端 **`add_to_fd_table`** 失败时 **`close_file_like(fd1)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-149 resolved（**`accept4`/`accept`**：**`addr` 非空**时先 **`write_to_user`** 再 **`add_to_fd_table`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-148 resolved（**`pipe2`**：**`fds.vm_write`** 失败时 **`close_file_like`** 读/写两端；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-147 resolved（**`rt_sigaction`**：**`act` 非空**时先 **`vm_read(act)`** 再 **`vm_write(oldact)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-146 resolved（**`rt_sigprocmask`**：**`set` 非空**时先 **`how`**/**`vm_read(set)`**，再 **`vm_write(oldset)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-145 resolved（**`io_uring_setup`**：**`add_file_like`** 成功后再 **`params.vm_write`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-144 resolved（**`epoll_ctl`** **`ADD`/`MOD`**：**`get_file_like(fd)`** 先于 **`parse_event`/`event.get_as_ref`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-143 resolved（**`Directory`** **`FileLike::read`/`write`** → **`IsADirectory`**（**EISDIR**），对齐 Linux目录 fd；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-142 resolved（**`sched_setaffinity`**/**`sched_setscheduler`**：**`sched_resolve_task(pid)`** 先于 **`vm_load`**/**`vm_read_uninit(param)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-141 resolved（**`prlimit64`**：内核内快照 **`old`** →读/校验/应用 **`new_limit`** → 再 **`vm_write(old_limit)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-140 resolved（**`readlink`**/**`readlinkat`**：**`bufsiz==0`** → **`InvalidInput`**（**EINVAL**），在 **`vm_load_string(path)`** 与 VFS 解析前；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-139 resolved（**`fchownat`**/**`fchmodat`**：**`VALID_FCHOWNAT_FLAGS`**/**`VALID_FCHMODAT_FLAGS`** 先于 **`vm_load_string(path)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-138 resolved（**`unlinkat`**/**`renameat2`**：**`flags`** / **`RENAME_*`** 校验先于 **`vm_load_string`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-137 resolved（**`linkat`**：**`VALID_LINKAT_FLAGS`** + **`resolve_flags`** 先于 **`vm_load_string`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-136 resolved（**`fcntl`** 记录锁 **`F_SETLK`/`F_GETLK`** 与 **`F_OFD_*`**：**`get_file_like(fd)`** 先于用户 **`flock64`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-135 resolved（**`recvmsg`**：**`validate_recvmsg_flags`** + **`Socket::from_fd`** 先于 **`msghdr`** 与 **`IoVectorBuf`**；**`recv_on_socket`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-134 resolved（**`sendmsg`**：**`Socket::from_fd`** 与 **`SENDMSG_FLAGS_MASK`** 先于 **`msghdr`**/cmsg/iovec；**`send_on_socket`** 复用；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-133 resolved（**`sendto`/`sendmsg`**：**`send_impl`** 内 **`Socket::from_fd`** 先于 **`SocketAddrEx::read_from_user`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-132 resolved（**`bind`/`connect`**：**`Socket::from_fd`** 先于 **`SocketAddrEx::read_from_user`**，**`EBADF`** 先于 **EFAULT** 类；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-131 resolved（**`fstatat`/`newfstatat`**：**`VALID_NEWFSTATAT_FLAGS`** + **`AT_STATX_SYNC_TYPE`** 互斥先于 **`vm_load_string(path)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-130 resolved（**`fsync`/`fdatasync`**：**`get_file_like`** 后仅 **`File`**/**`MemfdCreatedFile`** 调 **`sync`**，否则 **`InvalidInput`**（**EINVAL**），对齐 Linux **pipe/socket** 等；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-129 resolved（**`faccessat2`**：**`VALID_FACCESSAT_FLAGS`** 与 **`VALID_ACCESS_MODE`** 先于 **`vm_load_string(path)`**（issue-128 掩码语义不变）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-128 resolved（**`faccessat2`**：**`mode`** 须为 **`F_OK|R_OK|W_OK|X_OK`** 子集（**`VALID_ACCESS_MODE`**），先于 **`resolve_at`**，非法位 **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-127 resolved（**`statx`**：**`flags`** 掩码/互斥先于 **`vm_load_string(path)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-126 resolved（**`truncate`**：**`length < 0`** 先于 **`path.get_as_str`**，对齐 Linux **EINVAL** 前序；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-125 resolved（**`pwrite64`**：**`len==0`** 仍 **`File::from_fd`**，无效 fd → **EBADF**，对齐 Linux；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-124 resolved（**`sendfile`**：**`offset` 非空**时 **`File::from_fd(in_fd)`** 先于 **`vm_read`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-123 resolved（**`splice`**：**`off_in`/`off_out` 非空**时 **`File::from_fd`** 先于 **`vm_read`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-122 resolved（**`getsockopt`**：**`from_fd`** 先于 **`optlen.get_as_mut`**，**`EBADF`** 先于 **EFAULT** 类；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-121 resolved（**`make_queue_signal_info`**：**`signo!=0`** 且 **`sig==NULL`** → **`BadAddress`**，在 **`vm_read_uninit`** 前；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-120 resolved（**`getrusage`**：先 **`who`** **`EINVAL`**，再 **`usage==NULL` → `BadAddress`**，后聚合 **`Rusage`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-119 resolved（**`getsockname`/`getpeername`**：**`addrlen.get_as_mut`** 早于 **`local_addr`/`peer_addr`**，对齐 **EFAULT** 前序；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-118 resolved（**`rt_sigqueueinfo`/`rt_tgsigqueueinfo`**：对齐 Linux **3/4 参**，去掉误用的 **`sigsetsize`/`check_sigset_size`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-117 resolved（**`rt_sigtimedwait`/`rt_sigsuspend`**：必填 **`set==NULL`** → **`BadAddress`**，在 **`vm_read_uninit`** 前；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-116 resolved（**`signalfd4`**：**`mask==NULL`** → **`BadAddress`**，在 **`vm_read_uninit`** 前；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-115 resolved（**`rt_sigpending`**：**`set==NULL`** → **`BadAddress`**（**EFAULT**），在 **`check_sigset_size`** 后、**`vm_write`** 前；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-114 resolved（**`fadvise64`** 桩：**`offset`/`len` 非负** + **`checked_add`** 溢出 **`InvalidInput`**，与 **`fallocate`** 一致；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-113 resolved（**`timerfd_gettime`**：**`curr_value==NULL`**在 **`from_fd`** 后、**`gettime`/`vm_write`** 前 **`BadAddress`**（**EFAULT**），与 **`settime`** 对 **`new_value`** 的显式校验对称；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-112 resolved（**`io_uring_setup`**：成功写回 **`IoUringParams`** 前 **`sq_off`/`cq_off`/`resv`/线程字段** 清零，注释标明 **无 ring mmap布局**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-111 resolved（**`membarrier`**：**`ProcessData` 三注册位**；**`GLOBAL_EXPEDITED`/`PRIVATE_*`** 须先 **`REGISTER_*`** 否则 **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-110 resolved（**`timerfd_settime`**：**`flags` 仅允许 `TFD_TIMER_ABSTIME`**，未知位 **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-109 resolved（**`pidfd_open`**：**`pid==0` → `InvalidInput`**，对齐 Linux **EINVAL**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-108 resolved（**`sys_pwrite64`**：**`offset < 0` → `InvalidInput`**，对齐 **`pread64`** / **`pwritev2`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-107 resolved（**`sys_fallocate`**：**`offset`/`len` 非负** + **`checked_add`** 算 **`end`**，溢出 **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-106 resolved（**`sys_ftruncate`**：**`length < 0` → `InvalidInput`**，对齐 **`sys_truncate`** / Linux **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-105 resolved（**`check_sigset_size`**：**`sigsetsize` 须严格 `sizeof(SignalSet)`**，**`0` 与错误长度 → `InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-103 resolved（**`recvfrom`/`recvmsg`**：**`RecvFlags`对齐 Linux `MSG_*`**、**`recv_poller` `MSG_DONTWAIT`**；**`MSG_WAITALL`** 在 **TCP/vsock/Unix stream**；**`MSG_OOB`/`ERRQUEUE`/…** **`Unsupported`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-104 resolved（**`pwritev`/`pwritev2`**：**`write_at` + `IoVectorBuf::into_io()`**；**`preadv2`/`pwritev2`**：**`offset < 0` → `InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-102 resolved（**`sendto`/`sendmsg`**：**`SendFlags::from_bits_truncate`**；**vendor `axnet-ng`**：**`SendFlags`** 对齐 Linux **`MSG_*`**，**`send_poller`** 合并 **`MSG_DONTWAIT`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-16：issue-101 resolved（**`exit`/`exit_group`**：**`(exit_code & 0xff) << 8`** 再 **`do_exit`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-15：issue-100 resolved（**`capget`/`capset` V3**：**2×`__user_cap_data_struct`**，高位槽 0 / **`capset`** 校验；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-14：issue-099 resolved（**`prctl(PR_SET_NAME)`**：**`UserConstPtr` 读 ≤16 字节**须含 **NUL**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-14：issue-098 resolved（**`sigaltstack`**：**`ss.size < MINSIGSTKSZ` → `InvalidInput`**，非 **`NoMemory`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-097 resolved（**`get_mempolicy`**：**`nodemask` 非空且 `maxnode==0` → `InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-096 resolved（**`clone3`**：**`size`** 上限 **`min(buffer.len())`**，避免 **`buffer[..size]`** panic；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-095 resolved（**`execve`**：多线程等兄弟退出 — **`thread_group_exit_event` + `block_on(interruptible)`**；去 **`WouldBlock`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-094 resolved（**`clone`/`clone3`**：**`CLONE_PARENT_SETTID`** **`parent_tid` `vm_write` `?`**，与 **`PIDFD`** 一致；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-093 resolved（**`setpgid`**：仅调用者或子进程、子须同 session；**`zombie` → `NoSuchProcess`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-092 resolved（**`with_blocked_signals`**：**`f()`** **`Ok`/`Err`** 均恢复 **`sigmask`**；**`ppoll`/`pselect`/`epoll_pwait`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-091 resolved（**`getresuid`/`getresgid`**：**`NULL`** 输出指针按字段跳过写入；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-090 resolved（**`fill_addr`**：**`*addrlen==0` → `InvalidInput`**；**`getsockname`/`getpeername`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-089 resolved（**`brk`**：无效 **`addr`** → **`InvalidInput`/`NoMemory`**；**`map`/`unmap`** 失败 **`?`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-088 resolved（**`getppid`**：无 **`parent`** 时 **`0`**，与 **`TaskStat`/proc ppid** 一致；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-087 resolved（**`waitpid`/`wait4`**：**`__WALL`/`__WCLONE`**与 **`ProcessData::is_clone_child`** 过滤子集合；**`__WNOTHREAD`** 文档化未实现；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-086 resolved（**`gettimeofday`/`times`**：输出 **`NULL`** 时跳过 **`vm_write`**，对齐 Linux；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-085 resolved（**`lseek`**：**`SEEK_DATA`/`SEEK_HOLE`（3/4）** 稠密文件语义 + **`ENXIO`**；未跟踪真实稀疏 extent；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-084 resolved（**`getrusage`**：**`RUSAGE_SELF`/`RUSAGE_THREAD`**填 **`ru_maxrss`**（**`AddrSpace::resident_set_size_kb`**，当前常驻快照）；**`Rusage`** 注释未实现域；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-083 resolved（**`mincore`**：**`length==0`** 在 **`vec` NULL 检查**之前 **`Ok(0)`**，对齐 Linux/POSIX no-op；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-082 resolved（**`futex`** **`FUTEX_REQUEUE`/`CMP_REQUEUE`**：**`wake` 后无条件对剩余等待者 `requeue`（上限 `nr_requeue`）**；返回 **`woke + requeued`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-081 resolved（**`select`/`pselect6`** **`do_select`**：输出 **`fd_set`** 先在内核缓冲中构建，**仅在 `with_blocked_signals` 成功返回后**拷贝到用户；阻塞期间保留用户侧输入位图；空 **`FdPollSet`** 立即写回零并 **`Ok(0)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-080 resolved（**`poll`/`ppoll`** **`do_poll`**：不再因 **`POLLNVAL` 早退**；**`nval_ready` + `poll_io`**；**`fd_indices`** 写 **`revents`**；超时按全表 **`revents`** 计数；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-079 resolved（**`getcwd`**：**`buf==NULL`** 时 **`size>0` → `BadAddress`**、**`size==0` → `OutOfRange`**；成功返回**写入长度（含 NUL）**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-078 resolved（**`epoll_ctl`** **`parse_event`**：**`KNOWN_EPOLL_EVENTS_MASK`**（**`IoEvents` ∪ `EpollFlags` ∪ `EPOLLEXCLUSIVE`/`EPOLLWAKEUP`**）未知位 **`InvalidInput`**；**`EPOLLEXCLUSIVE`/`EPOLLWAKEUP`** 剥离后 **`from_bits`** 拆分；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-077 resolved（**`memfd_create`/`memfd_secret`**：**`MemfdCreatedFile`**，**`path`** → **`/memfd:{name}`**；仍用 **`/tmp/memfd-*`** 作实际文件；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-076 resolved（**`poll`/`ppoll`** **`do_poll`**：**`fd < 0`** 时 **`revents = 0`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-075 resolved（**`sendmsg` `CMsg::parse`**：**`SOL_SOCKET`** 下 **`SCM_CREDENTIALS`/`SCM_TIMESTAMP*`/`SCM_SECURITY`** → **`Unsupported`**；**`SCM_RIGHTS`** 仍支持；其它未知 **`InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-074 resolved（**`addr.rs`** INET：**`addrlen >= sizeof(sockaddr_in|in6)`**，vsock：**`sockaddr_vm`** 同理；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-073 resolved（**`setsockopt`**：**`optlen >= sizeof(T)`** 即接受（与 **`getsockopt`** 一致），只读 **`sizeof(T)`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-072 resolved（**`socket`/`socketpair`**：**`type`** 须为 **`SOCK_TYPE_MASK|O_CLOEXEC|O_NONBLOCK`** 子集，否则 **`InvalidInput`**；**`ty = raw_ty & 0xf`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-071 resolved（**`nanosleep`/`clock_nanosleep`**：**`sleep_impl`** 文档化 **`axtask::sleep`** 与单调时间线一致；**`CLOCK_REALTIME`** 非零睡眠 → **`Unsupported`**，**`dur==0`** → **`Ok(0)`**；**`CLOCK_MONOTONIC`** 不变；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-070 resolved（**`waitpid`/`wait4`**：**`WaitOptions::from_bits(options).ok_or(InvalidInput)`**，勿 **`from_bits_truncate`**；未知 **`options` 位 → EINVAL**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-068 resolved（**`syslog`/`klogctl`**：**`SYSLOG_ACTION_*`** 分支；读/清/控制台 → **`Unsupported`**；**`SIZE_*` → 0**；**`OPEN`/`CLOSE` → Ok(0)**；未知 **`action` → `InvalidInput`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-067 resolved（**`mount`**：**`flags`** 须为 **0**（未实现 **`MS_*`**）；**`data`** 仅 **`NULL`** 或空 C 串；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-066 resolved（**`mmap`** **`flags`**：**`ALLOWED_MAP_FLAGS`**（uapi **`MAP_*`** + **`MAP_HUGE_*`** 域）未知位 **`InvalidInput`**；扩展 **`MmapFlags`** 使合法组合走 **`from_bits`**；**`from_bits` 失败** 时 **`SHARED_VALIDATE` 类型 → `OperationNotSupported`** 否则 **`InvalidInput`**，勿 **`from_bits_truncate`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-065 resolved（**`mmap`**：**`MmapProt::from_bits`** 与 **`mprotect`** 一致；**`PROT_GROWSDOWN`/`GROWSUP`** 在 **`mmap`** 拒绝；**`mprotect`** 用 **`intersects`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-064 resolved（**`copy_file_range`**：常规文件 + 同 inode 区间重叠 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-063 resolved（**`clone3`**：**`set_tid`**/**`set_tid_size`**/**`cgroup`** 非零 → **`InvalidInput`**，勿仅 **`warn`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-062 resolved（**`statfs`**/**`fstatfs`**：**`f_fsid`** 由 **`device`/`f_type`** 双字编码；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-060 resolved（**`fanotify_init`**：**`VALID_FANOTIFY_INIT_FLAGS`** + **`event_f_flags`** 对齐 **`FANOTIFY_INIT_ALL_EVENT_F_BITS`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-13：issue-069 resolved（**`clone`**/**`clone3`**：**`CLONE_NEW*`** → **`Unsupported`**，勿仅 **`warn`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-059 resolved（**`sync`**/**`syncfs`**：**`flush_mount_subtree`** + **`FilesystemOps::flush`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-058 resolved（**`shmat`**：**`SHM_RND`**/**`SHM_REMAP`**/`SHMLBA` 附着语义 + **`shmflg`** 掩码；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu -p starryos`** 通过）
- 日期：2026-04-12：issue-057 resolved（**`statx`**：**`flags`** 掩码 **`AT_EMPTY_PATH|AT_SYMLINK_NOFOLLOW|AT_STATX_SYNC_TYPE`** + **`FORCE`/`DONT` 互斥**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-056 resolved（**`accept4`**：**`flags`** 仅 **`O_CLOEXEC | O_NONBLOCK`**，未知位 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-061 resolved（**`msgsnd`/`msgrcv`**：**`MessageQueue`** 上 **`recv_notify`/`send_notify`** + **`block_on(interruptible)`** 阻塞与唤醒；**`IPC_RMID`** **`wake_waiters`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-055 resolved（**`madvise`**：**`KNOWN_MADV_ADVICE`**（**`linux_raw_sys` 全部 `MADV_*`**），未知 **`advice`** **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-054 resolved（**`faccessat2`**：**`VALID_FACCESSAT_FLAGS`**（**`AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH | AT_EACCESS`**），未知位 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-053 resolved（**`memfd_create`/`memfd_secret`**：**`validate_memfd_flags`**，低位 **`MFD_*`** + **`MFD_HUGE_*`** 离散编码；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-052 resolved（**`fchownat`**：**`flags`** 须为 **`VALID_FCHOWNAT_FLAGS`**（**`AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW`**），未知位 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-051 resolved（**`utimensat`**：**`path==NULL`** 时并入 **`AT_EMPTY_PATH`** 后，按 **`VALID_UTIMENSAT_FLAGS`**（**`AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH`**）校验，未知位 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-049 resolved（**`fchmodat`**：**`flags`** 须为 Linux **`VALID_FCHMODAT_FLAGS`**（**`AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW`**），未知位 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-048 resolved（**`unlinkat`**：**`flags`** 仅 **`0`** 或 **`AT_REMOVEDIR`**，否则 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-047 resolved（**`linkat`**：**`flags`** 须为 **`AT_EMPTY_PATH | AT_SYMLINK_FOLLOW`**（Linux **`VALID_LINKAT_FLAGS`**），未知位 **`EINVAL`**；传入 **`resolve_at`** 时将 **`FOLLOW`** 映射为 **`AT_SYMLINK_NOFOLLOW`** 语义；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-046 resolved（**`pipe2`**：**`PipeFlags::from_bits`**，未知位 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-045 resolved（**`getrusage(RUSAGE_CHILDREN)`**：**`waited_children_cpu_nanos`**，勿累加 **`proc.threads()`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-044 resolved（**`prlimit64`**：提高硬上限超过当前 **`limit.max`** → **`OperationNotPermitted`（EPERM）**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-043 resolved（**`ioctl(FIONBIO)`**：按 **`c_int`** 读用户参数，**`!= 0`** 即非阻塞；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-042 resolved（**`getrandom`**：**`flags`** 仅允许 **`GRND_NONBLOCK|GRND_RANDOM|GRND_INSECURE`**，否则 **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-040 resolved（**`fadvise64`**：管道 fd 返回 **`LinuxError::ESPIPE`**，勿用 **`BrokenPipe`→`EPIPE`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-16：issue-050 resolved（**`renameat2`**：**`flags`** 掩码 **`RENAME_NOREPLACE|EXCHANGE|WHITEOUT`**，未知位 **`EINVAL`**；**`NOREPLACE`** 目标存在 **`EEXIST`**；**`EXCHANGE`** 三次 **`rename`**；**`WHITEOUT`** 无 overlay **`EINVAL`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-13：issue-033 resolved（**`setitimer`/`getitimer`**：**`ITIMER_REAL`** 独占 wall **`alarm_task`**；**`last_wall_ns`** 初值单调时钟；**`ITIMER_VIRTUAL`/`PROF`** 仅 **`poll`** 推进；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-13：issue-031 resolved（**`ioctl`**：非字符设备上对 TTY 驱动 ioctl 提前 **`ENOTTY`**；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过）
- 日期：2026-04-12：issue-023 resolved（**`get_mempolicy`**：写入 **`MPOL_DEFAULT`** + 可选 **`nodemask`** 清零）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-12：issue-041 resolved（**`Socket::stat`**：**`st_ino`** 自增、**`st_dev`** 伪 sockfs；**`path`** **`socket:[ino]`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-12：issue-039 resolved（**`times(2)`**：**`tms_utime`/`tms_stime`** 为线程组累计；**`tms_cutime`/`tms_cstime`** 为已 **`wait`** 子进程 CPU；僵尸 **`ProcessData`** 表 + **`waitpid`** 累加）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-12：issue-038 resolved（**`recvfrom`/`recvmsg`/`sendto`/`sendmsg`**：**`MSG_*`** 掩码校验，非法位 **`EINVAL`**；**`SendFlags`** 仍待 **`axnet-ng`** 扩展后再透传）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-12：issue-037 resolved（**`splice`/`copy_file_range`**：**`flags`** 掩码校验，非法位 **`EINVAL`**；**`SPLICE_F_*`** 来自 **`linux_raw_sys`**，**`COPY_FILE_RANGE_*`** 对齐 **`uapi/linux/fs.h`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-035 resolved（**`clock_gettime`/`clock_getres`** 不支持 **`clock_id`** → **`EINVAL`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-032 resolved（**`mount`** **`fstype`** 白名单 + **`umount2`** **`flags`** 掩码）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-030 resolved（**`getpriority`** 返回 **`20-nice`**（Linux ABI）；**`setpriority`** 分发已存在）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-029 resolved（**`getresuid`/`getresgid`** syscall + **`ProcessData::get_resuid`/`get_resgid`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-036 resolved（**`prctl`** **`PR_SET_SECCOMP`**/**`PR_MCE_KILL`**：非法参数 **`EINVAL`**，未实现 **`Unsupported`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-027 resolved（**`sys_sysinfo`**：**`totalram`**/**`freeram`**/**`uptime`** 来自 **`axhal`/`axalloc`**）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-034 resolved（**`sendmsg`/`recvmsg`** **`CMSG_ALIGN`**；**`cmsg_align`** + **`CMsgBuilder`** 填充）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-021 resolved（**`capget`/`capset`** → **`ProcessData`** 三域 **`AtomicU32`**，**`copy_credentials_from`** 继承）；**`cargo clippy --target riscv64gc-unknown-none-elf -F qemu`** 通过
- 日期：2026-04-13：issue-020 resolved（**`mremap`** 保留 File/COW；**`msync`** → **`CachedFile::sync`**）

## 修复历史
| Issue ID | 标题 | 结果 | 日期 |
|----------|------|------|------|
| issue-152 | shmctl IPC_STAT buf 非 NULL | resolved | 2026-04-12 |
| issue-151 | getrandom len==0 仍先校验 flags | resolved | 2026-04-12 |
| issue-150 | socketpair 第二 fd 失败回滚第一端 | resolved | 2026-04-12 |
| issue-149 | accept4先写 sockaddr 再装 fd | resolved | 2026-04-12 |
| issue-148 | pipe2 vm_write 失败关闭两端 fd | resolved | 2026-04-12 |
| issue-147 | rt_sigaction oldact after vm_read(act) | resolved | 2026-04-12 |
| issue-146 | rt_sigprocmask oldset after how/set校验 | resolved | 2026-04-12 |
| issue-145 | io_uring_setup vm_write after add_file_like | resolved | 2026-04-12 |
| issue-144 | epoll_ctl ADD/MOD get_file_like 先于 parse_event | resolved | 2026-04-12 |
| issue-143 | 目录 fd read/write → EISDIR | resolved | 2026-04-12 |
| issue-142 | sched_setaffinity pid 解析先于 user_mask | resolved | 2026-04-12 |
| issue-141 | prlimit64 old 出参晚于 new 读/校验 | resolved | 2026-04-12 |
| issue-140 | readlinkat bufsiz==0 → EINVAL | resolved | 2026-04-12 |
| issue-139 | fchownat/fchmodat flags 先于 vm_load_string | resolved | 2026-04-12 |
| issue-138 | unlinkat/renameat2 flags 先于路径 | resolved | 2026-04-12 |
| issue-137 | linkat flags 先于 vm_load_string | resolved | 2026-04-12 |
| issue-136 | fcntl 记录锁 get_file_like 先于 flock 用户访问 | resolved | 2026-04-12 |
| issue-135 | recvmsg flags/from_fd 先于 msghdr/iov | resolved | 2026-04-12 |
| issue-134 | sendmsg from_fd 先于 copy msghdr/iov | resolved | 2026-04-12 |
| issue-133 | sendto/sendmsg send_impl from_fd 先于读 addr | resolved | 2026-04-12 |
| issue-132 | bind/connect from_fd 先于读 sockaddr | resolved | 2026-04-12 |
| issue-131 | fstatat/newfstatat flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-130 | fsync/fdatasync 非文件 fd → EINVAL | resolved | 2026-04-12 |
| issue-129 | faccessat2 flags/mode 先于读 path | resolved | 2026-04-12 |
| issue-128 | faccessat2 mode 非法位 EINVAL | resolved | 2026-04-12 |
| issue-127 | statx flags 校验先于读 path | resolved | 2026-04-12 |
| issue-126 | truncate 先校验 length 再读 path | resolved | 2026-04-12 |
| issue-125 | pwrite64 len==0 仍校验 fd | resolved | 2026-04-12 |
| issue-124 | sendfile 非空 offset 时 from_fd 先于 vm_read | resolved | 2026-04-12 |
| issue-123 | splice 非空 off_* 时 from_fd 先于 vm_read | resolved | 2026-04-12 |
| issue-122 | getsockopt from_fd 先于 optlen 用户访问 | resolved | 2026-04-12 |
| issue-121 | make_queue_signal_info NULL sig → BadAddress | resolved | 2026-04-12 |
| issue-120 | getrusage NULL usage → BadAddress | resolved | 2026-04-12 |
| issue-119 | getsockname/getpeername 先校验 addrlen 指针 | resolved | 2026-04-12 |
| issue-118 | rt_sigqueueinfo/tgsigqueueinfo 去掉 sigsetsize ABI | resolved | 2026-04-12 |
| issue-117 | rt_sigtimedwait/rt_sigsuspend NULL set → BadAddress | resolved | 2026-04-12 |
| issue-116 | signalfd4 NULL mask → BadAddress | resolved | 2026-04-12 |
| issue-115 | rt_sigpending NULL set → BadAddress | resolved | 2026-04-12 |
| issue-114 | fadvise64校验 offset/len 非负与 overflow | resolved | 2026-04-12 |
| issue-113 | timerfd_gettime NULL curr_value → BadAddress | resolved | 2026-04-12 |
| issue-112 | io_uring_setup 写回 params 清零 sq_off/cq_off/resv | resolved | 2026-04-12 |
| issue-101 | exit/exit_group mask status to low byte before <<8 | resolved | 2026-04-16 |
| issue-100 | capget/capset V3 two __user_cap_data_struct slots | resolved | 2026-04-15 |
| issue-099 | prctl PR_SET_NAME bounded 16-byte read + NUL | resolved | 2026-04-14 |
| issue-098 | sigaltstack small stack: InvalidInput not NoMemory | resolved | 2026-04-14 |
| issue-097 | get_mempolicy nodemask set requires maxnode > 0 | resolved | 2026-04-13 |
| issue-096 | clone3 clamp vm_read to sizeof(Clone3Args) | resolved | 2026-04-13 |
| issue-095 | execve wait siblings: PollSet not WouldBlock spin cap | resolved | 2026-04-13 |
| issue-094 | clone PARENT_SETTID ptid vm_write propagate Err | resolved | 2026-04-13 |
| issue-093 | setpgid caller/child same session; zombie ESRCH | resolved | 2026-04-13 |
| issue-092 | with_blocked_signals restore mask on Err too | resolved | 2026-04-13 |
| issue-091 | getresuid/getresgid NULL ptr per-field optional | resolved | 2026-04-13 |
| issue-090 | fill_addr addrlen 0 → EINVAL | resolved | 2026-04-13 |
| issue-089 | brk invalid addr / map fail → Err not Ok old | resolved | 2026-04-13 |
| issue-088 | getppid parent None → 0 not ESRCH | resolved | 2026-04-13 |
| issue-087 | waitpid WALL/WCLONE clone-child filter | resolved | 2026-04-13 |
| issue-086 | gettimeofday/times NULL out ptr skip write | resolved | 2026-04-13 |
| issue-085 | lseek SEEK_DATA/HOLE dense-file semantics | resolved | 2026-04-13 |
| issue-084 | getrusage ru_maxrss from AddrSpace RSS KiB | resolved | 2026-04-13 |
| issue-083 | mincore length 0 before vec NULL check | resolved | 2026-04-13 |
| issue-082 | futex REQUEUE wake then always requeue | resolved | 2026-04-13 |
| issue-081 | select fd_set kernel buffer, copy on return | resolved | 2026-04-12 |
| issue-080 | poll POLLNVAL no early return + timeout count | resolved | 2026-04-12 |
| issue-079 | getcwd NULL buf EFAULT/ERANGE + return length | resolved | 2026-04-12 |
| issue-078 | epoll_ctl events KNOWN_MASK + from_bits EINVAL | resolved | 2026-04-12 |
| issue-077 | memfd name → MemfdCreatedFile path /memfd: | resolved | 2026-04-13 |
| issue-076 | poll fd<0 清零 revents | resolved | 2026-04-13 |
| issue-075 | sendmsg cmsg 已知 SCM_* → Unsupported | resolved | 2026-04-13 |
| issue-074 | INET sockaddr addrlen >= sizeof struct | resolved | 2026-04-13 |
| issue-073 | setsockopt optlen >= sizeof(T) | resolved | 2026-04-13 |
| issue-072 | socket type SOCK_TYPE_MASK + SOCK_* flags | resolved | 2026-04-13 |
| issue-071 | nanosleep sleep_impl 单调；REALTIME → Unsupported | resolved | 2026-04-13 |
| issue-070 | waitpid/wait4 WaitOptions from_bits EINVAL | resolved | 2026-04-13 |
| issue-068 | syslog SYSLOG_ACTION_* 分支，勿恒 Ok(0) | resolved | 2026-04-13 |
| issue-067 | mount flags==0 + data NULL/empty | resolved | 2026-04-13 |
| issue-066 | mmap flags ALLOWED_MAP_FLAGS + 勿 from_bits_truncate | resolved | 2026-04-13 |
| issue-065 | mmap prot from_bits 与 mprotect 一致 | resolved | 2026-04-13 |
| issue-064 | copy_file_range 重叠/常规文件 EINVAL | resolved | 2026-04-13 |
| issue-063 | clone3 set_tid/cgroup 非零 EINVAL | resolved | 2026-04-13 |
| issue-062 | statfs/fstatfs f_fsid 双字编码 | resolved | 2026-04-13 |
| issue-060 | fanotify_init flags / event_f_flags 掩码 EINVAL | resolved | 2026-04-13 |
| issue-069 | clone/clone3 拒绝 CLONE_NEW*（Unsupported） | resolved | 2026-04-13 |
| issue-059 | sync/syncfs 调用 VFS flush 与嵌套挂载 | resolved | 2026-04-12 |
| issue-058 | shmat SHM_RND/SHM_REMAP 与 shmflg 掩码 | resolved | 2026-04-12 |
| issue-057 | statx AT_* / AT_STATX_* flags 校验 | resolved | 2026-04-12 |
| issue-056 | accept4 flags 仅 O_CLOEXEC|O_NONBLOCK | resolved | 2026-04-12 |
| issue-061 | SysV msgsnd/msgrcv 阻塞与唤醒 | resolved | 2026-04-12 |
| issue-055 | madvise 未知 advice EINVAL | resolved | 2026-04-12 |
| issue-054 | faccessat2 AT_* flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-053 | memfd_create MFD_* flags 校验 EINVAL | resolved | 2026-04-12 |
| issue-052 | fchownat AT_* flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-051 | utimensat AT_* flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-049 | fchmodat AT_* flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-048 | unlinkat flags 仅 0 或 AT_REMOVEDIR | resolved | 2026-04-12 |
| issue-047 | linkat AT_* flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-046 | pipe2 flags 未知位 EINVAL | resolved | 2026-04-16 |
| issue-045 | getrusage CHILDREN 非兄弟线程 | resolved | 2026-04-16 |
| issue-044 | prlimit64 提高硬上限 EPERM | resolved | 2026-04-16 |
| issue-043 | ioctl FIONBIO 读 c_int 非零语义 | resolved | 2026-04-16 |
| issue-042 | getrandom flags 未知位 EINVAL | resolved | 2026-04-16 |
| issue-040 | fadvise64 pipe → ESPIPE | resolved | 2026-04-16 |
| issue-050 | renameat2 flags 校验与 NOREPLACE/EXCHANGE | resolved | 2026-04-16 |
| issue-033 | setitimer 虚拟/剖析与 wall alarm 解耦 | resolved | 2026-04-13 |
| issue-031 | ioctl 非 TTY 终端命令 ENOTTY | resolved | 2026-04-13 |
| issue-023 | get_mempolicy 写入 MPOL_DEFAULT | resolved | 2026-04-12 |
| issue-041 | Socket fstat 唯一 st_ino | resolved | 2026-04-12 |
| issue-039 | times tms_cutime 子进程累计 | resolved | 2026-04-12 |
| issue-038 | recv/send MSG_* 掩码 EINVAL | resolved | 2026-04-12 |
| issue-037 | splice/copy_file_range flags 掩码 EINVAL | resolved | 2026-04-12 |
| issue-035 | clock_gettime 无效 clockid 回退墙钟 | resolved | 2026-04-13 |
| issue-032 | mount fstype / umount2 flags 校验 | resolved | 2026-04-13 |
| issue-030 | getpriority/setpriority nice ABI | resolved | 2026-04-13 |
| issue-029 | getresuid/getresgid 缺失 ENOSYS | resolved | 2026-04-13 |
| issue-036 | prctl SECCOMP/MCE 假成功 | resolved | 2026-04-13 |
| issue-027 | sysinfo totalram/uptime 等为零 | resolved | 2026-04-13 |
| issue-034 | sendmsg/recvmsg CMSG 对齐（多段 SCM_RIGHTS） | resolved | 2026-04-13 |
| issue-021 | capget 全 CAP / capset 空操作 | resolved | 2026-04-13 |
| issue-020 | mremap 丢失 MAP_SHARED 文件后端 | resolved | 2026-04-13 |
| issue-019 | madvise/msync/mlock 桩 | resolved | 2026-04-13 |
| issue-018 | sched_getscheduler RR 桩（同 issue-002） | resolved | 2026-04-13 |
| issue-008 | shared futex 全局 FutexTables Mutex | resolved | 2026-04-13 |
| issue-007 | ELF 加载器 Mutex 串行 execve | resolved | 2026-04-13 |
| issue-003 | getpriority 固定 nice / setpriority | resolved | 2026-04-13 |
| issue-002 | sched_get/setscheduler/getparam 桩 | resolved | 2026-04-13 |
| issue-001 | sched_get/setaffinity 仅当前任务 | resolved | 2026-04-13 |
| issue-028 | 多线程 execve WouldBlock | resolved | 2026-04-12 |
| issue-025 | 补充组 stub / seccomp 空成功 | resolved | 2026-04-12 |
| issue-024 | membarrier compiler_fence / cmd 编码 | resolved | 2026-04-12 |
| issue-022 | setuid/getuid 恒0 / setresuid 空操作 | resolved | 2026-04-12 |
| issue-014 | 挂载 API / memfd_secret dummy fd | resolved | 2026-04-12 |
| issue-013 | bpf/io_uring 等 dummy fd 误导 | resolved | 2026-04-12 |
| issue-010 | timerfd DummyFd / poll 永不触发 | resolved | 2026-04-12 |
| issue-005 | membarrier 非 QUERY 路径仅用 compiler_fence | resolved | 2026-04-12 |
| issue-006 | BLOCK_NEXT_SIGNAL_CHECK 全局 AtomicBool | resolved | 2026-04-12 |
| issue-011 | timerfd dummy fd / poll 永不就绪 | resolved | 2026-04-12 |
| issue-012 | inotify/fanotify dummy anon_inode | resolved | 2026-04-12 |
| issue-015 | POSIX timer_create 假成功 Ok(0) | resolved | 2026-04-12 |
| issue-016 | flock 恒 Ok(0) 无互斥 | resolved | 2026-04-12 |
| issue-017 | fcntl F_SETLK 记录锁 no-op | resolved | 2026-04-12 |
| issue-026 | SIGSTOP 误 exit / SIGCONT 空操作 | resolved | 2026-04-12 |
| issue-004 | aspace 全局 Mutex 串行化 | resolved | 2026-04-12 |
| issue-009 | Stop/CONT 作业控制（重复单） | resolved | 2026-04-12 |

## 当前卡点
（无）

## 代码知识积累
- membarrier：Linux `cmd`（除 `QUERY=0`）为 **单位掩码**（`GLOBAL=1<<0`、`GLOBAL_EXPEDITED=1<<1`、`REGISTER_GLOBAL_EXPEDITED=1<<2`…），非单 bit 组合须 **`EINVAL`**；`QUERY` 返回 **已实现命令的按位或**。执行类命令用 **`atomic::fence(SeqCst)`**（勿用 `compiler_fence`）；`PRIVATE_EXPEDITED_SYNC_CORE` 在 **riscv64** 上额外 **`fence.i`**。`REGISTER_*` 仅占位返回0。真 **`GLOBAL` 全系统** 语义需 IPI（当前仅本 hart 最强屏障）。
- 全核 membarrier（多 hart）在 Linux 上依赖 IPI；若未来启用 `axfeat/smp` + `axfeat/ipi`，可在各核 IPI handler 中执行与 `sys_membarrier` 相同的 fence，并用同步原语等待全部完成。
- `rt_sigreturn` 通过 `block_next_signal` 标记「下一次回到用户循环时跳过一次 `check_signals`」；该标志必须是 **per-thread**（`Thread::skip_next_signal_check`），不可用进程级或全局 AtomicBool。
- **`rt_sigpending`**：**`set`** 为必填输出（Linux **`NULL` → EFAULT**），**`set.is_null()` → `BadAddress`**，须在 **`vm_write`** 前校验。**`rt_sigprocmask(2)`**：**`set==NULL`** 时仅 **`copy_to_user(old)`**（若 **`oldset` 非空**），**`how`** 忽略；**`set` 非空**时须先校验 **`how`**（**`SIG_BLOCK`/`SIG_UNBLOCK`/`SIG_SETMASK`**）再 **`vm_read(set)`**，成功后再 **`vm_write(oldset)`** 与 **`set_blocked`**，非法 **`how`** 或 **`set`** 读失败不得改写 **`oldset`**（issue-146）。**`rt_sigaction(2)`**：持锁后先 **`clone`** 当前表项为 **`old`**；**`act` 非空**时先 **`vm_read(act)`** 并写回 **`actions[signo]`**，再 **`vm_write(oldact)`**（**`old.into()`**）；**`act`** 读失败不得改写 **`oldact`**（issue-147）。**`act==NULL`** 时仅 **`vm_write(oldact)`** 查询。**`oldact`** 仍为 **`nullable()`**。
- **`signalfd4`**：**`mask`** 为必填输入（Linux **`NULL` → EFAULT**），**`mask.is_null()` → `BadAddress`**，须在 **`vm_read_uninit`** 前校验。
- **`rt_sigtimedwait`/`rt_sigsuspend`**：**`set`** 为必填可读 **`sigset_t`**（**`NULL` → EFAULT**），**`set.is_null()` → `BadAddress`**，须在 **`vm_read_uninit`** 前校验；**`timeout`/`info`** 等仍 **`nullable()`**。
- **`rt_sigqueueinfo`/`rt_tgsigqueueinfo`**：Linux 分别为 **3/4 个**用户参数（**`uinfo`** 为最后一参），**无** **`sigsetsize`**；勿从 **`a3`/`a4`** 读 **`sigsetsize`** 或调用 **`check_sigset_size`**（残留寄存器会误 **EINVAL**）。**`rt_sig*`** 中带 **`sigsetsize`** 的是 **`rt_sigprocmask`**、**`rt_sigaction`**、**`rt_sigpending`**、**`rt_sigtimedwait`**、**`rt_sigsuspend`** 等，勿与 queueinfo 混淆。**`make_queue_signal_info`**：**`signo==0`** → **`Ok(None)`**；否则 **`parse_signo`** 后 **`uinfo==NULL` → `BadAddress`** 再 **`vm_read`**（**`pidfd_send_signal`** 同路径）。
- timerfd：`TimerFd` 实现 `FileLike` + `Pollable`；到期逻辑在 `process_expirations` 中根据时钟纳秒与 `next_deadline_nanos` 比较；通过 `axtask::register_timer_callback`（首次创建时注册）在每次内核 timer tick 中扫描弱引用列表并 `wake` `PollSet`；创建 fd 用 `add_file_like`（与 eventfd2 相同），勿对 `Arc<TimerFd>` 误用 `add_to_fd_table(self)`。**`timerfd_settime`**：**`new_value.is_null()` → `InvalidInput`**。**`timerfd_gettime`**：**`curr_value.is_null()` → `BadAddress`**（Linux **EFAULT**），在 **`from_fd`** 之后、**`gettime`/`vm_write`** 之前校验，避免先算定时器再因用户指针失败。
- **`mmap(2)` `flags`**：先 **`flags & !ALLOWED_MAP_FLAGS`**（**`MAP_TYPE|MAP_FIXED|MAP_ANONYMOUS|…|MAP_DROPPABLE|(MAP_HUGE_MASK<<MAP_HUGE_SHIFT)`** 等 uapi 位），非零 → **`InvalidInput`**；**`MmapFlags::from_bits(flags)`**，**`None`** 时 **`MAP_SHARED_VALIDATE` 类型 → `OperationNotSupported`**，否则 **`InvalidInput`**；勿在未知/非法组合上 **`from_bits_truncate`**。合法 **`MAP_HUGE_*`** 等须在 **`MmapFlags`** 中声明以便 **`from_bits`** 成功。
- **`pipe2`**：**`flags`** 仅允许 **`O_CLOEXEC`/`O_NONBLOCK`**（**`PipeFlags`**）；用 **`from_bits(...).ok_or(InvalidInput)`**，勿 **`from_bits_truncate`**（与 **`eventfd2`/`epoll_create1`** 一致）。**`add_to_fd_table`** 成功后若 **`fds.vm_write`** 失败（**EFAULT** 等），须 **`close_file_like`** 已安装的读/写 **fd**，勿留孤儿表项（issue-148）；第二端分配失败时仍应关闭已装读端，勿对 **`close_file_like`** **`unwrap`**。
- **`poll(2)`/`ppoll(2)`**：**`pollfd.fd < 0`** 的条目被忽略，**`revents`** 须置 **0**（勿保留陈旧位）。
- **`epoll_ctl(2)`**：**`EPOLL_CTL_ADD`/`MOD`** 须先 **`get_file_like(fd)`**（无效目标 fd → **EBADF**），再 **`event.get_as_ref`**/**`parse_event`**（**EFAULT**/**EINVAL** 类），与 Linux 顺序一致（issue-144）；**`events`** 未知位等仍按 issue-078 **`KNOWN_EPOLL_EVENTS_MASK`**。**`EPOLL_CTL_DEL`** 不读 **`event`**。
- **`readlink(2)`/`readlinkat(2)`**：**`bufsiz`**（**`size`**）须 **> 0**，否则 **`InvalidInput`**（**EINVAL**），须在 **`vm_load_string(path)`** 与 **`resolve_no_follow`/`read_link`** 之前校验（issue-140）；勿对 **`size==0`** 返回成功 **0**。
- **`linkat(2)`**：**`flags`** 仅允许 **`AT_EMPTY_PATH | AT_SYMLINK_FOLLOW`**（与 Linux **`VALID_LINKAT_FLAGS`**），否则 **`EINVAL`**；上述与 **`resolve_flags`**（**`FOLLOW` ↔ `NOFOLLOW`** 映射）须在 **`vm_load_string(old_path/new_path)`** 之前完成（issue-137，非法 flags 先 **EINVAL**）。**`resolve_at`** 仍用 **`AT_SYMLINK_NOFOLLOW`** 表示「不 follow」。
- **`unlinkat(2)`**：**`flags`** 仅 **`0`** 或 **`AT_REMOVEDIR`**，须在 **`vm_load_string(path)`** 之前校验，否则 **`EINVAL`**（issue-138）；勿将未知位当作「删文件」分支。
- **`fchmodat(2)`** / **`fchmod`**： **`flags`** 须为 Linux **`AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW`** 的子集（**`sys_fchmod`** 用 **`AT_EMPTY_PATH`**），否则 **`EINVAL`**；掩码须在 **`vm_load_string(path)`** 之前完成（issue-139，非法 flags 先 **EINVAL** 于 **EFAULT**）；勿未校验即传入 **`resolve_at`**。
- **`utimensat(2)`**：**`path==NULL`** 时逻辑上含 **`AT_EMPTY_PATH`**；**`flags`** 须为 **`AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH`** 的子集（与 Linux **`VALID_UTIMENSAT_FLAGS`**），在 **`update_times`/`resolve_at`** 前校验，非法位 **`EINVAL`**（含双 **`UTIME_OMIT`** 时亦应先拒绝非法 **`flags`**）。
- **`fchownat(2)`** / **`fchown`** / **`lchown`**：**`flags`** 须为 **`AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW`** 的子集（**`VALID_FCHOWNAT_FLAGS`**），否则 **`EINVAL`**；掩码须在 **`vm_load_string(path)`** 之前完成（issue-139）。**`sys_fchown`** 用 **`AT_EMPTY_PATH`**，**`lchown`** 用 **`AT_SYMLINK_NOFOLLOW`**。
- **`memfd_create(2)`/`memfd_secret(2)`**：**`flags`** 低位须为 **`MFD_CLOEXEC|ALLOW_SEALING|HUGETLB|EXEC|NOEXEC_SEAL`** 的子集；**`MFD_HUGE_MASK<<MFD_HUGE_SHIFT`** 域须为 **0** 或等于某一 **`linux_raw_sys::general::MFD_HUGE_*`**（勿仅用 **`(MFD_HUGE_MASK<<SHIFT)`** 整域放行，否则 **`0x80000000`** 等无效编码会误过）。**`name`** 经 **`UserConstPtr::get_as_str`** 读入，**`MemfdCreatedFile`** 的 **`FileLike::path`** 为 **`/memfd:{sanitized}`**（供 **`/proc/self/fd`** readlink）；**`tmpfs`** 上仍用唯一 **`/tmp/memfd-....`** 作真实 backing。
- **`faccessat2(2)`**：**`flags`** 须为 **`AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH | AT_EACCESS`** 的子集（**`VALID_FACCESSAT_FLAGS`**），否则 **`EINVAL`**；**`mode`** 须为 **`F_OK|R_OK|W_OK|X_OK`** 子集（**`linux_raw_sys`** 上 **`F_OK==0`**，与 Linux **`~(F_OK|R_OK|W_OK|X_OK)`** 掩码一致），否则 **`EINVAL`**（issue-128）。上述 **`flags`/`mode`** 须在 **`vm_load_string(path)`** 与 **`resolve_at`** 之前完成（issue-129，非法参数先 **EINVAL** 于 **EFAULT**）；**`AT_EACCESS`** 与 **`resolve_at`** 语义可仍简化，但须先拒绝未知位。
- **`fstatat(2)`/`newfstatat(2)`**（**`sys_fstatat`**）：**`flags`** 须为 **`AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT | AT_STATX_SYNC_TYPE`** 的子集（Linux **`VALID_NEWFSTATAT_FLAGS`**）；**`AT_STATX_FORCE_SYNC`** 与 **`AT_STATX_DONT_SYNC`** 不能同时置位；须在 **`vm_load_string(path)`** 之前校验（issue-131）。**`AT_NO_AUTOMOUNT`** 等可仍为 no-op，但未知位须 **`EINVAL`**。**`resolve_at`** 仍只消费 **`AT_EMPTY_PATH`**/**`AT_SYMLINK_NOFOLLOW`**。
- **`statx(2)`**：**`flags`** 须为 **`AT_EMPTY_PATH | AT_SYMLINK_NOFOLLOW | AT_STATX_SYNC_TYPE`** 的子集；**`AT_STATX_FORCE_SYNC`** 与 **`AT_STATX_DONT_SYNC`** 不能同时置位（**`(flags & AT_STATX_SYNC_TYPE) == AT_STATX_SYNC_TYPE`** → **`EINVAL`**）；上述须在 **`vm_load_string(path)`** 之前完成（issue-127，非法 flags 先 **EINVAL**）；再 **`resolve_at`**。
- **SysV `msgsnd`/`msgrcv`**：**`MessageQueue`** 含 **`recv_notify`/`send_notify`**（**`event_listener::Event`**）；满且非 **`IPC_NOWAIT`** 时 **`msgsnd`** 在 **`send_notify`** 上阻塞，空且非 **`IPC_NOWAIT`** 时 **`msgrcv`** 在 **`recv_notify`** 上阻塞（**`block_on(interruptible(listener))`**）；入队后 **`recv_notify.notify`**，出队后 **`send_notify.notify`**；**`msgctl(IPC_RMID)`** 置 **`mark_removed`** 后 **`wake_waiters`**。信号 **`EINTR`** 依赖 **`interruptible`**。
- **SysV `shmctl(IPC_STAT)`**：**`buf`** 须为可写 **`shmid_ds`**（**`UserPtr::get_as_mut`**），**`NULL`** → **`BadAddress`**（**EFAULT**），勿 **`nullable!`** 跳过拷贝仍 **`Ok(0)`** 并更新 **`shm_ctime`**（issue-152）；与 **`IPC_SET`** 对 **`buf`** 一致。
- **`getrusage(RUSAGE_CHILDREN)`**：须为 **`wait`** 回收子进程的 **CPU** 累计（**`ProcessData::child_utime_ns`/`child_stime_ns`**），与 **`times`/`waitpid`** 累加路径一致；**勿**把 **`proc.threads()`** 中除当前线程外的 **pthread** 当作子进程。
- **`getrusage(2)`**：**`who`** 非法 → **`InvalidInput`**（**EINVAL**）；**`usage`** 为 **NULL** → **`BadAddress`**（**EFAULT**），须在聚合 **`Rusage`** 之前检查，与 Linux 顺序一致。
- **`prlimit64`**：对齐 Linux **`do_prlimit`**（issue-141）：**`old_limit` 非空**时先在进程内保存变更前的 **`rlimit64`**；**`new_limit` 非空**时先 **`vm_read`** 并校验（**`rlim_cur`≤`rlim_max`**、非法抬高硬上限 **`EPERM`**，同 issue-044），成功后再更新内核 **`rlimit`**；最后再 **`vm_write(old_limit)`** 写出快照。**`new_limit`** 读/校验失败时不应已写出 **`old`**。可降低硬上限或保持不变并更新 **`rlim_cur`**（在 **`rlim_cur <= rlim_max`** 前提下）。
- **`ioctl(FIONBIO)`**：第三参为 **`int *`**（Linux）；用 **`(arg as *const c_int).vm_read()`** 读整型，**`set_nonblocking(value != 0)`**；勿只读单字节、勿将取值限制为 0/1（**`2`**、**`256`** 等小端首字节为 0 的非零值须启用 **`O_NONBLOCK`**）。
- **`getrandom(2)`**：**`flags`** 须为 **`linux_raw_sys::general`** 中 **`GRND_NONBLOCK|GRND_RANDOM|GRND_INSECURE`** 的子集（与 **`uapi/linux/random.h`** 一致），否则 **`EINVAL`**；掩码校验须先于 **`len==0`** 的 **`Ok(0)`** 早退（issue-151，与 issue-042 互补）。勿用 **`from_bits_retain`** 静默丢弃未知位。
- **`fadvise64`**：管道等不可 seek 的 fd 上 Linux 为 **`ESPIPE`**（非法 seek），非 **`EPIPE`**（断管）。实现上 **`Pipe::from_fd`** 分支返回 **`LinuxError::ESPIPE.into()`**（**`axerrno::LinuxError`**），勿用 **`AxError::BrokenPipe`**。真 **`fadvise`** 语义仍为桩；**`advice≤5`** 且非 pipe 时 **`offset`/`len`** 须非负，且 cast 为 **`u64`** 后 **`checked_add`** 不溢出，否则 **`InvalidInput`**（**EINVAL**），与 **`sys_fallocate`** 一致。
- **`fsync(2)`/`fdatasync(2)`**：须 **`get_file_like`** 后对 **`File`** 或 **`MemfdCreatedFile`**（包装 **`File`**）调用 **`axfs::File::sync`**；**pipe**、**socket**、**目录**、**epoll** 等其它 **`FileLike`** → **`InvalidInput`**（**EINVAL**），勿用 **`File::from_fd`**（会得到 **`BrokenPipe`**/**`IsADirectory`**，issue-130）。无效 **`fd`** 仍为 **`BadFileDescriptor`**（**EBADF**）。
- **`read(2)`/`write(2)`**（经 **`readv`/`writev`**）：**`Directory`** **`FileLike::read`/`write`** 须 **`IsADirectory`**（**EISDIR**），勿 **`BadFileDescriptor`**（**EBADF**），与 Linux 及 **`pread`/`pwrite`** 经 **`File::from_fd`** 行为一致（issue-143）。
- **`renameat2(2)`**：**`flags`** 掩码、**`NOREPLACE`+`EXCHANGE`** 互斥、**`WHITEOUT`** 须在 **`vm_load_string(old_path/new_path)`** 之前完成（issue-138）。**`flags`** 须为 **`linux_raw_sys::general`** 中 **`RENAME_NOREPLACE|RENAME_EXCHANGE|RENAME_WHITEOUT`** 的子集，否则 **`EINVAL`**；**`WHITEOUT`** 未建模 overlay → **`EINVAL`**。**`RENAME_NOREPLACE`** 且 **`new_dir.lookup_no_follow(new_name)`** 已存在 → **`EEXIST`**。**`RENAME_EXCHANGE`**：两端须存在（否则 **`ENOENT`**）；同目录同名 → **`EINVAL`**；交换以临时名三次 **`Location::rename`**（非崩溃原子）。**`new_path`** 用 **`resolve_parent`**。
- **`setitimer`/`getitimer`**：**`ITIMER_REAL`** 独占 wall 时钟 **`alarm_task`**（**`ITimer::schedule_wall_alarm`**）；**`ITIMER_VIRTUAL`/`PROF`** 仅在 **`TimeManager::poll`** 中按 **`TimerState::User`/`Kernel`** 推进 **`remained_ns`**，**`set_itimer`** 与周期重载不再为二者注册 wall alarm。**`last_wall_ns`** 在 **`TimeManager::new`** 中初始化为 **`monotonic_time_nanos()`**，避免首次 **`delta`** 近似为开机时长。抢占与内核内 steal 时间仍弱于 Linux（TODO）。
- **`ioctl`（`kernel/src/file/fs.rs` 的 `File`）**：与 **`Tty`** 驱动已实现的终端/PTY 命令（**`TCGETS`**/**`TCSETS`** 族、**`TIOCGWINSZ`**、**`TIOCGPGRP`**、**`TIOCSCTTY`** 等）在 **`NodeType != CharacterDevice`** 时于转发 **`location().ioctl`** 前返回 **`NotATty`（ENOTTY）**；字符设备仍走 **`Device`**/**`DeviceOps`**（TTY 与其它设备各自处理）。
- **`clock_gettime` / `clock_getres`**：未实现的 **`clockid_t`** 须 **`EINVAL`**，**勿**对未知 id 回退 **`wall_time()`**。**`clock_id_supported`** 与已实现时钟一致（**REALTIME/REALTIME_COARSE、MONOTONIC/RAW/COARSE、BOOTTIME、CPUTIME_ID** 等）；**`clock_getres`** 对不支持 id 同样 **`EINVAL`**（即使 **`res==NULL`**）。
- **`nanosleep(2)`**/**`clock_nanosleep(2)`**：**`axtask::future::sleep`** 按**单调**时间推进；**`sleep_impl`** 应用 **`monotonic_time`** 测量 **`elapsed`** 与 **`rem`**。**`clock_nanosleep(CLOCK_MONOTONIC, …)`** 走 **`sleep_impl`**；**`CLOCK_REALTIME`** 且 **`dur` 非零**（相对睡眠或未到时的 **`TIMER_ABSTIME`**）→ **`Unsupported`**（无墙钟驱动睡眠）；**`dur==0`** → **`Ok(0)`**。
- `bpf` / `userfaultfd`：未实现时返回 **`AxError::Unsupported`（ENOSYS）**，勿再 `sys_dummy_fd`；`perf_event_open` 可返回 **`PermissionDenied`（EPERM）** 以匹配测试与常见无能力场景。**`io_uring_setup`**：桩实现返回 **`anon_inode:[io_uring]`** 的 **`IoUringFd`**；须先 **`add_file_like`** 成功，再 **`vm_write`** **`IoUringParams`**（**`sq_entries`/`cq_entries`** 等）；**`add_file_like`** 失败（如 **EMFILE**）不得改写用户 **`params`**（issue-145）；**`vm_write`** 失败则 **`close_file_like`** 回收 fd。成功写回前仍将 **`sq_off`/`cq_off`/`resv`/`sq_thread_*`** **清零**（无真实 ring 布局，issue-112），勿把用户输入的布局垃圾写回；真 io_uring 需 ring mmap 与提交队列。
- `fsopen`：无 fs-context 实现时返回 **`NoSuchDevice`（ENODEV）**（或 EINVAL），勿发 `anon_inode:[dummy]`；`fspick`/`open_tree` 可 **`Unsupported`**。`memfd_secret` 在用户态常以两参探测（与 `memfd_create` 同形）时，可 **`sys_memfd_create` 复用** 以获得真实 memfd 路径。已移除 **`sys_dummy_fd`** 分配假 fd 的路径。
- **`mount`/`umount2`**：**`sys_mount`** 仅允许 **`SUPPORTED_MOUNT_FSTYPES`**（当前 **`tmpfs`**）；空或未知 **`fstype`** → **`EINVAL`**。**`mount`** 的 **`flags`** 当前须为 **0**（未实现 **`MS_RDONLY`/`MS_BIND`/…**）；**`data`** 须为 **`NULL`** 或空 C 字符串（非空 **`tmpfs` 选项** → **`EINVAL`**）。**`sys_umount2`** 的 **`flags`** 须为 **`MNT_FORCE|DETACH|EXPIRE|UMOUNT_NOFOLLOW`** 子集，否则 **`EINVAL`**。未建模 **`CAP_SYS_ADMIN`** 等挂载权限。
- `/proc/self/fd/N` 的 readlink 内容来自 `FileLike::path()`；`inotify_init1`/`fanotify_init` 须使用独立 `InotifyFd`/`FanotifyFd`（`anon_inode:[inotify]` / `anon_inode:[fanotify]`），不能再用 `anon_inode:[dummy]`，否则用户态假阳性。
- POSIX `timer_create` / `timer_settime` / `timer_gettime` / `timer_delete`：未实现时须返回 **`AxError::Unsupported`（ENOSYS）**，禁止 `Ok(0)` 导致用户态 `timer_t` 未写入却被当作成功；若将来实现，需向 `timer_create` 第四参写入非空 id 并接 `sigevent`/线程定时逻辑。
- `flock(2)`：按 inode（`File`/`Directory` 的 `metadata` dev/ino）维护 BSD 风格互斥/共享锁；同一 fd 升级/幂等、关闭 fd 须从全局表移除；`LOCK_NB` 冲突映射 `AxError::WouldBlock`（EAGAIN）；阻塞模式在 `WouldBlock` 上 `yield_now` 轮询。
- `fcntl` 记录锁（`F_SETLK`/`F_SETLKW`/`F_GETLK` 及 `F_OFD_*` 同路径）：**`sys_fcntl`** 须先 **`get_file_like(fd)`** 再读/写用户 **`flock64`**（**`EBADF`** 先于 **EFAULT**，issue-136）；**`record_lock`** 内仍会 **`inode_key`/`get_file_like`**。仅对普通 **`File`** fd；**`flock64`** 区间经 SEEK_SET/CUR/END 解析；写锁与读/写冲突、读锁仅与写冲突；同进程占位前先对重叠区间解锁再插入；**`F_SETLK`** 冲突返回 **`WouldBlock`**；**`close_file_like`** 调用 **`record_lock::release_fd`** 清除该 fd 登记锁。
- 作业控制：`ProcessData::jobctl` 记录 `stop_sig` / `stop_wait_pending` / `continued_wait_pending`；`SignalOSAction::Stop` 不再 `do_exit`，而是唤醒父 `child_exit_event` 并在内核循环中等待 `SIGCONT`（循环内调用 `check_signals` 以处理入队信号）；`Continue` 清除停止并在曾停止时置 `continued_wait_pending`；`waitpid` 对 `WUNTRACED` 或 **options==0** 写 `(sig<<8)|0x7f`，对 `WCONTINUED` 或 **options==0** 写 `0xffff`。
- **`waitpid(2)`/`wait4(2)`**：**`options`** 须为 **`WaitOptions::from_bits`** 可解析的 **`WNOHANG|WUNTRACED|WEXITED|WCONTINUED|WNOWAIT|__WNOTHREAD|__WALL|__WCLONE`** 子集（与 **`linux_raw_sys::general`** 一致），否则 **`InvalidInput`（EINVAL）**；勿 **`from_bits_truncate`**。**`WALL`/`WCLONE`** 过滤仍有 **FIXME**。
- 地址空间并发：`ProcessData.aspace` 为 `Arc<RwLock<AddrSpace>>`；修改页表（缺页 populate、mmap 等）用 `write()`；纯查询（如 mincore、`mremap` 查 VMA、futex 地址解析、部分 `can_access_range`）用 `read()`。缺页仍会写锁直至支持按页或 per-VMA 锁。
- 进程凭证：`ProcessData` 中 **`ruid/euid/suid`** 与 **`rgid/egid/sgid`**（`AtomicU32`）；`getuid`→`ruid`，`geteuid`→`euid`，gid 同理；**`get_resuid`/`get_resgid`** 供 **`getresuid(2)`/`getresgid(2)`** 一次读三组；`setuid`/`setgid` 为三 ID 同步设置；`setresuid`/`setresgid` 中 **`CRED_NO_CHANGE`=`u32::MAX`** 表示不改；**`euid==0`（或 `egid==0`）** 视为特权可任意改，否则新值须属于当前 `{r,e,s}` 之一否则 **`PermissionDenied`**。`clone` 新建进程在 `ProcessData::new` 后 **`copy_credentials_from`** 父 `ProcessData`。完整 Linux 能力集与 setfsuid 等仍未建模。
- 补充组：**`supplementary_gids`**（`Mutex<Vec<u32>>`，上限 **`SUPP_GROUPS_MAX`**）；`getgroups` 仅列补充组不含主 `rgid`；`setgroups` 需 **`euid==0`**；`getgroups(0,…)` 返回个数。**`seccomp(2)`** 未实现时 **`Unsupported`（ENOSYS）**。**`prctl(PR_SET_SECCOMP)`**：**`arg2`>2**（非 **`SECCOMP_MODE_*`**）→ **`EINVAL`**；mode **0/1/2** 未实现 → **`Unsupported`**。**`prctl(PR_MCE_KILL)`**：**`arg2`** 须 **`CLEAR`/`SET`**；**`SET`** 时 **`arg3`**≤**`DEFAULT`**，否则 **`EINVAL`**；合法组合未实现 → **`Unsupported`**。
- **`execve` 多线程**：在替换映像前若 **`proc.threads().len() > 1`**，对其余 tid **`SIGKILL`** 并 **`yield_now`** 直至仅剩当前线程（对齐 Linux 先杀线程组再 exec）；长时间未收敛则 **`WouldBlock`**。非 vfork/线程本地存储析构等细语义仍弱于 Linux。
- **`accept` / `accept4`**：向用户写入的 sockaddr 必须是 **`peer_addr()`**（远端），勿用 **`local_addr()`**（本端监听地址）；与 **`getpeername(accepted_fd)`** 一致。**`accept4`** 第四参 **`flags`** 须为 **`O_CLOEXEC | O_NONBLOCK`**（与 Linux **`SOCK_CLOEXEC`/`SOCK_NONBLOCK`** 同值），否则 **`EINVAL`**。**`addr` 非空**时须先 **`write_to_user`**/**`addrlen`**（**`get_as_mut`**）再 **`add_to_fd_table`**，避免 **`copy_to_user`** 失败时已装新 **fd** 却未返回 **fd** 号（issue-149，与 issue-148 **`pipe2`** 同类）。**`addr==NULL`** 无地址写回，仍直接装表。
- **`getsockname`/`getpeername`**（**`net/name.rs`**）：**`addrlen.get_as_mut()`** 须在 **`local_addr`/`peer_addr`** 之前，使 **NULL** 或不可写的 **`addrlen`** 尽早 **EFAULT**，再取内核地址并 **`write_to_user`**（**`addr`** 仍在 **`fill_addr`** 写回时访问）。
- **`socket(2)`/`socketpair(2)`**：**`type`** 仅允许 **`SOCK_TYPE_MASK`（0xf）** 内 **`SOCK_*`** 与 **`O_CLOEXEC|O_NONBLOCK`**；其它位 **`InvalidInput`**（对齐 Linux **`EINVAL`**）。**`ty`** 取 **`raw_ty & SOCK_TYPE_MASK`**。**`socketpair`**：先 **`fds.get_as_mut()`** 再依次 **`add_to_fd_table`**；第二端失败须 **`close_file_like`** 已装第一端（issue-150，与 issue-148 **`pipe2`** 同类）。
- **`setsockopt(2)`**：**`optlen`** 须 **`>=`** 选项值 **`sizeof(T)`**（与 Linux / **`getsockopt`** 侧一致），只使用缓冲区前 **`sizeof(T)`** 字节；**`optlen < sizeof(T)`** → **`EINVAL`**。
- **`getsockopt(2)`**（**`net/opt.rs`**）：**`Socket::from_fd`** 须早于 **`optlen.get_as_mut`**，使无效 **`fd`** 先 **`EBADF`**，再触碰 **`optlen`/`optval`**（与 Linux **`sockfd_lookup`** 顺序及 **`setsockopt`** 对称）；日志仅用 **`UserPtr::address`** 避免提前用户读。
- **`bind(2)`/`connect(2)`**（**`socket.rs`**）：**`Socket::from_fd`** 须早于 **`SocketAddrEx::read_from_user`**，无效 **`fd`** 先 **`BadFileDescriptor`**（**EBADF**），与 Linux **`sockfd_lookup`** 及 **`sys_accept4`** 顺序一致（issue-132）。
- **`sendto(2)`**（**`net/io.rs`**）：**`send_impl`**：**`SENDMSG_FLAGS_MASK`** 后 **`Socket::from_fd`**，再 **`send_on_socket`**（**`msg_name`** **`read_from_user`** + **`send`**），对齐 **`__sys_sendto`**（issue-133）。
- **`sendmsg(2)`**：**`Socket::from_fd`** → **`SENDMSG_FLAGS_MASK`** → **`msghdr.get_as_ref`** / cmsg / **`IoVectorBuf::new`** → **`send_on_socket`**（无二次 **`from_fd`**），对齐 **`__sys_sendmsg`** **`sockfd_lookup` 先于 `copy_msghdr_from_user`**（issue-134）。
- **`recvmsg(2)`**：**`validate_recvmsg_flags`**（**`RECVMSG_FLAGS_MASK`** + **`RECVMSG_FLAGS_UNSUPPORTED`**）→ **`Socket::from_fd`** → **`msghdr.get_as_mut`** / **`IoVectorBuf::new`** → **`recv_on_socket`**，与 **`__sys_recvmsg`** 及 issue-134 **`sendmsg`** 对称（issue-135）。**`recvfrom`** 仍走 **`recv_impl`**（相同校验 + **`recv_on_socket`**）。
- **`bind`/`connect`/`sendto` 等 INET 地址**：**`SocketAddrV4`/`SocketAddrV6::read_from_user`** 要求 **`addrlen >= sizeof(sockaddr_in|sockaddr_in6)`**，只按固定布局读 **`sockaddr_in`/`sockaddr_in6`**；**`addrlen` 大于结构体**时与 Linux 一样忽略尾部字节。**vsock** **`sockaddr_vm`** 同理。
- **`sendmsg` / `recvmsg` 与 ancillary**：控制缓冲区须与 Linux 一致使用 **`CMSG_ALIGN(sizeof(cmsghdr)+payload)`** 作为**占用步长**；**`cmsg_len`** 仍为含头的逻辑长度。遍历下一条头用 **`ptr += CMSG_ALIGN(cmsg_len)`**；**`CMsgBuilder::push`** 在 **`msg_controllen`** 与下一 **`cmsghdr`** 指针上前移对齐后长度，**`cmsg_len` 至对齐边界**建议填 **0**。**`CMsg::parse`（`sendmsg`）** 仅实现 **`(SOL_SOCKET, SCM_RIGHTS)`**；**`SCM_CREDENTIALS`/`SCM_TIMESTAMP`/`SCM_TIMESTAMPNS`/`SCM_TIMESTAMPING`/`SCM_SECURITY`** → **`Unsupported`**，勿与格式错误混用 **`EINVAL`**。
- **`sched_getaffinity` / `sched_setaffinity`**：`pid==0` 为当前任务；非零先 **`get_task(pid)`**，失败再 **`get_process_data(pid)`** 取 **`proc.threads()` 最小 tid** 定位线程组代表线程。set 时当前任务走 **`set_current_affinity`**（SMP 迁移），其它任务仅 **`set_cpumask`**。**`sched_setaffinity`** 须先 **`sched_resolve_task`**（**`ESRCH`**）再 **`vm_load(user_mask)`**（**EFAULT** 类），与 Linux 一致（issue-142）。未完整建模 CAP、僵尸 **`ESRCH`** 等。
- **`sched_setscheduler`**：**`param==NULL`** → **`EINVAL`**（仍可先于 **`pid`** 解析）；非空时先 **`sched_resolve_task`**/**`try_as_thread`**，再 **`vm_read_uninit(sched_param)`**（issue-142，**`ESRCH`** 先于 **EFAULT**）。
- **`sched_getscheduler` / `sched_setscheduler` / `sched_getparam`**：每线程在 **`Thread`** 上存 **`sched_policy`**（默认0，即 `SCHED_NORMAL`/`SCHED_OTHER`）与 **`sched_priority`**（默认 0）。`setscheduler` 从用户读 **`sched_param`** 并校验策略与优先级范围后写入；`getscheduler`/`getparam` 返回已存值。策略未接入 axtask 真实 RT 调度，仅保证与用户态查询一致。**issue-018** 与 **issue-002** 描述同一修复；验收可用 **`test_sched_stubs.c`**（默认 **`sched_getscheduler(0)==SCHED_OTHER`**）或 **`test_sched_policy_stubs.c`**。
- **`getpriority` / `setpriority`**：每进程 **`ProcessData::nice`**（**-20..=19**，默认 **0**）；`fork` 经 **`copy_credentials_from`** 继承。**`sys_getpriority`** 成功返回值须为 Linux **`20 - nice`**（**`nice_to_rlimit`**），**非**裸 **`nice`**（**`nice=0`** → **20**）。**`setpriority`** 已 syscall 分发。**`PRIO_PGRP`/`PRIO_USER`** 在 **`processes()`** 上取匹配进程的 **最小 nice**（最高调度优先级）。未建模 **`CAP_SYS_NICE`** 与特权 **`nice`** 下限等 **`EPERM`**。
- **ELF `execve` 缓存**：全局 **`ELF_LOADER`** 为 **`spin::RwLock<ElfLoader>`**（LRU 32）。**`ensure_elf_cached`**：先读锁命中则返回；未命中则在**无 ELF 锁**下 **`ElfCacheEntry::load`**，再写锁 **去重插入**。**`map_cached_elf_into_uspace`** 持**读锁**做 **`lookup_entry`**、**`uspace.clear`**、**`map_elf`**（多进程可并发读同一缓存项）。写锁仅覆盖 LRU 变更；高并发 + 满缓存时仍存在 **LRU 驱逐** 与「装入后、映射前被挤掉」的极小理论窗口（可后续改为 `Arc` 条目或分片）。
- **跨进程 shared futex**：**`futex_table_for(Shared)`** 使用 **`SHARED_FUTEX_TABLES[shard]`**（**16** 个 **`Mutex<FutexTables>`**），**`shard = (ptr ^ ptr>>12 ^ ptr>>24) % 16`**，`ptr` 为 **`Weak::as_ptr(region)`**。每片内仍为 **`BTreeMap` + 约每 100 次 `retain` GC**；不同共享 region 多数走不同分片。私有 futex 仍 **`ProcessData::futex_table`**。
- **`madvise` / `msync` / `mlock`**：**`advice`** 须为 **`linux_raw_sys::general`** 中 **`KNOWN_MADV_ADVICE`**（全部 **`MADV_*`**），否则 **`EINVAL`**；**`MADV_DONTNEED`/`MADV_FREE`** 对 **`CowBackend`** 调用 **`BackendOps::unmap`** 后由缺页 **`populate`** 再分配零页；**`Shared`/`File`/`Linear`** 不丢页（跳过）；其余已定义 **`MADV_*`** 当前 no-op **`Ok(0)`**。**`msync`** 校验 **`MS_ASYNC`与 `MS_SYNC` 二选一**、允许标志位及区间已映射可读；对重叠的 **`File`** VMA 经 **`AddrSpace::msync_file_mappings`** 去重后 **`CachedFile::sync`** 回写页缓存（**`MS_ASYNC`/`MS_SYNC`** 当前均走此路径；**`MS_INVALIDATE`** 未实现丢弃语义）。**`mlock`/`mlock2`** 仅校验 **`MLOCK_ONFAULT`**、**`len>0`**、映射可读；未接 **`RLIMIT_MEMLOCK`** 与物理钉页。
- **`mremap`**：**`AddrSpace::mremap`** 要求 **`addr` 为 VMA 起点且 `old_size` 等于该 VMA 长度**。**缩小**：`unmap` 尾部。**原地放大**：尾部虚拟区间无其它映射时 **`unmap` 全段 + `map` 新尺寸**，**`FileBackend::remap_at`** / **`CowBackend::with_virt_start`** 保持同一 **`CachedFile`/文件偏移或 COW 状态**。**`MREMAP_MAYMOVE`**：尾部冲突时 **`read`→`find_free_area`→搬迁 `map`→`write`**。**`Shared`** 变长（缺页框）与 **`Linear`**：**`OperationNotSupported`**。**`MREMAP_FIXED`/`DONTUNMAP`**：**`EINVAL`**。
- **`capget` / `capset`**：**`ProcessData`** 存 **`cap_effective`/`cap_permitted`/`cap_inheritable`**（**`AtomicU32`**，v3 低 32 位；默认 **`u32::MAX`**）。**`sys_capget`**/**`sys_capset`** 仅允许操作**当前进程**（**`header.pid==0` 或正 pid 解析到同一 `ProcessData`**，否则 **`PermissionDenied`**）。**`capset`** 要求 **`effective`/`inheritable` ⊆ `permitted`**；仅 **`euid==0`** 或当前 **`effective`** 含 **`CAP_SETPCAP`（1<<8）** 可改，否则 **`EPERM`**。未实现 64 位第二组 **`__user_cap_data_struct`**。
- **`sysinfo(2)`**：**`totalram`** ← **`axhal::mem::total_ram_size()`**；**`freeram`** ← **`min(available_pages * PAGE_SIZE_4K, totalram)`**（**`axalloc::global_allocator()`** 空闲页池，近似值）；**`uptime`** ← **`monotonic_time_nanos / NANOS_PER_SEC`**；**`loads`/swap/buffer/high** 仍为 **0**；**`mem_unit=1`**。与 Linux **MemAvailable** 级统计仍有差距。
- **`syslog(2)`/`klogctl`**：**`action`** 须在 **`SYSLOG_ACTION_CLOSE`..=`SIZE_BUFFER`（0..=10）**，否则 **`InvalidInput`**。无 printk 环：**`READ`/`READ_ALL`/`READ_CLEAR`** 与 **`CLEAR`/`CONSOLE_*`** → **`Unsupported`**；**`SIZE_UNREAD`/`SIZE_BUFFER`** 返回 **0**（空环）；**`OPEN`/`CLOSE`** → **`Ok(0)`**。勿对任意参数无条件 **`Ok(0)`**。
- **`splice(2)` / `copy_file_range(2)`**：**`flags`** 须在 Linux 已知掩码内（**`SPLICE_F_MOVE|NONBLOCK|MORE|GIFT`**；**`copy_file_range`** 为 **`COPY_FILE_RANGE_COMPRESS|DEDUPE`**），否则 **`EINVAL`**。**`SPLICE_F_*`** 语义（如非阻塞）若与底层 **`do_send`** 未完全对齐，属后续增强；非法位须先拒绝。**`splice`**：**`off_in`/`off_out` 非空**时须先 **`File::from_fd`**（**`EBADF`**）再读用户偏移（**EFAULT** 类），与 **Linux `do_splice`** / **issue-122** 同类顺序。**`sendfile(2)`**：**`offset` 非空**时同样先 **`File::from_fd(in_fd)`** 再 **`offset.vm_read`**（**`offset==NULL`** 分支仍为 **`get_file_like(in_fd)`** 在前）。
- **`pwrite64(2)`**：**`offset < 0` → `InvalidInput`**（issue-108）；**`len == 0`** 仍须先 **`File::from_fd`** 再 **`Ok(0)`**，勿在 **`from_fd`** 前早退，以便无效 fd 得 **EBADF**（issue-125，对齐 Linux **`vfs_write`/`fget`**）。
- **`truncate(2)`**：**`length < 0` → `InvalidInput`** 须先于 **`path.get_as_str()`**，以便坏 **`path`** 与负长度组合时优先 **EINVAL**（issue-126，与 **`ftruncate`** 及 Linux **`do_truncate`** 一致）。
- **`recvmsg`/`recvfrom`/`sendmsg`/`sendto`**：**`flags`** 须在 **`linux_raw_sys::net::MSG_*`** 定义的 **接收** 与 **发送** 掩码内（**`RECVMSG_FLAGS_MASK`** 含 **`MSG_PEEK`**；**`SENDMSG_FLAGS_MASK`** 不含 **`MSG_PEEK`**），否则 **`EINVAL`**。**`axnet::SendFlags`** 仍为占位 **`bitflags!`**，合法 **`MSG_*`** 尚未透传到 **`SendOptions.flags`**；**`MSG_DONTWAIT`** 等语义需在 **`axnet-ng`** 扩展 **`SendFlags`** 并在各 **`SocketOps::send`/`recv`** 中实现。
- **`times(2)`**：**`tms_utime`/`tms_stime`** = 线程组用户/系统时间（已退出线程计入 **`exited_threads_*_ns`**，存活线程取 **`TimeManager::cpu_nanos`**）；**`tms_cutime`/`tms_cstime`** = 已通过 **`wait`** 回收的子进程线程组 CPU 累计（**`waitpid`** 从僵尸 **`ProcessData`** 读 **`thread_group_cpu_nanos`** 后加到父 **`child_*_ns`**）。末线程退出时 **`register_zombie_process_data`**，**`wait`** **`free`** 后 **`remove_zombie_process_data`**。
- **`Socket`/`fstat`**：每个 **`Socket::new`** 分配单调 **`sock_ino`**（**`AtomicU64`**）与固定 **`SOCKFS_STAT_DEV`**；**`FileLike::stat`** 填 **`Kstat::dev`/`ino`**；**`path`** 为 **`socket:[ino]`**，与 **`st_ino`** 一致。
- **`get_mempolicy(2)`**：无 NUMA 建模时 **`policy`** 写入 **`MPOL_DEFAULT`（0）**；若 **`nodemask`/`maxnode`** 有效则清零 **`maxnode`** 位对应字节（上限 8192 字节）以匹配 **默认** 策略的空节点掩码。

## 给 Debugger 的消息
- issue-057：请跑 **`/bin/test_statx_invalid_flags`**（**`statx(..., flags=0x80000000)`** → **`EINVAL`**）。
- issue-056：请跑 **`/bin/test_accept4_invalid_flags`**（非法 **`flags`** → **`EINVAL`**）。
- issue-061：请在 QEMU 做双进程 **`msgrcv` 阻塞 + `msgsnd` 唤醒** 冒烟（无现成 **`/bin`** 用例）。
- issue-055：请跑 **`/bin/test_madvise_invalid_advice`**（**`madvise(..., 0xdeadbeef)`** → **`EINVAL`**）。
- issue-054：请跑 **`/bin/test_faccessat2_invalid_flags`**（非法 **`flags`** → **`EINVAL`**）。
- issue-128：可跑 **`faccessat2(AT_FDCWD, \"/\", R_OK|0x100, 0)`**（非法 **`mode`** → **`EINVAL`**；Linux 对齐）。
- issue-129：非法 **`flags`** 或 **`mode`** + 坏 **`path`** 指针，首错应 **`EINVAL`**（先于 **EFAULT** 类）。
- issue-130：**`fsync`/`fdatasync`** 在 **pipe**/**socket** fd 上应 **`EINVAL`**，勿 **`BrokenPipe`**/**`IsADirectory`**。
- issue-131：**`fstatat`** 非法 **`flags`**（如保留高位）→ **`EINVAL`**；可与 **`statx`** 非法 flags 用例类比。
- issue-132：无效 **socket `fd`** + 坏 **`addr`**，首错应 **`EBADF`**（先于 **EFAULT**）。
- issue-133：**`sendto`** 或 **`sendmsg`** 仅坏 **`msg_name`**（**`msghdr`** 本身合法）：无效 **`fd`** 时首错 **`EBADF`**。
- issue-134：**`sendmsg`** 无效 **`fd`** + 坏 **`msghdr`/`iov`**：首错应 **`EBADF`**（先于 **EFAULT**）。
- issue-135：**`recvmsg`** 非法 **`flags`** 先于 **`msghdr`**；无效 **`fd`** + 坏 **`msghdr`/`iov`**：首错 **`EBADF`** 或 **`EINVAL`**（**flags**）先于 **EFAULT**。
- issue-136：**`fcntl`** **`F_SETLK`/`F_GETLK`**（含 **`F_OFD_*`**）：无效 **`fd`** + 坏 **`flock64 *`**，首错 **`EBADF`**。
- issue-137：**`linkat`** 非法 **`flags`** + 坏 path指针，首错 **`EINVAL`**（先于 **EFAULT**）。
- issue-138：**`unlinkat`**/**`renameat2`** 非法 **`flags`** + 坏 path，首错 **`EINVAL`**（先于 **EFAULT**）。
- issue-139：**`fchownat`**/**`fchmodat`** 非法 **`flags`** + 坏 **`path`**，首错 **`EINVAL`**（先于 **EFAULT**）；可扩展现有 **`/bin/test_fchownat_invalid_flags`**/**`test_fchmodat_invalid_flags`** 思路加坏指针组合。
- issue-140：**`readlinkat`** 对已存在 symlink，**`bufsiz==0`** → **`EINVAL`**，勿 **`Ok(0)`**；可与坏 **`path`** 组合验证 **EINVAL** 先于 **ENOENT/EFAULT**。
- issue-141：**`prlimit64`** 同时 **`old_limit`** + 非法 **`new_limit`**（**`rlim_cur`>`rlim_max`** 或不可读 **`new`**）：**`old`** 缓冲应未被写入；首错 **`EINVAL`**/**`EFAULT`**。
- issue-142：无效 **`pid`** + 坏 **`user_mask`** / **`sched_param`**：首错 **`ESRCH`**（先于 **EFAULT**）；**`sched_setscheduler`** **`NULL` param** 仍 **`EINVAL`**。
- issue-143：已 **`open` 目录** fd 上 **`read`/`write`** → **`EISDIR`**，勿 **EBADF**。
- issue-144：合法 **`epfd`** + 无效 **`fd`** + 坏 **`event`**（**`EPOLL_CTL_ADD`/`MOD`**）：首错 **`EBADF`**（先于 **EFAULT**）。
- issue-145：**`io_uring_setup`** 在 **fd 表满**等导致 **`add_file_like`** 失败时，用户 **`IoUringParams`** 应保持未被内核写回。
- issue-146：**`rt_sigprocmask`**：**`set` 非空** + 非法 **`how`** + **`oldset` 非空**：**`EINVAL`** 时 **`oldset`** 不应被写入。
- issue-147：**`rt_sigaction`**：**`act`/`oldact` 均非空** + 不可读 **`act`**：**`EFAULT`** 时 **`oldact`** 不应被写入。
- issue-148：**`pipe2`** +坏 **`fds`**：**`vm_write`** 失败不应泄漏两个管道 **fd**。
- issue-149：**`accept4`** **`addr` 非空** + 坏 **`sockaddr`/`addrlen`**：**`EFAULT`** 不应多出新 **socket fd**。
- issue-150：**`socketpair`** 仅余 1 **fd** 槽或第二端 **`add_to_fd_table`** 失败：不应残留已装第一端 **fd**。
- issue-151：**`getrandom(..., len=0, flags=~0)`** → **`EINVAL`**，勿 **`Ok(0)`**。
- issue-152：**`shmctl(..., IPC_STAT, NULL)`** → **`EFAULT`**，勿成功刷新 **`shm_ctime`**。
- issue-053：请跑 **`/bin/test_memfd_create_invalid_flags`**（**`memfd_create(..., 0x80000000)`** → **`EINVAL`**）。
- issue-052：请跑 **`/bin/test_fchownat_invalid_flags`**（非法 **`flags`** → **`EINVAL`**，**`uid`/`gid`** 不变）。
- issue-051：请跑 **`/bin/test_utimensat_invalid_flags`**（非法 **`flags`** → **`EINVAL`**，**`mtime`** 不变）。
- issue-049：请跑 **`/bin/test_fchmodat_invalid_flags`**（非法 **`flags`** → **`EINVAL`**，**`st_mode`** 不变）。
- issue-048：请跑 **`/bin/test_unlinkat_invalid_flags`**（非法 **`flags`** → **`EINVAL`**，目标文件未删）。
- issue-047：请跑 **`/bin/test_linkat_invalid_flags`**（**`linkat(..., 0x80000000)`** → **`EINVAL`**）。
- issue-023：请跑 **`/bin/test_mempolicy_stub`**（**`get_mempolicy`** 成功后 **`mode != -1`**）。
- issue-041：请跑 **`/bin/test_socket_stat_unique_ino`**（两 **`socket(AF_INET,SOCK_STREAM)`** 的 **`fstat.st_ino`** 不同）。
- issue-039：请跑 **`/bin/test_times_cutime`**（**`wait`** 后父 **`tms_cutime`** 相对 **`wait`** 前增加）。
- issue-038：请跑 **`/bin/test_recvmsg_invalid_flags`**（**`recvmsg(..., 0x80000000)`** 有数据仍 **`EINVAL`**）。
- issue-037：请跑 **`/bin/test_splice_flags_invalid`**（**`splice(..., flags=0xdeadbeef)`** → **`EINVAL`**）。
- issue-035：请跑 **`/bin/test_clock_gettime_invalid`**（无效 **`clock_gettime`** **`clockid`** → **`EINVAL`**）。
- issue-032：请跑 **`/bin/test_mount_partial`**（非法 **`fstype`** → **`EINVAL`/`ENODEV`/`ENOENT`** 或 **`EPERM`** 跳过）。
- issue-030：请跑 **`/bin/test_nice_enosys`**（**`setpriority` 非 ENOSYS**）；可配合 **`/bin/test_getpriority`** 核对 **`getpriority`** 与 **`nice`**。
- issue-029：请跑 **`/bin/test_getresuid_enosys`**（**`getresuid`/`getresgid`** 返回 0 并写入三组 id）。
- issue-036：请跑 **`/bin/test_prctl_seccomp_stub`**（**`PR_SET_SECCOMP` + 非法 mode → EINVAL**）。
- issue-027：请跑 **`/bin/test_sysinfo_partial`**（**`sysinfo.totalram > 0`**）。
- issue-021：请跑 **`/bin/test_cap_stub`**（**`capset` 清零后 `capget` 全 0**；若 **`capset` EPERM** 则测试会跳过断言）。
- issue-020：请跑 **`/bin/test_mremap_shared`**（**`MAP_SHARED`扩展 + **`msync`** +文件第二页）。
- issue-019：请跑 **`/bin/test_mm_noop`**（**`MADV_DONTNEED`** 后读零）。
- issue-018 / issue-002：可在 rootfs 跑 **`/bin/test_sched_stubs`** 或 **`/bin/test_sched_policy_stubs`**。
- issue-008：请在 rootfs 跑 `/bin/test_futex_shared_stress`（pthread/musl futex）。
- issue-007：请在 rootfs 放 `/bin/true`，跑 `/bin/test_elf_parallel_exec`（双进程并行 `execve`）。
- issue-003：请在 rootfs 跑 `/bin/test_getpriority`（`setpriority`/`getpriority` 对 `PRIO_PROCESS`）。
- issue-002：请在 rootfs 跑 `/bin/test_sched_policy_stubs`（`SCHED_OTHER` 往返与 `sched_getparam` 写缓冲区）。
- issue-001：请在 rootfs 跑 `/bin/test_sched_affinity`（对存活子进程 `sched_getaffinity`）；多线程非 leader PID 行为弱于 Linux。
- issue-034：请在 rootfs 跑 **`/bin/test_sendmsg_cmsg_align`**（多段 **`SCM_RIGHTS`**，依赖 **`CMSG_ALIGN`**）。
- **`accept`/`accept4` peer**：请跑 **`/bin/test_accept_peer_addr`**（与上条 issue 编号无关）。
- issue-028：rootfs 需 `/bin/true`，跑 `/bin/test_execve_multithread`；若 SIGKILL 路径未调度退出可再查 `check_signals`/pthread 阻塞点。
- issue-025：请在 rootfs 跑 `/bin/test_identity_seccomp_stub`；真 seccomp-bpf 未实现；`PR_SET_SECCOMP` 与 `seccomp` syscall 行为不一致属已知简化。
- issue-024：`/bin/test_membarrier_stub` 仅测 `QUERY`；RSEQ/`GET_REGISTRATIONS` 等返回 `EINVAL`；多核全局屏障需后续 IPI。
- issue-022：请在 rootfs 跑 `/bin/test_setuid_stub`；未接 `setfsuid`/文件置位 exec 等。
- issue-014：请在 rootfs 跑 `/bin/test_dummy_fsapi`；真 Linux `memfd_secret` 常为单参 flags，本内核按测试与 `memfd_create` 同形两参接入。
- issue-013：请在 rootfs 跑 `/bin/test_dummy_fd_advanced`；`io_uring_setup` 仅为路径与 params 桩，真实 liburing 仍可能因缺 ring失败。
- issue-010：timerfd 与 issue-011 同一套 `kernel/src/file/timerfd.rs`；请在带 `/bin/test_timerfd` 的 rootfs 中 QEMU 验证 poll+read。
- issue-009：与 issue-026 同一套 `JobCtl` + `check_signals` Stop/CONT + `waitpid`；`raise(SIGSTOP)` 与 `kill(..., SIGSTOP)` 同源。请在 QEMU 跑 `test_sigstop_sigcont`。
- issue-004：`aspace` 已迁 `RwLock`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 QEMU 跑 `test_aspace_concurrent_mmap` 做功能基线；多线程同时缺页仍互斥写锁，进一步优化需更细粒度锁。
- issue-026：`SIGSTOP`/`SIGCONT` 与 `waitpid` 已接 `JobCtl`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 跑 `test_sigstop_semantic`（`WUNTRACED` + `WSTOPSIG==SIGSTOP`）。多线程全进程停表仍简化为单线程路径；仅测试 fork 子进程场景。
- issue-017：`record_lock.rs` + `sys_fcntl` 接入；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 中跑 `test_fcntl_lock_stub` 验证第二进程 `F_SETLK` 得 EAGAIN/EACCES。OFD 锁与 `dup` 共享同一 open file description 的精细语义仍弱化为与进程锁相同路径，如遇真实用例可再细化。
- issue-016：`kernel/src/file/flock.rs` 实现按 (dev,ino) 的 flock 表；`close_file_like` 前 `release_fd`；`cargo clippy --target riscv64gc-unknown-none-elf -F qemu` 通过。请在 rootfs 中交叉编译并运行 `test_flock_stub` 验证父持 LOCK_EX 时子 LOCK_NB 得 EAGAIN/EWOULDBLOCK。
- issue-015：`test_posix_timer_stub.c` 在 `timer_create` 返回 ENOSYS 时显式 PASS；已交叉编译并通过 `clippy`/`make build`。
- issue-012 已修复：`test_dummy_inotify_fanotify.c` 已交叉编译；`cargo fmt`、`clippy -F qemu`、`make ARCH=riscv64 build` 通过。请在带 procfs 的 rootfs 中实跑 readlink。
- `inotify_add_watch` 等仍为未实现时可继续标 ENOSYS；若需端到端监控，可再开 issue 实现队列与事件格式。
