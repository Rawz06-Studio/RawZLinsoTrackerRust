//! The top-level screen: loads pages, and hosts the Go Live and error dialogs.

use crate::app::TrackerApp;
use crate::assets::Assets;
use crate::home::{self, Home};
use crate::net::Net;
use crate::route::Route;
use crate::source::{Bundle, Source};
use eframe::egui::{self, Color32, Context, Key, Modal, TextEdit, Vec2, vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use tracker_core::{ErrorCode, Tracker};

const BAR_HEIGHT: f32 = 36.0;
const GO_LIVE_COLOR: Color32 = Color32::from_rgb(0x00, 0xc1, 0x6a);
const PAGE_COLOR: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Reset,
    GoLive,
    #[cfg(target_arch = "wasm32")]
    SmallWindow,
}

pub struct Options {
    pub source: Source,
    pub server_url: String,
    pub small: bool,
    pub screenshot: Option<PathBuf>,
}

struct Live {
    name: String,
    room: Option<String>,
    view: TrackerApp,
    #[cfg(target_arch = "wasm32")]
    size: Vec2,
    /// Text of the open Go Live dialog.
    go_live: Option<String>,
}

enum Screen {
    LoadingHome(Bundle),
    LoadingTracker {
        route: Route,
        bundle: Bundle,
    },
    LoadingAssets {
        route: Route,
        tracker: Box<Tracker>,
        bundle: Bundle,
    },
    /// New fonts only apply on the next frame, and drawing with a family that
    /// is not registered yet panics, so a page waits one frame before showing.
    Settling(Box<Screen>),
    Home(Box<Home>),
    Tracker(Box<Live>),
    Failed(String),
}

pub struct Shell {
    options: Options,
    screen: Screen,
}

fn tracker_dir(name: &str) -> String {
    format!("sources/{name}/")
}

impl Shell {
    pub fn new(ctx: &Context, route: Option<Route>, options: Options) -> Self {
        #[cfg(target_arch = "wasm32")]
        if matches!(route, Some(Route::Tracker { .. })) {
            crate::web::install_shortcuts(ctx.clone());
        }
        let mut shell = Self {
            options,
            screen: Screen::Failed("Page not found".to_owned()),
        };
        if let Some(route) = route {
            shell.start(ctx, route);
        }
        shell
    }

    fn start(&mut self, ctx: &Context, route: Route) {
        self.screen = match &route {
            Route::Home => {
                let mut bundle = Bundle::new(ctx, &self.options.source, "");
                Home::request_files(&mut bundle);
                Screen::LoadingHome(bundle)
            }
            Route::Tracker { name, .. } => {
                let mut bundle = Bundle::new(ctx, &self.options.source, tracker_dir(name));
                bundle.request("tracker.json");
                Screen::LoadingTracker { route, bundle }
            }
        };
    }

    /// Full page load on the web, as on the old site; in place on desktop.
    fn navigate(&mut self, ctx: &Context, route: Route) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            crate::web::navigate(&route);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.start(ctx, route);
    }

    fn set_title(ctx: &Context, title: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            crate::web::set_title(title);
        }
        #[cfg(not(target_arch = "wasm32"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
    }

    fn resize(ctx: &Context, size: Vec2) {
        #[cfg(not(target_arch = "wasm32"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        #[cfg(target_arch = "wasm32")]
        let _ = (ctx, size);
    }

    fn shows_bar(&self, room: &Option<String>) -> bool {
        let wanted_on_this_platform = cfg!(not(target_arch = "wasm32")) || room.is_none();
        !self.options.small && wanted_on_this_platform
    }

    fn enter_home(ctx: &Context, files: &HashMap<String, Vec<u8>>) -> Screen {
        match Home::load(ctx, files) {
            Ok(home) => {
                Self::set_title(ctx, "RawZLinsoTracker");
                Self::resize(ctx, home::SIZE);
                Screen::Settling(Box::new(Screen::Home(Box::new(home))))
            }
            Err(error) => Screen::Failed(error.to_string()),
        }
    }

    fn enter_tracker(
        &mut self,
        ctx: &Context,
        route: Route,
        tracker: Tracker,
        files: &HashMap<String, Vec<u8>>,
    ) -> Screen {
        let Route::Tracker { name, room } = route else {
            return Screen::Failed("Page not found".to_owned());
        };
        let assets = match Assets::load(ctx, &tracker, files) {
            Ok(assets) => assets,
            Err(error) => return Screen::Failed(error.to_string()),
        };
        let net = room.as_ref().map(|room| {
            Net::new(
                ctx.clone(),
                self.options.server_url.clone(),
                room.clone(),
                name.clone(),
            )
        });
        let title = match &room {
            Some(room) => format!("Tracker {name} - {room}"),
            None => format!("Tracker {name}"),
        };
        Self::set_title(ctx, &title);
        let size = vec2(tracker.dimensions.width, tracker.dimensions.height);
        let bar = if self.shows_bar(&room) {
            BAR_HEIGHT
        } else {
            0.0
        };
        Self::resize(ctx, size + vec2(0.0, bar));
        let screenshot = self.options.screenshot.take();
        let view = TrackerApp::new(ctx, tracker, assets, net, screenshot);
        Screen::Settling(Box::new(Screen::Tracker(Box::new(Live {
            name,
            room,
            view,
            #[cfg(target_arch = "wasm32")]
            size,
            go_live: None,
        }))))
    }

    fn advance_loading(&mut self, ctx: &Context) {
        let placeholder = Screen::Failed(String::new());
        self.screen = match std::mem::replace(&mut self.screen, placeholder) {
            Screen::LoadingHome(mut bundle) => match bundle.poll() {
                None => Screen::LoadingHome(bundle),
                Some(Ok(files)) => Self::enter_home(ctx, &files),
                Some(Err(error)) => Screen::Failed(error),
            },
            Screen::LoadingTracker { route, mut bundle } => match bundle.poll() {
                None => Screen::LoadingTracker { route, bundle },
                Some(Err(_)) => Screen::Failed("Tracker not found".to_owned()),
                Some(Ok(files)) => self.request_assets(ctx, route, &files),
            },
            Screen::LoadingAssets {
                route,
                tracker,
                mut bundle,
            } => match bundle.poll() {
                None => Screen::LoadingAssets {
                    route,
                    tracker,
                    bundle,
                },
                Some(Err(error)) => Screen::Failed(error),
                Some(Ok(files)) => self.enter_tracker(ctx, route, *tracker, &files),
            },
            other => other,
        };
    }

    fn request_assets(
        &self,
        ctx: &Context,
        route: Route,
        files: &HashMap<String, Vec<u8>>,
    ) -> Screen {
        let Route::Tracker { name, .. } = &route else {
            return Screen::Failed("Page not found".to_owned());
        };
        let parsed = files
            .get("tracker.json")
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(Tracker::parse);
        match parsed {
            Some(Ok(tracker)) => {
                let mut bundle = Bundle::new(ctx, &self.options.source, tracker_dir(name));
                for file in tracker.files() {
                    bundle.request(&file);
                }
                Screen::LoadingAssets {
                    route,
                    tracker: Box::new(tracker),
                    bundle,
                }
            }
            Some(Err(error)) => Screen::Failed(format!("Invalid tracker: {error}")),
            None => Screen::Failed("Invalid tracker".to_owned()),
        }
    }

    fn shortcuts(ctx: &Context) -> Vec<Shortcut> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            crate::web::take_shortcuts()
        }
        #[cfg(not(target_arch = "wasm32"))]
        ctx.input(|i| {
            let mut shortcuts = Vec::new();
            if i.modifiers.ctrl && i.modifiers.shift {
                if i.key_pressed(Key::R) {
                    shortcuts.push(Shortcut::Reset);
                }
                if i.key_pressed(Key::L) {
                    shortcuts.push(Shortcut::GoLive);
                }
            }
            shortcuts
        })
    }

    fn tick(&mut self, ctx: &Context) {
        self.advance_loading(ctx);
        let Screen::Tracker(live) = &mut self.screen else {
            return;
        };
        live.view.tick(ctx);
        for shortcut in Self::shortcuts(ctx) {
            match shortcut {
                Shortcut::Reset => live.view.reset(),
                Shortcut::GoLive if live.room.is_none() => live.go_live = Some(String::new()),
                Shortcut::GoLive => {}
                #[cfg(target_arch = "wasm32")]
                Shortcut::SmallWindow => crate::web::open_small_window(live.size.x, live.size.y),
            }
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        match &mut self.screen {
            Screen::Settling(_) => {
                let placeholder = Screen::Failed(String::new());
                if let Screen::Settling(next) = std::mem::replace(&mut self.screen, placeholder) {
                    self.screen = *next;
                }
                ctx.request_repaint();
            }
            Screen::LoadingHome(_)
            | Screen::LoadingTracker { .. }
            | Screen::LoadingAssets { .. } => {
                ui.centered_and_justified(|ui| ui.spinner());
            }
            Screen::Failed(message) => {
                ui.centered_and_justified(|ui| ui.label(message.as_str()));
            }
            Screen::Home(home) => {
                if let Some(name) = home.show(ui) {
                    let route = Route::Tracker {
                        name: name.to_owned(),
                        room: None,
                    };
                    self.navigate(&ctx, route);
                }
            }
            Screen::Tracker(_) => self.show_tracker(ui),
        }
    }

    fn show_tracker(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let show_bar = match &self.screen {
            Screen::Tracker(live) => self.shows_bar(&live.room),
            _ => return,
        };
        let mut route = None;
        let Screen::Tracker(live) = &mut self.screen else {
            return;
        };

        let mut origin = ui.max_rect().min;
        if show_bar {
            let bar = egui::Rect::from_min_size(origin, vec2(ui.available_width(), BAR_HEIGHT));
            let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink(4.0)));
            bar_ui.horizontal(|ui| {
                if cfg!(not(target_arch = "wasm32")) && ui.button("Home").clicked() {
                    route = Some(Route::Home);
                }
                if live.room.is_none() {
                    let label = egui::RichText::new("Go Live").color(Color32::BLACK);
                    if ui
                        .add(egui::Button::new(label).fill(GO_LIVE_COLOR))
                        .clicked()
                    {
                        live.go_live = Some(String::new());
                    }
                }
            });
            origin.y += BAR_HEIGHT;
        }
        live.view.show(ui, origin);

        if let Some(text) = &mut live.go_live {
            let mut submit = false;
            let mut close = false;
            let modal = Modal::new(ui.scope_id().with("go-live")).show(&ctx, |ui| {
                ui.heading("Go Live");
                ui.label("Name");
                let field = ui.add(TextEdit::singleline(text));
                field.request_focus();
                // The field keeps the focus, so `lost_focus` would never report Enter.
                submit = ui.button("Go").clicked() || ui.input(|i| i.key_pressed(Key::Enter));
            });
            close |= modal.should_close();
            let room = text.trim().to_owned();
            if submit && !room.is_empty() {
                route = Some(Route::Tracker {
                    name: live.name.clone(),
                    room: Some(room),
                });
            } else if close {
                live.go_live = None;
            }
        }

        if let Some(error) = live.view.fatal_error() {
            let room = live.room.as_deref().unwrap_or_default();
            let message = match error {
                ErrorCode::NameMismatch => format!(
                    "The tracker with id {room} is not tracker {}. Please connect with correct tracker",
                    live.name
                ),
                _ => "The limit of available tracker online has been reached.".to_owned(),
            };
            Modal::new(ui.scope_id().with("connection-error")).show(&ctx, |ui| {
                ui.set_max_width(320.0);
                ui.label(message);
                if ui.button("Close and back to offline").clicked() {
                    route = Some(Route::Tracker {
                        name: live.name.clone(),
                        room: None,
                    });
                }
            });
        }

        if let Some(route) = route {
            self.navigate(&ctx, route);
        }
    }
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(PAGE_COLOR).to_array()
    }
}
