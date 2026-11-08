#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod home;
#[cfg(not(target_arch = "wasm32"))]
mod instance;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod menu;
mod net;
mod route;
mod shell;
mod source;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser;
    use eframe::egui;
    use route::Route;
    use shell::{Options, Shell};
    use source::Source;
    use std::path::PathBuf;

    // CI passes an empty value when the repository variable is not set.
    const DEFAULT_SERVER: &str = match option_env!("TRACKER_SERVER_URL") {
        Some(url) if !url.is_empty() => url,
        _ => "ws://127.0.0.1:3000/ws",
    };

    #[derive(Parser)]
    #[command(about = "Randomizer tracker")]
    struct Args {
        /// Tracker to open directly, e.g. `compact`. Shows the menu otherwise.
        tracker: Option<String>,
        /// Shared room to join. Without it the tracker stays local.
        #[arg(long, requires = "tracker")]
        room: Option<String>,
        #[arg(long, default_value = DEFAULT_SERVER)]
        server: String,
        /// Folder of tracker folders, to use instead of the bundled ones.
        #[arg(long)]
        sources: Option<PathBuf>,
        /// Hide the button bar above the tracker.
        #[arg(long)]
        small: bool,
        /// Save the first synchronized frame to this PNG, then quit (for testing).
        #[arg(long)]
        screenshot: Option<PathBuf>,
    }

    let args = Args::parse();
    let route = match args.tracker {
        Some(name) => Route::Tracker {
            name,
            room: args.room,
        },
        None => Route::Home,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(home::SIZE)
            .with_resizable(false)
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
                "../../../public/default/icon.png"
            ))?),
        ..Default::default()
    };
    let shell_options = Options {
        source: Source {
            sources_dir: args.sources,
        },
        server_url: args.server,
        small: args.small,
        screenshot: args.screenshot,
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        menu: None,
    };
    eframe::run_native(
        "RawZLinsoTracker",
        options,
        Box::new(move |cc| {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let shell_options = Options {
                // On Windows the menu is part of the window, which `--small` keeps bare.
                menu: (cfg!(target_os = "macos") || !shell_options.small)
                    .then(|| menu::NativeMenu::install(cc))
                    .transpose()?,
                ..shell_options
            };
            Ok(Box::new(Shell::new(
                &cc.egui_ctx,
                Some(route),
                shell_options,
            )))
        }),
    )?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use shell::{Options, Shell};

    wasm_bindgen_futures::spawn_local(async {
        let result = eframe::WebRunner::new()
            .start(
                web::canvas(),
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    let options = Options {
                        source: Default::default(),
                        server_url: web::websocket_url(),
                        small: web::is_small_window(),
                        screenshot: None,
                    };
                    Ok(Box::new(Shell::new(
                        &cc.egui_ctx,
                        web::current_route(),
                        options,
                    )))
                }),
            )
            .await;
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    });
}
