//! Evora build (patch E9): the display overlay catalog (`overlay.txt`). Third-party product terms
//! are renamed where the UI shows text, for every language and for English, after the language
//! catalog; command ids, catalog keys, the engine crates and saved files keep upstream's wording.
//!
//! The lookups return borrowed strings, so a rewritten string is built once and kept for the rest
//! of the run (the strings that need it come from the catalogs and a few labels: a small, bounded
//! set). The CI gate applies the same file to the catalogs and checks that no sensitive term is
//! left on screen.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The rules, as shipped (see the file's header for the format).
const RULES_TEXT: &str = include_str!("overlay.txt");

/// Rewritten strings kept at most; past this, a rewrite is still returned but no longer cached.
const MAX_CACHED: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The whole string equals `from`.
    Exact,
    /// Every occurrence of `from`.
    Sub,
    /// The word `from` with the space next to it.
    Drop,
}

#[derive(Debug, PartialEq, Eq)]
struct Rule {
    kind: Kind,
    lang: String,
    from: String,
    to: String,
}

/// Parse `overlay.txt`. A malformed line is skipped (and logged), never a panic.
fn parse(text: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let (kind, columns) = match cols.first().copied() {
            Some("exact") => (Kind::Exact, 4),
            Some("sub") => (Kind::Sub, 4),
            Some("drop") => (Kind::Drop, 3),
            _ => {
                log::error!("overlay.txt line {}: unknown kind", n + 1);
                continue;
            }
        };
        let (Some(lang), Some(from)) = (cols.get(1), cols.get(2)) else {
            log::error!("overlay.txt line {}: expected kind, language, from", n + 1);
            continue;
        };
        if from.is_empty() || cols.len() != columns {
            log::error!("overlay.txt line {}: empty pattern or wrong number of columns", n + 1);
            continue;
        }
        let to = cols.get(3).copied().unwrap_or("");
        rules.push(Rule { kind, lang: (*lang).to_owned(), from: (*from).to_owned(), to: to.to_owned() });
    }
    rules
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| parse(RULES_TEXT))
}

fn applies(rule: &Rule, lang: &str) -> bool {
    rule.lang == "*" || rule.lang.eq_ignore_ascii_case(lang)
}

/// The rewrite of `s` for `lang`, or `None` when no rule touches it.
fn rewrite(rules: &[Rule], lang: &str, s: &str) -> Option<String> {
    let exact = |wanted_lang: &str| rules.iter().find(|r| r.kind == Kind::Exact && r.from == s && r.lang.eq_ignore_ascii_case(wanted_lang));
    if let Some(r) = exact(lang).or_else(|| exact("*")) {
        return Some(r.to.clone());
    }
    let mut out: Option<String> = None;
    for r in rules.iter().filter(|r| r.kind != Kind::Exact && applies(r, lang)) {
        let current = out.as_deref().unwrap_or(s);
        if !current.contains(r.from.as_str()) {
            continue;
        }
        out = Some(match r.kind {
            Kind::Drop => current.replace(&format!("{} ", r.from), "").replace(&format!(" {}", r.from), ""),
            _ => current.replace(r.from.as_str(), &r.to),
        });
    }
    out
}

/// `s` as the UI shows it in `lang`: unchanged (the common case, no allocation), or rewritten.
pub fn apply<'a>(lang: &str, s: &'a str) -> &'a str {
    let rules = rules();
    if !rules.iter().any(|r| if r.kind == Kind::Exact { r.from == s } else { s.contains(r.from.as_str()) }) {
        return s;
    }
    static CACHE: OnceLock<Mutex<HashMap<(String, String), &'static str>>> = OnceLock::new();
    let key = (lang.to_ascii_lowercase(), s.to_owned());
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&done) = cache.get(&key) {
        return done;
    }
    let Some(text) = rewrite(rules, lang, s) else { return s };
    let done: &'static str = Box::leak(text.into_boxed_str());
    if cache.len() < MAX_CACHED {
        cache.insert(key, done);
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_rules_parse() {
        let rules = parse(RULES_TEXT);
        assert!(rules.len() >= 10, "{rules:?}");
        assert_eq!(rules.len(), RULES_TEXT.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count());
    }

    #[test]
    fn adobe_terms_are_renamed_in_every_language() {
        assert_eq!(apply("en", "Adobe RGB (1998) compatible"), "ClayRGB (1998) compatible");
        assert_eq!(apply("de", "Adobe-RGB(1998)-kompatibel"), "ClayRGB(1998)-kompatibel");
        assert_eq!(apply("fr", "Composeur ligne par ligne Adobe"), "Composeur ligne par ligne");
        assert_eq!(apply("en", "Adobe Single-line Composer"), "Single-line Composer");
        assert_eq!(apply("en", "Adobe PDF Presets…"), "PDF Presets…");
        assert_eq!(apply("fr", "Adobe"), "Intégré");
        assert_eq!(apply("en", "Adobe"), "Built-in");
        assert_eq!(apply("hu", "Adobe egysoros szedőmotor"), "Egysoros szedőmotor");
        assert_eq!(apply("ja", "Adobe 1 行コンポーザー"), "1 行コンポーザー");
        assert_eq!(apply("pt-br", "Predefinições do Adobe PDF…"), "Predefinições do PDF…");
        assert_eq!(apply("en", "Layers"), "Layers");
    }
}
