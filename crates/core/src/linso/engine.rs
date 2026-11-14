//! Item, map and rule behaviour, ported from LinSoTracker's `Entities` and
//! `Engine/Tracker.py`. Only state lives here; layout and drawing belong to
//! the client.

use super::expr::{Comparison, Expr};
use super::model::{
    CheckDef, HiddenChecks, ItemDef, Kind, MapDatas, MapDef, RuleAction, RulesList, Template,
};
use crate::model::Point;
use std::cell::RefCell;
use std::collections::HashMap;

pub type ItemId = usize;

/// Guards `do` and `SetRule` chains that refer back to themselves.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    WheelUp,
    WheelDown,
}

/// What was dropped on a draggable item or a map check.
#[derive(Debug, Clone, PartialEq)]
pub struct Dragged {
    pub name: String,
    pub base_name: String,
    /// The evolution stage of the dropped item, `None` for other kinds.
    pub index: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemState {
    /// `show_item`: hidden children are neither drawn nor clickable.
    pub show: bool,
    pub enable: bool,
    pub hint_show: bool,
    /// The current name, which follows the evolution stage.
    pub name: String,
    pub label: Option<String>,
    pub increments_position: i64,
    pub next_index: i64,
    /// `CountItem` and `AlternateCountItem`.
    pub value: i64,
    pub used_max: i64,
    pub check: bool,
    pub label_count: usize,
    pub dragged: Option<Dragged>,
    /// Whether a `SubMenuItem` is open.
    pub open: bool,
    /// Text of an `EditableBox`.
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub def: ItemDef,
    pub state: ItemState,
    initial: ItemState,
    pub hint_children: Vec<ItemId>,
    pub active_children: Vec<ItemId>,
    pub inactive_children: Vec<ItemId>,
    /// Items of a `SubMenuItem`, in drawing order.
    pub submenu: Vec<ItemId>,
}

impl Node {
    pub fn base_enable(&self) -> bool {
        self.def.always_enable || self.def.is_active
    }

    /// `can_drag` in LinSoTracker.
    pub fn can_drag(&self) -> bool {
        !matches!(
            self.def.kind,
            Kind::DraggableEvolutionItem
                | Kind::GoModeItem
                | Kind::ImageItem
                | Kind::OpenLinkItem
                | Kind::SubMenuItem
                | Kind::EditableBox
        )
    }

    /// The `value` attribute used by `have(name, '>=2')`.
    fn value(&self) -> Option<i64> {
        match self.def.kind {
            Kind::IncrementalItem => Some(self.state.increments_position),
            Kind::CountItem | Kind::AlternateCountItem => Some(self.state.value),
            Kind::AlternateEvolutionItem => Some(self.state.next_index + 1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logic {
    Done,
    Logic,
    NotLogic,
}

#[derive(Debug, Clone)]
pub struct SubCheck {
    pub id: i64,
    pub name: String,
    expr: Expr,
    pub group: Option<String>,
    pub checked: bool,
    pub hide: bool,
    pub focused: bool,
    pub dragged: Option<Dragged>,
    pub logic: Logic,
}

impl SubCheck {
    fn new(id: i64, name: String, conditions: &str, group: Option<String>) -> Self {
        // A condition the parser cannot read is never in logic, rather than
        // making the whole template unusable.
        let expr = Expr::parse(conditions).unwrap_or(Expr::Const(false));
        Self {
            id,
            name,
            expr,
            group,
            checked: false,
            hide: false,
            focused: false,
            dragged: None,
            logic: Logic::NotLogic,
        }
    }

    fn open(&self) -> bool {
        !self.hide && !self.checked
    }
}

#[derive(Debug, Clone)]
pub enum CheckKind {
    Simple(SubCheck),
    Block(Vec<SubCheck>),
}

#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub zone: Option<String>,
    pub position: Point,
    pub kind: CheckKind,
}

/// Colour of a block of checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockLogic {
    Done,
    Logic,
    HaveLogic,
    NotLogic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockState {
    pub logic: BlockLogic,
    /// Open checks in logic, drawn on the block.
    pub in_logic: usize,
    pub focused: bool,
    pub has_dragged: bool,
    pub all_hidden: bool,
}

impl Check {
    pub fn block_state(&self) -> Option<BlockState> {
        let CheckKind::Block(checks) = &self.kind else {
            return None;
        };
        let open: Vec<&SubCheck> = checks.iter().filter(|c| c.open()).collect();
        let in_logic = open.iter().filter(|c| c.logic == Logic::Logic).count();
        let logic = if open.is_empty() {
            BlockLogic::Done
        } else if in_logic == open.len() {
            BlockLogic::Logic
        } else if in_logic > 0 {
            BlockLogic::HaveLogic
        } else {
            BlockLogic::NotLogic
        };
        Some(BlockState {
            logic,
            in_logic,
            focused: open.iter().any(|c| c.focused),
            has_dragged: checks.iter().any(|c| c.dragged.is_some() && !c.hide),
            all_hidden: checks.iter().all(|c| c.hide),
        })
    }

    pub fn checks(&self) -> &[SubCheck] {
        match &self.kind {
            CheckKind::Simple(check) => std::slice::from_ref(check),
            CheckKind::Block(checks) => checks,
        }
    }

    fn checks_mut(&mut self) -> &mut [SubCheck] {
        match &mut self.kind {
            CheckKind::Simple(check) => std::slice::from_mut(check),
            CheckKind::Block(checks) => checks,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Map {
    pub datas: MapDatas,
    pub checks: Vec<Check>,
}

/// A check is either a simple check, or one check of a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckRef {
    pub map: usize,
    pub check: usize,
    /// Index inside the block; 0 for a simple check.
    pub sub: usize,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    /// LinSoTracker's naming: a checked rule is *off*.
    pub checked: bool,
    pub clickable: bool,
    hide_checks: Vec<HiddenChecks>,
    actions: Vec<RuleAction>,
}

impl Rule {
    pub fn is_active(&self) -> bool {
        !self.checked
    }
}

#[derive(Debug, Clone)]
pub struct RuleList {
    pub def: RulesList,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    Item(ItemId),
    Check(CheckRef),
}

pub struct Engine {
    pub template: Template,
    pub nodes: Vec<Node>,
    /// Top-level items and every shown-on-condition child, in drawing order.
    pub main: Vec<ItemId>,
    pub maps: Vec<Map>,
    pub rule_lists: Vec<RuleList>,
    actions: HashMap<String, Expr>,
    /// While rule lists are being built, LinSoTracker only finds rules of the
    /// lists already complete.
    searchable_lists: usize,
    logic_stale: bool,
    /// What the engine was built from, for [`Engine::reset`].
    source: (Template, Vec<MapDef>),
}

fn initial_state(def: &ItemDef) -> ItemState {
    let enable = def.always_enable || def.is_active;
    ItemState {
        show: true,
        // `GoModeItem` ignores `AlwaysEnable`.
        enable: if def.kind == Kind::GoModeItem {
            def.is_active
        } else {
            enable
        },
        hint_show: false,
        name: def.name.clone(),
        label: def.label.clone(),
        increments_position: -1,
        next_index: -1,
        value: match def.kind {
            Kind::CountItem => def.value_start,
            _ => 0,
        },
        used_max: def.max_value,
        check: false,
        label_count: 0,
        dragged: None,
        open: false,
        text: String::new(),
    }
}

impl Engine {
    /// `maps` are the parsed files of the fifth section, in its order.
    pub fn new(template: Template, maps: Vec<MapDef>) -> Self {
        let source = (template.clone(), maps.clone());
        let mut template = template;
        let items = std::mem::take(&mut template.items);
        let actions = template
            .maps
            .as_ref()
            .map(|section| {
                section
                    .actions
                    .iter()
                    .map(|(name, source)| {
                        let expr = Expr::parse(source).unwrap_or(Expr::Const(false));
                        (name.clone(), expr)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut engine = Self {
            template,
            nodes: Vec::new(),
            main: Vec::new(),
            maps: Vec::new(),
            rule_lists: Vec::new(),
            actions,
            searchable_lists: 0,
            logic_stale: true,
            source,
        };
        engine.main = engine.init_items(items);
        for id in engine.main.clone() {
            engine.update(id);
        }
        engine.init_maps(maps);
        engine
    }

    /// `Tracker.init_items`: items first, then their conditional children.
    fn init_items(&mut self, defs: Vec<ItemDef>) -> Vec<ItemId> {
        let mut group: Vec<ItemId> = defs.into_iter().map(|def| self.init_item(def)).collect();
        for id in group.clone() {
            let show = self.nodes[id].state.show;
            self.add_children(id, &mut group, Children::Hint, false);
            self.add_children(id, &mut group, Children::Active, false);
            self.add_children(id, &mut group, Children::Inactive, show);
        }
        group
    }

    fn init_item(&mut self, mut def: ItemDef) -> ItemId {
        let submenu_defs = std::mem::take(&mut def.items_list);
        let submenu = if def.kind == Kind::SubMenuItem {
            self.init_items(submenu_defs)
        } else {
            Vec::new()
        };
        let state = initial_state(&def);
        self.nodes.push(Node {
            initial: state.clone(),
            state,
            def,
            hint_children: Vec::new(),
            active_children: Vec::new(),
            inactive_children: Vec::new(),
            submenu,
        });
        self.nodes.len() - 1
    }

    /// `Tracker.add_sub_special_item`.
    fn add_children(&mut self, id: ItemId, group: &mut Vec<ItemId>, which: Children, show: bool) {
        let defs = std::mem::take(which.defs_mut(&mut self.nodes[id].def));
        if defs.is_empty() {
            return;
        }
        let mut children = Vec::new();
        for def in defs {
            let child = self.init_item(def);
            self.nodes[child].state.show = show;
            group.push(child);
            children.push(child);
            self.add_children(child, group, Children::Hint, false);
            self.add_children(child, group, Children::Active, false);
            self.add_children(child, group, Children::Inactive, show);
        }
        *which.ids_mut(&mut self.nodes[id]) = children;
    }

    fn init_maps(&mut self, maps: Vec<MapDef>) {
        for def in maps {
            let checks = def
                .checks
                .into_iter()
                .map(|check| match check {
                    CheckDef::Block {
                        name,
                        zone,
                        position,
                        checks,
                        ..
                    } => Check {
                        name,
                        zone,
                        position,
                        kind: CheckKind::Block(
                            checks
                                .into_iter()
                                .map(|c| SubCheck::new(c.id, c.name, &c.conditions, c.group))
                                .collect(),
                        ),
                    },
                    CheckDef::SimpleCheck {
                        id,
                        name,
                        zone,
                        position,
                        conditions,
                        group,
                    } => Check {
                        name: name.clone(),
                        zone,
                        position,
                        kind: CheckKind::Simple(SubCheck::new(id, name, &conditions, group)),
                    },
                })
                .collect();
            self.maps.push(Map {
                datas: def.datas,
                checks,
            });
        }

        let Some(section) = self.template.maps.clone() else {
            return;
        };
        for list in section.rules_lists {
            self.searchable_lists = self.rule_lists.len();
            let name = list.name.clone();
            self.rule_lists.push(RuleList {
                def: list,
                rules: Vec::new(),
            });
            let index = self.rule_lists.len() - 1;
            for def in section
                .rules
                .iter()
                .filter(|r| r.list.as_deref() == Some(name.as_str()))
            {
                self.rule_lists[index].rules.push(Rule {
                    name: def.name.clone(),
                    checked: true,
                    clickable: def.clickable,
                    hide_checks: def.hide_checks.clone().unwrap_or_default(),
                    actions: def.actions.clone().unwrap_or_default(),
                });
                let rule = (index, self.rule_lists[index].rules.len() - 1);
                self.set_hidden_checks(rule);
                if def.active {
                    self.toggle_rule(rule, true);
                }
            }
        }
        self.searchable_lists = self.rule_lists.len();
        self.logic_stale = true;
    }

    pub fn is_map_template(&self) -> bool {
        !self.maps.is_empty()
    }

    pub fn item(&self, id: ItemId) -> &Node {
        &self.nodes[id]
    }

    pub fn set_text(&mut self, id: ItemId, text: String) {
        self.nodes[id].state.text = text;
    }

    /// The open submenu, which takes every click while it is shown.
    pub fn open_submenu(&self) -> Option<ItemId> {
        self.main
            .iter()
            .copied()
            .find(|&id| self.nodes[id].def.kind == Kind::SubMenuItem && self.nodes[id].state.open)
    }

    pub fn close_submenu(&mut self, id: ItemId) {
        self.nodes[id].state.open = false;
    }

    /// Back to the state the template starts in.
    pub fn reset(&mut self) {
        let (template, maps) = self.source.clone();
        *self = Self::new(template, maps);
    }

    // ---- items --------------------------------------------------------

    /// A click or a wheel notch on an item, as `Tracker.items_click` does it.
    pub fn click(&mut self, id: ItemId, button: Button) {
        let kind = self.nodes[id].def.kind;
        if kind == Kind::ImageItem {
            return;
        }
        match button {
            Button::Left | Button::WheelUp => self.left_click(id),
            Button::Right => self.right_click(id),
            Button::WheelDown if kind == Kind::LabelItem => {
                let len = self.nodes[id].def.label_list.len();
                let state = &mut self.nodes[id].state;
                state.label_count = if state.label_count > 0 {
                    state.label_count - 1
                } else {
                    len.saturating_sub(1)
                };
                self.update(id);
            }
            Button::WheelDown => self.right_click(id),
            Button::Middle => self.wheel_click(id),
        }
        self.logic_stale = true;
    }

    /// The link an `OpenLinkItem` opens on a left click or wheel up.
    pub fn link(&self, id: ItemId, button: Button) -> Option<&str> {
        let node = &self.nodes[id];
        (node.def.kind == Kind::OpenLinkItem && matches!(button, Button::Left | Button::WheelUp))
            .then_some(node.def.link.as_deref())
            .flatten()
    }

    fn set_stage(&mut self, id: ItemId, index: i64) {
        let node = &mut self.nodes[id];
        node.state.next_index = index;
        match usize::try_from(index)
            .ok()
            .and_then(|i| node.def.next_items.get(i))
        {
            Some(next) => {
                node.state.name = next.name.clone();
                node.state.label = next.label.clone();
            }
            None => {
                node.state.name = node.def.name.clone();
                node.state.label = node.def.label.clone();
            }
        }
    }

    fn left_click(&mut self, id: ItemId) {
        let node = &mut self.nodes[id];
        let always = node.def.always_enable && node.def.kind != Kind::GoModeItem;
        match node.def.kind {
            Kind::Item | Kind::GoModeItem | Kind::CheckItem | Kind::LabelItem => {
                if !always {
                    node.state.enable = !node.state.enable;
                }
            }
            Kind::IncrementalItem => {
                let state = &mut node.state;
                if state.enable {
                    if state.increments_position < node.def.increments.len() as i64 - 1 {
                        state.increments_position += 1;
                    } else {
                        state.increments_position = -1;
                        if !always {
                            state.enable = false;
                        }
                    }
                } else {
                    state.enable = true;
                    if let Some(start) = node.def.start_increment_index {
                        state.increments_position = start;
                    }
                }
            }
            Kind::EvolutionItem | Kind::DraggableEvolutionItem => {
                let len = node.def.next_items.len() as i64;
                if node.state.enable {
                    if node.state.next_index < len - 1 {
                        let next = node.state.next_index + 1;
                        self.set_stage(id, next);
                    } else {
                        self.set_stage(id, -1);
                        if !always {
                            self.nodes[id].state.enable = false;
                        }
                    }
                } else {
                    node.state.enable = true;
                }
                if self.nodes[id].def.kind == Kind::DraggableEvolutionItem {
                    self.clear_dropped(id);
                }
            }
            Kind::AlternateEvolutionItem => node.state.enable = !node.state.enable,
            Kind::CountItem => {
                let def = &node.def;
                node.state.value = if node.state.value < def.value_max {
                    node.state.value + def.value_increase
                } else {
                    def.value_min
                };
            }
            Kind::AlternateCountItem => {
                let state = &mut node.state;
                if state.value < state.used_max {
                    state.value += 1;
                    state.enable = true;
                } else {
                    state.value = 0;
                    state.enable = false;
                }
            }
            Kind::SubMenuItem => node.state.open = !node.state.open,
            Kind::ImageItem | Kind::OpenLinkItem | Kind::EditableBox => {}
        }
        self.update(id);
    }

    fn right_click(&mut self, id: ItemId) {
        let node = &mut self.nodes[id];
        let always = node.def.always_enable && node.def.kind != Kind::GoModeItem;
        match node.def.kind {
            Kind::Item | Kind::GoModeItem => {
                self.left_click(id);
                return;
            }
            Kind::IncrementalItem => {
                let last = node.def.increments.len() as i64 - 1;
                let state = &mut node.state;
                if !state.enable {
                    state.enable = true;
                    state.increments_position = last;
                } else if state.increments_position >= 0 {
                    state.increments_position -= 1;
                    if node
                        .def
                        .start_increment_index
                        .is_some_and(|start| state.increments_position == start - 1)
                    {
                        state.increments_position = -1;
                        state.enable = false;
                    }
                } else if !always {
                    state.enable = false;
                } else {
                    state.increments_position = last;
                }
            }
            Kind::EvolutionItem | Kind::DraggableEvolutionItem => {
                let last = node.def.next_items.len() as i64 - 1;
                let index = node.state.next_index;
                if !node.state.enable {
                    node.state.enable = true;
                    self.set_stage(id, last);
                } else if index >= 0 {
                    self.set_stage(id, index - 1);
                } else if !always {
                    node.state.enable = false;
                    self.set_stage(id, -1);
                } else {
                    self.set_stage(id, last);
                }
                if self.nodes[id].def.kind == Kind::DraggableEvolutionItem {
                    self.clear_dropped(id);
                }
            }
            Kind::AlternateEvolutionItem => {
                let len = node.def.next_items.len() as i64;
                let index = node.state.next_index;
                node.state.next_index = if (-1..len - 1).contains(&index) {
                    index + 1
                } else {
                    -1
                };
            }
            Kind::CountItem => {
                let def = &node.def;
                node.state.value = if node.state.value > def.value_min {
                    node.state.value - def.value_increase
                } else {
                    def.value_max
                };
            }
            Kind::AlternateCountItem => {
                let state = &mut node.state;
                state.value = if state.value > 0 {
                    state.value - 1
                } else {
                    state.used_max
                };
                state.enable = state.value != 0;
            }
            Kind::CheckItem => node.state.check = !node.state.check,
            Kind::LabelItem => {
                let len = node.def.label_list.len();
                let state = &mut node.state;
                state.label_count = if state.label_count + 1 < len {
                    state.label_count + 1
                } else {
                    0
                };
            }
            Kind::ImageItem | Kind::OpenLinkItem | Kind::SubMenuItem | Kind::EditableBox => {}
        }
        self.update(id);
    }

    fn wheel_click(&mut self, id: ItemId) {
        let kind = self.nodes[id].def.kind;
        if matches!(
            kind,
            Kind::ImageItem | Kind::OpenLinkItem | Kind::SubMenuItem | Kind::EditableBox
        ) {
            return;
        }
        if self.nodes[id].def.hint.is_some() {
            let state = &mut self.nodes[id].state;
            state.hint_show = !state.hint_show;
            for child in self.nodes[id].hint_children.clone() {
                if self.nodes[child].state.show {
                    self.close_hint_children(id);
                } else {
                    self.nodes[child].state.show = true;
                    self.update(child);
                }
            }
        }
        if kind == Kind::AlternateCountItem {
            let node = &mut self.nodes[id];
            node.state.used_max = match (node.state.hint_show, node.def.max_value_alternate) {
                (true, Some(alternate)) => alternate,
                _ => node.def.max_value,
            };
            node.state.value = node.state.value.min(node.state.used_max);
        }
        self.update(id);
    }

    fn close_hint_children(&mut self, id: ItemId) {
        for child in self.nodes[id].hint_children.clone() {
            self.reinitialize(child);
            self.update(child);
            self.close_hint_children(child);
        }
    }

    fn clear_dropped(&mut self, id: ItemId) {
        let node = &mut self.nodes[id];
        if node.state.dragged.take().is_some() {
            node.state.next_index = -1;
            node.state.name = node.def.name.clone();
        }
    }

    /// `Item.reinitialize`, for children that get hidden.
    fn reinitialize(&mut self, id: ItemId) {
        let base_enable = self.nodes[id].base_enable();
        let node = &mut self.nodes[id];
        let state = &mut node.state;
        state.hint_show = false;
        state.show = false;
        state.enable = if node.def.kind == Kind::GoModeItem {
            node.def.is_active
        } else {
            base_enable
        };
        match node.def.kind {
            Kind::IncrementalItem => state.increments_position = -1,
            Kind::EvolutionItem | Kind::DraggableEvolutionItem | Kind::AlternateEvolutionItem => {
                state.next_index = -1;
                state.name = node.def.name.clone();
                state.label = node.def.label.clone();
            }
            Kind::CountItem => state.value = node.def.value_start,
            Kind::AlternateCountItem => state.value = 0,
            Kind::CheckItem => state.check = false,
            Kind::LabelItem => state.label_count = 0,
            _ => {}
        }
    }

    /// `Item.update`: a shown item shows the children of its current state.
    fn update(&mut self, id: ItemId) {
        if !self.nodes[id].state.show {
            return;
        }
        let enable = self.nodes[id].state.enable;
        self.set_children_visible(id, Children::Active, enable);
        self.set_children_visible(id, Children::Inactive, !enable);
    }

    fn set_children_visible(&mut self, id: ItemId, which: Children, visible: bool) {
        for child in which.ids(&self.nodes[id]).to_vec() {
            self.nodes[child].state.show = visible;
            if !visible {
                self.reinitialize(child);
            }
            self.update(child);
            if !visible {
                self.set_children_visible(child, which, false);
            }
        }
    }

    /// `Item.reset`: back to the item's starting state.
    fn reset_item(&mut self, id: ItemId) {
        let node = &mut self.nodes[id];
        node.state = node.initial.clone();
        self.update(id);
    }

    /// `Tracker.find_item`: by current name, or by template name.
    pub fn find_item(&self, name: &str, base_name: bool) -> Option<ItemId> {
        let matches = |id: &ItemId| {
            let node = &self.nodes[*id];
            if base_name {
                node.def.name == name
            } else {
                node.state.name == name
            }
        };
        for &id in &self.main {
            let node = &self.nodes[id];
            if node.def.kind == Kind::SubMenuItem {
                if let Some(&found) = node.submenu.iter().find(|id| matches(id)) {
                    return Some(found);
                }
            } else if matches(&id) {
                return Some(id);
            }
        }
        None
    }

    // ---- drag and drop ------------------------------------------------

    pub fn drop_on(&mut self, source: ItemId, target: DropTarget) {
        let node = &self.nodes[source];
        let dragged = Dragged {
            name: node.state.name.clone(),
            base_name: node.def.name.clone(),
            index: node.def.kind.evolves().then_some(node.state.next_index),
        };
        match target {
            DropTarget::Item(id) => {
                let state = &mut self.nodes[id].state;
                state.enable = true;
                state.next_index = -1;
                state.dragged = Some(dragged);
                self.update(id);
            }
            DropTarget::Check(at) => {
                self.sub_check_mut(at).dragged = Some(dragged);
            }
        }
        self.logic_stale = true;
    }

    /// The item and stage a dropped reference points to, for drawing.
    pub fn dropped_item(&self, dragged: &Dragged) -> Option<(ItemId, Option<i64>)> {
        let base = dragged.name == dragged.base_name;
        let id = self.find_item(&dragged.name, base)?;
        let stage = if self.nodes[id].def.kind.evolves() {
            Some(dragged.index.unwrap_or(-1))
        } else {
            None
        };
        Some((id, stage))
    }

    // ---- maps ---------------------------------------------------------

    pub fn sub_check(&self, at: CheckRef) -> &SubCheck {
        &self.maps[at.map].checks[at.check].checks()[at.sub]
    }

    fn sub_check_mut(&mut self, at: CheckRef) -> &mut SubCheck {
        &mut self.maps[at.map].checks[at.check].checks_mut()[at.sub]
    }

    /// Left click on a simple check, or on a check in a block's list.
    pub fn toggle_check(&mut self, at: CheckRef) {
        let check = self.sub_check_mut(at);
        check.checked = !check.checked;
        if check.checked {
            check.focused = false;
        }
        let (checked, group) = (check.checked, check.group.clone());
        if let Some(group) = group {
            for (index, other) in self.maps[at.map].checks.iter_mut().enumerate() {
                for (sub, other) in other.checks_mut().iter_mut().enumerate() {
                    let same = index == at.check && sub == at.sub;
                    if !same && !other.hide && other.group.as_deref() == Some(group.as_str()) {
                        other.checked = checked;
                        if checked {
                            other.focused = false;
                        }
                    }
                }
            }
        }
        self.logic_stale = true;
    }

    /// Middle click: marks a check to come back to.
    pub fn toggle_focus(&mut self, at: CheckRef) {
        let check = self.sub_check_mut(at);
        if !check.checked {
            check.focused = !check.focused;
        }
    }

    /// Right click: forgets the item dropped on the check.
    pub fn clear_check_drop(&mut self, at: CheckRef) {
        self.sub_check_mut(at).dragged = None;
    }

    /// Right click on a block: checks all of it, or none once all are done.
    pub fn toggle_block(&mut self, map: usize, check: usize) {
        let done = self.maps[map].checks[check]
            .block_state()
            .is_some_and(|state| state.logic == BlockLogic::Done);
        for sub in self.maps[map].checks[check].checks_mut() {
            sub.checked = !done;
            if sub.checked {
                sub.focused = false;
            }
        }
        self.logic_stale = true;
    }

    /// Middle click on a block.
    pub fn focus_block(&mut self, map: usize, check: usize) {
        for sub in self.maps[map].checks[check].checks_mut() {
            sub.focused = !sub.focused && !sub.checked;
        }
    }

    /// Recomputes which checks are in logic after a change.
    pub fn refresh_logic(&mut self) {
        if !self.logic_stale {
            return;
        }
        self.logic_stale = false;
        let memo = RefCell::new(HashMap::new());
        let mut results = Vec::new();
        for map in &self.maps {
            for check in &map.checks {
                for sub in check.checks() {
                    let logic = sub
                        .expr
                        .eval(&mut |name, args| self.call(name, args, &memo, 0));
                    results.push(match (sub.checked, logic) {
                        (true, _) => Logic::Done,
                        (false, true) => Logic::Logic,
                        (false, false) => Logic::NotLogic,
                    });
                }
            }
        }
        let mut results = results.into_iter();
        for map in &mut self.maps {
            for check in &mut map.checks {
                for sub in check.checks_mut() {
                    sub.logic = results.next().unwrap_or(Logic::NotLogic);
                }
            }
        }
    }

    /// `(in logic, left)` over every map, for the counter under the map.
    pub fn check_counts(&self) -> (usize, usize) {
        let mut in_logic = 0;
        let mut left = 0;
        for check in self.maps.iter().flat_map(|map| &map.checks) {
            for sub in check.checks().iter().filter(|c| c.open()) {
                left += 1;
                if sub.logic == Logic::Logic {
                    in_logic += 1;
                }
            }
        }
        (in_logic, left)
    }

    fn call(
        &self,
        name: &str,
        args: &[String],
        memo: &RefCell<HashMap<String, bool>>,
        depth: usize,
    ) -> bool {
        let first = args.first().map(String::as_str).unwrap_or_default();
        match name {
            "have" => self.find_item(first, false).is_some_and(|id| {
                let node = &self.nodes[id];
                node.state.enable
                    && match args.get(1) {
                        None => true,
                        Some(condition) => Comparison::parse(condition)
                            .zip(node.value())
                            .is_some_and(|(comparison, value)| comparison.holds(value)),
                    }
            }),
            "haveAlternateValue" => self
                .find_item(first, false)
                .is_some_and(|id| self.nodes[id].state.hint_show),
            "isChecked" => self.find_item(first, false).is_some_and(|id| {
                self.nodes[id].def.kind == Kind::CheckItem && self.nodes[id].state.check
            }),
            "isVisible" => self
                .find_item(first, false)
                .is_some_and(|id| self.nodes[id].state.show),
            "rules" => self.rule_active(first),
            "haveCheck" => self.have_check(first, args.get(1).map(String::as_str)),
            "do" => {
                if let Some(&known) = memo.borrow().get(first) {
                    return known;
                }
                if depth > MAX_DEPTH {
                    return false;
                }
                let result = self.actions.get(first).is_some_and(|expr| {
                    expr.eval(&mut |name, args| self.call(name, args, memo, depth + 1))
                });
                memo.borrow_mut().insert(first.to_owned(), result);
                result
            }
            _ => false,
        }
    }

    fn have_check(&self, check_name: &str, block_name: Option<&str>) -> bool {
        self.maps
            .iter()
            .flat_map(|map| &map.checks)
            .filter(|check| match &check.kind {
                CheckKind::Block(_) => block_name == Some(check.name.as_str()),
                CheckKind::Simple(_) => check.name == check_name,
            })
            .flat_map(|check| check.checks())
            .find(|sub| sub.name == check_name)
            .is_some_and(|sub| sub.checked)
    }

    // ---- rules --------------------------------------------------------

    pub fn rule_active(&self, name: &str) -> bool {
        self.rule_lists
            .iter()
            .flat_map(|list| &list.rules)
            .find(|rule| rule.name == name)
            .is_some_and(Rule::is_active)
    }

    /// A click on a rule of an open rules list.
    pub fn click_rule(&mut self, list: usize, rule: usize) {
        self.toggle_rule((list, rule), false);
    }

    fn toggle_rule(&mut self, (list, index): (usize, usize), forced: bool) {
        let rule = &mut self.rule_lists[list].rules[index];
        if !(rule.clickable || forced) {
            return;
        }
        rule.checked = !rule.checked;
        self.set_hidden_checks((list, index));
        self.do_rule_actions((list, index), 0);
        self.logic_stale = true;
    }

    fn set_hidden_checks(&mut self, (list, index): (usize, usize)) {
        let rule = &self.rule_lists[list].rules[index];
        let hide = rule.checked;
        let specs = rule.hide_checks.clone();
        let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
        for spec in &specs {
            for map in &mut self.maps {
                for check in &mut map.checks {
                    match (&mut check.kind, spec.kind.as_str()) {
                        (CheckKind::Simple(sub), "SimpleCheck") => {
                            if spec.checks.iter().any(|name| same(name, &sub.name)) {
                                sub.hide = hide;
                            }
                        }
                        (CheckKind::Block(subs), "Block")
                            if spec.name.as_deref().is_some_and(|n| same(n, &check.name)) =>
                        {
                            for sub in subs {
                                if spec.checks.iter().any(|name| same(name, &sub.name)) {
                                    sub.hide = hide;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn do_rule_actions(&mut self, (list, index): (usize, usize), depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let rule = &self.rule_lists[list].rules[index];
        let (checked, actions) = (rule.checked, rule.actions.clone());
        for action in actions {
            match action {
                RuleAction::SetRule { rule, active } => {
                    let active = !checked && active;
                    let found = self.rule_lists[..self.searchable_lists]
                        .iter()
                        .enumerate()
                        .flat_map(|(l, list)| {
                            list.rules
                                .iter()
                                .enumerate()
                                .filter(|(_, r)| r.name == rule)
                                .map(move |(i, _)| (l, i))
                        })
                        .last();
                    if let Some(found) = found {
                        self.rule_lists[found.0].rules[found.1].checked = !active;
                        self.do_rule_actions(found, depth + 1);
                        self.set_hidden_checks(found);
                    }
                }
                RuleAction::SetLeftClick(action) => {
                    self.item_action(&action.item, checked, Some(Button::Left), action.counter)
                }
                RuleAction::SetRightClick(action) => {
                    self.item_action(&action.item, checked, Some(Button::Right), action.counter)
                }
                RuleAction::SetWheelClick(action) => {
                    self.item_action(&action.item, checked, Some(Button::Middle), action.counter)
                }
                RuleAction::ResetItem(action) => self.item_action(&action.item, checked, None, 0),
            }
        }
    }

    /// Resets the item, then, while the rule is on, clicks it `counter` times.
    fn item_action(&mut self, item: &str, rule_off: bool, button: Option<Button>, counter: u32) {
        let Some(id) = self.find_item(item, true) else {
            return;
        };
        self.reset_item(id);
        if let (false, Some(button)) = (rule_off, button) {
            for _ in 0..counter {
                match button {
                    Button::Right => self.right_click(id),
                    Button::Middle => self.wheel_click(id),
                    _ => self.left_click(id),
                }
            }
        }
        self.update(id);
    }
}

#[derive(Debug, Clone, Copy)]
enum Children {
    Hint,
    Active,
    Inactive,
}

impl Children {
    fn defs_mut(self, def: &mut ItemDef) -> &mut Vec<ItemDef> {
        match self {
            Children::Hint => &mut def.hint_items,
            Children::Active => &mut def.active_items,
            Children::Inactive => &mut def.inactive_items,
        }
    }

    fn ids(self, node: &Node) -> &[ItemId] {
        match self {
            Children::Hint => &node.hint_children,
            Children::Active => &node.active_children,
            Children::Inactive => &node.inactive_children,
        }
    }

    fn ids_mut(self, node: &mut Node) -> &mut Vec<ItemId> {
        match self {
            Children::Hint => &mut node.hint_children,
            Children::Active => &mut node.active_children,
            Children::Inactive => &mut node.inactive_children,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn dir(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../public/sources/linso")
            .join(name)
    }

    pub(crate) fn load(name: &str) -> Engine {
        let dir = dir(name);
        let template = Template::parse(&std::fs::read_to_string(dir.join("tracker.json")).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let maps = template
            .maps
            .iter()
            .flat_map(|section| &section.maps)
            .map(|map| {
                MapDef::parse(&std::fs::read_to_string(dir.join(&map.file)).unwrap()).unwrap()
            })
            .collect();
        Engine::new(template, maps)
    }

    fn id(engine: &Engine, name: &str) -> ItemId {
        engine
            .find_item(name, true)
            .unwrap_or_else(|| panic!("no item {name}"))
    }

    #[test]
    fn every_template_loads_with_its_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/sources/linso");
        let mut names: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names.len(), 12, "{names:?}");
        for name in names {
            let engine = load(&name);
            assert!(!engine.main.is_empty(), "{name}");
            for file in engine.template.files() {
                assert!(dir(&name).join(&file).is_file(), "{name}: {file} missing");
            }
            for map in &engine.maps {
                for file in [&map.datas.background, &map.datas.list.background] {
                    assert!(dir(&name).join(file).is_file(), "{name}: {file} missing");
                }
            }
            for node in &engine.nodes {
                if let Some(sheet) = &node.def.sheet {
                    assert!(
                        engine.template.sheets.contains_key(&sheet.sheet),
                        "{name}: {} uses an unknown sheet",
                        node.def.name
                    );
                }
            }
        }
    }

    #[test]
    fn evolution_items_walk_their_stages_then_turn_off() {
        let mut engine = load("oot64");
        let hookshot = id(&engine, "Hookshot");
        let node = |e: &Engine| e.item(hookshot).state.clone();
        assert!(!node(&engine).enable);
        engine.click(hookshot, Button::Left);
        assert!(node(&engine).enable);
        assert_eq!(node(&engine).next_index, -1);
        engine.click(hookshot, Button::Left);
        assert_eq!(node(&engine).name, "Longshot");
        engine.click(hookshot, Button::Left);
        assert!(!node(&engine).enable);
        assert_eq!(node(&engine).name, "Hookshot");
        // Right click from off goes straight to the last stage.
        engine.click(hookshot, Button::Right);
        assert_eq!(node(&engine).name, "Longshot");
    }

    #[test]
    fn active_and_inactive_children_follow_their_parent() {
        let mut engine = load("NewFeatures");
        let boomerang = id(&engine, "Boomerang");
        let bomb = engine.item(boomerang).active_children[0];
        let nut = engine.item(boomerang).inactive_children[0];
        assert!(!engine.item(bomb).state.show);
        assert!(engine.item(nut).state.show);
        engine.click(nut, Button::Left);
        assert!(engine.item(nut).state.enable);

        engine.click(boomerang, Button::Left);
        assert!(engine.item(bomb).state.show);
        assert!(!engine.item(nut).state.show);
        // A hidden child forgets its state.
        assert!(!engine.item(nut).state.enable);

        // The middle click shows the hint, and the hint child.
        let shield = engine.item(boomerang).hint_children[0];
        engine.click(boomerang, Button::Middle);
        assert!(engine.item(boomerang).state.hint_show);
        assert!(engine.item(shield).state.show);
        engine.click(boomerang, Button::Middle);
        assert!(!engine.item(shield).state.show);
    }

    #[test]
    fn alternate_counts_switch_their_maximum_with_the_hint() {
        let mut engine = load("oot64keysanity");
        // 8 keys, or 5 in Master Quest.
        let counter = id(&engine, "Fire Key");
        let max = engine.item(counter).def.max_value;
        for _ in 0..max {
            engine.click(counter, Button::Left);
        }
        assert_eq!(engine.item(counter).state.value, max);
        engine.click(counter, Button::Middle);
        let state = &engine.item(counter).state;
        assert!(state.hint_show);
        assert_eq!(state.value, state.used_max);
        assert!(state.used_max < max);
    }

    #[test]
    fn submenus_open_with_a_left_click_and_hold_their_items() {
        let mut engine = load("twilightprincess");
        let submenu = engine
            .main
            .iter()
            .copied()
            .find(|&i| engine.item(i).def.kind == Kind::SubMenuItem)
            .unwrap();
        assert!(!engine.item(submenu).submenu.is_empty());
        engine.click(submenu, Button::Left);
        assert_eq!(engine.open_submenu(), Some(submenu));
        let first = engine.item(submenu).submenu[0];
        let name = engine.item(first).def.name.clone();
        assert_eq!(engine.find_item(&name, true), Some(first));
    }

    #[test]
    fn map_checks_follow_items_and_rules() {
        let mut engine = load("twilightprincessmap");
        engine.refresh_logic();
        let (in_logic, left) = engine.check_counts();
        assert!(left > 100, "{left}");
        assert!(in_logic > 0 && in_logic < left, "{in_logic}/{left}");

        // Getting the Lantern can only open more checks.
        let lantern = id(&engine, "Lantern");
        engine.click(lantern, Button::Left);
        engine.refresh_logic();
        let (with_lantern, _) = engine.check_counts();
        assert!(with_lantern > in_logic, "{with_lantern} <= {in_logic}");

        // Turning BugSanity on reveals the bug checks it hides.
        let bug = (0..engine.rule_lists.len())
            .flat_map(|l| (0..engine.rule_lists[l].rules.len()).map(move |r| (l, r)))
            .find(|&(l, r)| engine.rule_lists[l].rules[r].name == "BugSanity")
            .unwrap();
        assert!(!engine.rule_active("BugSanity"));
        engine.click_rule(bug.0, bug.1);
        assert!(engine.rule_active("BugSanity"));
        engine.refresh_logic();
        let (_, left_with_bugs) = engine.check_counts();
        assert!(left_with_bugs > left, "{left_with_bugs} <= {left}");
    }

    #[test]
    fn checking_a_whole_block_and_dropping_items() {
        let mut engine = load("twilightprincessmap");
        let block = engine.maps[0]
            .checks
            .iter()
            .position(|c| matches!(c.kind, CheckKind::Block(_)))
            .unwrap();
        engine.toggle_block(0, block);
        engine.refresh_logic();
        let state = engine.maps[0].checks[block].block_state().unwrap();
        assert_eq!(state.logic, BlockLogic::Done);

        let lantern = id(&engine, "Lantern");
        let at = CheckRef {
            map: 0,
            check: block,
            sub: 0,
        };
        engine.drop_on(lantern, DropTarget::Check(at));
        let dropped = engine.sub_check(at).dragged.clone().unwrap();
        assert_eq!(engine.dropped_item(&dropped), Some((lantern, None)));
        engine.clear_check_drop(at);
        assert!(engine.sub_check(at).dragged.is_none());
    }

    #[test]
    fn reset_restores_the_start() {
        let mut engine = load("twilightprincessmap");
        engine.refresh_logic();
        let before = engine.check_counts();
        let lantern = id(&engine, "Lantern");
        engine.click(lantern, Button::Left);
        engine.reset();
        engine.refresh_logic();
        assert_eq!(engine.check_counts(), before);
        assert!(!engine.item(lantern).state.enable);
    }
}
