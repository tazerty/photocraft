//! Evora build (patch LUM-14 r4, the verification of LF-3's correction 1): what a placed PNG or
//! JPEG keeps of its metadata once cleaned, so that the engine shows it as before.
//!
//! The engine turns a photo upright from its EXIF Orientation, and takes its resolution from its
//! metadata blocks when the format's own field gives none (`photocraft_codecs::decode_as_with`).
//! The exporter measures a smart object's source from the bytes it embeds (its size and resolution
//! in the placed-layer block). A placed photo cleaned of every metadata block would therefore come
//! out of an export lying on its side, its width and height swapped, at another resolution.
//!
//! This module reads both facts as the engine's decoders read them, from the same blocks in the
//! same order, with the engine's own readers (`exif_orientation`, `exif_resolution`,
//! `xmp_resolution`, `photoshop_resolution`). It writes them back as the one EXIF directory a
//! cleaned file carries: IFD0 with Orientation, XResolution, YResolution and ResolutionUnit (inch),
//! and nothing else (no location, no description, no other directory).
//!
//! The format's own resolution field (PNG `pHYs`, JPEG JFIF density) stays in the cleaned file as
//! it was: where the decoder takes it before the metadata, the written resolution is not read.
//! A PNG whose last XMP text chunk is compressed (`zTXt`, compressed `iTXt`) cannot be read by this
//! build, which links no inflater of its own: its resolution is then the EXIF one, if any.

use photocraft_codecs::{exif_orientation, exif_resolution, photoshop_resolution, xmp_resolution};

/// The JPEG segments the decoder reads the metadata from (APP1 EXIF and XMP, APP13 resources).
const JPEG_EXIF: &[u8] = b"Exif\0\0";
const JPEG_XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const JPEG_RESOURCES: &[u8] = b"Photoshop 3.0\0";
/// The JPEG decoder joins APP13 segments while it holds less than this.
const MAX_JPEG_RESOURCES: usize = 1 << 24;
/// The most data one JPEG segment holds (its 16-bit length counts its own two bytes).
const MAX_JPEG_SEGMENT: usize = 0xFFFF - 2;
/// A PNG's signature, its EXIF chunk, its text chunks and the keyword of an XMP packet.
const PNG_SIGNATURE_LEN: usize = 8;
const PNG_EXIF: [u8; 4] = *b"eXIf";
const PNG_IEND: [u8; 4] = *b"IEND";
const PNG_XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";
/// The IFD0 tags written, their types, and the size of a classic TIFF header.
const TAG_ORIENTATION: u16 = 0x0112;
const TAG_X_RESOLUTION: u16 = 0x011A;
const TAG_Y_RESOLUTION: u16 = 0x011B;
const TAG_RESOLUTION_UNIT: u16 = 0x0128;
const TYPE_SHORT: u16 = 3;
const TYPE_RATIONAL: u16 = 5;
const UNIT_INCH: u16 = 2;
const TIFF_HEADER: usize = 8;
/// A resolution that is not whole is written over this denominator, as the engine writes it.
const RATIONAL_DENOMINATOR: u32 = 1000;
/// A resolution this close to a whole number (pixels per inch) is written as that number.
const SAME_PPI: f64 = 1e-3;

/// What the engine reads from a placed image's metadata to show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    /// The EXIF Orientation (1 to 8) that turns the pixels upright; 1: as stored.
    pub orientation: u16,
    /// The resolution (pixels per inch) the metadata blocks record, in the decoder's precedence;
    /// `None`: the format's own field decides, or the default.
    pub ppi: Option<(f32, f32)>,
}

impl Geometry {
    fn new(exif: Option<&[u8]>, ppi: Option<(f32, f32)>) -> Geometry {
        Geometry { orientation: exif.map_or(1, exif_orientation), ppi }
    }

    /// The TIFF body (big-endian) of the one EXIF directory that records this geometry; `None`
    /// when there is nothing to record (upright, no resolution of the metadata).
    pub(super) fn exif(&self) -> Option<Vec<u8>> {
        let turned = (2..=8).contains(&self.orientation);
        let resolution = self.ppi.and_then(|(x, y)| Some((rational(x)?, rational(y)?)));
        let count = usize::from(turned) + if resolution.is_some() { 3 } else { 0 };
        if count == 0 {
            return None;
        }
        let mut out = b"MM\0\x2A".to_vec();
        out.extend_from_slice(&u32::try_from(TIFF_HEADER).ok()?.to_be_bytes());
        out.extend_from_slice(&u16::try_from(count).ok()?.to_be_bytes());
        // The two rationals follow the directory and its (empty) next-directory offset.
        let values = u32::try_from(TIFF_HEADER + 2 + 12 * count + 4).ok()?;
        if turned {
            entry(&mut out, TAG_ORIENTATION, TYPE_SHORT, short(self.orientation));
        }
        let mut data = Vec::new();
        if let Some(((xn, xd), (yn, yd))) = resolution {
            entry(&mut out, TAG_X_RESOLUTION, TYPE_RATIONAL, values.to_be_bytes());
            entry(&mut out, TAG_Y_RESOLUTION, TYPE_RATIONAL, values.checked_add(8)?.to_be_bytes());
            entry(&mut out, TAG_RESOLUTION_UNIT, TYPE_SHORT, short(UNIT_INCH));
            for v in [xn, xd, yn, yd] {
                data.extend_from_slice(&v.to_be_bytes());
            }
        }
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&data);
        Some(out)
    }
}

/// One IFD entry of one value: tag, type, count 1, the value (or its offset), big-endian.
fn entry(out: &mut Vec<u8>, tag: u16, ty: u16, field: [u8; 4]) {
    out.extend_from_slice(&tag.to_be_bytes());
    out.extend_from_slice(&ty.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&field);
}

/// A SHORT, left-justified in its 4-byte field (big-endian).
fn short(v: u16) -> [u8; 4] {
    let [a, b] = v.to_be_bytes();
    [a, b, 0, 0]
}

/// `ppi` as a TIFF RATIONAL, as the engine writes it: a whole number over 1, else over 1000.
/// `None` when it does not give a positive value.
fn rational(ppi: f32) -> Option<(u32, u32)> {
    let p = f64::from(ppi);
    let (value, denominator) =
        if (p - p.round()).abs() < SAME_PPI { (p.round(), 1) } else { ((p * f64::from(RATIONAL_DENOMINATOR)).round(), RATIONAL_DENOMINATOR) };
    (1.0..=f64::from(u32::MAX)).contains(&value).then(|| (value as u32, denominator))
}

/// A JPEG's geometry, as its decoder reads the segments before the first scan: the first EXIF
/// segment, the first XMP segment that is UTF-8, the image resource segments joined; the resolution
/// of the resources' ResolutionInfo, else of the XMP, else of the EXIF (the JFIF density, which a
/// cleaned file keeps, comes last).
pub(super) fn jpeg(b: &[u8]) -> Geometry {
    let mut exif: Option<&[u8]> = None;
    let mut xmp: Option<&str> = None;
    let mut resources = Vec::new();
    let mut at = 2;
    while let (Some(&0xFF), Some(&marker)) = (b.get(at), b.get(at + 1)) {
        match marker {
            // A fill byte.
            0xFF => {
                at += 1;
                continue;
            }
            // Markers without a length.
            0x01 | 0xD0..=0xD8 => {
                at += 2;
                continue;
            }
            // The first scan, or the end of image.
            0xD9 | 0xDA => break,
            _ => {}
        }
        let Some(len) = b.get(at + 2..at + 4).map(|l| usize::from(u16::from_be_bytes([l[0], l[1]]))) else { break };
        let Some(segment) = b.get(at + 4..at + 2 + len).filter(|_| len >= 2) else { break };
        match marker {
            0xE1 if exif.is_none() && segment.starts_with(JPEG_EXIF) => exif = segment.get(JPEG_EXIF.len()..),
            0xE1 if xmp.is_none() && segment.starts_with(JPEG_XMP) => {
                xmp = segment.get(JPEG_XMP.len()..).and_then(|x| std::str::from_utf8(x).ok()).map(|x| x.trim_end_matches('\0'));
            }
            0xED if resources.len() < MAX_JPEG_RESOURCES && segment.starts_with(JPEG_RESOURCES) => {
                resources.extend_from_slice(segment.get(JPEG_RESOURCES.len()..).unwrap_or_default());
            }
            _ => {}
        }
        at += 2 + len;
    }
    let ppi = photoshop_resolution(&resources).or_else(|| xmp.and_then(xmp_resolution)).or_else(|| exif.and_then(exif_resolution));
    Geometry::new(exif, ppi)
}

/// The APP1 segment (marker included) of a cleaned JPEG's EXIF, `tiff` its TIFF body; `None` when
/// it does not fit in one segment.
pub(super) fn jpeg_exif_segment(tiff: &[u8]) -> Option<Vec<u8>> {
    let len = JPEG_EXIF.len() + tiff.len();
    if len > MAX_JPEG_SEGMENT {
        return None;
    }
    let mut out = vec![0xFF, 0xE1];
    out.extend_from_slice(&u16::try_from(len + 2).ok()?.to_be_bytes());
    out.extend_from_slice(JPEG_EXIF);
    out.extend_from_slice(tiff);
    Some(out)
}

/// A PNG's geometry, as its decoder reads the chunks up to `IEND`: the first `eXIf` chunk (a
/// second one is ignored, before or after the pixels); the last text chunk with the XMP keyword
/// that its text reader admits (valid CRC, keyword of 1 to 79 bytes; `tEXt` in Latin-1, `iTXt` in
/// UTF-8 with an ASCII language tag and a UTF-8 translated keyword); the resolution of that XMP,
/// else of the EXIF. A compressed XMP chunk counts as one without a resolution (module docs).
pub(super) fn png(b: &[u8]) -> Geometry {
    let mut exif: Option<&[u8]> = None;
    let mut xmp_ppi = None;
    let mut at = PNG_SIGNATURE_LEN;
    while let Some((kind, data, end)) = png_chunk(b, at) {
        if kind == PNG_IEND {
            break;
        }
        if kind == PNG_EXIF && exif.is_none() {
            exif = Some(data);
        }
        if let Some(xmp) = png_xmp(&kind, data, b.get(at + 8 + data.len()..end)) {
            xmp_ppi = xmp.and_then(|text| xmp_resolution(&text));
        }
        at = end;
    }
    let ppi = xmp_ppi.or_else(|| exif.and_then(exif_resolution));
    Geometry::new(exif, ppi)
}

/// The chunk at `at`: its type, its data and where the next one starts.
fn png_chunk(b: &[u8], at: usize) -> Option<([u8; 4], &[u8], usize)> {
    let length: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    let length = usize::try_from(u32::from_be_bytes(length)).ok()?;
    let kind: [u8; 4] = b.get(at + 4..at + 8)?.try_into().ok()?;
    let data = b.get(at + 8..at.checked_add(8)?.checked_add(length)?)?;
    let end = at.checked_add(12)?.checked_add(length)?;
    b.get(at..end)?;
    Some((kind, data, end))
}

/// A text chunk the PNG text reader admits as the XMP packet: `Some(Some(text))`, or `Some(None)`
/// for a compressed one; `None` for any other chunk. `crc`: the chunk's CRC bytes.
fn png_xmp(kind: &[u8; 4], data: &[u8], crc: Option<&[u8]>) -> Option<Option<String>> {
    if !matches!(kind, b"tEXt" | b"zTXt" | b"iTXt") {
        return None;
    }
    let nul = data.iter().position(|&c| c == 0)?;
    let (keyword, value) = (data.get(..nul)?, data.get(nul + 1..)?);
    if keyword != PNG_XMP_KEYWORD || crc != Some(crc32(&[kind.as_slice(), data]).to_be_bytes().as_slice()) {
        return None;
    }
    match kind {
        b"tEXt" => Some(Some(value.iter().map(|&c| char::from(c)).collect())),
        b"zTXt" => (value.first() == Some(&0)).then_some(None),
        _ => {
            let (&flag, &method) = (value.first()?, value.get(1)?);
            if flag > 1 || (flag == 1 && method != 0) {
                return None;
            }
            let rest = value.get(2..)?;
            let language_end = rest.iter().position(|&c| c == 0)?;
            let after_language = rest.get(language_end + 1..)?;
            let translated_end = after_language.iter().position(|&c| c == 0)?;
            if !rest.get(..language_end)?.is_ascii() || std::str::from_utf8(after_language.get(..translated_end)?).is_err() {
                return None;
            }
            let text = after_language.get(translated_end + 1..)?;
            if flag == 1 {
                return Some(None);
            }
            std::str::from_utf8(text).ok().map(|t| Some(t.to_owned()))
        }
    }
}

/// The `eXIf` chunk of a cleaned PNG, `tiff` its TIFF body.
pub(super) fn png_exif_chunk(tiff: &[u8]) -> Option<Vec<u8>> {
    let mut out = u32::try_from(tiff.len()).ok()?.to_be_bytes().to_vec();
    out.extend_from_slice(&PNG_EXIF);
    out.extend_from_slice(tiff);
    out.extend_from_slice(&crc32(&[PNG_EXIF.as_slice(), tiff]).to_be_bytes());
    Some(out)
}

/// The CRC-32 of a PNG chunk (ISO 3309), over `parts` in order.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &byte in parts.iter().flat_map(|p| p.iter()) {
        c ^= u32::from(byte);
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}
