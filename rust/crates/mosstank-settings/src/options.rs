//! VTank's option catalogue as MossTank knows it: every setting's name,
//! declared value type and default; the pages each is shown on, the text
//! that explains it, the labels of a choice's values, and the default lists
//! a new settings profile starts with.
//!
//! Names compare in any case (`StringComparer.OrdinalIgnoreCase`).

use std::collections::HashMap;
use std::sync::LazyLock;

use mosstank_dotnet::text::{casing, ordinal};

use crate::model::RechargeHandlerRow;
use crate::monster::MonsterValue;

/// The `eSettingValueType` a Settings row declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum VtankSettingValueType {
    Bool = 1,
    Double = 2,
    Int = 3,
    Single = 4,
    String = 5,
    Enum = 6,
    Custom = 7,
}

/// An option's default, as the catalogue spells it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptionDefault {
    /// A switch.
    Bool(bool),
    /// A number.
    Number(f64),
    /// Text.
    Text(&'static str),
}

impl OptionDefault {
    /// As a [`MonsterValue`].
    #[must_use]
    pub fn value(self) -> MonsterValue {
        match self {
            Self::Bool(value) => MonsterValue::from_boolean(value),
            Self::Number(value) => MonsterValue::from_number(value),
            Self::Text(value) => MonsterValue::from_text(value),
        }
    }
}

/// MossTank's own option: walk to this character's rare corpse.
pub const WALK_TO_RARE_CORPSE: &str = "WalkToRareCorpse";
const WALK_TO_OWN_CORPSES_ALIAS: &str = "WalkToOwnCorpses";
const WALK_TO_OWN_RARE_CORPSES_ALIAS: &str = "WalkToOwnRareCorpses";
const SHOW_NAV_LINES: &str = "ShowNavLines";

/// The one name the reference's settings table holds that nothing reads.
/// A macro may set and read it; it has no row on the options page.
pub const UNUSED_SETTING: &str = "WhoYouGonnaCall";

use OptionDefault as D;
use VtankSettingValueType as T;

/// Every option MossTank answers to, with its default and declared type,
/// in catalogue order: VTank's settings plus [`WALK_TO_RARE_CORPSE`] and
/// `ShowNavLines`.
pub const OPTIONS: [(&str, OptionDefault, VtankSettingValueType); 139] = [
    ("EnableLooting", D::Bool(false), T::Bool),
    (WALK_TO_RARE_CORPSE, D::Bool(false), T::Bool),
    ("ShowNavLines", D::Bool(false), T::Bool),
    ("EnableNav", D::Bool(false), T::Bool),
    ("EnableBuffing", D::Bool(true), T::Bool),
    ("EnableCombat", D::Bool(true), T::Bool),
    ("SpellDiffExcessThreshold-Hunt", D::Number(25.0), T::Int),
    ("SpellDiffExcessThreshold-Buff", D::Number(5.0), T::Int),
    (
        "ArrowheadFletchDiffExcessThreshold",
        D::Number(10.0),
        T::Int,
    ),
    ("Recharge-Norm-HitP", D::Number(75.0), T::Int),
    ("Recharge-Norm-Stam", D::Number(50.0), T::Int),
    ("Recharge-Norm-Mana", D::Number(50.0), T::Int),
    ("Recharge-NoTarg-HitP", D::Number(1.0), T::Int),
    ("Recharge-NoTarg-Stam", D::Number(1.0), T::Int),
    ("Recharge-NoTarg-Mana", D::Number(1.0), T::Int),
    ("Recharge-Helper-HitP", D::Number(20.0), T::Int),
    ("Recharge-Helper-Stam", D::Number(1.0), T::Int),
    ("Recharge-Helper-Mana", D::Number(1.0), T::Int),
    ("DoHelp", D::Bool(true), T::Bool),
    ("AttackDistance", D::Number(0.0208333333333333), T::Double),
    ("AttackMinimumDistance", D::Number(0.0), T::Double),
    ("ApproachDistance", D::Number(0.0), T::Double),
    ("RingDistance", D::Number(0.0208333333333333), T::Double),
    ("CorpseApproachRange-Max", D::Number(0.0), T::Double),
    ("CorpseApproachRange-Min", D::Number(0.014), T::Double),
    (
        "NavCloseStopRange",
        D::Number(0.00833333333333333),
        T::Double,
    ),
    ("NavFarStopRange", D::Number(999999.0), T::Double),
    (
        "UsePortalDistance",
        D::Number(0.0166666666666667),
        T::Double,
    ),
    (
        "HelperDistanceHitP",
        D::Number(0.310416666666667),
        T::Double,
    ),
    (
        "HelperDistanceStam",
        D::Number(0.310416666666667),
        T::Double,
    ),
    (
        "HelperDistanceMana",
        D::Number(0.166666666666667),
        T::Double,
    ),
    ("MinimumRingTargets", D::Number(4.0), T::Int),
    ("DefaultMeleeAttackHeight", D::Number(2.0), T::Int),
    ("CastDispelSelf", D::Bool(false), T::Bool),
    ("UseDispelItems", D::Bool(false), T::Bool),
    ("AutoCram", D::Bool(false), T::Bool),
    ("AutoStack", D::Bool(true), T::Bool),
    ("ReadUnknownScrolls", D::Bool(true), T::Bool),
    ("UseDispelDrum", D::Bool(false), T::Bool),
    ("SwitchWandsToDebuff", D::Bool(false), T::Bool),
    ("AutoCraftItems", D::Bool(true), T::Bool),
    ("UseHealersHeart", D::Bool(true), T::Bool),
    ("JumpOutWandCasting", D::Bool(false), T::Bool),
    ("LootAllCorpses", D::Bool(false), T::Bool),
    ("LootFellowCorpses", D::Bool(false), T::Bool),
    ("DoJiggle", D::Bool(false), T::Bool),
    ("RandomHelperBuffs", D::Bool(false), T::Bool),
    ("RandomHelperIntervalSeconds", D::Number(5.0), T::Double),
    ("IdlePeaceMode", D::Bool(false), T::Bool),
    ("TargetLock", D::Bool(false), T::Bool),
    ("StopMacroOnDeath", D::Bool(true), T::Bool),
    ("UseArcs", D::Number(1.0), T::Enum),
    ("ArcRange", D::Number(0.0208333333333333), T::Double),
    ("TargetSelectMethod", D::Number(3.0), T::Enum),
    (
        "TargetSelectAngleRange",
        D::Number(0.0208333333333333),
        T::Double,
    ),
    ("IdleBuffTopoff", D::Bool(false), T::Bool),
    ("IdleBuffTopoffTimeSeconds", D::Number(1200.0), T::Int),
    ("RebuffTimeRemainingSeconds", D::Number(300.0), T::Int),
    ("RefillWornMana", D::Bool(true), T::Bool),
    ("RefillWornMana-Item-ManaPercent", D::Number(33.0), T::Int),
    ("BuffProfile-Prots", D::Text("ALFCBPS"), T::String),
    ("BuffProfile-Banes", D::Text("ALFCBPS"), T::String),
    ("BuffProfile_Prots", D::Number(2.0), T::Enum),
    ("BuffProfile_Banes", D::Number(2.0), T::Enum),
    ("DebuffEachFirst", D::Number(1.0), T::Enum),
    ("AutoAttackPower", D::Bool(true), T::Bool),
    ("LootPriorityBoost", D::Bool(false), T::Bool),
    ("CorpseCacheTimeoutMinutes", D::Number(60.0), T::Double),
    (
        "CorpseItemAppearanceTimeoutSeconds",
        D::Number(6.0),
        T::Double,
    ),
    ("CorpseItemIDTimeoutSeconds", D::Number(60.0), T::Double),
    ("DebuffSelectionMethod", D::Number(2.0), T::Enum),
    ("ManaStoneLootCount", D::Number(4.0), T::Int),
    ("ManaTankMinimumMana", D::Number(1000.0), T::Int),
    ("SplitPeas", D::Bool(true), T::Bool),
    ("SpellCompMin-Critical", D::Number(4.0), T::Int),
    ("SpellCompMin-Normal", D::Number(20.0), T::Int),
    ("SpellCompMin-Idle", D::Number(20.0), T::Int),
    ("RechargeBoostTimeSeconds", D::Number(5.0), T::Double),
    ("RechargeBoostAmount", D::Number(40.0), T::Int),
    ("UseSpecialAmmo", D::Number(0.0), T::Enum),
    ("OpenDoors", D::Bool(false), T::Bool),
    ("DoorIDRange", D::Number(0.0833333333333333), T::Double),
    ("DoorOpenRange", D::Number(0.0166666666666667), T::Double),
    ("DoorLockpickDiffExcessThreshold", D::Number(-50.0), T::Int),
    ("ManaChargesWhenOff", D::Bool(true), T::Bool),
    ("AutoFellowManagement", D::Bool(true), T::Bool),
    ("MinimumHealKitSuccessChance", D::Number(95.0), T::Int),
    ("UseKitsInMagicMode", D::Bool(true), T::Bool),
    ("StaminaToHealthMultiplier", D::Number(1.9), T::Double),
    ("ManaToHealthMultiplier", D::Number(2.8), T::Double),
    ("NavPriorityBoost", D::Bool(false), T::Bool),
    ("DeleteGhostMonsters", D::Bool(true), T::Bool),
    ("GhostMonsterSpellAttemptCount", D::Number(200.0), T::Int),
    ("WhoYouGonnaCall", D::Bool(true), T::Bool),
    ("BlacklistMonsterAttemptCount", D::Number(4.0), T::Int),
    ("BlacklistMonsterTimeoutSeconds", D::Number(120.0), T::Int),
    ("CombineSalvage", D::Bool(true), T::Bool),
    ("LootOnlyRareCorpses", D::Bool(false), T::Bool),
    ("DeleteGhostMonstersByHPTracker", D::Bool(true), T::Bool),
    ("GhostDeleteHPTrackerSeconds", D::Number(30.0), T::Int),
    ("GoToPeaceModeToUseKits", D::Bool(false), T::Bool),
    ("UseRecklessness", D::Bool(true), T::Bool),
    ("DebuffPrecastSeconds", D::Number(5.0), T::Int),
    ("ClearLevelBoostFlagOnCast", D::Bool(true), T::Bool),
    ("IdleCraftCount_HealthKits", D::Number(2.0), T::Int),
    ("IdleCraftCount_StamKits", D::Number(2.0), T::Int),
    ("IdleCraftCount_ManaKits", D::Number(2.0), T::Int),
    ("IdleCraftCount_HealthFood", D::Number(15.0), T::Int),
    ("IdleCraftCount_StamFood", D::Number(15.0), T::Int),
    ("IdleCraftCount_ManaFood", D::Number(15.0), T::Int),
    ("BuffCastRecast_Seconds", D::Number(30.0), T::Int),
    ("BuffCastRecastReset_Seconds", D::Number(30.0), T::Int),
    ("EnableMeta", D::Bool(false), T::Bool),
    ("BlacklistedSpellComps", D::Text(""), T::String),
    ("DropToPeaceModeRetryCount", D::Number(34.0), T::Int),
    ("FollowAroundCorners", D::Bool(true), T::Bool),
    ("BlacklistCorpseOpenAttemptCount", D::Number(30.0), T::Int),
    (
        "BlacklistCorpseOpenTimeoutSeconds",
        D::Number(200.0),
        T::Int,
    ),
    ("SummonPets", D::Bool(true), T::Bool),
    ("PetRangeMode", D::Number(0.0), T::Enum),
    ("PetCustomRange", D::Number(0.0208333333333333), T::Double),
    ("PetRefillCount-Idle", D::Number(3.0), T::Int),
    ("PetRefillCount-Normal", D::Number(1.0), T::Int),
    ("CorpseOpenTimeoutSeconds", D::Number(1.5), T::Double),
    ("PetMonsterDensity", D::Number(1.0), T::Int),
    ("CorpseLootItemMaxAttempts", D::Number(20.0), T::Int),
    ("FastCastBuffs", D::Bool(false), T::Bool),
    ("UseBreakableTurnTo", D::Bool(true), T::Bool),
    ("UseProjectileAwareness", D::Bool(true), T::Bool),
    ("CollisionProjectileRadius", D::Number(0.4), T::Double),
    ("CollisionStepDistance", D::Number(0.7), T::Double),
    ("ShowCollisionDebug", D::Bool(false), T::Bool),
    ("MaximumCollisionChecksPerTick", D::Number(500.0), T::Int),
    ("SpellRangeFudge", D::Number(1.0), T::Double),
    ("BuffWithUntrained-Item", D::Number(80.0), T::Int),
    ("BuffWithUntrained-Creature", D::Number(80.0), T::Int),
    ("BuffWithUntrained-Life", D::Number(80.0), T::Int),
    ("AllowDebuffFallback", D::Bool(false), T::Bool),
    (
        "RechargeHandlerSet",
        D::Text("RechargeHandlerSet"),
        T::Custom,
    ),
];

/// The settings of VTank's Settings table, in its order (every option but
/// MossTank's two).
pub static NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    OPTIONS
        .iter()
        .map(|(name, _, _)| *name)
        .filter(|name| *name != WALK_TO_RARE_CORPSE && *name != SHOW_NAV_LINES)
        .collect()
});

/// The names `/vt opt` answers to: the settings, then MossTank's two.
pub static COMMAND_NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut names = NAMES.clone();
    names.push(WALK_TO_RARE_CORPSE);
    names.push(SHOW_NAV_LINES);
    names
});

/// The names the advanced options edit: the settings and the rare walk.
pub static ADVANCED_NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut names = NAMES.clone();
    names.push(WALK_TO_RARE_CORPSE);
    names
});

/// Each option's place in [`OPTIONS`], by upper-cased name.
static BY_NAME: LazyLock<HashMap<String, usize>> = LazyLock::new(|| {
    OPTIONS
        .iter()
        .enumerate()
        .map(|(index, (name, _, _))| (casing::to_upper_invariant(name), index))
        .collect()
});

fn is_legacy_rare_walk_name(name: &str) -> bool {
    ordinal::equals_ignore_case(name, WALK_TO_OWN_CORPSES_ALIAS)
        || ordinal::equals_ignore_case(name, WALK_TO_OWN_RARE_CORPSES_ALIAS)
}

/// Whether `/vt opt` knows the name (in any case, old rare-walk spellings
/// included).
#[must_use]
pub fn is_known(name: &str) -> bool {
    canonical(name).is_some()
}

/// The catalogue's spelling of a known name.
#[must_use]
pub fn canonical(name: &str) -> Option<&'static str> {
    if is_legacy_rare_walk_name(name) {
        return Some(WALK_TO_RARE_CORPSE);
    }
    COMMAND_INDEX.get(name).map(|&index| COMMAND_NAMES[index])
}

/// The place of the first of `names` equal to each name ignoring case: the
/// answer a walk of `names` with `equals_ignore_case` finds.
fn first_by_name(
    names: impl Iterator<Item = &'static str>,
) -> mosstank_dotnet::collections::IgnoreCaseStringMap<usize> {
    let mut index = mosstank_dotnet::collections::IgnoreCaseStringMap::new();
    for (at, name) in names.enumerate() {
        index.try_add(name.to_owned(), at);
    }
    index
}

/// [`COMMAND_NAMES`] by name, any case.
static COMMAND_INDEX: LazyLock<mosstank_dotnet::collections::IgnoreCaseStringMap<usize>> =
    LazyLock::new(|| first_by_name(COMMAND_NAMES.iter().copied()));

/// [`PAGES`] by name, any case.
static PAGE_INDEX: LazyLock<mosstank_dotnet::collections::IgnoreCaseStringMap<usize>> =
    LazyLock::new(|| first_by_name(PAGES.iter().map(|(name, _)| *name)));

/// [`DESCRIPTIONS`] by name, any case.
static DESCRIPTION_INDEX: LazyLock<mosstank_dotnet::collections::IgnoreCaseStringMap<usize>> =
    LazyLock::new(|| first_by_name(DESCRIPTIONS.iter().map(|(name, _)| *name)));

/// The option's default, the number 0 for a name the catalogue lacks.
#[must_use]
pub fn default(name: &str) -> MonsterValue {
    BY_NAME.get(&casing::to_upper_invariant(name)).map_or_else(
        || MonsterValue::from_number(0.0),
        |&index| OPTIONS[index].1.value(),
    )
}

/// The type VTank's own Settings row declares for the name, `Double` for a
/// name the catalogue lacks.
#[must_use]
pub fn declared_type(name: &str) -> VtankSettingValueType {
    BY_NAME
        .get(&casing::to_upper_invariant(name))
        .map_or(VtankSettingValueType::Double, |&index| OPTIONS[index].2)
}

/// Every page bit some setting is filed under, ascending.
pub static CATEGORY_BITS: LazyLock<Vec<i32>> = LazyLock::new(|| {
    let mut bits: Vec<i32> = PAGES
        .iter()
        .flat_map(|(_, mask)| decompose_bits(mask.0))
        .collect();
    bits.sort_unstable();
    bits.dedup();
    bits
});

/// Each page bit's name, from a setting known to be filed under it alone.
pub static CATEGORY_NAMES_BY_BIT: LazyLock<HashMap<i32, &'static str>> = LazyLock::new(|| {
    let anchors = [
        ("RandomHelperBuffs", "Misc"),
        ("Recharge-Norm-HitP", "Recharge"),
        ("DefaultMeleeAttackHeight", "MeleeCombat"),
        ("SpellDiffExcessThreshold-Hunt", "SpellCombat"),
        ("AttackDistance", "Ranges"),
        ("EnableNav", "Navigation"),
        ("EnableBuffing", "Buffing"),
        ("ArrowheadFletchDiffExcessThreshold", "Crafting"),
        ("EnableLooting", "Looting"),
    ];
    let mut map = HashMap::new();
    for (setting, category) in anchors {
        if let Some(page) = page_of(setting) {
            map.insert(page.0, category);
        }
    }
    map
});

fn decompose_bits(mask: i32) -> Vec<i32> {
    let mut bits = Vec::new();
    let mut bit = 1i32;
    while bit != 0 && bit <= mask {
        if mask & bit != 0 {
            bits.push(bit);
        }
        bit = bit.wrapping_shl(1);
    }
    bits
}

/// The pages of the advanced options a setting is shown on; the bits are
/// the ones a profile's `SettingsCategories` table stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VtankOptionPage(pub i32);

#[allow(missing_docs)]
impl VtankOptionPage {
    pub const NONE: Self = Self(0);
    pub const MISC: Self = Self(0x001);
    pub const RECHARGE: Self = Self(0x002);
    pub const MELEE_COMBAT: Self = Self(0x004);
    pub const SPELL_COMBAT: Self = Self(0x008);
    pub const RANGES: Self = Self(0x010);
    pub const NAVIGATION: Self = Self(0x020);
    pub const BUFFING: Self = Self(0x040);
    pub const CRAFTING: Self = Self(0x080);
    pub const LOOTING: Self = Self(0x100);
}

const MISC: i32 = VtankOptionPage::MISC.0;
const RECHARGE: i32 = VtankOptionPage::RECHARGE.0;
const MELEE: i32 = VtankOptionPage::MELEE_COMBAT.0;
const SPELL: i32 = VtankOptionPage::SPELL_COMBAT.0;
const COMBAT: i32 = MELEE | SPELL;
const RANGES: i32 = VtankOptionPage::RANGES.0;
const NAVIGATION: i32 = VtankOptionPage::NAVIGATION.0;
const BUFFING: i32 = VtankOptionPage::BUFFING.0;
const CRAFTING: i32 = VtankOptionPage::CRAFTING.0;
const LOOTING: i32 = VtankOptionPage::LOOTING.0;

const fn page(bits: i32) -> VtankOptionPage {
    VtankOptionPage(bits)
}

/// The pages each setting is shown on; a setting not listed has no page.
pub const PAGES: [(&str, VtankOptionPage); 136] = [
    ("EnableLooting", page(LOOTING)),
    ("EnableNav", page(NAVIGATION)),
    ("EnableBuffing", page(BUFFING)),
    ("EnableCombat", page(COMBAT)),
    ("SpellDiffExcessThreshold-Hunt", page(SPELL)),
    ("SpellDiffExcessThreshold-Buff", page(BUFFING)),
    ("ArrowheadFletchDiffExcessThreshold", page(CRAFTING)),
    ("RECHARGE-Norm-HitP", page(RECHARGE)),
    ("RECHARGE-Norm-Stam", page(RECHARGE)),
    ("RECHARGE-Norm-Mana", page(RECHARGE)),
    ("RECHARGE-NoTarg-HitP", page(RECHARGE)),
    ("RECHARGE-NoTarg-Stam", page(RECHARGE)),
    ("RECHARGE-NoTarg-Mana", page(RECHARGE)),
    ("RECHARGE-Helper-HitP", page(RECHARGE)),
    ("RECHARGE-Helper-Stam", page(RECHARGE)),
    ("RECHARGE-Helper-Mana", page(RECHARGE)),
    ("DoHelp", page(RECHARGE)),
    ("AttackDistance", page(RANGES)),
    ("AttackMinimumDistance", page(RANGES)),
    ("ApproachDistance", page(RANGES | NAVIGATION)),
    ("RingDistance", page(RANGES | SPELL)),
    ("CorpseApproachRange-Max", page(RANGES | NAVIGATION)),
    ("CorpseApproachRange-Min", page(RANGES | NAVIGATION)),
    ("NavCloseStopRange", page(RANGES | NAVIGATION)),
    ("NavFarStopRange", page(RANGES | NAVIGATION)),
    ("UsePortalDistance", page(RANGES | NAVIGATION)),
    ("HelperDistanceHitP", page(RANGES | RECHARGE)),
    ("HelperDistanceStam", page(RANGES | RECHARGE)),
    ("HelperDistanceMana", page(RANGES | RECHARGE)),
    ("MinimumRingTargets", page(SPELL)),
    ("DefaultMeleeAttackHeight", page(MELEE)),
    ("CastDispelSelf", page(RECHARGE)),
    ("UseDispelItems", page(RECHARGE)),
    ("AutoCram", page(MISC)),
    ("AutoStack", page(MISC)),
    ("ReadUnknownScrolls", page(LOOTING)),
    ("UseDispelDrum", page(RECHARGE)),
    ("SwitchWandsToDebuff", page(SPELL)),
    ("AutoCraftItems", page(CRAFTING)),
    ("UseHealersHeart", page(RECHARGE)),
    ("JumpOutWandCasting", page(RECHARGE | SPELL)),
    ("LootAllCorpses", page(LOOTING)),
    ("LootFellowCorpses", page(LOOTING)),
    ("DoJiggle", page(SPELL)),
    ("RandomHelperBuffs", page(MISC)),
    ("RandomHelperIntervalSeconds", page(MISC)),
    ("IdlePeaceMode", page(MISC)),
    ("TargetLock", page(COMBAT)),
    ("StopMacroOnDeath", page(MISC)),
    ("UseArcs", page(SPELL)),
    ("ArcRange", page(RANGES | SPELL)),
    ("TargetSelectMethod", page(COMBAT)),
    ("TargetSelectAngleRange", page(RANGES | COMBAT)),
    ("IdleBuffTopoff", page(BUFFING)),
    ("IdleBuffTopoffTimeSeconds", page(BUFFING)),
    ("RebuffTimeRemainingSeconds", page(BUFFING)),
    ("RefillWornMana", page(MISC)),
    ("RefillWornMana-Item-ManaPercent", page(MISC)),
    ("BuffProfile-Prots", page(BUFFING)),
    ("BuffProfile-Banes", page(BUFFING)),
    ("BuffProfile_Prots", page(BUFFING)),
    ("BuffProfile_Banes", page(BUFFING)),
    ("DebuffEachFirst", page(COMBAT)),
    ("AutoAttackPower", page(MELEE)),
    ("LootPriorityBoost", page(LOOTING)),
    ("CorpseCacheTimeoutMinutes", page(NAVIGATION)),
    ("CorpseItemAppearanceTimeoutSeconds", page(LOOTING)),
    ("CorpseItemIDTimeoutSeconds", page(LOOTING)),
    ("DebuffSelectionMethod", page(COMBAT)),
    ("ManaStoneLootCount", page(MISC | LOOTING)),
    ("ManaTankMinimumMana", page(MISC | LOOTING)),
    ("SplitPeas", page(CRAFTING)),
    ("SpellCompMin-Critical", page(CRAFTING)),
    ("SpellCompMin-Normal", page(CRAFTING)),
    ("SpellCompMin-Idle", page(CRAFTING)),
    ("RechargeBoostTimeSeconds", page(RECHARGE)),
    ("RechargeBoostAmount", page(RECHARGE)),
    ("UseSpecialAmmo", page(CRAFTING)),
    ("OpenDoors", page(NAVIGATION)),
    ("DoorIDRange", page(RANGES | NAVIGATION)),
    ("DoorOpenRange", page(RANGES | NAVIGATION)),
    (
        "DoorLockpickDiffExcessThreshold",
        page(NAVIGATION | CRAFTING),
    ),
    ("ManaChargesWhenOff", page(MISC)),
    ("AutoFellowManagement", page(MISC)),
    ("MinimumHealKitSuccessChance", page(RECHARGE)),
    ("UseKitsInMagicMode", page(RECHARGE)),
    ("StaminaToHealthMultiplier", page(RECHARGE)),
    ("ManaToHealthMultiplier", page(RECHARGE)),
    ("NavPriorityBoost", page(NAVIGATION)),
    ("DeleteGhostMonsters", page(MISC)),
    ("GhostMonsterSpellAttemptCount", page(MISC)),
    ("WhoYouGonnaCall", page(MISC)),
    ("BlacklistMonsterAttemptCount", page(MISC)),
    ("BlacklistMonsterTimeoutSeconds", page(MISC)),
    ("CombineSalvage", page(LOOTING)),
    ("LootOnlyRareCorpses", page(LOOTING)),
    ("DeleteGhostMonstersByHPTracker", page(MISC | COMBAT)),
    ("GhostDeleteHPTrackerSeconds", page(MISC | COMBAT)),
    ("GoToPeaceModeToUseKits", page(MISC | RECHARGE)),
    ("UseRecklessness", page(MELEE)),
    ("DebuffPrecastSeconds", page(SPELL)),
    ("ClearLevelBoostFlagOnCast", page(RECHARGE)),
    ("IdleCraftCount_HealthKits", page(RECHARGE | CRAFTING)),
    ("IdleCraftCount_StamKits", page(RECHARGE | CRAFTING)),
    ("IdleCraftCount_ManaKits", page(RECHARGE | CRAFTING)),
    ("IdleCraftCount_HealthFood", page(RECHARGE | CRAFTING)),
    ("IdleCraftCount_StamFood", page(RECHARGE | CRAFTING)),
    ("IdleCraftCount_ManaFood", page(RECHARGE | CRAFTING)),
    ("BuffCastRecast_Seconds", page(BUFFING)),
    ("BuffCastRecastReset_Seconds", page(BUFFING)),
    ("EnableMeta", page(MISC)),
    ("BlacklistedSpellComps", page(RECHARGE | SPELL | BUFFING)),
    ("DropToPeaceModeRetryCount", page(MISC | COMBAT)),
    ("FollowAroundCorners", page(NAVIGATION)),
    ("BlacklistCorpseOpenAttemptCount", page(MISC | LOOTING)),
    ("BlacklistCorpseOpenTimeoutSeconds", page(MISC | LOOTING)),
    ("SummonPets", page(COMBAT)),
    ("PetRangeMode", page(RANGES | COMBAT)),
    ("PetCustomRange", page(RANGES | COMBAT)),
    ("PetRefillCount-Idle", page(COMBAT)),
    ("PetRefillCount-Normal", page(COMBAT)),
    ("CorpseOpenTimeoutSeconds", page(LOOTING)),
    ("PetMonsterDensity", page(COMBAT)),
    ("CorpseLootItemMaxAttempts", page(LOOTING)),
    ("FastCastBuffs", page(BUFFING)),
    ("UseBreakableTurnTo", page(SPELL)),
    ("UseProjectileAwareness", page(COMBAT)),
    ("CollisionProjectileRadius", page(COMBAT)),
    ("CollisionStepDistance", page(COMBAT)),
    ("ShowCollisionDebug", page(COMBAT)),
    ("MaximumCollisionChecksPerTick", page(COMBAT)),
    ("SpellRangeFudge", page(SPELL)),
    ("BuffWithUntrained-Item", page(BUFFING)),
    ("BuffWithUntrained-Creature", page(BUFFING)),
    ("BuffWithUntrained-Life", page(BUFFING)),
    ("AllowDebuffFallback", page(COMBAT)),
];

/// The pages a setting is shown on, if any.
#[must_use]
pub fn page_of(name: &str) -> Option<VtankOptionPage> {
    PAGE_INDEX.get(name).map(|&index| PAGES[index].1)
}

/// What each setting does, in MossTank's words.
pub const DESCRIPTIONS: [(&str, &str); 136] = [
    (
        "EnableLooting",
        "Opens corpses and picks up what the loot profile keeps.",
    ),
    ("EnableNav", "Walks the loaded navigation route."),
    ("EnableBuffing", "Keeps the character's own buffs up."),
    (
        "EnableCombat",
        "Fights the monsters the monster rules pick.",
    ),
    (
        "SpellDiffExcessThreshold-Hunt",
        "Skill points a combat spell needs above its difficulty before it is cast.",
    ),
    (
        "SpellDiffExcessThreshold-Buff",
        "Skill points a buff needs above its difficulty before it is cast.",
    ),
    (
        "ArrowheadFletchDiffExcessThreshold",
        "Fletching points needed above a recipe's difficulty before arrowheads are made.",
    ),
    (
        "Recharge-Norm-HitP",
        "Health percent under which the character heals itself while fighting.",
    ),
    (
        "Recharge-Norm-Stam",
        "Stamina percent under which the character restores it while fighting.",
    ),
    (
        "Recharge-Norm-Mana",
        "Mana percent under which the character restores it while fighting.",
    ),
    (
        "Recharge-NoTarg-HitP",
        "Health percent the character tops up to when there is nothing to fight.",
    ),
    (
        "Recharge-NoTarg-Stam",
        "Stamina percent the character tops up to when there is nothing to fight.",
    ),
    (
        "Recharge-NoTarg-Mana",
        "Mana percent the character tops up to when there is nothing to fight.",
    ),
    (
        "Recharge-Helper-HitP",
        "Health percent under which another player is healed.",
    ),
    (
        "Recharge-Helper-Stam",
        "Stamina percent under which another player is given stamina.",
    ),
    (
        "Recharge-Helper-Mana",
        "Mana percent under which another player is given mana.",
    ),
    (
        "DoHelp",
        "Heals and restores stamina and mana for fellowship members and other allies nearby.",
    ),
    (
        "AttackDistance",
        "How far away a monster may be and still be attacked.",
    ),
    (
        "AttackMinimumDistance",
        "Monsters nearer than this are not attacked.",
    ),
    (
        "ApproachDistance",
        "How near the character walks to a monster it cannot hit from where it stands.",
    ),
    (
        "RingDistance",
        "How near monsters must be for a ring spell to count them.",
    ),
    (
        "CorpseApproachRange-Max",
        "How far away a corpse may be for the character to walk to it.",
    ),
    (
        "CorpseApproachRange-Min",
        "How near the character walks to a corpse before opening it.",
    ),
    (
        "NavCloseStopRange",
        "How near a route point counts as reached.",
    ),
    (
        "NavFarStopRange",
        "How far the character may stray from the route before the route stops.",
    ),
    (
        "UsePortalDistance",
        "How near a portal must be for a route step to use it.",
    ),
    (
        "HelperDistanceHitP",
        "How near another player must be to be healed.",
    ),
    (
        "HelperDistanceStam",
        "How near another player must be to be given stamina.",
    ),
    (
        "HelperDistanceMana",
        "How near another player must be to be given mana.",
    ),
    (
        "MinimumRingTargets",
        "How many monsters must be in ring range before a ring spell is cast.",
    ),
    (
        "DefaultMeleeAttackHeight",
        "The height melee attacks aim at when nothing else decides it.",
    ),
    (
        "CastDispelSelf",
        "Casts a self dispel when a vulnerability lands on the character.",
    ),
    (
        "UseDispelItems",
        "Uses a dispelling item when a vulnerability lands on the character.",
    ),
    (
        "AutoCram",
        "Moves items out of the main pack into side packs.",
    ),
    ("AutoStack", "Merges stacks of the same item."),
    (
        "ReadUnknownScrolls",
        "Picks up and reads scrolls of spells the character does not know yet (looting must be on).",
    ),
    (
        "UseDispelDrum",
        "Uses a dispelling drum when a vulnerability lands on the character.",
    ),
    (
        "SwitchWandsToDebuff",
        "Changes to the wand the monster rule names even for a debuff, not only for an attack.",
    ),
    (
        "AutoCraftItems",
        "Makes the consumables listed in the profile from their parts when none are left.",
    ),
    (
        "UseHealersHeart",
        "Uses a healer's heart from the item list to help others.",
    ),
    (
        "JumpOutWandCasting",
        "Jumps right after using a healer's heart, so its heal goes out at once.",
    ),
    (
        "LootAllCorpses",
        "Also loots corpses killed by players outside the fellowship.",
    ),
    (
        "LootFellowCorpses",
        "Also loots corpses killed by fellowship members.",
    ),
    (
        "DoJiggle",
        "Moves a little while casting, to throw off incoming spells.",
    ),
    (
        "RandomHelperBuffs",
        "Now and then casts a buff on a nearby player.",
    ),
    (
        "RandomHelperIntervalSeconds",
        "Seconds between buffs for nearby players.",
    ),
    (
        "IdlePeaceMode",
        "Stays in peace mode while looting, walking and salvaging.",
    ),
    (
        "TargetLock",
        "Attacks the selected monster first while it is a fair target.",
    ),
    (
        "StopMacroOnDeath",
        "Turns the macro off when the character dies.",
    ),
    (
        "UseArcs",
        "Whether arc spells are cast instead of bolts when both are known at the best level.",
    ),
    (
        "ArcRange",
        "How far away a monster may be for an arc to be chosen over a bolt.",
    ),
    (
        "TargetSelectMethod",
        "How the next monster is picked: by distance, by angle, or by angle when near and distance when far.",
    ),
    (
        "TargetSelectAngleRange",
        "Beyond this distance, picking by angle and distance switches to distance.",
    ),
    (
        "IdleBuffTopoff",
        "Recasts buffs while there is nothing else to do.",
    ),
    (
        "IdleBuffTopoffTimeSeconds",
        "Seconds of a buff left under which it is recast while idle.",
    ),
    (
        "RebuffTimeRemainingSeconds",
        "Seconds of a buff left under which it is recast.",
    ),
    (
        "RefillWornMana",
        "Refills worn items with the mana charges listed in the profile.",
    ),
    (
        "RefillWornMana-Item-ManaPercent",
        "Mana percent under which a worn item is refilled.",
    ),
    (
        "BuffProfile-Prots",
        "The protection elements cast when the protection choice is Custom.",
    ),
    (
        "BuffProfile-Banes",
        "The bane elements cast when the bane choice is Custom.",
    ),
    (
        "BuffProfile_Prots",
        "Which protection spells the character casts on itself.",
    ),
    (
        "BuffProfile_Banes",
        "Which bane spells the character casts on its armour.",
    ),
    (
        "DebuffEachFirst",
        "How much debuffing comes before the attack: one monster, every monster of the same priority, or every monster.",
    ),
    (
        "AutoAttackPower",
        "Sets the melee power bar to suit the weapon.",
    ),
    (
        "LootPriorityBoost",
        "Loots corpses before attacking the next monster.",
    ),
    (
        "CorpseCacheTimeoutMinutes",
        "Minutes a remembered corpse is kept before it is forgotten.",
    ),
    (
        "CorpseItemAppearanceTimeoutSeconds",
        "Seconds to wait for a corpse's items to appear.",
    ),
    (
        "CorpseItemIDTimeoutSeconds",
        "Seconds to wait for a corpse's items to be identified.",
    ),
    (
        "DebuffSelectionMethod",
        "What a debuff's source is chosen by: the spell's level or the caster's skill. Skill suits monsters with strong magic defence.",
    ),
    (
        "ManaStoneLootCount",
        "How many usable mana stones looting keeps in the pack.",
    ),
    (
        "ManaTankMinimumMana",
        "Mana an item must hold before it is drained into a mana stone.",
    ),
    (
        "SplitPeas",
        "Splits peas into spell components for the components the profile lists.",
    ),
    (
        "SpellCompMin-Critical",
        "Components of a kind under which a pea is split before anything else, healing included.",
    ),
    (
        "SpellCompMin-Normal",
        "Components of a kind under which a pea is split, after buffing.",
    ),
    (
        "SpellCompMin-Idle",
        "Components of a kind under which a pea is split while idle.",
    ),
    (
        "RechargeBoostTimeSeconds",
        "Seconds the recharge boost lasts after changing to magic mode to recharge.",
    ),
    (
        "RechargeBoostAmount",
        "Points of health, stamina or mana the recharge boost adds to the wish to recharge.",
    ),
    (
        "UseSpecialAmmo",
        "Which special ammunition may be made and used.",
    ),
    ("OpenDoors", "Opens closed doors along the route."),
    ("DoorIDRange", "How near a door must be to be identified."),
    ("DoorOpenRange", "How near a door must be to be opened."),
    (
        "DoorLockpickDiffExcessThreshold",
        "Lockpick points needed above a door's difficulty before it is picked.",
    ),
    (
        "ManaChargesWhenOff",
        "Keeps worn items filled with mana charges and stones even while the macro is off.",
    ),
    (
        "AutoFellowManagement",
        "Recruits players who ask and handles the waiting list and votes when leading a fellowship.",
    ),
    (
        "MinimumHealKitSuccessChance",
        "The lowest chance, in percent, of a healing kit working that is still worth using it.",
    ),
    (
        "UseKitsInMagicMode",
        "Uses healing kits in magic mode too, ahead of spells.",
    ),
    (
        "StaminaToHealthMultiplier",
        "How much better than a heal a stamina-to-health spell must be to be cast.",
    ),
    (
        "ManaToHealthMultiplier",
        "How much better than a heal a mana-to-health spell must be to be cast.",
    ),
    ("NavPriorityBoost", "Walks the route before attacking."),
    (
        "DeleteGhostMonsters",
        "Removes monsters the client still shows but that are not really there.",
    ),
    (
        "GhostMonsterSpellAttemptCount",
        "Spells that must fail to start on a monster before it counts as not there.",
    ),
    (
        "BlacklistMonsterAttemptCount",
        "Attacks that must miss a monster before it is left alone for a while.",
    ),
    (
        "BlacklistMonsterTimeoutSeconds",
        "Seconds a monster that cannot be hit is left alone.",
    ),
    (
        "CombineSalvage",
        "Combines salvage bags as the loot profile's ranges say (looting must be on).",
    ),
    (
        "LootOnlyRareCorpses",
        "Loots only corpses that can hold a rare.",
    ),
    (
        WALK_TO_RARE_CORPSE,
        "Walks up to 100 metres to this character's own rare corpse and holds the route until it is looted.",
    ),
    (
        "DeleteGhostMonstersByHPTracker",
        "Also removes a monster whose health stops being reported, for every kind of attack.",
    ),
    (
        "GhostDeleteHPTrackerSeconds",
        "Seconds without a health report before a monster counts as not there.",
    ),
    (
        "GoToPeaceModeToUseKits",
        "Changes to peace mode before using a healing kit.",
    ),
    (
        "UseRecklessness",
        "Attacks with recklessness when the character has it.",
    ),
    (
        "DebuffPrecastSeconds",
        "Seconds before a debuff runs out that it is cast again (not for damage-over-time spells).",
    ),
    (
        "ClearLevelBoostFlagOnCast",
        "Lets the recharge boost count only for the first spell after changing to magic mode.",
    ),
    (
        "IdleCraftCount_HealthKits",
        "Health kits of each kind to make while idle.",
    ),
    (
        "IdleCraftCount_StamKits",
        "Stamina kits of each kind to make while idle.",
    ),
    (
        "IdleCraftCount_ManaKits",
        "Mana kits of each kind to make while idle.",
    ),
    (
        "IdleCraftCount_HealthFood",
        "Health food and potions of each kind to make while idle.",
    ),
    (
        "IdleCraftCount_StamFood",
        "Stamina food and potions of each kind to make while idle.",
    ),
    (
        "IdleCraftCount_ManaFood",
        "Mana food and potions of each kind to make while idle.",
    ),
    (
        "BuffCastRecast_Seconds",
        "After any buff is cast, every other buff is recast this many seconds earlier, for the reset time.",
    ),
    (
        "BuffCastRecastReset_Seconds",
        "Seconds the earlier recast lasts after a buff is cast.",
    ),
    ("EnableMeta", "Runs the loaded meta."),
    (
        "BlacklistedSpellComps",
        "Spell components never to cast with.",
    ),
    (
        "DropToPeaceModeRetryCount",
        "Tries at changing to peace mode to swap weapons before a wand is used instead.",
    ),
    (
        "FollowAroundCorners",
        "Follows the leader's path instead of running straight at where the leader is.",
    ),
    (
        "BlacklistCorpseOpenAttemptCount",
        "Tries at opening a corpse before it is left alone for a while.",
    ),
    (
        "BlacklistCorpseOpenTimeoutSeconds",
        "Seconds a corpse that will not open is left alone.",
    ),
    (
        "SummonPets",
        "Summons combat pets from the items the profile lists.",
    ),
    (
        "PetRangeMode",
        "Which distance counts monsters for summoning a pet: the attack distance or the pet's own.",
    ),
    (
        "PetCustomRange",
        "The pet's own distance for counting monsters.",
    ),
    (
        "PetRefillCount-Idle",
        "Charges left on a pet item under which it is refilled while idle.",
    ),
    (
        "PetRefillCount-Normal",
        "Charges left on a pet item under which it is refilled while fighting.",
    ),
    (
        "CorpseOpenTimeoutSeconds",
        "Seconds to wait for a corpse to open before trying again.",
    ),
    (
        "PetMonsterDensity",
        "Monsters that must be in pet range before a pet is summoned.",
    ),
    (
        "CorpseLootItemMaxAttempts",
        "Tries at picking up an item from a corpse before it is left behind.",
    ),
    (
        "FastCastBuffs",
        "Casts buffs quickly, which can move the character while casting.",
    ),
    (
        "UseBreakableTurnTo",
        "Lets something more urgent interrupt turning to face a spell's target.",
    ),
    (
        "UseProjectileAwareness",
        "Checks that an arc, bolt or arrow can reach its target before it is let go.",
    ),
    (
        "CollisionProjectileRadius",
        "The width of the test shape the reach check moves along the flight.",
    ),
    (
        "CollisionStepDistance",
        "How far the reach check moves its test shape at a time; keep it under twice the width.",
    ),
    ("ShowCollisionDebug", "Draws the reach check's test shape."),
    (
        "MaximumCollisionChecksPerTick",
        "The most monsters checked for reach in one pass.",
    ),
    (
        "SpellRangeFudge",
        "Meters taken off a spell's range to allow for positions not being exact.",
    ),
    (
        "BuffWithUntrained-Item",
        "Character level above which untrained Item Enchantment is not used to buff.",
    ),
    (
        "BuffWithUntrained-Creature",
        "Character level above which untrained Creature Enchantment is not used to buff.",
    ),
    (
        "BuffWithUntrained-Life",
        "Character level above which untrained Life Magic is not used to buff.",
    ),
    (
        "AllowDebuffFallback",
        "When a wall blocks a thrown debuff, uses one that does not fly instead.",
    ),
];

/// The setting's description, empty when it has none.
#[must_use]
pub fn description_of(name: &str) -> &'static str {
    DESCRIPTION_INDEX
        .get(name)
        .map_or("", |&index| DESCRIPTIONS[index].1)
}

/// The label of each value a choice setting can take, in order.
pub const CHOICE_LABELS: [(&str, i32, &str); 33] = [
    ("UseArcs", 1, "No"),
    ("UseArcs", 2, "At Range"),
    ("UseArcs", 3, "Yes"),
    ("TargetSelectMethod", 1, "By range"),
    ("TargetSelectMethod", 2, "By angle"),
    ("TargetSelectMethod", 3, "Both"),
    ("BuffProfile_Prots", 1, "Custom"),
    ("BuffProfile_Prots", 2, "All"),
    ("BuffProfile_Prots", 3, "None"),
    ("BuffProfile_Prots", 4, "B"),
    ("BuffProfile_Prots", 5, "BPS"),
    ("BuffProfile_Prots", 6, "BPSA"),
    ("BuffProfile_Prots", 7, "ALFC"),
    ("BuffProfile_Prots", 8, "BPSAC"),
    ("BuffProfile_Banes", 1, "Custom"),
    ("BuffProfile_Banes", 2, "All"),
    ("BuffProfile_Banes", 3, "None"),
    ("BuffProfile_Banes", 4, "B"),
    ("BuffProfile_Banes", 5, "BPS"),
    ("BuffProfile_Banes", 6, "BPSA"),
    ("BuffProfile_Banes", 7, "BPSAC"),
    ("BuffProfile_Banes", 8, "ALFC"),
    ("DebuffEachFirst", 1, "One"),
    ("DebuffEachFirst", 2, "Priority"),
    ("DebuffEachFirst", 3, "All"),
    ("DebuffSelectionMethod", 1, "SpellLevel"),
    ("DebuffSelectionMethod", 2, "Skill"),
    ("UseSpecialAmmo", 0, "None"),
    ("UseSpecialAmmo", 1, "Raider"),
    ("UseSpecialAmmo", 2, "SCurrency"),
    ("UseSpecialAmmo", 3, "Both"),
    ("PetRangeMode", 0, "AttackDistance"),
    ("PetRangeMode", 1, "PetCustomRange"),
];

/// The recharge plan a new profile starts with: for each vital (1 health,
/// 2 stamina, 3 mana), stance (1 magic, 2 any other) and band, which way of
/// restoring it is tried, in order.
#[must_use]
pub fn default_recharge_handlers() -> Vec<RechargeHandlerRow> {
    vec![
        RechargeHandlerRow::new(1, "Stamina to Health", 0, 15, 1),
        RechargeHandlerRow::new(1, "Mana to Health", 0, 15, 1),
        RechargeHandlerRow::new(1, "Regular Spell", 0, 15, 1),
        RechargeHandlerRow::new(1, "Recharge With Food", 0, 15, 1),
        RechargeHandlerRow::new(1, "Kit Recharge", 0, 100, 1),
        RechargeHandlerRow::new(1, "Stamina to Health", 0, 100, 1),
        RechargeHandlerRow::new(1, "Mana to Health", 0, 100, 1),
        RechargeHandlerRow::new(1, "Regular Spell", 0, 100, 1),
        RechargeHandlerRow::new(1, "Recharge With Food", 0, 100, 1),
        RechargeHandlerRow::new(2, "Kit Recharge", 0, 100, 1),
        RechargeHandlerRow::new(2, "Regular Spell", 0, 100, 1),
        RechargeHandlerRow::new(2, "Recharge With Food", 0, 100, 1),
        RechargeHandlerRow::new(3, "Kit Recharge", 0, 100, 1),
        RechargeHandlerRow::new(3, "Recharge With Food", 0, 100, 1),
        RechargeHandlerRow::new(3, "Regular Spell", 0, 100, 1),
        RechargeHandlerRow::new(1, "Recharge With Food", 0, 15, 2),
        RechargeHandlerRow::new(1, "Kit Recharge", 0, 100, 2),
        RechargeHandlerRow::new(1, "Recharge With Food", 0, 100, 2),
        RechargeHandlerRow::new(1, "Stamina to Health", 0, 10, 2),
        RechargeHandlerRow::new(1, "Regular Spell", 0, 100, 2),
        RechargeHandlerRow::new(2, "Kit Recharge", 0, 100, 2),
        RechargeHandlerRow::new(2, "Recharge With Food", 0, 100, 2),
        RechargeHandlerRow::new(2, "Regular Spell", 0, 100, 2),
        RechargeHandlerRow::new(3, "Kit Recharge", 0, 100, 2),
        RechargeHandlerRow::new(3, "Recharge With Food", 0, 100, 2),
        RechargeHandlerRow::new(3, "Regular Spell", 0, 100, 2),
    ]
}

/// The gems a new profile uses for their buff: the two blessings anyone may
/// carry.
pub const DEFAULT_GEM_FOOD_ITEMS: [(&str, u32); 2] =
    [("Asheron's Benediction", 3810), ("Blackmoor's Favor", 3811)];

#[cfg(test)]
mod index_tests {
    use super::*;

    // The indexes answer every name, in any case, as a walk of the table.
    #[test]
    fn the_name_indexes_answer_as_a_walk_does() {
        let mut names: Vec<String> = Vec::new();
        for name in COMMAND_NAMES
            .iter()
            .chain(PAGES.iter().map(|(name, _)| name))
            .chain(DESCRIPTIONS.iter().map(|(name, _)| name))
        {
            names.push((*name).to_owned());
            names.push(name.to_ascii_uppercase());
            names.push(name.to_ascii_lowercase());
        }
        names.extend(["", "nothing", "enableloot", "EnableLooting "].map(str::to_owned));
        for name in &names {
            assert_eq!(
                canonical(name),
                if is_legacy_rare_walk_name(name) {
                    Some(WALK_TO_RARE_CORPSE)
                } else {
                    COMMAND_NAMES
                        .iter()
                        .copied()
                        .find(|known| ordinal::equals_ignore_case(known, name))
                },
                "{name}"
            );
            assert_eq!(
                page_of(name),
                PAGES
                    .iter()
                    .find(|(setting, _)| ordinal::equals_ignore_case(setting, name))
                    .map(|(_, page)| *page),
                "{name}"
            );
            assert_eq!(
                description_of(name),
                DESCRIPTIONS
                    .iter()
                    .find(|(setting, _)| ordinal::equals_ignore_case(setting, name))
                    .map_or("", |(_, text)| text),
                "{name}"
            );
        }
    }
}
