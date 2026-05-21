//! Syscall-filter layer using seccomp.

use crate::SeccompPosture;

#[cfg(all(target_os = "linux", feature = "seccomp"))]
pub(crate) fn apply() -> SeccompPosture {
    use seccompiler::BpfProgram;

    let arch = match target_arch() {
        Some(a) => a,
        None => {
            return SeccompPosture::Unsupported {
                reason: "unsupported architecture",
            };
        }
    };

    let filter = match build_filter(arch) {
        Ok(f) => f,
        Err(reason) => return SeccompPosture::Unsupported { reason },
    };

    let program: BpfProgram = match filter.try_into() {
        Ok(p) => p,
        Err(_) => {
            return SeccompPosture::Unsupported {
                reason: "filter compile failed",
            };
        }
    };

    if seccompiler::apply_filter(&program).is_err() {
        return SeccompPosture::Unsupported {
            reason: "filter apply failed",
        };
    }

    SeccompPosture::Enforced
}

#[cfg(all(target_os = "linux", feature = "seccomp"))]
fn build_filter(arch: seccompiler::TargetArch) -> Result<seccompiler::SeccompFilter, &'static str> {
    use seccompiler::{
        SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule,
    };
    use std::collections::BTreeMap;

    // The allowlist below is a structured superset of what upstream
    // glycin allows inside its bwrap, minus the categories that bwrap
    // would otherwise contain for them and that we can NOT afford to
    // open up because we run in-process:
    //
    //   - network: socket, connect, bind, listen, accept*, recv*,
    //     send*, socketcall, getsockopt, setsockopt, getsockname,
    //     getpeername. Upstream relies on bwrap's network namespace
    //     making these no-ops. We have no network namespace, so
    //     allowing them would let a malformed image phone home.
    //   - process spawn / replace: execve, execveat, fork, vfork.
    //   - namespace manipulation: unshare, setns, pivot_root, chroot,
    //     mount, umount, umount2, chdir, fchdir.
    //   - capability transfer: capget, capset.
    //   - debug / cross-process memory: ptrace, process_vm_readv,
    //     process_vm_writev, bpf, perf_event_open, keyctl, request_key.
    //
    // Container escape via a newly created namespace still requires
    // those denied syscalls to be useful, so an unrestricted clone3
    // (which we have to allow because seccomp BPF cannot dereference
    // the clone_args struct) is largely defanged.
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
        libc::SYS_fstat,
        libc::SYS_fstatfs,
        libc::SYS_statx,
        libc::SYS_newfstatat,
        libc::SYS_fadvise64,
        // File opening and metadata. Landlock, when active, restricts
        // which paths these can reach; without landlock they can read
        // anything the host user can. We document that posture via
        // `SandboxPosture` rather than blocking the syscall here.
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
        // syscalls we do not allow (mount, setns, pivot_root, chroot,
        // unshare, execve, socket).
        libc::SYS_clone3,
    ];

    #[cfg(target_arch = "x86_64")]
    {
        // Legacy aliases that glibc on x86_64 still calls into.
        allowed.extend_from_slice(&[
            libc::SYS_arch_prctl,
            libc::SYS_access,
            libc::SYS_open,
            libc::SYS_stat,
            libc::SYS_creat,
            libc::SYS_dup2,
            libc::SYS_epoll_create,
            libc::SYS_epoll_wait,
            libc::SYS_eventfd,
            libc::SYS_pipe,
            libc::SYS_poll,
            libc::SYS_readlink,
            libc::SYS_signalfd,
            libc::SYS_time,
        ]);
    }

    let mut rules: BTreeMap<i64, Vec<SeccompRule>> =
        allowed.into_iter().map(|sc| (sc, Vec::new())).collect();

    // `clone` is allowed only when no namespace-creation flags are set
    // in arg 0 (`flags`). This keeps pthread_create working on glibcs
    // that still call `clone` while denying CLONE_NEW* attempts.
    let ns_mask: u64 = (libc::CLONE_NEWNS
        | libc::CLONE_NEWCGROUP
        | libc::CLONE_NEWUTS
        | libc::CLONE_NEWIPC
        | libc::CLONE_NEWUSER
        | libc::CLONE_NEWPID
        | libc::CLONE_NEWNET) as u64;
    let no_ns_cond = SeccompCondition::new(
        0,
        SeccompCmpArgLen::Qword,
        SeccompCmpOp::MaskedEq(ns_mask),
        0,
    )
    .map_err(|_| "filter build failed")?;
    let no_ns_rule = SeccompRule::new(vec![no_ns_cond]).map_err(|_| "filter build failed")?;
    rules.insert(libc::SYS_clone, vec![no_ns_rule]);

    SeccompFilter::new(
        rules,
        SeccompAction::Errno(libc::EPERM as u32),
        SeccompAction::Allow,
        arch,
    )
    .map_err(|_| "filter build failed")
}

#[cfg(all(target_os = "linux", feature = "seccomp"))]
fn target_arch() -> Option<seccompiler::TargetArch> {
    match std::env::consts::ARCH {
        "x86_64" => Some(seccompiler::TargetArch::x86_64),
        "aarch64" => Some(seccompiler::TargetArch::aarch64),
        _ => None,
    }
}

#[cfg(not(all(target_os = "linux", feature = "seccomp")))]
pub(crate) fn apply() -> SeccompPosture {
    SeccompPosture::Disabled
}
