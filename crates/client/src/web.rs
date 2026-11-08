//! Browser-only glue.

use crate::route::Route;
use crate::shell::Shortcut;
use eframe::egui::Context;
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

thread_local! {
    static SHORTCUTS: RefCell<Vec<Shortcut>> = const { RefCell::new(Vec::new()) };
}

fn window() -> web_sys::Window {
    web_sys::window().expect("running in a browser")
}

pub fn canvas() -> web_sys::HtmlCanvasElement {
    window()
        .document()
        .and_then(|document| document.get_element_by_id("tracker"))
        .expect("index.html has the canvas")
        .unchecked_into()
}

pub fn current_route() -> Option<Route> {
    Route::parse(&window().location().pathname().unwrap_or_default())
}

pub fn is_small_window() -> bool {
    window()
        .location()
        .search()
        .unwrap_or_default()
        .contains("isSmallWindow")
}

pub fn websocket_url() -> String {
    let location = window().location();
    let scheme = match location.protocol().as_deref() {
        Ok("https:") => "wss",
        _ => "ws",
    };
    format!("{scheme}://{}/ws", location.host().unwrap_or_default())
}

pub fn navigate(route: &Route) {
    let _ = window().location().assign(&route.path());
}

pub fn set_title(title: &str) {
    if let Some(document) = window().document() {
        document.set_title(title);
    }
}

/// The browser's "open in a small window" popup, sized to the tracker.
pub fn open_small_window(width: f32, height: f32) {
    let window = window();
    let location = window.location();
    let url = format!("{}?isSmallWindow=true", location.href().unwrap_or_default());
    let features = format!(
        "width={width},height={height},toolbar=no,location=no,status=no,menubar=no,scrollbars=no,resizable=yes"
    );
    let _ = window.open_with_url_and_target_and_features(&url, "_blank", &features);
}

/// eframe stops key events from propagating, and these combinations also belong
/// to the browser (hard reload, address bar), so they are caught in the capture
/// phase, before anyone else sees them.
pub fn install_shortcuts(ctx: Context) {
    let listener =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            if !event.ctrl_key() {
                return;
            }
            let shortcut = match (event.key().to_lowercase().as_str(), event.shift_key()) {
                ("r", true) => Shortcut::Reset,
                ("l", true) => Shortcut::GoLive,
                ("l", false) => Shortcut::SmallWindow,
                _ => return,
            };
            event.prevent_default();
            SHORTCUTS.with(|pending| pending.borrow_mut().push(shortcut));
            ctx.request_repaint();
        });
    let _ = window().add_event_listener_with_callback_and_bool(
        "keydown",
        listener.as_ref().unchecked_ref(),
        true,
    );
    listener.forget(); // Lives as long as the page.
}

pub fn take_shortcuts() -> Vec<Shortcut> {
    SHORTCUTS.with(|pending| std::mem::take(&mut *pending.borrow_mut()))
}
