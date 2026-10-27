//! Evora build (patch LUM-TOUCHES): the shortcuts browsers keep for themselves, typed with other
//! keys under the host bridge.
//!
//! A browser handles some keys before the page ever sees them: a new window (Cmd+N), closing,
//! opening or reopening a tab (Cmd+W, Cmd+T, Cmd+Shift+T), a private window (Cmd+Shift+N), the
//! next or previous tab (Ctrl+Tab, Ctrl+Shift+Tab). In Lumbra they are File › New, File › Close,
//! Edit › Free Transform, Edit › Transform › Again, Layer › New › Layer…, and the next or previous
//! document, which would otherwise be out of reach from the keyboard, or close Evora's tab. Under
//! the bridge each has a replacement, bound to no command of this version, that the engine reads
//! as the reserved key ([`rewrite`]); the menus, the palette, tooltips and the Keyboard Shortcuts
//! dialog show the replacement wherever they would show the reserved key
//! (`photocraft_ui_egui::shortcuts::set_host_key_aliases`). A key the user bound to a command of
//! their own keeps that command, and nothing is rewritten while a text field has the keyboard.
//! `diag.stats` reports the table (`keys`), which the smoke test compares with the menus. Without
//! the bridge, the keys are upstream's.

use egui::{Event, Modifiers, RawInput};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::shortcuts::{key_matches, parse, pretty};
use serde_json::{Value, json};

/// (reserved, replacement), in the engine's shortcut notation (`Cmd` is ⌘ on macOS, Ctrl
/// elsewhere). Reviewable at delivery (Evora's question « Raccourcis de remplacement »).
pub const ALIASES: [(&str, &str); 6] = [
    // File › New (the browser opens a window).
    ("Cmd+N", "Cmd+Alt+N"),
    // File › Close (the browser closes Evora's tab).
    ("Cmd+W", "Alt+Shift+W"),
    // Edit › Free Transform (the browser opens a tab).
    ("Cmd+T", "Alt+Shift+F"),
    // Edit › Transform › Again (the browser reopens the last closed tab).
    ("Cmd+Shift+T", "Alt+Shift+R"),
    // Layer › New › Layer… (the browser opens a private window).
    ("Cmd+Shift+N", "Alt+Shift+N"),
    // Window › next document (the browser shows its next tab).
    ("Ctrl+Tab", "Alt+Shift+Right"),
];

/// The previous document is the same key with ⇧, also kept by browsers.
const PREVIOUS: (&str, &str) = ("Ctrl+Shift+Tab", "Alt+Shift+Left");

/// Every pair this build installs.
fn pairs() -> Vec<(&'static str, &'static str)> {
    ALIASES.iter().copied().chain(std::iter::once(PREVIOUS)).collect()
}

/// Show the replacements in the menus (under the bridge only).
pub fn install() {
    photocraft_ui_egui::shortcuts::set_host_key_aliases(&pairs());
}

/// Before the frame: a press (or release) of a replacement becomes the reserved key it stands for,
/// unless a text field has the keyboard or the user bound that replacement to a command.
pub fn rewrite(app: &PhotocraftApp, ctx: &egui::Context, raw: &mut RawInput) {
    if ctx.text_edit_focused() || app.ui.text_edit.is_some() {
        return;
    }
    let mac = ctx.os() == egui::os::OperatingSystem::Mac;
    let table: Vec<(egui::KeyboardShortcut, egui::KeyboardShortcut)> =
        pairs().into_iter().filter_map(|(reserved, replacement)| Some((parse(reserved)?, parse(replacement)?))).collect();
    let mut user: Option<Vec<egui::KeyboardShortcut>> = None;
    for event in &mut raw.events {
        let Event::Key { key, physical_key, modifiers, .. } = event else { continue };
        let Some((reserved, replacement)) = table.iter().find(|(_, replacement)| key_matches(replacement, *key, *modifiers)) else { continue };
        // The user's own binding of the replacement wins (Edit › Keyboard Shortcuts).
        let bound = user.get_or_insert_with(|| photocraft_ui_egui::shortcut_dispatch::bindings(app).into_iter().map(|(_, sc)| sc).collect());
        if bound.iter().any(|sc| sc == replacement) {
            continue;
        }
        *key = reserved.logical_key;
        *physical_key = Some(reserved.logical_key);
        *modifiers = pressed_as(reserved.modifiers, mac);
    }
}

/// The modifiers a keyboard reports for `m` (⌘ on macOS, Ctrl elsewhere, for `Cmd`).
fn pressed_as(m: Modifiers, mac: bool) -> Modifiers {
    Modifiers {
        alt: m.alt,
        shift: m.shift,
        ctrl: m.ctrl || (m.command && !mac),
        mac_cmd: m.command && mac,
        command: m.command || (m.ctrl && !mac),
    }
}

/// `diag.stats` `keys`: each pair, the command the reserved key runs here, and what the menus show
/// for it (the replacement, in the platform's notation).
pub fn report(app: &PhotocraftApp) -> Value {
    let bindings = photocraft_ui_egui::shortcut_dispatch::bindings(app);
    let rows: Vec<Value> = pairs()
        .into_iter()
        .map(|(reserved, replacement)| {
            let sc = parse(reserved);
            let command = sc.and_then(|sc| bindings.iter().find(|(_, b)| *b == sc).map(|(id, _)| id.clone()));
            // A replacement is never itself a reserved key: `pretty` writes it as it is.
            json!({"reserved": reserved, "replacement": replacement, "command": command, "shown": pretty(reserved), "expected": pretty(replacement)})
        })
        .collect();
    Value::Array(rows)
}
