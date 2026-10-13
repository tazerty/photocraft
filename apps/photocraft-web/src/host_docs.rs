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

use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::ThemeKind;
use serde_json::json;

use crate::ehp::{Bridge, HostEvent};

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

/// The web `write` service while the bridge runs: a host document's save becomes `evora:save`;
/// anything else is refused (no download).
pub fn write(bridge: &Bridge, path: &str, bytes: &[u8]) -> Result<(), String> {
    let Some((doc_ref, file)) = host_target(path) else {
        log::warn!("photocraft-web: write outside the Evora document refused: {path}");
        return Err("Only the document opened from Evora can be saved here, with File > Save".into());
    };
    let request = bridge.take_expected_save(doc_ref);
    match bridge.save(doc_ref, file, bytes, request.as_deref()) {
        Ok(()) => Ok(()),
        Err(code) => {
            if let Some(request) = request {
                bridge.ack(&request, false, Some(code));
            }
            Err(format!("Evora could not take the document ({code})"))
        }
    }
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
}

impl HostDocs {
    pub fn new(bridge: Bridge) -> Self {
        HostDocs { bridge, opened: Vec::new(), reported: Vec::new() }
    }

    /// At the start of a frame: what the host asked.
    pub fn apply(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        for event in self.bridge.take_events() {
            match event {
                HostEvent::Open { id, doc_ref, name, bytes } => self.open(app, &id, doc_ref, &name, &bytes),
                HostEvent::RequestSave { id, doc_ref } => self.request_save(app, ctx, &id, doc_ref),
                HostEvent::Theme { id, dark } => {
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
            }
        }
    }

    /// At the end of a frame: `evora:dirty` for each host document whose state changed.
    pub fn report(&mut self, app: &PhotocraftApp) {
        let states: Vec<(String, bool)> = self
            .opened
            .iter()
            .filter_map(|r| index_of(app, r).and_then(|i| app.session.documents().get(i)).map(|d| (r.clone(), d.is_dirty())))
            .collect();
        for (doc_ref, dirty) in states {
            match self.reported.iter().position(|(r, _)| *r == doc_ref) {
                Some(i) if self.reported.get(i).is_some_and(|(_, last)| *last == dirty) => {}
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
    }

    fn open(&mut self, app: &mut PhotocraftApp, id: &str, doc_ref: String, name: &str, bytes: &[u8]) {
        if index_of(app, &doc_ref).is_some() {
            // The same docRef twice: a single document per reference.
            self.bridge.ack(id, false, Some("busy"));
            return;
        }
        let before: Vec<photocraft_doc::DocId> = app.session.documents().iter().map(|d| d.doc.id).collect();
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
        app.session.set_active(*index);
        if let Some(st) = app.session.active_mut() {
            st.path = Some(host_path(&doc_ref, name));
        }
        self.opened.push(doc_ref);
        self.bridge.ack(id, true, None);
    }

    fn request_save(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, doc_ref: Option<String>) {
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
        let previous = app.session.active_index();
        app.session.set_active(index);
        self.bridge.expect_save(&doc_ref, id);
        let result = photocraft_ui_egui::menus::invoke(app, ctx, "file.save", json!({}));
        if let Some(previous) = previous {
            app.session.set_active(previous);
        }
        // The writer never ran: the save stopped before it (an open prompt, an encoding error).
        if self.bridge.cancel_expected_save(id) {
            if let Err(e) = result {
                log::warn!("photocraft-web: save requested by the host failed: {e}");
            }
            self.bridge.ack(id, false, Some("busy"));
        }
    }
}

/// Index of the open document whose working path carries `doc_ref`.
fn index_of(app: &PhotocraftApp, doc_ref: &str) -> Option<usize> {
    app.session.documents().iter().position(|d| d.path.as_deref().and_then(host_target).is_some_and(|(r, _)| r == doc_ref))
}
