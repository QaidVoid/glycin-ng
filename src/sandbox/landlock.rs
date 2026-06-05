//! Filesystem-restriction layer using Linux landlock.

use crate::LandlockPosture;

#[cfg(all(target_os = "linux", feature = "landlock"))]
pub(crate) fn apply() -> LandlockPosture {
    use landlock::{ABI, Access, AccessFs, AccessNet, Ruleset, RulesetAttr, RulesetStatus, Scope};

    // Probe the kernel's maximum supported ABI for reporting. The
    // `landlock_create_ruleset` syscall with a NULL attr and the
    // `_VERSION` flag returns the highest ABI version the running
    // kernel knows about (1..=N), or -ENOSYS on kernels predating
    // landlock entirely.
    let kernel_abi = probe_kernel_abi();
    if kernel_abi == 0 {
        return LandlockPosture::Unsupported {
            reason: "kernel does not support landlock",
        };
    }

    // Build the strictest ruleset known to the crate (ABI V6). The
    // default `CompatLevel::BestEffort` downgrades any feature the
    // running kernel does not understand. With zero `add_rule` calls
    // every handled access type is denied: no path the decoder did
    // not already have open is reachable, no TCP bind / connect is
    // permitted on V4+, and no signal / abstract-unix-socket escape
    // out of the scope is permitted on V6+.
    let ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(ABI::V6))
        .and_then(|r| r.handle_access(AccessNet::BindTcp | AccessNet::ConnectTcp))
        .and_then(|r| r.scope(Scope::AbstractUnixSocket | Scope::Signal));
    let ruleset = match ruleset {
        Ok(r) => r,
        Err(_) => {
            return LandlockPosture::Unsupported {
                reason: "handle_access failed",
            };
        }
    };

    let created = match ruleset.create() {
        Ok(c) => c,
        Err(_) => {
            return LandlockPosture::Unsupported {
                reason: "ruleset create failed",
            };
        }
    };
    let status = match created.restrict_self() {
        Ok(s) => s,
        Err(_) => {
            return LandlockPosture::Unsupported {
                reason: "restrict_self failed",
            };
        }
    };

    match status.ruleset {
        RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => {
            LandlockPosture::Enforced { abi: kernel_abi }
        }
        RulesetStatus::NotEnforced => LandlockPosture::Unsupported {
            reason: "kernel does not support landlock",
        },
    }
}

#[cfg(all(target_os = "linux", feature = "landlock"))]
fn probe_kernel_abi() -> u32 {
    // LANDLOCK_CREATE_RULESET_VERSION = 1, from
    // include/uapi/linux/landlock.h.
    const VERSION_FLAG: u32 = 1;
    // SAFETY: passing a NULL attr pointer with size 0 and the
    // VERSION flag is the documented way to query the supported
    // ABI; the kernel does not dereference the pointer.
    let v = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0_usize,
            VERSION_FLAG,
        )
    };
    if v < 0 { 0 } else { v as u32 }
}

#[cfg(not(all(target_os = "linux", feature = "landlock")))]
pub(crate) fn apply() -> LandlockPosture {
    LandlockPosture::Disabled
}
