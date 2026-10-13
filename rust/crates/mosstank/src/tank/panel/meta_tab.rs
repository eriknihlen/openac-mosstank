//! The Meta page and the meta rule editor window: the ordered rule grid,
//! the live state picker, and the drafts a rule is built from.

use ac_plugin_abi::ui::UiValue;
use mosstank_expr::ExpressionProgram;
use mosstank_formats::meta::{
    DEFAULT_STATE, MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaRule,
};
use openac_plugin_compat::number;

use super::{Arg, b, clamp_row, i, t, tl};
use crate::tank::TankCx;

/// The move-up arrow icon.
pub(crate) const MOVE_UP_ICON: u32 = 0x0600_28FC;
/// The move-down arrow icon.
pub(crate) const MOVE_DOWN_ICON: u32 = 0x0600_28FD;

/// The meta editor's drafts and selection (the C# panel's `_meta*` fields).
#[derive(Debug, Clone)]
pub(crate) struct MetaColumns {
    pub(crate) selected: i32,
    pub(crate) state_draft: String,
    pub(crate) condition_text_draft: String,
    pub(crate) action_text_draft: String,
    pub(crate) secondary_text_draft: String,
    pub(crate) condition_kind: MetaConditionKind,
    pub(crate) action_kind: MetaActionKind,
    pub(crate) number: i32,
    pub(crate) secondary_number: i32,
}

impl Default for MetaColumns {
    fn default() -> Self {
        Self {
            selected: 0,
            state_draft: DEFAULT_STATE.to_owned(),
            condition_text_draft: String::new(),
            action_text_draft: String::new(),
            secondary_text_draft: String::new(),
            condition_kind: MetaConditionKind::Always,
            action_kind: MetaActionKind::None,
            number: 0,
            secondary_number: 0,
        }
    }
}

/// `{value:0.###}`.
fn number_text(value: f64) -> String {
    number::format_f64(value, "0.###").unwrap_or_default()
}

/// One line for a meta condition (`DescribeMetaCondition`).
pub(crate) fn describe_meta_condition(condition: &MetaCondition) -> String {
    use MetaConditionKind as K;
    match condition.kind {
        K::All => compound("All", &condition.children),
        K::Any => compound("Any", &condition.children),
        K::Not => {
            if condition.children.len() == 1 {
                format!("Not {}", describe_meta_condition(&condition.children[0]))
            } else {
                compound("Not", &condition.children)
            }
        }
        K::Expression => format!("Expression: {}", condition.text),
        K::ChatMessage | K::ChatMessageCapture => {
            format!("{}: {}", condition.kind.name(), condition.text)
        }
        K::Always
        | K::Never
        | K::CharacterDeath
        | K::NavigationRouteEmpty
        | K::AnyVendorOpen
        | K::VendorClosed
        | K::NeedToBuff
        | K::PortalspaceEntered
        | K::PortalspaceExited
        | K::LoginComplete
        | K::Logoff
        | K::PortalTransition
        | K::ItemUseCompleted
        | K::ContainerOpened
        | K::ContainerClosed
        | K::ConfirmationRequested => condition.kind.name().to_owned(),
        _ => format!(
            "{} {}",
            condition.kind.name(),
            number_text(condition.number)
        ),
    }
}

fn compound(name: &str, children: &[MetaCondition]) -> String {
    if children.is_empty() {
        return name.to_owned();
    }
    let parts: Vec<String> = children.iter().map(describe_meta_condition).collect();
    format!("{name} ({})", parts.join("; "))
}

/// One line for a meta action (`DescribeMetaAction`).
pub(crate) fn describe_meta_action(action: &MetaAction) -> String {
    use MetaActionKind as K;
    match action.kind {
        K::All => {
            if action.children.is_empty() {
                "All".to_owned()
            } else {
                let parts: Vec<String> = action.children.iter().map(describe_meta_action).collect();
                format!("All ({})", parts.join("; "))
            }
        }
        K::SetMetaState
        | K::CallMetaState
        | K::ChatCommand
        | K::ExpressionAction
        | K::ChatExpression => format!("{}: {}", action.kind.name(), action.text),
        _ => action.kind.name().to_owned(),
    }
}

/// `{value,-width}`: padded on the right to `width` characters.
fn pad(value: &str, width: usize) -> String {
    let length = openac_plugin_compat::text::utf16::len(value);
    if length >= width {
        value.to_owned()
    } else {
        format!("{value}{}", " ".repeat(width - length))
    }
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    let rules = &cx.meta.profile().rules;
    let count = rules.len();
    match name {
        "MetaRows" => tl(rules
            .iter()
            .map(|rule| {
                format!(
                    "{}  {}  {}",
                    pad(&rule.state, 16),
                    pad(&describe_meta_condition(&rule.condition), 34),
                    describe_meta_action(&rule.action)
                )
            })
            .collect()),
        "MetaDeleteColumn" => tl(vec!["X".to_owned(); count]),
        "MetaMoveUpIcons" => Some(UiValue::UintList(vec![MOVE_UP_ICON; count])),
        "MetaMoveDownIcons" => Some(UiValue::UintList(vec![MOVE_DOWN_ICON; count])),
        "MetaStateColumn" => tl(rules.iter().map(|rule| rule.state.clone()).collect()),
        "MetaConditionColumn" => tl(rules
            .iter()
            .map(|rule| describe_meta_condition(&rule.condition))
            .collect()),
        "MetaActionColumn" => tl(rules
            .iter()
            .map(|rule| describe_meta_action(&rule.action))
            .collect()),
        "SelectedMetaRuleIndex" => i(clamp_row(cx.core.panel.meta.selected, count)),
        "MetaState" | "SelectedMetaCurrentState" => t(cx.meta.current_state()),
        "MetaStateText" => t(format!("State: {}", cx.meta.current_state())),
        "MetaStatus" => t(cx.meta.status()),
        "MetaCurrentStateNames" => tl(cx.meta.states()),
        "MetaEditorVisible" => b(cx.core.panel.meta_editor_visible),
        "MetaConditionNames" => tl(MetaConditionKind::ALL
            .iter()
            .map(|kind| kind.name().to_owned())
            .collect()),
        "MetaActionNames" => tl(MetaActionKind::ALL
            .iter()
            .map(|kind| kind.name().to_owned())
            .collect()),
        "SelectedMetaCondition" => t(cx.core.panel.meta.condition_kind.name()),
        "SelectedMetaAction" => t(cx.core.panel.meta.action_kind.name()),
        "MetaStateDraft" => t(cx.core.panel.meta.state_draft.clone()),
        "MetaConditionTextDraft" => t(cx.core.panel.meta.condition_text_draft.clone()),
        "MetaActionTextDraft" => t(cx.core.panel.meta.action_text_draft.clone()),
        "MetaSecondaryTextDraft" => t(cx.core.panel.meta.secondary_text_draft.clone()),
        "MetaNumberText" => t(cx.core.panel.meta.number.to_string()),
        "MetaNumberLabel" => t(format!("N: {}", cx.core.panel.meta.number)),
        "MetaSecondaryNumberText" => t(cx.core.panel.meta.secondary_number.to_string()),
        "MetaSecondaryNumberLabel" => t(format!("N2: {}", cx.core.panel.meta.secondary_number)),
        "MetaNotice" => t(cx.core.meta_notice.clone()),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "DeleteMetaRuleAt" => {
            cx.select_meta_rule(arg.int());
            cx.remove_meta_rule();
        }
        "MoveMetaRuleUpAt" => {
            cx.select_meta_rule(arg.int());
            cx.move_meta_rule(-1);
        }
        "MoveMetaRuleDownAt" => {
            cx.select_meta_rule(arg.int());
            cx.move_meta_rule(1);
        }
        "SelectMetaRule" => {
            cx.select_meta_rule(arg.int());
            cx.core.panel.meta_editor_visible = true;
        }
        "HideMetaEditor" => cx.core.panel.meta_editor_visible = false,
        "SetMetaCurrentState" => {
            let state = arg.text().to_owned();
            cx.with_engine(|engine, host, services| engine.transition(host, services, &state));
            cx.meta
                .current_state()
                .clone_into(&mut cx.core.settings.combat.meta_state);
        }
        "SelectMetaCondition" => {
            if let Some(kind) = MetaConditionKind::ALL
                .iter()
                .copied()
                .find(|kind| ignore_case(kind.name(), arg.text()))
            {
                cx.core.panel.meta.condition_kind = kind;
            }
        }
        "SelectMetaAction" => {
            if let Some(kind) = MetaActionKind::ALL
                .iter()
                .copied()
                .find(|kind| ignore_case(kind.name(), arg.text()))
            {
                cx.core.panel.meta.action_kind = kind;
            }
        }
        "SetMetaStateDraft" => arg.text().clone_into(&mut cx.core.panel.meta.state_draft),
        "SetMetaConditionTextDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.meta.condition_text_draft);
        }
        "SetMetaActionTextDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.meta.action_text_draft);
        }
        "SetMetaSecondaryTextDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.meta.secondary_text_draft);
        }
        "MetaNumberDown" => cx.core.panel.meta.number -= 1,
        "MetaNumberUp" => cx.core.panel.meta.number += 1,
        "MetaSecondaryNumberDown" => cx.core.panel.meta.secondary_number -= 1,
        "MetaSecondaryNumberUp" => cx.core.panel.meta.secondary_number += 1,
        "AddMetaRule" => cx.add_meta_rule(),
        "CreateMetaRule" => {
            cx.add_meta_rule();
            cx.core.panel.meta_editor_visible = true;
        }
        "ApplyMetaRule" => {
            cx.apply_meta_rule();
            cx.core.panel.meta_editor_visible = false;
        }
        "RemoveMetaRule" => cx.remove_meta_rule(),
        "MoveMetaRuleUp" => cx.move_meta_rule(-1),
        "MoveMetaRuleDown" => cx.move_meta_rule(1),
        _ => return false,
    }
    true
}

/// `Enum.TryParse(value, ignoreCase: true)` over member names.
fn ignore_case(name: &str, value: &str) -> bool {
    openac_plugin_compat::text::ordinal::equals_ignore_case(name, value.trim())
}

impl TankCx<'_> {
    fn selected_meta_index(&self) -> Option<usize> {
        let count = self.meta.profile().rules.len();
        usize::try_from(self.core.panel.meta.selected)
            .ok()
            .filter(|index| *index < count)
    }

    /// `RefreshMetaEditor`: the selection kept inside the list and the
    /// drafts loaded from the selected rule (the grid is read fresh).
    pub(crate) fn refresh_meta_editor(&mut self) {
        let count = self.meta.profile().rules.len();
        self.core.panel.meta.selected = clamp_row(self.core.panel.meta.selected, count);
        let Some(index) = self.selected_meta_index() else {
            return;
        };
        let rule = self.meta.profile().rules[index].clone();
        let drafts = &mut self.core.panel.meta;
        drafts.state_draft = rule.state;
        drafts.condition_kind = rule.condition.kind;
        drafts.condition_text_draft = rule.condition.text;
        drafts.action_kind = rule.action.kind;
        drafts.action_text_draft = rule.action.text;
        drafts.secondary_text_draft = rule.action.secondary_text;
        drafts.number = clamp_to_i32(rule.condition.number);
        drafts.secondary_number = clamp_to_i32(rule.condition.secondary_number);
    }

    /// `SelectMetaRuleCore`.
    fn select_meta_rule(&mut self, index: i32) {
        let count = self.meta.profile().rules.len();
        self.core.panel.meta.selected = clamp_row(index, count);
        self.refresh_meta_editor();
        self.core.meta_notice = if self.selected_meta_index().is_none() {
            "Add a rule or select one to edit.".to_owned()
        } else {
            "Editing the selected ordered Meta rule.".to_owned()
        };
    }

    /// `TryBuildMetaRule`.
    fn try_build_meta_rule(&self) -> Result<MetaRule, String> {
        let drafts = &self.core.panel.meta;
        let state = if openac_plugin_compat::text::casing::is_white_space_only(&drafts.state_draft)
        {
            DEFAULT_STATE.to_owned()
        } else {
            drafts.state_draft.trim().to_owned()
        };
        if drafts.condition_kind == MetaConditionKind::Expression {
            ExpressionProgram::compile(&drafts.condition_text_draft)
                .map_err(|error| error.message())?;
        }
        if matches!(
            drafts.action_kind,
            MetaActionKind::ExpressionAction | MetaActionKind::ChatExpression
        ) {
            ExpressionProgram::compile(&drafts.action_text_draft)
                .map_err(|error| error.message())?;
        }
        Ok(MetaRule {
            state,
            condition: MetaCondition {
                kind: drafts.condition_kind,
                text: drafts.condition_text_draft.clone(),
                number: f64::from(drafts.number),
                secondary_number: f64::from(drafts.secondary_number),
                ..MetaCondition::default()
            },
            action: MetaAction {
                kind: drafts.action_kind,
                text: drafts.action_text_draft.clone(),
                secondary_text: drafts.secondary_text_draft.clone(),
                number: f64::from(drafts.number),
                secondary_number: f64::from(drafts.secondary_number),
                ..MetaAction::default()
            },
            ..MetaRule::default()
        })
    }

    /// `AddMetaRuleCore`.
    fn add_meta_rule(&mut self) {
        let rule = match self.try_build_meta_rule() {
            Ok(rule) => rule,
            Err(error) => {
                self.core.meta_notice = error;
                return;
            }
        };
        let state = rule.state.clone();
        self.meta.add_rule(rule);
        self.core.panel.meta.selected =
            i32::try_from(self.meta.profile().rules.len() - 1).unwrap_or(0);
        self.refresh_meta_editor();
        if self.save_meta_profile() {
            self.core.meta_notice = format!("Added rule in {state}.");
        }
    }

    /// `ApplyMetaRuleCore`.
    fn apply_meta_rule(&mut self) {
        let Some(index) = self.selected_meta_index() else {
            self.add_meta_rule();
            return;
        };
        let replacement = match self.try_build_meta_rule() {
            Ok(rule) => rule,
            Err(error) => {
                self.core.meta_notice = error;
                return;
            }
        };
        let state = replacement.state.clone();
        self.meta.replace_rule(index, replacement);
        self.refresh_meta_editor();
        if self.save_meta_profile() {
            self.core.meta_notice = format!("Updated rule in {state}.");
        }
    }

    /// `RemoveMetaRuleCore`.
    fn remove_meta_rule(&mut self) {
        let Some(index) = self.selected_meta_index() else {
            return;
        };
        let state = self.meta.profile().rules[index].state.clone();
        self.meta.remove_rule(index);
        let last = i32::try_from(self.meta.profile().rules.len().saturating_sub(1)).unwrap_or(0);
        self.core.panel.meta.selected = self.core.panel.meta.selected.min(last.max(0));
        self.refresh_meta_editor();
        if self.save_meta_profile() {
            self.core.meta_notice = format!("Removed rule from {state}.");
        }
    }

    /// `MoveMetaRule`.
    fn move_meta_rule(&mut self, direction: i32) {
        let count = self.meta.profile().rules.len();
        let selected = self.core.panel.meta.selected;
        let destination = selected + direction.signum();
        let (Some(from), Some(to)) = (super::row(selected, count), super::row(destination, count))
        else {
            return;
        };
        let state = self.meta.profile().rules[from].state.clone();
        self.meta.move_rule(from, to);
        self.core.panel.meta.selected = destination;
        self.refresh_meta_editor();
        if self.save_meta_profile() {
            self.core.meta_notice = format!("Moved {state} rule.");
        }
    }

    /// `SaveMetaProfile`.
    fn save_meta_profile(&mut self) -> bool {
        let profile = self.meta.profile().clone();
        if self
            .core
            .meta_profiles
            .save_current(self.client.stores(), &profile)
        {
            return true;
        }
        if let Some(notice) = self.core.meta_profiles.save_notice() {
            notice.clone_into(&mut self.core.meta_notice);
        }
        false
    }
}

/// `checked((int)Math.Clamp(value, int.MinValue, int.MaxValue))`; NaN
/// reads as 0.
#[allow(clippy::cast_possible_truncation)]
fn clamp_to_i32(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}
