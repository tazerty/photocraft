//! Evora build (patch E5): `storageNs`, the opaque namespace the host passes in the page's query
//! (`?storageNs=`, a digest of the account), prefixes every name the engine keeps in the browser:
//! the localStorage preferences and the IndexedDB presets database (the web build uses no OPFS).
//! `evora:purge`, posted by the host page, empties the namespace.
//!
//! It is a namespace, not a partition: any script of the engine's origin can still list every key.
//! Isolation between accounts comes from the host (a separate origin per mode B, ARCHITECTURE
//! §3.6); the namespace keeps two accounts of one browser from reading each other's settings and
//! lets the host erase one account's traces on sign-out.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};

/// What the page's `storageNs` parameter allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// No parameter: upstream's names (admin lab only).
    Shared,
    /// A valid namespace: every name is prefixed with `<ns>:`.
    Scoped(String),
    /// A malformed parameter: nothing is read from or written to browser storage.
    Refused,
}

static NAMESPACE: OnceLock<Namespace> = OnceLock::new();

thread_local! {
    /// Set by a purge: no browser storage is read or written for the rest of the page's life.
    static PURGED: Cell<bool> = const { Cell::new(false) };
    /// The open presets database, closed by a purge so it can be deleted.
    static DATABASE: RefCell<Option<web_sys::IdbDatabase>> = const { RefCell::new(None) };
}

/// A namespace is 8 to 128 characters of `A-Z a-z 0-9 _ -` (a hex or base64url digest).
fn parse(raw: Option<&str>) -> Namespace {
    match raw {
        None => Namespace::Shared,
        Some(ns) if (8..=128).contains(&ns.len()) && ns.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') => {
            Namespace::Scoped(ns.to_owned())
        }
        Some(_) => Namespace::Refused,
    }
}

/// Read the parameter once, at startup, before any storage is touched.
pub fn init(raw: Option<&str>) {
    let ns = NAMESPACE.get_or_init(|| parse(raw));
    match ns {
        Namespace::Shared => log::info!("photocraft-web: no storageNs, upstream storage names"),
        Namespace::Scoped(_) => log::info!("photocraft-web: storage namespaced by storageNs"),
        Namespace::Refused => log::error!("photocraft-web: malformed storageNs: browser storage is not used"),
    }
}

fn current() -> &'static Namespace {
    NAMESPACE.get_or_init(|| Namespace::Shared)
}

/// The browser-storage name for `name` (`<ns>:name`, or `name` without a namespace); `None` when
/// storage is refused or the namespace was purged.
pub fn key(name: &str) -> Option<String> {
    if PURGED.get() {
        return None;
    }
    match current() {
        Namespace::Shared => Some(name.to_owned()),
        Namespace::Scoped(ns) => Some(format!("{ns}:{name}")),
        Namespace::Refused => None,
    }
}

/// Has the host purged the namespace? Nothing may be written afterwards.
pub fn purged() -> bool {
    PURGED.get()
}

/// Remember the open presets database, for a purge to close it.
pub fn register_database(db: &web_sys::IdbDatabase) {
    DATABASE.with(|d| *d.borrow_mut() = Some(db.clone()));
}

/// Empty the namespace: every localStorage key under `<ns>:` and the presets database. Later
/// writes are refused for the rest of the page's life.
fn purge(ns: &str, presets_database: &str) {
    PURGED.set(true);
    let prefix = format!("{ns}:");
    let window = web_sys::window();
    if let Some(storage) = window.as_ref().and_then(|w| w.local_storage().ok().flatten()) {
        let len = storage.length().unwrap_or(0);
        let keys: Vec<String> = (0..len).filter_map(|i| storage.key(i).ok().flatten()).filter(|k| k.starts_with(&prefix)).collect();
        for k in &keys {
            let _ = storage.remove_item(k);
        }
        log::info!("photocraft-web: evora:purge removed {} localStorage key(s)", keys.len());
    }
    if let Some(db) = DATABASE.with(|d| d.borrow_mut().take()) {
        db.close();
    }
    if let Some(factory) = window.as_ref().and_then(|w| w.indexed_db().ok().flatten()) {
        match factory.delete_database(&format!("{prefix}{presets_database}")) {
            Ok(_) => log::info!("photocraft-web: evora:purge deletes the presets database"),
            Err(e) => log::error!("photocraft-web: evora:purge could not delete the presets database: {e:?}"),
        }
    }
}

/// An origin as the host passes it (`https://evora.example`, `http://localhost:8888`): scheme,
/// host and optional port, nothing else.
fn valid_origin(origin: &str) -> bool {
    let rest = origin.strip_prefix("https://").or_else(|| origin.strip_prefix("http://"));
    matches!(rest, Some(host) if !host.is_empty()
        && host.len() <= 260
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']')))
}

/// Listen for `{type: "evora:purge", storageNs: <ns>}` from the host page. A message is accepted
/// only when its source is `window.parent`, its origin is exactly `parent_origin` (the page's
/// `?parentOrigin=`) and its `storageNs` is this page's namespace; anything else is ignored.
/// Without a namespace or a valid parent origin there is nothing to listen for.
pub fn listen_purge(parent_origin: Option<String>, presets_database: &'static str) {
    let Namespace::Scoped(ns) = current() else { return };
    let Some(parent_origin) = parent_origin.filter(|o| valid_origin(o)) else {
        log::warn!("photocraft-web: no valid parentOrigin, evora:purge is not listened to");
        return;
    };
    let Some(window) = web_sys::window() else { return };
    let ns = ns.clone();
    let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
        if e.origin() != parent_origin {
            return;
        }
        let Some(parent) = web_sys::window().and_then(|w| w.parent().ok().flatten()) else { return };
        let parent: JsValue = parent.into();
        if !e.source().is_some_and(|source| js_sys::Object::is(&source, &parent)) {
            return;
        }
        let data = e.data();
        let field = |name: &str| js_sys::Reflect::get(&data, &JsValue::from_str(name)).ok().and_then(|v| v.as_string());
        if field("type").as_deref() != Some("evora:purge") || field("storageNs").as_deref() != Some(ns.as_str()) {
            return;
        }
        purge(&ns, presets_database);
    });
    if window.add_event_listener_with_callback("message", cb.as_ref().unchecked_ref()).is_ok() {
        cb.forget();
    }
}
