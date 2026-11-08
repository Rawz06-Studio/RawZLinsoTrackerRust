//! Where tracker files come from: bundled in the executable, or fetched from
//! the server on the web. Results arrive through a [`Bundle`].

use eframe::egui::Context;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

type FileResult = (String, Result<Vec<u8>, String>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../public/"]
#[exclude = "sources/archives/*"]
#[exclude = "*.DS_Store"]
#[exclude = "*.wav"]
struct Bundled;

#[derive(Clone, Default)]
pub struct Source {
    /// Replaces the bundled `sources` folder, to try other trackers.
    #[cfg(not(target_arch = "wasm32"))]
    pub sources_dir: Option<std::path::PathBuf>,
}

impl Source {
    #[cfg(not(target_arch = "wasm32"))]
    fn read(&self, path: String, tx: Sender<FileResult>, ctx: Context) {
        let override_path = self.sources_dir.as_ref().and_then(|dir| {
            let relative = path.strip_prefix("sources/")?;
            Some(dir.join(relative))
        });
        let result = match override_path {
            Some(file) => std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display())),
            None => Bundled::get(&path)
                .map(|file| file.data.into_owned())
                .ok_or_else(|| format!("{path}: not found")),
        };
        let _ = tx.send((path, result));
        ctx.request_repaint();
    }

    #[cfg(target_arch = "wasm32")]
    fn read(&self, path: String, tx: Sender<FileResult>, ctx: Context) {
        ehttp::fetch(ehttp::Request::get(format!("/{path}")), move |response| {
            let result = match response {
                Ok(response) if response.ok => Ok(response.bytes),
                Ok(response) => Err(format!("{path}: HTTP {}", response.status)),
                Err(error) => Err(format!("{path}: {error}")),
            };
            let _ = tx.send((path, result));
            ctx.request_repaint();
        });
    }
}

/// A set of files being loaded. `poll` yields them once they have all arrived,
/// keyed by the names passed to `request`.
pub struct Bundle {
    source: Source,
    ctx: Context,
    prefix: String,
    tx: Sender<FileResult>,
    rx: Receiver<FileResult>,
    pending: usize,
    files: HashMap<String, Vec<u8>>,
    error: Option<String>,
}

impl Bundle {
    pub fn new(ctx: &Context, source: &Source, prefix: impl Into<String>) -> Self {
        let (tx, rx) = channel();
        Self {
            source: source.clone(),
            ctx: ctx.clone(),
            prefix: prefix.into(),
            tx,
            rx,
            pending: 0,
            files: HashMap::new(),
            error: None,
        }
    }

    pub fn request(&mut self, name: &str) {
        self.pending += 1;
        let path = format!("{}{name}", self.prefix);
        self.source.read(path, self.tx.clone(), self.ctx.clone());
    }

    pub fn poll(&mut self) -> Option<Result<HashMap<String, Vec<u8>>, String>> {
        while let Ok((path, result)) = self.rx.try_recv() {
            self.pending -= 1;
            match result {
                Ok(bytes) => {
                    let name = path.strip_prefix(&self.prefix).unwrap_or(&path).to_owned();
                    self.files.insert(name, bytes);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if self.pending > 0 {
            return None;
        }
        Some(match self.error.take() {
            Some(error) => Err(error),
            None => Ok(std::mem::take(&mut self.files)),
        })
    }
}
