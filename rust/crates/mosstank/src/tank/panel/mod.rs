//! The windows' side of the shell (`MossTankPanel`'s bindings): every value
//! a window shows ([`Tank::ui_value`]) and every handler a control fires
//! ([`Tank::ui_act`]), by the binding names the markup uses.
//!
//! The C# panel's properties and actions become one value lookup and one
//! handler dispatch; what only the windows keep (the open tab, the drafts,
//! the selected rows, the cached grid columns) lives in [`PanelState`],
//! and the grids are rebuilt by the same `Refresh*` steps the C# runs.

mod advanced;
mod chrome;
mod items;
mod meta_tab;
mod monsters;
mod profiles_tab;
mod route;
mod tinker;
mod ub_pages;

use ac_plugin_abi::ui::{ActionValue, UiValue};
use mosstank_formats::route::{RouteInsertMode, RouteRecallKind};

use super::{Tank, TankCx};
use crate::client::TankClient;

pub(crate) use items::ItemAdmission;

/// The main window's pages (`TankTab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TankTab {
    #[default]
    Options,
    Profiles,
    Vitals,
    Monsters,
    Items,
    Consumables,
    Buffs,
    Route,
    Meta,
    Ub,
    UbSettings,
    Tinker,
}

/// What only the windows keep (the C# panel's private fields).
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PanelState {
    pub(crate) active_tab: TankTab,
    pub(crate) profile_notice: String,
    pub(crate) profile_name_draft: String,
    pub(crate) monster_expression_draft: String,
    pub(crate) monster_editor_notice: String,
    pub(crate) monster_list_selected_row: i32,
    pub(crate) items: items::ItemRows,
    pub(crate) selected_item_row: i32,
    pub(crate) selected_consumable_row: i32,
    /// Handedness per item name (0 Auto, 1 1-Handed, 2 2-Handed).
    pub(crate) item_handedness: Vec<(String, usize)>,
    pub(crate) selected_excluded_component_row: i32,
    pub(crate) buff_picker_visible: bool,
    pub(crate) buff_picker_for_blacklist: bool,
    pub(crate) buff_picker_search_text: String,
    pub(crate) selected_buff_picker_row: i32,
    pub(crate) selected_extra_buff_row: i32,
    pub(crate) selected_blacklisted_buff_row: i32,
    pub(crate) loot_editor_visible: bool,
    pub(crate) advanced: advanced::AdvancedState,
    pub(crate) loot_rule_rows: Vec<String>,
    pub(crate) selected_loot_rule: i32,
    pub(crate) loot_expression_draft: String,
    pub(crate) loot_profile_name_draft: String,
    pub(crate) selected_route_waypoint: i32,
    pub(crate) route_profile_name_draft: String,
    pub(crate) route_chat_draft: String,
    pub(crate) route_pause_seconds: i32,
    pub(crate) route_recall_kind: RouteRecallKind,
    pub(crate) route_insert_mode: RouteInsertMode,
    pub(crate) meta: meta_tab::MetaColumns,
    pub(crate) meta_profile_name_draft: String,
    pub(crate) meta_editor_visible: bool,
    pub(crate) vitals: chrome::VitalsLine,
    pub(crate) admission: ItemAdmission,
    /// The Items page's rows must be rebuilt at the end of the call.
    pub(crate) items_dirty: bool,
    pub(crate) ub: ub_pages::UbPageState,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            active_tab: TankTab::Options,
            profile_notice: "Select an inventory item, then add it to this profile.".to_owned(),
            profile_name_draft: String::new(),
            monster_expression_draft: String::new(),
            monster_editor_notice: "Add a monster name or expression, or select one in the world."
                .to_owned(),
            monster_list_selected_row: 0,
            items: items::ItemRows::default(),
            selected_item_row: 0,
            selected_consumable_row: 0,
            item_handedness: Vec::new(),
            selected_excluded_component_row: 0,
            buff_picker_visible: false,
            buff_picker_for_blacklist: false,
            buff_picker_search_text: String::new(),
            selected_buff_picker_row: 0,
            selected_extra_buff_row: 0,
            selected_blacklisted_buff_row: 0,
            loot_editor_visible: false,
            advanced: advanced::AdvancedState::default(),
            loot_rule_rows: Vec::new(),
            selected_loot_rule: 0,
            loot_expression_draft: "*".to_owned(),
            loot_profile_name_draft: String::new(),
            selected_route_waypoint: 0,
            route_profile_name_draft: String::new(),
            route_chat_draft: "/ls".to_owned(),
            route_pause_seconds: 5,
            route_recall_kind: RouteRecallKind::PrimaryPortalRecall,
            route_insert_mode: RouteInsertMode::AddToEnd,
            meta: meta_tab::MetaColumns::default(),
            meta_profile_name_draft: String::new(),
            meta_editor_visible: false,
            vitals: chrome::VitalsLine::default(),
            admission: ItemAdmission::default(),
            items_dirty: true,
            ub: ub_pages::UbPageState::default(),
        }
    }
}

/// `ClampRow`: a row inside a list of `count`, 0 for an empty one.
pub(crate) fn clamp_row(index: i32, count: usize) -> i32 {
    let count = i32::try_from(count).unwrap_or(i32::MAX);
    if count == 0 {
        0
    } else {
        index.clamp(0, count - 1)
    }
}

/// A list's row as an index, when it is one.
pub(crate) fn row(index: i32, count: usize) -> Option<usize> {
    usize::try_from(index).ok().filter(|row| *row < count)
}

/// The argument a handler fired with.
#[derive(Debug, Clone)]
pub(crate) enum Arg {
    None,
    Int(i32),
    Float(f32),
    Text(String),
}

impl Arg {
    fn from(value: ActionValue) -> Self {
        match value {
            ActionValue::None => Self::None,
            ActionValue::Int(v) => Self::Int(v),
            ActionValue::Float(v) => Self::Float(v),
            ActionValue::Text(v) => Self::Text(v),
        }
    }

    pub(crate) fn int(&self) -> i32 {
        match self {
            Self::Int(v) => *v,
            _ => 0,
        }
    }

    pub(crate) fn float(&self) -> f32 {
        match self {
            Self::Float(v) => *v,
            _ => 0.0,
        }
    }

    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Text(v) => v,
            _ => "",
        }
    }
}

/// `true` → a bound bool.
pub(crate) fn b(value: bool) -> Option<UiValue> {
    Some(UiValue::Bool(value))
}

/// A bound string.
pub(crate) fn t(value: impl Into<String>) -> Option<UiValue> {
    Some(UiValue::Text(value.into()))
}

/// A bound string list.
pub(crate) fn tl(value: Vec<String>) -> Option<UiValue> {
    Some(UiValue::TextList(value))
}

/// A bound int.
pub(crate) fn i(value: i32) -> Option<UiValue> {
    Some(UiValue::Int(value))
}

/// A bound float.
pub(crate) fn f(value: f32) -> Option<UiValue> {
    Some(UiValue::Float(value))
}

impl TankCx<'_> {
    /// One binding's value.
    pub(crate) fn ui_value(&mut self, name: &str) -> Option<UiValue> {
        chrome::value(self, name)
            .or_else(|| profiles_tab::value(self, name))
            .or_else(|| monsters::value(self, name))
            .or_else(|| items::value(self, name))
            .or_else(|| route::value(self, name))
            .or_else(|| meta_tab::value(self, name))
            .or_else(|| advanced::value(self, name))
            .or_else(|| ub_pages::value(self, name))
            .or_else(|| tinker::value(self, name))
    }

    /// One handler; false for a name no page answers.
    pub(crate) fn ui_act(&mut self, name: &str, arg: &Arg) -> bool {
        chrome::act(self, name, arg)
            || profiles_tab::act(self, name, arg)
            || monsters::act(self, name, arg)
            || items::act(self, name, arg)
            || route::act(self, name, arg)
            || meta_tab::act(self, name, arg)
            || advanced::act(self, name, arg)
            || ub_pages::act(self, name, arg)
            || tinker::act(self, name, arg)
    }

    /// `SelectTab`: a page change closes the editors that float over it.
    pub(crate) fn select_tab(&mut self, tab: TankTab) {
        let panel = &mut self.core.panel;
        panel.active_tab = tab;
        panel.loot_editor_visible = false;
        panel.advanced.visible = false;
        panel.buff_picker_visible = false;
        panel.meta_editor_visible = false;
        self.ub.page.hide_color_editor();
        self.ub.page.hide_list_editor();
    }

    /// The grids a settings change rebuilds (`ResetProfileConsumers`'s
    /// `Refresh*` calls).
    pub(crate) fn refresh_editors(&mut self) {
        monsters::ensure_default_monster_rule(self);
        self.refresh_item_editors();
        self.refresh_loot_editor(false);
        self.refresh_route_editor();
        self.refresh_advanced_options();
    }
}

impl Tank {
    /// A binding's value as the windows show it now (the C# panel's
    /// property of that name); `None` for a name no window uses.
    pub fn ui_value(&mut self, client: &mut dyn TankClient, name: &str) -> Option<UiValue> {
        let (mut cx, _) = self.cx(client);
        cx.ui_value(name)
    }

    /// A control's handler fired (the C# panel's action of that name);
    /// false for a name no window uses.
    pub fn ui_act(&mut self, client: &mut dyn TankClient, name: &str, value: ActionValue) -> bool {
        self.begin_call(client);
        let arg = Arg::from(value);
        let (mut cx, _) = self.cx(client);
        cx.refresh_live_ub_rows(true);
        let handled = cx.ui_act(name, &arg);
        cx.core.write_out(cx.client);
        self.end_call(client);
        handled
    }
}
