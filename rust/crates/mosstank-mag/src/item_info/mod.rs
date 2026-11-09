//! The one-line description Mag-Tools prints when an item is identified:
//! a port of `ItemInfo.ToString()` (Mag-Tools `ItemInfo/ItemInfo.cs`,
//! 43ee2ec), its 29 segments in its order with its strings. Numbers are
//! printed as .NET Framework printed them, in en-US (see [`crate::fmt`]).
//!
//! Mag-Tools read the item through Decal's keys; [`Values`] answers each
//! key from where the client holds it: the appraised weapon profile for
//! damage, variance and the damage and attack modifiers, the armor profile
//! for the protections, the object's description for the material, value,
//! burden, uses and workmanship it carries, and every other key by its game
//! property id.
//!
//! One fix: an id the spell table does not know prints as `Unknown spell
//! <id>` and the rest of the line still prints (Mag-Tools threw and printed
//! nothing).

mod buffed;
pub mod names;
mod spells;

use std::fmt::Write as _;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{ItemProperties, ObjectClass};

use crate::fmt;

/// What the line is made from.
#[derive(Debug, Clone, Copy)]
pub struct ItemInfoInput<'a> {
    /// The item as the client lists it (its name, class, spells and the
    /// values its description carried).
    pub item: &'a InventoryItem,
    /// Its properties by game property id, with the appraised profiles.
    pub properties: &'a ItemProperties,
    /// The `ShowBuffedValues` setting: the buffed block of weapons and
    /// casters.
    pub show_buffed_values: bool,
    /// The `ShowValueAndBurden` setting.
    pub show_value_and_burden: bool,
}

/// Every key the line reads: Decal's key numbers (`LongValueKey`,
/// `DoubleValueKey`), which are the game property ids except for the
/// profile and description values (`0x0A0000xx`, `0x0D0000xx`). The
/// comments give the game's name where Decal's differs.
pub(crate) mod key {
    /// Material (MaterialType).
    pub const MATERIAL: u32 = 131;
    /// Weapon mastery (WeaponType).
    pub const MASTERY: u32 = 353;
    /// ArmorSet (EquipmentSetId).
    pub const EQUIPMENT_SET: u32 = 265;
    /// ArmorLevel.
    pub const ARMOR_LEVEL: u32 = 28;
    /// Imbued (ImbuedEffect).
    pub const IMBUED: u32 = 179;
    /// NumberTimesTinkered (NumTimesTinkered).
    pub const TINKS: u32 = 171;
    /// The weapon profile's maximum damage.
    pub const MAX_DAMAGE: u32 = 218_103_842;
    /// The weapon profile's variance.
    pub const VARIANCE: u32 = 167_772_171;
    /// ElementalDmgBonus (ElementalDamageBonus).
    pub const ELEMENTAL_DMG_BONUS: u32 = 204;
    /// The weapon profile's damage modifier.
    pub const DAMAGE_BONUS: u32 = 167_772_174;
    /// ElementalDamageVersusMonsters (ElementalDamageMod).
    pub const ELEMENTAL_VS_MONSTERS: u32 = 152;
    /// The weapon profile's offense modifier.
    pub const ATTACK_BONUS: u32 = 167_772_172;
    /// MeleeDefenseBonus (WeaponDefense).
    pub const MELEE_DEFENSE_BONUS: u32 = 29;
    /// MagicDBonus (WeaponMagicDefense).
    pub const MAGIC_D_BONUS: u32 = 150;
    /// MissileDBonus (WeaponMissileDefense).
    pub const MISSILE_D_BONUS: u32 = 149;
    /// ManaCBonus (ManaConversionMod).
    pub const MANA_C_BONUS: u32 = 144;
    /// WieldReqType (WieldRequirements).
    pub const WIELD_REQ_TYPE: u32 = 158;
    /// WieldReqAttribute (WieldSkillType).
    pub const WIELD_REQ_ATTRIBUTE: u32 = 159;
    /// WieldReqValue (WieldDifficulty).
    pub const WIELD_REQ_VALUE: u32 = 160;
    /// UseRequiresLevel.
    pub const USE_REQUIRES_LEVEL: u32 = 369;
    /// SkillLevelReq (ItemSkillLevelLimit).
    pub const SKILL_LEVEL_REQ: u32 = 115;
    /// ActivationReqSkillId (AppraisalItemSkill).
    pub const ACTIVATION_REQ_SKILL_ID: u32 = 176;
    /// UseRequiresSkill.
    pub const USE_REQUIRES_SKILL: u32 = 366;
    /// UseRequiresSkillLevel.
    pub const USE_REQUIRES_SKILL_LEVEL: u32 = 367;
    /// UseRequiresSkillSpec.
    pub const USE_REQUIRES_SKILL_SPEC: u32 = 368;
    /// LoreRequirement (ItemDifficulty).
    pub const LORE_REQUIREMENT: u32 = 109;
    /// The workmanship the object's description carries.
    pub const SALVAGE_WORKMANSHIP: u32 = 167_772_169;
    /// Workmanship (ItemWorkmanship).
    pub const WORKMANSHIP: u32 = 105;
    /// Unenchantable (ResistMagic).
    pub const UNENCHANTABLE: u32 = 36;
    /// The armor profile's protections (`0x0A000000`-`0x0A000006`):
    /// slashing.
    pub const SLASH_PROT: u32 = 167_772_160;
    /// Piercing.
    pub const PIERCE_PROT: u32 = 167_772_161;
    /// Bludgeoning.
    pub const BLUDGEON_PROT: u32 = 167_772_162;
    /// Acid.
    pub const ACID_PROT: u32 = 167_772_163;
    /// Lightning.
    pub const LIGHTNING_PROT: u32 = 167_772_164;
    /// Fire.
    pub const FIRE_PROT: u32 = 167_772_165;
    /// Cold.
    pub const COLD_PROT: u32 = 167_772_166;
    /// Value.
    pub const VALUE: u32 = 19;
    /// Burden (EncumbranceVal).
    pub const BURDEN: u32 = 5;
    /// Damage rating (GearDamage).
    pub const DAM_RATING: u32 = 370;
    /// Damage resistance rating (GearDamageResist).
    pub const DAM_RESIST_RATING: u32 = 371;
    /// Critical rating (GearCrit).
    pub const CRIT_RATING: u32 = 372;
    /// Critical resistance rating (GearCritResist).
    pub const CRIT_RESIST_RATING: u32 = 373;
    /// Critical damage rating (GearCritDamage).
    pub const CRIT_DAM_RATING: u32 = 374;
    /// Critical damage resistance rating (GearCritDamageResist).
    pub const CRIT_DAM_RESIST_RATING: u32 = 375;
    /// Healing boost rating (GearHealingBoost).
    pub const HEAL_BOOST_RATING: u32 = 376;
    /// Vitality rating (GearMaxHealth).
    pub const VITALITY_RATING: u32 = 379;
    /// KeysHeld (NumKeys).
    pub const KEYS_HELD: u32 = 193;
    /// UsesRemaining (Structure).
    pub const USES_REMAINING: u32 = 92;
}

/// The object description's flag for a material.
const DESCRIPTION_HAS_MATERIAL: u32 = 0x8000_0000;

/// The item's values under Decal's keys (`wo.Values(key)`, `LongKeys`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Values<'a> {
    item: &'a InventoryItem,
    properties: &'a ItemProperties,
}

impl<'a> Values<'a> {
    pub(crate) fn new(item: &'a InventoryItem, properties: &'a ItemProperties) -> Self {
        Self { item, properties }
    }

    /// The object's description carried a material: its flag says so; with
    /// no flags known, a material was kept.
    fn described_material(&self) -> bool {
        match self
            .item
            .header
            .as_ref()
            .and_then(|h| h.weenie_header_flags)
        {
            Some(flags) => flags & DESCRIPTION_HAS_MATERIAL != 0,
            None => self.item.material_type != 0,
        }
    }

    /// A long key, when the item has it.
    pub(crate) fn long_opt(&self, key: u32) -> Option<i32> {
        if key == key::MAX_DAMAGE {
            return self.properties.weapon.as_ref().map(|w| w.damage);
        }
        if let Some(v) = self.properties.int(key) {
            return Some(v);
        }
        // The values the object's description carries (none when it sent
        // zero, which reads the same as missing everywhere they are read).
        let item = self.item;
        let described = match key {
            key::MATERIAL if self.described_material() => {
                return Some(item.material_type.cast_signed());
            }
            key::VALUE => item.value,
            key::BURDEN => item.burden,
            key::USES_REMAINING => item.structure,
            _ => 0,
        };
        (described != 0).then_some(described)
    }

    /// `wo.Values(key)`: a long key, 0 when missing.
    pub(crate) fn long(&self, key: u32) -> i32 {
        self.long_opt(key).unwrap_or(0)
    }

    /// `wo.LongKeys.Contains(key)`.
    pub(crate) fn has_long(&self, key: u32) -> bool {
        self.long_opt(key).is_some()
    }

    /// A double key, when the item has it.
    pub(crate) fn double_opt(&self, key: u32) -> Option<f64> {
        let weapon = self.properties.weapon.as_ref();
        let armor = self.properties.armor.as_ref();
        match key {
            key::VARIANCE => weapon.map(|w| w.damage_variance),
            key::DAMAGE_BONUS => weapon.map(|w| w.damage_mod),
            key::ATTACK_BONUS => weapon.map(|w| w.weapon_offense),
            key::SALVAGE_WORKMANSHIP => {
                let w = f64::from(self.item.workmanship);
                (w != 0.0).then_some(w)
            }
            key::SLASH_PROT => armor.map(|a| f64::from(a.slash_mod)),
            key::PIERCE_PROT => armor.map(|a| f64::from(a.pierce_mod)),
            key::BLUDGEON_PROT => armor.map(|a| f64::from(a.bludgeon_mod)),
            key::ACID_PROT => armor.map(|a| f64::from(a.acid_mod)),
            key::LIGHTNING_PROT => armor.map(|a| f64::from(a.electric_mod)),
            key::FIRE_PROT => armor.map(|a| f64::from(a.fire_mod)),
            key::COLD_PROT => armor.map(|a| f64::from(a.cold_mod)),
            _ => self
                .properties
                .floats
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| *v),
        }
    }

    /// `wo.Values(key, default)` for a double key.
    pub(crate) fn double(&self, key: u32, default: f64) -> f64 {
        self.double_opt(key).unwrap_or(default)
    }
}

/// Append to a line (writing to a `String` cannot fail).
macro_rules! put {
    ($sb:expr, $($arg:tt)*) => {
        let _ = write!($sb, $($arg)*);
    };
}
pub(crate) use put;

/// Append `skill + " " + level`, or `"Unknown skill: " + skill + " " +
/// level`.
fn put_skill_and_level(sb: &mut String, skill: i32, level: i32) {
    match names::skill(skill) {
        Some(name) => {
            put!(sb, "{name} {level}");
        }
        None => {
            put!(sb, "Unknown skill: {skill} {level}");
        }
    }
}

/// `Math.Round(value)` joined into a string.
fn rounded(value: f64) -> String {
    fmt::g15(fmt::round(value, 0))
}

/// `Math.Round(value, 1)` joined into a string.
fn rounded_1(value: f64) -> String {
    fmt::g15(fmt::round(value, 1))
}

/// The item's name with its material first ("Bronze Acid Katar"), as its
/// info line begins; the name alone when it has no material the table names.
#[must_use]
pub fn display_name(item: &InventoryItem, properties: &ItemProperties) -> String {
    let material = Values::new(item, properties).long(key::MATERIAL);
    match names::material(material).filter(|_| material > 0) {
        Some(name) => format!("{name} {}", item.name),
        None => item.name.clone(),
    }
}

/// `ItemInfo.ToString()`: the item's info line. `spells` answers a spell
/// id from the client's spell table.
#[must_use]
pub fn item_info(
    input: &ItemInfoInput<'_>,
    spells: &mut dyn FnMut(u32) -> Option<SpellInfo>,
) -> String {
    let item = input.item;
    let v = Values::new(item, input.properties);
    let lists = spells::SpellLists::of(item);
    let class = item.object_class;
    let mut sb = String::new();

    // 1. Material.
    let material = v.long(key::MATERIAL);
    if material > 0 {
        match names::material(material) {
            Some(name) => {
                put!(sb, "{name} ");
            }
            None => {
                put!(sb, "unknown material {material} ");
            }
        }
    }

    // 2. Name.
    sb.push_str(&item.name);

    // 3. Mastery.
    let mastery = v.long(key::MASTERY);
    if mastery > 0 {
        match names::mastery(mastery) {
            Some(name) => {
                put!(sb, " ({name})");
            }
            None => {
                put!(sb, " (Unknown mastery {mastery})");
            }
        }
    }

    // 4. Equipment set.
    let set = v.long(key::EQUIPMENT_SET);
    if set != 0 {
        sb.push_str(", ");
        match names::attribute_set(set) {
            Some(name) => sb.push_str(name),
            None => {
                put!(sb, "Unknown set {set}");
            }
        }
    }

    // 5. Armor level.
    let armor_level = v.long(key::ARMOR_LEVEL);
    if armor_level > 0 {
        put!(sb, ", AL {armor_level}");
    }

    // 6. Imbues.
    let imbued = v.long(key::IMBUED);
    if imbued > 0 {
        sb.push(',');
        for (bit, text) in IMBUES {
            if imbued & bit == *bit {
                sb.push_str(text);
            }
        }
    }

    // 7. Tinks.
    let tinks = v.long(key::TINKS);
    if tinks > 0 {
        put!(sb, ", Tinks {tinks}");
    }

    // 8. Damage.
    let max_damage = v.long(key::MAX_DAMAGE);
    let variance = v.double(key::VARIANCE, 0.0);
    if max_damage != 0 && variance != 0.0 {
        let max = f64::from(max_damage);
        let low = fmt::n2(max - max * variance);
        put!(sb, ", {low}-{max_damage}");
    } else if max_damage != 0 {
        put!(sb, ", {max_damage}");
    }

    // 9. Elemental damage bonus.
    let elemental = v.long(key::ELEMENTAL_DMG_BONUS);
    if elemental != 0 {
        put!(sb, ", +{elemental}");
    }

    // 10-16. The modifiers.
    let damage_bonus = v.double(key::DAMAGE_BONUS, 1.0);
    if damage_bonus != 1.0 {
        put!(sb, ", +{}%", rounded((damage_bonus - 1.0) * 100.0));
    }
    let vs_monsters = v.double(key::ELEMENTAL_VS_MONSTERS, 1.0);
    if vs_monsters != 1.0 {
        put!(
            sb,
            ", +{}%vs. Monsters",
            rounded((vs_monsters - 1.0) * 100.0)
        );
    }
    let attack = v.double(key::ATTACK_BONUS, 1.0);
    if attack != 1.0 {
        put!(sb, ", +{}%a", rounded((attack - 1.0) * 100.0));
    }
    let melee_defense = v.double(key::MELEE_DEFENSE_BONUS, 1.0);
    if melee_defense != 1.0 {
        put!(sb, ", {}%md", rounded((melee_defense - 1.0) * 100.0));
    }
    let magic_defense = v.double(key::MAGIC_D_BONUS, 1.0);
    if magic_defense != 1.0 {
        put!(sb, ", {}%mgc.d", rounded_1((magic_defense - 1.0) * 100.0));
    }
    let missile_defense = v.double(key::MISSILE_D_BONUS, 1.0);
    if missile_defense != 1.0 {
        put!(sb, ", {}%msl.d", rounded_1((missile_defense - 1.0) * 100.0));
    }
    let mana_c = v.double(key::MANA_C_BONUS, 0.0);
    if mana_c != 0.0 {
        put!(sb, ", {}%mc", rounded(mana_c * 100.0));
    }

    // 17. The buffed block.
    if input.show_buffed_values
        && matches!(
            class,
            ObjectClass::MeleeWeapon | ObjectClass::MissileWeapon | ObjectClass::WandStaffOrb
        )
    {
        buffed::append(&mut sb, &v, &lists, class);
    }

    // 18. Spells.
    spells::append(&mut sb, &v, &lists, spells);

    // 19. Wield requirement.
    let wield_type = v.long(key::WIELD_REQ_TYPE);
    let wield_skill = v.long(key::WIELD_REQ_ATTRIBUTE);
    let wield_value = v.long(key::WIELD_REQ_VALUE);
    if wield_value > 0 {
        if wield_type == 7 && wield_skill == 1 {
            put!(sb, ", Wield Lvl {wield_value}");
        } else {
            sb.push_str(", ");
            put_skill_and_level(&mut sb, wield_skill, wield_value);
        }
    }

    // 20. Level (summoning gems).
    let level = v.long(key::USE_REQUIRES_LEVEL);
    if level > 0 {
        put!(sb, ", Lvl {level}");
    }

    // 21. Activation requirement, unless the wield requirement covers it.
    let skill_level_req = v.long(key::SKILL_LEVEL_REQ);
    let activation_skill = v.long(key::ACTIVATION_REQ_SKILL_ID);
    if skill_level_req > 0 && (wield_skill != activation_skill || wield_value < skill_level_req) {
        sb.push_str(", ");
        put_skill_and_level(&mut sb, activation_skill, skill_level_req);
        sb.push_str(" to Activate");
    }

    // 22-23. Use requirements (summoning gems).
    let use_skill = v.long(key::USE_REQUIRES_SKILL);
    let use_level = v.long(key::USE_REQUIRES_SKILL_LEVEL);
    let use_spec = v.long(key::USE_REQUIRES_SKILL_SPEC);
    if use_skill > 0 && use_level > 0 {
        sb.push_str(", ");
        put_skill_and_level(&mut sb, use_skill, use_level);
    }
    if use_spec > 0 && use_level > 0 {
        match names::skill(use_spec) {
            Some(name) => {
                put!(sb, ", Spec {name} {use_level}");
            }
            None => {
                put!(sb, ", Unknown skill spec: {use_spec} {use_level}");
            }
        }
    }

    // 24. Lore difficulty.
    let difficulty = v.long(key::LORE_REQUIREMENT);
    if difficulty > 0 {
        put!(sb, ", Diff {difficulty}");
    }

    // 25. Workmanship.
    if class == ObjectClass::Salvage {
        let work = v.double(key::SALVAGE_WORKMANSHIP, 0.0);
        if work > 0.0 {
            put!(sb, ", Work {}", fmt::n2(work));
        }
    } else {
        let craft = v.long(key::WORKMANSHIP);
        if craft > 0 && tinks != 10 {
            put!(sb, ", Craft {craft}");
        }
    }

    // 26. Protections of unenchantable armor.
    if class == ObjectClass::Armor && v.long(key::UNENCHANTABLE) != 0 {
        let protections = [
            key::SLASH_PROT,
            key::PIERCE_PROT,
            key::BLUDGEON_PROT,
            key::COLD_PROT,
            key::FIRE_PROT,
            key::ACID_PROT,
            key::LIGHTNING_PROT,
        ];
        sb.push_str(", [");
        for (i, k) in protections.into_iter().enumerate() {
            if i > 0 {
                sb.push('/');
            }
            sb.push_str(&fmt::n1(v.double(k, 0.0)));
        }
        sb.push(']');
    }

    // 27. Value and burden.
    if input.show_value_and_burden {
        let value = v.long(key::VALUE);
        if value > 0 {
            put!(sb, ", Value {}", fmt::grouped(i64::from(value)));
        }
        let burden = v.long(key::BURDEN);
        if burden > 0 {
            put!(sb, ", BU {burden}");
        }
    }

    // 28. Ratings.
    let ratings = RATINGS.map(|(id, label)| (label, v.long_opt(id).unwrap_or(-1)));
    let total = if ratings.iter().all(|(_, r)| *r == -1) {
        -1
    } else {
        ratings
            .iter()
            .fold(0_i32, |sum, (_, r)| sum.wrapping_add((*r).max(0)))
    };
    if total > 0 {
        sb.push_str(", [");
        let mut first = true;
        for (label, r) in ratings {
            if r > 0 {
                if !first {
                    sb.push_str(", ");
                }
                put!(sb, "{label} {r}");
                first = false;
            }
        }
        sb.push(']');
    }

    // 29. Keyrings.
    if class == ObjectClass::Misc && item.name.contains("Keyring") {
        let keys = v.long(key::KEYS_HELD);
        let uses = v.long(key::USES_REMAINING);
        put!(sb, ", Keys: {keys}, Uses: {uses}");
    }

    sb
}

/// The imbue bits and their text, in the order printed
/// (`ItemInfo.cs:68-81`).
const IMBUES: &[(i32, &str)] = &[
    (1, " CS"),
    (2, " CB"),
    (4, " AR"),
    (8, " SlashRend"),
    (16, " PierceRend"),
    (32, " BludgeRend"),
    (64, " AcidRend"),
    (128, " FrostRend"),
    (256, " LightRend"),
    (512, " FireRend"),
    (1024, " MeleeImbue"),
    (4096, " MagicImbue"),
    (8192, " Hematited"),
    (536_870_912, " MagicAbsorb"),
];

/// The gear ratings in the order printed, critical damage before critical
/// resistance (`ItemInfo.cs:340-347`, `MyWorldObject.cs:195-209`).
const RATINGS: [(u32, &str); 8] = [
    (key::DAM_RATING, "D"),
    (key::DAM_RESIST_RATING, "DR"),
    (key::CRIT_RATING, "C"),
    (key::CRIT_DAM_RATING, "CD"),
    (key::CRIT_RESIST_RATING, "CR"),
    (key::CRIT_DAM_RESIST_RATING, "CDR"),
    (key::HEAL_BOOST_RATING, "HB"),
    (key::VITALITY_RATING, "V"),
];
