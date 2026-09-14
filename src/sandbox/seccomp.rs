//! Syscall-filter layer using seccomp.
//!
//! The classic-BPF program is assembled here rather than through
//! `seccompiler`. That crate's root is gated on
//! `target_endian = "little"` and its architecture table only covers
//! x86_64, aarch64 and riscv64, which leaves ppc64, ppc64le and
//! loongarch64 with no filter at all. The program we need is a flat
//! allowlist plus a single argument check, so emitting it directly is
//! cheaper than carrying a second backend for those ports.

#[cfg(all(target_os = "linux", feature = "seccomp", target_pointer_width = "64"))]
pub(crate) use imp::apply;

#[cfg(all(
    target_os = "linux",
    feature = "seccomp",
    not(target_pointer_width = "64")
))]
pub(crate) fn apply() -> crate::SeccompPosture {
    crate::SeccompPosture::Unsupported {
        reason: "unsupported architecture",
    }
}

#[cfg(not(all(target_os = "linux", feature = "seccomp")))]
pub(crate) fn apply() -> crate::SeccompPosture {
    crate::SeccompPosture::Disabled
}

#[cfg(all(target_os = "linux", feature = "seccomp", target_pointer_width = "64"))]
mod imp {
    use crate::SeccompPosture;

    const BPF_MAX_LEN: usize = 4096;

    const AUDIT_ARCH_64BIT: u32 = 0x8000_0000;

    /// `__AUDIT_ARCH_LE`, which the kernel sets for every little-endian
    /// personality and clears for ppc64 and the other big-endian ports.
    #[cfg(target_endian = "little")]
    const AUDIT_ARCH_LE: u32 = 0x4000_0000;
    #[cfg(target_endian = "big")]
    const AUDIT_ARCH_LE: u32 = 0;

    /// Value the kernel reports in `seccomp_data.arch`, or `None` on a
    /// 64-bit architecture whose syscall numbering we have not vetted
    /// the allowlist against. The machine numbers are the `EM_*`
    /// values from `elf.h`; the combinations come from `linux/audit.h`.
    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: Option<u32> = Some(62 | AUDIT_ARCH_64BIT | AUDIT_ARCH_LE);
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: Option<u32> = Some(183 | AUDIT_ARCH_64BIT | AUDIT_ARCH_LE);
    #[cfg(target_arch = "riscv64")]
    const AUDIT_ARCH: Option<u32> = Some(243 | AUDIT_ARCH_64BIT | AUDIT_ARCH_LE);
    #[cfg(target_arch = "powerpc64")]
    const AUDIT_ARCH: Option<u32> = Some(21 | AUDIT_ARCH_64BIT | AUDIT_ARCH_LE);
    #[cfg(target_arch = "loongarch64")]
    const AUDIT_ARCH: Option<u32> = Some(258 | AUDIT_ARCH_64BIT | AUDIT_ARCH_LE);
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "powerpc64",
        target_arch = "loongarch64"
    )))]
    const AUDIT_ARCH: Option<u32> = None;

    // `struct seccomp_data` is `{ int nr; __u32 arch; __u64 ip; __u64 args[6]; }`.
    const NR_OFFSET: u32 = 0;
    const ARCH_OFFSET: u32 = 4;
    const ARGS_OFFSET: u32 = 16;

    // The kernel rewrites `BPF_LD | BPF_W | BPF_ABS` into a native-endian
    // word load, so the halves of a 64-bit `args[n]` trade places on
    // big-endian targets, as in libseccomp's `_BPF_ARG_LO` / `_BPF_ARG_HI`.
    #[cfg(target_endian = "little")]
    const ARG0_LOW_HALF: u32 = ARGS_OFFSET;
    #[cfg(target_endian = "big")]
    const ARG0_LOW_HALF: u32 = ARGS_OFFSET + 4;

    const LD_W_ABS: u16 = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
    const JEQ_K: u16 = (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16;
    const AND_K: u16 = (libc::BPF_ALU | libc::BPF_AND | libc::BPF_K) as u16;
    const RET_K: u16 = (libc::BPF_RET | libc::BPF_K) as u16;

    const RET_ALLOW: u32 = libc::SECCOMP_RET_ALLOW;
    const RET_KILL: u32 = libc::SECCOMP_RET_KILL_PROCESS;
    const RET_EPERM: u32 =
        libc::SECCOMP_RET_ERRNO | ((libc::EPERM as u32) & libc::SECCOMP_RET_DATA);

    pub(crate) fn apply() -> SeccompPosture {
        let Some(audit_arch) = AUDIT_ARCH else {
            return SeccompPosture::Unsupported {
                reason: "unsupported architecture",
            };
        };

        let mut program = match build_program(audit_arch) {
            Ok(p) => p,
            Err(reason) => return SeccompPosture::Unsupported { reason },
        };

        let Ok(len) = u16::try_from(program.len()) else {
            return SeccompPosture::Unsupported {
                reason: "filter too long",
            };
        };

        // SAFETY: `prctl` with `PR_SET_NO_NEW_PRIVS` reads no pointers.
        // The kernel requires it before an unprivileged thread may
        // install a filter.
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return SeccompPosture::Unsupported {
                reason: "no_new_privs failed",
            };
        }

        let fprog = libc::sock_fprog {
            len,
            filter: program.as_mut_ptr(),
        };

        // SAFETY: the kernel copies the program out of `fprog` and
        // retains neither pointer, and `program` outlives the call.
        // Without `SECCOMP_FILTER_FLAG_TSYNC` this binds the calling
        // thread only, which is what the worker wants.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_SET_MODE_FILTER,
                0,
                std::ptr::from_ref(&fprog),
            )
        };

        if rc != 0 {
            return SeccompPosture::Unsupported {
                reason: "filter apply failed",
            };
        }

        SeccompPosture::Enforced
    }

    fn stmt(code: u16, k: u32) -> libc::sock_filter {
        libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }

    fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
        libc::sock_filter { code, jt, jf, k }
    }

    fn build_program(audit_arch: u32) -> Result<Vec<libc::sock_filter>, &'static str> {
        let allowed = allowed_syscalls();
        let mut program = Vec::with_capacity(allowed.len() * 2 + 10);

        // The syscall numbers below only mean anything for the personality
        // we were compiled for, and a filter outlives `execve`.
        program.push(stmt(LD_W_ABS, ARCH_OFFSET));
        program.push(jump(JEQ_K, audit_arch, 1, 0));
        program.push(stmt(RET_K, RET_KILL));

        program.push(stmt(LD_W_ABS, NR_OFFSET));
        for nr in allowed {
            let nr = u32::try_from(nr).map_err(|_| "syscall number out of range")?;
            // Pairing each comparison with its own return keeps every jump
            // offset at 0 or 1, well inside a `sock_filter` branch's reach.
            program.push(jump(JEQ_K, nr, 0, 1));
            program.push(stmt(RET_K, RET_ALLOW));
        }

        // `clone` is allowed only when arg 0 (`flags`) carries no
        // namespace-creation bit, keeping `pthread_create` working on libcs
        // that still route through `clone`. Every `CLONE_NEW*` bit lives in
        // the low 32 bits of `flags`, so testing the low half alone is exact.
        let ns_mask = (libc::CLONE_NEWNS
            | libc::CLONE_NEWCGROUP
            | libc::CLONE_NEWUTS
            | libc::CLONE_NEWIPC
            | libc::CLONE_NEWUSER
            | libc::CLONE_NEWPID
            | libc::CLONE_NEWNET) as u32;
        let clone_nr = u32::try_from(libc::SYS_clone).map_err(|_| "syscall number out of range")?;
        program.push(jump(JEQ_K, clone_nr, 0, 4));
        program.push(stmt(LD_W_ABS, ARG0_LOW_HALF));
        program.push(stmt(AND_K, ns_mask));
        program.push(jump(JEQ_K, 0, 0, 1));
        program.push(stmt(RET_K, RET_ALLOW));

        program.push(stmt(RET_K, RET_EPERM));

        if program.len() > BPF_MAX_LEN {
            return Err("filter too long");
        }

        Ok(program)
    }

    /// Syscalls that run unconditionally.
    ///
    /// The list is a structured superset of what upstream glycin allows
    /// inside its bwrap, minus the categories that bwrap would
    /// otherwise contain for them and that we can NOT afford to open up
    /// because we run in-process:
    ///
    ///   - network: socket, connect, bind, listen, accept*, recv*,
    ///     send*, socketcall, getsockopt, setsockopt, getsockname,
    ///     getpeername. Upstream relies on bwrap's network namespace
    ///     making these no-ops. We have no network namespace, so
    ///     allowing them would let a malformed image phone home.
    ///   - process spawn / replace: execve, execveat, fork, vfork.
    ///   - namespace manipulation: unshare, setns, pivot_root, chroot,
    ///     mount, umount, umount2, chdir, fchdir. On ppc64 this also
    ///     denies switch_endian, which would otherwise let a caller
    ///     slip out from under the architecture check above.
    ///   - capability transfer: capget, capset.
    ///   - debug / cross-process memory: ptrace, process_vm_readv,
    ///     process_vm_writev, bpf, perf_event_open, keyctl, request_key.
    ///
    /// Container escape via a newly created namespace still requires
    /// those denied syscalls to be useful, so an unrestricted clone3
    /// (which we have to allow because seccomp BPF cannot dereference
    /// the clone_args struct) is largely defanged.
    fn allowed_syscalls() -> Vec<i64> {
        let mut allowed: Vec<i64> = vec![
            // Process and thread state.
            libc::SYS_exit,
            libc::SYS_exit_group,
            libc::SYS_restart_syscall,
            libc::SYS_rt_sigreturn,
            libc::SYS_rt_sigprocmask,
            libc::SYS_rt_sigaction,
            libc::SYS_sigaltstack,
            libc::SYS_sched_yield,
            libc::SYS_sched_getaffinity,
            libc::SYS_getpriority,
            libc::SYS_setpriority,
            libc::SYS_prctl,
            libc::SYS_gettid,
            libc::SYS_getpid,
            libc::SYS_getppid,
            libc::SYS_getuid,
            libc::SYS_geteuid,
            libc::SYS_getgid,
            libc::SYS_getegid,
            libc::SYS_getrandom,
            libc::SYS_uname,
            libc::SYS_sysinfo,
            libc::SYS_prlimit64,
            libc::SYS_tgkill,
            libc::SYS_set_robust_list,
            libc::SYS_get_robust_list,
            libc::SYS_set_tid_address,
            libc::SYS_rseq,
            libc::SYS_membarrier,
            libc::SYS_wait4,
            // Time and sleep.
            libc::SYS_futex,
            libc::SYS_nanosleep,
            libc::SYS_clock_nanosleep,
            libc::SYS_clock_gettime,
            libc::SYS_clock_getres,
            libc::SYS_gettimeofday,
            // Memory.
            libc::SYS_brk,
            libc::SYS_mmap,
            libc::SYS_munmap,
            libc::SYS_mprotect,
            libc::SYS_mremap,
            libc::SYS_madvise,
            libc::SYS_memfd_create,
            libc::SYS_get_mempolicy,
            libc::SYS_set_mempolicy,
            // I/O on already-open file descriptors.
            libc::SYS_read,
            libc::SYS_write,
            libc::SYS_readv,
            libc::SYS_writev,
            libc::SYS_pread64,
            libc::SYS_pwrite64,
            libc::SYS_lseek,
            libc::SYS_close,
            libc::SYS_close_range,
            libc::SYS_dup,
            libc::SYS_dup3,
            libc::SYS_fcntl,
            libc::SYS_ftruncate,
            libc::SYS_ioctl,
            libc::SYS_fstatfs,
            libc::SYS_statx,
            // File opening and metadata. Landlock, when active,
            // restricts which paths these can reach; without landlock
            // they can read anything the host user can. We document
            // that posture via `SandboxPosture` rather than blocking the
            // syscall here.
            libc::SYS_openat,
            libc::SYS_openat2,
            libc::SYS_getcwd,
            libc::SYS_getdents64,
            libc::SYS_faccessat,
            libc::SYS_faccessat2,
            libc::SYS_readlinkat,
            // Event and poll FDs (timer / signal / pipe / epoll).
            libc::SYS_epoll_create1,
            libc::SYS_epoll_ctl,
            libc::SYS_epoll_pwait,
            libc::SYS_eventfd2,
            libc::SYS_pipe2,
            libc::SYS_ppoll,
            libc::SYS_signalfd4,
            libc::SYS_timerfd_create,
            libc::SYS_timerfd_settime,
            // Thread creation. `clone3` takes a `clone_args` struct by
            // pointer and seccomp BPF cannot dereference user memory, so
            // the flag bits inside the struct cannot be filtered here.
            // Container escape through a fresh namespace still needs
            // syscalls we do not allow (mount, setns, pivot_root,
            // chroot, unshare, execve, socket). `clone` itself is
            // handled by a conditional rule in `build_program`.
            libc::SYS_clone3,
        ];

        // loongarch64 landed after `__ARCH_WANT_NEW_STAT` was retired,
        // so it reaches `statx` above for every stat variant.
        #[cfg(not(target_arch = "loongarch64"))]
        allowed.extend_from_slice(&[libc::SYS_fstat, libc::SYS_newfstatat]);

        // Pre-`at` aliases that glibc still calls into on the ports
        // whose syscall tables predate the asm-generic set.
        #[cfg(any(target_arch = "x86_64", target_arch = "powerpc64"))]
        allowed.extend_from_slice(&[
            libc::SYS_access,
            libc::SYS_creat,
            libc::SYS_dup2,
            libc::SYS_epoll_create,
            libc::SYS_epoll_wait,
            libc::SYS_eventfd,
            libc::SYS_open,
            libc::SYS_pipe,
            libc::SYS_poll,
            libc::SYS_readlink,
            libc::SYS_signalfd,
            libc::SYS_stat,
            libc::SYS_time,
        ]);

        #[cfg(any(
            target_arch = "x86_64",
            target_arch = "riscv64",
            target_arch = "loongarch64"
        ))]
        allowed.push(libc::SYS_fadvise64);

        #[cfg(target_arch = "x86_64")]
        allowed.push(libc::SYS_arch_prctl);

        allowed.sort_unstable();
        allowed.dedup();
        allowed
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// `struct seccomp_data` laid out byte for byte the way the
        /// kernel hands it to the filter. Building it from
        /// `to_ne_bytes` rather than from word indices is what makes
        /// `ARG0_LOW_HALF` a real assertion on big-endian targets.
        fn seccomp_data(nr: u32, arch: u32, arg0: u64) -> [u8; 64] {
            let mut data = [0u8; 64];
            data[0..4].copy_from_slice(&nr.to_ne_bytes());
            data[4..8].copy_from_slice(&arch.to_ne_bytes());
            data[16..24].copy_from_slice(&arg0.to_ne_bytes());
            data
        }

        /// Minimal interpreter for the four opcodes `build_program`
        /// emits, so the jump offsets are exercised on the host. The
        /// word load is native-endian, matching the `BPF_LDX_MEM` the
        /// kernel rewrites `BPF_LD | BPF_W | BPF_ABS` into.
        fn run(program: &[libc::sock_filter], arch: u32, nr: u32, arg0: u64) -> u32 {
            let data = seccomp_data(nr, arch, arg0);
            let mut acc = 0u32;
            let mut pc = 0usize;
            loop {
                let ins = &program[pc];
                pc += 1;
                match ins.code {
                    LD_W_ABS => {
                        let at = ins.k as usize;
                        acc = u32::from_ne_bytes(data[at..at + 4].try_into().unwrap());
                    }
                    AND_K => acc &= ins.k,
                    JEQ_K => {
                        let taken = if acc == ins.k { ins.jt } else { ins.jf };
                        pc += taken as usize;
                    }
                    RET_K => return ins.k,
                    other => panic!("unexpected opcode {other:#x}"),
                }
            }
        }

        /// `None` on 64-bit targets that have no vetted allowlist, where
        /// `imp` still compiles but there is no filter to exercise.
        fn vetted_program() -> Option<(u32, Vec<libc::sock_filter>)> {
            let arch = AUDIT_ARCH?;
            Some((arch, build_program(arch).expect("filter builds")))
        }

        #[test]
        fn allowlisted_syscalls_pass_and_others_are_denied() {
            let Some((arch, program)) = vetted_program() else {
                return;
            };

            for nr in allowed_syscalls() {
                let nr = u32::try_from(nr).unwrap();
                assert_eq!(run(&program, arch, nr, 0), RET_ALLOW, "syscall {nr}");
            }

            let socket = u32::try_from(libc::SYS_socket).unwrap();
            assert_eq!(run(&program, arch, socket, 0), RET_EPERM);
        }

        #[test]
        fn clone_is_allowed_only_without_namespace_flags() {
            let Some((arch, program)) = vetted_program() else {
                return;
            };
            let clone = u32::try_from(libc::SYS_clone).unwrap();

            let thread_flags = (libc::CLONE_VM | libc::CLONE_FS | libc::CLONE_THREAD) as u64;
            assert_eq!(run(&program, arch, clone, thread_flags), RET_ALLOW);
            assert_eq!(
                run(&program, arch, clone, libc::CLONE_NEWUSER as u64),
                RET_EPERM
            );
        }

        #[test]
        fn foreign_architecture_is_killed() {
            let Some((arch, program)) = vetted_program() else {
                return;
            };
            let read = u32::try_from(libc::SYS_read).unwrap();

            assert_eq!(run(&program, arch ^ 1, read, 0), RET_KILL);
        }
    }
}
