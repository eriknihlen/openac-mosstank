//! Monster rules: the spec a row matches (see [`expression`]), what the row
//! does to the monsters it matches, the resolver that picks a row, and the
//! two monster facts a spec reads from the game database.

pub mod expression;

use std::collections::HashMap;
use std::sync::Arc;

use mosstank_data::damage::MonsterDamageType;
use mosstank_dotnet::{eq_ignore_case, is_white_space};
use mosstank_formats::gameinfo::VtankGameInfoDatabase;

pub use expression::{MonsterExpression, MonsterExpressionContext, MonsterValue, MonsterValueKind};

/// What a rule does to the monsters it matches (C# `[Flags]` enum; any
/// combination of bits is kept).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MonsterActionFlags(pub i32);

#[allow(missing_docs)]
impl MonsterActionFlags {
    pub const NONE: Self = Self(0);
    pub const FESTER: Self = Self(1 << 0);
    pub const BROADSIDE: Self = Self(1 << 1);
    pub const GRAVITY_WELL: Self = Self(1 << 2);
    pub const IMPERIL: Self = Self(1 << 3);
    pub const YIELD: Self = Self(1 << 4);
    pub const VULNERABILITY: Self = Self(1 << 5);
    pub const ATTACK: Self = Self(1 << 6);
    pub const RING: Self = Self(1 << 7);
    pub const STREAK: Self = Self(1 << 8);
    pub const WEAKENING_CURSE: Self = Self(1 << 9);
    pub const FESTERING_CURSE: Self = Self(1 << 10);
    pub const CORRUPTION: Self = Self(1 << 11);
    pub const DESTRUCTIVE_CURSE: Self = Self(1 << 12);
    pub const CORROSION: Self = Self(1 << 13);

    /// Whether any bit of `flag` is set.
    #[must_use]
    pub const fn intersects(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }
}

impl std::ops::BitOr for MonsterActionFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for MonsterActionFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for MonsterActionFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

/// The four named off-hand modes below `LISTEDTYPES_END`; anything at or
/// above it is an object id.
pub mod secondary_equip {
    /// Automatic.
    pub const AUTO: i32 = 0;
    /// An automatic shield.
    pub const AUTO_SHIELD: i32 = 1;
    /// An automatic weapon.
    pub const AUTO_WEAPON: i32 = 2;
    /// Nothing in the off hand.
    pub const NONE: i32 = 3;
    /// The first value that is an object id.
    pub const LISTED_TYPES_END: i32 = 4;
}

/// A rule row's action columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterRuleActions {
    /// What the row does.
    pub flags: MonsterActionFlags,
    /// Its priority (unclamped as stored).
    pub priority: i32,
    /// The element to attack with.
    pub damage_type: MonsterDamageType,
    /// A second element vulnerability stacked on the chain, off unless the
    /// row asks for it (automatic would quietly debuff almost every monster).
    pub extra_vulnerability: MonsterDamageType,
    /// The main-hand weapon's object id, 0 for none.
    pub weapon_object_id: u32,
    /// The off-hand item's object id, 0 for none.
    pub offhand_object_id: u32,
    /// The weapon column as stored: 0 caster handling, -1 automatic, else
    /// an object id.
    pub weapon_to_use_raw: i32,
    /// The off-hand column as stored (a [`secondary_equip`] mode or an id).
    pub secondary_equip_raw: i32,
    /// The main-hand weapon's name.
    pub weapon_name: String,
    /// The off-hand item's name.
    pub offhand_name: String,
    /// The pet's element.
    pub pet_damage_type: MonsterDamageType,
}

impl Default for MonsterRuleActions {
    fn default() -> Self {
        Self {
            flags: MonsterActionFlags::ATTACK,
            priority: 0,
            damage_type: MonsterDamageType::Auto,
            extra_vulnerability: MonsterDamageType::None,
            weapon_object_id: 0,
            offhand_object_id: 0,
            weapon_to_use_raw: -1,
            secondary_equip_raw: 0,
            weapon_name: String::new(),
            offhand_name: String::new(),
            pet_damage_type: MonsterDamageType::PlayerAuto,
        }
    }
}

impl MonsterRuleActions {
    /// The columns a brand-new row starts with: priority one, attack and
    /// streak, automatic element, no extra vulnerability, automatic weapon,
    /// off-hand and pet element.
    #[must_use]
    pub fn fresh_row() -> Self {
        Self {
            priority: 1,
            flags: MonsterActionFlags::ATTACK | MonsterActionFlags::STREAK,
            damage_type: MonsterDamageType::Auto,
            extra_vulnerability: MonsterDamageType::None,
            weapon_to_use_raw: -1,
            secondary_equip_raw: secondary_equip::AUTO,
            pet_damage_type: MonsterDamageType::PlayerAuto,
            ..Self::default()
        }
    }

    /// Actions with only a priority set.
    #[must_use]
    pub fn with_priority(priority: i32) -> Self {
        Self {
            priority,
            ..Self::default()
        }
    }

    /// The priority held to -1..=4.
    #[must_use]
    pub fn bounded_priority(&self) -> i32 {
        self.priority.clamp(-1, 4)
    }

    /// Whether the row attacks (a primary attack or a ring).
    #[must_use]
    pub fn attacks(&self) -> bool {
        self.flags
            .intersects(MonsterActionFlags::ATTACK | MonsterActionFlags::RING)
    }

    /// Whether the row makes a primary attack.
    #[must_use]
    pub fn uses_primary_attack(&self) -> bool {
        self.flags.intersects(MonsterActionFlags::ATTACK)
    }

    /// Whether the row casts rings.
    #[must_use]
    pub fn uses_ring(&self) -> bool {
        self.flags.intersects(MonsterActionFlags::RING)
    }

    /// Whether the row finishes with a streak.
    #[must_use]
    pub fn uses_streak(&self) -> bool {
        self.flags.intersects(MonsterActionFlags::STREAK)
    }
}

/// The fallback row's name in the file.
pub const AUTHENTIC_DEFAULT_NAME: &str = "<DEFAULT>";

/// One monster rule: a spec and its actions.
#[derive(Debug, Clone)]
pub struct MonsterRule {
    expression: String,
    actions: MonsterRuleActions,
    is_ignored_spec: bool,
    compiled: Option<Arc<MonsterExpression>>,
}

fn normalize_expression(expression: &str) -> String {
    if is_white_space(expression) {
        "DEFAULT".to_owned()
    } else {
        expression.trim().to_owned()
    }
}

impl MonsterRule {
    /// A rule; a blank spec is the default row.
    ///
    /// # Errors
    /// The spec does not compile.
    pub fn new(expression: &str, actions: MonsterRuleActions) -> Result<Self, String> {
        let expression = normalize_expression(expression);
        let compiled = if is_default_name(&expression) {
            None
        } else {
            Some(Arc::new(MonsterExpression::compile(&expression)?))
        };
        Ok(Self {
            expression,
            actions,
            is_ignored_spec: false,
            compiled,
        })
    }

    /// A rule with only a priority.
    ///
    /// # Errors
    /// The spec does not compile.
    pub fn with_priority(expression: &str, priority: i32) -> Result<Self, String> {
        Self::new(expression, MonsterRuleActions::with_priority(priority))
    }

    /// A rule, or, when its spec does not parse, an ignored row (which never
    /// matches but is kept and written back) and the warning to log.
    #[must_use]
    pub fn compile(expression: &str, actions: MonsterRuleActions) -> (Self, Option<String>) {
        match Self::new(expression, actions.clone()) {
            Ok(rule) => (rule, None),
            Err(error) => (
                Self {
                    expression: normalize_expression(expression),
                    actions,
                    is_ignored_spec: true,
                    compiled: None,
                },
                Some(format!(
                    "Parse error in monster spec: \"{expression}\", ignoring entry ({error})."
                )),
            ),
        }
    }

    /// The same row with other action columns; the spec is shared.
    #[must_use]
    pub fn with_actions(&self, actions: MonsterRuleActions) -> Self {
        Self {
            actions,
            ..self.clone()
        }
    }

    /// The row a profile with no default row falls back to: the fresh row.
    #[must_use]
    pub fn authentic_default() -> Self {
        Self::fresh_default()
    }

    /// A fresh `DEFAULT` row.
    #[must_use]
    pub fn fresh_default() -> Self {
        Self {
            expression: "DEFAULT".to_owned(),
            actions: MonsterRuleActions::fresh_row(),
            is_ignored_spec: false,
            compiled: None,
        }
    }

    /// A fresh row with this spec.
    ///
    /// # Errors
    /// The spec does not compile.
    pub fn fresh(expression: &str) -> Result<Self, String> {
        Self::new(expression, MonsterRuleActions::fresh_row())
    }

    /// The spec.
    #[must_use]
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// The action columns.
    #[must_use]
    pub fn actions(&self) -> &MonsterRuleActions {
        &self.actions
    }

    /// The priority.
    #[must_use]
    pub fn priority(&self) -> i32 {
        self.actions.priority
    }

    /// Whether this is the default row.
    #[must_use]
    pub fn is_default(&self) -> bool {
        is_default_name(&self.expression)
    }

    /// Whether the spec reads something that changes during a fight.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.compiled
            .as_ref()
            .is_some_and(|compiled| compiled.is_dynamic())
    }

    /// Whether the spec did not parse (the row is kept, never matches).
    #[must_use]
    pub fn is_ignored_spec(&self) -> bool {
        self.is_ignored_spec
    }

    /// Whether the row matches the monster.
    ///
    /// # Errors
    /// The spec failed to evaluate (no match): the message.
    pub fn matches(&self, context: &MonsterExpressionContext<'_>) -> Result<bool, String> {
        match &self.compiled {
            None => Ok(self.is_default() && !self.is_ignored_spec),
            Some(compiled) => compiled.is_match(context),
        }
    }
}

/// Either spelling of the fallback row's name.
#[must_use]
pub fn is_default_name(expression: &str) -> bool {
    eq_ignore_case(expression, "DEFAULT") || eq_ignore_case(expression, AUTHENTIC_DEFAULT_NAME)
}

/// The rule a monster resolved to.
#[derive(Debug, Clone)]
pub struct ResolvedMonsterRule {
    /// The rule.
    pub rule: MonsterRule,
    /// Its place in the list, `None` for the built-in fallback.
    pub index: Option<usize>,
    /// The first evaluation error met on the way, if any.
    pub evaluation_error: Option<String>,
}

impl ResolvedMonsterRule {
    /// The rule's actions.
    #[must_use]
    pub fn actions(&self) -> &MonsterRuleActions {
        self.rule.actions()
    }

    /// The rule's priority.
    #[must_use]
    pub fn priority(&self) -> i32 {
        self.rule.priority()
    }
}

/// The first non-default row that matches, else the first default row,
/// else the fresh default; with the first evaluation error met.
#[must_use]
pub fn resolve(
    rules: &[MonsterRule],
    context: &MonsterExpressionContext<'_>,
) -> ResolvedMonsterRule {
    let mut fallback: Option<usize> = None;
    let mut first_error: Option<String> = None;
    for (index, rule) in rules.iter().enumerate() {
        if rule.is_default() {
            fallback.get_or_insert(index);
            continue;
        }
        match rule.matches(context) {
            Ok(true) => {
                return ResolvedMonsterRule {
                    rule: rule.clone(),
                    index: Some(index),
                    evaluation_error: first_error,
                };
            }
            Ok(false) => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match fallback {
        Some(index) => ResolvedMonsterRule {
            rule: rules[index].clone(),
            index: Some(index),
            evaluation_error: first_error,
        },
        None => ResolvedMonsterRule {
            rule: MonsterRule::authentic_default(),
            index: None,
            evaluation_error: first_error,
        },
    }
}

/// The two monster facts a spec reads from the game database rather than
/// from the live object: the species and the maximum health. A monster the
/// database does not list has neither, and rules on them do not match it.
#[derive(Debug, Clone, Default)]
pub struct MonsterFactTable {
    species_words: HashMap<i32, String>,
    database: VtankGameInfoDatabase,
}

impl MonsterFactTable {
    /// Facts from this database.
    #[must_use]
    pub fn new(database: VtankGameInfoDatabase) -> Self {
        Self {
            species_words: HashMap::new(),
            database,
        }
    }

    /// The database read.
    #[must_use]
    pub fn database(&self) -> &VtankGameInfoDatabase {
        &self.database
    }

    /// Reads a newer database from now on; the species words learned stay.
    pub fn replace(&mut self, database: VtankGameInfoDatabase) {
        self.database = database;
    }

    /// Records this client's word for a species it has met (the database
    /// stores only the number).
    pub fn learn(&mut self, species_id: i32, word: Option<&str>) {
        let Some(word) = word else {
            return;
        };
        if species_id <= 0 || is_white_space(word) {
            return;
        }
        self.species_words.insert(species_id, word.to_owned());
    }

    /// The word for the monster's species, empty when it is not listed or
    /// the word is not known.
    #[must_use]
    pub fn species(&self, monster_name: &str) -> String {
        let species = self.database.species_of(monster_name);
        if species <= 0 {
            return String::new();
        }
        self.species_words
            .get(&species)
            .cloned()
            .unwrap_or_default()
    }

    /// The monster's maximum health, -1 when it is not listed.
    #[must_use]
    pub fn maximum_health(&self, monster_name: &str) -> i32 {
        self.database.maximum_health_of(monster_name)
    }

    /// Whether the database has a row for this monster at all.
    #[must_use]
    pub fn is_listed(&self, monster_name: &str) -> bool {
        self.database.lists_monster(monster_name)
    }

    /// Whether the monster is listed as unaffectable by magic.
    #[must_use]
    pub fn is_immune_to_magic(&self, monster_name: &str) -> bool {
        self.database.is_immune_to_magic(monster_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(name: &str) -> MonsterExpressionContext<'_> {
        MonsterExpressionContext {
            name,
            type_id: 1234,
            species: "Drudge",
            maximum_health: 450,
            range: 8.0,
            has_shield: true,
            meta_state: "Hunting",
            setting: None,
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: ResolverChecksRowsInOrderAfterDefaultAndCarriesAllActions
    #[test]
    fn resolver_checks_rows_in_order_after_default_and_carries_all_actions() {
        let fallback = MonsterRule::with_priority("DEFAULT", 0).unwrap();
        let first = MonsterRule::new(
            "species==drudge",
            MonsterRuleActions {
                priority: 3,
                flags: MonsterActionFlags::IMPERIL | MonsterActionFlags::ATTACK,
                damage_type: MonsterDamageType::Fire,
                extra_vulnerability: MonsterDamageType::Fire,
                weapon_object_id: 0x7000_0001,
                offhand_object_id: 0x7000_0002,
                pet_damage_type: MonsterDamageType::Cold,
                ..MonsterRuleActions::default()
            },
        )
        .unwrap();
        let later = MonsterRule::with_priority("range>1", 4).unwrap();

        let resolved = resolve(&[fallback, first, later], &context("Drudge Lurker"));

        assert_eq!(resolved.index, Some(1));
        assert_eq!(resolved.priority(), 3);
        assert_eq!(
            resolved.actions().flags,
            MonsterActionFlags::IMPERIL | MonsterActionFlags::ATTACK
        );
        assert_eq!(resolved.actions().damage_type, MonsterDamageType::Fire);
        assert_eq!(resolved.actions().weapon_object_id, 0x7000_0001);
        assert_eq!(resolved.actions().offhand_object_id, 0x7000_0002);
        assert_eq!(resolved.actions().pet_damage_type, MonsterDamageType::Cold);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterExpressionTests.cs :: ResolverFallsBackToDefaultAndKeepsAuthenticUnclampedPriority
    #[test]
    fn resolver_falls_back_to_default_and_keeps_authentic_unclamped_priority() {
        let fallback = MonsterRule::with_priority("DEFAULT", 99).unwrap();
        let resolved = resolve(
            &[
                fallback,
                MonsterRule::with_priority("species==olthoi", 4).unwrap(),
            ],
            &context("Drudge Lurker"),
        );
        assert_eq!(resolved.index, Some(0));
        assert_eq!(resolved.priority(), 99);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MonsterFactTableTests.cs :: AProfileWithNoDefaultRowFallsBackToTheSeedRow
    #[test]
    fn a_profile_with_no_default_row_falls_back_to_the_seed_row() {
        let resolved = resolve(
            &[MonsterRule::with_priority("name==Olthoi Slasher", 4).unwrap()],
            &MonsterExpressionContext {
                name: "Drudge Prowler",
                type_id: 42,
                species: "",
                maximum_health: -1,
                range: 3.0,
                has_shield: false,
                meta_state: "Default",
                setting: None,
            },
        );
        assert_eq!(resolved.priority(), 1);
        assert!(resolved.actions().uses_primary_attack());
        assert!(resolved.actions().uses_streak());
        assert_eq!(resolved.index, None);
    }
}
