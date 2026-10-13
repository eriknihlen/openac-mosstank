//! The tinkering arithmetic: which skill a material rolls against, how hard
//! an attempt is, the odds of success, and the damage estimate the salvage
//! chooser weighs iron against granite with. Plain functions of the
//! material, the two workmanships, the attempts taken and the skill.

use ac_plugin_abi::world::ObjectClass;

use crate::material::{self, Material};

/// The skill id of Magic Item Tinkering.
pub const MAGIC_ITEM_TINKERING_SKILL: u32 = 30;
/// The skill id of Armor Tinkering.
pub const ARMOR_TINKERING_SKILL: u32 = 29;
/// The skill id of Weapon Tinkering.
pub const WEAPON_TINKERING_SKILL: u32 = 28;
/// The skill id of Item Tinkering.
pub const ITEM_TINKERING_SKILL: u32 = 18;

/// An ordinary tinker: one of the ten attempts, no imbue.
pub const ORDINARY_SALVAGE: i32 = 1;
/// An imbue: a rend or a critical bonus burned into a weapon, once.
pub const IMBUE_SALVAGE: i32 = 2;

/// How much harder each attempt on the same item is, by the attempts it has
/// already taken: the first is the plain difficulty, the tenth four and a
/// half times it.
pub const ATTEMPT_MODIFIERS: [f64; 10] = [1.0, 1.1, 1.3, 1.6, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5];

/// The most attempts any one item takes.
pub const MAXIMUM_ATTEMPTS: i32 = 10;

/// Which tinkering skill an attempt with this material rolls against; 0
/// when the material is not salvage.
#[must_use]
pub fn skill_id(material: i32) -> u32 {
    use Material as M;
    match Material::from_id(material) {
        Some(
            M::Agate
            | M::Azurite
            | M::BlackOpal
            | M::Bloodstone
            | M::Carnelian
            | M::Citrine
            | M::FireOpal
            | M::GreenGarnet
            | M::Hematite
            | M::LapisLazuli
            | M::LavenderJade
            | M::Malachite
            | M::Opal
            | M::RedJade
            | M::RoseQuartz
            | M::SmokeyQuartz
            | M::Sunstone,
        ) => MAGIC_ITEM_TINKERING_SKILL,
        Some(
            M::Alabaster
            | M::ArmoredilloHide
            | M::Bronze
            | M::Ceramic
            | M::Marble
            | M::Peridot
            | M::ReedSharkHide
            | M::Steel
            | M::Wool
            | M::YellowTopaz
            | M::Zircon,
        ) => ARMOR_TINKERING_SKILL,
        Some(
            M::Aquamarine
            | M::BlackGarnet
            | M::Brass
            | M::Emerald
            | M::Granite
            | M::ImperialTopaz
            | M::Iron
            | M::Jet
            | M::Mahogany
            | M::Oak
            | M::RedGarnet
            | M::Velvet
            | M::WhiteSapphire,
        ) => WEAPON_TINKERING_SKILL,
        Some(
            M::Amber
            | M::Copper
            | M::Diamond
            | M::Ebony
            | M::Gold
            | M::GromnieHide
            | M::Linen
            | M::Moonstone
            | M::Pine
            | M::Porcelain
            | M::Pyreal
            | M::Ruby
            | M::Sapphire
            | M::Satin
            | M::Silver
            | M::Teak,
        ) => ITEM_TINKERING_SKILL,
        _ => 0,
    }
}

/// How much the material itself adds to the difficulty: gold and oak 10,
/// the eight spell-power gems 25, anything not named 20.
#[must_use]
pub fn material_modifier(material: i32) -> f64 {
    use Material as M;
    match Material::from_id(material) {
        Some(M::Gold | M::Oak) => 10.0,
        Some(
            M::Alabaster
            | M::ArmoredilloHide
            | M::Brass
            | M::Bronze
            | M::Ceramic
            | M::Granite
            | M::Linen
            | M::Marble
            | M::Moonstone
            | M::Opal
            | M::Pine
            | M::ReedSharkHide
            | M::Velvet
            | M::Wool,
        ) => 11.0,
        Some(
            M::Ebony
            | M::GreenGarnet
            | M::Iron
            | M::Mahogany
            | M::Porcelain
            | M::Satin
            | M::Steel
            | M::Teak,
        ) => 12.0,
        Some(
            M::Bloodstone
            | M::Carnelian
            | M::Citrine
            | M::Hematite
            | M::LavenderJade
            | M::Malachite
            | M::RedJade
            | M::RoseQuartz,
        ) => 25.0,
        _ => 20.0,
    }
}

/// Whether the material imbues ([`IMBUE_SALVAGE`]) or tinkers
/// ([`ORDINARY_SALVAGE`]).
#[must_use]
pub fn salvage_kind(material: i32) -> i32 {
    use Material as M;
    match Material::from_id(material) {
        Some(
            M::BlackOpal
            | M::FireOpal
            | M::Sunstone
            | M::Aquamarine
            | M::BlackGarnet
            | M::Emerald
            | M::ImperialTopaz
            | M::Jet
            | M::RedGarnet
            | M::WhiteSapphire,
        ) => IMBUE_SALVAGE,
        _ => ORDINARY_SALVAGE,
    }
}

/// The kinds of item this material may be imbued into; empty when it
/// imbues nothing. Sunstone raises a critical chance, which a casting
/// implement has no use for, so it alone leaves wands out.
#[must_use]
pub fn imbue_targets(material: i32) -> &'static [ObjectClass] {
    use Material as M;
    match Material::from_id(material) {
        Some(
            M::BlackOpal
            | M::FireOpal
            | M::Aquamarine
            | M::BlackGarnet
            | M::Emerald
            | M::ImperialTopaz
            | M::Jet
            | M::RedGarnet
            | M::WhiteSapphire,
        ) => &[
            ObjectClass::WandStaffOrb,
            ObjectClass::MeleeWeapon,
            ObjectClass::MissileWeapon,
        ],
        Some(M::Sunstone) => &[ObjectClass::MeleeWeapon, ObjectClass::MissileWeapon],
        _ => &[],
    }
}

/// The chance a skill has against a difficulty: an S-curve through an even
/// chance at equal numbers, clamped to 0..1. `factor` is how sharp it is
/// (0.03 for tinkering).
#[must_use]
pub fn skill_chance(skill: i32, difficulty: i32, factor: f64) -> f64 {
    let chance = 1.0 - (1.0 / (1.0 + (factor * f64::from(skill - difficulty)).exp()));
    chance.clamp(0.0, 1.0)
}

/// The multiplier for an item that has taken this many attempts, clamped to
/// the table.
#[must_use]
pub fn attempt_modifier(attempts_already_taken: i32) -> f64 {
    let row = usize::try_from(attempts_already_taken.max(0))
        .unwrap_or(0)
        .min(ATTEMPT_MODIFIERS.len() - 1);
    ATTEMPT_MODIFIERS[row]
}

/// The attempt's difficulty: the material's cost five times, plus twice it
/// per point of the item's workmanship, less a fifth of it per point of the
/// salvage's (doubled when the salvage is at least as good as the item),
/// times the attempt modifier, rounded down.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn difficulty(
    salvage_material: i32,
    salvage_workmanship: f64,
    item_workmanship: f64,
    attempts_already_taken: i32,
) -> i32 {
    let material = material_modifier(salvage_material);
    let attempt = attempt_modifier(attempts_already_taken);
    let workmanship = if salvage_workmanship >= item_workmanship {
        2.0
    } else {
        1.0
    };
    let rating = ((material * 5.0) + (item_workmanship * material * 2.0)
        - (salvage_workmanship * workmanship * material / 5.0))
        * attempt;
    rating.floor() as i32
}

/// The share of attempts like this one that succeed. An imbue is a third of
/// the ordinary chance, and the Charmed Smith augmentation adds five points
/// to an imbue.
#[must_use]
pub fn success_chance(
    salvage_material: i32,
    salvage_workmanship: f64,
    item_workmanship: f64,
    attempts_already_taken: i32,
    skill: i32,
    charmed_smith: bool,
) -> f64 {
    let difficulty = difficulty(
        salvage_material,
        salvage_workmanship,
        item_workmanship,
        attempts_already_taken,
    );
    let mut chance = skill_chance(skill, difficulty, 0.03);
    if salvage_kind(salvage_material) != IMBUE_SALVAGE {
        return chance;
    }
    chance /= 3.0;
    if charmed_smith {
        chance += 0.05;
    }
    chance
}

/// Damage per second as the salvage chooser weighs it: nine swings in ten
/// land between the floor and the ceiling, the tenth crits for twice the
/// ceiling. `variance` is how far below the ceiling a swing can land, as a
/// share of it.
#[must_use]
pub fn damage_per_second(maximum_damage: f64, variance: f64) -> f64 {
    let minimum = maximum_damage * (1.0 - variance);
    let critical = maximum_damage * 2.0;
    let average = (maximum_damage + minimum) / 2.0;
    (0.9 * average) + (0.1 * critical)
}

/// Whether iron (a point on the ceiling) is worth at least as much as
/// granite (a fifth off the spread) on this weapon now.
#[must_use]
pub fn iron_beats_granite(maximum_damage: f64, variance: f64) -> bool {
    damage_per_second(maximum_damage + 1.0, variance)
        >= damage_per_second(maximum_damage, variance * 0.8)
}

/// The damage types a rend can be chosen for: the name the choice offers,
/// the bit the server uses, and the salvage that rends it.
pub const IMBUE_DAMAGE_TYPES: [(&str, i32, Material); 10] = [
    ("Acid", 32, Material::Emerald),
    ("Bludgeoning", 4, Material::WhiteSapphire),
    ("Cold", 8, Material::Aquamarine),
    ("Electric", 64, Material::Jet),
    ("Fire", 16, Material::RedGarnet),
    ("Nether", 1024, Material::BlackOpal),
    ("Normal", 0, Material::BlackOpal),
    ("Piercing", 2, Material::BlackGarnet),
    ("SlashPierce", 3, Material::ImperialTopaz),
    ("Slashing", 1, Material::ImperialTopaz),
];

/// The salvage that rends this damage type (by its bit); `None` when none
/// does.
#[must_use]
pub fn default_salvage(damage_bit: i32) -> Option<Material> {
    IMBUE_DAMAGE_TYPES
        .iter()
        .find(|(_, bit, _)| *bit == damage_bit)
        .map(|(_, _, material)| *material)
}

/// The bit the server uses for a damage type named on the page, ignoring
/// case and surrounding space; -1 when the name is none of them.
#[must_use]
pub fn damage_bit(damage_type_name: &str) -> i32 {
    let name = damage_type_name.trim();
    IMBUE_DAMAGE_TYPES
        .iter()
        .find(|(own, _, _)| own.eq_ignore_ascii_case(name))
        .map_or(-1, |(_, bit, _)| *bit)
}

/// The salvage a damage type is rended with, by its name; the empty string
/// when the name is none of them.
#[must_use]
pub fn default_salvage_name(damage_type_name: &str) -> &'static str {
    match damage_bit(damage_type_name) {
        -1 => "",
        bit => default_salvage(bit).map_or("", |salvage| material::name(salvage.id())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(value: f64, decimals: i32) -> f64 {
        let scale = 10f64.powi(decimals);
        (value * scale).round_ties_even() / scale
    }

    const IRON: i32 = Material::Iron as i32;
    const BLACK_GARNET: i32 = Material::BlackGarnet as i32;

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: TheDifficultyOfAFirstIronTinkIsTheMaterialCostPlusTheItemLessTheBag
    #[test]
    fn the_difficulty_of_a_first_iron_tink_is_the_material_cost_plus_the_item_less_the_bag() {
        // (12 x 5) + (6 x 12 x 2) - (7.4 x 2 x 12 / 5) = 168.48 -> 168
        assert_eq!(difficulty(IRON, 7.4, 6.0, 0), 168);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: ASkillOfThreeHundredAgainstThatDifficultyIsANinetyEightPercentChance
    #[test]
    fn a_skill_of_three_hundred_against_that_difficulty_is_a_ninety_eight_percent_chance() {
        assert_eq!(
            round(success_chance(IRON, 7.4, 6.0, 0, 300, false), 5),
            0.98129
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: ABagNoBetterThanTheItemTakesTheSingleWorkmanshipDiscount
    #[test]
    fn a_bag_no_better_than_the_item_takes_the_single_workmanship_discount() {
        assert_eq!(difficulty(BLACK_GARNET, 9.5, 10.0, 0), 462);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: AnImbueIsAThirdOfTheOrdinaryChanceAndCharmedSmithAddsFivePoints
    #[test]
    fn an_imbue_is_a_third_of_the_ordinary_chance_and_charmed_smith_adds_five_points() {
        assert_eq!(
            round(success_chance(BLACK_GARNET, 9.5, 10.0, 0, 500, false), 5),
            0.25256
        );
        assert_eq!(
            round(success_chance(BLACK_GARNET, 9.5, 10.0, 0, 500, true), 5),
            0.30256
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: TheSixthAttemptIsTwoAndAHalfTimesTheFirst
    #[test]
    fn the_sixth_attempt_is_two_and_a_half_times_the_first() {
        assert_eq!(attempt_modifier(5), 2.5);
        assert_eq!(difficulty(BLACK_GARNET, 9.5, 10.0, 5), 1155);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: TheAttemptTableIsTheTenRowsItWasPortedFrom
    #[test]
    fn the_attempt_table_is_the_ten_rows_it_was_ported_from() {
        assert_eq!(
            ATTEMPT_MODIFIERS,
            [1.0, 1.1, 1.3, 1.6, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5]
        );
        assert_eq!(attempt_modifier(-3), 1.0);
        assert_eq!(attempt_modifier(99), 4.5);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: EqualSkillAndDifficultyIsAnEvenChanceAndTheCurveIsClamped
    #[test]
    fn equal_skill_and_difficulty_is_an_even_chance_and_the_curve_is_clamped() {
        assert_eq!(round(skill_chance(400, 400, 0.03), 10), 0.5);
        assert!((0.0..=1.0).contains(&skill_chance(4000, 0, 0.03)));
        assert!((0.0..=1.0).contains(&skill_chance(0, 4000, 0.03)));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: IronBeatsGraniteOnAFortyDamageQuarterSpreadWeapon
    #[test]
    fn iron_beats_granite_on_a_forty_damage_quarter_spread_weapon() {
        assert_eq!(round(damage_per_second(41.0, 0.25), 6), 40.4875);
        assert_eq!(round(damage_per_second(40.0, 0.2), 6), 40.4);
        assert!(iron_beats_granite(40.0, 0.25));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: GraniteBeatsIronWhenTheSpreadIsWideEnoughToBeWorthNarrowing
    #[test]
    fn granite_beats_iron_when_the_spread_is_wide_enough_to_be_worth_narrowing() {
        assert_eq!(round(damage_per_second(21.0, 0.5), 6), 18.375);
        assert_eq!(round(damage_per_second(20.0, 0.4), 6), 18.4);
        assert!(!iron_beats_granite(20.0, 0.5));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: EachMaterialNamesTheTinkeringSkillItRollsAgainst
    #[test]
    fn each_material_names_the_tinkering_skill_it_rolls_against() {
        assert_eq!(skill_id(Material::Iron.id()), 28);
        assert_eq!(skill_id(Material::Steel.id()), 29);
        assert_eq!(skill_id(Material::Opal.id()), 30);
        assert_eq!(skill_id(Material::Gold.id()), 18);
        assert_eq!(skill_id(Material::Leather.id()), 0);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: TheMaterialCostBandsAreTheFourTheyWerePortedFrom
    #[test]
    fn the_material_cost_bands_are_the_four_they_were_ported_from() {
        assert_eq!(material_modifier(Material::Gold.id()), 10.0);
        assert_eq!(material_modifier(Material::Oak.id()), 10.0);
        assert_eq!(material_modifier(Material::Linen.id()), 11.0);
        assert_eq!(material_modifier(Material::Iron.id()), 12.0);
        assert_eq!(material_modifier(Material::RedJade.id()), 25.0);
        assert_eq!(material_modifier(Material::Diamond.id()), 20.0);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: OnlyTheTenImbueSalvagesImbueAndSunstoneAloneLeavesWandsOut
    #[test]
    fn only_the_ten_imbue_salvages_imbue_and_sunstone_alone_leaves_wands_out() {
        let imbues: Vec<Material> = material::named()
            .filter(|material| salvage_kind(material.id()) == IMBUE_SALVAGE)
            .collect();
        assert_eq!(imbues.len(), 10);
        assert!(imbues.contains(&Material::BlackOpal));
        assert!(imbues.contains(&Material::Sunstone));
        assert!(!imbues.contains(&Material::Iron));
        assert!(imbue_targets(Material::BlackOpal.id()).contains(&ObjectClass::WandStaffOrb));
        assert!(!imbue_targets(Material::Sunstone.id()).contains(&ObjectClass::WandStaffOrb));
        assert!(imbue_targets(Material::Iron.id()).is_empty());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: MaterialNamesResolveBothWaysAndTheGroupHeadingsNameNothing
    #[test]
    fn material_names_resolve_both_ways_and_the_group_headings_name_nothing() {
        assert_eq!(material::name(Material::Iron.id()), "Iron");
        assert_eq!(material::id("iron"), Material::Iron.id());
        assert_eq!(material::id("Green Garnet"), Material::GreenGarnet.id());
        assert_eq!(material::name(Material::Metal.id()), "");
        assert_eq!(material::id("Metal"), 0);
        assert!(!material::named().any(|material| material == Material::Metal));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/TinkerCalcTests.cs :: EachDamageTypeNamesTheSalvageThatRendsIt
    #[test]
    fn each_damage_type_names_the_salvage_that_rends_it() {
        for (name, salvage) in [
            ("Acid", "Emerald"),
            ("Bludgeoning", "White Sapphire"),
            ("Cold", "Aquamarine"),
            ("Electric", "Jet"),
            ("Fire", "Red Garnet"),
            ("Nether", "Black Opal"),
            ("Piercing", "Black Garnet"),
            ("Slashing", "Imperial Topaz"),
            ("SlashPierce", "Imperial Topaz"),
            ("Normal", "Black Opal"),
            ("Bogus", ""),
        ] {
            assert_eq!(default_salvage_name(name), salvage, "{name}");
        }
        assert_eq!(damage_bit("Slashing"), 1);
        assert_eq!(damage_bit("Nether"), 1024);
        assert_eq!(damage_bit("Bogus"), -1);
    }
}
