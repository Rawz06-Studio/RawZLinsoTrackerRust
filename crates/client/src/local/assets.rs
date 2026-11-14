//! Images and fonts of a LinSoTracker template.

use crate::assets::{read_png, to_sfnt};
use eframe::egui::{
    self, Color32, ColorImage, Context, FontData, FontFamily, FontId, TextureHandle,
    TextureOptions, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracker_core::linso::Template;
use tracker_core::linso::model::{MapDef, list_files};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Files = HashMap<String, Vec<u8>>;

pub struct Image {
    pub normal: TextureHandle,
    /// Only made for sprite sheets and list arrows, which get greyed out.
    pub gray: Option<TextureHandle>,
    pub size: Vec2,
}

pub struct Sheet {
    pub image: Image,
    pub cell: Vec2,
}

pub struct LocalAssets {
    pub background: Image,
    pub sheets: HashMap<String, Sheet>,
    /// Glows, submenu and map backgrounds, list arrows, by file name.
    pub images: HashMap<String, Image>,
    /// `(file, size)` of each font, by the name the template gives it.
    fonts: HashMap<String, f32>,
    registered: HashSet<String>,
}

/// pygame's `convert_to_gs`: the plain average of the three channels.
fn grayscale(image: &ColorImage) -> ColorImage {
    let mut gray = image.clone();
    for pixel in &mut gray.pixels {
        let [r, g, b, a] = pixel.to_srgba_unmultiplied();
        let average = ((r as u16 + g as u16 + b as u16) / 3) as u8;
        *pixel = Color32::from_rgba_unmultiplied(average, average, average, a);
    }
    gray
}

fn family(name: &str) -> String {
    format!("linso-{name}")
}

fn load_image(
    ctx: &Context,
    files: &Files,
    name: &str,
    with_gray: bool,
    options: TextureOptions,
) -> Result<Image, Error> {
    let image = read_png(files, name)?;
    let size = egui::vec2(image.width() as f32, image.height() as f32);
    let gray =
        with_gray.then(|| ctx.load_texture(format!("{name}-gray"), grayscale(&image), options));
    Ok(Image {
        normal: ctx.load_texture(name, image, options),
        gray,
        size,
    })
}

fn is_image(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

impl LocalAssets {
    pub fn load(
        ctx: &Context,
        template: &Template,
        maps: &[MapDef],
        files: &Files,
    ) -> Result<Self, Error> {
        let mut defs = egui::FontDefinitions::default();
        let fallback = defs.families[&FontFamily::Proportional].clone();
        let mut registered = HashSet::new();
        let mut fonts = HashMap::new();
        for (name, font) in template.all_fonts() {
            let bytes = files
                .get(&font.file)
                .ok_or_else(|| format!("{}: missing", font.file))?
                .clone();
            let bytes = to_sfnt(bytes).map_err(|e| format!("{}: {e}", font.file))?;
            defs.font_data
                .insert(family(name), Arc::new(FontData::from_owned(bytes)));
            let chain = std::iter::once(family(name))
                .chain(fallback.iter().cloned())
                .collect();
            defs.families
                .insert(FontFamily::Name(family(name).into()), chain);
            registered.insert(name.clone());
            fonts.insert(name.clone(), font.size);
        }
        ctx.set_fonts(defs);

        let background = load_image(
            ctx,
            files,
            &template.background,
            false,
            TextureOptions::LINEAR,
        )?;

        let mut sheets = HashMap::new();
        for (name, def) in &template.sheets {
            sheets.insert(
                name.clone(),
                Sheet {
                    // Nearest keeps neighbouring sprites from bleeding into the crop.
                    image: load_image(ctx, files, &def.image, true, TextureOptions::NEAREST)?,
                    cell: egui::vec2(def.cell.width, def.cell.height),
                },
            );
        }

        let mut arrows = HashSet::new();
        let mut names: Vec<String> = template.files();
        for list in template
            .maps
            .iter()
            .flat_map(|section| {
                section
                    .rules_lists
                    .iter()
                    .map(|rules| &rules.list)
                    .chain(section.maps_list.iter().map(|maps| &maps.list))
            })
            .chain(maps.iter().map(|map| &map.datas.list))
        {
            let [_, left, right] = list_files(list);
            arrows.insert(left);
            arrows.insert(right);
        }
        for map in maps {
            names.extend(map.files());
        }
        names.sort();
        names.dedup();
        let sheet_files: HashSet<&String> = template.sheets.values().map(|s| &s.image).collect();
        let mut images = HashMap::new();
        for name in names {
            if !is_image(&name) || name == template.background || sheet_files.contains(&name) {
                continue;
            }
            let with_gray = arrows.contains(&name);
            images.insert(
                name.clone(),
                load_image(ctx, files, &name, with_gray, TextureOptions::LINEAR)?,
            );
        }

        Ok(Self {
            background,
            sheets,
            images,
            fonts,
            registered,
        })
    }

    /// The template's font of this name, at its size times `scale`.
    pub fn font(&self, name: &str) -> Option<FontId> {
        let size = *self.fonts.get(name)?;
        self.registered
            .contains(name)
            .then(|| FontId::new(size, FontFamily::Name(family(name).into())))
    }

    pub fn cell(&self, sheet: &str) -> Vec2 {
        self.sheets
            .get(sheet)
            .map_or(Vec2::ZERO, |sheet| sheet.cell)
    }
}
