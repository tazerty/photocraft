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

use photocraft_ui_egui::PhotocraftApp;
use serde_json::{Value, json};
use wasm_bindgen::JsValue;

/// The engine's own menu commands the host may run: File › New (the engine's New Document
/// dialog; the document it makes is the engine's, not one of the host's), File › Export ›
/// Export As (the export dialog) and Edit › Keyboard Shortcuts (the shortcut editor).
const ENGINE_COMMANDS: [&str; 3] = ["file.new", "file.export.exportAs", "edit.keyboardShortcuts"];
/// Diagnostics answered by the bridge: version, renderer, WebAssembly memory, open documents and
/// dialogs (`diag.stats`); the live menu items, the input of patch LUM-09 (`diag.menus`).
const DIAGNOSTICS: [&str; 2] = ["diag.stats", "diag.menus"];

/// Is `command` on the compiled list?
pub fn allowed(command: &str) -> bool {
    ENGINE_COMMANDS.contains(&command) || DIAGNOSTICS.contains(&command)
}

/// `diag.stats`: what the engine runs and holds. `host` lists the host's documents still open,
/// as (`docRef`, index of the document in the session).
pub fn stats(app: &PhotocraftApp, host: &[(String, usize)], renderer: &str) -> Value {
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
        "open": open
    })
}

/// `diag.menus`: the live menu items (implemented by the engine or its interface), each once, in
/// menu order, and those of them enabled right now.
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
    json!({"live": live, "enabled": enabled})
}

/// Run an engine command of the list as the menus do (its dialog opens in the engine). `Err`:
/// the `evora:ack` code of the refusal.
pub fn run_engine(app: &mut PhotocraftApp, ctx: &egui::Context, command: &str) -> Result<(), &'static str> {
    if !allowed(command) || DIAGNOSTICS.contains(&command) {
        return Err("capability");
    }
    if command == "file.export.exportAs" && app.session.active().is_none() {
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
