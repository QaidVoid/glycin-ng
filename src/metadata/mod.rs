//! Metadata helpers: EXIF, ICC, and orientation transforms.
//!
//! Decoders extract the EXIF and ICC blobs and store them on the
//! returned [`Image`](crate::Image). Functions in this module parse
//! those blobs and, when the caller has asked for it, apply the
//! orientation transform to the decoded pixels.

pub(crate) mod exif;
pub(crate) mod transform;

use crate::{Image, Orientation};

/// If the image carries an EXIF blob with an orientation tag, parse
/// it and (optionally) bake the transform into the pixel buffer.
///
/// When `apply` is true the function rewrites every frame so that
/// the visual orientation is normal, then sets
/// [`Image::orientation`] to [`Orientation::Normal`]. When `apply`
/// is false the orientation is reported on the image but pixels are
/// left untouched.
pub(crate) fn apply_orientation_if_present(image: &mut Image, apply: bool) {
    let Some(blob) = image.exif() else { return };
    let Some(raw) = exif::parse_orientation(blob) else {
        return;
    };
    let orientation = Orientation::from_exif(raw);
    if orientation == Orientation::Normal {
        return;
    }
    if !apply {
        image.set_orientation(orientation);
        return;
    }
    transform::bake_into_frames(image, orientation);
    image.set_orientation(Orientation::Normal);
}
