//! Evora build (patch LUM-14): no location leaves the engine in an export (Evora's
//! CONCEPTION-PRO C41, LQ-26: the location is removed by default from the first version on).
//!
//! Under the host bridge, every file but the `.pcraft` working document leaves the engine for the
//! user's Drive as an export (`host_docs::write`). Before such a file is encoded, the document's
//! metadata loses what locates the picture, whatever the format writes (PNG, JPEG, WebP, TIFF,
//! PSD…) and whatever the export dialog chose:
//! - the GPS directory of its EXIF: IFD0's pointer, the directory and the values it points to;
//! - the location properties of its XMP, as attributes or as elements, under any namespace prefix:
//!   every property whose local name starts with `GPS`, and the place names (City, State,
//!   Country, Location, Sublocation…);
//! - the place datasets of a PSD's IPTC record (image resource 1028);
//! - in its free-form text (PNG `tEXt`, `zTXt` and `iTXt`, TIFF ASCII tags): the raw profiles that
//!   other tools write in text (`Raw profile type exif` and `xmp`, hexadecimal, decoded and cleaned
//!   as above; every other raw profile but a colour profile left out), an entry that holds an XMP
//!   packet (cleaned as above), and any entry that still names a GPS or a place.
//!
//! What the engine cannot read with certainty is left out whole, never passed on unread: an EXIF
//! block or an IPTC record it cannot walk, an EXIF block with more than one GPS pointer or a GPS
//! pointer outside IFD0, an XMP packet it cannot scan or that still names a GPS or a place once
//! cleaned (any prefix), a raw profile it cannot decode. The working document keeps everything: it
//! stays the user's own file. Without the bridge, upstream's exports are unchanged.

use std::sync::Arc;

use photocraft_doc::{Document, Metadata};

/// IFD0's pointer to the GPS directory (TIFF tag 34853).
const TAG_GPS_IFD: u16 = 0x8825;
/// The PSD image resource of the IPTC-NAA record.
const IPTC_RESOURCE: u16 = 1028;
/// IPTC record 2 datasets that name a place: content location code and name, city, sub-location,
/// province or state, country code and name.
const IPTC_PLACES: [u8; 7] = [26, 27, 90, 92, 95, 100, 101];
/// The local names of the XMP properties that name a place (Photoshop, IPTC Core and Extension),
/// whatever their prefix; every local name that starts with `GPS` names a coordinate.
const XMP_PLACES: [&str; 14] = [
    "City",
    "State",
    "Country",
    "CountryCode",
    "CountryName",
    "Location",
    "LocationCreated",
    "LocationShown",
    "LocationId",
    "LocationName",
    "Sublocation",
    "ProvinceState",
    "ProvinceName",
    "WorldRegion",
];
/// What a free-form text entry or a cleaned XMP packet must not hold any more.
const GPS_MARK: &str = "GPS";
/// The raw profiles (`Raw profile type <name>`) kept: cleaned (EXIF, XMP) or without location
/// (colour profiles). Any other is left out.
const RAW_PROFILE: &str = "raw profile type ";
/// Text entries whose keyword names a location (lowercase, as a part of the keyword).
const TEXT_PLACE_KEYS: [&str; 8] = ["gps", "latitude", "longitude", "location", "city", "country", "province", "geotag"];

/// One piece of metadata, once examined.
enum Edit<T> {
    /// Nothing locates the picture: kept as it is.
    Kept,
    /// The location removed.
    Changed(T),
    /// Unreadable with certainty: left out whole.
    Dropped,
}

/// A copy of `doc` to encode for `path`: without its location, when it has one and `path` is an
/// export (not a `.pcraft` working document). `None`: encode `doc` itself.
pub fn for_export(doc: &Document, path: &str) -> Option<Document> {
    if photocraft_engine::file_cmds::extension(path).as_deref() == Some("pcraft") {
        return None;
    }
    let metadata = without_location(&doc.metadata)?;
    let mut out = doc.clone();
    out.metadata = metadata;
    Some(out)
}

/// The metadata without its location; `None` when nothing locates the picture.
fn without_location(m: &Metadata) -> Option<Metadata> {
    let exif = m.exif.as_deref().map_or(Edit::Kept, |block| exif_without_gps(block));
    let xmp = m.xmp.as_deref().map_or(Edit::Kept, xmp_without_location);
    let mut resources = Vec::with_capacity(m.psd_resources.len());
    let mut resources_changed = false;
    for (id, name, data) in &m.psd_resources {
        if *id != IPTC_RESOURCE {
            resources.push((*id, name.clone(), data.clone()));
            continue;
        }
        match iptc_without_location(data) {
            Edit::Kept => resources.push((*id, name.clone(), data.clone())),
            Edit::Changed(record) => {
                resources.push((*id, name.clone(), Arc::new(record)));
                resources_changed = true;
            }
            Edit::Dropped => resources_changed = true,
        }
    }
    let text = text_without_location(&m.text);
    if matches!(exif, Edit::Kept) && matches!(xmp, Edit::Kept) && !resources_changed && text.is_none() {
        return None;
    }
    let mut out = m.clone();
    match exif {
        Edit::Kept => {}
        Edit::Changed(block) => out.exif = Some(Arc::new(block)),
        Edit::Dropped => out.exif = None,
    }
    match xmp {
        Edit::Kept => {}
        Edit::Changed(packet) => out.xmp = Some(packet),
        Edit::Dropped => out.xmp = None,
    }
    if resources_changed {
        out.psd_resources = resources;
    }
    if let Some(text) = text {
        out.text = text;
    }
    Some(out)
}

/// The free-form text entries without their location; `None` when nothing locates the picture.
fn text_without_location(entries: &[(String, String)]) -> Option<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(entries.len());
    let mut changed = false;
    for (key, value) in entries {
        match text_entry(key, value) {
            Edit::Kept => out.push((key.clone(), value.clone())),
            Edit::Changed(v) => {
                out.push((key.clone(), v));
                changed = true;
            }
            Edit::Dropped => changed = true,
        }
    }
    changed.then_some(out)
}

/// One free-form text entry (see the module docs).
fn text_entry(key: &str, value: &str) -> Edit<String> {
    let lower = key.to_ascii_lowercase();
    let edit = if let Some(name) = lower.strip_prefix(RAW_PROFILE) {
        raw_profile(name.trim(), value)
    } else if value.contains("<x:xmpmeta") || value.contains("<rdf:RDF") {
        xmp_without_location(value)
    } else {
        Edit::Kept
    };
    if matches!(edit, Edit::Dropped) {
        return Edit::Dropped;
    }
    // Whatever is left must name neither a GPS nor a place.
    let left = match &edit {
        Edit::Changed(v) => v.as_str(),
        _ => value,
    };
    if TEXT_PLACE_KEYS.iter().any(|k| lower.contains(k)) || left.to_ascii_lowercase().contains("gps") {
        return Edit::Dropped;
    }
    edit
}

/// A raw profile in text (`\n<name>\n<length>\n<hexadecimal lines>\n`, as ImageMagick writes
/// it): EXIF and XMP decoded, cleaned and written again; colour profiles kept; anything else, or
/// a profile that cannot be decoded, left out.
fn raw_profile(name: &str, value: &str) -> Edit<String> {
    if matches!(name, "icc" | "icm") {
        return Edit::Kept;
    }
    if !matches!(name, "exif" | "xmp") {
        return Edit::Dropped;
    }
    let Some(bytes) = raw_profile_bytes(value) else { return Edit::Dropped };
    let cleaned = if name == "exif" {
        exif_without_gps(&bytes)
    } else {
        match std::str::from_utf8(&bytes).map(xmp_without_location) {
            Ok(Edit::Changed(x)) => Edit::Changed(x.into_bytes()),
            Ok(Edit::Kept) => Edit::Kept,
            _ => Edit::Dropped,
        }
    };
    match cleaned {
        Edit::Kept => Edit::Kept,
        Edit::Changed(b) => Edit::Changed(raw_profile_text(name, &b)),
        Edit::Dropped => Edit::Dropped,
    }
}

/// The bytes of a raw profile: its name, its decimal length, then the hexadecimal digits, which
/// must give exactly that length. `None` otherwise.
fn raw_profile_bytes(value: &str) -> Option<Vec<u8>> {
    let mut words = value.split_whitespace();
    let _name = words.next()?;
    let length: usize = words.next()?.parse().ok()?;
    let hex: String = words.collect();
    if hex.len() != length.checked_mul(2)? || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..length).map(|i| hex.get(2 * i..2 * i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect()
}

/// A raw profile written back in the same form, 72 hexadecimal digits per line.
fn raw_profile_text(name: &str, bytes: &[u8]) -> String {
    let mut out = format!("\n{name}\n{:8}\n", bytes.len());
    for line in bytes.chunks(36) {
        for b in line {
            out.push_str(&format!("{b:02x}"));
        }
        out.push('\n');
    }
    out
}

/// An EXIF block (with or without its `Exif\0\0` prefix) without its GPS directory.
fn exif_without_gps(block: &[u8]) -> Edit<Vec<u8>> {
    let start = if block.starts_with(b"Exif\0\0") { 6 } else { 0 };
    let (prefix, tiff) = block.split_at(start);
    match gps_removed(tiff) {
        Some(None) => Edit::Kept,
        Some(Some(body)) => {
            let mut out = prefix.to_vec();
            out.extend_from_slice(&body);
            Edit::Changed(out)
        }
        None => Edit::Dropped,
    }
}

/// Reads a TIFF structure in its byte order, every read bounded.
struct Reader<'a> {
    b: &'a [u8],
    le: bool,
}

impl Reader<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        let s: [u8; 2] = self.b.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if self.le { u16::from_le_bytes(s) } else { u16::from_be_bytes(s) })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let s: [u8; 4] = self.b.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if self.le { u32::from_le_bytes(s) } else { u32::from_be_bytes(s) })
    }

    fn offset(&self, at: usize) -> Option<usize> {
        usize::try_from(self.u32(at)?).ok()
    }
}

/// The TIFF body of an EXIF block without IFD0's GPS pointer, the GPS directory and its values
/// zeroed. `Some(None)`: no GPS directory. `None`: the structure cannot be walked.
fn gps_removed(tiff: &[u8]) -> Option<Option<Vec<u8>>> {
    let order = tiff.get(0..2)?;
    let le = if order == b"II" {
        true
    } else if order == b"MM" {
        false
    } else {
        return None;
    };
    let r = Reader { b: tiff, le };
    if r.u16(2)? != 42 {
        return None;
    }
    let ifd0 = r.offset(4)?;
    let count = usize::from(r.u16(ifd0)?);
    let entry = |i: usize| ifd0.checked_add(2 + 12 * i);
    // IFD0's entries and the offset of the next directory, inside the block.
    let end = entry(count)?.checked_add(4)?;
    if end > tiff.len() {
        return None;
    }
    // A GPS pointer in the next directory (IFD1) is not read with certainty: the block is left out.
    let next = r.offset(end - 4)?;
    if next != 0 {
        let n = usize::from(r.u16(next)?);
        let next_end = next.checked_add(2 + 12 * n)?;
        if next_end > tiff.len() || (0..n).any(|i| r.u16(next + 2 + 12 * i) == Some(TAG_GPS_IFD)) {
            return None;
        }
    }
    let pointers: Vec<usize> = (0..count).filter(|&i| entry(i).and_then(|e| r.u16(e)) == Some(TAG_GPS_IFD)).collect();
    // More than one GPS pointer: the block is left out whole.
    let at = match pointers.as_slice() {
        [] => return Some(None),
        [at] => *at,
        _ => return None,
    };
    let pointer = entry(at)?;
    let mut out = tiff.to_vec();
    clear_directory(&r, &mut out, r.offset(pointer + 8)?)?;
    // IFD0 without the pointer: the entries after it and the next offset move up one entry, the
    // freed bytes are zeroed, and the count goes down by one.
    let from = entry(at + 1)?;
    out.copy_within(from..end, pointer);
    out.get_mut(end - 12..end)?.fill(0);
    put_u16(&mut out, ifd0, u16::try_from(count - 1).ok()?, le)?;
    Some(Some(out))
}

/// Zero the directory at `at` (count, entries, next offset) and the values its entries keep
/// outside themselves. `None` when the directory or a value lies outside the block, or an entry
/// has a type of unknown size.
fn clear_directory(r: &Reader<'_>, out: &mut [u8], at: usize) -> Option<()> {
    let n = usize::from(r.u16(at)?);
    let entries_end = at.checked_add(2 + 12 * n)?.checked_add(4)?;
    if entries_end > out.len() {
        return None;
    }
    for i in 0..n {
        let e = at + 2 + 12 * i;
        let size = type_size(r.u16(e + 2)?)?.checked_mul(r.offset(e + 4)?)?;
        if size > 4 {
            let value = r.offset(e + 8)?;
            out.get_mut(value..value.checked_add(size)?)?.fill(0);
        }
    }
    out.get_mut(at..entries_end)?.fill(0);
    Some(())
}

/// Bytes of one value of a TIFF field type (TIFF 6.0, BigTIFF); `None` for an unknown type.
fn type_size(t: u16) -> Option<usize> {
    Some(match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16..=18 => 8,
        _ => return None,
    })
}

fn put_u16(out: &mut [u8], at: usize, v: u16, le: bool) -> Option<()> {
    let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
    out.get_mut(at..at.checked_add(2)?)?.copy_from_slice(&bytes);
    Some(())
}

/// Does the XMP property `name` (`prefix:local`, any prefix) locate the picture?
fn locates(name: &str) -> bool {
    let local = name.split_once(':').map_or(name, |(_, local)| local);
    local.starts_with(GPS_MARK) || XMP_PLACES.contains(&local)
}

/// Does a cleaned XMP packet still name a GPS, or a place property under any prefix?
fn names_location(xmp: &str) -> bool {
    if xmp.contains(GPS_MARK) {
        return true;
    }
    XMP_PLACES.iter().any(|place| {
        let needle = format!(":{place}");
        xmp.match_indices(needle.as_str()).any(|(i, _)| {
            xmp.as_bytes().get(i + needle.len()).is_none_or(|&c| c.is_ascii_whitespace() || matches!(c, b'=' | b'>' | b'/'))
        })
    })
}

/// An XMP packet without its location properties: elements (with their content) and attributes.
fn xmp_without_location(xmp: &str) -> Edit<String> {
    let mut out = String::with_capacity(xmp.len());
    let mut copied = 0;
    let mut at = 0;
    let mut changed = false;
    while let Some(rel) = xmp.get(at..).and_then(|s| s.find('<')) {
        let open = at + rel;
        let Some(close) = tag_end(xmp, open) else { return Edit::Dropped };
        let Some(tag) = xmp.get(open..=close) else { return Edit::Dropped };
        at = close + 1;
        if tag.starts_with("</") || tag.starts_with("<?") || tag.starts_with("<!") {
            continue;
        }
        let name = element_name(tag);
        if locates(name) {
            let end = if tag.ends_with("/>") { Some(close) } else { closing_tag(xmp, close + 1, name) };
            let Some(end) = end else { return Edit::Dropped };
            out.push_str(xmp.get(copied..open).unwrap_or_default());
            copied = end + 1;
            at = end + 1;
            changed = true;
            continue;
        }
        match tag_without_attributes(tag) {
            Edit::Kept => {}
            Edit::Changed(clean) => {
                out.push_str(xmp.get(copied..open).unwrap_or_default());
                out.push_str(&clean);
                copied = close + 1;
                changed = true;
            }
            Edit::Dropped => return Edit::Dropped,
        }
    }
    if changed {
        out.push_str(xmp.get(copied..).unwrap_or_default());
    }
    let cleaned = if changed { out.as_str() } else { xmp };
    // A coordinate or a place left (in text, or in a structure not read): the packet is not passed on.
    if names_location(cleaned) {
        return Edit::Dropped;
    }
    if changed { Edit::Changed(out) } else { Edit::Kept }
}

/// Index of the `>` that ends the tag opened at `open` (quoted values, comments and CDATA
/// sections skipped).
fn tag_end(xmp: &str, open: usize) -> Option<usize> {
    let rest = xmp.get(open..)?;
    if rest.starts_with("<!--") {
        return rest.find("-->").map(|i| open + i + 2);
    }
    if rest.starts_with("<![CDATA[") {
        return rest.find("]]>").map(|i| open + i + 2);
    }
    let mut quote: Option<u8> = None;
    for (i, c) in rest.bytes().enumerate().skip(1) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'"' || c == b'\'' => quote = Some(c),
            None if c == b'>' => return Some(open + i),
            None => {}
        }
    }
    None
}

/// The qualified name of the element a start tag opens.
fn element_name(tag: &str) -> &str {
    let inner = tag.get(1..).unwrap_or_default();
    let end = inner.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(inner.len());
    inner.get(..end).unwrap_or_default()
}

/// Index of the `>` of the `</name>` tag that closes an element, searched from `from`.
fn closing_tag(xmp: &str, from: usize, name: &str) -> Option<usize> {
    let needle = format!("</{name}");
    let mut at = from;
    loop {
        let i = at + xmp.get(at..)?.find(needle.as_str())?;
        let after = i + needle.len();
        let next = xmp.as_bytes().get(after).copied()?;
        if next == b'>' || next.is_ascii_whitespace() {
            return xmp.get(after..)?.find('>').map(|j| after + j);
        }
        at = after;
    }
}

/// A start tag without its location attributes.
fn tag_without_attributes(tag: &str) -> Edit<String> {
    let b = tag.as_bytes();
    let mut i = 1 + element_name(tag).len();
    let mut out = String::with_capacity(tag.len());
    let mut copied = 0;
    let mut changed = false;
    loop {
        let space = i;
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        match b.get(i).copied() {
            None => return Edit::Dropped,
            Some(b'>' | b'/') => break,
            Some(_) => {}
        }
        // Attributes are separated by white space.
        if i == space {
            return Edit::Dropped;
        }
        let name_start = i;
        while b.get(i).is_some_and(|&c| !c.is_ascii_whitespace() && c != b'=' && c != b'>' && c != b'/') {
            i += 1;
        }
        let name = tag.get(name_start..i).unwrap_or_default();
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if b.get(i) != Some(&b'=') {
            return Edit::Dropped;
        }
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let Some(&quote) = b.get(i).filter(|&&c| c == b'"' || c == b'\'') else { return Edit::Dropped };
        let Some(len) = tag.get(i + 1..).and_then(|s| s.find(char::from(quote))) else { return Edit::Dropped };
        i += 2 + len;
        if locates(name) {
            out.push_str(tag.get(copied..space).unwrap_or_default());
            copied = i;
            changed = true;
        }
    }
    if !changed {
        return Edit::Kept;
    }
    out.push_str(tag.get(copied..).unwrap_or_default());
    Edit::Changed(out)
}

/// A PSD's IPTC-NAA record without the datasets that name a place.
fn iptc_without_location(data: &[u8]) -> Edit<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut at = 0;
    let mut changed = false;
    while let Some(rest) = data.get(at..).filter(|r| !r.is_empty()) {
        // The resource's padding.
        if rest.iter().all(|&c| c == 0) {
            out.extend_from_slice(rest);
            break;
        }
        let (Some(&0x1C), Some(&record), Some(&dataset), Some(length)) = (rest.first(), rest.get(1), rest.get(2), rest.get(3..5)) else { return Edit::Dropped };
        let length = u16::from_be_bytes([length[0], length[1]]);
        let (header, size) = if length & 0x8000 == 0 {
            (5, usize::from(length))
        } else {
            // Extended dataset: the length is held in the next 1 to 3 bytes.
            let n = usize::from(length & 0x7FFF);
            let Some(bytes) = rest.get(5..5 + n).filter(|_| (1..=3).contains(&n)) else { return Edit::Dropped };
            (5 + n, bytes.iter().fold(0usize, |v, &c| (v << 8) | usize::from(c)))
        };
        let Some(set) = header.checked_add(size).and_then(|total| rest.get(..total)) else { return Edit::Dropped };
        if record == 2 && IPTC_PLACES.contains(&dataset) {
            changed = true;
        } else {
            out.extend_from_slice(set);
        }
        at += set.len();
    }
    if !changed {
        return Edit::Kept;
    }
    if out.iter().all(|&c| c == 0) { Edit::Dropped } else { Edit::Changed(out) }
}
