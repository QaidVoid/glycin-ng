//! Minimal HEIF (ISO/IEC 23008-12) container reader for AVIF still
//! images.
//!
//! Only the boxes needed to locate the primary AV1 item, its alpha
//! auxiliary item and their properties are parsed. Grid items and
//! image sequence tracks are not supported.

use crate::{Error, Result};

const ALPHA_URNS: [&[u8]; 2] = [
    b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha",
    b"urn:mpeg:hevc:2015:auxid:1",
];

/// Colour information from a `colr` box.
#[derive(Clone, Debug, Default)]
pub(super) struct Colr {
    pub icc: Option<Vec<u8>>,
    pub nclx: Option<Nclx>,
}

/// CICP values from an `nclx` colour box.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct Nclx {
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
    pub full_range: bool,
}

/// Item properties relevant to decoding.
#[derive(Clone, Debug, Default)]
pub(super) struct Props {
    pub width: u32,
    pub height: u32,
    pub colr: Colr,
    /// Counter-clockwise rotation in 90 degree steps (`irot`).
    pub rotation: u8,
    /// Mirror axis (`imir`): 0 swaps top/bottom, 1 swaps left/right.
    pub mirror: Option<u8>,
    /// Clean aperture (`clap`).
    pub clap: Option<Clap>,
    /// AV1 operating point to decode (`a1op`).
    pub operating_point: u8,
}

/// Clean aperture from a `clap` box, as numerator and denominator
/// pairs. Offsets are measured from the image centre.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct Clap {
    pub width: (u32, u32),
    pub height: (u32, u32),
    pub horiz_off: (i32, u32),
    pub vert_off: (i32, u32),
}

impl Clap {
    /// Crop rectangle `(x, y, width, height)` inside a `width` by
    /// `height` image, or `None` if the aperture is not a whole-pixel
    /// rectangle within it.
    pub fn rect(&self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let (x, w) = aperture_axis(self.width, self.horiz_off, width)?;
        let (y, h) = aperture_axis(self.height, self.vert_off, height)?;
        Some((x, y, w, h))
    }
}

/// Origin and size of the aperture along one axis of length `full`.
fn aperture_axis((n, d): (u32, u32), (off_n, off_d): (i32, u32), full: u32) -> Option<(u32, u32)> {
    if d == 0 || off_d == 0 || n % d != 0 {
        return None;
    }
    let size = n / d;
    if size == 0 || size > full {
        return None;
    }
    let num = 2 * off_n as i128 + (full - size) as i128 * off_d as i128;
    let den = 2 * off_d as i128;
    if num % den != 0 {
        return None;
    }
    let origin = u32::try_from(num / den).ok()?;
    (origin.checked_add(size)? <= full).then_some((origin, size))
}

/// AV1 item resolved from the container.
pub(super) struct Item<'a> {
    pub data: Vec<&'a [u8]>,
    pub props: Props,
}

/// Primary image and optional alpha plane of an AVIF file.
pub(super) struct Avif<'a> {
    pub color: Item<'a>,
    pub alpha: Option<Item<'a>>,
    pub premultiplied: bool,
}

#[derive(Clone, Copy, Debug)]
struct Extent {
    offset: u64,
    length: u64,
}

#[derive(Debug, Default)]
struct Location {
    construction_method: u8,
    base_offset: u64,
    extents: Vec<Extent>,
}

#[derive(Debug)]
enum Property {
    Ispe(u32, u32),
    Colr(Colr),
    Irot(u8),
    Imir(u8),
    AuxC(bool),
    Clap(Clap),
    A1op(u8),
    Lsel(u16),
    Other,
}

#[derive(Default)]
struct Meta<'a> {
    primary: Option<u32>,
    item_types: Vec<(u32, [u8; 4])>,
    locations: Vec<(u32, Location)>,
    references: Vec<([u8; 4], u32, Vec<u32>)>,
    properties: Vec<Property>,
    associations: Vec<(u32, Vec<u16>)>,
    idat: &'a [u8],
}

/// Parse the container and resolve the primary item and its alpha.
pub(super) fn parse(file: &[u8]) -> Result<Avif<'_>> {
    if !crate::sniff::is_avif(file) {
        return Err(malformed("missing avif brand in ftyp"));
    }
    let mut meta = None;
    let mut rest = file;
    while let Some((ty, body, tail)) = next_box(rest)? {
        if ty == *b"meta" {
            meta = Some(parse_meta(full_box(body)?.1)?);
            break;
        }
        rest = tail;
    }
    let meta = meta.ok_or_else(|| malformed("missing meta box"))?;

    let primary = meta.primary.ok_or_else(|| malformed("missing pitm box"))?;
    match meta.item_type(primary) {
        Some(t) if t == *b"av01" => {}
        Some(t) if t == *b"grid" => {
            return Err(Error::Decoder {
                format: "avif",
                message: "grid images are not supported".into(),
            });
        }
        _ => return Err(malformed("primary item is not av01")),
    }
    let color = meta.item(file, primary)?;

    let alpha_id = meta
        .references
        .iter()
        .filter(|(ty, _, to)| ty == b"auxl" && to.contains(&primary))
        .map(|(_, from, _)| *from)
        .find(|id| {
            meta.item_type(*id) == Some(*b"av01")
                && meta.props(*id).any(|p| matches!(p, Property::AuxC(true)))
        });
    let alpha = alpha_id.map(|id| meta.item(file, id)).transpose()?;
    let premultiplied = alpha_id.is_some_and(|a| {
        meta.references
            .iter()
            .any(|(ty, from, to)| ty == b"prem" && *from == primary && to.contains(&a))
    });

    Ok(Avif {
        color,
        alpha,
        premultiplied,
    })
}

impl<'a> Meta<'a> {
    fn item_type(&self, id: u32) -> Option<[u8; 4]> {
        let at = self.item_types.partition_point(|(i, _)| *i < id);
        self.item_types
            .get(at)
            .filter(|(i, _)| *i == id)
            .map(|(_, t)| *t)
    }

    fn props(&self, id: u32) -> impl Iterator<Item = &Property> {
        let at = self.associations.partition_point(|(i, _)| *i < id);
        self.associations[at..]
            .iter()
            .take_while(move |(i, _)| *i == id)
            .flat_map(|(_, idx)| idx.iter())
            .filter_map(|&i| self.properties.get((i as usize).checked_sub(1)?))
    }

    fn item(&self, file: &'a [u8], id: u32) -> Result<Item<'a>> {
        let mut props = Props::default();
        for p in self.props(id) {
            match p {
                Property::Ispe(w, h) => (props.width, props.height) = (*w, *h),
                Property::Colr(c) => {
                    props.colr.icc = props.colr.icc.take().or_else(|| c.icc.clone());
                    props.colr.nclx = props.colr.nclx.or(c.nclx);
                }
                Property::Irot(r) => props.rotation = *r,
                Property::Imir(m) => props.mirror = Some(*m),
                Property::Clap(c) => props.clap = Some(*c),
                Property::A1op(op) => props.operating_point = *op,
                Property::Lsel(layer) if *layer != 0xffff => {
                    return Err(Error::Decoder {
                        format: "avif",
                        message: "layer selection is not supported".into(),
                    });
                }
                Property::Lsel(_) => {}
                Property::AuxC(_) | Property::Other => {}
            }
        }
        if props.width == 0 || props.height == 0 {
            return Err(malformed("missing ispe property"));
        }

        let loc = self
            .locations
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, l)| l)
            .ok_or_else(|| malformed("item has no location"))?;
        let source = match loc.construction_method {
            0 => file,
            1 => self.idat,
            _ => return Err(malformed("unsupported iloc construction method")),
        };
        let data = loc
            .extents
            .iter()
            .map(|e| {
                let start = loc
                    .base_offset
                    .checked_add(e.offset)
                    .ok_or_else(|| malformed("extent offset overflow"))?;
                let start = usize::try_from(start).map_err(|_| malformed("extent out of range"))?;
                let end = if e.length == 0 {
                    source.len()
                } else {
                    usize::try_from(e.length)
                        .ok()
                        .and_then(|l| start.checked_add(l))
                        .ok_or_else(|| malformed("extent out of range"))?
                };
                source
                    .get(start..end)
                    .ok_or_else(|| malformed("extent out of range"))
            })
            .collect::<Result<Vec<_>>>()?;
        let total = data
            .iter()
            .try_fold(0usize, |sum, d| sum.checked_add(d.len()));
        if total.is_none_or(|t| t > source.len()) {
            return Err(malformed("item extents exceed their source"));
        }
        Ok(Item { data, props })
    }
}

fn parse_meta(mut buf: &[u8]) -> Result<Meta<'_>> {
    let mut meta = Meta::default();
    while let Some((ty, body, rest)) = next_box(buf)? {
        match &ty {
            b"pitm" => {
                let (version, mut r) = full_box(body)?;
                meta.primary = Some(read_id(&mut r, version > 0)?);
            }
            b"iinf" => parse_iinf(body, &mut meta)?,
            b"iloc" => parse_iloc(body, &mut meta)?,
            b"iref" => parse_iref(body, &mut meta)?,
            b"iprp" => parse_iprp(body, &mut meta)?,
            b"idat" => meta.idat = body,
            _ => {}
        }
        buf = rest;
    }
    meta.item_types.sort_by_key(|(id, _)| *id);
    meta.associations.sort_by_key(|(id, _)| *id);
    Ok(meta)
}

fn parse_iinf(body: &[u8], meta: &mut Meta) -> Result<()> {
    let (version, mut r) = full_box(body)?;
    let _count = if version == 0 {
        read_u16(&mut r)? as u32
    } else {
        read_u32(&mut r)?
    };
    while let Some((ty, infe, rest)) = next_box(r)? {
        if ty == *b"infe" {
            let (version, mut b) = full_box(infe)?;
            if version >= 2 {
                let id = read_id(&mut b, version > 2)?;
                let _protection = read_u16(&mut b)?;
                let item_type = take(&mut b, 4)?.try_into().unwrap_or_default();
                meta.item_types.push((id, item_type));
            }
        }
        r = rest;
    }
    Ok(())
}

fn parse_iloc(body: &[u8], meta: &mut Meta) -> Result<()> {
    let (version, mut r) = full_box(body)?;
    if version > 2 {
        return Err(malformed("unsupported iloc version"));
    }
    let sizes = read_u16(&mut r)?;
    let offset_size = (sizes >> 12) as u8;
    let length_size = ((sizes >> 8) & 0xf) as u8;
    let base_offset_size = ((sizes >> 4) & 0xf) as u8;
    let index_size = if version > 0 { (sizes & 0xf) as u8 } else { 0 };
    let count = if version < 2 {
        read_u16(&mut r)? as u32
    } else {
        read_u32(&mut r)?
    };
    for _ in 0..count {
        let id = read_id(&mut r, version == 2)?;
        let construction_method = if version > 0 {
            (read_u16(&mut r)? & 0xf) as u8
        } else {
            0
        };
        let _data_reference_index = read_u16(&mut r)?;
        let base_offset = read_sized(&mut r, base_offset_size)?;
        let extent_count = read_u16(&mut r)?;
        if index_size + offset_size + length_size == 0 && extent_count > 1 {
            return Err(malformed("iloc extents have no fields"));
        }
        let mut extents = Vec::with_capacity(extent_count.min(64) as usize);
        for _ in 0..extent_count {
            read_sized(&mut r, index_size)?;
            let offset = read_sized(&mut r, offset_size)?;
            let length = read_sized(&mut r, length_size)?;
            extents.push(Extent { offset, length });
        }
        meta.locations.push((
            id,
            Location {
                construction_method,
                base_offset,
                extents,
            },
        ));
    }
    Ok(())
}

fn parse_iref(body: &[u8], meta: &mut Meta) -> Result<()> {
    let (version, mut r) = full_box(body)?;
    while let Some((ty, mut b, rest)) = next_box(r)? {
        let from = read_id(&mut b, version > 0)?;
        let count = read_u16(&mut b)?;
        let to = (0..count)
            .map(|_| read_id(&mut b, version > 0))
            .collect::<Result<Vec<_>>>()?;
        meta.references.push((ty, from, to));
        r = rest;
    }
    Ok(())
}

fn parse_iprp(mut body: &[u8], meta: &mut Meta) -> Result<()> {
    while let Some((ty, b, rest)) = next_box(body)? {
        match &ty {
            b"ipco" => parse_ipco(b, meta)?,
            b"ipma" => parse_ipma(b, meta)?,
            _ => {}
        }
        body = rest;
    }
    Ok(())
}

fn parse_ipco(mut body: &[u8], meta: &mut Meta) -> Result<()> {
    while let Some((ty, b, rest)) = next_box(body)? {
        let prop = match &ty {
            b"ispe" => {
                let (_, mut r) = full_box(b)?;
                Property::Ispe(read_u32(&mut r)?, read_u32(&mut r)?)
            }
            b"colr" => Property::Colr(parse_colr(b)?),
            b"irot" => Property::Irot(b.first().copied().unwrap_or(0) & 3),
            b"imir" => Property::Imir(b.first().copied().unwrap_or(0) & 1),
            b"a1op" => Property::A1op(take(&mut { b }, 1)?[0]),
            b"lsel" => Property::Lsel(read_u16(&mut { b })?),
            b"clap" => {
                let mut r = b;
                let mut next = || read_u32(&mut r);
                Property::Clap(Clap {
                    width: (next()?, next()?),
                    height: (next()?, next()?),
                    horiz_off: (next()? as i32, next()?),
                    vert_off: (next()? as i32, next()?),
                })
            }
            b"auxC" => {
                let (_, r) = full_box(b)?;
                let urn = r.split(|&c| c == 0).next().unwrap_or(r);
                Property::AuxC(ALPHA_URNS.contains(&urn))
            }
            _ => Property::Other,
        };
        meta.properties.push(prop);
        body = rest;
    }
    Ok(())
}

fn parse_colr(mut b: &[u8]) -> Result<Colr> {
    let kind = take(&mut b, 4)?;
    let mut colr = Colr::default();
    match kind {
        b"nclx" => {
            let primaries = read_u16(&mut b)?;
            let transfer = read_u16(&mut b)?;
            let matrix = read_u16(&mut b)?;
            let full_range = take(&mut b, 1)?[0] & 0x80 != 0;
            colr.nclx = Some(Nclx {
                primaries: primaries.min(255) as u8,
                transfer: transfer.min(255) as u8,
                matrix: matrix.min(255) as u8,
                full_range,
            });
        }
        b"prof" | b"rICC" => colr.icc = Some(b.to_vec()),
        _ => {}
    }
    Ok(colr)
}

fn parse_ipma(body: &[u8], meta: &mut Meta) -> Result<()> {
    let (version, mut r) = full_box(body)?;
    let flags = u32::from_be_bytes(body[0..4].try_into().unwrap_or_default()) & 0xff_ffff;
    let count = read_u32(&mut r)?;
    for _ in 0..count {
        let id = read_id(&mut r, version > 0)?;
        let n = take(&mut r, 1)?[0];
        let mut indices = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let index = if flags & 1 != 0 {
                read_u16(&mut r)? & 0x7fff
            } else {
                (take(&mut r, 1)?[0] & 0x7f) as u16
            };
            indices.push(index);
        }
        meta.associations.push((id, indices));
    }
    Ok(())
}

/// Box type, body and the bytes following the box.
type Split<'a> = ([u8; 4], &'a [u8], &'a [u8]);

/// Split the next box off `buf`. Returns `Ok(None)` at the end of the
/// buffer.
fn next_box(buf: &[u8]) -> Result<Option<Split<'_>>> {
    if buf.is_empty() {
        return Ok(None);
    }
    let mut r = buf;
    let size = read_u32(&mut r)? as u64;
    let ty: [u8; 4] = take(&mut r, 4)?.try_into().unwrap_or_default();
    let (header, size) = match size {
        0 => (8, buf.len() as u64),
        1 => (16, read_u64(&mut r)?),
        s => (8, s),
    };
    let size = usize::try_from(size).map_err(|_| malformed("box too large"))?;
    if size < header || size > buf.len() {
        return Err(malformed("box size out of range"));
    }
    let mut body = &buf[header..size];
    if ty == *b"uuid" {
        take(&mut body, 16)?;
    }
    Ok(Some((ty, body, &buf[size..])))
}

fn full_box(b: &[u8]) -> Result<(u8, &[u8])> {
    let mut r = b;
    let version = take(&mut r, 4)?[0];
    Ok((version, r))
}

fn take<'a>(r: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if r.len() < n {
        return Err(malformed("unexpected end of box"));
    }
    let (head, tail) = r.split_at(n);
    *r = tail;
    Ok(head)
}

fn read_u16(r: &mut &[u8]) -> Result<u16> {
    Ok(u16::from_be_bytes(
        take(r, 2)?.try_into().unwrap_or_default(),
    ))
}

fn read_u32(r: &mut &[u8]) -> Result<u32> {
    Ok(u32::from_be_bytes(
        take(r, 4)?.try_into().unwrap_or_default(),
    ))
}

fn read_u64(r: &mut &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(
        take(r, 8)?.try_into().unwrap_or_default(),
    ))
}

fn read_id(r: &mut &[u8], wide: bool) -> Result<u32> {
    if wide {
        read_u32(r)
    } else {
        Ok(read_u16(r)? as u32)
    }
}

fn read_sized(r: &mut &[u8], size: u8) -> Result<u64> {
    match size {
        0 => Ok(0),
        4 => Ok(read_u32(r)? as u64),
        8 => read_u64(r),
        _ => Err(malformed("unsupported iloc field size")),
    }
}

fn malformed(msg: &'static str) -> Error {
    Error::Malformed(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_box() {
        assert!(next_box(b"\0\0\0\x20ftyp").is_err());
    }

    #[test]
    fn clap_resolves_centred_and_offset_apertures() {
        let clap = |w, h, x: i32, y: i32| Clap {
            width: (w, 1),
            height: (h, 1),
            horiz_off: (x, 1),
            vert_off: (y, 1),
        };
        assert_eq!(
            clap(600, 440, 0, 0).rect(640, 480),
            Some((20, 20, 600, 440))
        );
        assert_eq!(
            clap(600, 440, -20, 20).rect(640, 480),
            Some((0, 40, 600, 440))
        );
        assert_eq!(clap(600, 440, -21, 0).rect(640, 480), None);
        assert_eq!(clap(641, 480, 0, 0).rect(640, 480), None);
        assert_eq!(clap(5, 5, 0, 0).rect(6, 6), None);
    }
}
