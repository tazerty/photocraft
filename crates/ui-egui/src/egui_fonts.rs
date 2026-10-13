//! egui's default fonts (Ubuntu Light, Hack and emoji-icon-font: the fallback stack behind Inter
//! and JetBrains Mono, for symbols), stored deflated.
//!
//! Evora build (patch E10, no emoji in the interface): egui's Noto Emoji face is left out of the
//! stack and of the binary, so no string can be drawn as an emoji glyph; the CI gate resolves every
//! symbol of the displayed catalogs against this stack.
//!
//! egui's `default_fonts` feature embeds them uncompressed (1.4 MB), which the web build's 24 MiB
//! size gate (packaging/web/package.sh) can't afford. The app builds egui without it; build.rs
//! deflates the same files from the `epaint_default_fonts` crate (0.78 MB) and they are inflated
//! once, the first time the fonts are installed. Test builds keep the feature (dev-dependencies),
//! so egui's own definitions are the oracle for [`definitions`].

use std::sync::{Arc, LazyLock};

use egui::{FontData, FontDefinitions, FontFamily, FontTweak};

/// A font deflated by build.rs.
macro_rules! deflated {
    ($name:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/egui_fonts/", $name, ".ttf.deflate"))
    };
}

static HACK: LazyLock<Vec<u8>> = LazyLock::new(|| inflate(deflated!("Hack-Regular")));
static UBUNTU_LIGHT: LazyLock<Vec<u8>> = LazyLock::new(|| inflate(deflated!("Ubuntu-Light")));
static EMOJI_ICON: LazyLock<Vec<u8>> = LazyLock::new(|| inflate(deflated!("emoji-icon-font")));

/// Inflated fonts are bounded (the largest is 0.42 MB), so corrupt data can't exhaust memory.
const MAX_FONT_BYTES: u64 = 16 << 20;

/// Inflate a font. Data that doesn't inflate gives empty bytes (the font is left out), never a panic.
fn inflate(deflated: &[u8]) -> Vec<u8> {
    use std::io::Read as _;
    let mut out = Vec::new();
    match flate2::read::DeflateDecoder::new(deflated).take(MAX_FONT_BYTES + 1).read_to_end(&mut out) {
        Ok(_) if out.len() as u64 <= MAX_FONT_BYTES => out,
        _ => Vec::new(),
    }
}

/// The emoji face of egui's defaults, left out of the Evora build (patch E10).
const NOTO_EMOJI: &str = "NotoEmoji-Regular";

/// `FontDefinitions::default()` with egui's default fonts, whether or not egui was built with
/// `default_fonts`: the same faces, tweaks and family order, without the emoji face (patch E10).
pub fn definitions() -> FontDefinitions {
    let fonts = FontDefinitions::default();
    without_emoji(if fonts.font_data.is_empty() { from_deflated() } else { fonts })
}

/// `fonts` without egui's Noto Emoji face, in its data and in every family stack.
fn without_emoji(mut fonts: FontDefinitions) -> FontDefinitions {
    fonts.font_data.remove(NOTO_EMOJI);
    for stack in fonts.families.values_mut() {
        stack.retain(|name| name != NOTO_EMOJI);
    }
    fonts
}

/// egui's default definitions, built from the deflated copies.
fn from_deflated() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    let faces: [(&str, &'static [u8], f32); 3] = [
        ("Hack", HACK.as_slice(), 1.0),
        ("Ubuntu-Light", UBUNTU_LIGHT.as_slice(), 1.0),
        // egui draws its icon font a little smaller.
        ("emoji-icon-font", EMOJI_ICON.as_slice(), 0.90),
    ];
    for (name, bytes, scale) in faces {
        if bytes.is_empty() {
            log::error!("egui default font {name} is unavailable");
            continue;
        }
        let data = FontData::from_static(bytes).tweak(FontTweak { scale, ..Default::default() });
        fonts.font_data.insert(name.to_owned(), Arc::new(data));
    }
    let stack = |names: &[&str]| -> Vec<String> { names.iter().filter(|n| fonts.font_data.contains_key(**n)).map(|n| (*n).to_owned()).collect() };
    let mono = stack(&["Hack", "Ubuntu-Light", "emoji-icon-font"]);
    let proportional = stack(&["Ubuntu-Light", "emoji-icon-font"]);
    fonts.families.insert(FontFamily::Monospace, mono);
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The definitions built from the deflated copies are egui's own, without the emoji face: the
    /// same bytes, tweaks and family order (test builds enable `default_fonts`, so
    /// `FontDefinitions::default()` is egui's).
    #[test]
    fn deflated_fonts_match_egui_defaults_without_emoji() {
        let egui = FontDefinitions::default();
        assert!(!egui.font_data.is_empty(), "test builds enable egui's default_fonts (dev-dependencies)");
        let expected = without_emoji(egui);
        assert_eq!(from_deflated(), expected);
        assert_eq!(definitions(), expected);
        assert!(!definitions().font_data.contains_key(NOTO_EMOJI));
        assert!(definitions().families.values().all(|stack| !stack.iter().any(|n| n == NOTO_EMOJI)));
    }

    #[test]
    fn corrupt_font_data_is_left_out_not_a_panic() {
        assert!(inflate(b"not deflate data").is_empty());
        assert!(inflate(&[]).is_empty());
    }
}
