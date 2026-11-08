//! Draws the tracker and turns input into item changes. Offsets and font sizes
//! mirror the Tailwind classes of `components/items/type/*.vue`.

use crate::assets::Assets;
use crate::net::Net;
use eframe::egui::{
    self, Align2, Color32, Event, FontFamily, FontId, MouseWheelUnit, Painter, PointerButton, Pos2,
    Rect, Sense, Shape, Vec2, emath::Rot2, epaint::Mesh, pos2, vec2,
};
use std::path::PathBuf;
use tracker_core::model::{Rgb, SheetRef};
use tracker_core::{Action, ClientMsg, ErrorCode, Item, ItemValue, Kind, SyncState, Tracker};

const INCREMENTAL_FONT: &str = "incrementalItemFont";
const EVOLUTION_FONT: &str = "evolutionItemFont";
const COUNT_FONT: &str = "countItemFont";
const LABEL_FONT: &str = "labelItemFont";

// Tailwind sizes with their line heights.
const TEXT_XS: (f32, f32) = (12.0, 16.0);
const TEXT_SM: (f32, f32) = (14.0, 20.0);
const TEXT_BASE: (f32, f32) = (16.0, 24.0);
const TEXT_LG: (f32, f32) = (18.0, 28.0);

const MQ_COLOR: Color32 = Color32::from_rgb(0x2b, 0x7f, 0xff);
const GLOW_SECONDS_PER_TURN: f64 = 8.0;
const GLOW_REPAINT_SECONDS: f32 = 1.0 / 30.0;
/// Scroll distance, in points, that counts as one wheel notch.
const WHEEL_NOTCH: f32 = 40.0;

fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

/// Keeps the same scale for a custom size, as with an inline `font-size` over a
/// Tailwind class.
fn sized((size, line): (f32, f32), custom: Option<f32>) -> (f32, f32) {
    match custom {
        Some(custom) => (custom, custom * line / size),
        None => (size, line),
    }
}

pub struct TrackerApp {
    tracker: Tracker,
    assets: Assets,
    sync: SyncState,
    net: Option<Net>,
    wheel: f32,
    screenshot: Option<PathBuf>,
    shown_frames: u32,
}

impl TrackerApp {
    pub fn new(
        ctx: &egui::Context,
        tracker: Tracker,
        assets: Assets,
        net: Option<Net>,
        screenshot: Option<PathBuf>,
    ) -> Self {
        // By default egui gives a click to any widget within 5 points, which
        // would toggle items when clicking the gaps between them.
        ctx.all_styles_mut(|style| style.interaction.interact_radius = 0.0);
        Self {
            tracker,
            assets,
            sync: SyncState::default(),
            net,
            wheel: 0.0,
            screenshot,
            shown_frames: 0,
        }
    }

    fn change(&mut self, item_index: usize, action: Action) {
        let item = &self.tracker.items[item_index];
        let Some(current) = item.resolve(self.sync.get(item.id)) else {
            return;
        };
        let Some(value) = item.apply(&current, action) else {
            return;
        };
        let id = item.id;
        let message = ClientMsg::Set {
            item: id,
            value: value.clone(),
        };
        let sent = self.net.as_mut().is_some_and(|net| net.send(&message));
        self.sync.set_local(id, value, sent);
    }

    pub fn reset(&mut self) {
        let sent = self
            .net
            .as_mut()
            .is_some_and(|net| net.send(&ClientMsg::Reset));
        // Online, wait for the server's order so a reset cannot race an update.
        if !sent {
            self.sync.clear();
        }
    }

    /// Whole notches scrolled this frame; positive means wheel up.
    fn wheel_notches(&mut self, ctx: &egui::Context) -> i32 {
        let delta: f32 = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    Event::MouseWheel { unit, delta, .. } => Some(match unit {
                        MouseWheelUnit::Point => delta.y,
                        MouseWheelUnit::Line | MouseWheelUnit::Page => delta.y * WHEEL_NOTCH,
                    }),
                    _ => None,
                })
                .sum()
        });
        // A change of direction should not have to cancel the previous scroll.
        if delta != 0.0 && delta.signum() != self.wheel.signum() {
            self.wheel = 0.0;
        }
        self.wheel += delta;
        let notches = (self.wheel / WHEEL_NOTCH).trunc();
        self.wheel -= notches * WHEEL_NOTCH;
        notches as i32
    }

    fn take_screenshot_when_ready(&mut self, ctx: &egui::Context) {
        let ready = self.net.as_ref().is_none_or(Net::is_joined);
        // Wait one frame so the synchronized state is on screen.
        if ready && self.shown_frames > 1 {
            if let Some(path) = self.screenshot.take() {
                let closer = ctx.clone();
                ctx.request_screenshot(move |image| {
                    let bytes: Vec<u8> = image.as_raw().to_vec();
                    let [width, height] = image.size;
                    image::save_buffer(
                        &path,
                        &bytes,
                        width as u32,
                        height as u32,
                        image::ColorType::Rgba8,
                    )
                    .expect("cannot write screenshot");
                    closer.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
        } else if self.screenshot.is_some() {
            ctx.request_repaint();
        }
    }
}

impl TrackerApp {
    pub fn tick(&mut self, ctx: &egui::Context) {
        if let Some(net) = &mut self.net {
            net.poll(&mut self.sync);
        }
        self.take_screenshot_when_ready(ctx);
    }

    pub fn fatal_error(&self) -> Option<ErrorCode> {
        self.net.as_ref().and_then(|net| net.fatal)
    }

    pub fn show(&mut self, ui: &mut egui::Ui, origin: Pos2) {
        self.shown_frames += 1;
        let notches = self.wheel_notches(ui.ctx());
        let painter = ui.painter().clone();
        let view = View {
            tracker: &self.tracker,
            assets: &self.assets,
            painter: &painter,
        };

        let size = self.tracker.dimensions;
        let bounds = Rect::from_min_size(origin, vec2(size.width, size.height));
        painter.rect_filled(bounds, 0.0, color(self.tracker.background_color));
        painter.image(
            self.assets.background.id(),
            Rect::from_min_size(origin, self.assets.background_size),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // Same stacking as the CSS z-index: items, then glows, then Go Mode icons.
        let is_go = |item: &Item| item.kind == Kind::GoModeItem;
        let mut interactions = Vec::new();
        for (index, item) in self
            .tracker
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| !is_go(i))
        {
            let Some(value) = item.resolve(self.sync.get(item.id)) else {
                continue;
            };
            view.draw_item(origin, item, &value);
            interactions.push(index);
        }
        let time = ui.input(|i| i.time);
        let mut spinning = false;
        for item in self.tracker.items.iter().filter(|i| is_go(i)) {
            if item.resolve(self.sync.get(item.id)) == Some(ItemValue::Flag(true)) {
                view.draw_glow(origin, item, time);
                spinning = true;
            }
        }
        for (index, item) in self
            .tracker
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| is_go(i))
        {
            let Some(value) = item.resolve(self.sync.get(item.id)) else {
                continue;
            };
            if value == ItemValue::Flag(true) {
                view.icon(
                    origin + vec2(item.position.x, item.position.y),
                    item,
                    true,
                    false,
                );
            }
            interactions.push(index);
        }
        if self
            .net
            .as_ref()
            .is_some_and(|net| !net.is_joined() && net.fatal.is_none())
        {
            painter.circle_filled(origin + vec2(6.0, 6.0), 3.0, Color32::RED);
        }
        if spinning {
            ui.ctx().request_repaint_after_secs(GLOW_REPAINT_SECONDS);
        }

        let mut changes = Vec::new();
        for index in interactions {
            let item = &self.tracker.items[index];
            let response = ui.interact(
                view.hit_rect(origin, item),
                ui.scope_id().with(("item", item.id)),
                Sense::click(),
            );
            if response.clicked_by(PointerButton::Primary) {
                changes.push((index, Action::Primary));
            }
            if response.clicked_by(PointerButton::Secondary) {
                changes.push((index, Action::Secondary));
            }
            if response.clicked_by(PointerButton::Middle) {
                changes.push((index, Action::Middle));
            }
            if response.hovered() {
                let action = if notches > 0 {
                    Action::WheelUp
                } else {
                    Action::WheelDown
                };
                let count = notches.unsigned_abs() as usize;
                changes.extend(std::iter::repeat_n((index, action), count));
            }
        }

        // Applied after painting, so one more frame is needed to show them.
        if !changes.is_empty() {
            ui.ctx().request_repaint();
        }
        for (index, action) in changes {
            self.change(index, action);
        }
    }
}

struct View<'a> {
    tracker: &'a Tracker,
    assets: &'a Assets,
    painter: &'a Painter,
}

impl View<'_> {
    fn cell(&self, sheet: &SheetRef) -> Vec2 {
        self.assets.sheets[&sheet.sheet].cell
    }

    fn font_color(&self, font: &str, max: bool) -> Color32 {
        self.tracker.fonts.get(font).map_or(Color32::WHITE, |f| {
            color(if max { f.colors.max } else { f.colors.normal })
        })
    }

    fn hit_rect(&self, origin: Pos2, item: &Item) -> Rect {
        let mut size = self.cell(&item.sheet);
        if item.kind == Kind::CountItem {
            size.x *= 2.0;
        }
        Rect::from_min_size(origin + vec2(item.position.x, item.position.y), size)
    }

    fn sprite(&self, sheet: &SheetRef, at: Pos2, gray: bool, opacity: f32) {
        let data = &self.assets.sheets[&sheet.sheet];
        let texture = if gray { &data.gray } else { &data.normal };
        let tex_size = texture.size_vec2();
        let min = vec2(
            (sheet.column.saturating_sub(1)) as f32 * data.cell.x,
            (sheet.row.saturating_sub(1)) as f32 * data.cell.y,
        );
        let uv = Rect::from_min_size(
            pos2(min.x / tex_size.x, min.y / tex_size.y),
            vec2(data.cell.x / tex_size.x, data.cell.y / tex_size.y),
        );
        self.painter.image(
            texture.id(),
            Rect::from_min_size(at, data.cell),
            uv,
            Color32::WHITE.gamma_multiply(opacity),
        );
    }

    /// Text with the 1px black outline of the `.text-shadow` class. `y` is the
    /// vertical centre of the CSS line box.
    fn text(&self, at: Pos2, anchor: Align2, text: &str, font: &str, size: f32, fill: Color32) {
        let id = FontId::new(size, FontFamily::Name(font.into()));
        for offset in [
            vec2(-1.0, -1.0),
            vec2(1.0, -1.0),
            vec2(-1.0, 1.0),
            vec2(1.0, 1.0),
        ] {
            self.painter
                .text(at + offset, anchor, text, id.clone(), Color32::BLACK);
        }
        self.painter.text(at, anchor, text, id, fill);
    }

    /// `IconItem.vue`: the sprite and its optional label.
    fn icon(&self, at: Pos2, item: &Item, active: bool, max_label: bool) {
        let opacity = if active { 1.0 } else { item.opacity_disabled };
        self.sprite(&item.sheet, at, !active, opacity);
        if active && let Some(label) = item.label.as_deref().filter(|l| !l.is_empty()) {
            let (size, line) = TEXT_BASE;
            let fill = self.font_color(EVOLUTION_FONT, max_label);
            let right = at.x + self.cell(&item.sheet).x;
            self.text(
                pos2(right, at.y + 20.0 + line / 2.0),
                Align2::RIGHT_CENTER,
                label,
                EVOLUTION_FONT,
                size,
                fill,
            );
        }
    }

    fn draw_glow(&self, origin: Pos2, item: &Item, time: f64) {
        let Some(texture) = item.glow.as_ref().and_then(|g| self.assets.glows.get(g)) else {
            return;
        };
        let center = origin + vec2(item.position.x + 18.0, item.position.y + 18.0);
        let mut mesh = Mesh::with_texture(texture.id());
        mesh.add_rect_with_uv(
            Rect::from_center_size(center, texture.size_vec2()),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let turn = (time / GLOW_SECONDS_PER_TURN).fract() as f32;
        mesh.rotate(Rot2::from_angle(turn * std::f32::consts::TAU), center);
        self.painter.add(Shape::mesh(mesh));
    }

    fn draw_item(&self, origin: Pos2, item: &Item, value: &ItemValue) {
        let at = origin + vec2(item.position.x, item.position.y);
        let cell = self.cell(&item.sheet);
        let center_x = at.x + cell.x / 2.0;

        match (item.kind, value) {
            (Kind::Item, ItemValue::Flag(on)) => self.icon(at, item, *on, false),

            (Kind::IncrementalItem, ItemValue::Number(v)) => {
                self.icon(at, item, *v > 0, false);
                if let Some(text) = usize::try_from(*v - 2)
                    .ok()
                    .and_then(|i| item.increments.get(i))
                {
                    let (size, line) = TEXT_SM;
                    let fill = self.font_color(INCREMENTAL_FONT, *v == item.stages() - 1);
                    self.text(
                        pos2(at.x, at.y + 20.0 + line / 2.0),
                        Align2::LEFT_CENTER,
                        text,
                        INCREMENTAL_FONT,
                        size,
                        fill,
                    );
                }
            }

            (Kind::EvolutionItem, ItemValue::Number(v)) => {
                let stage = usize::try_from(*v - 2).ok().and_then(|i| item.next.get(i));
                self.icon(at, stage.unwrap_or(item), *v > 0, *v == item.stages() - 1);
            }

            (Kind::AlternateEvolutionItem, ItemValue::Label { state, active }) => {
                let stage = usize::try_from(*state - 1)
                    .ok()
                    .and_then(|i| item.next.get(i));
                self.icon(
                    at,
                    stage.unwrap_or(item),
                    *active,
                    *state == item.stages() - 1,
                );
                if let Some(text) = &item.global_label {
                    let (size, line) = sized(TEXT_SM, item.label_size);
                    let fill = self.font_color(LABEL_FONT, false);
                    self.text(
                        pos2(center_x, at.y + 30.0 + line / 2.0),
                        Align2::CENTER_CENTER,
                        text,
                        LABEL_FONT,
                        size,
                        fill,
                    );
                }
            }

            (Kind::CountItem, ItemValue::Number(v)) => {
                self.icon(at, item, item.is_active, false);
                let fill = self.font_color(COUNT_FONT, *v == item.count_max());
                self.text(
                    pos2(at.x + cell.x * 1.5, at.y + cell.y / 2.0),
                    Align2::CENTER_CENTER,
                    &v.to_string(),
                    COUNT_FONT,
                    TEXT_LG.0,
                    fill,
                );
            }

            (Kind::LabelItem, ItemValue::Label { state, active }) => {
                self.icon(at, item, *active, false);
                if let Some(text) = usize::try_from(*state)
                    .ok()
                    .and_then(|i| item.labels.get(i))
                {
                    let (size, line) = sized(TEXT_XS, item.label_size);
                    let top = at.y + cell.y - item.label_offset.unwrap_or(0.0);
                    let fill = self.font_color(LABEL_FONT, false);
                    self.text(
                        pos2(center_x, top + line / 2.0),
                        Align2::CENTER_CENTER,
                        text,
                        LABEL_FONT,
                        size,
                        fill,
                    );
                }
            }

            (
                Kind::CheckItem,
                ItemValue::Check {
                    check_active,
                    active,
                },
            ) => {
                self.icon(at, item, *active, false);
                if *check_active && let Some(check) = &item.check_sheet {
                    let check_cell = self.cell(check);
                    let mark = pos2(at.x + cell.x + 10.0 - check_cell.x, at.y + cell.y - 46.0);
                    self.sprite(check, mark, false, 1.0);
                }
            }

            (Kind::AlternateCountItem, ItemValue::Count { count, mq }) => {
                self.icon(at, item, *count > 0, false);
                if *count > 0 {
                    self.alternate_count_label(at, cell, item, *count, *mq);
                }
                if *mq {
                    let (size, line) = TEXT_XS;
                    self.text(
                        pos2(at.x + cell.x, at.y - 5.0 + line / 2.0),
                        Align2::RIGHT_CENTER,
                        "MQ",
                        LABEL_FONT,
                        size,
                        MQ_COLOR,
                    );
                }
            }

            _ => {}
        }
    }

    fn alternate_count_label(&self, at: Pos2, cell: Vec2, item: &Item, count: i64, mq: bool) {
        // The max colour really is the incremental font's in the web app.
        let fill = self.font_color(INCREMENTAL_FONT, count == item.alternate_count_max(mq));
        let text = count.to_string();
        let bottom = at.y + cell.y;
        let (x, y, anchor, size) = match item.label_position.as_deref() {
            Some("left") => (
                at.x - 4.0,
                bottom + 6.0 - TEXT_SM.1 / 2.0,
                Align2::LEFT_CENTER,
                TEXT_SM.0,
            ),
            Some("right") => (
                at.x + 8.0 + cell.x / 2.0,
                bottom + 7.0 - TEXT_SM.1 / 2.0,
                Align2::CENTER_CENTER,
                TEXT_SM.0,
            ),
            _ => (
                at.x + 10.0 + cell.x / 2.0,
                bottom + 3.0 - TEXT_LG.1 / 2.0,
                Align2::CENTER_CENTER,
                TEXT_LG.0,
            ),
        };
        self.text(pos2(x, y), anchor, &text, COUNT_FONT, size, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const FIRE_ARROW: u32 = 4;
    const FIRE_ARROW_AT: Pos2 = pos2(310.0, 20.0);
    const SKULL_COUNT: u32 = 36;

    struct Harness {
        ctx: egui::Context,
        app: TrackerApp,
        time: f64,
    }

    impl Harness {
        fn new() -> Self {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/sources/compact");
            let json = std::fs::read_to_string(dir.join("tracker.json")).unwrap();
            let tracker = Tracker::parse(&json).unwrap();
            let ctx = egui::Context::default();
            let files = tracker
                .files()
                .into_iter()
                .map(|file| (std::fs::read(dir.join(&file)).unwrap(), file))
                .map(|(bytes, file)| (file, bytes))
                .collect();
            let assets = Assets::load(&ctx, &tracker, &files).unwrap();
            let app = TrackerApp::new(&ctx, tracker, assets, None, None);
            let mut harness = Self {
                ctx,
                app,
                time: 0.0,
            };
            harness.frame(vec![]); // Lets the fonts registered above take effect.
            harness
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(385.0, 270.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let (ctx, app) = (self.ctx.clone(), &mut self.app);
            let mut output = ctx.run_ui(input, |ui| {
                app.tick(ui.ctx());
                app.show(ui, Pos2::ZERO);
            });
            output.textures_delta.clear(); // No GPU in tests to hand them to.
        }

        fn click(&mut self, pos: Pos2, button: PointerButton) {
            let press = |pressed| Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            };
            self.frame(vec![Event::PointerMoved(pos)]);
            self.frame(vec![press(true)]);
            self.frame(vec![press(false)]);
            self.frame(vec![]);
        }

        fn value(&self, item: u32) -> Option<&ItemValue> {
            self.app.sync.get(item)
        }

        fn item_center(&self, id: u32) -> Pos2 {
            let item = self.app.tracker.items.iter().find(|i| i.id == id).unwrap();
            pos2(item.position.x + 17.0, item.position.y + 17.0)
        }
    }

    #[test]
    fn left_then_right_click_toggle_an_item() {
        let mut h = Harness::new();
        assert_eq!(h.value(FIRE_ARROW), None);
        h.click(FIRE_ARROW_AT, PointerButton::Primary);
        assert_eq!(h.value(FIRE_ARROW), Some(&ItemValue::Flag(true)));
        h.click(FIRE_ARROW_AT, PointerButton::Secondary);
        assert_eq!(h.value(FIRE_ARROW), Some(&ItemValue::Flag(false)));
    }

    #[test]
    fn a_click_on_empty_space_changes_nothing() {
        let mut h = Harness::new();
        // Empty, but 2 points left of the item at (5, 5).
        h.click(pos2(3.0, 20.0), PointerButton::Primary);
        assert_eq!(
            h.app
                .tracker
                .items
                .iter()
                .filter(|i| h.value(i.id).is_some())
                .count(),
            0
        );
    }

    #[test]
    fn wheel_changes_the_hovered_item_one_notch_at_a_time() {
        let mut h = Harness::new();
        let pos = h.item_center(SKULL_COUNT);
        h.frame(vec![Event::PointerMoved(pos)]);
        h.frame(vec![]);
        let scroll = |y| Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: Default::default(),
        };
        // Two small trackpad moves add up to one notch down.
        h.frame(vec![scroll(-25.0)]);
        h.frame(vec![scroll(-25.0)]);
        h.frame(vec![]);
        assert_eq!(h.value(SKULL_COUNT), Some(&ItemValue::Number(1)));
        h.frame(vec![scroll(40.0)]);
        h.frame(vec![]);
        assert_eq!(h.value(SKULL_COUNT), Some(&ItemValue::Number(0)));
    }

    #[test]
    fn reset_clears_local_state() {
        let mut h = Harness::new();
        h.click(FIRE_ARROW_AT, PointerButton::Primary);
        h.app.reset();
        assert_eq!(h.value(FIRE_ARROW), None);
    }
}
