//! `GlyPixelDensity` and `GlyFrameDetails` (glycin 2.2).
//!
//! `GlyPixelDensity` holds pixels per physical unit for each axis.
//! Conversion uses the same centimeter factors as upstream's
//! `gufo_common::physical_dimension`, so a density read back from
//! the shim matches what upstream would report for the same input.
//!
//! The engine does not parse physical density metadata (PNG `pHYs`,
//! JPEG JFIF density, TIFF resolution tags) yet, so every
//! `GlyFrameDetails` reports no density and
//! `gly_frame_details_get_pixel_density` returns `NULL`. Upstream
//! defines that property as nullable, and gdk-pixbuf skips the
//! `x-dpi` / `y-dpi` options when it is `NULL`. Encoders likewise do
//! not embed a density, so `gly_new_frame_set_pixel_density` accepts
//! and drops it.

use std::ffi::c_int;
use std::ptr;

use crate::ffi::GObject;
use crate::types::FrameState;
use crate::{attach_state, state_ref};

pub(crate) const GLY_PHYSICAL_DIMENSION_UNIT_INCH: c_int = 1;

/// `GlyPhysicalDimensionUnit`. Values match `glycin.h`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum Unit {
    Inch = 1,
    /// 1/6 inch
    Pica = 2,
    /// 1/72 inch
    Point = 3,
    Meter = 4,
    Centimeter = 5,
    Millimeter = 6,
}

impl Unit {
    /// Map a raw `GlyPhysicalDimensionUnit`, rejecting values outside
    /// the enum instead of aborting like upstream's `unwrap`.
    pub(crate) fn from_raw(raw: c_int) -> Option<Self> {
        Some(match raw {
            1 => Self::Inch,
            2 => Self::Pica,
            3 => Self::Point,
            4 => Self::Meter,
            5 => Self::Centimeter,
            6 => Self::Millimeter,
            _ => return None,
        })
    }

    /// Length of one unit in centimeters.
    fn centimeters(self) -> f64 {
        match self {
            Self::Inch => 2.54,
            Self::Pica => 2.54 / 6.,
            Self::Point => 2.54 / 72.,
            Self::Meter => 100.,
            Self::Centimeter => 1.,
            Self::Millimeter => 1. / 10.,
        }
    }
}

/// Pixels per one `unit` along a single axis.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct Axis {
    pub(crate) value: f64,
    pub(crate) unit: Unit,
}

impl Axis {
    /// Express the same density per a different unit. A larger unit
    /// holds more pixels, so the value scales with the unit length.
    pub(crate) fn convert(self, unit: Unit) -> Self {
        Self {
            value: self.value * unit.centimeters() / self.unit.centimeters(),
            unit,
        }
    }
}

/// State backing a `GlyPixelDensity`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct PixelDensity {
    pub(crate) x: Axis,
    pub(crate) y: Axis,
}

impl PixelDensity {
    pub(crate) fn convert(self, unit: Unit) -> Self {
        Self {
            x: self.x.convert(unit),
            y: self.y.convert(unit),
        }
    }
}

/// State backing a `GlyFrameDetails`.
pub(crate) struct FrameDetailsState {
    pub(crate) pixel_density: Option<PixelDensity>,
}

fn new_pixel_density(density: PixelDensity) -> *mut GObject {
    unsafe { attach_state(density) }
}

// ----- gly_pixel_density_* -----

/// Create a pixel density. Returns `NULL` when either unit is not a
/// `GlyPhysicalDimensionUnit` value.
///
/// # Safety
/// Always safe. The caller owns the returned reference.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_pixel_density_new(
    x_value: f64,
    x_unit: c_int,
    y_value: f64,
    y_unit: c_int,
) -> *mut GObject {
    let (Some(x_unit), Some(y_unit)) = (Unit::from_raw(x_unit), Unit::from_raw(y_unit)) else {
        return ptr::null_mut();
    };
    new_pixel_density(PixelDensity {
        x: Axis {
            value: x_value,
            unit: x_unit,
        },
        y: Axis {
            value: y_value,
            unit: y_unit,
        },
    })
}

/// # Safety
/// `pixel_density` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_pixel_density_get_x_value(pixel_density: *mut GObject) -> f64 {
    unsafe { state_ref::<PixelDensity>(pixel_density) }
        .map(|d| d.x.value)
        .unwrap_or(0.)
}

/// # Safety
/// `pixel_density` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_pixel_density_get_x_unit(pixel_density: *mut GObject) -> c_int {
    unsafe { state_ref::<PixelDensity>(pixel_density) }
        .map(|d| d.x.unit as c_int)
        .unwrap_or(GLY_PHYSICAL_DIMENSION_UNIT_INCH)
}

/// # Safety
/// `pixel_density` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_pixel_density_get_y_value(pixel_density: *mut GObject) -> f64 {
    unsafe { state_ref::<PixelDensity>(pixel_density) }
        .map(|d| d.y.value)
        .unwrap_or(0.)
}

/// # Safety
/// `pixel_density` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_pixel_density_get_y_unit(pixel_density: *mut GObject) -> c_int {
    unsafe { state_ref::<PixelDensity>(pixel_density) }
        .map(|d| d.y.unit as c_int)
        .unwrap_or(GLY_PHYSICAL_DIMENSION_UNIT_INCH)
}

/// Return a new density expressed per `unit` on both axes. Returns
/// `NULL` for a NULL density or an unknown unit.
///
/// # Safety
/// `pixel_density` must be valid or NULL. The caller owns the
/// returned reference.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_pixel_density_convert(
    pixel_density: *mut GObject,
    unit: c_int,
) -> *mut GObject {
    let Some(unit) = Unit::from_raw(unit) else {
        return ptr::null_mut();
    };
    match unsafe { state_ref::<PixelDensity>(pixel_density) } {
        Some(density) => new_pixel_density(density.convert(unit)),
        None => ptr::null_mut(),
    }
}

// ----- gly_frame_details_* / gly_frame_get_details -----

/// # Safety
/// `frame` must be valid or NULL. The caller owns the returned
/// reference.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_details(frame: *mut GObject) -> *mut GObject {
    if unsafe { state_ref::<FrameState>(frame) }.is_none() {
        return ptr::null_mut();
    }
    unsafe {
        attach_state(FrameDetailsState {
            pixel_density: None,
        })
    }
}

/// Return the frame's pixel density, or `NULL` when the image
/// carries none.
///
/// # Safety
/// `frame_details` must be valid or NULL. The caller owns the
/// returned reference.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_details_get_pixel_density(
    frame_details: *mut GObject,
) -> *mut GObject {
    match unsafe { state_ref::<FrameDetailsState>(frame_details) }.and_then(|s| s.pixel_density) {
        Some(density) => new_pixel_density(density),
        None => ptr::null_mut(),
    }
}

// ----- gly_new_frame_set_pixel_density -----

/// Accept a density for the encoded frame. The engine's encoders do
/// not write density metadata, so the value is dropped. The caller
/// keeps its reference to `pixel_density`.
///
/// # Safety
/// Both arguments may be anything; nothing is read or written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_new_frame_set_pixel_density(
    _new_frame: *mut GObject,
    _pixel_density: *mut GObject,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_UNITS: [Unit; 6] = [
        Unit::Inch,
        Unit::Pica,
        Unit::Point,
        Unit::Meter,
        Unit::Centimeter,
        Unit::Millimeter,
    ];

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.)
    }

    #[test]
    fn unit_from_raw_accepts_every_header_value() {
        for unit in ALL_UNITS {
            assert_eq!(Unit::from_raw(unit as c_int), Some(unit));
        }
    }

    #[test]
    fn unit_from_raw_rejects_values_outside_the_enum() {
        for raw in [0, 7, -1, c_int::MAX, c_int::MIN] {
            assert_eq!(Unit::from_raw(raw), None);
        }
    }

    #[test]
    fn dpi_to_dots_per_centimeter() {
        let axis = Axis {
            value: 254.,
            unit: Unit::Inch,
        };
        let converted = axis.convert(Unit::Centimeter);
        assert_eq!(converted.unit, Unit::Centimeter);
        assert!(close(converted.value, 100.));
    }

    #[test]
    fn png_phys_meter_density_to_dpi() {
        // PNG stores density as pixels per meter; 3780 px/m is the
        // usual encoding of 96 DPI.
        let axis = Axis {
            value: 3780.,
            unit: Unit::Meter,
        };
        assert!(close(axis.convert(Unit::Inch).value, 96.012));
    }

    #[test]
    fn pica_and_point_scale_from_inch() {
        let axis = Axis {
            value: 72.,
            unit: Unit::Inch,
        };
        assert!(close(axis.convert(Unit::Pica).value, 12.));
        assert!(close(axis.convert(Unit::Point).value, 1.));
        assert!(close(axis.convert(Unit::Millimeter).value, 72. / 25.4));
    }

    #[test]
    fn conversion_round_trips_through_every_unit() {
        for from in ALL_UNITS {
            for to in ALL_UNITS {
                let axis = Axis {
                    value: 300.,
                    unit: from,
                };
                let back = axis.convert(to).convert(from);
                assert_eq!(back.unit, from);
                assert!(close(back.value, 300.), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn density_convert_applies_to_both_axes() {
        let density = PixelDensity {
            x: Axis {
                value: 300.,
                unit: Unit::Inch,
            },
            y: Axis {
                value: 100.,
                unit: Unit::Centimeter,
            },
        };
        let dpi = density.convert(Unit::Inch);
        assert_eq!(dpi.x.unit, Unit::Inch);
        assert_eq!(dpi.y.unit, Unit::Inch);
        assert!(close(dpi.x.value, 300.));
        assert!(close(dpi.y.value, 254.));
    }

    #[test]
    fn zero_and_non_finite_values_pass_through_unchanged() {
        let zero = Axis {
            value: 0.,
            unit: Unit::Inch,
        };
        assert_eq!(zero.convert(Unit::Meter).value, 0.);
        let nan = Axis {
            value: f64::NAN,
            unit: Unit::Inch,
        };
        assert!(nan.convert(Unit::Meter).value.is_nan());
    }

    #[test]
    fn new_rejects_unknown_units() {
        unsafe {
            assert!(gly_pixel_density_new(96., 0, 96., Unit::Inch as c_int).is_null());
            assert!(gly_pixel_density_new(96., Unit::Inch as c_int, 96., 7).is_null());
        }
    }

    #[test]
    fn accessors_return_upstream_defaults_for_null() {
        unsafe {
            assert_eq!(gly_pixel_density_get_x_value(ptr::null_mut()), 0.);
            assert_eq!(gly_pixel_density_get_y_value(ptr::null_mut()), 0.);
            assert_eq!(
                gly_pixel_density_get_x_unit(ptr::null_mut()),
                GLY_PHYSICAL_DIMENSION_UNIT_INCH
            );
            assert_eq!(
                gly_pixel_density_get_y_unit(ptr::null_mut()),
                GLY_PHYSICAL_DIMENSION_UNIT_INCH
            );
        }
    }

    #[test]
    fn null_handles_yield_null_objects() {
        unsafe {
            assert!(gly_pixel_density_convert(ptr::null_mut(), Unit::Inch as c_int).is_null());
            assert!(gly_pixel_density_convert(ptr::null_mut(), 0).is_null());
            assert!(gly_frame_get_details(ptr::null_mut()).is_null());
            assert!(gly_frame_details_get_pixel_density(ptr::null_mut()).is_null());
            gly_new_frame_set_pixel_density(ptr::null_mut(), ptr::null_mut());
        }
    }
}
