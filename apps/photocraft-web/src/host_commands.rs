//! Evora build (patch LUM-10): the commands the host may run through `evora:command` (capability
//! `command`, Evora's HOST-PROTOCOL §4.3).
//!
//! The list is compiled in. The host intersects it with its own (`engines.json`, `commands`) and
//! refuses the rest before sending; the bridge refuses them again on arrival with
//! `evora:ack {ok: false, code: "capability"}` ([`allowed`] is the only list both sides of the
//! engine read). Every command takes no parameter (`params` = `{}`). Engine commands are named as
//! in the menu catalogue (`menu_catalog.rs`) and run as the menus run them; `diag.*` are answered
//! here, their result in the acknowledgement. No string is shown in the canvas: the host words
//! any message from the acknowledgement's code.
//!
//! Patch LUM-02: File › Place Embedded joins the list, the host's « Insert »: its picker is the
//! host's (`evora:pick {purpose: "import"}`, `host_pick`). Patch LUM-09: `diag.menus` also lists
//! the items the host build hides (`masked`, `host_menus`). Patch LUM-11: `diag.stats` lists the
//! families the host's fonts registered (`fonts`).
//!
//! Patch EVO-CMD (`command.catalog`, decision 3 of Evora: Evora runs every command of the software
//! on request): the catalogue ([`catalog`]) lists every live menu item of this build, the ones the
//! host build hides left out; with the capability effective, `evora:command` runs any of them as
//! its menu item runs it ([`run_catalog`]), a [`DESTRUCTIVE`] one only with `confirmed: true`.

use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::i18n;
use serde_json::{Value, json};
use wasm_bindgen::JsValue;

/// The engine's own menu commands the host may run: File › New (the engine's New Document
/// dialog; the document it makes is the engine's, not one of the host's), File › Export ›
/// Export As (the export dialog), Edit › Keyboard Shortcuts (the shortcut editor) and File ›
/// Place Embedded (the host's picker, then Free Transform; patch LUM-02).
const ENGINE_COMMANDS: [&str; 4] = ["file.new", "file.export.exportAs", "edit.keyboardShortcuts", "file.placeEmbedded"];
/// Diagnostics answered by the bridge: version, renderer, WebAssembly memory, open documents and
/// dialogs (`diag.stats`); the live menu items, the input of patch LUM-09 (`diag.menus`).
const DIAGNOSTICS: [&str; 2] = ["diag.stats", "diag.menus"];

/// Patch EVO-CMD: the commands Edit › Undo cannot take back and that lose work or close documents
/// (closing, reverting, purging the history or the clipboard, deleting or resetting a workspace,
/// resetting every tool). The host confirms them with the user first (D-24 of Evora); the engine
/// refuses them without `confirmed: true` (`forbidden`).
pub const DESTRUCTIVE: [&str; 12] = [
    "file.close",
    "file.closeAll",
    "file.closeOthers",
    "file.revert",
    "edit.purge.all",
    "edit.purge.clipboard",
    "edit.purge.histories",
    "edit.purge.undo",
    "edit.purge.videoCache",
    "window.workspace.deleteWorkspace",
    "window.workspace.resetWorkspace",
    "tool.resetAll",
];

/// FNV-1a 64 (offset basis and prime): the catalogue's fingerprint, an identity of the build's
/// commands, not a secret.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// `evora:commands` (patch EVO-CMD): every live menu item, once, in menu order, without the ones
/// the host build hides; its label and menu path in the UI language, whether it is enabled right
/// now, whether it is destructive. Every command takes no parameter (`params`), as its menu item.
/// `hash`: FNV-1a 64 of each id followed by its destructive flag, the same in every language and
/// state, which the host compares with the catalogue it provisioned.
pub fn catalog(app: &PhotocraftApp) -> Value {
    let items = photocraft_ui_egui::menus::menu_items(app);
    let mut seen: Vec<&str> = Vec::new();
    let mut commands: Vec<Value> = Vec::new();
    let mut hash = FNV_OFFSET;
    for item in &items {
        let id = item.id.as_str();
        if id == "---" || seen.contains(&id) || !photocraft_ui_egui::menus::is_live(id) {
            continue;
        }
        seen.push(id);
        let destructive = DESTRUCTIVE.contains(&id);
        for b in id.bytes().chain(std::iter::once(if destructive { b'!' } else { b'\n' })) {
            hash = (hash ^ u64::from(b)).wrapping_mul(FNV_PRIME);
        }
        let path: Vec<String> = item.path.iter().map(|p| i18n::t(p).to_string()).collect();
        commands.push(json!({"id": id, "label": i18n::t(&item.label), "path": path, "enabled": item.enabled, "destructive": destructive}));
    }
    let count = commands.len();
    json!({
        "lang": i18n::current().code(),
        "count": count,
        "hash": format!("{hash:016x}"),
        "params": {"type": "object", "maxProperties": 0},
        "commands": commands
    })
}

/// Patch EVO-CMD: is `command` in the catalogue (a live menu item the host build shows)?
pub fn in_catalog(app: &PhotocraftApp, command: &str) -> bool {
    photocraft_ui_egui::menus::is_live(command)
        && !photocraft_ui_egui::menus::is_host_hidden(command)
        && photocraft_ui_egui::menus::menu_items(app).iter().any(|i| i.id == command)
}

/// Patch EVO-CMD: run a command of the catalogue as its menu item runs it (its dialog opens in the
/// engine); a destructive one only with `confirmed`. `Err`: the `evora:ack` code.
pub fn run_catalog(app: &mut PhotocraftApp, ctx: &egui::Context, command: &str, confirmed: bool) -> Result<(), &'static str> {
    if DIAGNOSTICS.contains(&command) || !in_catalog(app, command) {
        return Err("capability");
    }
    if DESTRUCTIVE.contains(&command) && !confirmed {
        return Err("forbidden");
    }
    if !photocraft_ui_egui::menus::is_enabled(app, command) {
        return Err("busy");
    }
    photocraft_ui_egui::menus::invoke(app, ctx, command, json!({})).map(|_| ()).map_err(|e| {
        log::warn!("photocraft-web: host command {command} failed: {e}");
        "busy"
    })
}

/// Is `command` on the compiled list?
pub fn allowed(command: &str) -> bool {
    ENGINE_COMMANDS.contains(&command) || DIAGNOSTICS.contains(&command)
}

/// `diag.stats`: what the engine runs and holds. `host` lists the host's documents still open,
/// as (`docRef`, index of the document in the session); `fonts`, the families the host's fonts
/// registered (patch LUM-11).
pub fn stats(app: &PhotocraftApp, host: &[(String, usize)], renderer: &str, fonts: &[String]) -> Value {
    let docs = app.session.documents();
    let open: Vec<Value> = host
        .iter()
        .filter_map(|(doc_ref, i)| docs.get(*i).map(|d| json!({"docRef": doc_ref, "dirty": d.is_dirty(), "revision": d.revision})))
        .collect();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "renderer": renderer,
        "wasmMemoryBytes": wasm_memory_bytes(),
        "documents": docs.len(),
        "dialogs": app.ui.dialogs.len(),
        "open": open,
        "fonts": fonts
    })
}

/// `diag.menus`: the live menu items (implemented by the engine or its interface), each once, in
/// menu order, and those of them enabled right now; `masked`, the items of the whole menu tree
/// that the host build hides (patch LUM-09), none of which is live here.
pub fn menus(app: &PhotocraftApp) -> Value {
    let items = photocraft_ui_egui::menus::menu_items(app);
    let mut live: Vec<&str> = Vec::new();
    let mut enabled: Vec<&str> = Vec::new();
    for item in &items {
        let id = item.id.as_str();
        if id == "---" || live.contains(&id) || !photocraft_ui_egui::menus::is_live(id) {
            continue;
        }
        live.push(id);
        if item.enabled {
            enabled.push(id);
        }
    }
    let unmasked = photocraft_ui_egui::menus::menu_items_unmasked(app);
    let masked: Vec<&str> = crate::host_menus::HIDDEN.iter().copied().filter(|id| unmasked.iter().any(|i| i.id == *id)).collect();
    json!({"live": live, "enabled": enabled, "masked": masked})
}

/// Run an engine command of the list as the menus do (its dialog opens in the engine). `Err`:
/// the `evora:ack` code of the refusal.
pub fn run_engine(app: &mut PhotocraftApp, ctx: &egui::Context, command: &str) -> Result<(), &'static str> {
    if !allowed(command) || DIAGNOSTICS.contains(&command) {
        return Err("capability");
    }
    if matches!(command, "file.export.exportAs" | "file.placeEmbedded") && app.session.active().is_none() {
        return Err("not_found");
    }
    if !photocraft_ui_egui::menus::is_enabled(app, command) {
        return Err("busy");
    }
    photocraft_ui_egui::menus::invoke(app, ctx, command, json!({})).map(|_| ()).map_err(|e| {
        log::warn!("photocraft-web: host command {command} failed: {e}");
        "busy"
    })
}

/// Size of the WebAssembly memory, in bytes (0 when unreadable).
fn wasm_memory_bytes() -> f64 {
    let read = |v: &JsValue, key: &str| js_sys::Reflect::get(v, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
    read(&read(&wasm_bindgen::memory(), "buffer"), "byteLength").as_f64().unwrap_or(0.0)
}
