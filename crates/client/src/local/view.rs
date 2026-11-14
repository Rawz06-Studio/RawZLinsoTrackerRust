//! Draws LinSoTracker items the way pygame composes them: each step of an
//! item's `update` wraps the previous image in a new surface, which crops it,
//! and may add a text with `ptext`'s outline.

use super::assets::LocalAssets;
use eframe::egui::{
    Color32, FontId, Painter, Pos2, Rect, Shape, Vec2, emath::Rot2, epaint::Mesh, pos2, vec2,
};
use tracker_core::linso::engine::{Engine, ItemId, Node};
use tracker_core::linso::model::Kind;
use tracker_core::model::{Rgb, SheetRef};

pub const UNIT_UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

pub fn rgb(color: Rgb) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

/// `ptext`'s outline width in pixels for `owidth` at `size`.
pub fn outline_px(owidth: f32, size: f32) -> f32 {
    (owidth * size / 24.0).ceil()
}

/// `MainMenu.draw_text` widens 1-pixel outlines at zoom 1.
pub fn menu_outline(outline: f32) -> f32 {
    if outline == 1.0 { 2.0 } else { outline }
}

#[derive(Clone)]
pub struct Text {
    pub text: String,
    pub font: FontId,
    pub color: Color32,
    pub outline_color: Color32,
    pub outline: f32,
}

impl Text {
    pub fn new(text: impl Into<String>, font: FontId, color: Color32, owidth: f32) -> Self {
        let outline = outline_px(owidth, font.size);
        Self {
            text: text.into(),
            font,
            color,
            outline_color: Color32::BLACK,
            outline,
        }
    }

    /// The size of the `ptext` surface: the text plus its outline.
    pub fn size(&self, painter: &Painter) -> Vec2 {
        let galley = painter.layout_no_wrap(self.text.clone(), self.font.clone(), self.color);
        galley.size() + Vec2::splat(2.0 * self.outline)
    }

    /// Draws the surface with its top left corner at `at`.
    pub fn draw(&self, painter: &Painter, at: Pos2, gray: bool, opacity: f32) {
        let tint = |color: Color32| {
            let color = if gray { gray_color(color) } else { color };
            color.gamma_multiply(opacity)
        };
        let inner = at + Vec2::splat(self.outline);
        if self.outline > 0.0 {
            let outline = painter.layout_no_wrap(
                self.text.clone(),
                self.font.clone(),
                tint(self.outline_color),
            );
            let r = self.outline;
            let d = r * std::f32::consts::FRAC_1_SQRT_2;
            for offset in [
                vec2(r, 0.0),
                vec2(-r, 0.0),
                vec2(0.0, r),
                vec2(0.0, -r),
                vec2(d, d),
                vec2(-d, d),
                vec2(d, -d),
                vec2(-d, -d),
            ] {
                painter.galley(inner + offset, outline.clone(), Color32::BLACK);
            }
        }
        let fill = painter.layout_no_wrap(self.text.clone(), self.font.clone(), tint(self.color));
        painter.galley(inner, fill, Color32::BLACK);
    }
}

fn gray_color(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let average = ((r as u16 + g as u16 + b as u16) / 3) as u8;
    Color32::from_rgba_unmultiplied(average, average, average, a)
}

#[derive(Clone)]
enum Paint {
    Sprite(SheetRef),
    Glow { image: String, angle: f32 },
    Text(Text),
}

#[derive(Clone)]
struct Layer {
    paint: Paint,
    /// Where the content goes, in composite coordinates.
    rect: Rect,
    /// The surfaces it went through crop it to this.
    clip: Rect,
    gray: bool,
    opacity: f32,
}

/// An item's image: layers in a surface of `size`, whose top left corner is
/// drawn at the item position plus `offset`.
#[derive(Clone)]
pub struct Composite {
    pub size: Vec2,
    pub offset: Vec2,
    layers: Vec<Layer>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TextPos {
    Left,
    Center,
    Right,
    CountItem,
    Hint { left: bool },
    Label { offset: f32 },
}

impl Composite {
    pub fn sprite(sheet: &SheetRef, cell: Vec2) -> Self {
        let rect = Rect::from_min_size(Pos2::ZERO, cell);
        Self {
            size: cell,
            offset: Vec2::ZERO,
            layers: vec![Layer {
                paint: Paint::Sprite(sheet.clone()),
                rect,
                clip: rect,
                gray: false,
                opacity: 1.0,
            }],
        }
    }

    /// `convert_to_gs` then `set_image_transparent`.
    pub fn gray(mut self, opacity: f32) -> Self {
        for layer in &mut self.layers {
            layer.gray = true;
            layer.opacity *= opacity;
        }
        self
    }

    /// `Item.get_drawing_text`. `base` is the size of `base_image`, which only
    /// differs from the composite for count items.
    pub fn with_text(
        mut self,
        painter: &Painter,
        text: Option<Text>,
        base: Vec2,
        at: TextPos,
    ) -> Self {
        let Some(text) = text else {
            return self;
        };
        let t = text.size(painter);
        let (iw, ih) = (self.size.x, self.size.y);
        let (bw, bh) = (base.x, base.y);
        let mut x = 0.0;
        let mut y = bh - t.y / 1.5;
        let mut w = iw;
        let mut h = ih + t.y / 1.5;
        let mut shift = 0.0;
        match at {
            TextPos::Left => {}
            TextPos::Center => x = iw / 2.0 - t.x / 2.0,
            TextPos::Right => {
                x = iw - t.x;
                y = bh - t.y + t.y / 4.0;
                w += t.x / 1.5;
            }
            TextPos::CountItem => {
                w = bw * 2.0;
                x = bw + (bw / 2.0 - t.x / 2.0);
                y = bh / 2.0 - t.y / 2.0;
            }
            TextPos::Hint { left } => {
                x = if left { 0.0 } else { bw - t.x };
                y = 0.0;
                h = ih;
            }
            TextPos::Label { offset } => {
                x = iw / 2.0 - t.x / 2.0;
                y = ih - t.y / 5.0 - offset;
                h = ih + t.y;
                if t.x > iw {
                    shift = iw - t.x;
                    w = t.x - shift;
                    x -= shift;
                }
            }
        }
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(w, h));
        for layer in &mut self.layers {
            layer.rect = layer.rect.translate(vec2(-shift, 0.0));
            layer.clip = layer.clip.translate(vec2(-shift, 0.0)).intersect(bounds);
        }
        self.layers.push(Layer {
            paint: Paint::Text(text),
            rect: Rect::from_min_size(pos2(x, y), t),
            clip: bounds,
            gray: false,
            opacity: 1.0,
        });
        self.size = vec2(w, h);
        self.offset.x += shift;
        self
    }

    /// `CheckItem.update`: the check mark at a quarter of its size, cropped.
    pub fn with_check(mut self, check: &SheetRef, cell: Vec2) -> Self {
        let bounds = Rect::from_min_size(Pos2::ZERO, self.size);
        let rect = Rect::from_min_size(pos2(cell.x / 4.0, -cell.y / 4.0), cell);
        self.layers.push(Layer {
            paint: Paint::Sprite(check.clone()),
            rect,
            clip: bounds,
            gray: false,
            opacity: 1.0,
        });
        self
    }

    /// `GoModeItem.update`: the item centred on its turning glow.
    pub fn with_glow(mut self, image: &str, glow: Vec2, angle: f32) -> Self {
        let inset = (glow - self.size) / 2.0;
        let bounds = Rect::from_min_size(Pos2::ZERO, glow);
        for layer in &mut self.layers {
            layer.rect = layer.rect.translate(inset);
            layer.clip = layer.clip.translate(inset).intersect(bounds);
        }
        self.layers.insert(
            0,
            Layer {
                paint: Paint::Glow {
                    image: image.to_owned(),
                    angle,
                },
                rect: bounds,
                clip: bounds,
                gray: false,
                opacity: 1.0,
            },
        );
        self.offset -= inset;
        self.size = glow;
        self
    }

    /// Drops the texts, as LinSoTracker does for an item being dragged.
    pub fn without_texts(mut self) -> Self {
        self.layers
            .retain(|layer| !matches!(layer.paint, Paint::Text(_)));
        self
    }

    pub fn paint(&self, painter: &Painter, assets: &LocalAssets, top_left: Pos2) {
        for layer in &self.layers {
            let clip = layer
                .clip
                .translate(top_left.to_vec2())
                .intersect(painter.clip_rect());
            if !clip.is_positive() {
                continue;
            }
            let painter = painter.with_clip_rect(clip);
            let rect = layer.rect.translate(top_left.to_vec2());
            match &layer.paint {
                Paint::Sprite(sheet) => {
                    draw_sprite(&painter, assets, sheet, rect, layer.gray, layer.opacity)
                }
                Paint::Text(text) => text.draw(&painter, rect.min, layer.gray, layer.opacity),
                Paint::Glow { image, angle } => {
                    let Some(image) = assets.images.get(image) else {
                        continue;
                    };
                    let mut mesh = Mesh::with_texture(image.normal.id());
                    mesh.add_rect_with_uv(rect, UNIT_UV, Color32::WHITE);
                    // pygame turns counter-clockwise for a positive angle.
                    mesh.rotate(Rot2::from_angle(-angle.to_radians()), rect.center());
                    painter.add(Shape::mesh(mesh));
                }
            }
        }
    }
}

pub fn draw_sprite(
    painter: &Painter,
    assets: &LocalAssets,
    sheet: &SheetRef,
    rect: Rect,
    gray: bool,
    opacity: f32,
) {
    let Some(data) = assets.sheets.get(&sheet.sheet) else {
        return;
    };
    let texture = match (gray, &data.image.gray) {
        (true, Some(gray)) => gray,
        _ => &data.image.normal,
    };
    let size = data.image.size;
    let min = vec2(
        sheet.column.saturating_sub(1) as f32 * data.cell.x,
        sheet.row.saturating_sub(1) as f32 * data.cell.y,
    );
    let uv = Rect::from_min_size(
        pos2(min.x / size.x, min.y / size.y),
        vec2(data.cell.x / size.x, data.cell.y / size.y),
    );
    painter.image(
        texture.id(),
        rect,
        uv,
        Color32::WHITE.gamma_multiply(opacity),
    );
}

pub struct ItemView<'a> {
    pub engine: &'a Engine,
    pub assets: &'a LocalAssets,
    pub painter: &'a Painter,
    /// Degrees the Go Mode glows have turned.
    pub glow_angle: f32,
}

impl ItemView<'_> {
    fn text(&self, font: &str, color: &str, text: Option<&str>, owidth: f32) -> Option<Text> {
        let text = text?;
        let def = self.engine.template.fonts.named.get(font)?;
        Some(Text::new(
            text,
            self.assets.font(font)?,
            rgb(def.color(color)),
            owidth,
        ))
    }

    fn custom_text(&self, node: &Node, color: &str, text: &str) -> Option<Text> {
        if let Some(name) = &node.def.custom_font {
            let def = self.engine.template.fonts.custom.get(name)?;
            return Some(Text::new(
                text,
                self.assets.font(name)?,
                rgb(def.color(color)),
                2.0,
            ));
        }
        self.text("incrementalItemFont", color, Some(text), 2.0)
    }

    fn sprite(&self, sheet: &SheetRef) -> Composite {
        Composite::sprite(sheet, self.assets.cell(&sheet.sheet))
    }

    /// The colored image of an item at an evolution stage, as a dropped
    /// reference shows it.
    fn stage_image(&self, node: &Node, stage: Option<i64>) -> Option<Composite> {
        match stage {
            Some(index) if index > -1 => {
                let next = node.def.next_items.get(index as usize)?;
                Some(self.sprite(&next.sheet))
            }
            _ => node.def.sheet.as_ref().map(|sheet| self.sprite(sheet)),
        }
    }

    pub fn cell(&self, node: &Node) -> Vec2 {
        node.def
            .sheet
            .as_ref()
            .map_or(Vec2::ZERO, |sheet| self.assets.cell(&sheet.sheet))
    }

    /// The image LinSoTracker would show for this item, or `None` when the
    /// item draws nothing.
    pub fn composite(&self, id: ItemId) -> Option<Composite> {
        self.composite_at_depth(id, 0)
    }

    fn composite_at_depth(&self, id: ItemId, depth: usize) -> Option<Composite> {
        let node = self.engine.item(id);
        let state = &node.state;
        let def = &node.def;
        if !state.show || def.kind == Kind::EditableBox {
            return None;
        }
        let sheet = def.sheet.as_ref()?;
        let cell = self.cell(node);
        let hint = || self.text("hintFont", "Normal", def.hint.as_deref(), 3.0);
        let left_hint = def.kind == Kind::AlternateCountItem;

        // `Item.update`.
        let mut image = self.sprite(sheet);
        if !state.enable {
            image = image.gray(def.opacity_disable);
        }
        if state.hint_show {
            image = image.with_text(
                self.painter,
                hint(),
                cell,
                TextPos::Hint { left: left_hint },
            );
        }

        let image = match def.kind {
            Kind::IncrementalItem => {
                let position = state.increments_position;
                let last = def.increments.len() as i64 - 1;
                let text = usize::try_from(position)
                    .ok()
                    .and_then(|i| def.increments.get(i))
                    .map(String::as_str);
                let color = if position == last { "Max" } else { "Normal" };
                let text = self.text("incrementalItemFont", color, text, 2.0);
                image.with_text(self.painter, text, cell, TextPos::Left)
            }
            Kind::EvolutionItem | Kind::AlternateEvolutionItem | Kind::DraggableEvolutionItem => {
                let at = if def.label_center {
                    TextPos::Center
                } else {
                    TextPos::Right
                };
                let index = state.next_index;
                let mut image = match usize::try_from(index)
                    .ok()
                    .and_then(|i| def.next_items.get(i))
                {
                    Some(next) => {
                        let last = index == def.next_items.len() as i64 - 1;
                        let color = if last { "Max" } else { "Normal" };
                        let mut stage = self.sprite(&next.sheet);
                        if state.hint_show {
                            stage = stage.with_text(
                                self.painter,
                                hint(),
                                cell,
                                TextPos::Hint { left: false },
                            );
                        }
                        let label = if state.hint_show {
                            next.alternative_label.as_deref()
                        } else {
                            next.label.as_deref()
                        };
                        if !state.enable {
                            stage = stage.gray(def.opacity_disable);
                        }
                        let size = stage.size;
                        let text = self.text("evolutionItemFont", color, label, 2.0);
                        stage.with_text(self.painter, text, size, at)
                    }
                    None if state.enable => {
                        let color = if def.next_items.is_empty() {
                            "Max"
                        } else {
                            "Normal"
                        };
                        let label = match (&def.alternative_label, state.hint_show) {
                            (Some(alternative), true) => Some(alternative.as_str()),
                            _ => state.label.as_deref(),
                        };
                        let size = image.size;
                        let text = self.text("evolutionItemFont", color, label, 2.0);
                        image.with_text(self.painter, text, size, at)
                    }
                    None => image,
                };
                if def.kind == Kind::AlternateEvolutionItem && def.global_label.is_some() {
                    let size = image.size;
                    let text =
                        self.text("labelItemFont", "Normal", def.global_label.as_deref(), 2.0);
                    image =
                        image.with_text(self.painter, text, size, TextPos::Label { offset: 10.0 });
                }
                if def.kind == Kind::DraggableEvolutionItem
                    && let Some(dragged) = &state.dragged
                    && let Some((source, stage)) = self.engine.dropped_item(dragged)
                {
                    let source_node = self.engine.item(source);
                    let dropped = match stage {
                        Some(_) => self.stage_image(source_node, stage),
                        None if depth < 4 => self.composite_at_depth(source, depth + 1),
                        None => None,
                    };
                    if let Some(dropped) = dropped {
                        image = dropped;
                    }
                }
                image
            }
            Kind::CountItem => {
                // The text goes on the colored image, even when inactive.
                let color = if state.value == def.value_max {
                    "Max"
                } else {
                    "Normal"
                };
                let value = state.value.to_string();
                let text = self.text("countItemFont", color, Some(&value), 2.0);
                self.sprite(sheet)
                    .with_text(self.painter, text, image.size, TextPos::CountItem)
            }
            Kind::AlternateCountItem if state.value > 0 => {
                let color = if state.value == state.used_max {
                    "Max"
                } else {
                    "Normal"
                };
                let text = self.custom_text(node, color, &state.value.to_string());
                let size = image.size;
                image.with_text(self.painter, text, size, TextPos::Right)
            }
            Kind::LabelItem => {
                let label = def.label_list.get(state.label_count).map(String::as_str);
                let text = self.text("labelItemFont", "Normal", label, 2.0);
                let size = image.size;
                image.with_text(
                    self.painter,
                    text,
                    size,
                    TextPos::Label {
                        offset: def.label_offset,
                    },
                )
            }
            Kind::CheckItem if state.check => match &def.check_sheet {
                Some(check) => image.with_check(check, self.assets.cell(&check.sheet)),
                None => image,
            },
            Kind::GoModeItem => {
                let glow = def
                    .glow
                    .as_ref()
                    .and_then(|name| Some((name, self.assets.images.get(name)?.size)));
                match (state.enable, glow) {
                    (true, Some((name, size))) => image.with_glow(name, size, self.glow_angle),
                    (true, None) => image,
                    // An inactive Go Mode item is invisible, but still clickable.
                    (false, _) => return None,
                }
            }
            Kind::SubMenuItem if def.show_numbers_items_active => {
                let items = &node.submenu;
                let enabled = items
                    .iter()
                    .filter(|&&i| self.engine.item(i).state.enable)
                    .count();
                let checked = items
                    .iter()
                    .filter(|&&i| {
                        let item = self.engine.item(i);
                        item.def.kind == Kind::CheckItem && item.state.check
                    })
                    .count();
                let color = if enabled == items.len() {
                    "Max"
                } else {
                    "Normal"
                };
                let text = if def.show_number_of_checked_items {
                    format!("{checked}/{enabled}")
                } else {
                    format!("{enabled}/{}", items.len())
                };
                let text = self.text("subMenuItemFont", color, Some(&text), 2.0);
                let size = image.size;
                image.with_text(self.painter, text, size, TextPos::Right)
            }
            _ => image,
        };
        Some(image)
    }

    /// Where a click lands on the item: its cell, as pygame's `base_rect`.
    pub fn click_rect(&self, origin: Pos2, id: ItemId) -> Rect {
        let node = self.engine.item(id);
        if node.def.kind == Kind::EditableBox {
            return Rect::from_min_size(
                origin + vec2(node.def.position.x, node.def.position.y),
                vec2(node.def.size.w, node.def.size.h),
            );
        }
        Rect::from_min_size(
            origin + vec2(node.def.position.x, node.def.position.y),
            self.cell(node),
        )
    }

    /// Where the pointer shows the item's name: count items count their text.
    pub fn hover_rect(&self, origin: Pos2, id: ItemId) -> Rect {
        let node = self.engine.item(id);
        let mut rect = self.click_rect(origin, id);
        if node.def.kind == Kind::CountItem {
            rect.set_width(rect.width() * 2.0);
        }
        rect
    }

    pub fn paint_item(&self, origin: Pos2, id: ItemId) {
        if let Some(image) = self.composite(id) {
            let node = self.engine.item(id);
            let at = origin + vec2(node.def.position.x, node.def.position.y) + image.offset;
            image.paint(self.painter, self.assets, at);
        }
    }
}
