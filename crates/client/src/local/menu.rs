//! The Escape menu of LinSoTracker: zoom, hints, sound, saves and the way
//! back to the menu.

use super::LocalApp;
use crate::storage::{self, ZOOMS};
use eframe::egui::{self, Modal};
use tracker_core::linso::engine::SaveState;

const WIDTH: f32 = 240.0;

fn zoom_label(zoom: f32) -> String {
    format!("x{zoom}")
}

impl LocalApp {
    pub fn toggle_menu(&mut self) {
        self.menu = match self.menu {
            Some(_) => None,
            None => Some(None),
        };
    }

    pub fn zoom(&self) -> f32 {
        self.settings.zoom
    }

    /// Whether "Back to main menu" was chosen since the last call.
    pub fn take_go_home(&mut self) -> bool {
        std::mem::take(&mut self.go_home)
    }

    fn save_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.engine.save(&self.name)).map_err(|e| e.to_string())
    }

    fn load_json(&mut self, json: &str) -> Result<(), String> {
        let save: SaveState =
            serde_json::from_str(json).map_err(|_| "This file is not a tracker save".to_owned())?;
        self.engine.load(&self.name, &save)?;
        self.popup = None;
        Ok(())
    }

    fn save_state(&self) -> Result<Option<String>, String> {
        let saved = storage::save_state(&self.name, &self.save_json()?)?;
        Ok(saved.then(|| "Tracker state saved".to_owned()))
    }

    fn load_state(&mut self) -> Result<Option<String>, String> {
        let Some(json) = storage::load_state(&self.name)? else {
            return Ok(None);
        };
        self.load_json(&json)?;
        Ok(Some("Tracker state loaded".to_owned()))
    }

    fn save_default(&self) -> Result<Option<String>, String> {
        storage::write_default_save(&self.name, &self.save_json()?)?;
        Ok(Some("Saved as default, loaded on opening".to_owned()))
    }

    fn load_default(&mut self) -> Result<Option<String>, String> {
        let json = storage::read_default_save(&self.name)
            .ok_or_else(|| "No default save for this template".to_owned())?;
        self.load_json(&json)?;
        Ok(Some("Default save loaded".to_owned()))
    }

    pub(super) fn show_menu(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        let mut outcome: Option<Result<Option<String>, String>> = None;
        let mut settings = self.settings;
        let message = self.menu.clone().flatten();

        let modal = Modal::new(ui.scope_id().with("escape-menu")).show(ui.ctx(), |ui| {
            ui.set_width(WIDTH);
            ui.vertical_centered(|ui| ui.heading("Options"));
            ui.separator();

            // Buttons rather than a combo box, whose popup a modal covers.
            ui.label("Zoom :");
            ui.horizontal(|ui| {
                for zoom in ZOOMS {
                    ui.selectable_value(&mut settings.zoom, zoom, zoom_label(zoom));
                }
            });
            ui.checkbox(&mut settings.show_hint, "Show Hint");
            ui.checkbox(&mut settings.sound, "Sound effect");
            ui.separator();

            let button = |ui: &mut egui::Ui, text: &str| {
                ui.add_sized([WIDTH, 22.0], egui::Button::new(text))
                    .clicked()
            };
            if button(ui, "Save tracker state") {
                outcome = Some(self.save_state());
            }
            if button(ui, "Load tracker state") {
                outcome = Some(self.load_state());
            }
            if button(ui, "Save as default") {
                outcome = Some(self.save_default());
            }
            if button(ui, "Load default save") {
                outcome = Some(self.load_default());
            }
            #[cfg(not(target_arch = "wasm32"))]
            if button(ui, "Open templates folder") {
                storage::open_templates_dir();
            }
            ui.separator();
            if button(ui, "Back to main menu") {
                self.go_home = true;
                close = true;
            }
            if button(ui, "Close menu") {
                close = true;
            }
            if let Some(message) = &message {
                ui.add_space(4.0);
                ui.vertical_centered(|ui| ui.label(message.as_str()));
            }
        });
        // Not `should_close`: it also reacts to the Escape press that opened it.
        close |= modal.backdrop_response.clicked();

        if settings != self.settings {
            self.settings = settings;
            storage::save_settings(&settings);
        }
        if close {
            self.menu = None;
        } else if let Some(outcome) = outcome {
            let message = match outcome {
                Ok(message) => message,
                Err(error) => Some(error),
            };
            if message.is_some() {
                self.menu = Some(message);
            }
        }
    }
}
