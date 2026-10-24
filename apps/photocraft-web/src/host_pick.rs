//! Evora build (patch LUM-02): the engine's file pickers become Evora Drive's (capability
//! `pick.drive`, Evora's HOST-PROTOCOL §4.4, §7.6).
//!
//! Under the bridge, every file the engine asks to read comes from the host, never from the
//! browser's own picker (no `rfd` fallback, E-26): File › Open sends `evora:pick {purpose:
//! "open"}`, every other reader (File › Place Embedded, presets, lookup tables, shortcut sets,
//! scripts) `evora:pick {purpose: "import"}`, with what it accepts. The host answers with one
//! `evora:file` per chosen file (its bytes, under `maxDocumentBytes`), then closes the exchange
//! with `evora:ack {ok: true}`; the engine then reads the files as from its own picker.
//! `evora:ack {ok: false}` (cancelled, unsupported…) answers the picker with nothing, as a
//! cancelled dialog does. For an open, this single-document engine leaves the choice to the host,
//! which loads the chosen document in its editor page (§7.6); files it hands over all the same
//! open as the engine's own documents. Without `pick.drive` effective, a picker answers nothing
//! at once. One exchange at a time, as the app shows one file dialog at a time.

use std::cell::RefCell;

use photocraft_ui_egui::{FileDialogAnswer, FileDialogReply};
use wasm_bindgen::{JsCast as _, JsValue};

use crate::ehp::{Bridge, get, object, only_keys};

/// What File › Open accepts: images (raw camera files included, by their MIME type) and the
/// document formats the engine opens.
const DOCUMENTS: [&str; 11] = ["image/*", ".pcraft", ".psd", ".psb", ".kra", ".krz", ".pdn", ".af", ".afphoto", ".afdesign", ".afpub"];
/// What a reader without a filter accepts (a shortcut set, notes, a placed file): any file the
/// host keeps, the engine reading its content itself.
const ANY_FILE: [&str; 3] = ["application/*", "image/*", "text/*"];
/// `error` of `evora:file` (§4.3).
const FILE_ERRORS: [&str; 4] = ["not_found", "forbidden", "too_large", "unsupported"];

/// The picker in progress: the `evora:pick` id, the files received so far, and the dialog to answer.
struct Pick {
    id: String,
    multiple: bool,
    files: Vec<(String, Vec<u8>)>,
    reply: FileDialogReply,
}

thread_local! {
    static PICK: RefCell<Option<Pick>> = const { RefCell::new(None) };
}

/// `FileDialogRequest::Open` under the bridge: `evora:pick`, or no file when the host has no
/// picker (or one is already in progress).
pub fn open(bridge: &Bridge, multiple: bool, extensions: Option<Vec<String>>, reply: FileDialogReply) {
    if !bridge.has("pick.drive") {
        log::warn!("photocraft-web: the host offers no file picker (pick.drive): nothing to read");
        reply.send(None);
        return;
    }
    if PICK.with(|p| p.borrow().is_some()) {
        reply.send(None);
        return;
    }
    // File › Open is the only reader of several files without a filter.
    let purpose = if multiple && extensions.is_none() { "open" } else { "import" };
    let accept = js_sys::Array::new();
    for a in accept_list(extensions.as_deref(), multiple) {
        accept.push(&JsValue::from_str(&a));
    }
    let payload = object(&[("purpose", purpose.into()), ("accept", accept.into()), ("multiple", multiple.into())]);
    let Some(id) = payload.and_then(|p| bridge.post("evora:pick", None, &p)) else {
        reply.send(None);
        return;
    };
    PICK.with(|p| *p.borrow_mut() = Some(Pick { id, multiple, files: Vec::new(), reply }));
}

/// `accept` of `evora:pick` (§4.4): the reader's extensions as `.ext`, else [`DOCUMENTS`] for File
/// › Open and [`ANY_FILE`] for another reader.
fn accept_list(extensions: Option<&[String]>, multiple: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in extensions.unwrap_or(&[]) {
        let e = e.trim_start_matches('.').to_ascii_lowercase();
        let item = format!(".{e}");
        if !e.is_empty() && e.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && !out.contains(&item) {
            out.push(item);
        }
    }
    if out.is_empty() {
        let fallback: &[&str] = if multiple { &DOCUMENTS } else { &ANY_FILE };
        out = fallback.iter().map(|s| (*s).to_string()).collect();
    }
    out
}

/// `evora:file {path, name?, bytes | url | error}` answering the picker `re` (§4.3): a file's bytes
/// (an `ArrayBuffer` of 1 byte up to `max_bytes`) are kept for the closing acknowledgement; an
/// error or an URL (`media.ranges` is not declared) is logged and skipped. A malformed message is
/// ignored (§5.2).
pub fn on_file(re: &str, p: &JsValue, max_bytes: f64) {
    if !only_keys(p, &["path", "name", "bytes", "url", "error"]) {
        return;
    }
    let Some(path) = get(p, "path").as_string().filter(|s| is_path(s)) else { return };
    let name = get(p, "name");
    let name = if name.is_undefined() {
        path.rsplit('/').next().unwrap_or_default().to_string()
    } else {
        match name.as_string().filter(|n| crate::ehp::is_name(n)) {
            Some(n) => n,
            None => return,
        }
    };
    let given: Vec<&str> = ["bytes", "url", "error"].into_iter().filter(|k| !get(p, k).is_undefined()).collect();
    let file = match given.as_slice() {
        ["bytes"] => {
            let bytes = get(p, "bytes");
            if !bytes.is_instance_of::<js_sys::ArrayBuffer>() {
                return;
            }
            let length = f64::from(bytes.unchecked_ref::<js_sys::ArrayBuffer>().byte_length());
            if length < 1.0 || length > max_bytes {
                log::warn!("photocraft-web: a picked file is empty or over the document limit: skipped");
                return;
            }
            Some(js_sys::Uint8Array::new(&bytes).to_vec())
        }
        ["error"] => {
            if !get(p, "error").as_string().is_some_and(|e| FILE_ERRORS.contains(&e.as_str())) {
                return;
            }
            None
        }
        ["url"] => {
            if get(p, "url").as_string().is_none() {
                return;
            }
            None
        }
        _ => return,
    };
    PICK.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(pick) = cell.as_mut().filter(|k| k.id == re) else { return };
        match file {
            Some(bytes) if !name.is_empty() => pick.files.push((name, bytes)),
            _ => log::warn!("photocraft-web: the host could not hand over a picked file: skipped"),
        }
    });
}

/// `evora:ack` closing the picker `re` (checked by the bridge): `ok` hands the files received to
/// the dialog, anything else answers it with nothing. `false` when `re` is not the picker's.
pub fn settle(re: &str, ok: bool, code: Option<&str>) -> bool {
    let pick = PICK.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.as_ref().is_some_and(|k| k.id == re) { cell.take() } else { None }
    });
    let Some(pick) = pick else { return false };
    if !ok {
        if code != Some("cancelled") {
            log::warn!("photocraft-web: the host's picker gave nothing ({})", code.unwrap_or_default());
        }
        pick.reply.send(None);
        return true;
    }
    let answer = if pick.files.is_empty() {
        None
    } else if pick.multiple {
        Some(FileDialogAnswer::ContentsMany(pick.files))
    } else {
        pick.files.into_iter().next().map(|(name, bytes)| FileDialogAnswer::Contents(name, bytes))
    };
    pick.reply.send(answer);
    true
}

/// `path` of §4.2: `/`, no control character, no `..` segment.
fn is_path(p: &str) -> bool {
    p.starts_with('/') && !p.chars().any(char::is_control) && !p.split('/').any(|s| s == "..")
}
