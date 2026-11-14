//! Typed view of the LinSoTracker `tracker.json` and map files.
//!
//! The layout is the one of `templates/*.template` in LinSoTracker 2.4: a
//! positional array `[informations, datas, fonts, items, maps?]`.

use crate::model::{Info, Point, Rgb, SheetDef, SheetRef, Size};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct FontDef {
    #[serde(rename = "Name")]
    pub file: String,
    #[serde(rename = "Size")]
    pub size: f32,
    /// `Normal` and `Max` for items; `Logic`, `NotLogic`, `Done`… for maps.
    #[serde(rename = "Colors", default)]
    pub colors: BTreeMap<String, Rgb>,
}

impl FontDef {
    pub fn color(&self, name: &str) -> Rgb {
        self.colors.get(name).copied().unwrap_or(Rgb {
            r: 255,
            g: 255,
            b: 255,
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Fonts {
    /// Fonts an item picks by name with `customFont`.
    #[serde(rename = "customFonts", default)]
    pub custom: BTreeMap<String, FontDef>,
    #[serde(flatten)]
    pub named: BTreeMap<String, FontDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Kind {
    #[default]
    Item,
    ImageItem,
    IncrementalItem,
    EvolutionItem,
    AlternateEvolutionItem,
    DraggableEvolutionItem,
    CountItem,
    AlternateCountItem,
    LabelItem,
    CheckItem,
    GoModeItem,
    SubMenuItem,
    OpenLinkItem,
    EditableBox,
}

impl Kind {
    /// Kinds that hold an evolution list, and so a `next_item_index`.
    pub fn evolves(self) -> bool {
        matches!(
            self,
            Kind::EvolutionItem | Kind::AlternateEvolutionItem | Kind::DraggableEvolutionItem
        )
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct BoxSize {
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NextItem {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Label", default)]
    pub label: Option<String>,
    #[serde(rename = "AlternativeLabel", default)]
    pub alternative_label: Option<String>,
    #[serde(rename = "SheetInformation")]
    pub sheet: SheetRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoxStyle {
    #[serde(rename = "BackgroundColor")]
    pub background: Rgb,
    #[serde(rename = "NormalTextColor")]
    pub text: Rgb,
    #[serde(rename = "SelectedBackgroundColor")]
    pub selected_background: Rgb,
    #[serde(rename = "SelectedTextColor")]
    pub selected_text: Rgb,
    #[serde(rename = "HoveredBackgroundColor")]
    pub hovered_background: Rgb,
    #[serde(rename = "HoveredTextColor")]
    pub hovered_text: Rgb,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    #[serde(rename = "Id", default)]
    pub id: i64,
    #[serde(rename = "Kind", default)]
    pub kind: Kind,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Positions", default)]
    pub position: Point,
    #[serde(rename = "SheetInformation", default)]
    pub sheet: Option<SheetRef>,
    #[serde(rename = "isActive", default)]
    pub is_active: bool,
    #[serde(rename = "AlwaysEnable", default)]
    pub always_enable: bool,
    #[serde(rename = "Hint", default)]
    pub hint: Option<String>,
    #[serde(rename = "OpacityDisable", default = "one")]
    pub opacity_disable: f32,

    #[serde(rename = "Label", default)]
    pub label: Option<String>,
    #[serde(rename = "LabelCenter", default)]
    pub label_center: bool,
    #[serde(rename = "AlternativeLabel", default)]
    pub alternative_label: Option<String>,
    #[serde(rename = "GlobalLabel", default)]
    pub global_label: Option<String>,
    #[serde(rename = "NextItems", default)]
    pub next_items: Vec<NextItem>,

    #[serde(rename = "Increment", default)]
    pub increments: Vec<String>,
    #[serde(rename = "StartIncrementIndex", default)]
    pub start_increment_index: Option<i64>,

    #[serde(rename = "LabelList", default)]
    pub label_list: Vec<String>,
    #[serde(rename = "OffsetLabel", default)]
    pub label_offset: f32,

    #[serde(rename = "valueMin", default)]
    pub value_min: i64,
    #[serde(rename = "valueMax", default)]
    pub value_max: i64,
    #[serde(rename = "valueIncrease", default)]
    pub value_increase: i64,
    #[serde(rename = "valueStart", default)]
    pub value_start: i64,

    #[serde(rename = "maxValue", default)]
    pub max_value: i64,
    #[serde(rename = "maxValueAlternate", default)]
    pub max_value_alternate: Option<i64>,
    #[serde(rename = "customFont", default)]
    pub custom_font: Option<String>,

    #[serde(rename = "CheckImageSheetInformation", default)]
    pub check_sheet: Option<SheetRef>,
    #[serde(rename = "BackgroundGlow", default)]
    pub glow: Option<String>,

    #[serde(rename = "Background", default)]
    pub background: Option<String>,
    #[serde(rename = "ItemsList", default)]
    pub items_list: Vec<ItemDef>,
    #[serde(rename = "ShowNumbersOfItemsActive", default)]
    pub show_numbers_items_active: bool,
    #[serde(rename = "ShowNumberOfCheckedItems", default)]
    pub show_number_of_checked_items: bool,

    #[serde(rename = "Link", default)]
    pub link: Option<String>,

    #[serde(rename = "Sizes", default)]
    pub size: BoxSize,
    #[serde(rename = "Lines", default)]
    pub lines: Vec<String>,
    #[serde(rename = "PlaceHolder", default)]
    pub placeholder: Option<String>,
    #[serde(rename = "Style", default)]
    pub style: Option<BoxStyle>,

    /// Shown and hidden with the middle click, like the hint.
    #[serde(rename = "HintItems", default)]
    pub hint_items: Vec<ItemDef>,
    /// Shown while this item is active.
    #[serde(rename = "ActiveItems", default)]
    pub active_items: Vec<ItemDef>,
    /// Shown while this item is inactive.
    #[serde(rename = "InactiveItems", default)]
    pub inactive_items: Vec<ItemDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arrow {
    #[serde(rename = "Image")]
    pub image: String,
    #[serde(rename = "Positions")]
    pub position: Point,
}

/// A paged list drawn over the tracker: map checks, rules or map names.
#[derive(Debug, Clone, Deserialize)]
pub struct ListBox {
    #[serde(rename = "SubMenuBackground")]
    pub background: String,
    #[serde(rename = "DrawBoxRect")]
    pub draw_box: Rect,
    #[serde(rename = "DrawBoxRectSubTitle", default)]
    pub draw_box_subtitle: Option<Rect>,
    #[serde(rename = "LabelY")]
    pub label_y: f32,
    #[serde(rename = "LeftArrow")]
    pub left_arrow: Arrow,
    #[serde(rename = "RightArrow")]
    pub right_arrow: Arrow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulesList {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "ButtonRect")]
    pub button: Rect,
    #[serde(rename = "ListBox")]
    pub list: ListBox,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HiddenChecks {
    #[serde(rename = "Kind")]
    pub kind: String,
    /// The block, for `Kind: Block`.
    #[serde(rename = "Name", default)]
    pub name: Option<String>,
    #[serde(rename = "Checks", default)]
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum RuleAction {
    SetRule {
        #[serde(rename = "RuleName")]
        rule: String,
        #[serde(rename = "Active", default)]
        active: bool,
    },
    SetLeftClick(ItemAction),
    SetRightClick(ItemAction),
    SetWheelClick(ItemAction),
    ResetItem(ItemAction),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemAction {
    #[serde(rename = "Item")]
    pub item: String,
    #[serde(rename = "Counter", default)]
    pub counter: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDef {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "HideChecks", default)]
    pub hide_checks: Option<Vec<HiddenChecks>>,
    #[serde(rename = "Actions", default)]
    pub actions: Option<Vec<RuleAction>>,
    #[serde(rename = "Active", default)]
    pub active: bool,
    #[serde(rename = "CanBeClickable", default = "yes")]
    pub clickable: bool,
    #[serde(rename = "ParentListName", default)]
    pub list: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapRef {
    #[serde(rename = "Id")]
    pub id: usize,
    #[serde(rename = "Datas")]
    pub file: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapsList {
    #[serde(rename = "MapListButtonLabelRect")]
    pub button: Rect,
    #[serde(rename = "MapsListBox")]
    pub list: ListBox,
}

/// The fifth section, present on map templates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MapsSection {
    #[serde(rename = "Maps", default)]
    pub maps: Vec<MapRef>,
    #[serde(rename = "MapsList", default)]
    pub maps_list: Option<MapsList>,
    #[serde(rename = "RulesOptionsLists", default)]
    pub rules_lists: Vec<RulesList>,
    #[serde(rename = "RulesOptions", default)]
    pub rules: Vec<RuleDef>,
    #[serde(rename = "SizeGroupChecks", default)]
    pub block_size: BoxSize,
    #[serde(rename = "SizeSimpleCheck", default)]
    pub check_size: BoxSize,
    #[serde(rename = "CptChecksPosition", default)]
    pub counter_position: Point,
    #[serde(rename = "ActionsConditions", default)]
    pub actions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubCheckDef {
    #[serde(rename = "Id")]
    pub id: i64,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Conditions")]
    pub conditions: String,
    #[serde(rename = "Group", default)]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "Kind")]
pub enum CheckDef {
    Block {
        #[serde(rename = "Id")]
        id: i64,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Zone", default)]
        zone: Option<String>,
        #[serde(rename = "Positions")]
        position: Point,
        #[serde(rename = "Checks", default)]
        checks: Vec<SubCheckDef>,
    },
    SimpleCheck {
        #[serde(rename = "Id")]
        id: i64,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Zone", default)]
        zone: Option<String>,
        #[serde(rename = "Positions")]
        position: Point,
        #[serde(rename = "Conditions")]
        conditions: String,
        #[serde(rename = "Group", default)]
        group: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapDatas {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Background")]
    pub background: String,
    #[serde(flatten)]
    pub list: ListBox,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapDef {
    #[serde(rename = "Datas")]
    pub datas: MapDatas,
    #[serde(rename = "ChecksList")]
    pub checks: Vec<CheckDef>,
}

impl MapDef {
    /// A map file is a one-element array.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let (map,): (MapDef,) = serde_json::from_str(strip_bom(json))?;
        Ok(map)
    }
}

#[derive(Debug, Clone)]
pub struct Template {
    pub info: Info,
    pub dimensions: Size,
    pub background: String,
    pub background_color: Rgb,
    pub sheets: BTreeMap<String, SheetDef>,
    pub fonts: Fonts,
    pub items: Vec<ItemDef>,
    pub maps: Option<MapsSection>,
}

#[derive(Deserialize)]
struct InfoSection {
    #[serde(rename = "Informations")]
    info: Info,
}

#[derive(Deserialize)]
struct Datas {
    #[serde(rename = "Dimensions")]
    dimensions: Size,
    #[serde(rename = "Background")]
    background: String,
    #[serde(rename = "BackgroundColor")]
    background_color: Rgb,
    #[serde(rename = "Items")]
    sheets: BTreeMap<String, SheetDef>,
}

#[derive(Deserialize)]
struct DatasSection {
    #[serde(rename = "Datas")]
    datas: Datas,
}

#[derive(Deserialize)]
struct FontsSection {
    #[serde(rename = "Fonts")]
    fonts: Fonts,
}

#[derive(Deserialize)]
struct ItemsSection {
    #[serde(rename = "Items")]
    items: Vec<ItemDef>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Sections {
    WithMaps(
        InfoSection,
        DatasSection,
        FontsSection,
        ItemsSection,
        Box<MapsSection>,
    ),
    Plain(InfoSection, DatasSection, FontsSection, ItemsSection),
}

/// Some templates were saved with a byte order mark.
fn strip_bom(json: &str) -> &str {
    json.strip_prefix('\u{feff}').unwrap_or(json)
}

impl Template {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_str(strip_bom(json))?;
        let (info, datas, fonts, items, maps) = match serde_json::from_value(value)? {
            Sections::WithMaps(info, datas, fonts, items, maps) => {
                (info, datas, fonts, items, Some(*maps))
            }
            Sections::Plain(info, datas, fonts, items) => (info, datas, fonts, items, None),
        };
        Ok(Self {
            info: info.info,
            dimensions: datas.datas.dimensions,
            background: datas.datas.background,
            background_color: datas.datas.background_color,
            sheets: datas.datas.sheets,
            fonts: fonts.fonts,
            items: items.items,
            maps,
        })
    }

    /// Fonts by the name items and maps refer to them, custom ones included.
    pub fn all_fonts(&self) -> impl Iterator<Item = (&String, &FontDef)> {
        self.fonts.named.iter().chain(self.fonts.custom.iter())
    }

    /// Every file needed before the maps are read, deduplicated.
    pub fn files(&self) -> Vec<String> {
        fn item_files(item: &ItemDef, out: &mut Vec<String>) {
            out.extend(item.glow.clone());
            out.extend(item.background.clone());
            for child in item
                .items_list
                .iter()
                .chain(&item.hint_items)
                .chain(&item.active_items)
                .chain(&item.inactive_items)
            {
                item_files(child, out);
            }
        }
        let mut files = vec![self.background.clone()];
        files.extend(self.sheets.values().map(|sheet| sheet.image.clone()));
        files.extend(self.all_fonts().map(|(_, font)| font.file.clone()));
        for item in &self.items {
            item_files(item, &mut files);
        }
        if let Some(maps) = &self.maps {
            files.extend(maps.maps.iter().map(|map| map.file.clone()));
            for list in maps.rules_lists.iter().map(|rules| &rules.list) {
                files.extend(list_files(list));
            }
            if let Some(maps_list) = &maps.maps_list {
                files.extend(list_files(&maps_list.list));
            }
        }
        files.sort();
        files.dedup();
        files
    }
}

pub fn list_files(list: &ListBox) -> [String; 3] {
    [
        list.background.clone(),
        list.left_arrow.image.clone(),
        list.right_arrow.image.clone(),
    ]
}

impl MapDef {
    pub fn files(&self) -> Vec<String> {
        let mut files = vec![self.datas.background.clone()];
        files.extend(list_files(&self.datas.list));
        files
    }
}
