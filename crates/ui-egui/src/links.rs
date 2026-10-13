//! External links. Evora build (patch E1): the upstream community and project links (Help menu,
//! About dialog, start screen and title bar) are removed, so the app neither shows nor opens a
//! third-party brand's site, chat or tracker. What is left is the helper that opens a URL
//! (contributor names in About › Contributors) and an empty Help-menu link table, kept so the
//! menu code stays as upstream wrote it.

use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Help-menu link commands: (id, url). Empty in the Evora build (patch E1).
pub const COMMANDS: &[(&str, &str)] = &[];

pub fn url_for(id: &str) -> Option<&'static str> {
    COMMANDS.iter().find(|c| c.0 == id).map(|c| c.1)
}

/// Open `url` in the system browser (a new tab on the web) and note it in the status bar. Prefers
/// the platform `open_url` service (reliable on Windows/macOS/Linux); falls back to `ctx.open_url`.
pub fn open(app: &mut PhotocraftApp, ctx: &egui::Context, url: &str) -> Value {
    let opened = app.services.open_url.as_ref().map(|f| f(url).is_ok()).unwrap_or(false);
    if !opened {
        ctx.open_url(egui::OpenUrl::new_tab(url));
    }
    let template = "Opened {url}";
    app.ui.status = crate::i18n::fmt(tl!(template), &[("url", url)]);
    json!({"url": url})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_goes_through_the_platform_service() {
        use std::sync::{Arc, Mutex};
        let opened: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = opened.clone();
        let services = crate::Services {
            open_url: Some(Box::new(move |u: &str| {
                rec.lock().unwrap().push(u.to_string());
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        let ctx = egui::Context::default();
        open(&mut app, &ctx, "https://example.org/");
        assert_eq!(opened.lock().unwrap().last().map(String::as_str), Some("https://example.org/"));
    }

    #[test]
    fn help_menu_has_no_external_links() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let help: Vec<String> = crate::menus::menu_items(&app).into_iter().filter(|i| i.path == ["Help"]).map(|i| i.id).collect();
        assert_eq!(help, ["help.systemInfo", "help.about"]);
        assert!(COMMANDS.is_empty());
    }
}
