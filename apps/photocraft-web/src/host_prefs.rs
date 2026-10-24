//! Evora build (patches LUM-06 r2, LUM-14 r2 and the first half of LUM-15): what the host's
//! `evora:init` sets besides the language and the theme (Evora's HOST-PROTOCOL §4.3: `accent`, and
//! `prefs`, the values the host imposes from `engines.json`), and what `diag.stats` reports of it.
//!
//! The preferences the engine takes are compiled in, as the commands of `host_commands` are: the
//! host intersects them with its own (`engines.json`, `prefs`), and this engine applies nothing else.
//! - `evora.locationPolicy`: what an export keeps of the metadata that locates the picture
//!   (`host_meta::Policy`): `none`, the default, `rights` (the rights alone) or `all`. Missing:
//!   `none`; any other value is refused and the default stays.
//! - `interface.canvasColor`: the engine's own preference for the canvas surround (Preferences ›
//!   Interface), among its neutral greys only: `default` (the theme's surround, neutral since
//!   LUM-06 r2), `black`, `darkGray`, `mediumGray` or `lightGray`; set at start through the
//!   engine's `prefs.set`, as the user would set it.
//!
//! Any other key, or a value off the list, is refused, logged and reported (`diag.stats`, `prefs`).
//!
//! Patch LUM-15 also asks for `evora:prefs`, the engine's own preferences sent to the host to be
//! kept per account (capability `prefs.sync`). It is not sent: the host's list of capabilities does
//! not admit `prefs.sync` (an unknown capability makes it refuse `evora:ready`, Evora's DT-16), the
//! norm leaves it to its decision D-P14, and the per-account store (Evora's LE-11) is outside the
//! first version. Values kept per account would come back through these same `evora:init`
//! preferences: that half needs no new message.

use photocraft_ui_egui::PhotocraftApp;
use serde_json::{Map, Value, json};

use crate::host_meta::Policy;

/// The location policy of exports (patch LUM-14 r2).
const LOCATION_POLICY: &str = "evora.locationPolicy";
/// The engine preferences the host may set, each with the values admitted.
const ENGINE_PREFS: [(&str, &[&str]); 1] = [("interface.canvasColor", &["default", "black", "darkGray", "mediumGray", "lightGray"])];

/// What `evora:init` set, once checked.
#[derive(Clone, Debug, Default)]
pub struct HostPrefs {
    /// The location policy of exports.
    pub policy: Policy,
    /// The engine preferences to set at start: (path, value), in the order received.
    engine: Vec<(String, String)>,
    /// The engine preferences set.
    applied: Vec<String>,
    /// The keys refused: off the list, a value off the list, or one the engine did not take.
    refused: Vec<String>,
}

impl HostPrefs {
    /// The preferences of `evora:init`, checked against the compiled list.
    pub fn from_init(prefs: &Map<String, Value>) -> HostPrefs {
        let mut out = HostPrefs::default();
        for (key, value) in prefs {
            if key == LOCATION_POLICY {
                match value.as_str().and_then(Policy::parse) {
                    Some(policy) => out.policy = policy,
                    None => out.refused.push(key.clone()),
                }
                continue;
            }
            match (ENGINE_PREFS.iter().find(|(path, _)| *path == key.as_str()), value.as_str()) {
                (Some((_, values)), Some(v)) if values.contains(&v) => out.engine.push((key.clone(), v.to_owned())),
                _ => out.refused.push(key.clone()),
            }
        }
        if !out.refused.is_empty() {
            log::warn!("photocraft-web: host preferences refused: {:?}", out.refused);
        }
        out
    }

    /// Sets the engine preferences, each on its own (one the engine refuses leaves the others).
    pub fn apply(self, app: &mut PhotocraftApp) -> HostPrefs {
        let mut applied = self.applied.clone();
        let mut refused = self.refused.clone();
        for (path, value) in &self.engine {
            match app.run("prefs.set", json!({"path": path, "value": value})) {
                Ok(_) => applied.push(path.clone()),
                Err(e) => {
                    log::warn!("photocraft-web: host preference {path} = {value} not applied: {e}");
                    refused.push(path.clone());
                }
            }
        }
        HostPrefs { applied, refused, ..self }
    }

    /// `diag.stats`, `prefs`: the location policy, the canvas surround preference and the colour
    /// it draws, what was set and what was refused.
    pub fn report(&self, app: &PhotocraftApp, ctx: &egui::Context) -> Value {
        json!({
            "locationPolicy": self.policy.name(),
            "canvasColor": app.session.prefs().interface.canvas_color.name(),
            "canvasEffective": hex(photocraft_ui_egui::crop_shield::canvas_color(app, ctx)),
            "applied": self.applied,
            "refused": self.refused,
        })
    }
}

/// The host's accent (`#` and six hex digits, checked by the bridge) as a colour.
pub fn accent(text: &str) -> Option<egui::Color32> {
    let digits = text.strip_prefix('#').filter(|d| d.len() == 6)?;
    let byte = |i: usize| digits.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    Some(egui::Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// `diag.stats`, `theme` (patch LUM-06 r2): the tokens the interface is drawn with, and the strokes
/// egui paints resting fields, focused or active widgets and text selections with. Colours as
/// `#RRGGBBAA`, unmultiplied.
pub fn theme_report(ctx: &egui::Context) -> Value {
    let t = photocraft_ui_egui::theme::Tokens::get(ctx);
    let style = ctx.global_style();
    let w = &style.visuals.widgets;
    json!({
        "kind": t.kind.id(),
        "accent": hex(t.accent),
        "accentText": hex(t.accent_text),
        "accentBorder": hex(t.accent_border),
        "accentSoft": hex(t.accent_soft),
        "primaryBg": hex(t.primary_bg),
        "primaryText": hex(t.primary_text),
        "fieldBorder": hex(t.field_border),
        "field": hex(t.field),
        "card": hex(t.card),
        "chrome": hex(t.chrome),
        "dock": hex(t.dock),
        "hover": hex(t.hover),
        "pressed": hex(t.pressed),
        "canvas": hex(t.canvas),
        "canvasDot": hex(t.canvas_dot),
        "inactiveBorder": hex(w.inactive.bg_stroke.color),
        "activeBorder": hex(w.active.bg_stroke.color),
        "selectionStroke": hex(style.visuals.selection.stroke.color),
    })
}

/// `#RRGGBBAA`, unmultiplied.
fn hex(c: egui::Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}
