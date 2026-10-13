//! The attack spells a monster rule's element and kind of attack name: the
//! spell line (family name) for each, and the best member of that line the
//! character knows.

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::ProjectilePathKind;
use mosstank_dotnet::collections::LookupMap;
use mosstank_dotnet::text::{casing, ordinal};

use crate::damage::MonsterDamageType;

/// The kinds of combat spell a rule asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VtankCombatSpellType {
    /// A vulnerability.
    Vuln,
    /// A bolt.
    War,
    /// An arc.
    Arc,
    /// A ring.
    Ring,
    /// A streak.
    Streak,
}

impl VtankCombatSpellType {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 5] = [Self::Vuln, Self::War, Self::Arc, Self::Ring, Self::Streak];
}

/// A spell picked for an attack.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackSpellChoice {
    /// The spell.
    pub spell: SpellInfo,
    /// What kind of attack it is.
    pub kind: VtankCombatSpellType,
    /// Its element.
    pub damage_type: MonsterDamageType,
    /// Whether it is cast at nothing.
    pub cast_without_target: bool,
}

/// The Void ring: the only entry of the reference's attack-spell table
/// named by spell id rather than by name.
pub const VOID_RING_SPELL_ID: u32 = 5361;
/// Tusker Fists.
pub const TUSKER_FISTS_SPELL_ID: u32 = 0x0B76;
const WAR_MAGIC_SKILL: u32 = 34;
const LIFE_MAGIC_SKILL: u32 = 33;
const VOID_MAGIC_SKILL: u32 = 43;

/// The skill with this id in the character's list.
#[must_use]
pub fn find_skill(skills: &[Skill], skill_id: u32) -> Option<&Skill> {
    skills.iter().find(|skill| skill.skill_id == skill_id)
}

/// The spells the character knows, by line.
#[derive(Debug, Clone, Default)]
pub struct AttackSpellCatalog {
    /// Every spell handed in, in order (the lists below hold places in it).
    spells: Vec<SpellInfo>,
    /// Lines by upper-cased name (any case matches), members in input order.
    families: LookupMap<String, Vec<usize>>,
    /// Every spell, in the order its id was first seen; a later spell with
    /// the same id replaces it where it stands.
    by_id: Vec<usize>,
    id_slots: LookupMap<u32, usize>,
}

impl AttackSpellCatalog {
    /// The catalog of `spells`.
    #[must_use]
    pub fn build(spells: &[SpellInfo]) -> Self {
        Self::build_owned(spells.to_vec())
    }

    /// The catalog of `spells`, kept as handed in.
    #[must_use]
    pub fn build_owned(spells: Vec<SpellInfo>) -> Self {
        let mut catalog = Self::default();
        for (index, spell) in spells.iter().enumerate() {
            match catalog.id_slots.get(&spell.spell_id) {
                Some(&slot) => catalog.by_id[slot] = index,
                None => {
                    catalog.id_slots.insert(spell.spell_id, catalog.by_id.len());
                    catalog.by_id.push(index);
                }
            }
            let family = base_name_span(&spell.name);
            if family.is_empty() {
                continue;
            }
            catalog
                .families
                .entry(casing::to_upper_invariant(family))
                .or_default()
                .push(index);
        }
        catalog.spells = spells;
        catalog
    }

    fn by_id(&self, spell_id: u32) -> Option<&SpellInfo> {
        self.id_slots
            .get(&spell_id)
            .map(|&slot| &self.spells[self.by_id[slot]])
    }

    fn members(&self, family: &str) -> Option<impl Iterator<Item = &SpellInfo>> {
        self.families
            .get(&casing::to_upper_invariant(family))
            .map(|members| members.iter().map(|&index| &self.spells[index]))
    }

    /// The best usable member of the line for this element and kind.
    #[must_use]
    pub fn resolve(
        &self,
        element: MonsterDamageType,
        kind: VtankCombatSpellType,
        usable: Option<&dyn Fn(&SpellInfo) -> bool>,
    ) -> Option<SpellInfo> {
        if kind == VtankCombatSpellType::Ring && is_void(element) {
            return self
                .by_id(VOID_RING_SPELL_ID)
                .and_then(|void_ring| self.resolve_family_of(void_ring, usable));
        }
        family_name(element, kind).and_then(|family| self.resolve_family(family, usable))
    }

    /// The first rung of the line rather than the best one known: the spell
    /// named "<line> I", else the lowest quality.
    #[must_use]
    pub fn resolve_base_tier(
        &self,
        element: MonsterDamageType,
        kind: VtankCombatSpellType,
    ) -> Option<SpellInfo> {
        if kind == VtankCombatSpellType::Ring && is_void(element) {
            return self.by_id(VOID_RING_SPELL_ID).cloned();
        }
        let family = family_name(element, kind)?;
        let members = self.members(family)?;
        let first_rung = format!("{family} I");
        let mut lowest: Option<&SpellInfo> = None;
        for spell in members {
            if ordinal::equals_ignore_case(&spell.name, &first_rung) {
                return Some(spell.clone());
            }
            if lowest.is_none_or(|current| spell.quality() < current.quality()) {
                lowest = Some(spell);
            }
        }
        lowest.cloned()
    }

    /// The best usable spell sharing the base spell's family, school,
    /// fellowship and targeting flags and components.
    #[must_use]
    pub fn resolve_family_of(
        &self,
        base: &SpellInfo,
        usable: Option<&dyn Fn(&SpellInfo) -> bool>,
    ) -> Option<SpellInfo> {
        let mut best: Option<&SpellInfo> = None;
        for spell in self.by_id.iter().map(|&index| &self.spells[index]) {
            if spell.family != base.family
                || spell.school != base.school
                || spell.is_fellowship != base.is_fellowship
                || spell.is_untargeted != base.is_untargeted
                || spell.component_set != base.component_set
                || usable.is_some_and(|usable| !usable(spell))
            {
                continue;
            }
            if best.is_none_or(|current| spell.quality() > current.quality()) {
                best = Some(spell);
            }
        }
        best.cloned()
    }

    /// The best usable member of a named line (the first of equal quality).
    #[must_use]
    pub fn resolve_family(
        &self,
        family_name: &str,
        usable: Option<&dyn Fn(&SpellInfo) -> bool>,
    ) -> Option<SpellInfo> {
        let members = self.members(family_name)?;
        let mut best: Option<&SpellInfo> = None;
        for spell in members {
            if best.is_some_and(|current| spell.quality() <= current.quality()) {
                continue;
            }
            if usable.is_some_and(|usable| !usable(spell)) {
                continue;
            }
            best = Some(spell);
        }
        best.cloned()
    }

    /// Tusker Fists: by its id, else by its line.
    #[must_use]
    pub fn resolve_tusker_fists(&self) -> Option<SpellInfo> {
        self.by_id(TUSKER_FISTS_SPELL_ID)
            .cloned()
            .or_else(|| self.resolve_family("Tusker Fists", None))
    }
}

fn is_void(element: MonsterDamageType) -> bool {
    matches!(
        element,
        MonsterDamageType::Nether | MonsterDamageType::VoidBasic
    )
}

/// The line the reference's attack table names for this element and kind,
/// or `None` (the void ring is named by id, not here).
#[must_use]
pub fn family_name(element: MonsterDamageType, kind: VtankCombatSpellType) -> Option<&'static str> {
    use MonsterDamageType as D;
    use VtankCombatSpellType as K;
    let void = is_void(element);
    Some(match (kind, element) {
        (K::Vuln, D::Acid) => "Acid Vulnerability Other",
        (K::Vuln, D::Bludgeon) => "Bludgeoning Vulnerability Other",
        (K::Vuln, D::Cold) => "Cold Vulnerability Other",
        (K::Vuln, D::Fire) => "Fire Vulnerability Other",
        (K::Vuln, D::Electric) => "Lightning Vulnerability Other",
        (K::Vuln, D::Pierce) => "Piercing Vulnerability Other",
        (K::Vuln, D::Slash) => "Blade Vulnerability Other",
        (K::Vuln, D::Harm) => "Drain Health Other",
        (K::Vuln, _) if void => "Destructive Curse",
        (K::War, D::Acid) => "Acid Stream",
        (K::War, D::Bludgeon) => "Shock Wave",
        (K::War, D::Cold) => "Frost Bolt",
        (K::War, D::Fire) => "Flame Bolt",
        (K::War, D::Electric) => "Lightning Bolt",
        (K::War, D::Pierce) => "Force Bolt",
        (K::War, D::Slash) => "Whirling Blade",
        (K::War, D::Harm) => "Martyr's Hecatomb",
        (K::War, _) if void => "Nether Bolt",
        (K::Arc, D::Acid) => "Acid Arc",
        (K::Arc, D::Bludgeon) => "Shock Arc",
        (K::Arc, D::Cold) => "Frost Arc",
        (K::Arc, D::Fire) => "Flame Arc",
        (K::Arc, D::Electric) => "Lightning Arc",
        (K::Arc, D::Pierce) => "Force Arc",
        (K::Arc, D::Slash) => "Blade Arc",
        (K::Arc, D::Harm) => "Harm Other",
        (K::Arc, _) if void => "Nether Arc",
        (K::Ring, D::Acid) => "Searing Disc",
        (K::Ring, D::Bludgeon) => "Tectonic Rifts",
        (K::Ring, D::Cold) => "Halo of Frost",
        (K::Ring, D::Fire) => "Cassius' Ring of Fire",
        (K::Ring, D::Electric) => "Eye of the Storm",
        (K::Ring, D::Pierce) => "Nuhmudira's Spines",
        (K::Ring, D::Slash) => "Horizon's Blades",
        (K::Ring, D::Harm) => "Curse of Raven Fury",
        (K::Streak, D::Acid) => "Acid Streak",
        (K::Streak, D::Bludgeon) => "Shock Wave Streak",
        (K::Streak, D::Cold) => "Frost Streak",
        (K::Streak, D::Fire) => "Flame Streak",
        (K::Streak, D::Electric) => "Lightning Streak",
        (K::Streak, D::Pierce) => "Force Streak",
        (K::Streak, D::Slash) => "Whirling Blade Streak",
        (K::Streak, D::Harm) => "Harm Other",
        (K::Streak, _) if void => "Nether Streak",
        _ => return None,
    })
}

/// The flight shape a spell's family declares: a straight bolt, an arc, or
/// none (never clearance-tested).
#[must_use]
pub fn projectile_shape_for(family: u32) -> Option<ProjectilePathKind> {
    match family {
        243..=249 | 639 | 117..=123 | 640 | 100_080 | 100_082 | 100_084 | 637 | 638 => {
            Some(ProjectilePathKind::Straight)
        }
        100_117..=100_123 | 100_640 => Some(ProjectilePathKind::Arc),
        _ => None,
    }
}

/// The furthest a spell reaches, metres, for the caster's skill in its own
/// school, less the fudge; held to 0..=75.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn range_meters(spell: &SpellInfo, skills: &[Skill], range_fudge: f64) -> f32 {
    let skill = if spell.school == 0 {
        0
    } else {
        find_skill(skills, spell.school).map_or(0, |info| info.current)
    };
    let range =
        spell.base_range_constant + spell.base_range_modifier * skill as f32 - range_fudge as f32;
    range.clamp(0.0, 75.0)
}

/// The element a magic attack plans for: prismatic is an ammunition policy
/// (automatic here), fists plan bludgeon, and automatic picks the school the
/// character is trained in (war, else void, else life's drain).
#[must_use]
pub fn resolve_magic_damage_mode(
    requested: MonsterDamageType,
    skills: &[Skill],
) -> MonsterDamageType {
    let requested = if requested == MonsterDamageType::Prismatic {
        MonsterDamageType::Auto
    } else {
        requested
    };
    if requested == MonsterDamageType::Fists {
        return MonsterDamageType::Bludgeon;
    }
    if requested != MonsterDamageType::Auto {
        return requested;
    }
    if is_trained(skills, WAR_MAGIC_SKILL) {
        return MonsterDamageType::Auto;
    }
    if is_trained(skills, VOID_MAGIC_SKILL) {
        return MonsterDamageType::VoidBasic;
    }
    if is_trained(skills, LIFE_MAGIC_SKILL) {
        MonsterDamageType::DrainAuto
    } else {
        MonsterDamageType::Auto
    }
}

fn is_trained(skills: &[Skill], skill_id: u32) -> bool {
    find_skill(skills, skill_id)
        .is_some_and(|skill| matches!(skill.training, Training::Trained | Training::Specialized))
}

/// A spell's line: the name without an "Incantation of " lead or a Roman
/// numeral tier.
#[must_use]
pub fn base_name(name: &str) -> String {
    base_name_span(name).to_owned()
}

/// [`base_name`], borrowed from the name.
fn base_name_span(name: &str) -> &str {
    const INCANTATION: &str = "Incantation of ";
    let mut span = name.trim();
    if span.is_ascii() {
        // An ASCII lead is its bytes: the same test without building it.
        if span.len() >= INCANTATION.len()
            && span.as_bytes()[..INCANTATION.len()].eq_ignore_ascii_case(INCANTATION.as_bytes())
        {
            span = span[INCANTATION.len()..].trim_start();
        }
    } else {
        let lead: String = span.chars().take(INCANTATION.len()).collect();
        if lead.chars().count() == INCANTATION.len()
            && ordinal::equals_ignore_case(&lead, INCANTATION)
        {
            span = span[lead.len()..].trim_start();
        }
    }
    if let Some(space) = span.rfind(' ')
        && space > 0
        && is_roman_numeral(&span[space + 1..])
    {
        span = span[..space].trim_end();
    }
    span
}

fn is_roman_numeral(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| matches!(c, 'I' | 'V' | 'X'))
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::character::{ActiveEnchantment, Skill, Training};

    use super::*;

    fn skill(id: u32) -> Skill {
        Skill {
            skill_id: id,
            name: format!("Skill {id}"),
            training: Training::Trained,
            current: 300,
            ..Skill::default()
        }
    }

    fn spell(id: u32, name: &str, difficulty: i32) -> SpellInfo {
        SpellInfo {
            spell_id: id,
            name: name.to_owned(),
            family: id,
            tier: 7,
            difficulty,
            mana_cost: 35,
            duration_seconds: 0.0,
            school: 34,
            is_offensive: true,
            is_projectile: true,
            target_mask: 0x10,
            ..SpellInfo::default()
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: FamilyTableMatchesAuthenticVerbatim
    #[test]
    fn family_table_matches_authentic_verbatim() {
        use MonsterDamageType as D;
        use VtankCombatSpellType as K;
        let cases = [
            (D::Acid, K::War, "Acid Stream"),
            (D::Bludgeon, K::War, "Shock Wave"),
            (D::Cold, K::War, "Frost Bolt"),
            (D::Fire, K::War, "Flame Bolt"),
            (D::Electric, K::War, "Lightning Bolt"),
            (D::Pierce, K::War, "Force Bolt"),
            (D::Slash, K::War, "Whirling Blade"),
            (D::Harm, K::War, "Martyr's Hecatomb"),
            (D::VoidBasic, K::War, "Nether Bolt"),
            (D::Acid, K::Arc, "Acid Arc"),
            (D::Bludgeon, K::Arc, "Shock Arc"),
            (D::Cold, K::Arc, "Frost Arc"),
            (D::Fire, K::Arc, "Flame Arc"),
            (D::Electric, K::Arc, "Lightning Arc"),
            (D::Pierce, K::Arc, "Force Arc"),
            (D::Slash, K::Arc, "Blade Arc"),
            (D::Harm, K::Arc, "Harm Other"),
            (D::VoidBasic, K::Arc, "Nether Arc"),
            (D::Acid, K::Ring, "Searing Disc"),
            (D::Bludgeon, K::Ring, "Tectonic Rifts"),
            (D::Cold, K::Ring, "Halo of Frost"),
            (D::Fire, K::Ring, "Cassius' Ring of Fire"),
            (D::Electric, K::Ring, "Eye of the Storm"),
            (D::Pierce, K::Ring, "Nuhmudira's Spines"),
            (D::Slash, K::Ring, "Horizon's Blades"),
            (D::Harm, K::Ring, "Curse of Raven Fury"),
            (D::Acid, K::Streak, "Acid Streak"),
            (D::Bludgeon, K::Streak, "Shock Wave Streak"),
            (D::Cold, K::Streak, "Frost Streak"),
            (D::Fire, K::Streak, "Flame Streak"),
            (D::Electric, K::Streak, "Lightning Streak"),
            (D::Pierce, K::Streak, "Force Streak"),
            (D::Slash, K::Streak, "Whirling Blade Streak"),
            (D::Harm, K::Streak, "Harm Other"),
            (D::VoidBasic, K::Streak, "Nether Streak"),
            (D::Acid, K::Vuln, "Acid Vulnerability Other"),
            (D::Slash, K::Vuln, "Blade Vulnerability Other"),
            (D::Harm, K::Vuln, "Drain Health Other"),
            (D::VoidBasic, K::Vuln, "Destructive Curse"),
        ];
        for (element, kind, expected) in cases {
            assert_eq!(
                family_name(element, kind),
                Some(expected),
                "{element:?} {kind:?}"
            );
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: ElementsAuthenticHasNoCaseForResolveToNothing
    #[test]
    fn elements_authentic_has_no_case_for_resolve_to_nothing() {
        use MonsterDamageType as D;
        for element in [
            D::Auto,
            D::None,
            D::Random,
            D::Fists,
            D::DrainAuto,
            D::Prismatic,
            D::Physical,
        ] {
            for kind in VtankCombatSpellType::ALL {
                assert_eq!(family_name(element, kind), None, "{element:?} {kind:?}");
            }
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: QualityWalkTakesTheHighestKnownMemberOfTheLine
    #[test]
    fn quality_walk_takes_the_highest_known_member_of_the_line() {
        let catalog = AttackSpellCatalog::build(&[
            spell(1, "Frost Bolt III", 100),
            spell(2, "Frost Bolt VII", 300),
            spell(3, "Incantation of Frost Bolt VII", 350),
            spell(4, "Frost Bolt V", 200),
        ]);
        let resolved = catalog.resolve(MonsterDamageType::Cold, VtankCombatSpellType::War, None);
        assert_eq!(resolved.map(|s| s.spell_id), Some(3));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: WallsBlastsAndVolleysAreNeverCandidates
    #[test]
    fn walls_blasts_and_volleys_are_never_candidates() {
        let catalog = AttackSpellCatalog::build(&[
            spell(1, "Frost Wall VII", 400),
            spell(2, "Frost Blast VII", 380),
            spell(3, "Frost Volley VII", 370),
            spell(4, "Frost Bolt II", 50),
        ]);
        assert_eq!(
            catalog
                .resolve(MonsterDamageType::Cold, VtankCombatSpellType::War, None)
                .map(|s| s.spell_id),
            Some(4)
        );
        assert!(
            catalog
                .resolve(MonsterDamageType::Cold, VtankCombatSpellType::Arc, None)
                .is_none()
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: VoidRingIsResolvedBySpellIdNotByName
    #[test]
    fn void_ring_is_resolved_by_spell_id_not_by_name() {
        let catalog =
            AttackSpellCatalog::build(&[spell(VOID_RING_SPELL_ID, "Coldeve's Fury", 400)]);
        for element in [MonsterDamageType::VoidBasic, MonsterDamageType::Nether] {
            assert_eq!(
                catalog
                    .resolve(element, VtankCombatSpellType::Ring, None)
                    .map(|s| s.spell_id),
                Some(VOID_RING_SPELL_ID)
            );
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: UsabilityPredicateSkipsToTheNextBestMemberOfTheLine
    #[test]
    fn usability_predicate_skips_to_the_next_best_member_of_the_line() {
        let catalog = AttackSpellCatalog::build(&[
            spell(1, "Flame Bolt VII", 300),
            spell(2, "Flame Bolt IV", 150),
        ]);
        let cheap = |s: &SpellInfo| s.difficulty <= 200;
        assert_eq!(
            catalog
                .resolve(
                    MonsterDamageType::Fire,
                    VtankCombatSpellType::War,
                    Some(&cheap)
                )
                .map(|s| s.spell_id),
            Some(2)
        );
        let none = |_: &SpellInfo| false;
        assert!(
            catalog
                .resolve(
                    MonsterDamageType::Fire,
                    VtankCombatSpellType::War,
                    Some(&none)
                )
                .is_none()
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: BaseNameStripsOnlyTheTierSuffixAndIncantationPrefix
    #[test]
    fn base_name_strips_only_the_tier_suffix_and_incantation_prefix() {
        for (name, expected) in [
            ("Frost Bolt VII", "Frost Bolt"),
            ("Incantation of Frost Bolt VII", "Frost Bolt"),
            ("Cassius' Ring of Fire", "Cassius' Ring of Fire"),
            ("Halo of Frost", "Halo of Frost"),
            ("Shock Wave Streak III", "Shock Wave Streak"),
            ("Harm Other I", "Harm Other"),
        ] {
            assert_eq!(base_name(name), expected);
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: AutoUsesVoidWhenWarIsUntrainedAndVoidIsTrained
    #[test]
    fn auto_uses_void_when_war_is_untrained_and_void_is_trained() {
        let auto = MonsterDamageType::Auto;
        assert_eq!(
            resolve_magic_damage_mode(auto, &[skill(34), skill(43)]),
            auto
        );
        assert_eq!(
            resolve_magic_damage_mode(auto, &[skill(43)]),
            MonsterDamageType::VoidBasic
        );
        assert_eq!(
            resolve_magic_damage_mode(auto, &[skill(33)]),
            MonsterDamageType::DrainAuto
        );
        assert_eq!(resolve_magic_damage_mode(auto, &[]), auto);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: PrismaticIsAnAmmunitionPolicyAndLeavesMagicOnAuto
    #[test]
    fn prismatic_is_an_ammunition_policy_and_leaves_magic_on_auto() {
        assert_eq!(
            resolve_magic_damage_mode(MonsterDamageType::Prismatic, &[skill(34)]),
            MonsterDamageType::Auto
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/AttackSpellCatalogTests.cs :: AFistsRowPlansBludgeonWhateverTheEnchantmentSays
    #[test]
    fn a_fists_row_plans_bludgeon_whatever_the_enchantment_says() {
        // The enchantment is not an input at all: the plan reads skills only.
        let _tusker = ActiveEnchantment {
            spell_id: 0x0B76,
            family: 1,
            tier: 1,
            seconds_remaining: 60.0,
        };
        assert_eq!(
            resolve_magic_damage_mode(MonsterDamageType::Fists, &[]),
            MonsterDamageType::Bludgeon
        );
    }
}
