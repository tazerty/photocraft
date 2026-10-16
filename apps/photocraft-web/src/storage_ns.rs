//! Evora build (patch E5, corrected with the EHP bridge of patch E4, then for several documents):
//! the browser storage the engine keeps, its namespace and its purge.
//!
//! The engine's own names are the localStorage key `photocraft.preferences` and the IndexedDB
//! database `photocraft.presets`. The web build uses no OPFS, Cache Storage, sessionStorage or
//! service worker (nothing else to name or purge; the smoke test checks OPFS and Cache Storage stay
//! empty).
//!
//! - Without the Evora bridge (no `?host=evora`): upstream's names and behaviour, unchanged.
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
//! - Under the namespace the engine also keeps a marker for its presets database: the localStorage
//!   key named like the database (`<storageNs>:photocraft.presets`), written before the database is
//!   opened and removed once it is deleted. With the preferences keys, it tells a purge which
//!   namespaces hold a database where the browser cannot list databases (`indexedDB.databases()`
//!   absent); the purge says so in the console.
//!
//! Several documents of the engine origin (HOST-PROTOCOL §7.7): another tab of the same account, or
//! the engine of the account that signed in next, may purge while this document runs. This document
//! then stops writing and closes its presets database at once, when the purge deletes that database
//! (its connection receives `versionchange`) or removes this account's keys (a `storage` event):
//! no engine document blocks a purge, and none recreates what it erased. The purging document
//! finds the databases to delete, deletes the preferences keys, then every database (asked at once,
//! awaited within the engine's storage deadline, `indexed_presets::TIMEOUT_MS`), then the
//! preferences keys again (a document that wrote one before it learnt of the purge). A connection
//! that does not cooperate (a page without the bridge, an older build) can still hold a deletion
//! back: `evora:purge` then answers `busy` instead of never answering, `evora:init` logs it, and the
//! browser completes the deletion once that connection closes.
//!
//! It is a namespace, not a partition: any script of the engine's origin can still list every key.
//! Isolation between accounts comes from the host (a separate origin in mode B, ARCHITECTURE §3.6);
//! the namespace keeps two accounts of one browser from reading each other's settings and lets the
//! host erase one account's traces on sign-out.

use std::cell::{Cell, RefCell};

use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};

/// The preferences key, inside the namespace (`<storageNs>:photocraft.preferences`).
pub const PREFS_KEY: &str = "photocraft.preferences";

/// A presets database's marker: the localStorage key named like the database
/// (`<storageNs>:photocraft.presets`).
const MARKER_KEY: &str = crate::indexed_presets::DATABASE;

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
    /// Does this document already listen to the other documents' purges (`storage` events)?
    static WATCHING: Cell<bool> = const { Cell::new(false) };
}

/// Without the Evora bridge: upstream's names, unchanged.
pub fn shared() {
    MODE.with(|m| *m.borrow_mut() = Mode::Shared);
    log::info!("photocraft-web: no Evora bridge, upstream storage names");
}

/// At `evora:init`: the account's namespace; its names are used only with `storage.ns`. From now
/// on another document's purge of this account stops this document's writes too.
pub fn scope(ns: &str, use_storage: bool) {
    NS.with(|n| *n.borrow_mut() = Some(ns.to_owned()));
    MODE.with(|m| *m.borrow_mut() = if use_storage { Mode::Scoped(ns.to_owned()) } else { Mode::Refused });
    watch_other_documents();
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

fn bridged() -> bool {
    MODE.with(|m| !matches!(*m.borrow(), Mode::Shared))
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Under the bridge, before the presets database is opened: its marker in localStorage, under the
/// namespace. Without the bridge, nothing (upstream).
pub fn mark_database() {
    if !bridged() {
        return;
    }
    let Some(marker) = key(MARKER_KEY) else { return };
    if let Some(storage) = local_storage()
        && let Err(e) = storage.set_item(&marker, "1")
    {
        log::warn!("photocraft-web: the presets database marker could not be written: {e:?}");
    }
}

/// Under the bridge, the presets database `db` as soon as it is open: when another document deletes
/// it (a purge: the connection receives `versionchange`), this document stops writing and closes it
/// at once, so it never holds the deletion back and never writes again. Without the bridge, nothing
/// (upstream).
pub fn watch_database(db: &web_sys::IdbDatabase) {
    if !bridged() {
        return;
    }
    let own = db.clone();
    let on_change = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        log::info!("photocraft-web: another document deletes the presets database: writes stop here");
        stop_writes();
        own.close();
    });
    db.set_onversionchange(Some(on_change.as_ref().unchecked_ref()));
    // One presets connection per page: the handler lives as long as the page.
    on_change.forget();
}

/// Under the bridge, once per page: a `storage` event (fired in every document of the origin but
/// the one that changed the storage) that removes this account's preferences or database marker,
/// or clears localStorage, comes from a purge in another document (another tab's `evora:purge`, the
/// next account's `evora:init`). Writes stop here too, at once, and the presets database is closed.
fn watch_other_documents() {
    if WATCHING.replace(true) {
        return;
    }
    let (Some(window), Some(local)) = (web_sys::window(), local_storage()) else { return };
    let local = JsValue::from(local);
    let on_storage = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let field = |name: &str| js_sys::Reflect::get(&event, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED);
        if PURGED.get() || !js_sys::Object::is(&field("storageArea"), &local) {
            return;
        }
        let key = field("key");
        // `clear()` (no key), or one of this account's keys removed (no new value).
        let erased = key.is_null() || (field("newValue").is_null() && key.as_string().is_some_and(|k| own_key(&k)));
        if erased {
            log::info!("photocraft-web: another document purged this account's storage: writes stop here");
            stop_writes();
        }
    });
    if window.add_event_listener_with_callback("storage", on_storage.as_ref().unchecked_ref()).is_ok() {
        on_storage.forget();
    }
}

/// Is `stored` one of this account's keys (its preferences or its database marker)?
fn own_key(stored: &str) -> bool {
    NS.with(|n| n.borrow().as_deref().is_some_and(|ns| [PREFS_KEY, MARKER_KEY].iter().any(|name| namespace_of(stored, name) == Some(ns))))
}

/// The namespace of `stored` when it is `<ns>:<name>` with a non-empty `ns`.
fn namespace_of<'a>(stored: &'a str, name: &str) -> Option<&'a str> {
    stored.strip_suffix(name)?.strip_suffix(':').filter(|ns| !ns.is_empty())
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
    stored == name || namespace_of(stored, name).is_some()
}

fn targeted(stored: &str, name: &str, scope: Scope, ns: Option<&str>) -> bool {
    if !engine_name(stored, name) {
        return false;
    }
    let mine = ns.is_some_and(|ns| namespace_of(stored, name) == Some(ns));
    match scope {
        Scope::All => true,
        Scope::Current => mine || stored == name,
        Scope::Others => !mine,
    }
}

/// Remove this engine's preferences keys in `scope`; how many.
fn remove_keys(scope: Scope, ns: Option<&str>) -> usize {
    let Some(storage) = local_storage() else { return 0 };
    let len = storage.length().unwrap_or(0);
    let keys: Vec<String> = (0..len).filter_map(|i| storage.key(i).ok().flatten()).filter(|k| targeted(k, PREFS_KEY, scope, ns)).collect();
    for k in &keys {
        let _ = storage.remove_item(k);
    }
    keys.len()
}

/// The namespaces under which localStorage holds this engine's preferences or a database marker.
fn stored_namespaces() -> Vec<String> {
    let Some(storage) = local_storage() else { return Vec::new() };
    let len = storage.length().unwrap_or(0);
    let mut found: Vec<String> = Vec::new();
    for stored in (0..len).filter_map(|i| storage.key(i).ok().flatten()) {
        for name in [PREFS_KEY, MARKER_KEY] {
            if let Some(ns) = namespace_of(&stored, name)
                && !found.iter().any(|f| f == ns)
            {
                found.push(ns.to_owned());
            }
        }
    }
    found
}

/// This engine's databases in `scope`: the names it knows, those the browser lists, and those
/// localStorage records (a namespace with preferences or a marker). The flag is `false` when the
/// browser cannot list databases.
async fn databases(scope: Scope, ns: Option<&str>) -> (Vec<String>, bool) {
    let name = crate::indexed_presets::DATABASE;
    let mut names = vec![name.to_owned()];
    if let (Scope::Current | Scope::All, Some(ns)) = (scope, ns) {
        names.push(format!("{ns}:{name}"));
    }
    let listed = listed_databases().await;
    let recorded = stored_namespaces().into_iter().map(|other| format!("{other}:{name}"));
    for candidate in listed.iter().flatten().cloned().chain(recorded) {
        if targeted(&candidate, name, scope, ns) && !names.contains(&candidate) {
            names.push(candidate);
        }
    }
    (names, listed.is_some())
}

/// `indexedDB.databases()` (absent from web-sys and from older browsers): `None` when the browser
/// cannot list its databases.
async fn listed_databases() -> Option<Vec<String>> {
    let factory = web_sys::window()?.indexed_db().ok().flatten()?;
    let list = js_sys::Reflect::get(&factory, &JsValue::from_str("databases")).ok()?;
    let list = list.dyn_ref::<js_sys::Function>()?;
    let promise = list.call0(&factory).ok()?.dyn_into::<js_sys::Promise>().ok()?;
    let found = wasm_bindgen_futures::JsFuture::from(promise).await.ok()?;
    if !js_sys::Array::is_array(&found) {
        return None;
    }
    Some(js_sys::Array::from(&found).iter().filter_map(|d| js_sys::Reflect::get(&d, &JsValue::from_str("name")).ok().and_then(|n| n.as_string())).collect())
}

/// A database deletion the browser is running: its request, the promise of its outcome (`true` once
/// deleted, `false` if it failed; it never rejects) and its handlers, detached when it is dropped.
struct Deletion {
    name: String,
    request: web_sys::IdbOpenDbRequest,
    outcome: js_sys::Promise,
    _handlers: Vec<Closure<dyn FnMut(web_sys::Event)>>,
}

impl Deletion {
    /// Ask the browser to delete database `name`. A `blocked` event (a connection still open) is
    /// only logged: the deletion completes when that connection closes.
    fn start(name: &str) -> Result<Self, String> {
        let factory = web_sys::window().and_then(|w| w.indexed_db().ok().flatten()).ok_or("IndexedDB unavailable")?;
        let request = factory.delete_database(name).map_err(|e| format!("deleting {name}: {e:?}"))?;
        let mut handlers: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
        let outcome = js_sys::Promise::new(&mut |resolve, _reject| {
            let deleted = resolve.clone();
            let done = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
                let _ = deleted.call1(&JsValue::NULL, &JsValue::TRUE);
            });
            let failed = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
                let _ = resolve.call1(&JsValue::NULL, &JsValue::FALSE);
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
        Ok(Deletion { name: name.to_owned(), request, outcome, _handlers: handlers })
    }

    /// Wait for the deletion until `deadline`; once deleted, the database's marker goes too. A
    /// deletion still held back then keeps running in the browser (its marker is removed if it
    /// completes later) and is reported as an `Err`.
    async fn finish(self, deadline: f64) -> Result<(), String> {
        let outcome = crate::indexed_presets::before(&self.outcome, deadline).await;
        match outcome {
            Ok(deleted) if deleted.as_bool() == Some(true) => {
                remove_marker(&self.name);
                Ok(())
            }
            Ok(_) => Err(format!("deleting {} failed", self.name)),
            Err(_) => {
                let message = format!("deleting {} is still held back by a connection that does not close", self.name);
                wasm_bindgen_futures::spawn_local(async move {
                    let late = wasm_bindgen_futures::JsFuture::from(self.outcome.clone()).await;
                    if late.ok().and_then(|v| v.as_bool()) == Some(true) {
                        remove_marker(&self.name);
                        log::info!("photocraft-web: {} deleted once the blocking connection closed", self.name);
                    }
                    drop(self);
                });
                Err(message)
            }
        }
    }
}

impl Drop for Deletion {
    fn drop(&mut self) {
        self.request.set_onsuccess(None);
        self.request.set_onerror(None);
        self.request.set_onblocked(None);
    }
}

/// The marker of database `name` (the localStorage key of the same name), once it is deleted.
fn remove_marker(name: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(name);
    }
}

/// Erase this engine's storage in `scope`: the databases to delete are found first (the browser's
/// list, and the namespaces localStorage records, preferences included), then the preferences keys
/// go (nothing reads them any more), then every database (asked at once, awaited within the engine's
/// storage deadline; each marker goes once its database is deleted), then the preferences keys again
/// (a document that wrote one before it learnt of the purge). `false` when a deletion failed or is
/// still held back.
async fn erase(scope: Scope, ns: Option<&str>, what: &str) -> bool {
    let (names, listed) = databases(scope, ns).await;
    let keys = remove_keys(scope, ns);
    if !listed && !matches!(scope, Scope::Current) {
        if local_storage().is_some() {
            log::warn!("photocraft-web: {what}: the browser cannot list databases; other namespaces are found through localStorage only");
        } else {
            log::warn!("photocraft-web: {what}: neither indexedDB.databases() nor localStorage: databases of other namespaces cannot be found, the purge is incomplete");
        }
    }
    let deadline = js_sys::Date::now() + crate::indexed_presets::TIMEOUT_MS;
    let started: Vec<Result<Deletion, String>> = names.iter().map(|name| Deletion::start(name)).collect();
    let mut complete = true;
    for deletion in started {
        let result = match deletion {
            Ok(deletion) => deletion.finish(deadline).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log::warn!("photocraft-web: {what}: {e}");
            complete = false;
        }
    }
    let again = remove_keys(scope, ns);
    let state = if complete { "deleted" } else { "not all deleted" };
    log::info!("photocraft-web: {what} removed {} localStorage key(s); {} database name(s) {state}", keys + again, names.len());
    complete
}

/// `evora:init` with `storage.purge`: delete every other namespace of this engine and the bare
/// names, awaited (within the storage deadline) before the engine opens its own storage.
pub async fn purge_others(ns: &str) {
    if !erase(Scope::Others, Some(ns), "evora:init").await {
        log::warn!("photocraft-web: evora:init: storage of other namespaces is left behind; the next start tries again");
    }
}

/// First step of `evora:purge`, run as soon as the message is accepted, and of another document's
/// purge seen here: no browser storage is read or written any more (no key set, no database opened
/// or recreated), and the open presets database is closed (its deletion waits for a transaction
/// still in flight, never the reverse).
pub fn stop_writes() {
    PURGED.set(true);
    if let Some(db) = DATABASE.with(|d| d.borrow_mut().take()) {
        db.close();
    }
}

/// `evora:purge`: stop every write, close the presets database, then delete this engine's keys and
/// databases (`all`: every namespace; otherwise the account's and the bare names). Resolves once the
/// deletions have completed; `Err` holds the `evora:ack` code (`busy`: a deletion failed or is still
/// held back by a connection that does not close).
pub async fn purge(all: bool) -> Result<(), &'static str> {
    stop_writes();
    let ns = NS.with(|n| n.borrow().clone());
    let scope = if all { Scope::All } else { Scope::Current };
    if erase(scope, ns.as_deref(), "evora:purge").await { Ok(()) } else { Err("busy") }
}
