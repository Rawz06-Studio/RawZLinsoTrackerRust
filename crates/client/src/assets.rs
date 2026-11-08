//! Loads sprites, backgrounds and fonts from a tracker folder.

use eframe::egui::{
    self, ColorImage, Context, FontData, FontFamily, TextureHandle, TextureOptions,
};
use resvg::{tiny_skia, usvg};
use std::collections::HashMap;
use std::sync::Arc;
use tracker_core::Tracker;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Files = HashMap<String, Vec<u8>>;

/// Fonts the item renderer refers to. egui panics on an unregistered family, so
/// these are always registered, even if a tracker does not define them.
pub const USED_FONTS: [&str; 4] = [
    "incrementalItemFont",
    "evolutionItemFont",
    "countItemFont",
    "labelItemFont",
];

/// SVG backgrounds are rasterized at this factor to stay sharp on HiDPI screens.
const SVG_RASTER_SCALE: f32 = 2.0;

pub struct Sheet {
    pub normal: TextureHandle,
    pub gray: TextureHandle,
    pub cell: egui::Vec2,
}

pub struct Assets {
    pub background: TextureHandle,
    pub background_size: egui::Vec2,
    pub sheets: HashMap<String, Sheet>,
    pub glows: HashMap<String, TextureHandle>,
}

fn get<'a>(files: &'a Files, name: &str) -> Result<&'a [u8], Error> {
    files
        .get(name)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{name}: missing").into())
}

fn read_png(files: &Files, name: &str) -> Result<ColorImage, Error> {
    let image = image::load_from_memory(get(files, name)?)
        .map_err(|e| format!("{name}: {e}"))?
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

fn read_svg(files: &Files, name: &str, target: egui::Vec2) -> Result<ColorImage, Error> {
    let tree = usvg::Tree::from_data(get(files, name)?, &usvg::Options::default())?;
    let (width, height) = (target.x * SVG_RASTER_SCALE, target.y * SVG_RASTER_SCALE);
    let mut pixmap =
        tiny_skia::Pixmap::new(width as u32, height as u32).ok_or("empty SVG target")?;
    let transform = tiny_skia::Transform::from_scale(
        width / tree.size().width(),
        height / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    Ok(ColorImage::from_rgba_premultiplied(size, pixmap.data()))
}

/// Item positions assume the web app's `max-width: 100%` on the background:
/// an image wider than the tracker is scaled down, never up.
fn background_size(natural: egui::Vec2, tracker_width: f32) -> egui::Vec2 {
    natural * (tracker_width / natural.x).min(1.0)
}

/// Same weights as the CSS `grayscale(100%)` filter.
fn grayscale(image: &ColorImage) -> ColorImage {
    let mut gray = image.clone();
    for pixel in &mut gray.pixels {
        let [r, g, b, a] = pixel.to_srgba_unmultiplied();
        let luma = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
        let luma = luma.round() as u8;
        *pixel = egui::Color32::from_rgba_unmultiplied(luma, luma, luma, a);
    }
    gray
}

/// egui only reads TTF/OTF, and panics on anything else.
fn to_sfnt(bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
    match bytes.get(..4) {
        Some(b"wOFF") => Ok(wuff::decompress_woff1(&bytes)?),
        Some(b"wOF2") => Ok(wuff::decompress_woff2(&bytes)?),
        Some(b"\0\x01\0\0" | b"OTTO" | b"true") => Ok(bytes),
        _ => Err("unsupported font format".into()),
    }
}

fn load_fonts(ctx: &Context, tracker: &Tracker, files: &Files) -> Result<(), Error> {
    let mut defs = egui::FontDefinitions::default();
    let fallback = defs.families[&FontFamily::Proportional].clone();

    for (name, font) in &tracker.fonts {
        let bytes = get(files, &font.file)?.to_vec();
        let bytes = to_sfnt(bytes).map_err(|e| format!("{}: {e}", font.file))?;
        defs.font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
    }
    let names = USED_FONTS
        .iter()
        .map(|s| s.to_string())
        .chain(tracker.fonts.keys().cloned());
    for name in names {
        let mut family = Vec::new();
        if tracker.fonts.contains_key(&name) {
            family.push(name.clone());
        }
        family.extend(fallback.iter().cloned());
        defs.families.insert(FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(defs);
    Ok(())
}

impl Assets {
    pub fn load(ctx: &Context, tracker: &Tracker, files: &Files) -> Result<Self, Error> {
        load_fonts(ctx, tracker, files)?;

        let (bg_image, background_size) = if tracker.background.ends_with(".svg") {
            let size = egui::vec2(tracker.dimensions.width, tracker.dimensions.height);
            (read_svg(files, &tracker.background, size)?, size)
        } else {
            let image = read_png(files, &tracker.background)?;
            let natural = egui::vec2(image.width() as f32, image.height() as f32);
            (image, background_size(natural, tracker.dimensions.width))
        };
        let background = ctx.load_texture("background", bg_image, TextureOptions::LINEAR);

        let mut sheets = HashMap::new();
        for (name, def) in &tracker.sheets {
            let image = read_png(files, &def.image)?;
            let gray = grayscale(&image);
            sheets.insert(
                name.clone(),
                Sheet {
                    // Nearest keeps neighbouring sprites from bleeding into the crop.
                    normal: ctx.load_texture(name.as_str(), image, TextureOptions::NEAREST),
                    gray: ctx.load_texture(format!("{name}-gray"), gray, TextureOptions::NEAREST),
                    cell: egui::vec2(def.cell.width, def.cell.height),
                },
            );
        }

        let mut glows = HashMap::new();
        for file in tracker.items.iter().filter_map(|item| item.glow.as_ref()) {
            if !glows.contains_key(file) {
                let image = read_png(files, file)?;
                glows.insert(
                    file.clone(),
                    ctx.load_texture(file, image, TextureOptions::LINEAR),
                );
            }
        }

        Ok(Self {
            background,
            background_size,
            sheets,
            glows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tracker_dirs(root: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.join("tracker.json").is_file() {
                out.push(path);
            } else if path.is_dir() {
                tracker_dirs(&path, out);
            }
        }
    }

    #[test]
    fn backgrounds_wider_than_the_tracker_are_scaled_down() {
        let compactfr = background_size(egui::vec2(520.0, 400.0), 390.0);
        assert_eq!(compactfr, egui::vec2(390.0, 300.0));
        let exact = background_size(egui::vec2(330.0, 220.0), 330.0);
        assert_eq!(exact, egui::vec2(330.0, 220.0));
        let narrow = background_size(egui::vec2(300.0, 200.0), 330.0);
        assert_eq!(narrow, egui::vec2(300.0, 200.0));
    }

    #[test]
    fn every_tracker_loads_and_its_fonts_can_be_used() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/sources");
        let mut dirs = Vec::new();
        tracker_dirs(&root, &mut dirs);
        assert!(dirs.len() >= 12, "found only {} trackers", dirs.len());

        for dir in dirs {
            let json = std::fs::read_to_string(dir.join("tracker.json")).unwrap();
            let tracker = Tracker::parse(&json).unwrap();
            let ctx = Context::default();
            let files: Files = tracker
                .files()
                .into_iter()
                .map(|file| (std::fs::read(dir.join(&file)).unwrap(), file))
                .map(|(bytes, file)| (file, bytes))
                .collect();
            let assets = Assets::load(&ctx, &tracker, &files)
                .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            assert_eq!(assets.sheets.len(), tracker.sheets.len());

            // Laying out text is what makes egui parse the font files.
            for _ in 0..2 {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    for name in USED_FONTS {
                        let id = egui::FontId::new(14.0, FontFamily::Name(name.into()));
                        ui.label(egui::RichText::new("MQ 40").font(id));
                    }
                });
                output.textures_delta.clear();
            }
        }
    }
}
