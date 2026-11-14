//! Tracker selection screen, ported from the old `pages/index.vue`.

use crate::route::Route;
use crate::source::Bundle;
use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, Context, CursorIcon, FontData, FontFamily, FontId,
    Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2, pos2, text::LayoutJob, vec2,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracker_core::Tracker;
use tracker_core::linso::Template;
use tracker_core::model::Info;

pub const SIZE: Vec2 = vec2(1280.0, 720.0);
/// The restream trackers, which can go online.
const RESTREAM: [&str; 5] = [
    "compact",
    "compactfr",
    "compactrsl",
    "compactrot",
    "compacttp",
];
/// LinSoTracker's own templates, local only.
const LOCAL: [&str; 12] = [
    "oot64",
    "oot64keysanity",
    "oot3D",
    "oot3Dkeysanity",
    "majorasmask64",
    "majorasmask3D",
    "OoTxMM",
    "alttp",
    "twilightprincess",
    "twilightprincessmap",
    "Tutorial",
    "NewFeatures",
];
const FONT: &str = "ViceCitySans";

const GRID_LEFT: f32 = 40.0;
/// Each section is a title, then rows of trackers under it.
const SECTIONS: [(&str, f32); 2] = [("Restream", 92.0), ("Local - LinSoTracker", 248.0)];
const SECTION_TITLE_HEIGHT: f32 = 20.0;
const GRID_COLUMNS: usize = 5;
/// Rows a section shows before it pages.
const SECTION_ROWS: [usize; 2] = [1, 3];
const CELL: f32 = 122.0;
const DETAIL_LEFT: f32 = 640.0;
const DETAIL_TOP: f32 = 60.0;
const DETAIL_WIDTH: f32 = 600.0;
const ILLUSTRATION_SIZE: Vec2 = vec2(600.0, 500.0);
const INFO_BOTTOM: f32 = 560.0;
const INFO_PADDING: f32 = 24.0;
const GLOW_FADE_SECONDS: f32 = 0.3;

const CREDITS: [(&str, Color32); 4] = [
    (
        "Based on LinSoTracker 2.2 - Developed by LinSoraK",
        Color32::from_rgb(0xf3, 0xf4, 0xf6),
    ),
    ("", Color32::from_rgb(0xf3, 0xf4, 0xf6)),
    (
        "Third version, rewritten in Rust from the web version",
        Color32::from_rgb(0xf3, 0xf4, 0xf6),
    ),
    (
        "Restream layouts are for Restream use only, use Local ones for personal use",
        Color32::from_rgb(0xfb, 0x2c, 0x36),
    ),
];

type Files = HashMap<String, Vec<u8>>;
type Error = Box<dyn std::error::Error + Send + Sync>;

struct Entry {
    section: usize,
    route: Route,
    info: Info,
    icon: TextureHandle,
    /// Templates the user adds may come without one.
    illustration: Option<TextureHandle>,
}

/// A tracker the menu lists, before its files are read.
struct Listed {
    section: usize,
    route: Route,
    dir: String,
    /// Built-in trackers must load; a broken template of the user's is skipped.
    required: bool,
}

pub struct Home {
    background: TextureHandle,
    glow: TextureHandle,
    menu: TextureHandle,
    arrows: [TextureHandle; 2],
    entries: Vec<Entry>,
    hovered: Option<usize>,
    pages: [usize; 2],
}

fn texture(ctx: &Context, files: &Files, path: &str) -> Result<TextureHandle, Error> {
    let bytes = files.get(path).ok_or_else(|| format!("{path}: missing"))?;
    let image = image::load_from_memory(bytes)
        .map_err(|e| format!("{path}: {e}"))?
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Ok(ctx.load_texture(path, image, TextureOptions::LINEAR))
}

/// Draws a texture at its natural size, centered on `center`.
fn draw_centered(painter: &egui::Painter, texture: &TextureHandle, center: Pos2, tint: Color32) {
    let rect = Rect::from_center_size(center, texture.size_vec2());
    painter.image(texture.id(), rect, UNIT_UV, tint);
}

const UNIT_UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

impl Home {
    pub fn request_files(bundle: &mut Bundle) {
        for file in [
            "default/background.png",
            "default/glow-official.png",
            "default/menu-official.png",
            "default/ViceCitySans.otf",
            "default/icon.png",
            "default/arrow-left.png",
            "default/arrow-right.png",
        ] {
            bundle.request(file);
        }
        for listed in Self::catalog() {
            let dir = &listed.dir;
            if listed.required {
                bundle.request(&format!("{dir}/tracker.json"));
            } else {
                bundle.request_optional(&format!("{dir}/tracker.json"));
            }
            bundle.request_optional(&format!("{dir}/icon.png"));
            bundle.request_optional(&format!("{dir}/illustration.png"));
        }
    }

    pub fn load(ctx: &Context, files: &Files) -> Result<Self, Error> {
        let mut fonts = egui::FontDefinitions::default();
        let font = files
            .get("default/ViceCitySans.otf")
            .ok_or("default font missing")?
            .clone();
        fonts
            .font_data
            .insert(FONT.to_owned(), Arc::new(FontData::from_owned(font)));
        let fallback = fonts.families[&FontFamily::Proportional].clone();
        let family = std::iter::once(FONT.to_owned()).chain(fallback).collect();
        fonts.families.insert(FontFamily::Name(FONT.into()), family);
        ctx.set_fonts(fonts);

        let default_icon = texture(ctx, files, "default/icon.png")?;
        let mut entries = Vec::new();
        for listed in Self::catalog() {
            match Self::entry(ctx, files, &listed, &default_icon) {
                Ok(entry) => entries.push(entry),
                Err(error) if listed.required => return Err(error),
                Err(error) => eprintln!("{}: skipped, {error}", listed.dir),
            }
        }
        Ok(Self {
            background: texture(ctx, files, "default/background.png")?,
            glow: texture(ctx, files, "default/glow-official.png")?,
            menu: texture(ctx, files, "default/menu-official.png")?,
            arrows: [
                texture(ctx, files, "default/arrow-left.png")?,
                texture(ctx, files, "default/arrow-right.png")?,
            ],
            entries,
            hovered: None,
            pages: [0, 0],
        })
    }

    fn entry(
        ctx: &Context,
        files: &Files,
        listed: &Listed,
        default_icon: &TextureHandle,
    ) -> Result<Entry, Error> {
        let dir = &listed.dir;
        let json = files
            .get(&format!("{dir}/tracker.json"))
            .ok_or_else(|| format!("{dir}: tracker.json missing"))?;
        let json = std::str::from_utf8(json)?;
        let info = match listed.section {
            0 => Tracker::parse(json)?.info,
            _ => Template::parse(json)?.info,
        };
        let optional = |file: &str| texture(ctx, files, &format!("{dir}/{file}")).ok();
        Ok(Entry {
            section: listed.section,
            route: listed.route.clone(),
            info,
            icon: optional("icon.png").unwrap_or_else(|| default_icon.clone()),
            illustration: optional("illustration.png"),
        })
    }

    /// Restream trackers, built-in templates, then the user's own templates.
    fn catalog() -> Vec<Listed> {
        let restream = RESTREAM.iter().map(|name| Listed {
            section: 0,
            route: Route::Tracker {
                name: (*name).to_owned(),
                room: None,
            },
            dir: format!("sources/restream/{name}"),
            required: true,
        });
        #[cfg(not(target_arch = "wasm32"))]
        let user = crate::user_templates::names();
        #[cfg(target_arch = "wasm32")]
        let user: Vec<String> = Vec::new();
        let added: Vec<String> = user
            .into_iter()
            .filter(|name| !LOCAL.contains(&name.as_str()))
            .collect();
        let local = LOCAL
            .iter()
            .map(|name| ((*name).to_owned(), true))
            .chain(added.into_iter().map(|name| (name, false)))
            .map(|(name, required)| Listed {
                section: 1,
                dir: format!("sources/linso/{name}"),
                route: Route::Local { name },
                required,
            });
        restream.chain(local).collect()
    }

    /// Returns the tracker the user clicked on.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Route> {
        let origin = ui.max_rect().min;
        let painter = ui.painter().clone();
        let background = Rect::from_min_size(origin, self.background.size_vec2());
        painter.image(self.background.id(), background, UNIT_UV, Color32::WHITE);
        self.draw_credits(&painter, origin);

        let title_font = FontId::new(18.0, FontFamily::Name(FONT.into()));
        for (title, top) in SECTIONS {
            painter.text(
                origin + vec2(GRID_LEFT, top),
                Align2::LEFT_TOP,
                title,
                title_font.clone(),
                Color32::from_rgb(0xf3, 0xf4, 0xf6),
            );
        }

        let mut chosen = None;
        self.hovered = None;
        self.page_arrows(ui, &painter, origin);
        let mut in_section = [0usize; SECTIONS.len()];
        for index in 0..self.entries.len() {
            let entry = &self.entries[index];
            let section = entry.section;
            let place = in_section[section];
            in_section[section] += 1;
            let per_page = SECTION_ROWS[section] * GRID_COLUMNS;
            if place / per_page != self.pages[section] {
                continue;
            }
            let place = place % per_page;
            let (column, row) = (place % GRID_COLUMNS, place / GRID_COLUMNS);
            let top = SECTIONS[entry.section].1 + SECTION_TITLE_HEIGHT;
            let cell = Rect::from_min_size(
                origin + vec2(GRID_LEFT, top) + vec2(column as f32, row as f32) * CELL,
                Vec2::splat(CELL),
            );
            let response = ui
                .interact(cell, ui.scope_id().with(entry.route.path()), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if response.hovered() {
                self.hovered = Some(index);
            }
            if response.clicked() {
                chosen = Some(entry.route.clone());
            }

            let fade =
                ui.ctx()
                    .animate_bool_with_time(response.id, response.hovered(), GLOW_FADE_SECONDS);
            draw_centered(
                &painter,
                &self.glow,
                cell.center(),
                Color32::WHITE.gamma_multiply(fade),
            );
            draw_centered(&painter, &self.menu, cell.center(), Color32::WHITE);
            draw_centered(
                &painter,
                &entry.icon,
                cell.center() - vec2(0.0, 5.0),
                Color32::WHITE,
            );
        }

        if let Some(index) = self.hovered {
            self.draw_detail(&painter, origin, &self.entries[index]);
        }
        chosen
    }

    /// Arrows after a section's title when it holds more than a page.
    fn page_arrows(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, origin: Pos2) {
        for (section, (_, top)) in SECTIONS.iter().enumerate() {
            let count = self.entries.iter().filter(|e| e.section == section).count();
            let per_page = SECTION_ROWS[section] * GRID_COLUMNS;
            let pages = count.div_ceil(per_page).max(1);
            if pages < 2 {
                continue;
            }
            self.pages[section] = self.pages[section].min(pages - 1);
            let right = origin.x + GRID_LEFT + GRID_COLUMNS as f32 * CELL;
            let center_y = origin.y + top + 9.0;
            let size = vec2(24.0, 24.0);
            let left_arrow = Rect::from_center_size(pos2(right - 90.0, center_y), size);
            let right_arrow = Rect::from_center_size(pos2(right - 14.0, center_y), size);
            let page = self.pages[section];
            for (rect, texture, enabled, step) in [
                (left_arrow, &self.arrows[0], page > 0, -1isize),
                (right_arrow, &self.arrows[1], page + 1 < pages, 1),
            ] {
                let tint = if enabled {
                    Color32::WHITE
                } else {
                    Color32::WHITE.gamma_multiply(0.3)
                };
                painter.image(texture.id(), rect, UNIT_UV, tint);
                let response = ui
                    .interact(
                        rect,
                        ui.scope_id().with(("page", section, step)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                if enabled && response.clicked() {
                    self.pages[section] = page.saturating_add_signed(step);
                }
            }
            painter.text(
                pos2(right - 52.0, center_y),
                Align2::CENTER_CENTER,
                format!("{}/{pages}", page + 1),
                FontId::new(16.0, FontFamily::Name(FONT.into())),
                Color32::from_rgb(0xf3, 0xf4, 0xf6),
            );
        }
    }

    fn draw_credits(&self, painter: &egui::Painter, origin: Pos2) {
        let font = FontId::new(14.0, FontFamily::Name(FONT.into()));
        for (line, (text, color)) in CREDITS.iter().enumerate() {
            let version;
            let text = if line == 1 {
                version = format!(
                    "Version {} - Developed by RawZ06",
                    env!("CARGO_PKG_VERSION")
                );
                version.as_str()
            } else {
                text
            };
            let at = origin + vec2(0.0, 10.0 + line as f32 * 20.0);
            painter.text(at, Align2::LEFT_CENTER, text, font.clone(), *color);
        }
    }

    fn draw_detail(&self, painter: &egui::Painter, origin: Pos2, entry: &Entry) {
        let left = origin.x + DETAIL_LEFT;
        if let Some(texture) = &entry.illustration {
            let illustration =
                Rect::from_min_size(pos2(left, origin.y + DETAIL_TOP), ILLUSTRATION_SIZE);
            painter.image(texture.id(), illustration, UNIT_UV, Color32::WHITE);
        }

        let text_width = DETAIL_WIDTH - 2.0 * INFO_PADDING;
        let line = |label: &str, value: &str| {
            let mut job = LayoutJob {
                halign: Align::LEFT,
                ..Default::default()
            };
            job.wrap.max_width = text_width;
            let format = |color| egui::TextFormat::simple(FontId::proportional(14.0), color);
            job.append(label, 0.0, format(Color32::WHITE));
            job.append(value, 0.0, format(Color32::from_gray(235)));
            painter.layout_job(job)
        };
        let title = painter.layout(
            entry.info.name.clone(),
            FontId::proportional(24.0),
            Color32::WHITE,
            text_width,
        );
        let mut lines = vec![
            line("Creator : ", &entry.info.creator),
            line("Version : ", &entry.info.version),
        ];
        if let Some(description) = entry.info.description.as_deref().filter(|d| !d.is_empty()) {
            lines.push(line("Description : ", description));
        }

        const LINE_GAP: f32 = 4.0;
        const DIVIDER_SPACE: f32 = 8.0;
        let lines_height: f32 = lines.iter().map(|l| l.size().y + LINE_GAP).sum();
        let height = 2.0 * INFO_PADDING + title.size().y + 2.0 * DIVIDER_SPACE + 1.0 + lines_height;
        let bottom = origin.y + INFO_BOTTOM;
        let panel = Rect::from_min_max(
            pos2(left, bottom - height),
            pos2(left + DETAIL_WIDTH, bottom),
        );
        painter.rect_filled(panel, 8.0, Color32::from_rgb(0x4a, 0x55, 0x65));

        let text_left = left + INFO_PADDING;
        let mut y = panel.top() + INFO_PADDING;
        let title_height = title.size().y;
        painter.galley(pos2(text_left, y), title, Color32::WHITE);
        y += title_height + DIVIDER_SPACE;
        painter.hline(
            text_left..=(left + DETAIL_WIDTH - INFO_PADDING),
            y,
            Stroke::new(1.0, Color32::from_rgb(0x6a, 0x72, 0x82)),
        );
        y += 1.0 + DIVIDER_SPACE;
        for galley in lines {
            let height = galley.size().y;
            painter.galley(pos2(text_left, y), galley, Color32::WHITE);
            y += height + LINE_GAP;
        }
    }
}
