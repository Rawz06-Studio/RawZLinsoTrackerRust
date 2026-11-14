//! LinSoTracker templates: drawing and input, after `Engine/Tracker.py`.
//! They stay local, so there is no network here.

pub mod assets;
mod menu;
mod view;

use crate::sound::Sounds;
use crate::storage::{self, Settings};
use assets::LocalAssets;
use eframe::egui::{
    self, Color32, CursorIcon, Event, MouseWheelUnit, Painter, PointerButton, Pos2, Rect, TextEdit,
    Vec2, pos2, vec2,
};
use std::path::PathBuf;
use tracker_core::linso::engine::{
    BlockLogic, Button, CheckKind, CheckRef, DropTarget, Engine, ItemId, Logic,
};
use tracker_core::linso::model::{Kind, ListBox, MapDef, Template};
use view::{Composite, ItemView, Text, UNIT_UV, menu_outline, rgb};

/// How long the left button stays down before an item is dragged instead of
/// clicked, as `CLICK_THRESHOLD` in LinSoTracker.
const DRAG_DELAY: f64 = 0.15;
/// Scroll distance, in points, that counts as one wheel notch.
const WHEEL_NOTCH: f32 = 40.0;
/// pygame's `is_on_element` ignores this margin inside an element.
const DEADZONE: f32 = 2.0;
/// The Go Mode glow turns 2 degrees a frame at 30 frames a second.
const GLOW_DEGREES_PER_SECOND: f64 = 60.0;
const GLOW_REPAINT_SECONDS: f32 = 1.0 / 30.0;
const OVERLAY: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 209);
const DROPPED_ICON: f32 = 30.0;
const DROPPED_LIST_ICON: f32 = 26.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Main,
    Submenu(ItemId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Popup {
    /// The checks of a block, on the current map.
    Block(usize),
    Rules(usize),
    Maps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryTarget {
    Check(CheckRef),
    Rule(usize, usize),
    Map(usize),
}

struct Entry {
    text: Text,
    strike: Option<Color32>,
    dropped: Option<tracker_core::model::SheetRef>,
    target: EntryTarget,
}

struct PopupView<'a> {
    list: &'a ListBox,
    /// Where the list is drawn from: the map, or the tracker's corner.
    index: Vec2,
    draw_box: tracker_core::linso::model::Rect,
    title: Text,
    subtitle: Option<Text>,
    entries: Vec<Entry>,
}

struct PopupLayout {
    rows: Vec<(Rect, usize)>,
    pages: usize,
    left: Rect,
    right: Rect,
}

pub struct LocalApp {
    /// The template's folder name, which saves refer to.
    name: String,
    engine: Engine,
    settings: Settings,
    sounds: Sounds,
    /// The Escape menu, with the outcome of its last action.
    menu: Option<Option<String>>,
    go_home: bool,
    assets: LocalAssets,
    wheel: f32,
    pressed_at: Option<f64>,
    dragging: Option<(ItemId, Group)>,
    popup: Option<Popup>,
    page: usize,
    map: usize,
    /// The editable box whose suggestions are shown.
    suggesting: Option<ItemId>,
    screenshot: Option<PathBuf>,
    shown_frames: u32,
}

/// pygame's `collidepoint`: the right and bottom edges are outside.
fn collides(rect: Rect, pos: Pos2) -> bool {
    pos.x >= rect.min.x && pos.x < rect.max.x && pos.y >= rect.min.y && pos.y < rect.max.y
}

/// `CoreService.is_on_element`.
fn on_element(rect: Rect, pos: Pos2) -> bool {
    pos.x >= rect.min.x + DEADZONE
        && pos.x <= rect.max.x - DEADZONE
        && pos.y >= rect.min.y + DEADZONE
        && pos.y <= rect.max.y - DEADZONE
}

impl LocalApp {
    pub fn new(
        ctx: &egui::Context,
        name: String,
        template: Template,
        maps: Vec<MapDef>,
        assets: LocalAssets,
        screenshot: Option<PathBuf>,
    ) -> Self {
        ctx.all_styles_mut(|style| style.interaction.interact_radius = 0.0);
        let mut engine = Engine::new(template, maps);
        if let Some(json) = storage::read_default_save(&name) {
            let loaded = serde_json::from_str(&json)
                .map_err(|e| e.to_string())
                .and_then(|save| engine.load(&name, &save));
            if let Err(error) = loaded {
                eprintln!("default save of {name} not loaded: {error}");
            }
        }
        Self {
            name,
            engine,
            settings: storage::load_settings(),
            sounds: Sounds::default(),
            menu: None,
            go_home: false,
            assets,
            wheel: 0.0,
            pressed_at: None,
            dragging: None,
            popup: None,
            page: 0,
            map: 0,
            suggesting: None,
            screenshot,
            shown_frames: 0,
        }
    }

    pub fn size(&self) -> Vec2 {
        let size = self.engine.template.dimensions;
        vec2(size.width, size.height)
    }

    pub fn reset(&mut self) {
        self.engine.reset();
        self.popup = None;
        self.dragging = None;
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        if self.shown_frames > 2
            && let Some(path) = self.screenshot.take()
        {
            let closer = ctx.clone();
            ctx.request_screenshot(move |image| {
                let [width, height] = image.size;
                image::save_buffer(
                    &path,
                    image.as_raw(),
                    width as u32,
                    height as u32,
                    image::ColorType::Rgba8,
                )
                .expect("cannot write screenshot");
                closer.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        } else if self.screenshot.is_some() {
            ctx.request_repaint();
        }
    }

    fn group_items(&self, group: Group) -> &[ItemId] {
        match group {
            Group::Main => &self.engine.main,
            Group::Submenu(id) => &self.engine.item(id).submenu,
        }
    }

    fn active_group(&self) -> Group {
        self.engine
            .open_submenu()
            .map_or(Group::Main, Group::Submenu)
    }

    fn view<'a>(&'a self, painter: &'a Painter, time: f64) -> ItemView<'a> {
        ItemView {
            engine: &self.engine,
            assets: &self.assets,
            painter,
            glow_angle: ((time * GLOW_DEGREES_PER_SECOND) % 360.0) as f32,
        }
    }

    fn map_origin(&self, origin: Pos2) -> Pos2 {
        origin + vec2(self.assets.background.size.x, 0.0)
    }

    fn font_text(
        &self,
        font: &str,
        color: &str,
        text: impl Into<String>,
        owidth: f32,
    ) -> Option<Text> {
        let def = self.engine.template.fonts.named.get(font)?;
        Some(Text::new(
            text,
            self.assets.font(font)?,
            rgb(def.color(color)),
            owidth,
        ))
    }

    fn map_color(&self, color: &str) -> Color32 {
        self.engine
            .template
            .fonts
            .named
            .get("mapFont")
            .map_or(Color32::WHITE, |font| rgb(font.color(color)))
    }

    // ---- map geometry ---------------------------------------------------

    fn check_rect(&self, origin: Pos2, check: usize) -> Rect {
        let section = self.engine.template.maps.clone().unwrap_or_default();
        let check = &self.engine.maps[self.map].checks[check];
        let at = self.map_origin(origin) + vec2(check.position.x, check.position.y);
        match check.kind {
            CheckKind::Block(_) => {
                Rect::from_min_size(at, vec2(section.block_size.w, section.block_size.h))
            }
            CheckKind::Simple(_) => {
                let size = vec2(section.check_size.w, section.check_size.h);
                Rect::from_min_size(at + size / 2.0, size * 2.0)
            }
        }
    }

    /// The check under the pointer, the last one drawn winning.
    fn check_at(&self, origin: Pos2, pos: Pos2) -> Option<usize> {
        let map = self.engine.maps.get(self.map)?;
        let mut found = None;
        for (index, check) in map.checks.iter().enumerate() {
            let hidden = match &check.kind {
                CheckKind::Simple(sub) => sub.hide,
                CheckKind::Block(_) => check.block_state().is_some_and(|s| s.all_hidden),
            };
            if !hidden && on_element(self.check_rect(origin, index), pos) {
                found = Some(index);
            }
        }
        found
    }

    fn popup_view(&self, popup: Popup) -> Option<PopupView<'_>> {
        let section = self.engine.template.maps.as_ref()?;
        let entry = |name: &str, color: Color32, outline: Color32, strike: bool, target| {
            let mut text = self.font_text("mapFont", "Normal", name, menu_outline(1.0))?;
            text.color = color;
            text.outline_color = outline;
            Some(Entry {
                text,
                strike: strike.then_some(color),
                dropped: None,
                target,
            })
        };
        let title = |title: &str, font: &str| self.font_text(font, "Normal", title, 2.0);
        match popup {
            Popup::Block(check) => {
                let map = self.engine.maps.get(self.map)?;
                let block = map.checks.get(check)?;
                let mut entries = Vec::new();
                for (sub, item) in block.checks().iter().enumerate() {
                    if item.hide {
                        continue;
                    }
                    let color = self.map_color(match item.logic {
                        Logic::Done => "Done",
                        Logic::Logic => "Logic",
                        Logic::NotLogic => "NotLogic",
                    });
                    let outline = if item.focused {
                        self.map_color("Focused")
                    } else {
                        Color32::BLACK
                    };
                    let target = EntryTarget::Check(CheckRef {
                        map: self.map,
                        check,
                        sub,
                    });
                    if let Some(mut entry) = entry(&item.name, color, outline, item.checked, target)
                    {
                        entry.dropped = item
                            .dragged
                            .as_ref()
                            .and_then(|dragged| self.dropped_sheet(dragged));
                        entries.push(entry);
                    }
                }
                let subtitle = block.zone.as_ref().and_then(|zone| {
                    self.font_text("mapFontSubTitle", "Normal", format!("- {zone} -"), 2.0)
                });
                let list = &map.datas.list;
                Some(PopupView {
                    list,
                    index: vec2(self.assets.background.size.x, 0.0),
                    draw_box: if block.zone.is_some() {
                        list.draw_box_subtitle.unwrap_or(list.draw_box)
                    } else {
                        list.draw_box
                    },
                    title: title(&block.name, "mapFontTitle")?,
                    subtitle,
                    entries,
                })
            }
            Popup::Rules(index) => {
                let list = self.engine.rule_lists.get(index)?;
                let entries = list
                    .rules
                    .iter()
                    .enumerate()
                    .filter_map(|(rule_index, rule)| {
                        let color = self.map_color(if rule.checked { "Done" } else { "Normal" });
                        entry(
                            &rule.name,
                            color,
                            Color32::BLACK,
                            rule.checked,
                            EntryTarget::Rule(index, rule_index),
                        )
                    })
                    .collect();
                Some(PopupView {
                    list: &list.def.list,
                    index: Vec2::ZERO,
                    draw_box: list.def.list.draw_box,
                    title: title(&list.def.name, "rulesOptionsFontTitle")?,
                    subtitle: None,
                    entries,
                })
            }
            Popup::Maps => {
                let maps_list = section.maps_list.as_ref()?;
                let entries = self
                    .engine
                    .maps
                    .iter()
                    .enumerate()
                    .filter_map(|(index, map)| {
                        let color = self.map_color("Normal");
                        entry(
                            &map.datas.name,
                            color,
                            Color32::BLACK,
                            false,
                            EntryTarget::Map(index),
                        )
                    })
                    .collect();
                Some(PopupView {
                    list: &maps_list.list,
                    index: Vec2::ZERO,
                    draw_box: maps_list.list.draw_box,
                    title: title("Maps", "mapFontTitle")?,
                    subtitle: None,
                    entries,
                })
            }
        }
    }

    fn dropped_sheet(
        &self,
        dragged: &tracker_core::linso::engine::Dragged,
    ) -> Option<tracker_core::model::SheetRef> {
        let (id, stage) = self.engine.dropped_item(dragged)?;
        let def = &self.engine.item(id).def;
        match stage {
            Some(index) if index > -1 => {
                def.next_items.get(index as usize).map(|n| n.sheet.clone())
            }
            _ => def.sheet.clone(),
        }
    }

    /// `PopupWindow.update`: one page of entries, centred in the box.
    fn popup_layout(&self, painter: &Painter, view: &PopupView, origin: Pos2) -> PopupLayout {
        let index = view.index;
        let box_rect = Rect::from_min_size(
            origin + index + vec2(view.draw_box.x, view.draw_box.y),
            vec2(view.draw_box.w, view.draw_box.h),
        );
        let row_height = view
            .entries
            .first()
            .map_or(1.0, |entry| entry.text.size(painter).y.max(1.0));
        let per_page = ((box_rect.height() / row_height) as usize).max(1);
        let pages = view.entries.len().div_ceil(per_page).max(1);
        let page = self.page.min(pages - 1);
        let start = page * per_page;
        let rows = (start..(start + per_page).min(view.entries.len()))
            .map(|i| {
                let size = view.entries[i].text.size(painter);
                let row = (i - start) as f32;
                let at = pos2(
                    box_rect.min.x + box_rect.width() / 2.0 - size.x / 2.0,
                    box_rect.min.y + size.y * row + 5.0,
                );
                (Rect::from_min_size(at, size), i)
            })
            .collect();
        let arrow = |arrow: &tracker_core::linso::model::Arrow| {
            let size = self
                .assets
                .images
                .get(&arrow.image)
                .map_or(Vec2::ZERO, |image| image.size);
            Rect::from_min_size(
                origin + index + vec2(arrow.position.x, arrow.position.y),
                size,
            )
        };
        PopupLayout {
            rows,
            pages,
            left: arrow(&view.list.left_arrow),
            right: arrow(&view.list.right_arrow),
        }
    }

    // ---- drawing --------------------------------------------------------

    fn draw_map(&self, painter: &Painter, origin: Pos2) {
        let Some(map) = self.engine.maps.get(self.map) else {
            return;
        };
        let map_origin = self.map_origin(origin);
        if let Some(image) = self.assets.images.get(&map.datas.background) {
            painter.image(
                image.normal.id(),
                Rect::from_min_size(map_origin, image.size),
                UNIT_UV,
                Color32::WHITE,
            );
        }
        // Items dropped on simple checks go under the pins.
        for (index, check) in map.checks.iter().enumerate() {
            if let CheckKind::Simple(sub) = &check.kind
                && let Some(sheet) = sub.dragged.as_ref().and_then(|d| self.dropped_sheet(d))
            {
                let pin = self.check_rect(origin, index);
                let icon = Rect::from_min_size(
                    pin.min - Vec2::splat(DROPPED_ICON / 3.0),
                    Vec2::splat(DROPPED_ICON),
                );
                view::draw_sprite(painter, &self.assets, &sheet, icon, false, 1.0);
            }
        }
        for (index, check) in map.checks.iter().enumerate() {
            let rect = self.check_rect(origin, index);
            match &check.kind {
                CheckKind::Simple(sub) if !sub.hide => {
                    let color = self.map_color(match sub.logic {
                        Logic::Done => "Done",
                        Logic::Logic => "Logic",
                        Logic::NotLogic => "NotLogic",
                    });
                    let center = rect.min + Vec2::splat((rect.width() / 2.0).floor());
                    let radius = (rect.width() / 2.0).floor();
                    if sub.focused {
                        painter.circle(
                            center,
                            radius + 3.0,
                            self.map_color("Focused"),
                            (1.0, Color32::BLACK),
                        );
                    }
                    painter.circle(center, radius, color, (1.0, Color32::BLACK));
                }
                CheckKind::Block(_) => {
                    let Some(state) = check.block_state() else {
                        continue;
                    };
                    if state.all_hidden {
                        continue;
                    }
                    let fill = self.map_color(match state.logic {
                        BlockLogic::Done => "Done",
                        BlockLogic::Logic => "Logic",
                        BlockLogic::HaveLogic => "HaveLogic",
                        BlockLogic::NotLogic => "NotLogic",
                    });
                    let border = if state.focused {
                        self.map_color("Focused")
                    } else {
                        Color32::BLACK
                    };
                    painter.rect_filled(rect, 0.0, border);
                    painter.rect_filled(rect.shrink(2.0), 0.0, fill);
                    if state.in_logic > 0
                        && let Some(text) = self.font_text(
                            "mapFontChecksNumber",
                            "Normal",
                            state.in_logic.to_string(),
                            0.5,
                        )
                    {
                        let size = text.size(painter);
                        let at = rect.center() - size / 2.0 + vec2(0.5, 1.5);
                        text.draw(painter, at, false, 1.0);
                    }
                    if state.has_dragged
                        && let Some(font) = self.assets.font("mapFont")
                    {
                        let font = egui::FontId::new(23.0, font.family);
                        let mark = Text::new("!", font, Color32::YELLOW, menu_outline(1.0));
                        let size = mark.size(painter);
                        let at = pos2(rect.max.x - 2.0, rect.center().y - size.y / 2.0);
                        mark.draw(painter, at, false, 1.0);
                    }
                }
                CheckKind::Simple(_) => {}
            }
        }

        if let Some(section) = &self.engine.template.maps {
            let (in_logic, left) = self.engine.check_counts();
            let label = format!("Checks : {in_logic} logics / {left} lefts");
            if let Some(mut text) =
                self.font_text("mapFontChecksNumber", "Normal", label, menu_outline(1.0))
            {
                text.color = Color32::WHITE;
                let at = origin + vec2(section.counter_position.x, section.counter_position.y);
                text.draw(painter, at, false, 1.0);
            }
            if let Some(maps_list) = &section.maps_list
                && let Some(mut text) = self.font_text(
                    "mapFontListMaps",
                    "Normal",
                    map.datas.name.clone(),
                    menu_outline(1.0),
                )
            {
                text.color = Color32::WHITE;
                let button = maps_list.button;
                let size = text.size(painter);
                let at = origin + vec2(button.x + button.w / 2.0, button.y + button.h / 2.0)
                    - size / 2.0;
                text.draw(painter, at, false, 1.0);
            }
        }
    }

    fn draw_popup(&self, painter: &Painter, origin: Pos2, popup: Popup) {
        let Some(view) = self.popup_view(popup) else {
            return;
        };
        let layout = self.popup_layout(painter, &view, origin);
        let base = origin + view.index;
        let background_width = self
            .assets
            .images
            .get(&view.list.background)
            .map_or(0.0, |image| {
                painter.image(
                    image.normal.id(),
                    Rect::from_min_size(base, image.size),
                    UNIT_UV,
                    Color32::WHITE,
                );
                image.size.x
            });
        let title_size = view.title.size(painter);
        let title_at = pos2(
            base.x + background_width / 2.0 - title_size.x / 2.0,
            origin.y + view.list.label_y,
        );
        view.title.draw(painter, title_at, false, 1.0);
        if let Some(subtitle) = &view.subtitle {
            let size = subtitle.size(painter);
            let at = pos2(
                base.x + background_width / 2.0 - size.x / 2.0,
                origin.y + view.list.label_y + title_size.y - size.y / 4.0,
            );
            subtitle.draw(painter, at, false, 1.0);
        }
        for &(rect, index) in &layout.rows {
            let entry = &view.entries[index];
            if let Some(sheet) = &entry.dropped {
                let icon = Rect::from_min_size(
                    pos2(
                        rect.min.x - DROPPED_LIST_ICON,
                        rect.center().y - DROPPED_LIST_ICON / 2.0,
                    ),
                    Vec2::splat(DROPPED_LIST_ICON),
                );
                view::draw_sprite(painter, &self.assets, sheet, icon, false, 1.0);
            }
            entry.text.draw(painter, rect.min, false, 1.0);
            if let Some(color) = entry.strike {
                painter.hline(rect.x_range(), rect.center().y, (2.0, color));
            }
        }
        let page = self.page.min(layout.pages - 1);
        for (rect, arrow, at_end) in [
            (layout.left, &view.list.left_arrow, page == 0),
            (
                layout.right,
                &view.list.right_arrow,
                page + 1 == layout.pages,
            ),
        ] {
            if let Some(image) = self.assets.images.get(&arrow.image) {
                let (texture, tint) = match (at_end, &image.gray) {
                    (true, Some(gray)) => (gray, Color32::WHITE.gamma_multiply(0.6)),
                    _ => (&image.normal, Color32::WHITE),
                };
                painter.image(texture.id(), rect, UNIT_UV, tint);
            }
        }
        if let Some(text) = self.font_text(
            "mapFontPagesIndicator",
            "Normal",
            format!("{}/{}", page + 1, layout.pages),
            menu_outline(1.0),
        ) {
            let size = text.size(painter);
            let gap = layout.right.min.x - layout.left.max.x;
            let at = pos2(
                layout.left.max.x + gap / 2.0 - size.x / 2.0,
                layout.left.min.y + layout.left.height() / 2.0 - size.y / 2.0,
            );
            text.draw(painter, at, false, 1.0);
        }
    }

    /// A text over a black box, as LinSoTracker's hints.
    fn draw_hint(painter: &Painter, lines: &[(Text, Pos2)]) {
        let mut area = Rect::NOTHING;
        for (text, at) in lines {
            area = area.union(Rect::from_min_size(*at, text.size(painter)));
        }
        painter.rect_filled(area, 0.0, Color32::BLACK);
        for (text, at) in lines {
            text.draw(painter, *at, false, 1.0);
        }
    }

    fn draw_check_hint(&self, painter: &Painter, origin: Pos2, check: usize) {
        let Some(found) = self.engine.maps[self.map].checks.get(check) else {
            return;
        };
        let rect = self.check_rect(origin, check);
        let Some(name) = self.font_text(
            "mapFontCheckHint",
            "Normal",
            found.name.clone(),
            menu_outline(1.0),
        ) else {
            return;
        };
        let centred = |size: Vec2| rect.min.x - (size.x / 2.0 - rect.width() / 2.0);
        let name_size = name.size(painter);
        let mut lines = Vec::new();
        let zone = found.zone.as_ref().and_then(|zone| {
            self.font_text(
                "mapFontCheckZoneHint",
                "Normal",
                format!("- {zone} -"),
                menu_outline(1.0),
            )
        });
        match zone {
            Some(zone) => {
                let zone_size = zone.size(painter);
                lines.push((
                    name,
                    pos2(centred(name_size), rect.min.y - name_size.y - zone_size.y),
                ));
                lines.push((zone, pos2(centred(zone_size), rect.min.y - zone_size.y)));
            }
            None => lines.push((name, pos2(centred(name_size), rect.min.y - name_size.y))),
        }
        Self::draw_hint(painter, &lines);
        if let CheckKind::Simple(sub) = &found.kind
            && let Some(sheet) = sub.dragged.as_ref().and_then(|d| self.dropped_sheet(d))
            && let Some(label) = self.font_text(
                "mapFontCheckZoneHint",
                "Normal",
                "Attached Item : ",
                menu_outline(1.0),
            )
        {
            let size = label.size(painter);
            let width = size.x + DROPPED_ICON;
            let at = pos2(rect.center().x - width / 2.0, rect.min.y);
            let height = size.y.max(DROPPED_ICON + 2.0);
            painter.rect_filled(
                Rect::from_min_size(at, vec2(width, height)),
                0.0,
                Color32::BLACK,
            );
            label.draw(painter, at + vec2(0.0, (height - size.y) / 2.0), false, 1.0);
            let icon = Rect::from_min_size(
                at + vec2(size.x, (height - DROPPED_ICON) / 2.0 + 1.0),
                Vec2::splat(DROPPED_ICON),
            );
            view::draw_sprite(painter, &self.assets, &sheet, icon, false, 1.0);
        }
    }

    fn draw_item_hint(&self, painter: &Painter, view: &ItemView, origin: Pos2, id: ItemId) {
        let node = self.engine.item(id);
        let Some(text) = self.font_text(
            "labelItemFont",
            "Normal",
            node.state.name.clone(),
            menu_outline(1.0),
        ) else {
            return;
        };
        let rect = view.hover_rect(origin, id);
        let size = text.size(painter);
        let at = pos2(
            rect.min.x - (size.x / 2.0 - rect.width() / 2.0),
            rect.max.y - size.y,
        );
        Self::draw_hint(painter, &[(text, at)]);
    }

    fn edit_boxes(&mut self, ui: &mut egui::Ui, origin: Pos2, enabled: bool) {
        let boxes: Vec<ItemId> = self
            .engine
            .main
            .iter()
            .copied()
            .filter(|&id| self.engine.item(id).def.kind == Kind::EditableBox)
            .collect();
        for id in boxes {
            let def = self.engine.item(id).def.clone();
            let rect = Rect::from_min_size(
                origin + vec2(def.position.x, def.position.y),
                vec2(def.size.w, def.size.h),
            );
            let (background, color) = def
                .style
                .as_ref()
                .map_or((Color32::WHITE, Color32::BLACK), |style| {
                    (rgb(style.background), rgb(style.text))
                });
            let mut text = self.engine.item(id).state.text.clone();
            let edit = TextEdit::singleline(&mut text)
                .hint_text(def.placeholder.clone().unwrap_or_default())
                .text_color(color)
                .background_color(background)
                .font(egui::FontId::proportional(14.0))
                .align(egui::Align2::LEFT_CENTER);
            let response = ui.add_enabled_ui(enabled, |ui| ui.put(rect, edit)).inner;
            if response.gained_focus() || response.changed() {
                self.suggesting = Some(id);
            }
            if response.changed() {
                self.engine.set_text(id, text.clone());
            }
            if self.suggesting != Some(id) || !enabled {
                continue;
            }
            let lowered = text.to_lowercase();
            let matches: Vec<&String> = def
                .lines
                .iter()
                .filter(|line| text.is_empty() || line.to_lowercase().starts_with(&lowered))
                .filter(|line| **line != text)
                .collect();
            if matches.is_empty() {
                continue;
            }
            let list =
                Rect::from_min_size(rect.left_bottom(), vec2(rect.width(), rect.height() * 2.5));
            let (list_background, hovered_background, hovered_text) = def.style.as_ref().map_or(
                (Color32::WHITE, Color32::DARK_BLUE, Color32::WHITE),
                |style| {
                    (
                        rgb(style.background),
                        rgb(style.hovered_background),
                        rgb(style.hovered_text),
                    )
                },
            );
            let mut chosen = None;
            ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
                ui.painter().rect_filled(list, 0.0, list_background);
                egui::ScrollArea::vertical()
                    .id_salt(("suggestions", id))
                    .max_height(list.height())
                    .show(ui, |ui| {
                        for line in matches {
                            let row = ui
                                .allocate_response(vec2(list.width(), 22.0), egui::Sense::click());
                            let (fill, ink) = if row.hovered() {
                                (hovered_background, hovered_text)
                            } else {
                                (list_background, color)
                            };
                            ui.painter().rect_filled(row.rect, 0.0, fill);
                            ui.painter().text(
                                row.rect.left_center() + vec2(4.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                line,
                                egui::FontId::proportional(14.0),
                                ink,
                            );
                            if row.clicked() {
                                chosen = Some(line.clone());
                            }
                        }
                    });
            });
            let pressed_outside = ui.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !rect.contains(p) && !list.contains(p))
            });
            if let Some(line) = chosen {
                self.engine.set_text(id, line);
                self.suggesting = None;
            } else if pressed_outside {
                self.suggesting = None;
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, origin: Pos2) {
        self.shown_frames += 1;
        self.engine.refresh_logic();
        let ctx = ui.ctx().clone();
        let painter = ui.painter().clone();
        let time = ctx.input(|i| i.time);
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let size = self.size();

        painter.rect_filled(
            Rect::from_min_size(origin, size),
            0.0,
            rgb(self.engine.template.background_color),
        );
        painter.image(
            self.assets.background.normal.id(),
            Rect::from_min_size(origin, self.assets.background.size),
            UNIT_UV,
            Color32::WHITE,
        );

        let view = self.view(&painter, time);
        let dragged = self.dragging.map(|(id, _)| id);
        for &id in &self.engine.main {
            if Some(id) != dragged {
                view.paint_item(origin, id);
            }
        }
        if self.engine.is_map_template() {
            self.draw_map(&painter, origin);
            if let Some(popup @ Popup::Block(_)) = self.popup {
                self.draw_popup(&painter, origin, popup);
            }
        }
        let spinning = self.engine.main.iter().any(|&id| {
            let node = self.engine.item(id);
            node.def.kind == Kind::GoModeItem && node.state.show && node.state.enable
        });

        let submenu = self.engine.open_submenu();
        let list_open = matches!(self.popup, Some(Popup::Rules(_) | Popup::Maps));
        self.edit_boxes(ui, origin, submenu.is_none());
        let view = self.view(&painter, time);

        if let Some(popup @ (Popup::Rules(_) | Popup::Maps)) = self.popup {
            painter.rect_filled(Rect::from_min_size(origin, size), 0.0, OVERLAY);
            self.draw_popup(&painter, origin, popup);
        }

        // The map's hints, while nothing covers the map.
        let mut over_check = false;
        if let Some(pos) = pointer
            && submenu.is_none()
            && self.popup.is_none()
            && let Some(check) = self.check_at(origin, pos)
        {
            over_check = true;
            self.draw_check_hint(&painter, origin, check);
        }

        if let Some(id) = submenu {
            painter.rect_filled(Rect::from_min_size(origin, size), 0.0, OVERLAY);
            if let Some(image) = self
                .engine
                .item(id)
                .def
                .background
                .as_ref()
                .and_then(|name| self.assets.images.get(name))
            {
                painter.image(
                    image.normal.id(),
                    Rect::from_min_size(origin, image.size),
                    UNIT_UV,
                    Color32::WHITE,
                );
            }
            for &item in &self.engine.item(id).submenu {
                if Some(item) != dragged {
                    view.paint_item(origin, item);
                }
            }
        }

        // The name of the item under the pointer.
        if let Some(pos) = pointer
            && self.settings.show_hint
            && self.menu.is_none()
            && !list_open
            && !over_check
            && self.dragging.is_none()
        {
            let group = self.active_group();
            let hovered = self.group_items(group).iter().copied().find(|&id| {
                let kind = self.engine.item(id).def.kind;
                kind != Kind::ImageItem
                    && kind != Kind::EditableBox
                    && on_element(view.hover_rect(origin, id), pos)
            });
            if let Some(id) = hovered.filter(|&id| self.engine.item(id).state.show) {
                self.draw_item_hint(&painter, &view, origin, id);
            }
        }

        if let (Some(id), Some(pos)) = (dragged, pointer)
            && let Some(image) = view.composite(id).map(Composite::without_texts)
        {
            let cell = view.cell(self.engine.item(id));
            image.paint(&painter, &self.assets, pos - cell / 2.0 + image.offset);
        }

        if over_check {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }
        if spinning {
            ctx.request_repaint_after_secs(GLOW_REPAINT_SECONDS);
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle_menu();
        }
        if self.menu.is_some() {
            self.show_menu(ui);
            // The tracker under the menu takes no clicks.
            self.pressed_at = None;
            self.wheel = 0.0;
        } else if let Some(pos) = pointer {
            self.handle_input(&ctx, &painter, origin, pos, time);
        }
    }

    // ---- input ----------------------------------------------------------

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
        if delta != 0.0 && delta.signum() != self.wheel.signum() {
            self.wheel = 0.0;
        }
        self.wheel += delta;
        let notches = (self.wheel / WHEEL_NOTCH).trunc();
        self.wheel -= notches * WHEEL_NOTCH;
        notches as i32
    }

    fn handle_input(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        origin: Pos2,
        pos: Pos2,
        time: f64,
    ) {
        let (pressed, down, released) = ctx.input(|i| {
            let p = &i.pointer;
            (
                p.button_pressed(PointerButton::Primary),
                p.button_down(PointerButton::Primary),
                [
                    (PointerButton::Primary, Button::Left),
                    (PointerButton::Secondary, Button::Right),
                    (PointerButton::Middle, Button::Middle),
                ]
                .into_iter()
                .filter(|(button, _)| p.button_released(*button))
                .map(|(_, button)| button)
                .collect::<Vec<_>>(),
            )
        });
        if pressed {
            self.pressed_at = Some(time);
        }
        if let Some(start) = self.pressed_at {
            if !down {
                self.pressed_at = None;
            } else if time - start >= DRAG_DELAY {
                self.pressed_at = None;
                self.start_drag(painter, origin, pos);
            } else {
                ctx.request_repaint_after_secs((DRAG_DELAY - (time - start)) as f32);
            }
        }

        // Dragging over a block opens its list, to drop on one of its checks.
        if self.dragging.is_some()
            && self.popup.is_none()
            && let Some(check) = self.check_at(origin, pos)
            && matches!(
                self.engine.maps[self.map].checks[check].kind,
                CheckKind::Block(_)
            )
        {
            self.popup = Some(Popup::Block(check));
            self.page = 0;
        }

        let notches = self.wheel_notches(ctx);
        let wheel = if notches > 0 {
            Button::WheelUp
        } else {
            Button::WheelDown
        };
        let mut buttons = released;
        buttons.extend(std::iter::repeat_n(wheel, notches.unsigned_abs() as usize));
        if !buttons.is_empty() {
            for button in buttons {
                self.click(ctx, painter, origin, pos, button);
            }
            ctx.request_repaint();
        }
    }

    fn item_at(&self, painter: &Painter, origin: Pos2, group: Group, pos: Pos2) -> Option<ItemId> {
        let view = self.view(painter, 0.0);
        self.group_items(group).iter().copied().find(|&id| {
            let node = self.engine.item(id);
            node.state.show
                && node.def.kind != Kind::ImageItem
                && collides(view.click_rect(origin, id), pos)
        })
    }

    fn start_drag(&mut self, painter: &Painter, origin: Pos2, pos: Pos2) {
        if matches!(self.popup, Some(Popup::Rules(_) | Popup::Maps)) {
            return;
        }
        let group = self.active_group();
        let view = self.view(painter, 0.0);
        // The last item under the pointer, as LinSoTracker keeps overwriting it.
        let found = self.group_items(group).iter().copied().rev().find(|&id| {
            let node = self.engine.item(id);
            node.state.show && node.can_drag() && collides(view.click_rect(origin, id), pos)
        });
        self.dragging = found.map(|id| (id, group));
    }

    fn click(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        origin: Pos2,
        pos: Pos2,
        button: Button,
    ) {
        if let Some((id, group)) = self.dragging.take() {
            if button == Button::Left {
                self.drop(painter, origin, pos, id, group);
            } else {
                self.dragging = Some((id, group));
            }
            return;
        }
        if let Some(submenu) = self.engine.open_submenu() {
            if !self.items_click(ctx, painter, origin, Group::Submenu(submenu), pos, button) {
                self.engine.close_submenu(submenu);
            }
            return;
        }
        if !self.engine.is_map_template() {
            self.items_click(ctx, painter, origin, Group::Main, pos, button);
            return;
        }

        if let Some(popup @ (Popup::Rules(_) | Popup::Maps)) = self.popup {
            self.popup_click(painter, origin, pos, button, popup);
            return;
        }
        if let Some(popup @ Popup::Block(_)) = self.popup {
            self.popup_click(painter, origin, pos, button, popup);
        } else if let Some(check) = self.check_at(origin, pos) {
            let map = self.map;
            match (&self.engine.maps[map].checks[check].kind, button) {
                (CheckKind::Block(_), Button::Left) => {
                    self.popup = Some(Popup::Block(check));
                    self.page = 0;
                }
                (CheckKind::Block(_), Button::Right) => self.engine.toggle_block(map, check),
                (CheckKind::Block(_), Button::Middle) => self.engine.focus_block(map, check),
                (CheckKind::Simple(_), Button::Left) => {
                    self.engine.toggle_check(CheckRef { map, check, sub: 0 })
                }
                (CheckKind::Simple(_), Button::Right) => {
                    self.engine
                        .clear_check_drop(CheckRef { map, check, sub: 0 })
                }
                (CheckKind::Simple(_), Button::Middle) => {
                    self.engine.toggle_focus(CheckRef { map, check, sub: 0 })
                }
                _ => {}
            }
        }
        self.items_click(ctx, painter, origin, Group::Main, pos, button);

        let Some(section) = self.engine.template.maps.clone() else {
            return;
        };
        let to_rect = |r: tracker_core::linso::model::Rect| {
            Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h))
        };
        if let Some(maps_list) = &section.maps_list
            && on_element(to_rect(maps_list.button), pos)
        {
            self.popup = Some(Popup::Maps);
            self.page = 0;
        }
        for (index, list) in section.rules_lists.iter().enumerate() {
            // Like LinSoTracker, a rules list opens over a block's checks.
            let list_open = matches!(self.popup, Some(Popup::Rules(_) | Popup::Maps));
            if !list_open && on_element(to_rect(list.button), pos) {
                self.popup = Some(Popup::Rules(index));
                self.page = 0;
            }
        }
    }

    /// `PopupWindow.left_click`.
    fn popup_click(
        &mut self,
        painter: &Painter,
        origin: Pos2,
        pos: Pos2,
        button: Button,
        popup: Popup,
    ) {
        let Some(view) = self.popup_view(popup) else {
            self.popup = None;
            return;
        };
        let layout = self.popup_layout(painter, &view, origin);
        let target = layout
            .rows
            .iter()
            .find(|(rect, _)| on_element(*rect, pos))
            .map(|&(_, index)| view.entries[index].target);
        let (left, right, pages) = (layout.left, layout.right, layout.pages);
        drop(view);
        let mut found = true;
        match (target, button) {
            (Some(EntryTarget::Check(at)), Button::Left) => self.engine.toggle_check(at),
            (Some(EntryTarget::Check(at)), Button::Middle) => self.engine.toggle_focus(at),
            (Some(EntryTarget::Check(at)), Button::Right) => self.engine.clear_check_drop(at),
            (Some(EntryTarget::Check(_)), _) => found = false,
            (Some(EntryTarget::Rule(list, rule)), _) => self.engine.click_rule(list, rule),
            (Some(EntryTarget::Map(map)), _) => {
                self.map = map;
                self.popup = None;
            }
            (None, _) if on_element(left, pos) => {
                self.page = self.page.min(pages - 1).saturating_sub(1)
            }
            (None, _) if on_element(right, pos) => self.page = (self.page + 1).min(pages - 1),
            (None, _) => found = false,
        }
        let tracker = Rect::from_min_size(origin, self.assets.background.size);
        if !found && !on_element(tracker, pos) {
            self.popup = None;
            self.page = 0;
        }
    }

    fn items_click(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        origin: Pos2,
        group: Group,
        pos: Pos2,
        button: Button,
    ) -> bool {
        let Some(id) = self.item_at(painter, origin, group, pos) else {
            return false;
        };
        if let Some(link) = self.engine.link(id, button) {
            ctx.open_url(egui::OpenUrl::new_tab(link));
        }
        self.engine.click(id, button);
        if self.settings.sound && matches!(button, Button::Left | Button::Right) {
            self.sounds.play(self.engine.is_enabled(id));
        }
        true
    }

    /// `Tracker._handle_moving_click`.
    fn drop(&mut self, painter: &Painter, origin: Pos2, pos: Pos2, id: ItemId, group: Group) {
        let view = self.view(painter, 0.0);
        let target = self.group_items(group).iter().copied().find(|&target| {
            let node = self.engine.item(target);
            node.def.kind == Kind::DraggableEvolutionItem
                && node.state.show
                && collides(view.click_rect(origin, target), pos)
        });
        if let Some(target) = target {
            self.engine.drop_on(id, DropTarget::Item(target));
            return;
        }
        if !self.engine.is_map_template() {
            return;
        }
        match self.popup {
            Some(popup @ Popup::Block(_)) => {
                let Some(view) = self.popup_view(popup) else {
                    return;
                };
                let layout = self.popup_layout(painter, &view, origin);
                let target = layout
                    .rows
                    .iter()
                    .find(|(rect, _)| on_element(*rect, pos))
                    .map(|&(_, index)| view.entries[index].target);
                drop(view);
                if let Some(EntryTarget::Check(at)) = target {
                    self.engine.drop_on(id, DropTarget::Check(at));
                }
            }
            None => {
                if let Some(check) = self.check_at(origin, pos)
                    && matches!(
                        self.engine.maps[self.map].checks[check].kind,
                        CheckKind::Simple(_)
                    )
                {
                    let at = CheckRef {
                        map: self.map,
                        check,
                        sub: 0,
                    };
                    self.engine.drop_on(id, DropTarget::Check(at));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn app(ctx: &egui::Context, name: &str) -> LocalApp {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../public/sources/linso")
            .join(name);
        let template =
            Template::parse(&std::fs::read_to_string(dir.join("tracker.json")).unwrap()).unwrap();
        let files = template
            .files()
            .into_iter()
            .map(|file| (std::fs::read(dir.join(&file)).unwrap(), file))
            .map(|(bytes, file)| (file, bytes))
            .collect();
        let assets = LocalAssets::load(ctx, &template, &[], &files).unwrap();
        LocalApp::new(ctx, name.to_owned(), template, Vec::new(), assets, None)
    }

    fn frame(ctx: &egui::Context, app: &mut LocalApp, events: Vec<Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 500.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.show(ui, Pos2::ZERO));
        output.textures_delta.clear();
    }

    fn escape() -> Event {
        Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn escape_opens_and_closes_the_menu() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx, "oot64");
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![escape()]);
        frame(&ctx, &mut app, vec![]);
        assert!(app.menu.is_some());
        frame(&ctx, &mut app, vec![escape()]);
        frame(&ctx, &mut app, vec![]);
        assert!(app.menu.is_none());
    }
}
