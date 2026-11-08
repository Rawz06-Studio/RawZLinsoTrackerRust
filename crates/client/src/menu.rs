//! The operating system's menu, which keeps the window the size of the tracker.

use crate::shell::Shortcut;
use eframe::egui::Context;
use muda::{Menu, MenuEvent, MenuId, MenuItem, Submenu};
use std::sync::mpsc::{self, Receiver};

type Error = Box<dyn std::error::Error + Send + Sync>;

pub struct NativeMenu {
    /// The menu disappears if this is dropped.
    _bar: Menu,
    home: MenuItem,
    go_live: MenuItem,
    enabled: (bool, bool),
    events: Receiver<MenuId>,
}

impl NativeMenu {
    pub fn install(cc: &eframe::CreationContext<'_>) -> Result<Self, Error> {
        let home = MenuItem::new("Home", false, None);
        let go_live = MenuItem::new("Go Live…", false, None);
        let bar = Menu::new();
        #[cfg(target_os = "macos")]
        bar.append(&Submenu::with_items(
            "RawZLinsoTracker",
            true,
            &[&muda::PredefinedMenuItem::quit(None)],
        )?)?;
        bar.append(&Submenu::with_items("File", true, &[&home, &go_live])?)?;
        attach(&bar, cc)?;

        let (sender, events) = mpsc::channel();
        let ctx: Context = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = sender.send(event.id);
            // egui only runs when there is something to redraw.
            ctx.request_repaint();
        }));

        Ok(Self {
            _bar: bar,
            home,
            go_live,
            enabled: (false, false),
            events,
        })
    }

    pub fn set_enabled(&mut self, home: bool, go_live: bool) {
        if self.enabled != (home, go_live) {
            self.home.set_enabled(home);
            self.go_live.set_enabled(go_live);
            self.enabled = (home, go_live);
        }
    }

    pub fn take_shortcuts(&self) -> Vec<Shortcut> {
        self.events
            .try_iter()
            .filter_map(|id| {
                if id == *self.home.id() {
                    Some(Shortcut::Home)
                } else if id == *self.go_live.id() {
                    Some(Shortcut::GoLive)
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
fn attach(bar: &Menu, _cc: &eframe::CreationContext<'_>) -> Result<(), Error> {
    bar.init_for_nsapp();
    Ok(())
}

#[cfg(target_os = "windows")]
fn attach(bar: &Menu, cc: &eframe::CreationContext<'_>) -> Result<(), Error> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if let RawWindowHandle::Win32(handle) = cc.window_handle()?.as_raw() {
        // SAFETY: the handle is the live window eframe just created for this
        // thread, and the menu is kept for as long as that window exists.
        unsafe { bar.init_for_hwnd(handle.hwnd.get())? };
    }
    Ok(())
}
