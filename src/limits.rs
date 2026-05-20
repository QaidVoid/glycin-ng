//! Per-decode resource caps.

use std::time::Duration;

/// Resource ceilings applied to a single decode.
///
/// Limits are enforced in two places: pre-decode header inspection
/// (`max_width`, `max_height`, `max_pixels`, `max_frames`,
/// `max_animation_duration`) and process-wide `setrlimit` when the
/// sandbox is on (`decode_memory_mib` -> `RLIMIT_AS`,
/// `decode_cpu_seconds` -> `RLIMIT_CPU`).
///
/// Defaults are sized for ordinary desktop and server workloads.
/// Callers loading user-supplied images should keep them; callers
/// loading their own trusted assets can raise or remove them by
/// constructing the struct directly.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum image width in pixels. Default: 32768.
    pub max_width: u32,
    /// Maximum image height in pixels. Default: 32768.
    pub max_height: u32,
    /// Maximum total pixels (width times height times frame count).
    /// Default: 256 MiPx.
    pub max_pixels: u64,
    /// Maximum animation frame count. Default: 1024.
    pub max_frames: u32,
    /// Maximum animation duration summed across frames. Default:
    /// 60 seconds.
    pub max_animation_duration: Duration,
    /// `RLIMIT_AS` cap in MiB applied when the sandbox is on.
    /// Default: 512.
    pub decode_memory_mib: u64,
    /// `RLIMIT_CPU` cap in seconds applied when the sandbox is on.
    /// Default: 30.
    pub decode_cpu_seconds: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_width: 32_768,
            max_height: 32_768,
            max_pixels: 256 * 1024 * 1024,
            max_frames: 1024,
            max_animation_duration: Duration::from_secs(60),
            decode_memory_mib: 512,
            decode_cpu_seconds: 30,
        }
    }
}

impl Limits {
    /// All caps removed. Use only for trusted input.
    pub fn unlimited() -> Self {
        Self {
            max_width: u32::MAX,
            max_height: u32::MAX,
            max_pixels: u64::MAX,
            max_frames: u32::MAX,
            max_animation_duration: Duration::MAX,
            decode_memory_mib: u64::MAX,
            decode_cpu_seconds: u64::MAX,
        }
    }

    /// Reject the image if width times height exceeds `max_pixels` or
    /// either dimension exceeds its cap.
    ///
    /// `frames` is the number of frames the decoder will produce
    /// (1 for non-animated formats). Pre-decode header inspection
    /// should call this with the values pulled from the header.
    pub fn check_dimensions(&self, width: u32, height: u32, frames: u32) -> crate::Result<()> {
        if width > self.max_width {
            return Err(crate::Error::LimitExceeded("max_width"));
        }
        if height > self.max_height {
            return Err(crate::Error::LimitExceeded("max_height"));
        }
        if frames > self.max_frames {
            return Err(crate::Error::LimitExceeded("max_frames"));
        }
        let total = (width as u64)
            .checked_mul(height as u64)
            .and_then(|p| p.checked_mul(frames as u64));
        match total {
            Some(p) if p > self.max_pixels => Err(crate::Error::LimitExceeded("max_pixels")),
            None => Err(crate::Error::LimitExceeded("max_pixels")),
            Some(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documentation() {
        let d = Limits::default();
        assert_eq!(d.max_width, 32_768);
        assert_eq!(d.max_height, 32_768);
        assert_eq!(d.max_pixels, 256 * 1024 * 1024);
        assert_eq!(d.max_frames, 1024);
        assert_eq!(d.max_animation_duration, Duration::from_secs(60));
        assert_eq!(d.decode_memory_mib, 512);
        assert_eq!(d.decode_cpu_seconds, 30);
    }

    #[test]
    fn check_dimensions_accepts_small_images() {
        let l = Limits::default();
        l.check_dimensions(1920, 1080, 1).unwrap();
        l.check_dimensions(1, 1, 1).unwrap();
    }

    #[test]
    fn check_dimensions_rejects_huge_width() {
        let l = Limits::default();
        let e = l.check_dimensions(40_000, 100, 1).unwrap_err();
        assert!(matches!(e, crate::Error::LimitExceeded("max_width")));
    }

    #[test]
    fn check_dimensions_rejects_huge_pixels() {
        let l = Limits::default();
        let e = l.check_dimensions(20_000, 20_000, 1).unwrap_err();
        assert!(matches!(e, crate::Error::LimitExceeded("max_pixels")));
    }

    #[test]
    fn check_dimensions_rejects_overflow() {
        let l = Limits::unlimited();
        // Force the multiplication to overflow u64 even though limits
        // are unbounded; we still report a LimitExceeded.
        let e = l
            .check_dimensions(u32::MAX, u32::MAX, u32::MAX)
            .unwrap_err();
        assert!(matches!(e, crate::Error::LimitExceeded("max_pixels")));
    }

    #[test]
    fn check_dimensions_rejects_too_many_frames() {
        let l = Limits::default();
        let e = l.check_dimensions(10, 10, 2000).unwrap_err();
        assert!(matches!(e, crate::Error::LimitExceeded("max_frames")));
    }
}
