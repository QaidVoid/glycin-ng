//! Pixel density (resolution) attached to a decoded image.
//!
//! Density is pixels per physical unit, for example 300 DPI (pixels
//! per inch). Decoders fill it in when the container carries
//! resolution metadata (PNG `pHYs`, JPEG JFIF, TIFF resolution tags,
//! or EXIF); images without such metadata report `None`.

/// Physical unit of a [`PixelDensity`] axis.
///
/// Discriminants match upstream glycin's `GlyPhysicalDimensionUnit`
/// (and the shim's `GType` table) so values cross the C ABI
/// unchanged: Inch = 1, Pica = 2, Point = 3, Meter = 4,
/// Centimeter = 5, Millimeter = 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum PhysicalDimensionUnit {
    /// Pixels per inch (DPI).
    Inch = 1,
    /// Pixels per pica (1/6 inch).
    Pica = 2,
    /// Pixels per point (1/72 inch).
    Point = 3,
    /// Pixels per meter.
    Meter = 4,
    /// Pixels per centimeter.
    Centimeter = 5,
    /// Pixels per millimeter.
    Millimeter = 6,
}

/// Pixels per physical unit on each axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelDensity {
    /// Horizontal pixels per `x_unit`.
    pub x_value: f64,
    /// Unit of `x_value`.
    pub x_unit: PhysicalDimensionUnit,
    /// Vertical pixels per `y_unit`.
    pub y_value: f64,
    /// Unit of `y_value`.
    pub y_unit: PhysicalDimensionUnit,
}

impl PixelDensity {
    /// Build a density from axis values and units.
    pub fn new(
        x_value: f64,
        x_unit: PhysicalDimensionUnit,
        y_value: f64,
        y_unit: PhysicalDimensionUnit,
    ) -> Self {
        Self {
            x_value,
            x_unit,
            y_value,
            y_unit,
        }
    }

    /// Convert both axes to `unit`, preserving the physical size.
    pub fn convert(self, unit: PhysicalDimensionUnit) -> Self {
        Self {
            x_value: convert_value(self.x_value, self.x_unit, unit),
            x_unit: unit,
            y_value: convert_value(self.y_value, self.y_unit, unit),
            y_unit: unit,
        }
    }
}

impl PhysicalDimensionUnit {
    /// Centimeters per unit, the common basis for conversion.
    pub(crate) fn cm_factor(self) -> f64 {
        match self {
            Self::Inch => 2.54,
            Self::Pica => 2.54 / 6.0,
            Self::Point => 2.54 / 72.0,
            Self::Meter => 100.0,
            Self::Centimeter => 1.0,
            Self::Millimeter => 0.1,
        }
    }

    /// Map a C ABI discriminant (1..=6) to a unit.
    pub(crate) fn from_discriminant(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::Inch),
            2 => Some(Self::Pica),
            3 => Some(Self::Point),
            4 => Some(Self::Meter),
            5 => Some(Self::Centimeter),
            6 => Some(Self::Millimeter),
            _ => None,
        }
    }
}

/// Convert a pixels-per-`from` value to pixels-per-`to`.
fn convert_value(value: f64, from: PhysicalDimensionUnit, to: PhysicalDimensionUnit) -> f64 {
    value * to.cm_factor() / from.cm_factor()
}
