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
//! requirements are read. [`OdRules::fingerprint`] tells when a profile's
//! OD rules changed without building them again.

use std::hash::{DefaultHasher, Hash, Hasher};

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_formats::utl::LootRule;

use crate::requirements;
use crate::rules;

/// The OD step a rule's name gives: N in its first `(OD +N)`, N a decimal
/// number; none when the name has no such part.
#[must_use]
pub fn od_step(name: &str) -> Option<i32> {
    const OPEN: &str = "(OD +";
    let mut rest = name;
    while let Some(at) = rest.find(OPEN) {
        rest = &rest[at + OPEN.len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0
            && rest.as_bytes().get(digits) == Some(&b')')
            && let Ok(step) = rest[..digits].parse()
        {
            return Some(step);
        }
    }
    None
}

/// One OD rule.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OdRule {
    /// Its step.
    od: i32,
    /// The object class it asks for, when it asks for one.
    class: Option<i32>,
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
            .filter(|rule| !requirements::is_disabled(rule))
            .filter_map(|rule| {
                let od = od_step(&rule.name)?;
                let class = rule
                    .vtank_requirements
                    .iter()
                    .find_map(requirements::required_object_class);
                Some(OdRule {
                    od,
                    class,
                    rule: rule.clone(),
                })
            })
            .collect();
        kept.sort_by_key(|rule| std::cmp::Reverse(rule.od));
        Self { rules: kept }
    }

    /// A number that changes when the OD rules of `rules` (their names,
    /// requirements, order) do, and not when any other rule does: read
    /// without building them.
    #[must_use]
    pub fn fingerprint(rules: &[LootRule]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for rule in rules {
            if od_step(&rule.name).is_some() {
                rule.hash(&mut hasher);
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
    /// OD rule does. Requirements on the character read nothing (the item
    /// may be another character's).
    #[must_use]
    pub fn od(&self, item: &InventoryItem, properties: &ItemProperties) -> Option<i32> {
        let class = i32::from(item.object_class.tag());
        self.rules
            .iter()
            .find(|rule| {
                rule.class.is_none_or(|wanted| wanted == class)
                    && rules::rule_matches(&rule.rule, item, properties, None).0
            })
            .map(|rule| rule.od)
    }
}
