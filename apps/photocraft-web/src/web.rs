//! The browser shell: web `Services`, drag-and-drop, and the eframe web runner.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use photocraft_codecs::{ChannelLayout, EncodeOptions, Image};
use photocraft_doc::Document;
use photocraft_ui_egui::i18n;
use photocraft_ui_egui::served_fonts;
use photocraft_ui_egui::theme::ThemeKind;
use photocraft_ui_egui::{FileDialogAnswer, FileDialogRequest, PhotocraftApp, Services};
use wasm_bindgen::JsCast as _;

type Inbox = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// Everything File › Open reads: PhotoCraft, Photoshop, OpenRaster, Krita and Affinity documents, flat images, and
/// Photoshop brushes (.abr), gradients (.grd), layer styles (.asl) and swatches (.aco, .act, .ase), which go to the preset libraries.
/// Evora build (patch E7): HEIC/HEIF (heic, heif, hif) are left out, the `heif` feature being off
/// by default until the legal review (D-16).
const OPEN_EXTS: &[&str] = &[
    "pcraft", "pdn", "ora", "xcf", "xcfgz", "gz", "kra", "krz", "psd", "psb", "psdt", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "bmp", "tga", "ico",
    "qoi", "exr", "hdr", "pbm", "pgm", "ppm", "pam", "pfm", "dds", "fits", "fit", "fts", "dpx", "cin", "dcm", "dicom", "dc3", "iff", "lbm", "ilbm", "dng",
    "cr2", "cr3", "nef", "nrw", "arw", "pef", "orf", "rw2", "raf", "abr", "grd", "asl", "svg", "svgz", "aco", "act", "ase", "af", "afdesign", "afphoto",
    "afpub",
];
const SVG_EXTS: &[&str] = &["svg", "svgz"];
const CANVAS_ID: &str = "photocraft_canvas";

/// Fonts the host serves next to the page (`photocraft_ui_egui::served_fonts`): craft-fonts' manifest
/// format, each file relative to the site root. No manifest (a 404) means no served fonts.
const FONTS_MANIFEST: &str = "fonts/manifest.txt";

/// The served fonts: the manifest once read, and what the network has delivered.
#[derive(Default)]
struct ServedFonts {
    /// Font files by family, each with its Subresource Integrity value when the manifest has one.
    files: HashMap<String, Vec<(String, Option<String>)>>,
    /// Families fetched or being fetched: each one once per page load.
    fetched: HashSet<String>,
    /// Downloaded files waiting for the next frame: (family, bytes).
    arrived: Vec<(String, Vec<u8>)>,
}

type Served = Arc<Mutex<ServedFonts>>;

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let q = query();
        // Evora build (patch E4, with LUM-01): every query parameter is read by name, never by a
        // substring search of the whole query. `?host=evora` speaks the Evora host protocol (EHP v1,
        // module `ehp`) with the parent origin `evora-host.json` admits: the engine announces itself,
        // then touches no browser storage and does not start before `evora:init`. Without an
        // admitted parent the bridge stays mute and the engine does not start at all.
        let bridge = if query_param(&q, "host").as_deref() == Some("evora") {
            let Some(bridge) = crate::ehp::connect().await else {
                log::warn!("photocraft-web: ?host=evora without a parent admitted by evora-host.json: the engine does not start");
                return;
            };
            Some(bridge)
        } else {
            None
        };
        let init = match &bridge {
            Some(bridge) => Some(bridge.wait_init().await),
            None => None,
        };
        let (forced_language, theme) = match &init {
            // E3 and E2 through `evora:init`: the engine language the host chose (English when it
            // has none) and its theme, light or dark.
            Some(init) => (Some(crate::host_docs::engine_language(init.engine_lang.as_deref())), crate::host_docs::theme_kind(init.dark)),
            // Evora build (patch E3): the UI language the host imposes (`?lang=`). Patch E2: the
            // Evora theme, light or dark as the host asks (`?theme=`), instead of upstream's Pro
            // theme imposed at every start.
            None => (host_language(query_param(&q, "lang").as_deref()), host_theme(query_param(&q, "theme").as_deref())),
        };
        // Evora build (patch E5, corrected with E4): the storage namespace comes only from
        // `evora:init`, before any storage is read (never from the URL); with `storage.purge`, the
        // other namespaces and upstream's bare names are deleted first (HOST-PROTOCOL §7.7 b),
        // awaited within the storage deadline before the engine opens its own storage; an engine
        // document of the previous account still open stops writing and lets them go.
        // Without the bridge, upstream's names.
        match (&bridge, &init) {
            (Some(bridge), Some(init)) => {
                crate::storage_ns::scope(&init.storage_ns, bridge.has("storage.ns"));
                if bridge.has("storage.purge") {
                    crate::storage_ns::purge_others(&init.storage_ns).await;
                }
            }
            _ => crate::storage_ns::shared(),
        }
        let force_cpu = query_flag(&q, "cpu");
        let mut options = eframe::WebOptions::default();
        photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut options.wgpu_options.wgpu_setup);
        if query_flag(&q, "webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        // Hydrate before constructing an interactive session: a late load must never undo a
        // user deletion/reorder. Storage denial or timeout still opens a usable session.
        let (database, presets, mut preset_warnings) = match crate::indexed_presets::load().await {
            Ok((db, bridge, warnings)) => {
                // Evora build (patch E5): a purge that arrived meanwhile closes it at once.
                let db = crate::storage_ns::register_database(&db).then_some(db);
                (db, bridge, warnings)
            }
            Err(e) => {
                let bridge = crate::preset_bridge::Bridge::default();
                bridge.unavailable(e);
                (None, bridge, Vec::new())
            }
        };
        let pen_target = canvas.clone();
        let shell_bridge = bridge.clone();
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    PhotocraftApp::setup_context(&cc.egui_ctx, theme);
                    let inbox: Inbox = Arc::default();
                    let place_inbox: Inbox = Arc::default();
                    let served: Served = Arc::default();
                    load_font_manifest(served.clone(), cc.egui_ctx.clone());
                    let (session, warnings) = crate::preset_bridge::session(presets.clone());
                    preset_warnings.extend(warnings);
                    let drag = listen_drag(&pen_target);
                    let mut app = PhotocraftApp::new(session, services(inbox.clone(), place_inbox.clone(), drag_pos_service(pen_target.clone(), drag), shell_bridge.clone()));
                    if !preset_warnings.is_empty() {
                        photocraft_ui_egui::notices::post(&mut app, i18n::t("Some brush presets could not be loaded"), preset_warnings, true, None);
                    }
                    listen_pen(&pen_target, app.stylus.feed.clone());
                    app.set_theme(&cc.egui_ctx, theme);
                    if let Some(code) = forced_language
                        && let Err(e) = app.run("prefs.set", serde_json::json!({"values": {"interface.language": code}}))
                    {
                        log::warn!("photocraft-web: host language {code} not applied: {e}");
                    }
                    // Evora build (patch LUM-10): the renderer, reported by `diag.stats`.
                    let renderer = match (&cc.wgpu_render_state, force_cpu) {
                        (Some(rs), false) => format!("{:?}", rs.adapter.get_info().backend),
                        _ => "cpu".to_owned(),
                    };
                    if let Some(rs) = cc.wgpu_render_state.clone()
                        && !force_cpu
                    {
                        log::info!("photocraft-web: wgpu backend {:?}", rs.adapter.get_info().backend);
                        app.set_wgpu(rs);
                    }
                    let unsaved = Arc::new(AtomicBool::new(false));
                    guard_unload(unsaved.clone());
                    // Evora build (patch E4): the host's requests reach the app between frames.
                    let host = shell_bridge.map(|bridge| {
                        bridge.attach(cc.egui_ctx.clone());
                        crate::host_docs::HostDocs::new(bridge, renderer)
                    });
                    Ok(Box::new(WebShell { app, inbox, place_inbox, unsaved, served, database, presets, host }))
                }),
            )
            .await;
        // Evora build (patch E4): `evora:init` is acknowledged once the engine runs, or answered by
        // `evora:failed` when it cannot start (HOST-PROTOCOL §4.5).
        if let (Some(bridge), Some(init)) = (&bridge, &init) {
            match &result {
                Ok(()) => bridge.ack(&init.id, true, None),
                Err(e) => {
                    let text = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    bridge.failed(if text.contains("WebGL") || text.contains("WebGPU") { "no_webgl2" } else { "init" });
                }
            }
        }
        if let Some(el) = document.get_element_by_id("photocraft_loading") {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>Lumbra failed to start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>")),
            }
        }
    });
}

/// Pen pressure, tilt, twist and the eraser button from Pointer Events (eframe forwards none of them for pens) into
/// the app's stylus feed. The sample is kept through `pointerup` so the stroke's last points keep
/// their pressure; hovering, a mouse, or leaving the canvas clears it.
fn listen_pen(target: &web_sys::HtmlCanvasElement, feed: photocraft_ui_egui::stylus::StylusFeed) {
    use photocraft_ui_egui::stylus::PenSample;
    use wasm_bindgen::closure::Closure;
    for kind in ["pointerdown", "pointermove", "pointerup", "pointercancel", "pointerleave"] {
        let feed = feed.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
            let ty = e.type_();
            if ty == "pointerup" && e.pointer_type() == "pen" {
                return;
            }
            let pen = e.pointer_type() == "pen" && e.buttons() != 0 && ty != "pointercancel" && ty != "pointerleave";
            // W3C Pointer Events: `buttons` bit 5 (32) is the pen's eraser.
            let eraser = e.buttons() & 32 != 0;
            feed.set(pen.then(|| PenSample {
                pressure: e.pressure(),
                tilt_x: e.tilt_x() as f32,
                tilt_y: e.tilt_y() as f32,
                rotation: e.twist() as f32,
                eraser,
            }));
        });
        if target.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref()).is_ok() {
            cb.forget();
        }
    }
}

/// The pointer in a drag over the page, in CSS pixels relative to the viewport (`clientX`,
/// `clientY`): the last `dragover` or `drop` event's.
type DragPos = Rc<Cell<Option<(f32, f32)>>>;

/// Follows files dragged over the canvas. eframe's drop events carry no pointer position and the
/// page gets no pointer events during a drag, so this is where a drop lands: on the document's
/// canvas it places the files, elsewhere it opens them (#5749).
fn listen_drag(target: &web_sys::HtmlCanvasElement) -> DragPos {
    use wasm_bindgen::closure::Closure;
    let pos = DragPos::default();
    for kind in ["dragover", "drop"] {
        let pos = pos.clone();
        // A DragEvent is a MouseEvent: only its client position is read.
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| pos.set(Some((e.client_x() as f32, e.client_y() as f32))));
        if target.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref()).is_ok() {
            cb.forget();
        }
    }
    pos
}

/// `Services::cursor_pos` for the web: the last drag position in egui points, mapped as eframe
/// maps pointer events (relative to the canvas, divided by the zoom factor).
fn drag_pos_service(canvas: web_sys::HtmlCanvasElement, pos: DragPos) -> photocraft_ui_egui::CursorPosFn {
    Box::new(move |ctx: &egui::Context| {
        let (x, y) = pos.get()?;
        let rect = canvas.get_bounding_client_rect();
        let zoom = ctx.zoom_factor();
        (zoom.is_finite() && zoom > 0.0).then(|| egui::pos2((x - rect.left() as f32) / zoom, (y - rect.top() as f32) / zoom))
    })
}

/// Closing or reloading the tab while a document has unsaved changes asks first, as closing the
/// desktop window does: the browser shows its own "Leave site?" prompt (#1380). `unsaved` is
/// refreshed every frame by [`WebShell`].
fn guard_unload(unsaved: Arc<AtomicBool>) {
    use wasm_bindgen::closure::Closure;
    let Some(window) = web_sys::window() else { return };
    let cb = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(move |e: web_sys::BeforeUnloadEvent| {
        if unsaved.load(Ordering::Relaxed) {
            e.prevent_default();
            // Older browsers show the prompt only when a return value is set.
            e.set_return_value("");
        }
    });
    if window.add_event_listener_with_callback("beforeunload", cb.as_ref().unchecked_ref()).is_ok() {
        cb.forget();
    }
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}

/// Evora build (LUM-01, with patch E4): is the flag `key` present (`?cpu`, `?cpu=1`)? Exact key
/// match; upstream searched the whole query for a substring, so a value such as a file or language
/// name containing "cpu" or "webgl" switched the renderer.
fn query_flag(query: &str, key: &str) -> bool {
    query.trim_start_matches('?').split('&').any(|pair| pair.split_once('=').map_or(pair, |(k, _)| k) == key)
}

/// The value of `key` in a query string (`?a=1&lang=fr`): exact key match (not a substring
/// search), `+` read as a space, percent-decoded. `None` when the key is absent or undecodable.
fn query_param(query: &str, key: &str) -> Option<String> {
    query.trim_start_matches('?').split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k != key {
            return None;
        }
        js_sys::decode_uri_component(&v.replace('+', " ")).ok().map(String::from)
    })
}

/// Evora build (patch E2): Evora's light theme for `?theme=light`, its dark theme otherwise.
fn host_theme(value: Option<&str>) -> ThemeKind {
    match value.map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("light") => ThemeKind::EvoraLight,
        _ => ThemeKind::Evora,
    }
}

/// Evora build (patch E3): the engine language for the host's language tag (BCP 47, as Evora
/// passes it: `fr`, `de-CH`, `zh-Hans`, `pt-BR`…). `zh-Hans`, `zh-CN` and `zh-SG` map to
/// `zh-hans`; `zh-Hant`, `zh-TW`, `zh-HK` and `zh-MO` to `zh-hant`; other tags to their exact
/// catalog, else to their primary language. `None` (the preference is left alone) for a missing or
/// malformed tag, a language without a catalog (Arabic, Hindi: the host shows its own notice), and
/// a bare `pt` or any Portuguese tag but `pt-BR`: no variant is chosen until decision D-23.
pub(crate) fn host_language(tag: Option<&str>) -> Option<&'static str> {
    let tag = tag?.trim().replace('_', "-").to_ascii_lowercase();
    if tag.is_empty() || tag.len() > 35 || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let primary = tag.split('-').next().unwrap_or_default();
    let code = match tag.as_str() {
        "zh-hans" | "zh-cn" | "zh-sg" | "zh-hans-cn" | "zh-hans-sg" => "zh-hans",
        "zh-hant" | "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant-tw" | "zh-hant-hk" | "zh-hant-mo" => "zh-hant",
        "pt-br" => "pt-br",
        _ if primary == "pt" || primary == "zh" => return None,
        other => other,
    };
    i18n::Lang::from_code(code).or_else(|| i18n::Lang::from_code(primary)).map(|lang| lang.code())
}

/// Reads the served font manifest, if the host has one, and lists its families in the font menus.
fn load_font_manifest(served: Served, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        // Most hosts serve no fonts: a missing manifest is the normal case, not an error.
        let Ok(bytes) = fetch_bytes(FONTS_MANIFEST, None).await else { return };
        let text = String::from_utf8_lossy(&bytes);
        // A host that answers unknown paths with its HTML page (single-page app fallback).
        if text.trim_start().starts_with('<') {
            return;
        }
        let (fonts, skipped) = served_fonts::parse_manifest(&text);
        for s in skipped {
            log::warn!("{FONTS_MANIFEST}: skipped {s}");
        }
        let mut files: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
        for f in &fonts {
            files.entry(f.family.clone()).or_default().push((f.file.clone(), f.integrity.clone()));
        }
        log::info!("photocraft-web: {} served font families", files.len());
        served.lock().unwrap_or_else(|e| e.into_inner()).files = files;
        // The families for the font menus, and the script fallbacks (the manifest's `scripts`).
        served_fonts::add_fonts(&fonts);
        // Fetch now the fallbacks of the scripts the browser's languages use (an Arabic reader gets
        // the Arabic font before typing); the rest is fetched when text or a font menu needs it.
        served_fonts::request_for_languages(browser_languages().iter().map(String::as_str));
        ctx.request_repaint();
    });
}

/// The browser's preferred languages (`navigator.languages`, e.g. `["ar-EG", "en"]`).
fn browser_languages() -> Vec<String> {
    web_sys::window().map(|w| w.navigator().languages().iter().filter_map(|l| l.as_string()).collect()).unwrap_or_default()
}

/// GET `url` (a relative path on the same origin) and read the whole body. With `integrity` (a
/// Subresource Integrity value), the browser rejects a body whose hash differs.
async fn fetch_bytes(url: &str, integrity: Option<&str>) -> Result<Vec<u8>, String> {
    use wasm_bindgen_futures::JsFuture;
    if !served_fonts::is_relative_path(url) {
        return Err("url must be a relative path".into());
    }
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let init = web_sys::RequestInit::new();
    init.set_mode(web_sys::RequestMode::SameOrigin);
    if let Some(sri) = integrity {
        init.set_integrity(sri);
    }
    let request = window.fetch_with_str_and_init(url, &init);
    let resp: web_sys::Response = JsFuture::from(request).await.map_err(js)?.dyn_into().map_err(|_| "not a Response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body = JsFuture::from(resp.array_buffer().map_err(js)?).await.map_err(js)?;
    Ok(js_sys::Uint8Array::new(&body).to_vec())
}

/// Wraps the app to read dropped files asynchronously (browsers can't read them synchronously,
/// so the app's own drop path can't handle them) and feed them through the inboxes.
struct WebShell {
    app: PhotocraftApp,
    /// Dropped files to open as documents.
    inbox: Inbox,
    /// Dropped files to place on the active document's canvas.
    place_inbox: Inbox,
    /// Read by the `beforeunload` listener ([`guard_unload`]).
    unsaved: Arc<AtomicBool>,
    served: Served,
    database: Option<web_sys::IdbDatabase>,
    presets: crate::preset_bridge::Bridge,
    /// Evora build (patch E4): the host's documents, with `?host=evora`.
    host: Option<crate::host_docs::HostDocs>,
}

impl WebShell {
    fn save_presets(&mut self, ctx: &egui::Context) {
        // Evora build (patch E5): after `evora:purge` nothing is written back.
        if crate::storage_ns::purged() {
            self.unsaved.store(false, Ordering::Relaxed);
            return;
        }
        if let Some(db) = self.database.clone()
            && let Some(batch) = self.presets.begin()
        {
            let presets = self.presets.clone();
            let wake = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = crate::indexed_presets::write(&db, &batch).await;
                // Evora build (patch E5): a write cut short by `evora:purge` is expected.
                if let Err(e) = &result
                    && !crate::storage_ns::purged()
                {
                    log::error!("Could not save browser presets: {e}");
                }
                presets.finish(batch, result);
                wake.request_repaint();
            });
        }
        self.unsaved.store(self.app.has_unsaved_work() || self.presets.unsaved(), Ordering::Relaxed);
    }

    /// Fetches the served families asked for (picked in a font menu, or needed by a layout) and
    /// installs the files that arrived.
    fn serve_fonts(&mut self, ctx: &egui::Context) {
        let requested = served_fonts::take_requests();
        let (fetch, arrived) = {
            let mut s = self.served.lock().unwrap_or_else(|e| e.into_inner());
            let mut fetch = Vec::new();
            for family in requested {
                if let Some(files) = s.files.get(&family).cloned()
                    && s.fetched.insert(family.clone())
                {
                    fetch.extend(files.into_iter().map(|(file, integrity)| (family.clone(), file, integrity)));
                }
            }
            (fetch, std::mem::take(&mut s.arrived))
        };
        for (family, file, integrity) in fetch {
            let served = self.served.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_bytes(&file, integrity.as_deref()).await {
                    Ok(bytes) => {
                        served.lock().unwrap_or_else(|e| e.into_inner()).arrived.push((family, bytes));
                        ctx.request_repaint();
                    }
                    Err(e) => log::error!("couldn't fetch served font {file}: {e}"),
                }
            });
        }
        served_fonts::install(&mut self.app, arrived);
    }
}

impl eframe::App for WebShell {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // A tap reaches eframe as Pointer Events and Touch Events: keep one press per tap, or the
        // twin press closes the menu the first one opened (#2095).
        photocraft_ui_egui::touch_twins::drop_twins(ctx, raw_input);
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        if !dropped.is_empty() {
            let names: Vec<String> =
                dropped.iter().map(|f| f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped".into())).collect();
            // Decided now, where the drop landed: on the document's canvas the files are placed
            // as layers (as on the desktop), elsewhere opened.
            let places = self.app.dropped_on_canvas(ctx, &names);
            let files: Vec<_> = dropped.into_iter().zip(names).zip(places).collect();
            let (inbox, place_inbox) = (self.inbox.clone(), self.place_inbox.clone());
            let ctx = ctx.clone();
            // One after another, so they open or place in the order they were dropped.
            wasm_bindgen_futures::spawn_local(async move {
                for ((f, name), place) in files {
                    match f.bytes_async().await {
                        Ok(bytes) => {
                            let queue = if place { &place_inbox } else { &inbox };
                            queue.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                            ctx.request_repaint();
                        }
                        Err(e) => log::error!("couldn't read dropped file {name}: {e}"),
                    }
                }
            });
        }
        self.serve_fonts(ctx);
        if let Some(host) = self.host.as_mut() {
            host.apply(&mut self.app, ctx);
        }
        self.app.logic(ctx, frame);
        self.save_presets(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Only while a preset change is unsaved. Storage the browser denies from the start (a
        // sandboxed iframe, blocked site data) loses nothing until the first change, and that
        // change brings the banner with the reason.
        if self.presets.unsaved()
            && let Some(error) = self.presets.error()
        {
            egui::Panel::top("browser_preset_storage").show(ui, |ui| {
                let tokens = photocraft_ui_egui::theme::Tokens::for_kind(self.app.ui.theme);
                // Put retry first so floating brush windows cannot cover the control.
                ui.horizontal_wrapped(|ui| {
                    if self.database.is_some() && ui.button(i18n::t("Retry saving presets")).clicked() {
                        self.presets.retry();
                        ui.ctx().request_repaint();
                    }
                    ui.colored_label(tokens.warning, i18n::fmt(i18n::t("Brush preset changes are not saved: {error}"), &[("error", &error)]));
                });
                ui.label(if self.database.is_some() {
                    i18n::t("Keep this tab open to retain your brushes.")
                } else {
                    i18n::t("Browser storage is unavailable; brushes are session-only. Keep this tab open to retain them.")
                });
            });
        }
        self.app.ui(ui, frame);
        self.save_presets(ui.ctx());
        if let Some(host) = self.host.as_mut() {
            host.report(&self.app);
        }
    }
}

fn services(inbox: Inbox, place_inbox: Inbox, cursor_pos: photocraft_ui_egui::CursorPosFn, bridge: Option<crate::ehp::Bridge>) -> Services {
    // Evora build (patch E4): with the bridge, the host's document is saved to the host and nothing
    // is downloaded (`host_docs::write`); without it, upstream's download.
    let write_unconfirmed = bridge.is_none();
    let write: photocraft_ui_egui::WriteFn = match bridge {
        Some(bridge) => Box::new(move |path: &str, bytes: &[u8]| crate::host_docs::write(&bridge, path, bytes)),
        None => Box::new(|path: &str, bytes: &[u8]| download(path, bytes)),
    };
    Services {
        screen_pick: screen_color_service(),
        import: Some(Box::new(|name: &str, bytes: &[u8], opts: &photocraft_io::ImportOptions| {
            photocraft_io::import_with_options(name, bytes, opts).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
        })),
        export: Some(Box::new(|doc: &Document, path: &str, settings: &photocraft_ui_egui::ExportSettings| {
            photocraft_io::export(doc, path, &settings.io_options()).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
        })),
        file_dialog: Some(Box::new(|request, _parent, reply| match request {
            // The browser's file picker hands over the files' contents, not paths. File › Open
            // asks for several files (`multiple`), each opened in its own tab.
            FileDialogRequest::Open { multiple, extensions, .. } => wasm_bindgen_futures::spawn_local(async move {
                let dialog = if let Some(exts) = extensions {
                    rfd::AsyncFileDialog::new().add_filter("Supported Files", &exts)
                } else {
                    rfd::AsyncFileDialog::new().add_filter("All Formats", OPEN_EXTS).add_filter("SVG", SVG_EXTS)
                };
                let answer = if multiple {
                    match dialog.pick_files().await {
                        Some(picked) if !picked.is_empty() => {
                            let mut files = Vec::with_capacity(picked.len());
                            for file in picked {
                                files.push((file.file_name(), file.read().await));
                            }
                            Some(FileDialogAnswer::ContentsMany(files))
                        }
                        _ => None,
                    }
                } else {
                    match dialog.pick_file().await {
                        Some(file) => Some(FileDialogAnswer::Contents(file.file_name(), file.read().await)),
                        None => None,
                    }
                };
                reply.send(answer);
            }),
            // Browser folder handles cannot be represented as filesystem paths for engine commands.
            FileDialogRequest::Folder { .. } => reply.send(None),
            // No save dialog on the web: the suggested name becomes the download name.
            FileDialogRequest::Save { suggested } => {
                let name = std::path::Path::new(&suggested).file_name().map_or_else(|| suggested.clone(), |n| n.to_string_lossy().to_string());
                reply.send(Some(FileDialogAnswer::SaveTo(name)));
            }
        })),
        write: Some(write),
        // A download may be cancelled and never reports back: it doesn't save the document.
        // Evora build (patch E4): the bridge's write does count as written, since `host_docs`
        // binds each save to the revision it wrote and keeps the document modified until the
        // host acknowledges it (doc.saved-ack, patch LUM-04).
        write_unconfirmed,
        encode_png: Some(Box::new(|w, h, rgba| {
            let img = Image::from_u8(w, h, ChannelLayout::Rgba, rgba.to_vec()).map_err(|e| e.to_string())?;
            photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &EncodeOptions::default()).map_err(|e| e.to_string())
        })),
        inbox: Some(inbox),
        place_inbox: Some(place_inbox),
        cursor_pos: Some(cursor_pos),
        // Evora build (patch E7): the engine opens no page outside its origin; navigation belongs
        // to the host shell. A link the UI would open is logged and dropped (reported as handled,
        // so egui's own `open_url` fallback doesn't open it either).
        open_url: Some(Box::new(|url: &str| {
            log::warn!("photocraft-web: external link not opened (Evora build): {url}");
            Ok(())
        })),
        // Preferences live in the browser's localStorage, under the host's storage namespace
        // (patch E5). A refused or purged namespace reads nothing and writes nothing.
        load_prefs: Some(Box::new(|| local_storage()?.get_item(&crate::storage_ns::key(crate::storage_ns::PREFS_KEY)?).ok().flatten())),
        save_prefs: Some(Box::new(|text: &str| {
            let Some(key) = crate::storage_ns::key(crate::storage_ns::PREFS_KEY) else { return Ok(()) };
            local_storage().ok_or("no localStorage")?.set_item(&key, text).map_err(|e| format!("{e:?}"))
        })),
        ..Default::default()
    }
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Trigger a browser download of `bytes` named after the last component of `path`.
fn download(path: &str, bytes: &[u8]) -> Result<(), String> {
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "photocraft".into());
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime_for(&name));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(&name);
    a.style().set_property("display", "none").map_err(js)?;
    let body = document.body().ok_or("no body")?;
    body.append_child(&a).map_err(js)?;
    a.click();
    a.remove();
    // Revoke after the click has been dispatched; the download keeps its own reference.
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        web_sys::Url::revoke_object_url(&url).ok();
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000).map_err(js)?;
    Ok(())
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("tif" | "tiff") => "image/tiff",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("psd" | "psb") => "image/vnd.adobe.photoshop",
        Some("ora") => "image/openraster",
        Some("kra" | "krz") => "application/x-krita",
        _ => "application/octet-stream",
    }
}

/// Rust bindings to the browser's user-activated EyeDropper API; unsupported browsers keep
/// the document picker and its normal canvas zoom. The browser owns its screen magnifier.
fn screen_color_service() -> Option<photocraft_ui_egui::screen_picker::Service> {
    use js_sys::{Function, Reflect};
    use photocraft_ui_egui::screen_picker::{Capture, Pending};
    use wasm_bindgen::{JsValue, closure::Closure};
    let window = web_sys::window()?;
    let constructor = Reflect::get(&window, &JsValue::from_str("EyeDropper")).ok()?.dyn_into::<Function>().ok()?;
    Some(Box::new(move |ctx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = (|| -> Result<_, String> {
            let eye = Reflect::construct(&constructor, &js_sys::Array::new()).map_err(|e| format!("Could not start browser eyedropper: {e:?}"))?;
            let controller = web_sys::AbortController::new().map_err(|e| format!("Could not create eyedropper cancellation: {e:?}"))?;
            let options = js_sys::Object::new();
            Reflect::set(&options, &JsValue::from_str("signal"), &controller.signal()).map_err(|e| format!("Could not set eyedropper options: {e:?}"))?;
            let open = Reflect::get(&eye, &JsValue::from_str("open"))
                .map_err(|e| format!("Browser eyedropper has no open method: {e:?}"))?
                .dyn_into::<Function>()
                .map_err(|_| "Invalid browser eyedropper method")?;
            let promise = open
                .call1(&eye, &options)
                .map_err(|e| format!("Could not open browser eyedropper: {e:?}"))?
                .dyn_into::<js_sys::Promise>()
                .map_err(|_| "Browser eyedropper returned no promise")?;
            let stop = cancelled.clone();
            let abort = controller.clone();
            let timer = Closure::<dyn FnMut()>::new(move || {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    abort.abort();
                }
            });
            let interval = window.set_interval_with_callback_and_timeout_and_arguments_0(timer.as_ref().unchecked_ref(), 50).map_err(|e| {
                controller.abort();
                format!("Could not watch eyedropper cancellation: {e:?}")
            })?;
            Ok((promise, timer, interval))
        })();
        match started {
            Err(e) => {
                let _ = tx.send(Err(e));
            }
            Ok((promise, timer, interval)) => {
                let wake = ctx.clone();
                let window = window.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = match wasm_bindgen_futures::JsFuture::from(promise).await {
                        Ok(value) => Reflect::get(&value, &JsValue::from_str("sRGBHex"))
                            .ok()
                            .and_then(|v| v.as_string())
                            .and_then(|h| photocraft_ui_egui::color_picker_ui::parse_hex(&h))
                            .map(|rgb| Capture::Color(Some(rgb)))
                            .ok_or_else(|| "Browser eyedropper returned an invalid color".into()),
                        Err(e) if Reflect::get(&e, &JsValue::from_str("name")).ok().and_then(|v| v.as_string()).as_deref() == Some("AbortError") => {
                            Ok(Capture::Color(None))
                        }
                        Err(e) => Err(format!("Browser screen color selection failed: {e:?}")),
                    };
                    window.clear_interval_with_handle(interval);
                    drop(timer);
                    let _ = tx.send(result);
                    wake.request_repaint();
                });
            }
        }
        Pending { receiver: rx, cancelled }
    }))
}
