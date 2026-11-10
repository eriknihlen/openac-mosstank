//! UtilityBelt's ItemInfo (`Tools/ItemInfo.cs`, its description classes
//! after Mag-Tools' `ItemInfoOnIdent`): selecting an object writes one chat
//! line with its name, the loot rule the looter's own decision on it names
//! (none without a loot profile in force), and what it is made of: armor level, imbues, damage, the spells
//! worth knowing about, what it takes to wield, workmanship, ratings.
//!
//! ```text
//! [UB] ItemInfo: <Tell:IIDString:...:select|id>Keep armor - Silver Breastplate</Tell>, AL 512, Tinks 2, ...
//! ```
//!
//! The name is a link that selects the object again. An object the client
//! holds no appraisal for is appraised first and described when the answer
//! comes (or, failing an answer, with what is known).
//!
//! The rows are `ItemInfo.Enabled` and `ItemInfo.DescribeOnSelect` (both off
//! as the original ships them), `ShowValueAndBurden`, `ShowDoorInfo` and
//! `ShowNpcInfo`. The client says that the selection changed, not which
//! mouse button changed it, so the left- and right-click switches are kept
//! for macros only; so are the describe-on-loot switch, the buffed-value, damage and retail-comparison
//! switches, whose weapon tables are not converted.

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use mosstank_loot::requirements::{item_double, item_int};
use openac_plugin_compat::number::{self, MidpointRounding};

use crate::chat;
use crate::commands::UbCall;

/// The tool's name, as its lines and settings carry it.
pub const TOOL: &str = "ItemInfo";
/// The tool's switch.
pub const ENABLED_SETTING: &str = "ItemInfo.Enabled";
/// Describe what is selected.
pub const DESCRIBE_ON_SELECT_SETTING: &str = "ItemInfo.DescribeOnSelect";
/// Add the value and the burden.
pub const SHOW_VALUE_AND_BURDEN_SETTING: &str = "ItemInfo.ShowValueAndBurden";
/// Describe doors too.
pub const SHOW_DOOR_INFO_SETTING: &str = "ItemInfo.ShowDoorInfo";
/// Describe non-player characters too.
pub const SHOW_NPC_INFO_SETTING: &str = "ItemInfo.ShowNpcInfo";

/// The id the reference puts in its chat links (`Util.GetChatId`).
const CHAT_LINK_ID: i32 = mosstank_vendor::controller::REFERENCE_CHAT_LINK_ID;

/// How long an appraisal is waited for before the object is described with
/// what is known.
pub const APPRAISAL_TIMEOUT_SECONDS: f64 = 5.0;

/// VTank's own whole-number keys start here (`0x0D000000`).
const VTANK_INT: u32 = 218_103_808;
/// VTank's own decimal keys start here (`0x0A000000`).
const VTANK_DOUBLE: u32 = 167_772_160;

// Decal's `LongValueKey`s the description reads.
const BURDEN: u32 = 5;
const VALUE: u32 = 19;
const ARMOR_LEVEL: u32 = 28;
const UNENCHANTABLE: u32 = 36;
const DAMAGE_TYPE: u32 = 45;
const ATTACK_TYPE: u32 = 47;
const USES_REMAINING: u32 = 92;
const WORKMANSHIP: u32 = 105;
const LORE_REQUIREMENT: u32 = 109;
const SKILL_LEVEL_REQ: u32 = 115;
const MATERIAL: u32 = 131;
const WIELD_REQ_TYPE: u32 = 158;
const WIELD_REQ_ATTRIBUTE: u32 = 159;
const WIELD_REQ_VALUE: u32 = 160;
const SLAYER: u32 = 166;
const NUMBER_TIMES_TINKERED: u32 = 171;
const ACTIVATION_REQ_SKILL_ID: u32 = 176;
const IMBUED: u32 = 179;
const KEYS_HELD: u32 = 193;
const ELEMENTAL_DMG_BONUS: u32 = 204;
const EQUIPMENT_SET: u32 = 265;
const EQUIPMENT_SET_LEVEL: u32 = 319;
const WEAPON_MASTERY: u32 = 353;
const SUMMONING_SKILL: u32 = 366;
const SUMMONING_SKILL_LEVEL: u32 = 367;
const SUMMONING_SPEC_SKILL: u32 = 368;
const SUMMONING_LEVEL: u32 = 369;
const EQUIPABLE_SLOTS: u32 = VTANK_INT + 14;
const MAX_DAMAGE: u32 = VTANK_INT + 34;

// Decal's `DoubleValueKey`s.
const MELEE_DEFENSE_BONUS: u32 = 29;
const CRUSHING_BLOW: u32 = 136;
const MANA_C_BONUS: u32 = 144;
const BITING_STRIKE: u32 = 147;
const MISSILE_D_BONUS: u32 = 149;
const MAGIC_D_BONUS: u32 = 150;
const ELEMENTAL_DAMAGE_VERSUS_MONSTERS: u32 = 152;
const ARMOR_CLEAVING: u32 = 155;
const SALVAGE_WORKMANSHIP: u32 = VTANK_DOUBLE + 9;
const VARIANCE: u32 = VTANK_DOUBLE + 11;
const ATTACK_BONUS: u32 = VTANK_DOUBLE + 12;
const DAMAGE_BONUS: u32 = VTANK_DOUBLE + 14;

/// The inscription (`StringValueKey.Inscription`).
const INSCRIPTION: u32 = 7;

/// The cloak slot (`EquipMask.Cloak`).
const CLOAK_SLOT: i32 = 134_217_728;
/// The clothing item type.
const CLOTHING_TYPE: u32 = 4;

/// A selection waiting on its appraisal.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    object_id: u32,
    asked: bool,
    waited: f64,
}

/// The tool: the one selection it is waiting to describe.
#[derive(Debug, Default)]
pub struct ItemInfoController {
    pending: Option<Pending>,
}

impl ItemInfoController {
    /// Nothing waiting.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The object waiting on its appraisal, if any.
    #[must_use]
    pub fn pending(&self) -> Option<u32> {
        self.pending.map(|pending| pending.object_id)
    }

    /// Forgets the object waiting (a session ended).
    pub fn reset(&mut self) {
        self.pending = None;
    }

    /// The selection changed (`Core_ItemSelected`): describes the object
    /// selected now, or asks for its appraisal and describes it when that
    /// comes. A new selection replaces the one waiting.
    pub fn on_selection_changed(&mut self, call: &mut UbCall<'_>, selected: Option<u32>) {
        self.pending = None;
        let Some(object_id) = selected.filter(|id| *id != 0) else {
            return;
        };
        if !call.flag(ENABLED_SETTING) || !call.flag(DESCRIBE_ON_SELECT_SETTING) {
            return;
        }
        let Some(object) = call.host.object(object_id) else {
            return;
        };
        if object.object_class == ObjectClass::Door && !call.flag(SHOW_DOOR_INFO_SETTING) {
            return;
        }
        if object.object_class == ObjectClass::Npc && !call.flag(SHOW_NPC_INFO_SETTING) {
            return;
        }
        if object.has_appraisal_data {
            display(call, &object);
            return;
        }
        let mut pending = Pending {
            object_id,
            asked: false,
            waited: 0.0,
        };
        ask(call, &mut pending);
        self.pending = Some(pending);
    }

    /// One frame: the object waiting is described once its appraisal is in,
    /// or once it has waited [`APPRAISAL_TIMEOUT_SECONDS`] or the client gave
    /// up on it.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        let Some(mut pending) = self.pending else {
            return;
        };
        let Some(object) = call.host.object(pending.object_id) else {
            self.pending = None;
            return;
        };
        pending.waited += elapsed_seconds;
        let abandoned =
            pending.asked && call.host.appraisal().last_abandoned_id == object.object_id;
        if object.has_appraisal_data || abandoned || pending.waited >= APPRAISAL_TIMEOUT_SECONDS {
            self.pending = None;
            display(call, &object);
            return;
        }
        if !pending.asked {
            ask(call, &mut pending);
        }
        self.pending = Some(pending);
    }
}

/// Asks for the object's appraisal unless the looter has the appraisal slot:
/// the answer goes to whoever asked last, and the looter's comes first.
fn ask(call: &mut UbCall<'_>, pending: &mut Pending) {
    if call.services.corpse_identifier_busy() {
        return;
    }
    let awaiting = call.host.appraisal().awaiting_id;
    if awaiting == pending.object_id {
        pending.asked = true;
        return;
    }
    if awaiting != 0 {
        return;
    }
    let result = call.host.identify(pending.object_id);
    // Busy is the client's backpressure: the next frame asks again.
    if result.status != ItemStatus::Busy {
        pending.asked = true;
    }
}

/// Writes the description (`DisplayItem`).
fn display(call: &mut UbCall<'_>, object: &WorldObject) {
    let line = describe(call, object);
    call.post_ub(&chat::tool(TOOL, &line));
}

/// The line after `ItemInfo: `: the name as a link (after the rule's name
/// when a rule matches), then the description.
#[must_use]
pub fn describe(call: &mut UbCall<'_>, object: &WorldObject) -> String {
    let properties = call.host.properties(object.object_id).unwrap_or_default();
    let item = call
        .host
        .owned_items()
        .into_iter()
        .find(|item| item.object_id == object.object_id)
        .or_else(|| {
            call.host
                .open_container_items()
                .into_iter()
                .find(|item| item.object_id == object.object_id)
        })
        .unwrap_or_else(|| inventory_item(object, &properties));
    // The rule the looter's decision names, whatever it says to do with the
    // item (VTank's classification names its rule either way); the item
    // alone when no rule is named.
    let rule_name = call.services.loot_rule_name(&item, &properties);
    let name = object_name(&item, &properties);
    let label = if rule_name.is_empty() {
        name
    } else {
        format!("{rule_name} - {name}")
    };
    #[allow(clippy::cast_possible_wrap)]
    let signed = object.object_id as i32;
    let show_value_and_burden = call.flag(SHOW_VALUE_AND_BURDEN_SETTING);
    let mut spell = |spell_id: u32| {
        call.host.spell(spell_id).map(|info| SpellFacts {
            name: info.name,
            family: info.family,
            difficulty: info.difficulty,
        })
    };
    let description = Description {
        object,
        item: &item,
        properties: &properties,
        show_value_and_burden,
    }
    .text(&mut spell);
    format!("<Tell:IIDString:{CHAT_LINK_ID}:select|{signed}>{label}</Tell>{description} ")
}

/// What an object the character neither carries nor sees in an open
/// container reads as to the loot rules: what the object and its appraisal
/// say.
fn inventory_item(object: &WorldObject, properties: &ItemProperties) -> InventoryItem {
    let weapon = properties.weapon.clone().unwrap_or_default();
    InventoryItem {
        object_id: object.object_id,
        weenie_class_id: object.weenie_class_id,
        name: object.name.clone(),
        item_type: object.item_type,
        container_id: object.container_id,
        wielder_id: object.wielder_id,
        stack_size: object.stack_size,
        icon_id: object.icon_id,
        coverage_mask: object.coverage_mask,
        items_capacity: object.items_capacity,
        containers_capacity: object.containers_capacity,
        appraised_spell_ids: object.spell_ids.clone(),
        material_type: properties
            .int(MATERIAL)
            .and_then(|id| u32::try_from(id).ok())
            .unwrap_or(0),
        damage: weapon.damage,
        damage_variance: weapon.damage_variance,
        damage_type: weapon.damage_type,
        ..InventoryItem::default()
    }
}

/// The name the reference shows (`Util.GetObjectName`): the material in
/// front, unless the name already carries it (a gem's material is its
/// name).
fn object_name(item: &InventoryItem, properties: &ItemProperties) -> String {
    let name = item.name.trim();
    let material = i32::try_from(item.material_type)
        .ok()
        .filter(|id| *id > 0)
        .or_else(|| properties.int(MATERIAL).filter(|id| *id > 0));
    match material.and_then(mosstank_data::material::material_name) {
        Some(material) if material != name && !name.contains(material) => {
            format!("{} {name}", material.trim())
        }
        _ => name.to_owned(),
    }
}

/// What the description reads of a spell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellFacts {
    /// Its name.
    pub name: String,
    /// Its family.
    pub family: u32,
    /// Its power (300 for a level 7 spell).
    pub difficulty: i32,
}

/// The description of one object (`ItemDescriptions.ToString`).
#[derive(Debug)]
pub struct Description<'a> {
    /// The object.
    pub object: &'a WorldObject,
    /// The object as the loot rules read it.
    pub item: &'a InventoryItem,
    /// Its appraisal.
    pub properties: &'a ItemProperties,
    /// Add the value and the burden.
    pub show_value_and_burden: bool,
}

impl Description<'_> {
    fn int(&self, key: u32) -> i32 {
        match key {
            // The armor profile carries it when the list does not.
            ARMOR_LEVEL => item_int(key, self.item, self.properties)
                .or_else(|| self.properties.armor.as_ref().map(|a| a.armor_level))
                .unwrap_or(0),
            MAX_DAMAGE => item_int(key, self.item, self.properties)
                .filter(|damage| *damage != 0)
                .or_else(|| self.properties.weapon.as_ref().map(|w| w.damage))
                .unwrap_or(0),
            _ => item_int(key, self.item, self.properties).unwrap_or(0),
        }
    }

    fn has_int(&self, key: u32) -> bool {
        self.properties.int(key).is_some()
    }

    fn double(&self, key: u32, default: f64) -> f64 {
        item_double(key, self.item, self.properties).unwrap_or(default)
    }

    fn class(&self) -> ObjectClass {
        self.object.object_class
    }

    fn spells(&self) -> &[u32] {
        if self.object.spell_ids.is_empty() {
            &self.item.appraised_spell_ids
        } else {
            &self.object.spell_ids
        }
    }

    /// The description, each part after a comma, as the reference writes
    /// it.
    pub fn text(&self, spell: &mut dyn FnMut(u32) -> Option<SpellFacts>) -> String {
        let mut sb = String::new();
        let class = self.class();
        if matches!(
            class,
            ObjectClass::MissileWeapon | ObjectClass::MeleeWeapon | ObjectClass::WandStaffOrb
        ) {
            self.append_damage_type_and_mastery(&mut sb);
        }

        let set = self.int(EQUIPMENT_SET);
        if set != 0 {
            sb.push_str(", ");
            if let Some(name) = lookup(&ATTRIBUTE_SETS, set) {
                sb.push_str(name);
                let level = self.int(EQUIPMENT_SET_LEVEL);
                if level > 0 {
                    sb.push_str(&format!(" (Lvl {level})"));
                }
            } else {
                sb.push_str(&format!("Unknown set {set}"));
            }
        }

        if class == ObjectClass::Clothing
            && self.object.item_type == CLOTHING_TYPE
            && self.int(EQUIPABLE_SLOTS) == CLOAK_SLOT
        {
            match self.spells().first().map(|id| spell(*id)) {
                Some(Some(facts)) => sb.push_str(&format!(", {}", facts.name)),
                Some(None) => {}
                None => sb.push_str(", -200 Damage Chance"),
            }
        }

        let armor_level = self.int(ARMOR_LEVEL);
        if armor_level > 0 {
            sb.push_str(&format!(", AL {armor_level}"));
        }

        let imbued = self.int(IMBUED);
        if imbued > 0 {
            sb.push(',');
            for (bit, name) in IMBUES {
                if imbued & bit == bit {
                    sb.push(' ');
                    sb.push_str(name);
                }
            }
        }

        if self.double(ARMOR_CLEAVING, 0.0) != 0.0 {
            sb.push_str(", AC");
        }
        let crushing = self.double(CRUSHING_BLOW, 0.0);
        if crushing >= 1.0 {
            sb.push_str(&format!(", CrushB ({})", text(crushing)));
        }
        let biting = self.double(BITING_STRIKE, 0.0);
        if biting >= 1.0 {
            sb.push_str(&format!(", BS ({})", text(biting)));
        }

        let slayer = self.int(SLAYER);
        if slayer >= 1
            && let Some(name) = lookup(&SLAYERS, slayer)
        {
            sb.push_str(&format!(", {name} Slayer"));
        }

        let attack_type = self.int(ATTACK_TYPE);
        if attack_type != 0 {
            if attack_type == 4 && self.int(WEAPON_MASTERY) == 11 {
                sb.push_str(", Cleaving");
            } else if attack_type == 160 || attack_type == 166 {
                sb.push_str(", Multi-Strike");
            } else if attack_type == 486 {
                sb.push_str(", Multi-Strike (3)");
            }
        }

        let tinks = self.int(NUMBER_TIMES_TINKERED);
        if tinks > 0 {
            sb.push_str(&format!(", Tinks {tinks}"));
        }

        let variance = self.double(VARIANCE, 0.0);
        if class != ObjectClass::MissileWeapon {
            let max_damage = self.int(MAX_DAMAGE);
            if max_damage != 0 && variance != 0.0 {
                let max = f64::from(max_damage);
                sb.push_str(&format!(
                    ", {}-{max_damage}",
                    fixed(max - max * variance, "N2")
                ));
            } else if max_damage != 0 {
                sb.push_str(&format!(", {max_damage}"));
            }
        }

        if variance > 0.0 {
            sb.push_str(&format!(", {}v", text(round_to(variance, 2))));
        }

        let elemental = self.int(ELEMENTAL_DMG_BONUS);
        if elemental != 0 {
            sb.push_str(&format!(", +{elemental}"));
        }

        let damage_bonus = self.double(DAMAGE_BONUS, 1.0);
        if damage_bonus != 1.0 {
            sb.push_str(&format!(", +{}%", percent(damage_bonus)));
        }

        let versus_monsters = self.double(ELEMENTAL_DAMAGE_VERSUS_MONSTERS, 1.0);
        if versus_monsters != 1.0 {
            sb.push_str(&format!(", +{}% vs. Monsters", percent(versus_monsters)));
        }

        let attack = self.double(ATTACK_BONUS, 1.0);
        if attack != 1.0 {
            sb.push_str(&format!(", {}%a", percent(attack)));
            let buffed = self.buffed(ATTACK_BONUS, attack);
            if buffed != attack {
                sb.push_str(&format!(" ({})", percent(buffed)));
            }
        }

        let melee_defense = self.double(MELEE_DEFENSE_BONUS, 1.0);
        if melee_defense != 1.0 {
            sb.push_str(&format!(", {}%md", percent(melee_defense)));
            let buffed = self.buffed(MELEE_DEFENSE_BONUS, melee_defense);
            if buffed != melee_defense {
                sb.push_str(&format!(" ({})", percent(buffed)));
            }
        }

        let magic_defense = self.double(MAGIC_D_BONUS, 1.0);
        if magic_defense != 1.0 {
            sb.push_str(&format!(
                ", {}%mgc.d",
                text(round_to((magic_defense - 1.0) * 100.0, 1))
            ));
        }
        let missile_defense = self.double(MISSILE_D_BONUS, 1.0);
        if missile_defense != 1.0 {
            sb.push_str(&format!(
                ", {}%msl.d",
                text(round_to((missile_defense - 1.0) * 100.0, 1))
            ));
        }
        let mana_conversion = self.double(MANA_C_BONUS, 0.0);
        if mana_conversion != 0.0 {
            sb.push_str(&format!(
                ", {}%mc",
                text(number::round(mana_conversion * 100.0))
            ));
        }

        self.append_spells(&mut sb, spell);
        self.append_requirements(&mut sb);

        if class == ObjectClass::Salvage {
            let work = self.double(SALVAGE_WORKMANSHIP, 0.0);
            if work > 0.0 {
                sb.push_str(&format!(", Work {}", fixed(work, "N2")));
            }
        } else {
            let craft = self.int(WORKMANSHIP);
            if craft > 0 && tinks != 10 {
                sb.push_str(&format!(", Craft {craft}"));
            }
        }

        if class == ObjectClass::Armor
            && self.int(UNENCHANTABLE) != 0
            && let Some(armor) = &self.properties.armor
        {
            let protections = [
                armor.slash_mod,
                armor.pierce_mod,
                armor.bludgeon_mod,
                armor.cold_mod,
                armor.fire_mod,
                armor.acid_mod,
                armor.electric_mod,
            ]
            .map(|value| fixed(f64::from(value), "N1"));
            sb.push_str(&format!(", [{}]", protections.join("/")));
        }

        if self.show_value_and_burden {
            let value = self.int(VALUE);
            if value > 0 {
                let value = number::format_i32(value, "N0").unwrap_or_else(|_| value.to_string());
                sb.push_str(&format!(", Value {value}"));
            }
            let burden = self.int(BURDEN);
            if burden > 0 {
                sb.push_str(&format!(", BU {burden}"));
            }
        }

        self.append_ratings(&mut sb);

        if class == ObjectClass::Misc && self.object.name.contains("Keyring") {
            sb.push_str(&format!(
                ", Keys: {}, Uses: {}",
                self.int(KEYS_HELD),
                self.int(USES_REMAINING)
            ));
        }

        if let Some((_, inscription)) = self
            .properties
            .strings
            .iter()
            .find(|(key, value)| *key == INSCRIPTION && !value.is_empty())
        {
            sb.push_str(&format!(", Inscription: {inscription}"));
        }
        sb
    }

    /// ` (Slashing Sword)` after a weapon's name
    /// (`AppendWeaponDamageTypeAndMastery`).
    fn append_damage_type_and_mastery(&self, sb: &mut String) {
        let damage = self.int(DAMAGE_TYPE);
        let mastery = self.int(WEAPON_MASTERY);
        if damage <= 0 && mastery <= 0 {
            return;
        }
        let damage = if damage > 0 {
            lookup(&DAMAGE_TYPES, damage).map_or_else(|| damage.to_string(), str::to_owned)
        } else {
            String::new()
        };
        let mastery = if mastery > 0 {
            lookup(&MASTERIES, mastery)
                .map_or_else(|| format!("Unknown mastery {mastery}"), str::to_owned)
        } else {
            String::new()
        };
        sb.push_str(&format!(" ({})", format!("{damage} {mastery}").trim()));
    }

    /// The spells worth naming, highest id first: on loot-generated gear
    /// the cantrips and the level 7 and 8 weapon buffs, not the ordinary
    /// spells every piece carries.
    fn append_spells(&self, sb: &mut String, spell: &mut dyn FnMut(u32) -> Option<SpellFacts>) {
        let mut spells = self.spells().to_vec();
        spells.sort_unstable();
        spells.reverse();
        let loot_generated = self.has_int(MATERIAL) || self.item.material_type != 0;
        let unenchantable = self.int(UNENCHANTABLE) != 0;
        for spell_id in spells {
            let Some(facts) = spell(spell_id) else {
                continue;
            };
            if shows_spell(&facts, loot_generated, unenchantable) {
                sb.push_str(&format!(", {}", facts.name));
            }
        }
    }

    /// What it takes to wield and to activate, and the summoning gem's
    /// levels and skills, and the arcane lore.
    fn append_requirements(&self, sb: &mut String) {
        let wield_value = self.int(WIELD_REQ_VALUE);
        let wield_attribute = self.int(WIELD_REQ_ATTRIBUTE);
        if wield_value > 0 {
            if self.int(WIELD_REQ_TYPE) == 7 && wield_attribute == 1 {
                sb.push_str(&format!(", Wield Lvl {wield_value}"));
            } else {
                sb.push_str(&skill_text(wield_attribute, wield_value, ""));
            }
        }

        let summoning_level = self.int(SUMMONING_LEVEL);
        if summoning_level > 0 {
            sb.push_str(&format!(", Lvl {summoning_level}"));
        }

        let activation = self.int(SKILL_LEVEL_REQ);
        let activation_skill = self.int(ACTIVATION_REQ_SKILL_ID);
        if activation > 0 && (wield_attribute != activation_skill || wield_value < activation) {
            sb.push_str(&skill_text(activation_skill, activation, " to Activate"));
        }

        let summoning_skill_level = self.int(SUMMONING_SKILL_LEVEL);
        let summoning_skill = self.int(SUMMONING_SKILL);
        if summoning_skill > 0 && summoning_skill_level > 0 {
            sb.push_str(&skill_text(summoning_skill, summoning_skill_level, ""));
        }
        let spec_skill = self.int(SUMMONING_SPEC_SKILL);
        if spec_skill > 0 && summoning_skill_level > 0 {
            match lookup(&SKILLS, spec_skill) {
                Some(name) => sb.push_str(&format!(", Spec {name} {summoning_skill_level}")),
                None => sb.push_str(&format!(
                    ", Unknown skill spec: {spec_skill} {summoning_skill_level}"
                )),
            }
        }

        let lore = self.int(LORE_REQUIREMENT);
        if lore > 0 {
            sb.push_str(&format!(", Diff {lore}"));
        }
    }

    /// The ratings: `[D 2, CD 1]`.
    fn append_ratings(&self, sb: &mut String) {
        let keys: &[(u32, &str)] = if self.class() == ObjectClass::Player {
            &[
                (307, "D"),
                (308, "DR"),
                (314, "CD"),
                (316, "CDR"),
                (323, "HB"),
                (379, "V"),
                (381, "PKD"),
                (382, "PKDR"),
            ]
        } else {
            &[
                (370, "D"),
                (371, "DR"),
                (372, "C"),
                (374, "CD"),
                (373, "CR"),
                (375, "CDR"),
                (376, "HB"),
                (379, "V"),
                (383, "PKD"),
                (384, "PKDR"),
            ]
        };
        let ratings: Vec<String> = keys
            .iter()
            .filter_map(|(key, label)| {
                let value = self.properties.int(*key).unwrap_or(0);
                (value > 0).then(|| format!("{label} {value}"))
            })
            .collect();
        if !ratings.is_empty() {
            sb.push_str(&format!(", [{}]", ratings.join(", ")));
        }
    }

    /// A value with the item's cantrips added and its active enchantments
    /// taken off (`GetBuffedDoubleValueKey`).
    fn buffed(&self, key: u32, base: f64) -> f64 {
        let mut value = base;
        for spell_id in &self.object.active_spell_ids {
            if let Some(&(_, _, change, _)) = DOUBLE_SPELL_EFFECTS
                .iter()
                .find(|(spell, effect_key, ..)| spell == spell_id && *effect_key == key)
            {
                if (change - 1.0).abs() < f64::EPSILON {
                    value /= change;
                } else {
                    value -= change;
                }
            }
        }
        for spell_id in self.spells() {
            if let Some(&(_, _, change, bonus)) = DOUBLE_SPELL_EFFECTS
                .iter()
                .find(|(spell, effect_key, ..)| spell == spell_id && *effect_key == key)
                && bonus.abs() > f64::EPSILON
            {
                if (change - 1.0).abs() < f64::EPSILON {
                    value *= bonus;
                } else {
                    value += bonus;
                }
            }
        }
        value
    }
}

/// Whether a spell on an item is named in its description.
fn shows_spell(facts: &SpellFacts, loot_generated: bool, unenchantable: bool) -> bool {
    let name = facts.name.as_str();
    // An item that is not loot generated shows every spell.
    if !loot_generated {
        return true;
    }
    if [
        "Minor Impenetrability",
        "Major Impenetrability",
        "Epic Impenetrability",
        "Legendary Impenetrability",
    ]
    .iter()
    .any(|impen| name.contains(impen))
    {
        return true;
    }
    // Trinket spells.
    if name.contains("Augmented") {
        return true;
    }
    let bane = name.contains(" Bane") || name.contains("Impen") || name.starts_with("Brogard");
    if bane {
        // Banes and impen show on unenchantable gear only.
        return unenchantable;
    }
    let weapon_buff =
        (152..=158).contains(&facts.family) || facts.family == 195 || facts.family == 325;
    if weapon_buff {
        // Level 7 and 8 and up; not 6 or below.
        return facts.difficulty == 300 || facts.difficulty >= 400;
    }
    let ordinary = [" I", " II", " III", " IV", " V", " VI"]
        .iter()
        .any(|level| name.ends_with(level));
    !(ordinary || facts.difficulty == 300 || name.contains("Incantation"))
}

/// `, Melee Defense 300` and the like, or the unknown skill's number.
fn skill_text(skill: i32, value: i32, after: &str) -> String {
    match lookup(&SKILLS, skill) {
        Some(name) => format!(", {name} {value}{after}"),
        None => format!(", Unknown skill: {skill} {value}{after}"),
    }
}

/// `Math.Round((value - 1) * 100)`, as the reference prints a bonus.
fn percent(value: f64) -> String {
    text(number::round((value - 1.0) * 100.0))
}

/// `Math.Round(value, digits)`, halves to even.
fn round_to(value: f64, digits: u32) -> f64 {
    number::round_digits(value, digits, MidpointRounding::ToEven)
}

/// `double.ToString()`.
fn text(value: f64) -> String {
    number::to_string_f64(value)
}

/// `value.ToString(format)`.
fn fixed(value: f64, format: &str) -> String {
    number::format_f64(value, format).unwrap_or_else(|_| text(value))
}

fn lookup(table: &[(i32, &'static str)], key: i32) -> Option<&'static str> {
    table
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, name)| *name)
}

/// The imbue bits and their short names.
const IMBUES: [(i32, &str); 14] = [
    (1, "CS"),
    (2, "CB"),
    (4, "AR"),
    (8, "SlashRend"),
    (16, "PierceRend"),
    (32, "BludgeRend"),
    (64, "AcidRend"),
    (128, "FrostRend"),
    (256, "LightRend"),
    (512, "FireRend"),
    (1024, "MeleeImbue"),
    (4096, "MagicImbue"),
    (8192, "Hematited"),
    (536_870_912, "MagicAbsorb"),
];

/// The damage types by their bits (`DamageTypes`).
const DAMAGE_TYPES: [(i32, &str); 10] = [
    (0x00, "Normal"),
    (0x01, "Slashing"),
    (0x02, "Piercing"),
    (0x03, "SlashPierce"),
    (0x04, "Bludgeoning"),
    (0x08, "Cold"),
    (0x10, "Fire"),
    (0x20, "Acid"),
    (0x40, "Electric"),
    (0x400, "Nether"),
];

/// Spells that change a decimal key: (spell, key, change, bonus), the
/// attack and melee defense rows of `DoubleValueKeySpellEffects`.
const DOUBLE_SPELL_EFFECTS: [(u32, u32, f64, f64); 16] = [
    (1592, ATTACK_BONUS, 0.15, 0.0),
    (2106, ATTACK_BONUS, 0.17, 0.0),
    (4405, ATTACK_BONUS, 0.20, 0.0),
    (2603, ATTACK_BONUS, 0.03, 0.03),
    (2591, ATTACK_BONUS, 0.05, 0.05),
    (4666, ATTACK_BONUS, 0.07, 0.07),
    (6094, ATTACK_BONUS, 0.09, 0.09),
    (1605, MELEE_DEFENSE_BONUS, 0.15, 0.0),
    (2101, MELEE_DEFENSE_BONUS, 0.17, 0.0),
    (4400, MELEE_DEFENSE_BONUS, 0.20, 0.0),
    (2600, MELEE_DEFENSE_BONUS, 0.03, 0.03),
    (3985, MELEE_DEFENSE_BONUS, 0.04, 0.04),
    (2588, MELEE_DEFENSE_BONUS, 0.05, 0.05),
    (4663, MELEE_DEFENSE_BONUS, 0.07, 0.07),
    (6091, MELEE_DEFENSE_BONUS, 0.09, 0.09),
    (3699, MELEE_DEFENSE_BONUS, 0.25, 0.0),
];

/// Skill ids and names (`Dictionaries.SkillInfo`, from Alinco).
const SKILLS: [(i32, &str); 51] = [
    (0x1, "Axe"),
    (0x2, "Bow"),
    (0x3, "Crossbow"),
    (0x4, "Dagger"),
    (0x5, "Mace"),
    (0x6, "Melee Defense"),
    (0x7, "Missile Defense"),
    (0x8, "Sling"),
    (0x9, "Spear"),
    (0xA, "Staff"),
    (0xB, "Sword"),
    (0xC, "Thrown Weapons"),
    (0xD, "Unarmed Combat"),
    (0xE, "Arcane Lore"),
    (0xF, "Magic Defense"),
    (0x10, "Mana Conversion"),
    (0x12, "Item Tinkering"),
    (0x13, "Assess Person"),
    (0x14, "Deception"),
    (0x15, "Healing"),
    (0x16, "Jump"),
    (0x17, "Lockpick"),
    (0x18, "Run"),
    (0x1B, "Assess Creature"),
    (0x1C, "Weapon Tinkering"),
    (0x1D, "Armor Tinkering"),
    (0x1E, "Magic Item Tinkering"),
    (0x1F, "Creature Enchantment"),
    (0x20, "Item Enchantment"),
    (0x21, "Life Magic"),
    (0x22, "War Magic"),
    (0x23, "Leadership"),
    (0x24, "Loyalty"),
    (0x25, "Fletching"),
    (0x26, "Alchemy"),
    (0x27, "Cooking"),
    (0x28, "Salvaging"),
    (0x29, "Two Handed Combat"),
    (0x2A, "Gearcraft"),
    (0x2B, "Void"),
    (0x2C, "Heavy Weapons"),
    (0x2D, "Light Weapons"),
    (0x2E, "Finesse Weapons"),
    (0x2F, "Missile Weapons"),
    (0x30, "Shield"),
    (0x31, "Dual Wield"),
    (0x32, "Recklessness"),
    (0x33, "Sneak Attack"),
    (0x34, "Dirty Fighting"),
    (0x35, "Challenge"),
    (0x36, "Summoning"),
];

/// Weapon masteries (`Dictionaries.MasteryInfo`).
const MASTERIES: [(i32, &str); 12] = [
    (1, "Unarmed Weapon"),
    (2, "Sword"),
    (3, "Axe"),
    (4, "Mace"),
    (5, "Spear"),
    (6, "Dagger"),
    (7, "Staff"),
    (8, "Bow"),
    (9, "Crossbow"),
    (10, "Thrown"),
    (11, "Two Handed Combat"),
    (12, "Wand"),
];

/// Slayer creature types (`Dictionaries.Slayers`).
const SLAYERS: [(i32, &str); 17] = [
    (1, "Olthoi"),
    (5, "Lugian"),
    (6, "Tumerok"),
    (8, "Tusker"),
    (14, "Undead"),
    (19, "Virindi"),
    (22, "Shadow"),
    (30, "Skeleton"),
    (31, "Human"),
    (38, "FireElemental"),
    (42, "LightningElemental"),
    (62, "Elemental"),
    (61, "FrostElemental"),
    (60, "AcidElemental"),
    (75, "Burun"),
    (77, "Ghost"),
    (101, "A'nekshay"),
];

/// Equipment sets (`Dictionaries.AttributeSetInfo`, from the Virindi Tank
/// loot editor).
const ATTRIBUTE_SETS: [(i32, &str); 138] = [
    (2, "Test"),
    (4, "Carraida's Benediction"),
    (5, "Noble Relic Set"),
    (6, "Ancient Relic Set"),
    (7, "Relic Alduressa Set"),
    (8, "Shou-jen Set"),
    (9, "Empyrean Rings Set"),
    (10, "Arm, Mind, Heart Set"),
    (11, "Coat of the Perfect Light Set"),
    (12, "Leggings of Perfect Light Set"),
    (13, "Soldier's Set"),
    (14, "Adept's Set"),
    (15, "Archer's Set"),
    (16, "Defender's Set"),
    (17, "Tinker's Set"),
    (18, "Crafter's Set"),
    (19, "Hearty Set"),
    (20, "Dexterous Set"),
    (21, "Wise Set"),
    (22, "Swift Set"),
    (23, "Hardenend Set"),
    (24, "Reinforced Set"),
    (25, "Interlocking Set"),
    (26, "Flame Proof Set"),
    (27, "Acid Proof Set"),
    (28, "Cold Proof Set"),
    (29, "Lightning Proof Set"),
    (30, "Dedication Set"),
    (31, "Gladiatorial Clothing Set"),
    (32, "Ceremonial Clothing"),
    (33, "Protective Clothing"),
    (34, "Noobie Armor"),
    (35, "Sigil of Defense"),
    (36, "Sigil of Destruction"),
    (37, "Sigil of Fury"),
    (38, "Sigil of Growth"),
    (39, "Sigil of Vigor"),
    (40, "Heroic Protector Set"),
    (41, "Heroic Destroyer Set"),
    (42, "Olthoi Armor D Red"),
    (43, "Olthoi Armor C Rat"),
    (44, "Olthoi Armor C Red"),
    (45, "Olthoi Armor D Rat"),
    (46, "Upgraded Relic Alduressa Set"),
    (47, "Upgraded Ancient Relic Set"),
    (48, "Upgraded Noble Relic Set"),
    (49, "Weave of Alchemy"),
    (50, "Weave of Arcane Lore"),
    (51, "Weave of Armor Tinkering"),
    (52, "Weave of Assess Person"),
    (53, "Weave of Light Weapons"),
    (54, "Weave of Missile Weapons"),
    (55, "Weave of Cooking"),
    (56, "Weave of Creature Enchantment"),
    (57, "Weave of Missile Weapons"),
    (58, "Weave of Finesse"),
    (59, "Weave of Deception"),
    (60, "Weave of Fletching"),
    (61, "Weave of Healing"),
    (62, "Weave of Item Enchantment"),
    (63, "Weave of Item Tinkering"),
    (64, "Weave of Leadership"),
    (65, "Weave of Life Magic"),
    (66, "Weave of Loyalty"),
    (67, "Weave of Light Weapons"),
    (68, "Weave of Magic Defense"),
    (69, "Weave of Magic Item Tinkering"),
    (70, "Weave of Mana Conversion"),
    (71, "Weave of Melee Defense"),
    (72, "Weave of Missile Defense"),
    (73, "Weave of Salvaging"),
    (74, "Weave of Light Weapons"),
    (75, "Weave of Light Weapons"),
    (76, "Weave of Heavy Weapons"),
    (77, "Weave of Missile Weapons"),
    (78, "Weave of Two Handed Combat"),
    (79, "Weave of Light Weapons"),
    (80, "Weave of Void Magic"),
    (81, "Weave of War Magic"),
    (82, "Weave of Weapon Tinkering"),
    (83, "Weave of Assess Creature "),
    (84, "Weave of Dirty Fighting"),
    (85, "Weave of Dual Wield"),
    (86, "Weave of Recklessness"),
    (87, "Weave of Shield"),
    (88, "Weave of Sneak Attack"),
    (89, "Ninja_New"),
    (90, "Weave of Summoning"),
    (91, "Shrouded Soul"),
    (92, "Darkened Mind"),
    (93, "Clouded Spirit"),
    (94, "Minor Stinging Shrouded Soul"),
    (95, "Minor Sparking Shrouded Soul"),
    (96, "Minor Smoldering Shrouded Soul"),
    (97, "Minor Shivering Shrouded Soul"),
    (98, "Minor Stinging Darkened Mind"),
    (99, "Minor Sparking Darkened Mind"),
    (100, "Minor Smoldering Darkened Mind"),
    (101, "Minor Shivering Darkened Mind"),
    (102, "Minor Stinging Clouded Spirit"),
    (103, "Minor Sparking Clouded Spirit"),
    (104, "Minor Smoldering Clouded Spirit"),
    (105, "Minor Shivering Clouded Spirit"),
    (106, "Major Stinging Shrouded Soul"),
    (107, "Major Sparking Shrouded Soul"),
    (108, "Major Smoldering Shrouded Soul"),
    (109, "Major Shivering Shrouded Soul"),
    (110, "Major Stinging Darkened Mind"),
    (111, "Major Sparking Darkened Mind"),
    (112, "Major Smoldering Darkened Mind"),
    (113, "Major Shivering Darkened Mind"),
    (114, "Major Stinging Clouded Spirit"),
    (115, "Major Sparking Clouded Spirit"),
    (116, "Major Smoldering Clouded Spirit"),
    (117, "Major Shivering Clouded Spirit"),
    (118, "Blackfire Stinging Shrouded Soul"),
    (119, "Blackfire Sparking Shrouded Soul"),
    (120, "Blackfire Smoldering Shrouded Soul"),
    (121, "Blackfire Shivering Shrouded Soul"),
    (122, "Blackfire Stinging Darkened Mind"),
    (123, "Blackfire Sparking Darkened Mind"),
    (124, "Blackfire Smoldering Darkened Mind"),
    (125, "Blackfire Shivering Darkened Mind"),
    (126, "Blackfire Stinging Clouded Spirit"),
    (127, "Blackfire Sparking Clouded Spirit"),
    (128, "Blackfire Smoldering Clouded Spirit"),
    (129, "Blackfire Shivering Clouded Spirit"),
    (130, "Shimmering Shadows"),
    (131, "Brown Society Locket"),
    (132, "Yellow Society Locket"),
    (133, "Red Society Band"),
    (134, "Green Society Band"),
    (135, "Purple Society Band"),
    (136, "Blue Society Band"),
    (137, "Gauntlet Garb"),
    (138, "Paragon"),
    (139, "Paragon"),
    (140, "Paragon"),
];
