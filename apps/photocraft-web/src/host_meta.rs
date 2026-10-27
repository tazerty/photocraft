//! Evora build (patch LUM-14): no location leaves the engine in an export (Evora's
//! CONCEPTION-PRO C41, LQ-26: the location is removed by default from the first version on).
//!
//! Under the host bridge, every file but the `.pcraft` working document leaves the engine for the
//! user's Drive as an export (`host_docs::write`). Before such a file is encoded, the document's
//! metadata loses what locates the picture, whatever the format writes (PNG, JPEG, WebP, TIFF,
//! PSD…) and whatever the export dialog chose:
//! - the GPS directory of its EXIF: IFD0's pointer, the directory and the values it points to;
//! - the location properties of its XMP, as attributes or as elements, under any namespace prefix
//!   and in any case: every property whose local name starts with `GPS` or holds `location`
//!   (shotLocation, LocationShown…), and the place names (City, State, Country, Sublocation…);
//! - the place datasets of a PSD's IPTC record (image resource 1028);
//! - in its free-form text (PNG `tEXt`, `zTXt` and `iTXt`, TIFF ASCII tags): the raw profiles that
//!   other tools write in text (`Raw profile type exif` and `xmp`, hexadecimal, decoded and cleaned
//!   as above; every other raw profile but a colour profile left out), an entry that holds an XMP
//!   packet (known by its namespace URIs or by an `xmpmeta` or `RDF` element of any prefix, in any
//!   case; cleaned as above), and any entry whose keyword or value still names a GPS or a place.
//!
//! What the engine cannot read with certainty is left out whole, never passed on unread: an EXIF
//! block or an IPTC record it cannot walk (every directory of the IFD0 → IFDn chain and of the Exif,
//! Interoperability and SubIFDs sub-directories is walked, with a bound and a loop check), an EXIF
//! block with a GPS pointer anywhere but one entry of IFD0, an XMP packet it cannot scan or that
//! still names a GPS or a place once cleaned (any prefix, any case), a raw profile it cannot
//! decode. The working document keeps everything: it
//! stays the user's own file. Without the bridge, upstream's exports are unchanged.
//!
//! Patch LUM-14 r2 (Evora's LQ-26 (b), C41): the host chooses the policy (`evora:init`, preference
//! `evora.locationPolicy`, read by `host_prefs`), and the layers are cleaned as well:
//! - `none`, the default (also for a missing or unknown value): as above;
//! - `rights`: the rights alone leave, whatever the export dialog chose: an XMP packet rebuilt
//!   from the document's copyright, its status and URL (the engine's own reading, Save for Web's
//!   « Copyright »), the free-form text entries named after the rights, no EXIF, no IPTC record or
//!   other description resource, no layer XMP;
//! - `all`: the export dialog's choice, location included (the user's explicit choice).
//!
//! Since upstream v0.15.1 a PSD (and a layered TIFF) writes each layer's own XMP packet
//! (`Layer::xmp`), and a layer's `shmd` block that still holds one (a document from an earlier
//! version) is written as it is: under `none` both lose their location like the document's packet
//! (an unreadable `shmd` block is left out whole), under `rights` both lose their packet. The
//! PSD's EXIF data 3 resource (1059), which the engine does not read, is left out.
//!
//! Patch LUM-14 r3 (the security review of LF-3): what an export to a format that writes layers
//! (PSD, PSB, layered TIFF) carries besides the metadata is cleaned too, under `none` as under
//! `rights`:
//! - the PSD global blocks: only those the exporter reads or writes again are kept (patterns, the
//!   type layers' text engine data, the smart filters' caches and mask colour), and the notes under
//!   `none`; every other block is left out, among them the files placed by smart objects (`lnk2`,
//!   `lnk3`, `lnkD`, `lnkE`), and under `rights` the notes too;
//! - the file a smart object places (embedded in it, or kept in those blocks for an imported PSD)
//!   is embedded again, cleaned: a PNG or a JPEG without its metadata (only the chunks or segments
//!   that code the image, its colour profile and geometry, copied as they are: the pixels are not
//!   encoded again), a document (`.pcraft`, PSD, PSB) opened, cleaned under the same policy and
//!   written as a `.pcraft` document (the exporter makes a PSB of it); any other file, a link to a
//!   file outside the document, or a file that cannot be read with certainty makes the layer its
//!   pixels, as Layer › Rasterize › Smart Object does; the placed-layer blocks are written again
//!   from the document;
//! - a layer's `shmd` block is rebuilt from what is known to be safe instead of cleaned in place:
//!   one `cust` item with the layer's `layerTime` and, under `none`, its one XMP packet without its
//!   location. A block that holds more than one packet, or a packet that is not text, is left out
//!   whole, and so is every other item (the layer comp states, which the exporter writes again from
//!   the document's comps, the timeline, anything else).
//!
//! Patch LUM-14 r4 (the verification of r3): a placed PNG or JPEG cleaned of its metadata keeps
//! what the engine reads from it to show it, written back as one EXIF directory without anything
//! else (module `geometry`): the Orientation that turns its pixels upright, and the resolution its
//! metadata records. A phone photo placed in a smart object thus leaves an export upright, at its
//! size and resolution, under `none` as under `rights`.

mod geometry;

use std::cell::RefCell;
use std::sync::Arc;

use photocraft_doc::{Document, Layer, LayerContent, Metadata, PsdGlobalBlock, SmartSource, Surface};
use photocraft_psd::descriptor::{Descriptor, Id, UnicodeString, Value, VersionedDescriptor};
use photocraft_psd::metadata::MetadataItem;

/// What an export keeps of the metadata that locates the picture (patch LUM-14 r2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// The export dialog's choice, without the location (the default).
    #[default]
    None,
    /// The rights alone.
    Rights,
    /// The export dialog's choice, location included.
    All,
}

impl Policy {
    /// The host's name for a policy (`none`, `rights`, `all`); `None` for any other.
    pub fn parse(name: &str) -> Option<Policy> {
        match name {
            "none" => Some(Policy::None),
            "rights" => Some(Policy::Rights),
            "all" => Some(Policy::All),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Policy::None => "none",
            Policy::Rights => "rights",
            Policy::All => "all",
        }
    }
}

/// IFD0's pointer to the GPS directory (TIFF tag 34853).
const TAG_GPS_IFD: u16 = 0x8825;
/// The pointers to sub-directories walked for a GPS pointer: Exif, Interoperability, SubIFDs.
const TAG_SUB_IFDS: [u16; 3] = [0x8769, 0xA005, 0x014A];
/// The most directories an EXIF block may have before it is left out (a walk bound, not a limit of
/// the product: real files have a handful).
const MAX_EXIF_DIRECTORIES: usize = 64;
/// The XMP namespace URIs that make a text entry an XMP packet (compared in lower case).
const XMP_URIS: [&str; 2] = ["adobe:ns:meta/", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"];
/// The PSD image resource of the IPTC-NAA record.
const IPTC_RESOURCE: u16 = 1028;
/// The PSD image resource « EXIF data 3 », which the engine does not read: left out of exports.
const EXIF_DATA_3: u16 = 1059;
/// PSD image resources that describe the picture (caption, IPTC-NAA record, EXIF data 3, caption
/// digest): left out when only the rights leave.
const DESCRIPTIVE_RESOURCES: [u16; 4] = [1008, IPTC_RESOURCE, EXIF_DATA_3, 1061];
/// The free-form text entries kept when only the rights leave (keyword, in lower case).
const RIGHTS_KEYS: [&str; 2] = ["copyright", "rights"];
/// A layer's metadata block in a PSD (`shmd`), where its own XMP packet lives.
const SHMD: [u8; 4] = *b"shmd";
/// The `shmd` item that holds a layer's own metadata, and the two values of it an export keeps
/// (patch LUM-14 r3): the layer's modify time (a number) and its XMP packet.
const CUST: [u8; 4] = *b"cust";
const LAYER_TIME: &str = "layerTime";
const LAYER_XMP: &str = "layerXMP";
/// The export formats that write layers (PSD, PSB, layered TIFF): the files placed by smart
/// objects and the PSD global blocks leave with them (patch LUM-14 r3).
const LAYERED_EXPORTS: [&str; 4] = ["psd", "psb", "tif", "tiff"];
/// The PSD blocks that hold the files placed by smart objects, embedded or linked.
const LINK_KEYS: [[u8; 4]; 4] = [*b"lnk2", *b"lnk3", *b"lnkD", *b"lnkE"];
/// A smart object's placed-layer blocks, which the exporter writes again from the document.
const PLACED_KEYS: [[u8; 4]; 3] = [*b"SoLd", *b"PlLd", *b"SoLE"];
/// The PSD global blocks an export keeps under `none` and `rights`, each read or written again by
/// the exporter: patterns (`Patt`, `Pat2`, `Pat3`), the type layers' text engine data (`Txt2`),
/// the smart filters' caches and mask colour (`FEid`, `FXid`, `FMsk`). Any other is left out.
const GLOBAL_KEPT: [[u8; 4]; 7] = [*b"Patt", *b"Pat2", *b"Pat3", *b"Txt2", *b"FEid", *b"FXid", *b"FMsk"];
/// The notes' global block: kept under `none`, left out with the notes under `rights`.
const ANNO: [u8; 4] = *b"Anno";
/// How deep documents placed by smart objects are cleaned: the exporter's own bound on nested smart
/// objects (`MAX_NESTING` of `photocraft_io::psd_export`). Deeper, the smart object is its pixels.
const MAX_NESTED_FILES: u32 = 8;
/// A PNG's signature, and the chunks a placed PNG keeps: its image, its colour and its geometry.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const PNG_KEPT: [[u8; 4]; 15] = [
    *b"IHDR", *b"PLTE", *b"IDAT", *b"IEND", *b"tRNS", *b"cHRM", *b"gAMA", *b"iCCP", *b"sBIT", *b"sRGB", *b"cICP", *b"mDCV", *b"cLLI", *b"bKGD",
    *b"pHYs",
];
/// A JPEG's start of image.
const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
/// A ZIP archive's first bytes: a `.pcraft` document, or another document the engine opens.
const ZIP_SIGNATURE: [u8; 4] = *b"PK\x03\x04";
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
/// What a free-form text entry or a cleaned XMP packet must not hold any more (in any case).
const GPS_MARK: &str = "gps";
/// What a property or a text must not hold any more, in any case: a location.
const LOCATION_MARK: &str = "location";
/// The raw profiles (`Raw profile type <name>`) kept: cleaned (EXIF, XMP) or without location
/// (colour profiles). Any other is left out.
const RAW_PROFILE: &str = "raw profile type ";
/// Text entries whose keyword names a location (lowercase, as a part of the keyword).
const TEXT_PLACE_KEYS: [&str; 12] =
    ["gps", "latitude", "longitude", "location", "sublocation", "city", "state", "country", "province", "region", "place", "geotag"];

/// One piece of metadata, once examined.
enum Edit<T> {
    /// Nothing locates the picture: kept as it is.
    Kept,
    /// The location removed.
    Changed(T),
    /// Unreadable with certainty: left out whole.
    Dropped,
}

/// A copy of `doc` to encode for `path` under `policy`: without its location (or with its rights
/// alone), when it has metadata to change and `path` is an export (not a `.pcraft` working
/// document). `None`: encode `doc` itself.
pub fn for_export(doc: &Document, path: &str, policy: Policy) -> Option<Document> {
    let ext = photocraft_engine::file_cmds::extension(path);
    if policy == Policy::All || ext.as_deref() == Some("pcraft") {
        return None;
    }
    let layered = ext.as_deref().is_some_and(|e| LAYERED_EXPORTS.contains(&e));
    export_copy(doc, policy, layered, 0)
}

/// The copy of `doc` an export encodes under `policy` (`none` or `rights`), `None` when nothing
/// changes; `layered`: the format writes layers (patch LUM-14 r3); `depth`: how deep in smart
/// objects `doc` lies (0: the exported document).
fn export_copy(doc: &Document, policy: Policy, layered: bool, depth: u32) -> Option<Document> {
    let metadata = match policy {
        Policy::Rights => Some(rights_only(doc)),
        _ => without_location(&doc.metadata),
    };
    let globals = if layered { globals_for_export(&doc.metadata.psd_global_blocks, policy) } else { None };
    let scope = Scope { doc, policy, layered, depth, files: RefCell::new(Vec::new()) };
    let layers = layers_for_export(&doc.layers, &scope);
    // Under `rights`, the notes are left out too (a format that writes layers keeps them in `Anno`).
    let notes = layered && policy == Policy::Rights && !doc.notes.is_empty();
    if metadata.is_none() && globals.is_none() && layers.is_none() && !notes {
        return None;
    }
    let mut out = doc.clone();
    if let Some(metadata) = metadata {
        out.metadata = metadata;
    }
    if let Some(globals) = globals {
        out.metadata.psd_global_blocks = globals;
    }
    if let Some(layers) = layers {
        out.layers = layers;
    }
    if notes {
        out.notes = Vec::new();
    }
    Some(out)
}

/// What the layers of one document need for an export: the document itself (its global blocks
/// hold the files of an imported PSD's smart objects), the policy, whether the format writes
/// layers, how deep in smart objects the document lies, and the placed files already cleaned (the
/// duplicates of a smart object share its file).
struct Scope<'a> {
    doc: &'a Document,
    policy: Policy,
    layered: bool,
    depth: u32,
    files: RefCell<Vec<(FileKey, Option<SmartSource>)>>,
}

/// A placed file, by identity: the bytes a smart object embeds (its duplicates share them), or
/// the id of a file in the document's global blocks.
#[derive(PartialEq)]
enum FileKey {
    Embedded(usize),
    Linked(String),
}

/// The PSD global blocks of an export under `policy` (`none` or `rights`): those of
/// [`GLOBAL_KEPT`], and the notes (`Anno`) under `none`; `None` when every block is kept.
fn globals_for_export(blocks: &[PsdGlobalBlock], policy: Policy) -> Option<Vec<PsdGlobalBlock>> {
    let kept = |key: &[u8; 4]| GLOBAL_KEPT.contains(key) || (policy == Policy::None && *key == ANNO);
    if blocks.iter().all(|(_, key, _)| kept(key)) {
        return None;
    }
    Some(blocks.iter().filter(|(_, key, _)| kept(key)).cloned().collect())
}

/// The rights alone (policy `rights`, see the module docs). What is kept is cleaned of any
/// location all the same.
fn rights_only(doc: &Document) -> Metadata {
    let m = &doc.metadata;
    let xmp = photocraft_engine::web_cmds::web_xmp(doc, photocraft_engine::web_cmds::WebMetadata::Copyright).and_then(|x| match xmp_without_location(&x) {
        Edit::Kept => Some(x),
        Edit::Changed(c) => Some(c),
        Edit::Dropped => None,
    });
    let text = m
        .text
        .iter()
        .filter(|(key, _)| RIGHTS_KEYS.contains(&key.to_ascii_lowercase().as_str()))
        .filter_map(|(key, value)| match text_entry(key, value) {
            Edit::Kept => Some((key.clone(), value.clone())),
            Edit::Changed(v) => Some((key.clone(), v)),
            Edit::Dropped => None,
        })
        .collect();
    let psd_resources = m.psd_resources.iter().filter(|(id, _, _)| !DESCRIPTIVE_RESOURCES.contains(id)).cloned().collect();
    // Since upstream v0.16.0 the document's packet is shared text (`Arc<str>`).
    Metadata { xmp: xmp.map(Arc::<str>::from), exif: None, text, psd_resources, ..m.clone() }
}

/// Does `layer`, or a layer of its group, hold an XMP packet, a `shmd` block or a placed file's
/// block, or, for a format that writes layers, is it a smart object?
fn needs_export_copy(layer: &Layer, layered: bool) -> bool {
    layer.xmp.is_some()
        || layer.psd_blocks.iter().any(|(key, _)| *key == SHMD || LINK_KEYS.contains(key))
        || (layered && matches!(layer.content, LayerContent::Smart(_)))
        || layer.children().is_some_and(|ch| ch.iter().any(|c| needs_export_copy(c, layered)))
}

/// The layers for an export under the scope's policy (`none` or `rights`); `None`: no layer needs
/// a change.
fn layers_for_export(layers: &[Layer], scope: &Scope<'_>) -> Option<Vec<Layer>> {
    if !layers.iter().any(|l| needs_export_copy(l, scope.layered)) {
        return None;
    }
    Some(layers.iter().map(|l| layer_for_export(l, scope)).collect())
}

/// One layer (and its group's layers) for an export: its XMP packet without its location, or none
/// for the rights alone; its `shmd` block rebuilt (`shmd_for_export`); no placed file's block
/// (patch LUM-14 r3); for a format that writes layers, a smart object's file cleaned
/// (`smart_for_export`).
fn layer_for_export(layer: &Layer, scope: &Scope<'_>) -> Layer {
    let mut out = layer.clone();
    out.xmp = match (&layer.xmp, scope.policy) {
        (Some(_), Policy::Rights) | (None, _) => None,
        (Some(x), _) => match xmp_without_location(x) {
            Edit::Kept => Some(x.clone()),
            Edit::Changed(c) => Some(Arc::from(c)),
            Edit::Dropped => None,
        },
    };
    out.psd_blocks = layer
        .psd_blocks
        .iter()
        .filter(|(key, _)| !LINK_KEYS.contains(key))
        .filter_map(|(key, data)| if *key == SHMD { shmd_for_export(data, scope.policy).map(|d| (*key, d)) } else { Some((*key, data.clone())) })
        .collect();
    if scope.layered {
        smart_for_export(&mut out, scope);
    }
    if let (Some(children), Some(source)) = (out.children_mut(), layer.children()) {
        *children = source.iter().map(|c| layer_for_export(c, scope)).collect();
    }
    out
}

/// A layer's `shmd` block for an export (patch LUM-14 r3), rebuilt from what is known to be safe
/// rather than cleaned in place: one `cust` item with the layer's `layerTime` (a number) and,
/// under `none`, its one XMP packet without its location. Every other item and value is left out
/// (the layer comp states, `cmls`, which the exporter writes again from the document's comps, the
/// timeline, anything the engine does not read). `None`, the block left out whole: it cannot be
/// read, it holds more than one packet (in one `cust` item or in several) or a packet that is not
/// text, or nothing is left.
fn shmd_for_export(data: &[u8], policy: Policy) -> Option<Arc<Vec<u8>>> {
    let items = photocraft_psd::metadata::parse_shmd(data).ok()?;
    let mut time = None;
    let mut packets = Vec::new();
    for item in items.iter().filter(|item| item.key == CUST) {
        let (cust, _) = VersionedDescriptor::parse_prefix(&item.data).ok()?;
        for (key, value) in cust.descriptor.items {
            if key.is(LAYER_XMP) {
                packets.push(value);
            } else if key.is(LAYER_TIME) && time.is_none() {
                if let Value::Double(t) = value {
                    time = Some(t);
                }
            }
        }
    }
    let packet = match packets.as_slice() {
        [] => None,
        [Value::Text(text)] => Some(text.to_string_lossy()),
        _ => return None,
    };
    let packet = match packet {
        Some(xmp) if policy == Policy::None => match xmp_without_location(&xmp) {
            Edit::Kept => Some(xmp),
            Edit::Changed(c) => Some(c),
            Edit::Dropped => None,
        },
        _ => None,
    };
    let mut cust = Descriptor { name: UnicodeString::new_nul(""), class_id: Id::new("metadata"), items: Vec::new() };
    if let Some(t) = time {
        cust.items.push((Id::new(LAYER_TIME), Value::Double(t)));
    }
    if let Some(xmp) = packet {
        cust.items.push((Id::new(LAYER_XMP), Value::Text(UnicodeString::new_nul(&xmp))));
    }
    if cust.items.is_empty() {
        return None;
    }
    let item = MetadataItem::new(CUST, VersionedDescriptor::new(cust).to_bytes());
    Some(Arc::new(photocraft_psd::metadata::write_shmd(&[item])))
}

/// A smart object, for a format that writes layers (patch LUM-14 r3): the file it places cleaned
/// and embedded in it (`placed_file`), its placed-layer blocks written again by the exporter from
/// the document; a file that cannot be cleaned with certainty, or a link to a file outside the
/// document, makes the layer its pixels, as Layer › Rasterize › Smart Object does (its rendered
/// appearance, in the document's pixel format).
fn smart_for_export(layer: &mut Layer, scope: &Scope<'_>) {
    let LayerContent::Smart(sm) = &mut layer.content else { return };
    match placed_file(&sm.source, scope) {
        Some(source) => {
            sm.source = source;
            sm.psd_raw = None;
        }
        None => {
            let fmt = scope.doc.pixel_format();
            let pixels = match sm.cache.take() {
                Some(c) if c.format() == fmt => c,
                Some(c) => c.convert(fmt),
                None => Surface::new(fmt),
            };
            layer.content = LayerContent::Raster(pixels);
        }
    }
    layer.psd_blocks.retain(|(key, _)| !PLACED_KEYS.contains(key));
}

/// The source of a smart object for an export: its file (embedded in it, or kept in the document's
/// global blocks for an imported PSD) cleaned by `file_for_export` and embedded; `None` when that
/// file cannot be cleaned with certainty, or lies outside the document.
fn placed_file(source: &SmartSource, scope: &Scope<'_>) -> Option<SmartSource> {
    let key = match source {
        SmartSource::Embedded { bytes, .. } => FileKey::Embedded(Arc::as_ptr(bytes) as usize),
        SmartSource::Linked { path } => FileKey::Linked(path.clone()),
    };
    if let Some((_, done)) = scope.files.borrow().iter().find(|(k, _)| *k == key) {
        return done.clone();
    }
    let linked;
    let (file_name, bytes) = match source {
        SmartSource::Embedded { file_name, bytes } => (file_name.as_str(), bytes.as_slice()),
        SmartSource::Linked { path } => {
            linked = photocraft_io::linked::find_linked_file(&scope.doc.metadata, path)?;
            (linked.file_name.as_str(), linked.bytes.as_slice())
        }
    };
    let done = file_for_export(file_name, bytes, scope).map(|cleaned| SmartSource::Embedded { file_name: file_name.to_owned(), bytes: Arc::new(cleaned) });
    scope.files.borrow_mut().push((key, done.clone()));
    done
}

/// A placed file cleaned for an export: a PNG or a JPEG without its metadata, its pixels as they
/// are (`png_without_metadata`, `jpeg_without_metadata`); a document the engine opens (a
/// `.pcraft` or another archive, a PSD or a PSB) opened, cleaned under the same policy and written
/// again as a `.pcraft` document, which the exporter makes a PSB of. `None` for any other file, a
/// file that cannot be read with certainty, or a document nested deeper than [`MAX_NESTED_FILES`].
fn file_for_export(name: &str, bytes: &[u8], scope: &Scope<'_>) -> Option<Vec<u8>> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        return png_without_metadata(bytes);
    }
    if bytes.starts_with(&JPEG_SOI) {
        return jpeg_without_metadata(bytes);
    }
    if !(photocraft_io::is_psd(bytes) || bytes.starts_with(&ZIP_SIGNATURE)) || scope.depth >= MAX_NESTED_FILES {
        return None;
    }
    let nested = photocraft_io::import(name, bytes).ok()?.document;
    let cleaned = export_copy(&nested, scope.policy, true, scope.depth + 1);
    let doc = cleaned.as_ref().unwrap_or(&nested);
    photocraft_io::export(doc, "pcraft", &photocraft_io::ExportOptions::default()).ok().map(|r| r.bytes)
}

/// A PNG with the chunks of [`PNG_KEPT`] alone, each copied as it is, and after its header the
/// `eXIf` chunk of its geometry (patch LUM-14 r4, module `geometry`); whatever follows `IEND` is
/// left out. `None` when the chunks cannot be walked up to `IEND`.
fn png_without_metadata(b: &[u8]) -> Option<Vec<u8>> {
    let mut exif = geometry::png(b).exif().and_then(|tiff| geometry::png_exif_chunk(&tiff));
    let mut out = PNG_SIGNATURE.to_vec();
    let mut at = PNG_SIGNATURE.len();
    loop {
        let length: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
        let end = at.checked_add(12)?.checked_add(usize::try_from(u32::from_be_bytes(length)).ok()?)?;
        let chunk = b.get(at..end)?;
        let kind: [u8; 4] = chunk.get(4..8)?.try_into().ok()?;
        if PNG_KEPT.contains(&kind) {
            out.extend_from_slice(chunk);
            if kind == *b"IHDR"
                && let Some(written) = exif.take()
            {
                out.extend_from_slice(&written);
            }
        }
        if kind == *b"IEND" {
            return Some(out);
        }
        at = end;
    }
}

/// A JPEG with the segments of [`jpeg_segment_kept`] alone, each copied as it is, and its scans'
/// coded data, with the EXIF segment of its geometry after the JFIF header (patch LUM-14 r4,
/// module `geometry`); whatever follows the end of image is left out (the further pictures of a
/// multi-picture file among them). `None` when the segments cannot be walked up to the end of
/// image.
fn jpeg_without_metadata(b: &[u8]) -> Option<Vec<u8>> {
    let mut exif = geometry::jpeg(b).exif().and_then(|tiff| geometry::jpeg_exif_segment(&tiff));
    let mut out = JPEG_SOI.to_vec();
    let mut at = JPEG_SOI.len();
    loop {
        if *b.get(at)? != 0xFF {
            return None;
        }
        // Fill bytes before a marker.
        while b.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        let marker = *b.get(at + 1)?;
        at += 2;
        match marker {
            0xD9 => {
                out.extend_from_slice(&[0xFF, 0xD9]);
                return Some(out);
            }
            // No marker there, or a second start of image.
            0x00 | 0xD8 => return None,
            // Markers without a length.
            0x01 | 0xD0..=0xD7 => {
                out.extend_from_slice(&[0xFF, marker]);
                continue;
            }
            _ => {}
        }
        let length = usize::from(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]));
        let segment = b.get(at..at.checked_add(length)?).filter(|_| length >= 2)?;
        let kept = jpeg_segment_kept(marker, segment);
        if kept {
            if marker != 0xE0
                && let Some(written) = exif.take()
            {
                out.extend_from_slice(&written);
            }
            out.extend_from_slice(&[0xFF, marker]);
            out.extend_from_slice(segment);
        }
        at += length;
        if marker == 0xDA {
            // The scan's coded data, up to the next marker that is neither a stuffed byte (`FF 00`)
            // nor a restart (`FF D0` to `FF D7`).
            let start = at;
            while !(*b.get(at)? == 0xFF && b.get(at + 1).is_some_and(|&m| m != 0x00 && !(0xD0..=0xD7).contains(&m))) {
                at += 1;
            }
            out.extend_from_slice(b.get(start..at)?);
        }
    }
}

/// Is a JPEG segment kept for an export: one that codes the image (frames and tables, `C0` to
/// `CF`; scan, quantization tables, number of lines, restart interval, hierarchical progression and
/// expansion, `DA` to `DF`), the JFIF header, an ICC profile, Adobe's colour transform. Every other
/// segment is left out: EXIF and XMP (APP1), IPTC (APP13), comments, any other application data.
fn jpeg_segment_kept(marker: u8, segment: &[u8]) -> bool {
    let body = segment.get(2..).unwrap_or_default();
    match marker {
        0xC0..=0xCF | 0xDA..=0xDF => true,
        0xE0 => body.starts_with(b"JFIF\0"),
        0xE2 => body.starts_with(b"ICC_PROFILE\0"),
        0xEE => body.starts_with(b"Adobe"),
        _ => false,
    }
}

/// The metadata without its location; `None` when nothing locates the picture.
fn without_location(m: &Metadata) -> Option<Metadata> {
    let exif = m.exif.as_deref().map_or(Edit::Kept, |block| exif_without_gps(block));
    let xmp = m.xmp.as_deref().map_or(Edit::Kept, xmp_without_location);
    let mut resources = Vec::with_capacity(m.psd_resources.len());
    let mut resources_changed = false;
    for (id, name, data) in &m.psd_resources {
        // EXIF data 3: not read by the engine, so never passed on unread.
        if *id == EXIF_DATA_3 {
            resources_changed = true;
            continue;
        }
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
        Edit::Changed(packet) => out.xmp = Some(Arc::<str>::from(packet)),
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
    } else if looks_like_xmp(value) {
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
    if TEXT_PLACE_KEYS.iter().any(|k| lower.contains(k)) || names_location(left) {
        return Edit::Dropped;
    }
    edit
}

/// Is a text value an XMP packet: one of the XMP namespace URIs, or an `xmpmeta` or `RDF` element
/// of any prefix, in any case?
fn looks_like_xmp(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if XMP_URIS.iter().any(|u| lower.contains(u)) {
        return true;
    }
    lower.match_indices('<').any(|(i, _)| {
        let name = element_name(lower.get(i..).unwrap_or_default());
        let local = name.rsplit(':').next().unwrap_or(name);
        local == "xmpmeta" || local == "rdf"
    })
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
    // Every other directory (the chain after IFD0, and the sub-directories of any of them) must be
    // walked and hold no GPS pointer; otherwise the block is left out.
    if !others_clean(&r, ifd0)? {
        return None;
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

/// Walk every directory of the block but IFD0 itself: the chain IFD0 → IFDn (from IFD0's next
/// offset) and the sub-directories (Exif, Interoperability, SubIFDs) of every directory, IFD0
/// included. `Some(true)`: walked, no GPS pointer anywhere but in IFD0. `Some(false)`: a GPS
/// pointer elsewhere. `None`: a directory out of the block, a loop, an unreadable sub-directory
/// pointer, or more than [`MAX_EXIF_DIRECTORIES`] directories.
fn others_clean(r: &Reader<'_>, ifd0: usize) -> Option<bool> {
    let mut seen: Vec<usize> = vec![ifd0];
    // Directories still to walk (IFD0's own entries are judged by the caller).
    let mut todo: Vec<usize> = Vec::new();
    let mut visit = |at: usize, todo: &mut Vec<usize>| -> Option<()> {
        if at == 0 {
            return Some(());
        }
        if seen.contains(&at) || seen.len() >= MAX_EXIF_DIRECTORIES {
            return None;
        }
        seen.push(at);
        todo.push(at);
        Some(())
    };
    // IFD0's sub-directories and its next directory.
    let (next, subs) = directory_links(r, ifd0)?;
    for at in subs {
        visit(at, &mut todo)?;
    }
    visit(next, &mut todo)?;
    while let Some(at) = todo.pop() {
        let count = usize::from(r.u16(at)?);
        if (0..count).any(|i| r.u16(at + 2 + 12 * i) == Some(TAG_GPS_IFD)) {
            return Some(false);
        }
        let (next, subs) = directory_links(r, at)?;
        for sub in subs {
            visit(sub, &mut todo)?;
        }
        visit(next, &mut todo)?;
    }
    Some(true)
}

/// The next-directory offset and the sub-directory offsets of the directory at `at`, every entry
/// inside the block. `None` when the directory, or a sub-directory pointer, cannot be read.
fn directory_links(r: &Reader<'_>, at: usize) -> Option<(usize, Vec<usize>)> {
    let count = usize::from(r.u16(at)?);
    let end = at.checked_add(2 + 12 * count)?.checked_add(4)?;
    if end > r.b.len() {
        return None;
    }
    let mut subs = Vec::new();
    for i in 0..count {
        let e = at + 2 + 12 * i;
        let tag = r.u16(e)?;
        if !TAG_SUB_IFDS.contains(&tag) {
            continue;
        }
        // A pointer is a LONG (4) or an IFD (13), one value or several.
        if !matches!(r.u16(e + 2)?, 4 | 13) {
            return None;
        }
        let n = r.offset(e + 4)?;
        match n {
            0 => {}
            1 => subs.push(r.offset(e + 8)?),
            _ => {
                let values = r.offset(e + 8)?;
                for k in 0..n {
                    subs.push(r.offset(values.checked_add(k.checked_mul(4)?)?)?);
                }
            }
        }
    }
    Some((r.offset(end - 4)?, subs))
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

/// Does the XMP property `name` (`prefix:local`, any prefix, any case) locate the picture?
fn locates(name: &str) -> bool {
    let local = name.split_once(':').map_or(name, |(_, local)| local).to_ascii_lowercase();
    local.starts_with(GPS_MARK) || local.contains(LOCATION_MARK) || XMP_PLACES.iter().any(|p| p.eq_ignore_ascii_case(&local))
}

/// Does a text (a cleaned XMP packet, or any free-form text value) still name a GPS, a location,
/// or a place property under any prefix, in any case?
fn names_location(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains(GPS_MARK) || lower.contains(LOCATION_MARK) {
        return true;
    }
    XMP_PLACES.iter().any(|place| {
        let needle = format!(":{}", place.to_ascii_lowercase());
        lower.match_indices(needle.as_str()).any(|(i, _)| {
            lower.as_bytes().get(i + needle.len()).is_none_or(|&c| c.is_ascii_whitespace() || matches!(c, b'=' | b'>' | b'/'))
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
