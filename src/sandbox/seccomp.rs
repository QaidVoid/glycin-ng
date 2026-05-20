//! Syscall-filter layer using seccomp.

use crate::SeccompPosture;

#[cfg(all(target_os = "linux", feature = "seccomp"))]
pub(crate) fn apply() -> SeccompPosture {
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter};
    use std::collections::BTreeMap;

    let arch = match target_arch() {
        Some(a) => a,
        None => {
            return SeccompPosture::Unsupported {
                reason: "unsupported architecture",
            };
        }
    };

    let mut allowed: Vec<i64> = vec![
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_pread64,
        libc::SYS_pwrite64,
        libc::SYS_close,
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mprotect,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_futex,
        libc::SYS_nanosleep,
        libc::SYS_clock_nanosleep,
        libc::SYS_clock_gettime,
        libc::SYS_getrandom,
        libc::SYS_exit,
        libc::SYS_exit_group,
        libc::SYS_rt_sigreturn,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigaction,
        libc::SYS_sigaltstack,
        libc::SYS_sched_yield,
        libc::SYS_sched_getaffinity,
        libc::SYS_restart_syscall,
        libc::SYS_prctl,
        libc::SYS_gettid,
        libc::SYS_getpid,
        libc::SYS_fstat,
        libc::SYS_lseek,
        libc::SYS_set_robust_list,
        libc::SYS_get_robust_list,
        libc::SYS_rseq,
        libc::SYS_membarrier,
        libc::SYS_tgkill,
    ];
    #[cfg(target_arch = "x86_64")]
    {
        allowed.push(libc::SYS_arch_prctl);
    }

    let rules: BTreeMap<i64, Vec<_>> = allowed.into_iter().map(|sc| (sc, Vec::new())).collect();

    let filter = match SeccompFilter::new(
        rules,
        SeccompAction::Errno(libc::EPERM as u32),
        SeccompAction::Allow,
        arch,
    ) {
        Ok(f) => f,
        Err(_) => {
            return SeccompPosture::Unsupported {
                reason: "filter build failed",
            };
        }
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
