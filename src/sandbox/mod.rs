//! Sandbox primitives and posture reporting.
//!
//! The decoder runs on a dedicated worker thread. The
//! [`run_in_worker`] entry point spawns that thread, applies every
//! requested layer in irreversible-after order
//! (`rlimit` -> `landlock` -> `seccomp`), runs the supplied closure,
//! and joins the thread before returning. A worker panic becomes
//! [`Error::Internal`](crate::Error::Internal).

pub(crate) mod landlock;
pub(crate) mod rlimit;
pub(crate) mod seccomp;

use crate::{Error, Limits, Result};

/// Result of applying every requested sandbox layer for one decode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SandboxPosture {
    /// Filesystem restrictions, see [`LandlockPosture`].
    pub landlock: LandlockPosture,
    /// Syscall filter, see [`SeccompPosture`].
    pub seccomp: SeccompPosture,
    /// Per-process resource caps, see [`RlimitPosture`].
    pub rlimit: RlimitPosture,
}

impl SandboxPosture {
    /// Posture in which no layer is active. Used when sandbox
    /// features are off at build time or the platform lacks support.
    pub const fn none() -> Self {
        Self {
            landlock: LandlockPosture::Disabled,
            seccomp: SeccompPosture::Disabled,
            rlimit: RlimitPosture::Disabled,
        }
    }

    /// Whether every layer reports as actively enforcing
    /// restrictions.
    pub fn is_fully_enforced(self) -> bool {
        matches!(self.landlock, LandlockPosture::Enforced { .. })
            && matches!(self.seccomp, SeccompPosture::Enforced)
            && matches!(self.rlimit, RlimitPosture::Applied { .. })
    }
}

impl Default for SandboxPosture {
    fn default() -> Self {
        Self::none()
    }
}

/// Filesystem-restriction layer status.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LandlockPosture {
    /// Landlock was applied; the value is the ruleset ABI version
    /// negotiated with the kernel.
    Enforced {
        /// Negotiated ABI version.
        abi: u32,
    },
    /// Kernel does not support landlock or rejected the ruleset.
    Unsupported {
        /// Short reason string for logging.
        reason: &'static str,
    },
    /// The `landlock` Cargo feature was off, or the loader was asked
    /// to skip this layer.
    Disabled,
}

/// Syscall-filter layer status.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SeccompPosture {
    /// Filter program installed on the decode thread.
    Enforced,
    /// Kernel rejected the filter or the platform does not support
    /// seccomp.
    Unsupported {
        /// Short reason string for logging.
        reason: &'static str,
    },
    /// The `seccomp` Cargo feature was off, or the loader was asked
    /// to skip this layer.
    Disabled,
}

/// `setrlimit` layer status.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RlimitPosture {
    /// Both `RLIMIT_AS` and `RLIMIT_CPU` applied.
    Applied {
        /// `RLIMIT_AS` cap in MiB.
        as_mib: u64,
        /// `RLIMIT_CPU` cap in seconds.
        cpu_seconds: u64,
    },
    /// One of the two limits could not be set.
    PartiallyApplied {
        /// Short reason string for logging.
        detail: &'static str,
    },
    /// The loader was asked to skip this layer.
    Disabled,
}

/// Caller-side selection of which sandbox layers to attempt.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SandboxSelector {
    /// Attempt landlock.
    pub landlock: bool,
    /// Attempt seccomp.
    pub seccomp: bool,
    /// Apply `RLIMIT_AS` and `RLIMIT_CPU` from
    /// [`Limits`](crate::Limits). Off by default because both limits
    /// are process-wide and affect the host's main thread.
    pub rlimit: bool,
    /// Refuse the decode if any selected layer fails to apply.
    pub strict: bool,
}

impl Default for SandboxSelector {
    fn default() -> Self {
        Self {
            landlock: true,
            seccomp: true,
            rlimit: false,
            strict: false,
        }
    }
}

impl SandboxSelector {
    /// No sandbox at all.
    pub const fn none() -> Self {
        Self {
            landlock: false,
            seccomp: false,
            rlimit: false,
            strict: false,
        }
    }
}

/// Run `work` on a dedicated worker thread with the selected
/// sandbox layers applied.
///
/// Returns the work's result along with the
/// [`SandboxPosture`] actually applied in the worker. Worker
/// panics become [`Error::Internal`].
pub(crate) fn run_in_worker<F, R>(
    selector: SandboxSelector,
    limits: Limits,
    work: F,
) -> Result<(R, SandboxPosture)>
where
    F: FnOnce() -> Result<R> + Send + 'static,
    R: Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name("glycin-ng-worker".into())
        .spawn(move || -> Result<(R, SandboxPosture)> {
            let posture = apply_layers(selector, limits);
            check_strict(&selector, &posture)?;
            let r = work()?;
            Ok((r, posture))
        })
        .map_err(Error::Io)?;

    match handle.join() {
        Ok(r) => r,
        Err(payload) => Err(Error::Internal(panic_message(payload))),
    }
}

fn apply_layers(s: SandboxSelector, l: Limits) -> SandboxPosture {
    let rlimit = if s.rlimit {
        rlimit::apply(l.decode_memory_mib, l.decode_cpu_seconds)
    } else {
        RlimitPosture::Disabled
    };
    let landlock = if s.landlock {
        landlock::apply()
    } else {
        LandlockPosture::Disabled
    };
    let seccomp = if s.seccomp {
        seccomp::apply()
    } else {
        SeccompPosture::Disabled
    };
    SandboxPosture {
        landlock,
        seccomp,
        rlimit,
    }
}

fn check_strict(s: &SandboxSelector, p: &SandboxPosture) -> Result<()> {
    if !s.strict {
        return Ok(());
    }
    if s.landlock && !matches!(p.landlock, LandlockPosture::Enforced { .. }) {
        return Err(Error::SandboxUnavailable("landlock"));
    }
    if s.seccomp && !matches!(p.seccomp, SeccompPosture::Enforced) {
        return Err(Error::SandboxUnavailable("seccomp"));
    }
    if s.rlimit && !matches!(p.rlimit, RlimitPosture::Applied { .. }) {
        return Err(Error::SandboxUnavailable("rlimit"));
    }
    Ok(())
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        format!("decode worker panicked: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("decode worker panicked: {s}")
    } else {
        "decode worker panicked".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_not_fully_enforced() {
        assert!(!SandboxPosture::none().is_fully_enforced());
    }

    #[test]
    fn enforced_posture_reports_fully_enforced() {
        let p = SandboxPosture {
            landlock: LandlockPosture::Enforced { abi: 5 },
            seccomp: SeccompPosture::Enforced,
            rlimit: RlimitPosture::Applied {
                as_mib: 512,
                cpu_seconds: 30,
            },
        };
        assert!(p.is_fully_enforced());
    }

    #[test]
    fn one_layer_disabled_breaks_full_enforcement() {
        let p = SandboxPosture {
            landlock: LandlockPosture::Disabled,
            seccomp: SeccompPosture::Enforced,
            rlimit: RlimitPosture::Applied {
                as_mib: 512,
                cpu_seconds: 30,
            },
        };
        assert!(!p.is_fully_enforced());
    }

    #[test]
    fn selector_default_attempts_landlock_and_seccomp() {
        let s = SandboxSelector::default();
        assert!(s.landlock);
        assert!(s.seccomp);
        assert!(!s.rlimit);
        assert!(!s.strict);
    }

    #[test]
    fn selector_none_disables_every_layer() {
        let s = SandboxSelector::none();
        assert!(!s.landlock);
        assert!(!s.seccomp);
        assert!(!s.rlimit);
    }

    #[test]
    fn worker_runs_closure() {
        let (r, _) =
            run_in_worker(SandboxSelector::none(), Limits::default(), || Ok(42_i32)).unwrap();
        assert_eq!(r, 42);
    }

    #[test]
    fn worker_panic_becomes_internal_error() {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r: Result<((), SandboxPosture)> = run_in_worker(
            SandboxSelector::none(),
            Limits::default(),
            || -> Result<()> { panic!("boom") },
        );
        std::panic::set_hook(prev);
        let err = r.unwrap_err();
        assert!(matches!(err, Error::Internal(_)), "got: {err:?}");
        if let Error::Internal(msg) = err {
            assert!(msg.contains("boom"), "got: {msg}");
        }
    }

    #[test]
    fn strict_mode_fails_when_landlock_unavailable() {
        let selector = SandboxSelector {
            landlock: true,
            seccomp: false,
            rlimit: false,
            strict: true,
        };
        let posture = SandboxPosture {
            landlock: LandlockPosture::Unsupported {
                reason: "no kernel support",
            },
            seccomp: SeccompPosture::Disabled,
            rlimit: RlimitPosture::Disabled,
        };
        let err = check_strict(&selector, &posture).unwrap_err();
        assert!(matches!(err, Error::SandboxUnavailable("landlock")));
    }

    #[test]
    fn non_strict_mode_accepts_unavailable_layer() {
        let selector = SandboxSelector {
            landlock: true,
            seccomp: true,
            rlimit: true,
            strict: false,
        };
        let posture = SandboxPosture::none();
        check_strict(&selector, &posture).unwrap();
    }

    #[test]
    fn disabled_selector_reports_disabled_posture() {
        let (_, posture) =
            run_in_worker(SandboxSelector::none(), Limits::default(), || Ok(())).unwrap();
        assert_eq!(posture, SandboxPosture::none());
    }

    #[cfg(all(target_os = "linux", feature = "landlock"))]
    #[test]
    fn landlock_blocks_arbitrary_fs_read() {
        let selector = SandboxSelector {
            landlock: true,
            seccomp: false,
            rlimit: false,
            strict: false,
        };
        let (blocked, posture) =
            run_in_worker(selector, Limits::default(), || {
                match std::fs::read("/etc/hostname") {
                    Ok(_) => Ok(false),
                    Err(_) => Ok(true),
                }
            })
            .unwrap();
        if matches!(posture.landlock, LandlockPosture::Enforced { .. }) {
            assert!(
                blocked,
                "landlock was enforced but /etc/hostname was readable"
            );
        }
    }

    #[cfg(all(target_os = "linux", feature = "seccomp"))]
    #[test]
    fn seccomp_denies_unlisted_syscall() {
        let selector = SandboxSelector {
            landlock: false,
            seccomp: true,
            rlimit: false,
            strict: false,
        };
        let (rc_and_errno, posture) = run_in_worker(selector, Limits::default(), || {
            // SYS_socket is intentionally outside the allowlist because
            // network containment is a core property of the in-process
            // sandbox. Expect -1 with errno EPERM.
            // SAFETY: only invokes a single syscall through libc.
            let rc =
                unsafe { libc::syscall(libc::SYS_socket, libc::AF_INET, libc::SOCK_STREAM, 0_i32) };
            let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            Ok((rc, err))
        })
        .unwrap();
        if matches!(posture.seccomp, SeccompPosture::Enforced) {
            assert_eq!(rc_and_errno.0, -1, "syscall should have been denied");
            assert_eq!(
                rc_and_errno.1,
                libc::EPERM,
                "denied syscall should set errno = EPERM"
            );
        }
    }
}
