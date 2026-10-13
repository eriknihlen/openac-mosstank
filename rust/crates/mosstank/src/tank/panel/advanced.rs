//! The advanced options window: every VTank setting with a plain value,
//! narrowed by category and search, edited in place (`MossTankPanel`'s
//! advanced options and `MossTankPanel.AdvancedCustomBuffs`).

use ac_plugin_abi::ui::UiValue;
use mosstank_dotnet::text::ordinal;
use mosstank_expr::ExpressionValue;
use mosstank_settings::defaults::setting_enum_values;
use mosstank_settings::options::{
    self, ADVANCED_NAMES, CATEGORY_BITS, CATEGORY_NAMES_BY_BIT, UNUSED_SETTING, VtankOptionPage,
    VtankSettingValueType, WALK_TO_RARE_CORPSE,
};

use super::{Arg, b, i, t, tl};
use crate::tank::TankCx;
use crate::tank::options::{boolean, number, try_parse_option_value};

/// The window's own state.
#[derive(Debug, Clone)]
pub(crate) struct AdvancedState {
    pub(crate) visible: bool,
    selected: i32,
    search_text: String,
    selection_key: String,
    category_enabled: Vec<bool>,
    names: Vec<String>,
    value_column: Vec<String>,
    choices: Vec<String>,
    selected_name: String,
    selected_description: String,
    value_draft: String,
    notice: String,
    custom_buff_draft: String,
}

impl Default for AdvancedState {
    fn default() -> Self {
        Self {
            visible: false,
            selected: 0,
            search_text: String::new(),
            selection_key: String::new(),
            category_enabled: vec![true; CATEGORY_BITS.len()],
            names: Vec::new(),
            value_column: Vec::new(),
            choices: Vec::new(),
            selected_name: String::new(),
            selected_description: String::new(),
            value_draft: String::new(),
            notice: "All VTank settings are available here.".to_owned(),
            custom_buff_draft: String::new(),
        }
    }
}

/// The categories as the list names them (`AdvancedOptionCategoryNames`).
fn category_names() -> Vec<String> {
    CATEGORY_BITS
        .iter()
        .map(|bit| {
            CATEGORY_NAMES_BY_BIT
                .get(bit)
                .map_or_else(|| format!("0x{bit:X}"), |name| (*name).to_owned())
        })
        .collect()
}

/// `IsPlainAdvancedOptionValue`: a switch, a choice, a whole number or a
/// decimal.
fn is_plain(kind: VtankSettingValueType) -> bool {
    matches!(
        kind,
        VtankSettingValueType::Bool
            | VtankSettingValueType::Enum
            | VtankSettingValueType::Int
            | VtankSettingValueType::Double
    )
}

/// A setting's description, when it has one.
fn description(name: &str) -> Option<&'static str> {
    let text = options::description_of(name);
    (!text.is_empty()).then_some(text)
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    let state = &cx.core.panel.advanced;
    match name {
        "AdvancedOptionsVisible" => b(state.visible),
        "AdvancedOptionNames" => tl(state.names.clone()),
        "AdvancedOptionValueColumn" => tl(state.value_column.clone()),
        "AdvancedOptionCategoryNames" => tl(category_names()),
        "AdvancedOptionCategoryEnabled" => Some(UiValue::BoolList(state.category_enabled.clone())),
        "SelectedAdvancedOptionCategoryIndex" => i(-1),
        "SelectedAdvancedOptionIndex" => i(state.selected),
        "AdvancedOptionName" => t(state.selected_name.clone()),
        "AdvancedOptionValueDraft" => t(state.value_draft.clone()),
        "AdvancedOptionDescription" => t(state.selected_description.clone()),
        "AdvancedOptionNotice" => t(state.notice.clone()),
        "AdvancedOptionSearchText" => t(state.search_text.clone()),
        "AdvancedOptionResultCount" => t(format!("{} options", state.names.len())),
        "AdvancedOptionEditorVisible" => b(!state.selected_name.is_empty()),
        "AdvancedOptionBooleanVisible" => b(cx.advanced_kind_visible(VtankSettingValueType::Bool)),
        "AdvancedOptionChoiceVisible" => b(cx.advanced_kind_visible(VtankSettingValueType::Enum)),
        "AdvancedOptionNumberVisible" => b(cx.advanced_number_visible()),
        "AdvancedOptionBooleanValue" => {
            let name = state.selected_name.clone();
            b(cx.advanced_kind_visible(VtankSettingValueType::Bool)
                && cx.get_meta_option(&name).is_truthy())
        }
        "AdvancedOptionChoices" => tl(state.choices.clone()),
        "SelectedAdvancedOptionChoice" => i(cx.selected_advanced_option_choice()),
        "AdvancedOptionChoiceText" => {
            let name = state.selected_name.clone();
            t(if cx.advanced_kind_visible(VtankSettingValueType::Enum) {
                cx.display_advanced_option_value(&name)
            } else {
                String::new()
            })
        }
        "AdvancedCustomBuffName" => t(custom_buff_name(&state.selected_name)),
        "AdvancedCustomBuffVisible" => b(cx.advanced_custom_buff_visible()),
        "AdvancedCustomBuffDraft" => t(state.custom_buff_draft.clone()),
        _ => None,
    }
}

/// `AdvancedCustomBuffName`.
fn custom_buff_name(selected: &str) -> String {
    match selected {
        "BuffProfile_Prots" => "BuffProfile-Prots".to_owned(),
        "BuffProfile_Banes" => "BuffProfile-Banes".to_owned(),
        _ => String::new(),
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "ToggleAdvancedOptionCategoryAt" => {
            let Some(at) = super::row(arg.int(), cx.core.panel.advanced.category_enabled.len())
            else {
                return true;
            };
            let enabled = &mut cx.core.panel.advanced.category_enabled;
            enabled[at] = !enabled[at];
            cx.refresh_advanced_options();
            cx.load_advanced_option_draft();
        }
        "SetAdvancedOptionSearchText" => cx.set_advanced_option_search_text(arg.text()),
        "ClearAdvancedOptionSearch" => cx.set_advanced_option_search_text(""),
        "ApplyAdvancedOption" => cx.apply_advanced_option(),
        "ToggleAdvancedOptionBoolean" => {
            if cx.advanced_kind_visible(VtankSettingValueType::Bool) {
                let selected = cx.core.panel.advanced.selected;
                cx.click_advanced_option_value(selected);
            }
        }
        "SelectAdvancedOptionChoiceText" => {
            let index = cx
                .core
                .panel
                .advanced
                .choices
                .iter()
                .position(|value| value == arg.text())
                .map_or(-1, |index| i32::try_from(index).unwrap_or(-1));
            cx.select_advanced_option_choice(index);
        }
        "SelectAdvancedOptionChoice" => cx.select_advanced_option_choice(arg.int()),
        "ShowAdvancedOptions" => cx.show_advanced_options(),
        "HideAdvancedOptions" => cx.core.panel.advanced.visible = false,
        "SelectAdvancedOption" => cx.select_advanced_option(arg.int()),
        "ClickAdvancedOptionValue" => cx.click_advanced_option_value(arg.int()),
        "SetAdvancedOptionValueDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.advanced.value_draft);
        }
        "SubmitAdvancedOption" => {
            arg.text()
                .clone_into(&mut cx.core.panel.advanced.value_draft);
            cx.apply_advanced_option();
        }
        "SetAdvancedCustomBuffDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.advanced.custom_buff_draft);
        }
        "SubmitAdvancedCustomBuff" => {
            arg.text()
                .clone_into(&mut cx.core.panel.advanced.custom_buff_draft);
            cx.apply_advanced_custom_buff();
        }
        "ApplyAdvancedCustomBuff" => cx.apply_advanced_custom_buff(),
        _ => return false,
    }
    true
}

impl TankCx<'_> {
    /// The value an option shows in the list (`DisplayAdvancedOptionValue`):
    /// a choice by its label.
    pub(crate) fn display_advanced_option_value(&mut self, name: &str) -> String {
        let value = self.get_meta_option(name);
        if options::declared_type(name) == VtankSettingValueType::Enum {
            let current = value.as_int32("advanced").unwrap_or(0);
            if let Some(entry) = setting_enum_values(name)
                .into_iter()
                .find(|entry| entry.value == current)
            {
                return entry.label;
            }
        }
        value.to_display_string()
    }

    fn advanced_kind_visible(&self, kind: VtankSettingValueType) -> bool {
        let name = &self.core.panel.advanced.selected_name;
        !name.is_empty() && options::declared_type(name) == kind
    }

    fn advanced_number_visible(&self) -> bool {
        !self.core.panel.advanced.selected_name.is_empty()
            && !self.advanced_kind_visible(VtankSettingValueType::Bool)
            && !self.advanced_kind_visible(VtankSettingValueType::Enum)
    }

    fn advanced_custom_buff_visible(&self) -> bool {
        let name = self.core.panel.advanced.selected_name.clone();
        !custom_buff_name(&name).is_empty()
            && self
                .get_meta_option(&name)
                .as_int32("advanced")
                .unwrap_or(0)
                == 1
    }

    fn selected_advanced_option_choice(&self) -> i32 {
        if !self.advanced_kind_visible(VtankSettingValueType::Enum) {
            return -1;
        }
        let name = &self.core.panel.advanced.selected_name;
        let current = self.get_meta_option(name).as_int32("advanced").unwrap_or(0);
        setting_enum_values(name)
            .iter()
            .position(|entry| entry.value == current)
            .map_or(-1, |index| i32::try_from(index).unwrap_or(-1))
    }

    /// `FilteredAdvancedOptionNames`.
    fn filtered_advanced_option_names(&self) -> Vec<String> {
        let state = &self.core.panel.advanced;
        let mut enabled_mask = 0;
        for (index, bit) in CATEGORY_BITS.iter().enumerate() {
            if state.category_enabled[index] {
                enabled_mask |= bit;
            }
        }
        let words: Vec<&str> = state.search_text.split_whitespace().collect();
        ADVANCED_NAMES
            .iter()
            .filter(|name| {
                **name != UNUSED_SETTING
                    && is_plain(options::declared_type(name))
                    && words.iter().all(|word| {
                        ordinal::contains_ignore_case(name, word)
                            || description(name)
                                .is_some_and(|text| ordinal::contains_ignore_case(text, word))
                    })
                    && if **name == WALK_TO_RARE_CORPSE {
                        enabled_mask & VtankOptionPage::LOOTING.0 != 0
                    } else {
                        options::page_of(name)
                            .is_none_or(|mask| mask.0 == 0 || mask.0 & enabled_mask != 0)
                    }
            })
            .map(|name| (*name).to_owned())
            .collect()
    }

    /// `RefreshAdvancedOptions`.
    pub(crate) fn refresh_advanced_options(&mut self) {
        let names = self.filtered_advanced_option_names();
        let state = &mut self.core.panel.advanced;
        if state.selection_key.is_empty()
            && let Some(first) = names.first()
        {
            state.selection_key = first.clone();
        }
        state.selected = names
            .iter()
            .position(|name| *name == state.selection_key)
            .map_or(-1, |index| i32::try_from(index).unwrap_or(-1));
        state.selected_name = usize::try_from(state.selected)
            .ok()
            .and_then(|index| names.get(index))
            .cloned()
            .unwrap_or_default();
        let values: Vec<String> = names
            .iter()
            .map(|name| self.display_advanced_option_value(name))
            .collect();
        let selected_name = self.core.panel.advanced.selected_name.clone();
        let choices = if self.advanced_kind_visible(VtankSettingValueType::Enum) {
            setting_enum_values(&selected_name)
                .into_iter()
                .map(|entry| entry.label)
                .collect()
        } else {
            Vec::new()
        };
        let state = &mut self.core.panel.advanced;
        state.names = names;
        state.value_column = values;
        state.choices = choices;
        state.selected_description = match description(&selected_name) {
            Some(text) => format!("{selected_name}: {text}"),
            None => selected_name,
        };
    }

    /// `LoadAdvancedOptionDraft`.
    pub(crate) fn load_advanced_option_draft(&mut self) {
        let custom = if self.advanced_custom_buff_visible() {
            let name = custom_buff_name(&self.core.panel.advanced.selected_name);
            self.get_meta_option(&name).to_display_string()
        } else {
            String::new()
        };
        self.core.panel.advanced.custom_buff_draft = custom;
        let name = self.core.panel.advanced.selected_name.clone();
        if name.is_empty() {
            let state = &mut self.core.panel.advanced;
            state.value_draft.clear();
            "Select an option to edit.".clone_into(&mut state.notice);
            return;
        }
        let draft = self.get_meta_option(&name).to_display_string();
        let state = &mut self.core.panel.advanced;
        state.value_draft = draft;
        state.notice = format!("Editing {name}.");
    }

    /// `ShowAdvancedOptions`.
    fn show_advanced_options(&mut self) {
        self.core.panel.advanced.visible = true;
        self.refresh_advanced_options();
        self.load_advanced_option_draft();
    }

    /// `ToggleAdvancedOptionsVisible`.
    pub(crate) fn toggle_advanced_options_visible(&mut self) {
        if self.core.panel.advanced.visible {
            self.core.panel.advanced.visible = false;
        } else {
            self.show_advanced_options();
        }
    }

    fn set_advanced_option_search_text(&mut self, value: &str) {
        value.clone_into(&mut self.core.panel.advanced.search_text);
        self.refresh_advanced_options();
        self.load_advanced_option_draft();
    }

    /// `SelectAdvancedOption`.
    fn select_advanced_option(&mut self, index: i32) {
        let Some(at) = super::row(index, self.core.panel.advanced.names.len()) else {
            return;
        };
        self.core.panel.advanced.selection_key = self.core.panel.advanced.names[at].clone();
        self.refresh_advanced_options();
        self.load_advanced_option_draft();
    }

    /// `ClickAdvancedOptionValue`: a switch flips, a choice steps, anything
    /// else is selected for typing.
    fn click_advanced_option_value(&mut self, index: i32) {
        let Some(at) = super::row(index, self.core.panel.advanced.names.len()) else {
            return;
        };
        let name = self.core.panel.advanced.names[at].clone();
        match options::declared_type(&name) {
            VtankSettingValueType::Bool => self.flip_advanced_option_bool(&name),
            VtankSettingValueType::Enum => self.cycle_advanced_option_enum(index, &name),
            _ => self.select_advanced_option(index),
        }
    }

    fn flip_advanced_option_bool(&mut self, name: &str) {
        name.clone_into(&mut self.core.panel.advanced.selection_key);
        let next = !self.get_meta_option(name).is_truthy();
        let written = self.set_meta_option(name, &boolean(next)).unwrap_or(false);
        self.core.panel.advanced.notice = if written {
            format!("Set {name} = {}.", boolean(next).to_display_string())
        } else {
            format!("{name} is unavailable.")
        };
        self.refresh_advanced_options();
        self.load_advanced_option_draft();
    }

    fn cycle_advanced_option_enum(&mut self, index: i32, name: &str) {
        name.clone_into(&mut self.core.panel.advanced.selection_key);
        let entries = setting_enum_values(name);
        if entries.is_empty() {
            self.select_advanced_option(index);
            return;
        }
        let current = self.get_meta_option(name).as_int32("advanced").unwrap_or(0);
        let position = entries.iter().position(|entry| entry.value == current);
        let next = &entries[position.map_or(0, |at| (at + 1) % entries.len())];
        let written = self
            .set_meta_option(name, &number(next.value))
            .unwrap_or(false);
        self.core.panel.advanced.notice = if written {
            format!("Set {name} = {}.", next.label)
        } else {
            format!("{name} is unavailable.")
        };
        self.refresh_advanced_options();
        self.load_advanced_option_draft();
    }

    fn select_advanced_option_choice(&mut self, index: i32) {
        if !self.advanced_kind_visible(VtankSettingValueType::Enum) {
            return;
        }
        let name = self.core.panel.advanced.selected_name.clone();
        let entries = setting_enum_values(&name);
        let Some(at) = super::row(index, entries.len()) else {
            return;
        };
        self.core.panel.advanced.value_draft = entries[at].value.to_string();
        self.apply_advanced_option();
    }

    /// `ApplyAdvancedOptionCore`.
    fn apply_advanced_option(&mut self) {
        let name = self.core.panel.advanced.selected_name.clone();
        if name.is_empty() {
            return;
        }
        let draft = self.core.panel.advanced.value_draft.trim().to_owned();
        let Some(value) = try_parse_option_value(&draft, None) else {
            "Enter a value first.".clone_into(&mut self.core.panel.advanced.notice);
            return;
        };
        match self.set_meta_option(&name, &value) {
            Ok(false) => self.core.panel.advanced.notice = format!("{name} is unavailable."),
            Ok(true) => {
                self.refresh_advanced_options();
                self.load_advanced_option_draft();
                self.core.panel.advanced.notice = format!("Applied {name}.");
            }
            Err(fault) => self.core.panel.advanced.notice = fault.message(),
        }
    }

    /// `ApplyAdvancedCustomBuff`: only the element letters, or nothing.
    fn apply_advanced_custom_buff(&mut self) {
        if !self.advanced_custom_buff_visible() {
            return;
        }
        let value = self.core.panel.advanced.custom_buff_draft.trim().to_owned();
        if value
            .chars()
            .any(|letter| !"ALFCBPS".contains(letter.to_ascii_uppercase()))
        {
            "Use only A, L, F, C, B, P, S (or leave empty for none)."
                .clone_into(&mut self.core.panel.advanced.notice);
            return;
        }
        let name = custom_buff_name(&self.core.panel.advanced.selected_name);
        if !self
            .set_meta_option(&name, &ExpressionValue::string(&value))
            .unwrap_or(false)
        {
            self.core.panel.advanced.notice = format!("{name} is unavailable.");
            return;
        }
        self.refresh_advanced_options();
        self.load_advanced_option_draft();
        self.core.panel.advanced.notice = format!("Applied {name}.");
    }
}
