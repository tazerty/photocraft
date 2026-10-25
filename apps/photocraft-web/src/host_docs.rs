//! Evora build (patch E4): the documents Evora opens in the engine, and their saves.
//!
//! A document the host opens (`evora:open`, bytes) gets a working path that carries its `docRef`:
//! `evora/<docRef>/<name>.pcraft`. File › Save on it (a shortcut, the menu, or the host's
//! `evora:request-save`) then writes the layered `.pcraft` working format through the web `write`
//! service, which hands the bytes to the host as `evora:save` instead of a browser download. No other
//! write leaves the engine while the bridge runs: a document that is not the host's (File › New, a
//! local file) and every export are refused with a message, as nothing can receive them yet
//! (`export.drive` is not implemented), and the engine never starts a download.
//!
//! Without `doc.saved-ack`, the engine counts a document saved once handed over; the host keeps its
//! own save state (HOST-PROTOCOL §6.3). Save As keeps the host's document (no new `docRef` can be
//! adopted without `evora:saved`), so it is refused like any write outside the host's documents.
//!
//! Patch LUM-04, when `doc.saved-ack` is effective (HOST-PROTOCOL §4.3, §4.5): a document is saved
//! only once the host answers `evora:saved {ok: true}`. The engine marks it saved as it hands the
//! bytes over; at the end of that frame the save is bound to its document with the revision it
//! wrote (its token), and the document goes back to its state before the save (still modified),
//! so no `evora:dirty {dirty: false}` leaves while the save is in flight. `ok: true` then marks it
//! saved at that revision, which leaves it modified when it changed since (§4.5); `ok: false`
//! keeps it, or makes it, modified. The state is reported again after each answer.
//! `evora:open` of a `docRef` already open replaces its content with the host's version (the
//! engine's changes are dropped, without a prompt), as the host's « reload the server version »
//! does after a conflict (§6.2).
//!
//! Patch LUM-03: no write is refused any more, and none becomes a download (`write`). A `.pcraft`
//! file written outside the host's documents (Save As, the save of the engine's own document)
//! becomes a new document of the host: `evora:save {saveAs: true}`, then, on `evora:saved
//! {ok: true, docRef}`, the engine document adopts that `docRef` (§7.3); refused, it is again
//! what it was. The engine's own documents get a `.pcraft` working file named after them, so
//! File › Save makes them new documents of the host. Every other file (exports, presets, shortcut
//! profiles) leaves as `evora:export`, one message per file (§7.4).
//!
//! Patch LUM-12: `evora:open {docRef, name, new: true}` opens the engine's New Document dialog,
//! named after the host's document; the document it makes is the host's `docRef` (acknowledged
//! then, `dirty: false` next, §7.1); cancelling the dialog answers `cancelled`.
//!
//! Patch LUM-11: `evora:font` registers the file in the engine's font database and lists its
//! families in the font menus; the type layers set in that family, or falling back to it, are
//! drawn again (`served_fonts::inject`). A file the engine cannot read as a font registers
//! nothing and is refused with `invalid_content`.
//!
//! Patch LUM-06 r2: `evora:theme` brings the host's accent with light or dark, both applied to the
//! Evora themes at once. `diag.stats` also reports the theme the interface is drawn with (`theme`)
//! and what `evora:init` set (`prefs`, `host_prefs`).
//!
//! Patch LUM-02 r2 (`doc.multi`, Evora's HOST-PROTOCOL §7.10, A-07): every host document keeps its
//! own tab, followed by the engine's document id (`tabs`), not by its working path, which a Save As
//! in flight changes for a while. `evora:open` of another `docRef` adds a tab (the same `docRef`
//! still replaces its content); an adopted `docRef` (§7.3) takes over the tab of the document it
//! came from, which is not a close. At the end of a frame, with `doc.multi` effective:
//! `evora:closed {docRef}` for each host document whose tab closed (a close that saves first sends
//! its `evora:save` before), then `evora:active {docRef}` when the tab in front is another host
//! document than the last one reported (nothing while the engine's own document is in front: the
//! host only hears of documents it opened). `evora:activate {docRef}` brings an open host document
//! to the front (`not_found` when it is not open, `busy` while a dialog of the engine is open).
//! `evora:request-save` without `docRef` saves every modified host document, oldest first: the
//! first save answers the request (`re`), the others leave as saves of the engine's own (no `re`,
//! one answer per request, §4.5); none modified answers `clean`. `diag.stats` reports the host
//! document in front (`active`).

use std::sync::Arc;

use photocraft_doc::DocId;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::ThemeKind;
use serde_json::json;

use crate::ehp::{Bridge, HostEvent};

/// Smoke build only (scenario pont-politique, patch LUM-14 r3): the content of the global block the
/// engine does not read, the author (and text) of the note, and the `layerTime` of the `shmd`
/// blocks that `evora:smoke-placed` and `evora:smoke-shmd` add (values of the bench, read back in
/// the exported bytes by `evora/ci/smoke_ehp.mjs`).
#[cfg(feature = "evora-smoke")]
const SMOKE_GLOBAL: &[u8] = b"EvoraGeoGlobal\0\0";
#[cfg(feature = "evora-smoke")]
const SMOKE_NOTE_AUTHOR: &str = "EvoraNoteAuthor";
#[cfg(feature = "evora-smoke")]
const SMOKE_LAYER_TIME: f64 = 1_700_000_000.0;

/// A save handed to the host and waiting for `evora:saved` (patch LUM-04).
struct Pending {
    /// The `evora:save` id the answer carries in `re`.
    id: String,
    /// The document saved (an id, not an index: tabs move and close meanwhile).
    doc: DocId,
    /// The revision the bytes were written from.
    token: u64,
    /// A new document of the host (`saveAs`, patch LUM-03): what to adopt or restore.
    adopt: Option<Adopt>,
}

/// A new document of the host in flight (patch LUM-03).
struct Adopt {
    /// The file written, named again under the new `docRef` once adopted.
    file: String,
    /// The engine document's working path and name before the save, restored if refused.
    path: Option<String>,
    name: String,
}

/// A blank document the host asked for (patch LUM-12), waiting for its New Document dialog.
struct NewDoc {
    /// The `evora:open` id, answered once the dialog closes.
    id: String,
    doc_ref: String,
    name: String,
    /// The dialog's id in the engine.
    dialog: u64,
    /// The documents open before it, to find the one the dialog makes.
    before: Vec<DocId>,
}

/// An open document at the end of the last frame (patch LUM-04): the state a save made in this
/// frame goes back to until the host answers.
struct Snapshot {
    doc: DocId,
    saved_revision: u64,
    path: Option<String>,
    name: String,
}

/// Where the host's documents live: `evora/<docRef>/<name>.pcraft`.
const HOST_DIR: &str = "evora";

/// The working path of the host's document `doc_ref`, named after `name` (its extension replaced).
fn host_path(doc_ref: &str, name: &str) -> String {
    let stem = std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if stem.trim().is_empty() { "document".to_string() } else { stem };
    format!("{HOST_DIR}/{doc_ref}/{stem}.pcraft")
}

/// `(docRef, file name)` of a host document's working path.
fn host_target(path: &str) -> Option<(&str, &str)> {
    let (doc_ref, file) = path.strip_prefix(HOST_DIR)?.strip_prefix('/')?.split_once('/')?;
    (crate::ehp::is_ulid(doc_ref) && crate::ehp::is_name(file) && file.ends_with(".pcraft")).then_some((doc_ref, file))
}

/// The web `write` service while the bridge runs, never a download (patches E4, LUM-03): a host
/// document's save becomes `evora:save`; another `.pcraft` file a new document of the host
/// (`evora:save {saveAs: true}`); any other file an `evora:export`, one message per file.
pub fn write(bridge: &Bridge, path: &str, bytes: &[u8]) -> Result<(), String> {
    let refused = |code: &str| format!("Evora could not take the document ({code})");
    if let Some((doc_ref, file)) = host_target(path) {
        let request = bridge.take_expected_save(doc_ref);
        return match bridge.save(doc_ref, path, file, bytes, request.as_deref()) {
            Ok(()) => Ok(()),
            Err(code) => {
                if let Some(request) = request {
                    bridge.ack(&request, false, Some(code));
                }
                Err(refused(code))
            }
        };
    }
    let result = if photocraft_engine::file_cmds::extension(path).as_deref() == Some("pcraft") {
        bridge.save_as(path, bytes)
    } else {
        let file = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        bridge.export(&file, bytes)
    };
    result.map_err(refused)
}

/// The working file of an engine document of its own (patch LUM-03): `<name>.pcraft`.
fn local_path(name: &str) -> String {
    let stem = std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if stem.trim().is_empty() { "document".to_string() } else { stem };
    format!("{stem}.pcraft")
}

/// The engine language for the host's `engineLang` (its E3 mapping): English when the host has
/// none or the engine has no catalog for it.
pub fn engine_language(engine_lang: Option<&str>) -> &'static str {
    crate::web::host_language(engine_lang).unwrap_or("en")
}

pub fn theme_kind(dark: bool) -> ThemeKind {
    if dark { ThemeKind::Evora } else { ThemeKind::EvoraLight }
}

/// The host's documents in the running app.
pub struct HostDocs {
    bridge: Bridge,
    /// The `docRef`s opened by the host, oldest first.
    opened: Vec<String>,
    /// The dirty state last reported for each of them.
    reported: Vec<(String, bool)>,
    /// The renderer the engine started with (`diag.stats`, patch LUM-10).
    renderer: String,
    /// Saves waiting for `evora:saved` (patch LUM-04).
    pending: Vec<Pending>,
    /// The open documents at the end of the last frame (patches LUM-04, LUM-03).
    saved_before: Vec<Snapshot>,
    /// `docRef`s whose state is reported again at the end of this frame, changed or not.
    forced: Vec<String>,
    /// The blank document the host asked for, its dialog open (patch LUM-12).
    new_doc: Option<NewDoc>,
    /// The families the host's fonts registered, in order (patch LUM-11, `diag.stats`).
    fonts: Vec<String>,
    /// What `evora:init` set (patches LUM-14 r2 and LUM-15, `diag.stats`).
    prefs: crate::host_prefs::HostPrefs,
    /// The tab of each open host document: its `docRef` and the engine's document id (patch
    /// LUM-02 r2).
    tabs: Vec<(String, DocId)>,
    /// The host document last reported in front (`evora:active`, patch LUM-02 r2).
    active_reported: Option<String>,
}

impl HostDocs {
    pub fn new(bridge: Bridge, renderer: String, prefs: crate::host_prefs::HostPrefs) -> Self {
        HostDocs {
            bridge,
            opened: Vec::new(),
            reported: Vec::new(),
            renderer,
            pending: Vec::new(),
            saved_before: Vec::new(),
            forced: Vec::new(),
            new_doc: None,
            fonts: Vec::new(),
            prefs,
            tabs: Vec::new(),
            active_reported: None,
        }
    }

    /// At the start of a frame: what the host asked.
    pub fn apply(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        // A save is always bound before its answer is read, even after a frame without `ui`.
        self.bind_saves(app);
        self.share_refs(app);
        for event in self.bridge.take_events() {
            match event {
                HostEvent::Open { id, doc_ref, name, bytes } => self.open(app, &id, doc_ref, &name, &bytes),
                HostEvent::OpenNew { id, doc_ref, name } => self.open_new(app, ctx, &id, doc_ref, name),
                HostEvent::RequestSave { id, doc_ref } => self.request_save(app, ctx, &id, doc_ref),
                HostEvent::Activate { id, doc_ref } => self.activate(app, &id, &doc_ref),
                HostEvent::Theme { id, dark, accent } => {
                    // Patch LUM-06 r2: the accent first, so the theme applied takes it.
                    photocraft_ui_egui::theme::set_host_accent(crate::host_prefs::accent(&accent));
                    app.set_theme(ctx, theme_kind(dark));
                    self.bridge.ack(&id, true, None);
                }
                HostEvent::Lang { id, engine_lang } => {
                    let code = engine_language(engine_lang.as_deref());
                    match app.run("prefs.set", json!({"values": {"interface.language": code}})) {
                        Ok(_) => self.bridge.ack(&id, true, None),
                        Err(e) => {
                            log::warn!("photocraft-web: host language {code} not applied: {e}");
                            self.bridge.ack(&id, false, Some("unsupported"));
                        }
                    }
                }
                HostEvent::Command { id, command } => self.command(app, ctx, &id, &command),
                HostEvent::Saved { re, ok, doc_ref } => self.saved(app, &re, ok, doc_ref),
                HostEvent::Font { id, family, bytes } => self.font(app, &id, &family, bytes),
                #[cfg(feature = "evora-smoke")]
                HostEvent::SmokeExport { id, ext } => self.smoke_export(app, &id, &ext),
                #[cfg(feature = "evora-smoke")]
                HostEvent::SmokeLayerXmp { id, xmp } => self.smoke_layer_xmp(app, &id, &xmp),
                #[cfg(feature = "evora-smoke")]
                HostEvent::SmokePlaced { id, files } => self.smoke_placed(app, &id, files),
                #[cfg(feature = "evora-smoke")]
                HostEvent::SmokeShmd { id, packets } => self.smoke_shmd(app, &id, &packets),
            }
        }
    }

    /// Smoke build only (scenario pont-politique, patch LUM-14 r2): the active document encoded as
    /// `ext` by the export service (the host's location policy applied), every metadata kept by the
    /// settings (the defaults, Metadata « All »), then written like any export (`evora:export`).
    #[cfg(feature = "evora-smoke")]
    fn smoke_export(&mut self, app: &mut PhotocraftApp, id: &str, ext: &str) {
        let Some(doc) = app.session.active().map(|st| st.doc.clone()) else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        let path = format!("evora-politique.{ext}");
        let settings = photocraft_ui_egui::ExportSettings::default();
        let encoded = match app.services.export.as_ref() {
            Some(export) => export(&doc, &path, &settings),
            None => Err("no exporter configured".to_owned()),
        };
        match encoded.and_then(|(bytes, _)| write(&self.bridge, &path, &bytes)) {
            Ok(()) => self.bridge.ack(id, true, None),
            Err(e) => {
                log::warn!("photocraft-web: smoke export {ext} failed: {e}");
                self.bridge.ack(id, false, Some("busy"));
            }
        }
    }

    /// Smoke build only (scenario pont-politique, patch LUM-14 r2): two copies of the active
    /// document's bottom layer above it, one with `xmp` as its own packet (`Layer::xmp`), the other
    /// with `xmp` in a raw `shmd` block, as a document from an earlier version keeps it.
    #[cfg(feature = "evora-smoke")]
    fn smoke_layer_xmp(&mut self, app: &mut PhotocraftApp, id: &str, xmp: &str) {
        let Ok(block) = photocraft_psd::metadata::put_layer_xmp(None, xmp) else {
            self.bridge.ack(id, false, Some("invalid_content"));
            return;
        };
        let Some(st) = app.session.active_mut() else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        let doc = Arc::make_mut(&mut st.doc);
        let Some(bottom) = doc.layers.first() else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        let mut own = bottom.duplicate();
        own.xmp = Some(Arc::from(xmp));
        let mut raw = bottom.duplicate();
        raw.xmp = None;
        raw.psd_blocks.retain(|(key, _)| key != b"shmd");
        raw.psd_blocks.push((*b"shmd", Arc::new(block)));
        doc.layers.push(own);
        doc.layers.push(raw);
        self.bridge.ack(id, true, None);
    }

    /// Smoke build only (scenario pont-politique, patch LUM-14 r3): what an export must clean
    /// besides the metadata, added to the active document. Above its layers, copies of its bottom
    /// layer made smart objects (the bottom layer's pixels as their rendered appearance) placing:
    /// the PNG `png`, the JPEG `jpeg`, the file `other` (of no kind the export can clean), a
    /// `.pcraft` document (the bottom layer, with `nested_xmp` as its XMP and the active document's
    /// EXIF), and the PNG `linked` as an imported PSD keeps it (a document holding that smart
    /// object, written as a PSD and opened again: the smart object names the file, kept in a global
    /// `lnk2` block, which the active document gets too); patch LUM-14 r4: the photos `turned_jpeg`
    /// and `turned_png`, stored on their side, that their EXIF turns upright. Then a global block
    /// the engine does not read (`evGP`, [`SMOKE_GLOBAL`]) and a note by [`SMOKE_NOTE_AUTHOR`].
    #[cfg(feature = "evora-smoke")]
    fn smoke_placed(&mut self, app: &mut PhotocraftApp, id: &str, files: crate::ehp::SmokeFiles) {
        use photocraft_doc::{Affine, LayerContent, Note, SmartObject, SmartSource};
        let Some(st) = app.session.active_mut() else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        let Some(bottom) = st.doc.layers.first().cloned() else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        let LayerContent::Raster(pixels) = &bottom.content else {
            self.bridge.ack(id, false, Some("invalid_content"));
            return;
        };
        let smart = |name: &str, file_name: &str, bytes: Vec<u8>| {
            let mut layer = bottom.duplicate();
            layer.name = name.to_owned();
            layer.xmp = None;
            layer.psd_blocks.clear();
            let source = SmartSource::Embedded { file_name: file_name.to_owned(), bytes: Arc::new(bytes) };
            layer.content = LayerContent::Smart(SmartObject::new(source, Affine::IDENTITY, Some(pixels.clone())));
            layer
        };
        let mut nested = (*st.doc).clone();
        nested.layers = vec![bottom.duplicate()];
        nested.metadata.xmp = Some(files.nested_xmp);
        let nested = photocraft_io::export(&nested, "pcraft", &photocraft_io::ExportOptions::default());
        let mut psd = (*st.doc).clone();
        psd.layers = vec![smart("evora-place-linked", "evora-place-linked.png", files.linked)];
        psd.metadata = photocraft_doc::Metadata::default();
        let reopened = photocraft_io::export(&psd, "psd", &photocraft_io::ExportOptions::default())
            .and_then(|r| photocraft_io::import("evora-place-linked.psd", &r.bytes));
        let (Ok(nested), Ok(reopened)) = (nested, reopened) else {
            self.bridge.ack(id, false, Some("busy"));
            return;
        };
        let reopened = reopened.document;
        let Some(linked) = reopened.layers.iter().find(|l| matches!(l.content, LayerContent::Smart(_))) else {
            self.bridge.ack(id, false, Some("invalid_content"));
            return;
        };
        let mut layers = vec![
            smart("evora-place-png", "evora-place.png", files.png),
            smart("evora-place-jpeg", "evora-place.jpg", files.jpeg),
            smart("evora-place-tourne-jpeg", "evora-tourne.jpg", files.turned_jpeg),
            smart("evora-place-tourne-png", "evora-tourne.png", files.turned_png),
            smart("evora-place-other", "evora-place.bin", files.other),
            smart("evora-place-pcraft", "evora-place.pcraft", nested.bytes),
            linked.duplicate(),
        ];
        let doc = Arc::make_mut(&mut st.doc);
        doc.layers.append(&mut layers);
        let link_blocks = reopened.metadata.psd_global_blocks.iter().filter(|(_, key, _)| matches!(key, b"lnk2" | b"lnk3" | b"lnkD" | b"lnkE"));
        doc.metadata.psd_global_blocks.extend(link_blocks.cloned());
        doc.metadata.psd_global_blocks.push((*b"8BIM", *b"evGP", Arc::new(SMOKE_GLOBAL.to_vec())));
        doc.notes.push(Note { author: SMOKE_NOTE_AUTHOR.to_owned(), text: SMOKE_NOTE_AUTHOR.to_owned(), ..Note::default() });
        self.bridge.ack(id, true, None);
    }

    /// Smoke build only (scenario pont-politique, patch LUM-14 r3): copies of the active document's
    /// bottom layer above it, each with one `shmd` block holding more than an export keeps: a
    /// second `cust` item with a located packet (`second`); two packets in one `cust` item (`clean`
    /// then `twice`); a packet that is not text (`raw`, as raw data); an item that is not `cust`
    /// (`item`, its data); and the block an export rebuilds, `layerTime` with a located packet
    /// (`witness`).
    #[cfg(feature = "evora-smoke")]
    fn smoke_shmd(&mut self, app: &mut PhotocraftApp, id: &str, p: &crate::ehp::SmokePackets) {
        use photocraft_psd::descriptor::{Descriptor, Id, UnicodeString, Value, VersionedDescriptor};
        use photocraft_psd::metadata::{MetadataItem, write_shmd};
        let cust = |values: Vec<(&str, Value)>| {
            let items = values.into_iter().map(|(key, value)| (Id::new(key), value)).collect();
            let d = Descriptor { name: UnicodeString::new_nul(""), class_id: Id::new("metadata"), items };
            MetadataItem::new(*b"cust", VersionedDescriptor::new(d).to_bytes())
        };
        let text = |xmp: &str| Value::Text(UnicodeString::new_nul(xmp));
        let time = || Value::Double(SMOKE_LAYER_TIME);
        let blocks = [
            ("evora-shmd-deux-cust", write_shmd(&[cust(vec![("layerTime", time()), ("layerXMP", text(&p.clean))]), cust(vec![("layerXMP", text(&p.second))])])),
            ("evora-shmd-deux-paquets", write_shmd(&[cust(vec![("layerXMP", text(&p.clean)), ("layerXMP", text(&p.twice))])])),
            ("evora-shmd-brut", write_shmd(&[cust(vec![("layerXMP", Value::RawData(p.raw.clone().into_bytes()))])])),
            (
                "evora-shmd-element",
                write_shmd(&[cust(vec![("layerTime", time()), ("layerXMP", text(&p.clean))]), MetadataItem::new(*b"tmln", p.item.clone().into_bytes())]),
            ),
            ("evora-shmd-temoin", write_shmd(&[cust(vec![("layerTime", time()), ("layerXMP", text(&p.witness))])])),
        ];
        let Some(st) = app.session.active_mut() else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        let doc = Arc::make_mut(&mut st.doc);
        let Some(bottom) = doc.layers.first().cloned() else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        for (name, block) in blocks {
            let mut layer = bottom.duplicate();
            layer.name = name.to_owned();
            layer.xmp = None;
            layer.psd_blocks = vec![(*b"shmd", Arc::new(block))];
            doc.layers.push(layer);
        }
        self.bridge.ack(id, true, None);
    }

    /// `evora:font` (patch LUM-11): the file registers its families, listed in the font menus, or
    /// nothing (`invalid_content`).
    fn font(&mut self, app: &mut PhotocraftApp, id: &str, family: &str, bytes: Vec<u8>) {
        let added = photocraft_ui_egui::served_fonts::inject(app, family, bytes);
        if added.is_empty() {
            log::warn!("photocraft-web: the host's font {family:?} could not be read as a font");
            self.bridge.ack(id, false, Some("invalid_content"));
            return;
        }
        for name in added {
            if !self.fonts.contains(&name) {
                self.fonts.push(name);
            }
        }
        self.bridge.ack(id, true, None);
    }

    /// `evora:saved` for a save in flight (patch LUM-04, HOST-PROTOCOL §4.3 and §4.5). `ok: true`
    /// marks the document saved at the revision written, so it stays modified when it changed
    /// since; `ok: false` keeps it, or makes it, modified. Its state is reported again either way.
    /// A new document of the host (patch LUM-03) adopts the `docRef` of `ok: true`; refused, or
    /// without a `docRef`, it is again what it was before the save, and modified.
    fn saved(&mut self, app: &mut PhotocraftApp, re: &str, ok: bool, new_ref: Option<String>) {
        let Some(at) = self.pending.iter().position(|p| p.id == re) else { return };
        let pending = self.pending.remove(at);
        // A document closed or replaced meanwhile: the answer no longer applies.
        let Some(index) = app.session.documents().iter().position(|d| d.doc.id == pending.doc) else { return };
        let adopted = match (&pending.adopt, ok, new_ref) {
            (Some(adopt), true, Some(new_ref)) => {
                let path = host_path(&new_ref, &adopt.file);
                with_document(app, index, |st| st.path = Some(path));
                Some(new_ref)
            }
            (Some(adopt), _, _) => {
                let (path, name) = (adopt.path.clone(), adopt.name.clone());
                with_document(app, index, |st| {
                    st.path = path;
                    if st.doc.name != name {
                        Arc::make_mut(&mut st.doc).name = name;
                    }
                });
                None
            }
            (None, _, _) => None,
        };
        let ok = ok && (pending.adopt.is_none() || adopted.is_some());
        if let Some(new_ref) = adopted {
            // Patch LUM-02 r2: the tab is the adopted document's now; the document it came from
            // is not closed by it.
            self.bind_tab(&new_ref, pending.doc);
            if !self.opened.contains(&new_ref) {
                self.opened.push(new_ref);
            }
        }
        with_document(app, index, |st| {
            if ok {
                if pending.token > st.saved_revision {
                    st.saved_revision = pending.token;
                }
            } else if !st.is_dirty() {
                // No revision is ever 0: the document reads modified until a save succeeds.
                st.saved_revision = 0;
            }
        });
        if let Some(doc_ref) = self.doc_ref_at(app, index) {
            self.force(&doc_ref);
        }
    }

    /// The host's `docRef` of the document at `index`, if it is one of the host's.
    fn doc_ref_at(&self, app: &PhotocraftApp, index: usize) -> Option<String> {
        self.opened.iter().find(|r| index_of(app, r) == Some(index)).cloned()
    }

    /// Report the state of `doc_ref` again at the end of this frame.
    fn force(&mut self, doc_ref: &str) {
        if !self.forced.iter().any(|r| r == doc_ref) {
            self.forced.push(doc_ref.to_owned());
        }
    }

    /// `evora:command` of the compiled list (patch LUM-10): `diag.*` answered with their result,
    /// an engine command run as its menu item runs it.
    fn command(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, command: &str) {
        match command {
            "diag.stats" => {
                let host: Vec<(String, usize)> = self.opened.iter().filter_map(|r| index_of(app, r).map(|i| (r.clone(), i))).collect();
                let mut stats = crate::host_commands::stats(app, &host, &self.renderer, &self.fonts);
                // Patch LUM-06 r2: the theme drawn; patches LUM-14 r2 and LUM-15: what `evora:init` set.
                stats["theme"] = crate::host_prefs::theme_report(ctx);
                stats["prefs"] = self.prefs.report(app, ctx);
                // Patch LUM-02 r2: the host document in front (null: none of the host's).
                stats["active"] = json!(self.front(app));
                self.bridge.ack_result(id, &stats);
            }
            "diag.menus" => self.bridge.ack_result(id, &crate::host_commands::menus(app)),
            _ => match crate::host_commands::run_engine(app, ctx, command) {
                Ok(()) => self.bridge.ack(id, true, None),
                Err(code) => self.bridge.ack(id, false, Some(code)),
            },
        }
    }

    /// At the end of a frame: `evora:dirty` for each host document whose state changed.
    /// Also, first, the saves made in this frame are bound to their documents (patch LUM-04);
    /// no `dirty: false` leaves for a document whose save is in flight; a state the host must hear
    /// again (after `evora:saved`, after an open) is reported even when unchanged.
    pub fn report(&mut self, app: &mut PhotocraftApp) {
        self.bind_saves(app);
        self.settle_new_doc(app);
        let states: Vec<(String, DocId, bool)> = self
            .opened
            .iter()
            .filter_map(|r| index_of(app, r).and_then(|i| app.session.documents().get(i)).map(|d| (r.clone(), d.doc.id, d.is_dirty())))
            .collect();
        for (doc_ref, doc, dirty) in states {
            if !dirty && self.pending.iter().any(|p| p.doc == doc) {
                continue;
            }
            let forced = self.forced.iter().any(|r| *r == doc_ref);
            match self.reported.iter().position(|(r, _)| *r == doc_ref) {
                Some(i) if !forced && self.reported.get(i).is_some_and(|(_, last)| *last == dirty) => {}
                Some(i) => {
                    if let Some(entry) = self.reported.get_mut(i) {
                        entry.1 = dirty;
                    }
                    self.bridge.dirty(&doc_ref, dirty);
                }
                None => {
                    self.reported.push((doc_ref.clone(), dirty));
                    self.bridge.dirty(&doc_ref, dirty);
                }
            }
        }
        self.forced.clear();
        self.report_tabs(app);
        self.name_local_documents(app);
        self.share_refs(app);
        self.saved_before = app
            .session
            .documents()
            .iter()
            .map(|d| Snapshot { doc: d.doc.id, saved_revision: d.saved_revision, path: d.path.clone(), name: d.doc.name.clone() })
            .collect();
    }

    /// The engine's own documents (File › New, a local file) get their `.pcraft` working file
    /// (patch LUM-03), so File › Save makes them new documents of the host instead of a flat
    /// export. Edit Contents documents keep saving into their smart object. Needs
    /// `doc.saved-ack`, without which no new document can be adopted.
    fn name_local_documents(&mut self, app: &mut PhotocraftApp) {
        if !self.bridge.has("doc.saved-ack") {
            return;
        }
        let unnamed: Vec<(usize, String)> = app
            .session
            .documents()
            .iter()
            .enumerate()
            .filter(|(_, d)| d.path.is_none() && !app.session.smart_links.iter().any(|l| l.child == d.doc.id))
            .map(|(i, d)| (i, local_path(&d.doc.name)))
            .collect();
        for (i, path) in unnamed {
            with_document(app, i, |st| st.path = Some(path));
        }
    }

    /// Tell the bridge which host document is active (an export's `docRef`) and which one the host
    /// opened last (the origin of a new document), patch LUM-03.
    fn share_refs(&self, app: &PhotocraftApp) {
        let active = app.session.active_index().and_then(|i| self.doc_ref_at(app, i));
        let last = self.opened.iter().rev().find(|r| index_of(app, r).is_some()).cloned();
        self.bridge.set_refs(active, last);
    }

    /// Bind each save handed to the host in this frame to its document (patch LUM-04): the save
    /// records the revision it wrote, and the document goes back to its saved revision of the last
    /// frame, so it reads modified until the host answers. Without `doc.saved-ack` effective, the
    /// engine keeps counting a document saved once handed over (HOST-PROTOCOL §6.3).
    fn bind_saves(&mut self, app: &mut PhotocraftApp) {
        let sent = self.bridge.take_sent();
        if !self.bridge.has("doc.saved-ack") {
            return;
        }
        for sent in sent {
            // A new document (patch LUM-03): the engine document written at that path (Save As,
            // or the save of the engine's own document), the active one first. None: a copy.
            let index = if sent.save_as {
                let active = app.session.active_index();
                let written: Vec<usize> =
                    app.session.documents().iter().enumerate().filter(|(_, d)| d.path.as_deref() == Some(sent.path.as_str())).map(|(i, _)| i).collect();
                written.iter().copied().find(|i| Some(*i) == active).or_else(|| written.first().copied())
            } else {
                index_of(app, &sent.doc_ref)
            };
            let Some(index) = index else { continue };
            let Some((doc, token, name)) = app.session.documents().get(index).map(|d| (d.doc.id, d.saved_revision, d.doc.name.clone())) else { continue };
            let before = self.saved_before.iter().find(|s| s.doc == doc);
            let file = std::path::Path::new(&sent.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let adopt = sent.save_as.then(|| match before {
                Some(s) => Adopt { file: file.clone(), path: s.path.clone(), name: s.name.clone() },
                None => Adopt { file: file.clone(), path: None, name },
            });
            if let Some(before) = before.map(|s| s.saved_revision)
                && before != token
            {
                with_document(app, index, |st| st.saved_revision = before);
            }
            self.pending.push(Pending { id: sent.id, doc, token, adopt });
        }
    }

    fn open(&mut self, app: &mut PhotocraftApp, id: &str, doc_ref: String, name: &str, bytes: &[u8]) {
        // The same docRef again: the host's version replaces the engine's (patch LUM-04).
        let replaced = index_of(app, &doc_ref);
        let before: Vec<DocId> = app.session.documents().iter().map(|d| d.doc.id).collect();
        if let Err(e) = app.open_bytes(name, bytes) {
            log::warn!("photocraft-web: the host's document could not be opened: {e}");
            self.bridge.ack(id, false, Some("invalid_content"));
            return;
        }
        let opened: Vec<usize> = app.session.documents().iter().enumerate().filter(|(_, d)| !before.contains(&d.doc.id)).map(|(i, _)| i).collect();
        let [index] = opened.as_slice() else {
            // Brushes, gradients and swatches go to the preset libraries: no document.
            self.bridge.ack(id, false, Some("unsupported"));
            return;
        };
        let mut index = *index;
        if let Some(old) = replaced {
            let Some(index_now) = self.replace(app, old, index) else {
                self.bridge.ack(id, false, Some("busy"));
                return;
            };
            index = index_now;
        }
        app.session.set_active(index);
        if let Some(st) = app.session.active_mut() {
            st.path = Some(host_path(&doc_ref, name));
        }
        // Patch LUM-02 r2: its tab (a reload keeps the docRef on the new tab).
        if let Some(doc) = app.session.documents().get(index).map(|d| d.doc.id) {
            self.bind_tab(&doc_ref, doc);
        }
        if !self.opened.contains(&doc_ref) {
            self.opened.push(doc_ref.clone());
        }
        // Its state follows the acknowledgement: `dirty: false` (HOST-PROTOCOL §7.1, step 8).
        self.force(&doc_ref);
        self.bridge.ack(id, true, None);
    }

    /// `evora:open {docRef, name, new: true}` (patch LUM-12): the engine's New Document dialog
    /// opens, named after the host's document; `settle_new_doc` answers once it closes. `busy`
    /// while another is open or when the `docRef` is already open.
    fn open_new(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, doc_ref: String, name: String) {
        if self.new_doc.is_some() || index_of(app, &doc_ref).is_some() {
            self.bridge.ack(id, false, Some("busy"));
            return;
        }
        let before: Vec<DocId> = app.session.documents().iter().map(|d| d.doc.id).collect();
        let dialog = match photocraft_ui_egui::menus::invoke(app, ctx, "file.new", json!({})) {
            Ok(opened) => opened.get("dialog").and_then(serde_json::Value::as_u64),
            Err(e) => {
                log::warn!("photocraft-web: the New Document dialog did not open: {e}");
                None
            }
        };
        let Some(dialog) = dialog else {
            self.bridge.ack(id, false, Some("busy"));
            return;
        };
        let stem = std::path::Path::new(&name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if !stem.trim().is_empty()
            && let Some(d) = app.ui.dialog_mut(dialog)
        {
            d.fields.insert("name".into(), json!(stem));
        }
        self.new_doc = Some(NewDoc { id: id.to_owned(), doc_ref, name, dialog, before });
    }

    /// Once the New Document dialog of `evora:open {new: true}` closed (patch LUM-12): the document
    /// it made becomes the host's `docRef` (`ok`, then `dirty: false`), or none was made
    /// (`cancelled`).
    fn settle_new_doc(&mut self, app: &mut PhotocraftApp) {
        let Some(new_doc) = self.new_doc.take() else { return };
        if app.ui.dialogs.iter().any(|d| d.id == new_doc.dialog) {
            self.new_doc = Some(new_doc);
            return;
        }
        let active = app.session.active_index();
        let made: Vec<usize> =
            app.session.documents().iter().enumerate().filter(|(_, d)| !new_doc.before.contains(&d.doc.id)).map(|(i, _)| i).collect();
        let Some(index) = made.iter().copied().find(|i| Some(*i) == active).or_else(|| made.first().copied()) else {
            self.bridge.ack(&new_doc.id, false, Some("cancelled"));
            return;
        };
        let path = host_path(&new_doc.doc_ref, &new_doc.name);
        with_document(app, index, |st| st.path = Some(path));
        if let Some(doc) = app.session.documents().get(index).map(|d| d.doc.id) {
            self.bind_tab(&new_doc.doc_ref, doc);
        }
        if !self.opened.contains(&new_doc.doc_ref) {
            self.opened.push(new_doc.doc_ref.clone());
        }
        self.force(&new_doc.doc_ref);
        self.bridge.ack(&new_doc.id, true, None);
    }

    /// The document just opened at `new` takes the place of the one at `old`, which closes without
    /// a prompt: its changes are dropped and its saves in flight no longer apply. Returns the new
    /// document's index; `None` (the new document closed again) when the old one would not close.
    fn replace(&mut self, app: &mut PhotocraftApp, old: usize, new: usize) -> Option<usize> {
        let old_doc = app.session.documents().get(old).map(|d| d.doc.id);
        let new_doc = app.session.documents().get(new).map(|d| d.doc.id)?;
        if let Err(e) = app.run("file.close", json!({"document": old})) {
            log::warn!("photocraft-web: the replaced document could not be closed: {e}");
            let _ = app.run("file.close", json!({"document": new}));
            return None;
        }
        self.pending.retain(|p| Some(p.doc) != old_doc);
        let now = app.session.documents().iter().position(|d| d.doc.id == new_doc)?;
        Some(app.session.move_document(now, old).unwrap_or(now))
    }

    fn request_save(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, doc_ref: Option<String>) {
        // Patch LUM-02 r2: without docRef, every modified host document (`doc.multi`).
        if doc_ref.is_none() && self.bridge.has("doc.multi") {
            self.request_save_all(app, ctx, id);
            return;
        }
        // The named document, or (no docRef) the last one the host opened that is still open.
        let target = match doc_ref {
            Some(r) => index_of(app, &r).map(|i| (i, r)),
            None => self.opened.iter().rev().find_map(|r| index_of(app, r).map(|i| (i, r.clone()))),
        };
        let Some((index, doc_ref)) = target else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        if self.bridge.read_only() {
            self.bridge.ack(id, false, Some("read_only"));
            return;
        }
        if !app.session.documents().get(index).is_some_and(|d| d.is_dirty()) {
            self.bridge.ack(id, true, Some("clean"));
            return;
        }
        // File › Save, as the user would: it writes the .pcraft working format to the host path,
        // which the `write` service turns into `evora:save` answering this request.
        self.bridge.expect_save(&doc_ref, id);
        let result = save_at(app, ctx, index);
        // The writer never ran: the save stopped before it (an open prompt, an encoding error).
        if self.bridge.cancel_expected_save(id) {
            if let Err(e) = result {
                log::warn!("photocraft-web: save requested by the host failed: {e}");
            }
            self.bridge.ack(id, false, Some("busy"));
        }
    }

    /// `evora:request-save` without `docRef`, `doc.multi` effective (patch LUM-02 r2, HOST-PROTOCOL
    /// §4.3): File › Save on every modified host document, oldest first. The first save the writer
    /// sends answers the request; the others leave without `re`, as the user's own saves do (one
    /// answer per request, §4.5). No host document open: `not_found`; none modified: `clean`; no
    /// save sent: `busy`.
    fn request_save_all(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context, id: &str) {
        let open: Vec<String> = self.opened.iter().filter(|r| index_of(app, r).is_some()).cloned().collect();
        if open.is_empty() {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        }
        if self.bridge.read_only() {
            self.bridge.ack(id, false, Some("read_only"));
            return;
        }
        let modified: Vec<String> =
            open.into_iter().filter(|r| index_of(app, r).and_then(|i| app.session.documents().get(i)).is_some_and(|d| d.is_dirty())).collect();
        if modified.is_empty() {
            self.bridge.ack(id, true, Some("clean"));
            return;
        }
        let mut answered = false;
        for doc_ref in modified {
            let Some(index) = index_of(app, &doc_ref) else { continue };
            // SABORDAGE QA (témoin EVORA-LUM-97, S2) : chaque enregistrement répond à la demande.
            let mut answered = false;
            if !answered {
                self.bridge.expect_save(&doc_ref, id);
            }
            let result = save_at(app, ctx, index);
            // The writer took the request: the next saves leave without it.
            let missed = !answered && self.bridge.cancel_expected_save(id);
            if !answered && !missed {
                answered = true;
            }
            if let Err(e) = result {
                log::warn!("photocraft-web: save of every host document: {doc_ref} failed: {e}");
            }
        }
        if !answered {
            self.bridge.ack(id, false, Some("busy"));
        }
    }

    /// `evora:activate {docRef}` (patch LUM-02 r2): the host's document to the front, as a click on
    /// its tab brings it; `not_found` when it is not open, `busy` while a dialog of the engine is
    /// open (a dialog acts on the document it was opened for). `evora:active` follows at the end
    /// of the frame when another host document was in front.
    fn activate(&mut self, app: &mut PhotocraftApp, id: &str, doc_ref: &str) {
        let index = self
            .tabs
            .iter()
            .find(|(r, _)| r == doc_ref)
            .and_then(|(_, doc)| app.session.documents().iter().position(|d| d.doc.id == *doc));
        let Some(index) = index else {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        };
        if self.new_doc.is_some() || !app.ui.dialogs.is_empty() {
            self.bridge.ack(id, false, Some("busy"));
            return;
        }
        if !app.session.set_active(index) {
            self.bridge.ack(id, false, Some("not_found"));
            return;
        }
        self.bridge.ack(id, true, None);
    }

    /// `doc_ref`'s tab is the engine document `doc` (patch LUM-02 r2): an open, a reload of the
    /// same `docRef` (its new tab), or an adoption (the tab changes `docRef`).
    fn bind_tab(&mut self, doc_ref: &str, doc: DocId) {
        self.tabs.retain(|(r, d)| r != doc_ref && *d != doc);
        self.tabs.push((doc_ref.to_owned(), doc));
    }

    /// The host document in front, if the tab in front is one of the host's (patch LUM-02 r2).
    fn front(&self, app: &PhotocraftApp) -> Option<String> {
        let doc = app.session.active().map(|d| d.doc.id)?;
        self.tabs.iter().find(|(_, d)| *d == doc).map(|(r, _)| r.clone())
    }

    /// At the end of a frame (patch LUM-02 r2, `doc.multi`): `evora:closed` for each host document
    /// whose tab closed, then `evora:active` when the tab in front is another host document than
    /// the last one reported. The bridge sends neither while `doc.multi` is not effective.
    fn report_tabs(&mut self, app: &PhotocraftApp) {
        let open: Vec<DocId> = app.session.documents().iter().map(|d| d.doc.id).collect();
        let (kept, gone): (Vec<(String, DocId)>, Vec<(String, DocId)>) = std::mem::take(&mut self.tabs).into_iter().partition(|(_, d)| open.contains(d));
        self.tabs = kept;
        for (doc_ref, _) in gone {
            self.opened.retain(|r| *r != doc_ref);
            self.reported.retain(|(r, _)| *r != doc_ref);
            if self.active_reported.as_deref() == Some(doc_ref.as_str()) {
                self.active_reported = None;
            }
            // SABORDAGE QA (témoin EVORA-LUM-97, S1) : la fermeture d'un onglet n'est jamais signalée.
            continue;
            self.bridge.closed(&doc_ref);
        }
        // SABORDAGE QA (témoin EVORA-LUM-97, S3) : le document devant n'est jamais signalé.
        return;
        if let Some(front) = self.front(app)
            && self.active_reported.as_deref() != Some(front.as_str())
        {
            self.bridge.active(&front);
            self.active_reported = Some(front);
        }
    }
}

/// File › Save on the document at `index`, as the user would, the active document left as it was.
fn save_at(app: &mut PhotocraftApp, ctx: &egui::Context, index: usize) -> Result<(), String> {
    let previous = app.session.active_index();
    app.session.set_active(index);
    let result = photocraft_ui_egui::menus::invoke(app, ctx, "file.save", json!({})).map(|_| ());
    if let Some(previous) = previous {
        app.session.set_active(previous);
    }
    result
}

/// Run `f` on the document at `index`, the active document left as it was.
fn with_document(app: &mut PhotocraftApp, index: usize, f: impl FnOnce(&mut photocraft_engine::DocState)) {
    let previous = app.session.active_index();
    if !app.session.set_active(index) {
        return;
    }
    if let Some(st) = app.session.active_mut() {
        f(st);
    }
    if let Some(previous) = previous {
        app.session.set_active(previous);
    }
}

/// Index of the open document whose working path carries `doc_ref`.
fn index_of(app: &PhotocraftApp, doc_ref: &str) -> Option<usize> {
    app.session.documents().iter().position(|d| d.path.as_deref().and_then(host_target).is_some_and(|(r, _)| r == doc_ref))
}
