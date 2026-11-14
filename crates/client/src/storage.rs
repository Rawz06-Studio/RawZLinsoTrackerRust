//! What stays between launches: settings, tracker saves and, on desktop, the
//! templates folder in Documents. On the web, the browser's local storage
//! holds settings and saves.

use serde::{Deserialize, Serialize};

pub const ZOOMS: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

/// The options of the Escape menu, shared by every LinSoTracker template.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub zoom: f32,
    /// Shows an item's name under the pointer.
    pub show_hint: bool,
    pub sound: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            show_hint: true,
            sound: false,
        }
    }
}

const SETTINGS: &str = "settings.json";

fn default_save_key(template: &str) -> String {
    format!("default_saves/{template}.trackersave")
}

pub fn load_settings() -> Settings {
    read(SETTINGS)
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = write(SETTINGS, &json);
    }
}

/// The save loaded whenever the template opens.
pub fn read_default_save(template: &str) -> Option<String> {
    read(&default_save_key(template))
}

pub fn write_default_save(template: &str, json: &str) -> Result<(), String> {
    write(&default_save_key(template), json)
}

#[cfg(not(target_arch = "wasm32"))]
mod disk {
    use std::path::{Path, PathBuf};

    const README: &str = "\
Put LinSoTracker templates here, as .template files or as folders holding a
tracker.json. They appear in the Local section of RawZLinsoTracker's menu,
and replace a built-in template of the same name.
";

    /// `Documents/RawZLinsoTracker`.
    pub fn app_dir() -> Option<PathBuf> {
        let base = dirs::document_dir().or_else(dirs::home_dir)?;
        Some(base.join("RawZLinsoTracker"))
    }

    pub fn templates_dir() -> Option<PathBuf> {
        app_dir().map(|dir| dir.join("templates"))
    }

    pub fn saves_dir() -> Option<PathBuf> {
        app_dir().map(|dir| dir.join("saves"))
    }

    /// Creates the templates folder on first launch, with a note on its use.
    pub fn prepare() {
        let Some(templates) = templates_dir() else {
            return;
        };
        if std::fs::create_dir_all(&templates).is_ok() {
            let readme = templates.join("README.txt");
            if !readme.exists() {
                let _ = std::fs::write(readme, README);
            }
        }
    }

    pub fn read(key: &str) -> Option<String> {
        std::fs::read_to_string(app_dir()?.join(key)).ok()
    }

    pub fn write(key: &str, text: &str) -> Result<(), String> {
        let path = app_dir().ok_or("no Documents folder")?.join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn dialog() -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new().add_filter("Tracker save", &["trackersave"]);
        if let Some(dir) = saves_dir() {
            let _ = std::fs::create_dir_all(&dir);
            dialog = dialog.set_directory(dir);
        }
        dialog
    }

    /// Asks where to save; `Ok(false)` when cancelled.
    pub fn save_state(template: &str, json: &str) -> Result<bool, String> {
        let Some(path) = dialog()
            .set_file_name(format!("{template}.trackersave"))
            .save_file()
        else {
            return Ok(false);
        };
        std::fs::write(&path, json)
            .map(|()| true)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Asks for a save to open; `Ok(None)` when cancelled.
    pub fn load_state(_template: &str) -> Result<Option<String>, String> {
        let Some(path) = dialog().pick_file() else {
            return Ok(None);
        };
        read_path(&path).map(Some)
    }

    fn read_path(path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn open_templates_dir() {
        let Some(dir) = templates_dir() else {
            return;
        };
        prepare();
        let program = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer"
        } else {
            "xdg-open"
        };
        if let Err(error) = std::process::Command::new(program).arg(&dir).spawn() {
            eprintln!("could not open {}: {error}", dir.display());
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod disk {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    fn key(key: &str) -> String {
        format!("rawzlinsotracker/{key}")
    }

    pub fn read(name: &str) -> Option<String> {
        storage()?.get_item(&key(name)).ok()?
    }

    pub fn write(name: &str, text: &str) -> Result<(), String> {
        storage()
            .ok_or("no local storage")?
            .set_item(&key(name), text)
            .map_err(|_| "the browser refused to store the save".to_owned())
    }

    /// The browser keeps one save per template.
    pub fn save_state(template: &str, json: &str) -> Result<bool, String> {
        write(&format!("saves/{template}.trackersave"), json).map(|()| true)
    }

    pub fn load_state(template: &str) -> Result<Option<String>, String> {
        read(&format!("saves/{template}.trackersave"))
            .map(Some)
            .ok_or_else(|| "No saved state for this template yet".to_owned())
    }
}

pub use disk::*;
