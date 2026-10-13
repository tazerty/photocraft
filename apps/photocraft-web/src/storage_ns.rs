//! Evora build (patch E5, corrected with the EHP bridge of patch E4): the browser storage the engine
//! keeps, its namespace and its purge.
//!
//! The engine's own names are the localStorage key `photocraft.preferences` and the IndexedDB
//! database `photocraft.presets`. The web build uses no OPFS, Cache Storage, sessionStorage or
//! service worker (nothing else to name or purge; the smoke test checks OPFS and Cache Storage stay
//! empty).
//!
//! - Without the Evora bridge (no `?host=evora`): upstream's names, unchanged.
//! - With the bridge: nothing is read or written before `evora:init`. `storageNs`, the account's
//!   opaque namespace, comes ONLY from `evora:init` (never from the URL, a message field elsewhere or
//!   the referrer): with the `storage.ns` capability every name becomes `<storageNs>:<name>`;
//!   without it the engine keeps no browser storage at all. With `storage.purge`, `evora:init`
//!   first deletes every other namespace of this engine and the bare upstream names (HOST-PROTOCOL
//!   §7.7 b), and `evora:purge` (checked by the bridge: parent window, admitted origin, envelope)
//!   empties the engine's storage: writes stop first (no key or database is ever reopened or
//!   recreated afterwards, for the rest of the page's life), the open presets database is closed,
//!   then the keys and databases are deleted, and the bridge acknowledges only once the deletions
//!   have completed. A purge covers the remnants outside the namespace too (the bare upstream
//!   names). Other engines that share the engine origin keep their own names: only this engine's
//!   names are touched.
//!
//! It is a namespace, not a partition: any script of the engine's origin can still list every key.
//! Isolation between accounts comes from the host (a separate origin in mode B, ARCHITECTURE §3.6);
//! the namespace keeps two accounts of one browser from reading each other's settings and lets the
//! host erase one account's traces on sign-out.

use std::cell::{Cell, RefCell};

use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};

/// The preferences key, inside the namespace (`<storageNs>:photocraft.preferences`).
pub const PREFS_KEY: &str = "photocraft.preferences";

/// Where the engine's names live.
enum Mode {
    /// Not decided yet (before `evora:init`), or no `storage.ns`: no browser storage.
    Refused,
    /// No bridge: upstream's names.
    Shared,
    /// Every name prefixed with `<ns>:`.
    Scoped(String),
}

thread_local! {
    static MODE: RefCell<Mode> = const { RefCell::new(Mode::Refused) };
    /// The account's namespace from `evora:init` (kept even without `storage.ns`, to aim purges).
    static NS: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Set by a purge, first: no browser storage is read or written for the rest of the page's life.
    static PURGED: Cell<bool> = const { Cell::new(false) };
    /// The open presets database, closed by a purge so it can be deleted.
    static DATABASE: RefCell<Option<web_sys::IdbDatabase>> = const { RefCell::new(None) };
}

/// Without the Evora bridge: upstream's names, unchanged.
pub fn shared() {
    MODE.with(|m| *m.borrow_mut() = Mode::Shared);
    log::info!("photocraft-web: no Evora bridge, upstream storage names");
}

/// At `evora:init`: the account's namespace; its names are used only with `storage.ns`.
pub fn scope(ns: &str, use_storage: bool) {
    NS.with(|n| *n.borrow_mut() = Some(ns.to_owned()));
    MODE.with(|m| *m.borrow_mut() = if use_storage { Mode::Scoped(ns.to_owned()) } else { Mode::Refused });
    if use_storage {
        log::info!("photocraft-web: browser storage under the host's namespace");
    } else {
        log::info!("photocraft-web: no storage.ns, browser storage is not used");
    }
}

/// The browser-storage name for `name` (`<ns>:name`, or `name` without the bridge); `None` while
/// storage is refused and after a purge.
pub fn key(name: &str) -> Option<String> {
    if PURGED.get() {
        return None;
    }
    MODE.with(|m| match &*m.borrow() {
        Mode::Shared => Some(name.to_owned()),
        Mode::Scoped(ns) => Some(format!("{ns}:{name}")),
        Mode::Refused => None,
    })
}

/// Has the host purged the storage? Nothing may be written afterwards.
pub fn purged() -> bool {
    PURGED.get()
}

/// Remember the open presets database, for a purge to close it. After a purge (one that arrived
/// while the database was opening) it is closed at once: `false`, the app keeps no handle.
pub fn register_database(db: &web_sys::IdbDatabase) -> bool {
    if PURGED.get() {
        db.close();
        return false;
    }
    DATABASE.with(|d| *d.borrow_mut() = Some(db.clone()));
    true
}

/// What a purge empties among this engine's names.
#[derive(Clone, Copy)]
enum Scope {
    /// `evora:purge {scope: "current"}`: the account's namespace and the bare upstream names.
    Current,
    /// `evora:purge {scope: "all"}`: every namespace of this engine, and the bare names.
    All,
    /// `evora:init`: every namespace but the account's, and the bare names (§7.7 b).
    Others,
}

/// Is `stored` this engine's `name`, bare or under a namespace (`<ns>:name`)?
fn engine_name(stored: &str, name: &str) -> bool {
    stored == name || stored.strip_suffix(name).is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with(':'))
}

fn targeted(stored: &str, name: &str, scope: Scope, ns: Option<&str>) -> bool {
    if !engine_name(stored, name) {
        return false;
    }
    let mine = ns.is_some_and(|ns| stored.strip_suffix(name).is_some_and(|p| p.strip_suffix(':') == Some(ns)));
    match scope {
        Scope::All => true,
        Scope::Current => mine || stored == name,
        Scope::Others => !mine,
    }
}

/// Remove this engine's localStorage keys in `scope`; how many.
fn remove_keys(scope: Scope, ns: Option<&str>) -> usize {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else { return 0 };
    let len = storage.length().unwrap_or(0);
    let keys: Vec<String> = (0..len).filter_map(|i| storage.key(i).ok().flatten()).filter(|k| targeted(k, PREFS_KEY, scope, ns)).collect();
    for k in &keys {
        let _ = storage.remove_item(k);
    }
    keys.len()
}

/// This engine's databases in `scope`: the names it knows, plus those the browser lists.
async fn databases(scope: Scope, ns: Option<&str>) -> Vec<String> {
    let name = crate::indexed_presets::DATABASE;
    let mut names = vec![name.to_owned()];
    if let (Scope::Current | Scope::All, Some(ns)) = (scope, ns) {
        names.push(format!("{ns}:{name}"));
    }
    for listed in listed_databases().await {
        if targeted(&listed, name, scope, ns) && !names.contains(&listed) {
            names.push(listed);
        }
    }
    names
}

/// `indexedDB.databases()` (absent from web-sys and from older browsers: then nothing is listed).
async fn listed_databases() -> Vec<String> {
    let Some(factory) = web_sys::window().and_then(|w| w.indexed_db().ok().flatten()) else { return Vec::new() };
    let Ok(list) = js_sys::Reflect::get(&factory, &JsValue::from_str("databases")) else { return Vec::new() };
    let Some(list) = list.dyn_ref::<js_sys::Function>() else { return Vec::new() };
    let Ok(promise) = list.call0(&factory) else { return Vec::new() };
    let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else { return Vec::new() };
    let Ok(found) = wasm_bindgen_futures::JsFuture::from(promise).await else { return Vec::new() };
    if !js_sys::Array::is_array(&found) {
        return Vec::new();
    }
    js_sys::Array::from(&found).iter().filter_map(|d| js_sys::Reflect::get(&d, &JsValue::from_str("name")).ok().and_then(|n| n.as_string())).collect()
}

/// Delete database `name`; resolved once the browser reports it deleted. A `blocked` event (another
/// connection still open) is only logged: the deletion completes when that connection closes.
async fn delete_database(name: &str) -> Result<(), String> {
    let factory = web_sys::window().and_then(|w| w.indexed_db().ok().flatten()).ok_or("IndexedDB unavailable")?;
    let request = factory.delete_database(name).map_err(|e| format!("{e:?}"))?;
    let mut handlers: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let done = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let failed = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            let _ = reject.call0(&JsValue::NULL);
        });
        let waiting = name.to_owned();
        let blocked = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            log::warn!("photocraft-web: deleting {waiting} waits for another connection to close");
        });
        request.set_onsuccess(Some(done.as_ref().unchecked_ref()));
        request.set_onerror(Some(failed.as_ref().unchecked_ref()));
        request.set_onblocked(Some(blocked.as_ref().unchecked_ref()));
        handlers.extend([done, failed, blocked]);
    });
    let result = wasm_bindgen_futures::JsFuture::from(promise).await;
    request.set_onsuccess(None);
    request.set_onerror(None);
    request.set_onblocked(None);
    drop(handlers);
    result.map(|_| ()).map_err(|_| format!("deleting {name} failed"))
}

/// `evora:init` with `storage.purge`: delete every other namespace of this engine and the bare
/// names. The deletions run on; none of them is a database this page opens.
pub async fn purge_others(ns: &str) {
    let keys = remove_keys(Scope::Others, Some(ns));
    let names = databases(Scope::Others, Some(ns)).await;
    log::info!("photocraft-web: evora:init removed {keys} key(s) of other namespaces, deletes {} database name(s)", names.len());
    for name in names {
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = delete_database(&name).await {
                log::warn!("photocraft-web: {e}");
            }
        });
    }
}

/// First step of `evora:purge`, run as soon as the message is accepted: no browser storage is read
/// or written any more (no key set, no database opened or recreated), and the open presets database
/// is closed (its deletion waits for a transaction still in flight, never the reverse).
pub fn stop_writes() {
    PURGED.set(true);
    if let Some(db) = DATABASE.with(|d| d.borrow_mut().take()) {
        db.close();
    }
}

/// `evora:purge`: stop every write, close the presets database, then delete this engine's keys and
/// databases (`all`: every namespace; otherwise the account's and the bare names). Resolves once the
/// deletions have completed; `Err` holds the `evora:ack` code.
pub async fn purge(all: bool) -> Result<(), &'static str> {
    stop_writes();
    let ns = NS.with(|n| n.borrow().clone());
    let scope = if all { Scope::All } else { Scope::Current };
    let keys = remove_keys(scope, ns.as_deref());
    let names = databases(scope, ns.as_deref()).await;
    let mut failed = false;
    for name in &names {
        if let Err(e) = delete_database(name).await {
            log::warn!("photocraft-web: evora:purge: {e}");
            failed = true;
        }
    }
    log::info!("photocraft-web: evora:purge removed {keys} localStorage key(s) and deleted {} database name(s)", names.len());
    if failed { Err("busy") } else { Ok(()) }
}
