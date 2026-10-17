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

use std::sync::Arc;

use photocraft_doc::DocId;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::theme::ThemeKind;
use serde_json::json;

use crate::ehp::{Bridge, HostEvent};

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
}

impl HostDocs {
    pub fn new(bridge: Bridge, renderer: String) -> Self {
        HostDocs { bridge, opened: Vec::new(), reported: Vec::new(), renderer, pending: Vec::new(), saved_before: Vec::new(), forced: Vec::new() }
    }

    /// At the start of a frame: what the host asked.
    pub fn apply(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        // A save is always bound before its answer is read, even after a frame without `ui`.
        self.bind_saves(app);
        self.share_refs(app);
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
                HostEvent::Command { id, command } => self.command(app, ctx, &id, &command),
                HostEvent::Saved { re, ok, doc_ref } => self.saved(app, &re, ok, doc_ref),
            }
        }
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
        if let Some(new_ref) = adopted
            && !self.opened.contains(&new_ref)
        {
            self.opened.push(new_ref);
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
                self.bridge.ack_result(id, &crate::host_commands::stats(app, &host, &self.renderer));
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
        if !self.opened.contains(&doc_ref) {
            self.opened.push(doc_ref.clone());
        }
        // Its state follows the acknowledgement: `dirty: false` (HOST-PROTOCOL §7.1, step 8).
        self.force(&doc_ref);
        self.bridge.ack(id, true, None);
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
