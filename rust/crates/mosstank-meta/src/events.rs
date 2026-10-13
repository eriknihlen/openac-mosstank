//! The settings page's event handlers as a simplified editor over the meta
//! profile (`GameEventHandlers`). A handler line is one rule in the default
//! state whose condition is an event edge and whose action is a chat
//! command, a chat expression or an expression, so a handler written on the
//! settings page shows on the meta tab, runs through the one engine and is
//! saved with the profile.
//!
//! A line reads `event = action`; `expr:` in front makes the action an
//! expression, `chatexpr:` a chat expression. The reference tool's own
//! names are accepted beside this client's: its `Login` lands on the
//! completed login, its `Logout` is the logoff.

use mosstank_expr::ExpressionProgram;
use mosstank_formats::meta::{
    DEFAULT_STATE, MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile,
    MetaRule,
};
use openac_plugin_compat::text::{casing, ordinal};

const EXPRESSION_PREFIX: &str = "expr:";
const CHAT_EXPRESSION_PREFIX: &str = "chatexpr:";

const EVENT_NAMES: [(&str, MetaConditionKind); 8] = [
    ("LoginComplete", MetaConditionKind::LoginComplete),
    ("Logoff", MetaConditionKind::Logoff),
    ("CharacterDeath", MetaConditionKind::CharacterDeath),
    ("PortalTransition", MetaConditionKind::PortalTransition),
    ("ItemUseCompleted", MetaConditionKind::ItemUseCompleted),
    ("ContainerOpened", MetaConditionKind::ContainerOpened),
    ("ContainerClosed", MetaConditionKind::ContainerClosed),
    (
        "ConfirmationRequested",
        MetaConditionKind::ConfirmationRequested,
    ),
];

const ALIASES: [(&str, MetaConditionKind); 3] = [
    ("Login", MetaConditionKind::LoginComplete),
    ("Logout", MetaConditionKind::Logoff),
    ("Death", MetaConditionKind::CharacterDeath),
];

/// Whether a condition kind is one of the client's notifications. The
/// engine re-arms a rule on one of these after every pass; character death
/// keeps its once-per-state behaviour for the profiles that rely on it.
#[must_use]
pub fn is_event_edge(kind: MetaConditionKind) -> bool {
    matches!(
        kind,
        MetaConditionKind::LoginComplete
            | MetaConditionKind::Logoff
            | MetaConditionKind::PortalTransition
            | MetaConditionKind::ItemUseCompleted
            | MetaConditionKind::ContainerOpened
            | MetaConditionKind::ContainerClosed
            | MetaConditionKind::ConfirmationRequested
    )
}

/// Whether a rule is one the settings page shows as a handler line:
/// enabled, in the default state, conditioned on an event, and acting as
/// one of the three shapes a line can spell.
#[must_use]
pub fn is_handler(rule: &MetaRule) -> bool {
    let default_state = casing::is_white_space_only(&rule.state)
        || ordinal::equals_ignore_case(rule.state.trim(), DEFAULT_STATE);
    let event_condition = is_event_edge(rule.condition.kind)
        || rule.condition.kind == MetaConditionKind::CharacterDeath;
    let line_shaped_action = matches!(
        rule.action.kind,
        MetaActionKind::ChatCommand
            | MetaActionKind::ChatExpression
            | MetaActionKind::ExpressionAction
    );
    rule.enabled
        && default_state
        && event_condition
        && rule.condition.children.is_empty()
        && line_shaped_action
        && rule.action.children.is_empty()
}

/// The handler rules of a profile as lines, in profile order.
#[must_use]
pub fn lines(profile: &MetaProfile) -> Vec<String> {
    profile
        .rules
        .iter()
        .filter(|rule| is_handler(rule))
        .map(format)
        .collect()
}

/// Replaces the profile's handler rules with the ones the lines spell,
/// leaving every other rule where it is; one sentence per line that could
/// not be read (those lines are dropped).
pub fn apply(profile: &mut MetaProfile, lines: &[String]) -> Vec<String> {
    let mut rejected = Vec::new();
    let mut parsed = Vec::new();
    for line in lines {
        match try_parse(line) {
            Ok(rule) => parsed.push(rule),
            Err(error) => rejected.push(format!("Event handler '{line}' was not saved: {error}")),
        }
    }
    profile.rules.retain(|rule| !is_handler(rule));
    profile.rules.extend(parsed);
    rejected
}

/// A handler rule as its line.
#[must_use]
pub fn format(rule: &MetaRule) -> String {
    let name = EVENT_NAMES
        .iter()
        .find(|(_, kind)| *kind == rule.condition.kind)
        .map_or("", |(name, _)| name);
    let action = match rule.action.kind {
        MetaActionKind::ExpressionAction => format!("{EXPRESSION_PREFIX} {}", rule.action.text),
        MetaActionKind::ChatExpression => {
            format!("{CHAT_EXPRESSION_PREFIX} {}", rule.action.text)
        }
        _ => rule.action.text.clone(),
    };
    format!("{name} = {action}")
}

/// Reads a handler line.
///
/// # Errors
/// Why the line is not a handler.
pub fn try_parse(line: &str) -> Result<MetaRule, String> {
    let text = line.trim();
    let Some(separator) = text.find('=') else {
        return Err("a handler reads 'event = action'.".to_owned());
    };
    let event_name = text[..separator].trim();
    let mut action_text = text[separator + 1..].trim();
    let Some(kind) = try_event(event_name) else {
        let names: Vec<&str> = EVENT_NAMES.iter().map(|(name, _)| *name).collect();
        return Err(format!(
            "'{event_name}' is not an event; one of {}.",
            names.join(", ")
        ));
    };
    if action_text.is_empty() {
        return Err("the action is empty.".to_owned());
    }
    let mut action_kind = MetaActionKind::ChatCommand;
    if ordinal::starts_with_ignore_case(action_text, EXPRESSION_PREFIX) {
        action_kind = MetaActionKind::ExpressionAction;
        action_text = action_text[EXPRESSION_PREFIX.len()..].trim();
    } else if ordinal::starts_with_ignore_case(action_text, CHAT_EXPRESSION_PREFIX) {
        action_kind = MetaActionKind::ChatExpression;
        action_text = action_text[CHAT_EXPRESSION_PREFIX.len()..].trim();
    }
    if action_kind != MetaActionKind::ChatCommand {
        if let Err(failure) = ExpressionProgram::compile(action_text) {
            return Err(format!(
                "the expression does not compile: {}",
                failure.message()
            ));
        }
        if action_text.is_empty() {
            return Err("the expression is empty.".to_owned());
        }
    }
    Ok(MetaRule::new(
        DEFAULT_STATE,
        MetaCondition::of(kind),
        MetaAction {
            kind: action_kind,
            text: action_text.to_owned(),
            ..MetaAction::default()
        },
    ))
}

fn try_event(name: &str) -> Option<MetaConditionKind> {
    EVENT_NAMES
        .iter()
        .chain(ALIASES.iter())
        .find(|(candidate, _)| ordinal::equals_ignore_case(candidate, name))
        .map(|(_, kind)| *kind)
}
