//! The rule list's verdict on one item (`LootRule.IsMatch`, `EarlyMatch`,
//! `LootRuleEngine`): the first rule that matches decides, a keep-up-to rule
//! stands down at its limit, and an item is appraised only when the answer
//! depends on it.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::Automation;
use mosstank_dotnet::collections::IgnoreCaseStringMap;
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::utl::{LootAction, LootRule};

use crate::expression::{LootRuleExpression, is_catch_all};
use crate::requirements;

/// What the rule list chose for an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LootDecision {
    /// What to do with it.
    pub action: LootAction,
    /// Its priority among the corpse's items.
    pub priority: i32,
    /// The rule's place in the list (`i32::MAX` for the looter's own
    /// choices, -1 for an external classifier's).
    pub rule_index: i32,
    /// The rule's name.
    pub rule_name: String,
    /// The external classifier that chose it; empty for MossTank's rules.
    pub classifier_id: String,
}

impl LootDecision {
    /// A decision of MossTank's own.
    #[must_use]
    pub fn new(action: LootAction, priority: i32, rule_index: i32, rule_name: &str) -> Self {
        Self {
            action,
            priority,
            rule_index,
            rule_name: rule_name.to_owned(),
            classifier_id: String::new(),
        }
    }
}

/// Why the rule list chose nothing for an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootRefusalKind {
    /// No rule in the list matched the item at all.
    NoRule,
    /// A rule matched, and it says the item stays where it is.
    RuleDeclined,
    /// A rule matched, and the character already holds its limit.
    RuleAtCap,
}

/// The rule list's answer when it chose nothing. All three reasons look the
/// same from outside -- the item is left behind -- so each says which it
/// is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LootRefusal {
    /// Which reason.
    pub kind: LootRefusalKind,
    /// The rule that matched.
    pub rule_name: String,
    /// A keep-up-to rule's limit.
    pub limit: i32,
    /// How many the character holds.
    pub held: i32,
}

impl LootRefusal {
    /// No rule matched.
    #[must_use]
    pub fn no_rule() -> Self {
        Self {
            kind: LootRefusalKind::NoRule,
            rule_name: String::new(),
            limit: 0,
            held: 0,
        }
    }

    fn declined(rule_name: &str) -> Self {
        Self {
            kind: LootRefusalKind::RuleDeclined,
            rule_name: rule_name.to_owned(),
            ..Self::no_rule()
        }
    }

    fn at_cap(rule_name: &str, limit: i32, held: i32) -> Self {
        Self {
            kind: LootRefusalKind::RuleAtCap,
            rule_name: rule_name.to_owned(),
            limit,
            held,
        }
    }

    /// The reason in words, for the log.
    #[must_use]
    pub fn describe(&self) -> String {
        match self.kind {
            LootRefusalKind::RuleDeclined => format!("{} says leave it", self.rule_name),
            LootRefusalKind::RuleAtCap => format!(
                "{} keeps up to {} (holding {})",
                self.rule_name, self.limit, self.held
            ),
            LootRefusalKind::NoRule => "no rule matched".to_owned(),
        }
    }
}

/// Whether the rule matches the item; with the reason when it could not be
/// read (and the answer is then no). Requirements, when the rule has any,
/// decide; otherwise its expression does.
#[must_use]
pub fn rule_matches(
    rule: &LootRule,
    item: &InventoryItem,
    properties: &ItemProperties,
    host: Option<&mut (dyn Automation + '_)>,
) -> (bool, Option<String>) {
    if !rule.vtank_requirements.is_empty() {
        return requirements::is_match(&rule.vtank_requirements, item, properties, host);
    }
    match LootRuleExpression::compile(rule.expression())
        .and_then(|compiled| compiled.is_match(item, properties))
    {
        Ok(matched) => (matched, None),
        Err(error) => (false, Some(error)),
    }
}

/// Whether the rule can decide the item without appraisal data, and when it
/// can, whether it matches: `(has_decision, is_match)`. A rule is only a
/// decided match when every requirement decided; one definite non-match
/// ends it, and one undecidable requirement leaves the whole rule open.
#[must_use]
pub fn rule_early_match(
    rule: &LootRule,
    item: &InventoryItem,
    properties: &ItemProperties,
    mut host: Option<&mut (dyn Automation + '_)>,
) -> (bool, bool) {
    if rule.vtank_requirements.is_empty() {
        // A catch-all needs nothing to decide. Anything written in MossTank's
        // own expression language stays open: that language has no
        // per-clause appraisal-dependency model.
        let decided = is_catch_all(rule.expression());
        return (decided, decided);
    }
    let mut any_open = false;
    for requirement in &rule.vtank_requirements {
        let (decided, matched) =
            requirements::early_match(requirement, item, properties, host.as_deref_mut());
        if decided && !matched {
            return (true, false);
        }
        if !decided {
            any_open = true;
        }
    }
    (!any_open, !any_open)
}

/// How many the character holds of an item by name (ignoring case), each
/// stack counting at least one, plus what is on its way.
pub(crate) fn held_count(
    name: &str,
    owned: &[InventoryItem],
    pending: Option<&IgnoreCaseStringMap<i32>>,
) -> i32 {
    let mut held = owned
        .iter()
        .filter(|candidate| ordinal::equals_ignore_case(&candidate.name, name))
        .fold(0i32, |sum, candidate| {
            sum.wrapping_add(candidate.stack_size.max(1))
        });
    if let Some(pending) = pending
        && let Some(count) = pending.get(name)
    {
        held = held.wrapping_add(*count);
    }
    held
}

/// The rule list's decision: the first rule that matches, unless it says
/// to leave the item or holds its keep-up-to limit already; with the reason
/// when there is none.
#[must_use]
pub fn decide(
    item: &InventoryItem,
    properties: &ItemProperties,
    rules: &[LootRule],
    owned: &[InventoryItem],
    pending_by_name: Option<&IgnoreCaseStringMap<i32>>,
    mut host: Option<&mut (dyn Automation + '_)>,
) -> (Option<LootDecision>, LootRefusal) {
    for (index, rule) in rules.iter().enumerate() {
        if !rule_matches(rule, item, properties, host.as_deref_mut()).0 {
            continue;
        }
        let rule_name = if casing::is_white_space_only(&rule.name) {
            format!("Rule {}", index + 1)
        } else {
            rule.name.clone()
        };
        if rule.action == LootAction::NoLoot {
            return (None, LootRefusal::declined(&rule_name));
        }
        if rule.action == LootAction::KeepUpTo {
            let limit = rule.keep_count.max(0);
            let held = held_count(&item.name, owned, pending_by_name);
            if held >= limit {
                return (None, LootRefusal::at_cap(&rule_name, limit, held));
            }
        }
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        return (
            Some(LootDecision::new(
                rule.action,
                rule.priority,
                index,
                &rule_name,
            )),
            LootRefusal::no_rule(),
        );
    }
    (None, LootRefusal::no_rule())
}

/// One forward pass that answers "would appraising this item change which
/// rule wins?". A rule that can already decide short-circuits; so does a
/// later rule carrying the same action as an earlier still-open one,
/// because the outcome is the same either way. Only a later rule with a
/// different action than a still-open one forces the appraisal.
#[must_use]
pub fn needs_identify(
    item: &InventoryItem,
    properties: &ItemProperties,
    rules: &[LootRule],
    mut host: Option<&mut (dyn Automation + '_)>,
) -> bool {
    let mut open = false;
    let mut open_action = LootAction::NoLoot;
    for rule in rules {
        if open && rule.action != open_action {
            return true;
        }
        let (decided, matched) = rule_early_match(rule, item, properties, host.as_deref_mut());
        if decided && matched {
            return false;
        }
        if decided {
            continue;
        }
        open = true;
        open_action = rule.action;
    }
    open
}
