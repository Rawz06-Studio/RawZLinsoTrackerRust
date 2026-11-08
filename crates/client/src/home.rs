//! Tracker selection screen, ported from the old `pages/index.vue`.

use crate::source::Bundle;
use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, Context, CursorIcon, FontData, FontFamily, FontId,
    Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2, pos2, text::LayoutJob, vec2,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracker_core::Tracker;
use tracker_core::model::Info;

pub const SIZE: Vec2 = vec2(1280.0, 720.0);
const TRACKERS: [&str; 5] = [
    "compact",
    "compactfr",
    "compactrsl",
    "compactrot",
    "compacttp",
];
const FONT: &str = "ViceCitySans";

const GRID_ORIGIN: Vec2 = vec2(40.0, 100.0);
const GRID_COLUMNS: usize = 5;
const CELL: f32 = 122.0;
const DETAIL_LEFT: f32 = 640.0;
const DETAIL_TOP: f32 = 60.0;
const DETAIL_WIDTH: f32 = 600.0;
const ILLUSTRATION_SIZE: Vec2 = vec2(600.0, 500.0);
const INFO_BOTTOM: f32 = 560.0;
const INFO_PADDING: f32 = 24.0;
const GLOW_FADE_SECONDS: f32 = 0.3;

const CREDITS: [(&str, Color32); 3] = [
    (
        "Based on LinSoTracker 2.2 - Developed by LinSoraK",
        Color32::from_rgb(0xf3, 0xf4, 0xf6),
    ),
    ("", Color32::from_rgb(0xf3, 0xf4, 0xf6)),
    (
        "Only for Restream use, please use Linsotracker app for personal use",
        Color32::from_rgb(0xfb, 0x2c, 0x36),
    ),
];

type Files = HashMap<String, Vec<u8>>;
type Error = Box<dyn std::error::Error + Send + Sync>;

struct Entry {
    key: &'static str,
    info: Info,
    icon: TextureHandle,
    illustration: TextureHandle,
}

pub struct Home {
    background: TextureHandle,
    glow: TextureHandle,
    menu: TextureHandle,
    entries: Vec<Entry>,
    hovered: Option<usize>,
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
        ] {
            bundle.request(file);
        }
        for tracker in TRACKERS {
            for file in ["tracker.json", "icon.png", "illustration.png"] {
                bundle.request(&format!("sources/{tracker}/{file}"));
            }
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

        let mut entries = Vec::new();
        for key in TRACKERS {
            let dir = format!("sources/{key}");
            let json = files
                .get(&format!("{dir}/tracker.json"))
                .ok_or_else(|| format!("{dir}: tracker.json missing"))?;
            let tracker = Tracker::parse(std::str::from_utf8(json)?)?;
            entries.push(Entry {
                key,
                info: tracker.info,
                icon: texture(ctx, files, &format!("{dir}/icon.png"))?,
                illustration: texture(ctx, files, &format!("{dir}/illustration.png"))?,
            });
        }
        Ok(Self {
            background: texture(ctx, files, "default/background.png")?,
            glow: texture(ctx, files, "default/glow-official.png")?,
            menu: texture(ctx, files, "default/menu-official.png")?,
            entries,
            hovered: None,
        })
    }

    /// Returns the tracker the user clicked on.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<&'static str> {
        let origin = ui.max_rect().min;
        let painter = ui.painter().clone();
        let background = Rect::from_min_size(origin, self.background.size_vec2());
        painter.image(self.background.id(), background, UNIT_UV, Color32::WHITE);
        self.draw_credits(&painter, origin);

        let mut chosen = None;
        self.hovered = None;
        for index in 0..self.entries.len() {
            let entry = &self.entries[index];
            let (column, row) = (index % GRID_COLUMNS, index / GRID_COLUMNS);
            let cell = Rect::from_min_size(
                origin + GRID_ORIGIN + vec2(column as f32, row as f32) * CELL,
                Vec2::splat(CELL),
            );
            let response = ui
                .interact(cell, ui.scope_id().with(entry.key), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if response.hovered() {
                self.hovered = Some(index);
            }
            if response.clicked() {
                chosen = Some(entry.key);
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
        let illustration =
            Rect::from_min_size(pos2(left, origin.y + DETAIL_TOP), ILLUSTRATION_SIZE);
        painter.image(
            entry.illustration.id(),
            illustration,
            UNIT_UV,
            Color32::WHITE,
        );

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
