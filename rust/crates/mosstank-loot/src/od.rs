//! Weapon OD: the step of the loot profile's OD ladders a weapon reaches.
//!
//! A loot profile ranks weapons by ladders of rules named `... (OD +N)`
//! (`(M) Xbow (OD +10)`, `(M) Xbow (OD +9)`, ... `(M) Xbow (OD +1)`), each
//! rule's requirements the damage that step needs (and the class, skill
//! and mastery of the ladder). An item's OD is the highest N among the
//! enabled OD rules whose requirements all match it, as the loot engine
//! matches them ([`rules::rule_matches`]); with none matching it has no OD.
//!
//! The rules are kept once per profile ([`OdRules::new`]), the highest N
//! first, so an item's first match is its OD; a rule asking for an object
//! class is passed over for an item of another class before its other
//! requirements are read. Only what decides an OD is kept (the step and
//! the requirements, or the expression of a rule without any), so a rule's
//! action, priority or the rest of its name change nothing;
//! [`OdRules::fingerprint`] tells when that changed without building the
//! rules again.
//!
//! The engine is given the client's spell table, so a cantrip-gated step
//! matches as it does on an appraisal, and nothing about the character: an
//! OD judges any character's item, so a requirement on the character
//! (skill, level, free pack slots) reads it as having none.

use std::hash::{DefaultHasher, Hash, Hasher};

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult};
use ac_plugin_abi::items::{InventoryItem, ItemResult};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::Automation;
use mosstank_automation::spells::SpellCatalog;
use mosstank_formats::utl::LootRule;

use crate::requirements;
use crate::rules;

/// The OD step a rule's name gives: N in its first `(OD +N)`, N a decimal
/// number, with optional white space around the parts and an optional
/// `>=` before the `+` (`(OD +10 )`, `(OD >= +8)`); none when the name has
/// no such part.
#[must_use]
pub fn od_step(name: &str) -> Option<i32> {
    const OPEN: &str = "(OD";
    let mut rest = name;
    while let Some(at) = rest.find(OPEN) {
        rest = &rest[at + OPEN.len()..];
        if let Some(step) = step_after_open(rest) {
            return Some(step);
        }
    }
    None
}

/// The step in what follows `(OD`: ws* (`>=` ws*)? `+` ws* digits ws* `)`.
fn step_after_open(text: &str) -> Option<i32> {
    let text = text.trim_start();
    let text = text.strip_prefix(">=").map_or(text, str::trim_start);
    let text = text.strip_prefix('+')?.trim_start();
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || !text[digits..].trim_start().starts_with(')') {
        return None;
    }
    text[..digits].parse().ok()
}

/// What decides a rule's OD: its requirements, or its expression when it
/// has none (the action, priority, name and the rest are left default).
fn deciding(rule: &LootRule) -> LootRule {
    let expression = if rule.vtank_requirements.is_empty() {
        rule.expression()
    } else {
        "*"
    };
    let mut kept = LootRule::named("", expression);
    kept.vtank_requirements.clone_from(&rule.vtank_requirements);
    kept
}

/// One OD rule.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OdRule {
    /// Its step.
    od: i32,
    /// The object class it asks for, when it asks for one.
    class: Option<i32>,
    /// What decides it ([`deciding`]).
    rule: LootRule,
}

/// A loot profile's enabled OD rules, the highest step first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OdRules {
    rules: Vec<OdRule>,
}

impl OdRules {
    /// The OD rules of `rules`: each enabled rule whose name has a step,
    /// the highest step first (rules of the same step in the profile's
    /// order).
    #[must_use]
    pub fn new(rules: &[LootRule]) -> Self {
        let mut kept: Vec<OdRule> = rules
            .iter()
            .filter_map(|rule| {
                let od = od_step(&rule.name)?;
                if requirements::is_disabled(rule) {
                    return None;
                }
                let class = rule
                    .vtank_requirements
                    .iter()
                    .find_map(requirements::required_object_class);
                Some(OdRule {
                    od,
                    class,
                    rule: deciding(rule),
                })
            })
            .collect();
        kept.sort_by_key(|rule| std::cmp::Reverse(rule.od));
        Self { rules: kept }
    }

    /// A number that changes when what decides the OD rules of `rules`
    /// does (each enabled one's step and requirements, or expression
    /// without any; their order), and not when anything else does: read
    /// without building them.
    #[must_use]
    pub fn fingerprint(rules: &[LootRule]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for rule in rules {
            let Some(od) = od_step(&rule.name) else {
                continue;
            };
            if requirements::is_disabled(rule) {
                continue;
            }
            od.hash(&mut hasher);
            rule.vtank_requirements.hash(&mut hasher);
            if rule.vtank_requirements.is_empty() {
                rule.expression().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// No rule has a step.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// How many OD rules there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// The item's OD: the highest step whose rule matches it; none when no
    /// OD rule does. A spell requirement reads the item's spells' names
    /// through `spell` (the client's spell table); a requirement on the
    /// character reads it as having none ([`SpellTableOnly`]).
    #[must_use]
    pub fn od(
        &self,
        item: &InventoryItem,
        properties: &ItemProperties,
        spell: &mut dyn FnMut(u32) -> Option<SpellInfo>,
    ) -> Option<i32> {
        let class = i32::from(item.object_class.tag());
        let mut client = SpellTableOnly { spell };
        self.rules
            .iter()
            .find(|rule| {
                rule.class.is_none_or(|wanted| wanted == class)
                    && rules::rule_matches(&rule.rule, item, properties, Some(&mut client)).0
            })
            .map(|rule| rule.od)
    }
}

/// The client as an OD rule reads it: the spell table, and nothing about
/// the character or the world. A character requirement (skill, base skill,
/// level, free pack slots) reads a character with nothing, as a rule read
/// without a client does: a skill or level at least some amount does not
/// match, a level at most some amount does. Nothing is cast, used or said.
struct SpellTableOnly<'a> {
    spell: &'a mut dyn FnMut(u32) -> Option<SpellInfo>,
}

impl SpellCatalog for SpellTableOnly<'_> {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        (self.spell)(spell_id)
    }
}

impl Automation for SpellTableOnly<'_> {
    fn is_available(&mut self) -> bool {
        false
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo::default()
    }

    fn skills(&mut self) -> Vec<Skill> {
        Vec::new()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::default()
    }

    fn cast(&mut self, _spell_id: u32, _target: Option<u32>) -> bool {
        false
    }

    fn combat_mode(&mut self) -> CombatMode {
        CombatMode::default()
    }

    fn enter_combat_mode(&mut self, _mode: CombatMode) -> CombatResult {
        CombatResult::default()
    }

    fn items_available(&mut self) -> bool {
        false
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        Vec::new()
    }

    fn use_item(&mut self, _object_id: u32) -> ItemResult {
        ItemResult::default()
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::default()
    }

    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }

    fn post_system_message(&mut self, _text: &str) {}

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}
