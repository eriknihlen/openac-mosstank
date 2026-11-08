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

/// Decal's key numbers that are not game property ids (`LongValueKey`,
/// `DoubleValueKey`), and the game ids the line reads.
pub(crate) mod key {
    /// Material (MaterialType).
    pub const MATERIAL: u32 = 131;
    /// The weapon profile's maximum damage.
    pub const MAX_DAMAGE: u32 = 218_103_842;
    /// The weapon profile's variance.
    pub const VARIANCE: u32 = 167_772_171;
    /// The weapon profile's damage modifier.
    pub const DAMAGE_BONUS: u32 = 167_772_174;
    /// The weapon profile's offense modifier.
    pub const ATTACK_BONUS: u32 = 167_772_172;
    /// The workmanship the object's description carries.
    pub const SALVAGE_WORKMANSHIP: u32 = 167_772_169;
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
    /// ElementalDamageBonus.
    pub const ELEMENTAL_DMG_BONUS: u32 = 204;
    /// ElementalDamageMod (Decal: ElementalDamageVersusMonsters).
    pub const ELEMENTAL_VS_MONSTERS: u32 = 152;
    /// WeaponDefense (Decal: MeleeDefenseBonus).
    pub const MELEE_DEFENSE_BONUS: u32 = 29;
    /// ManaConversionMod (Decal: ManaCBonus).
    pub const MANA_C_BONUS: u32 = 144;
    /// ImbuedEffect.
    pub const IMBUED: u32 = 179;
    /// NumTimesTinkered.
    pub const TINKS: u32 = 171;
    /// ResistMagic (Decal: Unenchantable).
    pub const UNENCHANTABLE: u32 = 36;
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

    /// The object's description carried a material.
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
            19 => item.value,
            5 => item.burden,
            92 => item.structure,
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

/// `skill + " " + level`, or `"Unknown skill: " + skill + " " + level`.
fn skill_and_level(skill: i32, level: i32) -> String {
    match names::skill(skill) {
        Some(name) => format!("{name} {level}"),
        None => format!("Unknown skill: {skill} {level}"),
    }
}

/// `Math.Round(value)` joined into a string.
fn rounded(value: f64) -> String {
    fmt::g15(fmt::round(value, 0))
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
            Some(name) => sb.push_str(&format!("{name} ")),
            None => sb.push_str(&format!("unknown material {material} ")),
        }
    }

    // 2. Name.
    sb.push_str(&item.name);

    // 3. Mastery.
    let mastery = v.long(353);
    if mastery > 0 {
        match names::mastery(mastery) {
            Some(name) => sb.push_str(&format!(" ({name})")),
            None => sb.push_str(&format!(" (Unknown mastery {mastery})")),
        }
    }

    // 4. Equipment set.
    let set = v.long(265);
    if set != 0 {
        sb.push_str(", ");
        match names::attribute_set(set) {
            Some(name) => sb.push_str(name),
            None => sb.push_str(&format!("Unknown set {set}")),
        }
    }

    // 5. Armor level.
    let armor_level = v.long(28);
    if armor_level > 0 {
        sb.push_str(&format!(", AL {armor_level}"));
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
        sb.push_str(&format!(", Tinks {tinks}"));
    }

    // 8. Damage.
    let max_damage = v.long(key::MAX_DAMAGE);
    let variance = v.double(key::VARIANCE, 0.0);
    if max_damage != 0 && variance != 0.0 {
        let max = f64::from(max_damage);
        let low = fmt::n2(max - max * variance);
        sb.push_str(&format!(", {low}-{max_damage}"));
    } else if max_damage != 0 {
        sb.push_str(&format!(", {max_damage}"));
    }

    // 9. Elemental damage bonus.
    let elemental = v.long(key::ELEMENTAL_DMG_BONUS);
    if elemental != 0 {
        sb.push_str(&format!(", +{elemental}"));
    }

    // 10-16. The modifiers.
    let damage_bonus = v.double(key::DAMAGE_BONUS, 1.0);
    if damage_bonus != 1.0 {
        sb.push_str(&format!(", +{}%", rounded((damage_bonus - 1.0) * 100.0)));
    }
    let vs_monsters = v.double(key::ELEMENTAL_VS_MONSTERS, 1.0);
    if vs_monsters != 1.0 {
        let text = rounded((vs_monsters - 1.0) * 100.0);
        sb.push_str(&format!(", +{text}%vs. Monsters"));
    }
    let attack = v.double(key::ATTACK_BONUS, 1.0);
    if attack != 1.0 {
        sb.push_str(&format!(", +{}%a", rounded((attack - 1.0) * 100.0)));
    }
    let melee_defense = v.double(key::MELEE_DEFENSE_BONUS, 1.0);
    if melee_defense != 1.0 {
        sb.push_str(&format!(", {}%md", rounded((melee_defense - 1.0) * 100.0)));
    }
    let magic_defense = v.double(150, 1.0);
    if magic_defense != 1.0 {
        let text = fmt::g15(fmt::round((magic_defense - 1.0) * 100.0, 1));
        sb.push_str(&format!(", {text}%mgc.d"));
    }
    let missile_defense = v.double(149, 1.0);
    if missile_defense != 1.0 {
        let text = fmt::g15(fmt::round((missile_defense - 1.0) * 100.0, 1));
        sb.push_str(&format!(", {text}%msl.d"));
    }
    let mana_c = v.double(key::MANA_C_BONUS, 0.0);
    if mana_c != 0.0 {
        sb.push_str(&format!(", {}%mc", rounded(mana_c * 100.0)));
    }

    // 17. The buffed block.
    if input.show_buffed_values
        && matches!(
            class,
            ObjectClass::MeleeWeapon | ObjectClass::MissileWeapon | ObjectClass::WandStaffOrb
        )
    {
        sb.push_str(&buffed::block(&v, &lists, class));
    }

    // 18. Spells.
    spells::append(&mut sb, &v, &lists, spells);

    // 19. Wield requirement.
    let wield_type = v.long(158);
    let wield_skill = v.long(159);
    let wield_value = v.long(160);
    if wield_value > 0 {
        if wield_type == 7 && wield_skill == 1 {
            sb.push_str(&format!(", Wield Lvl {wield_value}"));
        } else {
            sb.push_str(&format!(", {}", skill_and_level(wield_skill, wield_value)));
        }
    }

    // 20. Level (summoning gems).
    let level = v.long(369);
    if level > 0 {
        sb.push_str(&format!(", Lvl {level}"));
    }

    // 21. Activation requirement, unless the wield requirement covers it.
    let skill_level_req = v.long(115);
    let activation_skill = v.long(176);
    if skill_level_req > 0 && (wield_skill != activation_skill || wield_value < skill_level_req) {
        let text = skill_and_level(activation_skill, skill_level_req);
        sb.push_str(&format!(", {text} to Activate"));
    }

    // 22-23. Use requirements (summoning gems).
    let use_skill = v.long(366);
    let use_level = v.long(367);
    let use_spec = v.long(368);
    if use_skill > 0 && use_level > 0 {
        sb.push_str(&format!(", {}", skill_and_level(use_skill, use_level)));
    }
    if use_spec > 0 && use_level > 0 {
        match names::skill(use_spec) {
            Some(name) => sb.push_str(&format!(", Spec {name} {use_level}")),
            None => sb.push_str(&format!(", Unknown skill spec: {use_spec} {use_level}")),
        }
    }

    // 24. Lore difficulty.
    let difficulty = v.long(109);
    if difficulty > 0 {
        sb.push_str(&format!(", Diff {difficulty}"));
    }

    // 25. Workmanship.
    if class == ObjectClass::Salvage {
        let work = v.double(key::SALVAGE_WORKMANSHIP, 0.0);
        if work > 0.0 {
            sb.push_str(&format!(", Work {}", fmt::n2(work)));
        }
    } else {
        let craft = v.long(105);
        if craft > 0 && tinks != 10 {
            sb.push_str(&format!(", Craft {craft}"));
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
        ]
        .map(|k| fmt::n1(v.double(k, 0.0)));
        sb.push_str(&format!(", [{}]", protections.join("/")));
    }

    // 27. Value and burden.
    if input.show_value_and_burden {
        let value = v.long(19);
        if value > 0 {
            sb.push_str(&format!(", Value {}", fmt::grouped(i64::from(value))));
        }
        let burden = v.long(5);
        if burden > 0 {
            sb.push_str(&format!(", BU {burden}"));
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
        let parts: Vec<String> = ratings
            .iter()
            .filter(|(_, r)| *r > 0)
            .map(|(label, r)| format!("{label} {r}"))
            .collect();
        sb.push_str(&format!(", [{}]", parts.join(", ")));
    }

    // 29. Keyrings.
    if class == ObjectClass::Misc && item.name.contains("Keyring") {
        let keys = v.long(193);
        let uses = v.long(92);
        sb.push_str(&format!(", Keys: {keys}, Uses: {uses}"));
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
    (370, "D"),
    (371, "DR"),
    (372, "C"),
    (374, "CD"),
    (373, "CR"),
    (375, "CDR"),
    (376, "HB"),
    (379, "V"),
];
