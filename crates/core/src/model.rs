//! Typed view of the `tracker.json` files used by the web app.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SheetDef {
    #[serde(rename = "ItemsSheet")]
    pub image: String,
    #[serde(rename = "ItemsSheetDimensions")]
    pub cell: Size,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontColors {
    #[serde(rename = "Normal")]
    pub normal: Rgb,
    #[serde(rename = "Max")]
    pub max: Rgb,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontDef {
    #[serde(rename = "Name")]
    pub file: String,
    #[serde(rename = "Colors")]
    pub colors: FontColors,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SheetRef {
    pub row: u32,
    pub column: u32,
    #[serde(rename = "SpriteSheet")]
    pub sheet: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Kind {
    Item,
    IncrementalItem,
    EvolutionItem,
    AlternateEvolutionItem,
    CountItem,
    LabelItem,
    CheckItem,
    GoModeItem,
    AlternateCountItem,
    /// Present in the data but not rendered (`SubMenuItem`, nested evolutions).
    #[default]
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    #[serde(rename = "Id", default)]
    pub id: u32,
    #[serde(rename = "Kind", default)]
    pub kind: Kind,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Positions", default)]
    pub position: Point,
    #[serde(rename = "SheetInformation")]
    pub sheet: SheetRef,
    #[serde(rename = "isActive", default)]
    pub is_active: bool,
    #[serde(rename = "OpacityDisable", default = "one")]
    pub opacity_disabled: f32,
    #[serde(rename = "Label", default)]
    pub label: Option<String>,
    #[serde(rename = "LabelSize", default)]
    pub label_size: Option<f32>,
    #[serde(rename = "OffsetLabel", default)]
    pub label_offset: Option<f32>,
    #[serde(rename = "LabelList", default)]
    pub labels: Vec<String>,
    #[serde(rename = "LabelDefault", default)]
    pub label_default: Option<i64>,
    #[serde(rename = "GlobalLabel", default)]
    pub global_label: Option<String>,
    #[serde(rename = "Increment", default)]
    pub increments: Vec<String>,
    #[serde(rename = "NextItems", default)]
    pub next: Vec<Item>,
    #[serde(rename = "CheckImageSheetInformation", default)]
    pub check_sheet: Option<SheetRef>,
    #[serde(rename = "BackgroundGlow", default)]
    pub glow: Option<String>,
    #[serde(rename = "valueMax", default)]
    pub value_max: Option<i64>,
    #[serde(rename = "valueIncrease", default)]
    pub value_step: Option<i64>,
    #[serde(rename = "valueStart", default)]
    pub value_start: Option<i64>,
    #[serde(rename = "maxValue", default)]
    pub max: Option<i64>,
    #[serde(rename = "maxValueAlternate", default)]
    pub max_alternate: Option<i64>,
    #[serde(rename = "labelPosition", default)]
    pub label_position: Option<String>,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct Info {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Creator")]
    pub creator: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Comments", default)]
    pub description: Option<String>,
}

#[derive(Deserialize)]
struct InfoSection {
    #[serde(rename = "Informations")]
    info: Info,
}

#[derive(Debug, Clone)]
pub struct Tracker {
    pub info: Info,
    pub dimensions: Size,
    pub background: String,
    pub background_color: Rgb,
    pub sheets: BTreeMap<String, SheetDef>,
    pub fonts: BTreeMap<String, FontDef>,
    pub items: Vec<Item>,
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
    fonts: BTreeMap<String, FontDef>,
}

#[derive(Deserialize)]
struct ItemsSection {
    #[serde(rename = "Items")]
    items: Vec<Item>,
}

impl Tracker {
    /// The file is a positional array: `[informations, datas, fonts, items]`.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let (info, datas, fonts, items): (InfoSection, DatasSection, FontsSection, ItemsSection) =
            serde_json::from_str(json)?;
        Ok(Self {
            info: info.info,
            dimensions: datas.datas.dimensions,
            background: datas.datas.background,
            background_color: datas.datas.background_color,
            sheets: datas.datas.sheets,
            fonts: fonts.fonts,
            items: items.items,
        })
    }

    /// Every file of the tracker folder needed to display it, deduplicated.
    pub fn files(&self) -> Vec<String> {
        let mut files: Vec<String> = std::iter::once(self.background.clone())
            .chain(self.sheets.values().map(|sheet| sheet.image.clone()))
            .chain(self.fonts.values().map(|font| font.file.clone()))
            .chain(self.items.iter().filter_map(|item| item.glow.clone()))
            .collect();
        files.sort();
        files.dedup();
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn tracker_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                tracker_files(&path, out);
            } else if path.file_name().is_some_and(|n| n == "tracker.json") {
                out.push(path);
            }
        }
    }

    #[test]
    fn parses_every_tracker_in_the_repository() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/sources");
        let mut files = Vec::new();
        tracker_files(&root, &mut files);
        assert!(files.len() >= 12, "found only {} trackers", files.len());
        for file in files {
            let json = std::fs::read_to_string(&file).unwrap();
            let tracker = Tracker::parse(&json).unwrap_or_else(|e| panic!("{file:?}: {e}"));
            assert!(!tracker.items.is_empty(), "{file:?}");
            for item in &tracker.items {
                assert!(
                    tracker.sheets.contains_key(&item.sheet.sheet),
                    "{file:?}: unknown sheet for {}",
                    item.name
                );
            }
        }
    }
}
