//! YUV to RGB conversion for decoded AV1 pictures.
//!
//! Chroma is upsampled by nearest neighbour. Output is 8-bit RGB(A)
//! for 8-bit sources and 16-bit RGB(A) above that.

use super::av1::{Layout, Picture};

/// How to interpret the luma and chroma samples.
#[derive(Copy, Clone, Debug)]
pub(super) struct Matrix {
    pub coefficients: u8,
    pub full_range: bool,
}

#[derive(Copy, Clone)]
enum Kind {
    Mono,
    Identity,
    YCgCo,
    Ycbcr {
        cr_r: f32,
        cb_g: f32,
        cr_g: f32,
        cb_b: f32,
    },
}

impl Kind {
    fn new(coefficients: u8, layout: Layout) -> Self {
        if layout == Layout::I400 {
            return Self::Mono;
        }
        let (kr, kb) = match coefficients {
            0 if layout == Layout::I444 => return Self::Identity,
            8 => return Self::YCgCo,
            1 => (0.2126, 0.0722),
            4 => (0.30, 0.11),
            7 => (0.212, 0.087),
            9 | 10 => (0.2627, 0.0593),
            _ => (0.299, 0.114),
        };
        let kg = 1.0 - kr - kb;
        let cr_r = 2.0 * (1.0 - kr);
        let cb_b = 2.0 * (1.0 - kb);
        Self::Ycbcr {
            cr_r,
            cb_g: kb * cb_b / kg,
            cr_g: kr * cr_r / kg,
            cb_b,
        }
    }
}

/// Maps raw samples to output units: `(raw - offset) * scale`.
#[derive(Copy, Clone)]
struct Range {
    offset: f32,
    scale: f32,
}

impl Range {
    fn new(depth: u8, full: bool, chroma: bool, out_max: f32) -> Self {
        let shift = depth as u32 - 8;
        let max = ((1u32 << depth) - 1) as f32;
        let mid = (1u32 << (depth - 1)) as f32;
        let (offset, span) = match (full, chroma) {
            (true, false) => (0.0, max),
            (true, true) => (mid, max),
            (false, false) => ((16u32 << shift) as f32, (219u32 << shift) as f32),
            (false, true) => (mid, (224u32 << shift) as f32),
        };
        Self {
            offset,
            scale: out_max / span,
        }
    }

    #[inline(always)]
    fn map(self, raw: f32) -> f32 {
        (raw - self.offset) * self.scale
    }
}

trait Sample {
    const BYTES: usize;
    const MAX: f32;
    fn read(row: &[u8], x: usize) -> f32;
    fn write(out: &mut [u8], v: f32);
}

impl Sample for u8 {
    const BYTES: usize = 1;
    const MAX: f32 = 255.0;

    #[inline(always)]
    fn read(row: &[u8], x: usize) -> f32 {
        row[x] as f32
    }

    #[inline(always)]
    fn write(out: &mut [u8], v: f32) {
        out[0] = (v + 0.5) as u8;
    }
}

impl Sample for u16 {
    const BYTES: usize = 2;
    const MAX: f32 = 65535.0;

    #[inline(always)]
    fn read(row: &[u8], x: usize) -> f32 {
        u16::from_ne_bytes([row[2 * x], row[2 * x + 1]]) as f32
    }

    #[inline(always)]
    fn write(out: &mut [u8], v: f32) {
        out[..2].copy_from_slice(&((v + 0.5) as u16).to_ne_bytes());
    }
}

/// Convert `color` (and optional `alpha` luma) into packed RGB(A).
///
/// `alpha` must match `color` in size and bit depth. Returns the pixel
/// bytes and whether samples are 16 bits wide.
pub(super) fn to_rgb(color: &Picture, alpha: Option<&Picture>, matrix: Matrix) -> (Vec<u8>, bool) {
    if color.bit_depth() > 8 {
        (convert::<u16>(color, alpha, matrix), true)
    } else {
        (convert::<u8>(color, alpha, matrix), false)
    }
}

fn convert<S: Sample>(color: &Picture, alpha: Option<&Picture>, matrix: Matrix) -> Vec<u8> {
    let width = color.width() as usize;
    let depth = color.bit_depth();
    let layout = color.layout();
    let kind = Kind::new(matrix.coefficients, layout);
    let channels = if alpha.is_some() { 4 } else { 3 };
    let pixel_bytes = channels * S::BYTES;
    let row_bytes = width * S::BYTES;

    let luma = Range::new(depth, matrix.full_range, false, S::MAX);
    let chroma = Range::new(depth, matrix.full_range, true, S::MAX);
    let alpha_range = alpha.map(|a| Range::new(depth, a.color().full_range, false, S::MAX));
    let (sx, sy) = match layout {
        Layout::I420 => (1, 1),
        Layout::I422 => (1, 0),
        Layout::I400 | Layout::I444 => (0, 0),
    };

    let (y_plane, y_stride) = color.plane(0);
    let (u_plane, c_stride) = color.plane(1);
    let (v_plane, _) = color.plane(2);
    let (a_plane, a_stride) = alpha.map_or((&[][..], 0), |a| a.plane(0));
    let chroma_row_bytes = (width.div_ceil(1 << sx)) * S::BYTES;

    let mut out = vec![0u8; width * color.height() as usize * pixel_bytes];
    for (row, dst) in out.chunks_exact_mut(width * pixel_bytes).enumerate() {
        let yr = &y_plane[row * y_stride..][..row_bytes];
        let (ur, vr) = if matches!(kind, Kind::Mono) {
            (&[][..], &[][..])
        } else {
            let at = (row >> sy) * c_stride;
            (
                &u_plane[at..][..chroma_row_bytes],
                &v_plane[at..][..chroma_row_bytes],
            )
        };
        let ar = alpha_range.map(|_| &a_plane[row * a_stride..][..row_bytes]);

        for (x, px) in dst.chunks_exact_mut(pixel_bytes).enumerate() {
            let y = luma.map(S::read(yr, x));
            let [r, g, b] = match kind {
                Kind::Mono => [y; 3],
                Kind::Identity => [luma.map(S::read(vr, x)), y, luma.map(S::read(ur, x))],
                Kind::YCgCo => {
                    let cg = chroma.map(S::read(ur, x >> sx));
                    let co = chroma.map(S::read(vr, x >> sx));
                    let t = y - cg;
                    [t + co, y + cg, t - co]
                }
                Kind::Ycbcr {
                    cr_r,
                    cb_g,
                    cr_g,
                    cb_b,
                } => {
                    let cb = chroma.map(S::read(ur, x >> sx));
                    let cr = chroma.map(S::read(vr, x >> sx));
                    [y + cr_r * cr, y - cb_g * cb - cr_g * cr, y + cb_b * cb]
                }
            };
            S::write(&mut px[..], r);
            S::write(&mut px[S::BYTES..], g);
            S::write(&mut px[2 * S::BYTES..], b);
            if let (Some(range), Some(ar)) = (alpha_range, ar) {
                S::write(&mut px[3 * S::BYTES..], range.map(S::read(ar, x)));
            }
        }
    }
    out
}
