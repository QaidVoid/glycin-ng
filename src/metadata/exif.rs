//! Minimal EXIF parser focused on Orientation and resolution.
//!
//! Strict enough to read the TIFF-formatted EXIF blob attached to
//! PNG, JPEG, and WebP images and report the
//! [Orientation](crate::Orientation) tag value (`0x0112`) and the
//! pixel density from the resolution tags (`0x011A`, `0x011B`,
//! `0x0128`). Anything else returns `None` rather than erroring.

use crate::{PhysicalDimensionUnit, PixelDensity};

const EXIF_PREFIX: &[u8] = b"Exif\0\0";
const ORIENTATION_TAG: u16 = 0x0112;
const X_RESOLUTION_TAG: u16 = 0x011A;
const Y_RESOLUTION_TAG: u16 = 0x011B;
const RESOLUTION_UNIT_TAG: u16 = 0x0128;
const IFD_ENTRY_BYTES: usize = 12;

/// Read the Orientation tag value from an EXIF blob, if present.
///
/// Returns the raw tag value (1..=8 are valid EXIF orientations,
/// other values are reported as-is and treated as
/// [`Orientation::Normal`](crate::Orientation::Normal) downstream).
pub(crate) fn parse_orientation(blob: &[u8]) -> Option<u16> {
    let tiff = strip_exif_prefix(blob);
    if tiff.len() < 8 {
        return None;
    }
    let big_endian = match &tiff[0..4] {
        b"MM\0*" => true,
        b"II*\0" => false,
        _ => return None,
    };
    let ifd_offset = read_u32(&tiff[4..8], big_endian) as usize;
    let ifd = tiff.get(ifd_offset..)?;
    if ifd.len() < 2 {
        return None;
    }
    let num_entries = read_u16(&ifd[..2], big_endian) as usize;
    let entries_start = ifd_offset.checked_add(2)?;
    let entries_end = entries_start.checked_add(num_entries.checked_mul(IFD_ENTRY_BYTES)?)?;
    let entries = tiff.get(entries_start..entries_end)?;

    for i in 0..num_entries {
        let off = i * IFD_ENTRY_BYTES;
        let entry = &entries[off..off + IFD_ENTRY_BYTES];
        let tag = read_u16(&entry[0..2], big_endian);
        if tag != ORIENTATION_TAG {
            continue;
        }
        // SHORT type (3) with count 1; value sits in the first two
        // bytes of the value/offset field at entry[8..10].
        return Some(read_u16(&entry[8..10], big_endian));
    }
    None
}

/// Read pixel density from the EXIF resolution tags, if present.
///
/// Requires `XResolution` and `YResolution` (RATIONAL) plus
/// `ResolutionUnit` (SHORT): 2 maps to inch, 3 to centimeter.
/// Any other unit, missing tag, or non-positive value returns `None`,
/// matching upstream gufo behavior.
pub(crate) fn parse_resolution(blob: &[u8]) -> Option<PixelDensity> {
    let tiff = strip_exif_prefix(blob);
    if tiff.len() < 8 {
        return None;
    }
    let big_endian = match &tiff[0..4] {
        b"MM\0*" => true,
        b"II*\0" => false,
        _ => return None,
    };
    let ifd_offset = read_u32(&tiff[4..8], big_endian) as usize;
    let ifd = tiff.get(ifd_offset..)?;
    if ifd.len() < 2 {
        return None;
    }
    let num_entries = read_u16(&ifd[..2], big_endian) as usize;
    let entries_start = ifd_offset.checked_add(2)?;
    let entries_end = entries_start.checked_add(num_entries.checked_mul(IFD_ENTRY_BYTES)?)?;
    let entries = tiff.get(entries_start..entries_end)?;

    let mut x: Option<f64> = None;
    let mut y: Option<f64> = None;
    let mut unit: Option<PhysicalDimensionUnit> = None;
    for i in 0..num_entries {
        let off = i * IFD_ENTRY_BYTES;
        let entry = &entries[off..off + IFD_ENTRY_BYTES];
        match read_u16(&entry[0..2], big_endian) {
            X_RESOLUTION_TAG => x = read_rational(tiff, entry, big_endian),
            Y_RESOLUTION_TAG => y = read_rational(tiff, entry, big_endian),
            RESOLUTION_UNIT_TAG => {
                unit = match read_u16(&entry[8..10], big_endian) {
                    2 => Some(PhysicalDimensionUnit::Inch),
                    3 => Some(PhysicalDimensionUnit::Centimeter),
                    _ => None,
                };
            }
            _ => {}
        }
    }
    let (x, y, unit) = (x?, y?, unit?);
    if x <= 0.0 || y <= 0.0 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(PixelDensity::new(x, unit, y, unit))
}

/// Read a type-5 RATIONAL (count 1) entry value as `f64`.
fn read_rational(tiff: &[u8], entry: &[u8], big_endian: bool) -> Option<f64> {
    if read_u16(&entry[2..4], big_endian) != 5 {
        return None;
    }
    if read_u32(&entry[4..8], big_endian) != 1 {
        return None;
    }
    let offset = read_u32(&entry[8..12], big_endian) as usize;
    let bytes = tiff.get(offset..offset.checked_add(8)?)?;
    let num = read_u32(&bytes[..4], big_endian) as f64;
    let den = read_u32(&bytes[4..8], big_endian) as f64;
    if den == 0.0 {
        return None;
    }
    Some(num / den)
}

fn strip_exif_prefix(blob: &[u8]) -> &[u8] {
    if blob.starts_with(EXIF_PREFIX) {
        &blob[EXIF_PREFIX.len()..]
    } else {
        blob
    }
}

fn read_u16(b: &[u8], big_endian: bool) -> u16 {
    let arr = [b[0], b[1]];
    if big_endian {
        u16::from_be_bytes(arr)
    } else {
        u16::from_le_bytes(arr)
    }
}

fn read_u32(b: &[u8], big_endian: bool) -> u32 {
    let arr = [b[0], b[1], b[2], b[3]];
    if big_endian {
        u32::from_be_bytes(arr)
    } else {
        u32::from_le_bytes(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_tiff_with_orientation(big_endian: bool, value: u16) -> Vec<u8> {
        let mut blob = Vec::new();
        if big_endian {
            blob.extend_from_slice(b"MM\0*");
            blob.extend_from_slice(&8_u32.to_be_bytes());
        } else {
            blob.extend_from_slice(b"II*\0");
            blob.extend_from_slice(&8_u32.to_le_bytes());
        }
        // IFD starts at byte 8: 2-byte entry count, then entries.
        let count: u16 = 1;
        if big_endian {
            blob.extend_from_slice(&count.to_be_bytes());
            blob.extend_from_slice(&ORIENTATION_TAG.to_be_bytes());
            blob.extend_from_slice(&3_u16.to_be_bytes()); // type SHORT
            blob.extend_from_slice(&1_u32.to_be_bytes()); // count
            blob.extend_from_slice(&value.to_be_bytes());
            blob.extend_from_slice(&0_u16.to_be_bytes()); // padding
        } else {
            blob.extend_from_slice(&count.to_le_bytes());
            blob.extend_from_slice(&ORIENTATION_TAG.to_le_bytes());
            blob.extend_from_slice(&3_u16.to_le_bytes());
            blob.extend_from_slice(&1_u32.to_le_bytes());
            blob.extend_from_slice(&value.to_le_bytes());
            blob.extend_from_slice(&0_u16.to_le_bytes());
        }
        blob
    }

    #[test]
    fn reads_orientation_little_endian() {
        let blob = build_tiff_with_orientation(false, 6);
        assert_eq!(parse_orientation(&blob), Some(6));
    }

    #[test]
    fn reads_orientation_big_endian() {
        let blob = build_tiff_with_orientation(true, 3);
        assert_eq!(parse_orientation(&blob), Some(3));
    }

    #[test]
    fn handles_exif_prefix() {
        let mut blob = b"Exif\0\0".to_vec();
        blob.extend_from_slice(&build_tiff_with_orientation(false, 8));
        assert_eq!(parse_orientation(&blob), Some(8));
    }

    #[test]
    fn no_orientation_tag_returns_none() {
        let mut blob = Vec::new();
        blob.extend_from_slice(b"II*\0");
        blob.extend_from_slice(&8_u32.to_le_bytes());
        blob.extend_from_slice(&0_u16.to_le_bytes()); // zero entries
        assert_eq!(parse_orientation(&blob), None);
    }

    #[test]
    fn rejects_bogus_byte_order() {
        let blob = b"XX*\0\x08\0\0\0";
        assert_eq!(parse_orientation(blob), None);
    }

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(parse_orientation(b""), None);
        assert_eq!(parse_orientation(b"Exif\0\0"), None);
    }

    #[test]
    fn truncated_ifd_returns_none() {
        let mut blob = Vec::new();
        blob.extend_from_slice(b"II*\0");
        blob.extend_from_slice(&8_u32.to_le_bytes());
        // No IFD bytes at offset 8.
        assert_eq!(parse_orientation(&blob), None);
    }

    fn build_tiff_with_resolution(
        big_endian: bool,
        x_num: u32,
        x_den: u32,
        y_num: u32,
        y_den: u32,
        unit: u16,
    ) -> Vec<u8> {
        let mut blob = Vec::new();
        if big_endian {
            blob.extend_from_slice(b"MM\0*");
            blob.extend_from_slice(&8_u32.to_be_bytes());
        } else {
            blob.extend_from_slice(b"II*\0");
            blob.extend_from_slice(&8_u32.to_le_bytes());
        }
        // IFD with 3 entries; rational data appended after entries.
        // Header (8) + count (2) + 3*12 entries = 46; data at 46.
        let x_off: u32 = 46;
        let y_off: u32 = 54;
        let count: u16 = 3;
        if big_endian {
            blob.extend_from_slice(&count.to_be_bytes());
            blob.extend_from_slice(&X_RESOLUTION_TAG.to_be_bytes());
            blob.extend_from_slice(&5_u16.to_be_bytes());
            blob.extend_from_slice(&1_u32.to_be_bytes());
            blob.extend_from_slice(&x_off.to_be_bytes());
            blob.extend_from_slice(&Y_RESOLUTION_TAG.to_be_bytes());
            blob.extend_from_slice(&5_u16.to_be_bytes());
            blob.extend_from_slice(&1_u32.to_be_bytes());
            blob.extend_from_slice(&y_off.to_be_bytes());
            blob.extend_from_slice(&RESOLUTION_UNIT_TAG.to_be_bytes());
            blob.extend_from_slice(&3_u16.to_be_bytes());
            blob.extend_from_slice(&1_u32.to_be_bytes());
            blob.extend_from_slice(&unit.to_be_bytes());
            blob.extend_from_slice(&0_u16.to_be_bytes());
            blob.extend_from_slice(&x_num.to_be_bytes());
            blob.extend_from_slice(&x_den.to_be_bytes());
            blob.extend_from_slice(&y_num.to_be_bytes());
            blob.extend_from_slice(&y_den.to_be_bytes());
        } else {
            blob.extend_from_slice(&count.to_le_bytes());
            blob.extend_from_slice(&X_RESOLUTION_TAG.to_le_bytes());
            blob.extend_from_slice(&5_u16.to_le_bytes());
            blob.extend_from_slice(&1_u32.to_le_bytes());
            blob.extend_from_slice(&x_off.to_le_bytes());
            blob.extend_from_slice(&Y_RESOLUTION_TAG.to_le_bytes());
            blob.extend_from_slice(&5_u16.to_le_bytes());
            blob.extend_from_slice(&1_u32.to_le_bytes());
            blob.extend_from_slice(&y_off.to_le_bytes());
            blob.extend_from_slice(&RESOLUTION_UNIT_TAG.to_le_bytes());
            blob.extend_from_slice(&3_u16.to_le_bytes());
            blob.extend_from_slice(&1_u32.to_le_bytes());
            blob.extend_from_slice(&unit.to_le_bytes());
            blob.extend_from_slice(&0_u16.to_le_bytes());
            blob.extend_from_slice(&x_num.to_le_bytes());
            blob.extend_from_slice(&x_den.to_le_bytes());
            blob.extend_from_slice(&y_num.to_le_bytes());
            blob.extend_from_slice(&y_den.to_le_bytes());
        }
        blob
    }

    #[test]
    fn reads_resolution_little_endian() {
        let blob = build_tiff_with_resolution(false, 300, 1, 300, 1, 2);
        let d = parse_resolution(&blob).expect("density");
        assert_eq!(d.x_value, 300.0);
        assert_eq!(d.y_value, 300.0);
        assert_eq!(d.x_unit, PhysicalDimensionUnit::Inch);
        assert_eq!(d.y_unit, PhysicalDimensionUnit::Inch);
    }

    #[test]
    fn reads_resolution_big_endian_centimeter() {
        let blob = build_tiff_with_resolution(true, 118, 1, 119, 1, 3);
        let d = parse_resolution(&blob).expect("density");
        assert_eq!(d.x_value, 118.0);
        assert_eq!(d.y_value, 119.0);
        assert_eq!(d.x_unit, PhysicalDimensionUnit::Centimeter);
    }

    #[test]
    fn rejects_unknown_resolution_unit() {
        let blob = build_tiff_with_resolution(false, 300, 1, 300, 1, 1);
        assert_eq!(parse_resolution(&blob), None);
    }

    #[test]
    fn rejects_zero_denominator() {
        let blob = build_tiff_with_resolution(false, 300, 0, 300, 1, 2);
        assert_eq!(parse_resolution(&blob), None);
    }
}
