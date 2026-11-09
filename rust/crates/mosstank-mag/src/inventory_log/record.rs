//! One item as the log keeps it ([`ItemRecord`]), what `MyWorldObject`
//! held, by game property id, and the merge with the last save
//! ([`combine`], [`merge`]): `MyWorldObjectCreator.Combine` and
//! `DumpInventoryToFile`'s loop.

use ac_plugin_abi::items::{InventoryItem, ObjectHeader};
use ac_plugin_abi::world::{ArmorProfile, ItemProperties, ObjectClass, WeaponProfile, WorldObject};
use openac_plugin_compat::json::{JsonNumber, JsonValue, system_text_json};

/// The appraisal's mark on a spell active on the item.
const ACTIVE_ON_ITEM: u32 = 0x8000_0000;

/// The game property ids the client lists an item's numbers from.
mod key {
    /// ArmorLevel (int).
    pub const ARMOR_LEVEL: u32 = 28;
    /// Damage (int).
    pub const DAMAGE: u32 = 44;
    /// DamageType (int).
    pub const DAMAGE_TYPE: u32 = 45;
    /// AttackType (int).
    pub const ATTACK_TYPE: u32 = 47;
    /// WeaponSkill (int).
    pub const WEAPON_SKILL: u32 = 48;
    /// AmmoType (int).
    pub const AMMO_TYPE: u32 = 50;
    /// BoosterEnum (int).
    pub const BOOSTER_ENUM: u32 = 89;
    /// BoostValue (int).
    pub const BOOST_VALUE: u32 = 90;
    /// ItemSpellcraft (int).
    pub const ITEM_SPELLCRAFT: u32 = 106;
    /// ItemCurMana (int).
    pub const ITEM_CUR_MANA: u32 = 107;
    /// ItemMaxMana (int).
    pub const ITEM_MAX_MANA: u32 = 108;
    /// WieldRequirements (int).
    pub const WIELD_REQUIREMENTS: u32 = 158;
    /// WieldSkilltype (int).
    pub const WIELD_SKILLTYPE: u32 = 159;
    /// WieldDifficulty (int).
    pub const WIELD_DIFFICULTY: u32 = 160;
    /// NumTimesTinkered (int).
    pub const NUM_TIMES_TINKERED: u32 = 171;
    /// ImbuedEffect (int).
    pub const IMBUED_EFFECT: u32 = 179;
    /// PetClass (int).
    pub const PET_CLASS: u32 = 266;
    /// WeaponType (int).
    pub const WEAPON_TYPE: u32 = 353;
    /// SummoningMastery (int).
    pub const SUMMONING_MASTERY: u32 = 362;
    /// UseRequiresSkill (int).
    pub const USE_REQUIRES_SKILL: u32 = 366;
    /// UseRequiresSkillLevel (int).
    pub const USE_REQUIRES_SKILL_LEVEL: u32 = 367;
    /// UseRequiresSkillSpec (int).
    pub const USE_REQUIRES_SKILL_SPEC: u32 = 368;
    /// GearDamage (int).
    pub const GEAR_DAMAGE: u32 = 370;
    /// GearDamageResist (int).
    pub const GEAR_DAMAGE_RESIST: u32 = 371;
    /// GearCrit (int).
    pub const GEAR_CRIT: u32 = 372;
    /// GearCritResist (int).
    pub const GEAR_CRIT_RESIST: u32 = 373;
    /// GearCritDamage (int).
    pub const GEAR_CRIT_DAMAGE: u32 = 374;
    /// GearCritDamageResist (int).
    pub const GEAR_CRIT_DAMAGE_RESIST: u32 = 375;
    /// DamageVariance (float).
    pub const DAMAGE_VARIANCE: u32 = 22;
    /// HealkitMod (float).
    pub const HEALKIT_MOD: u32 = 100;
    /// ProcSpellRate (float).
    pub const PROC_SPELL_RATE: u32 = 156;
    /// Retained (bool).
    pub const RETAINED: u32 = 91;
    /// ProcSpellSelfTargeted (bool).
    pub const PROC_SPELL_SELF_TARGETED: u32 = 112;
    /// Spell (data id).
    pub const SPELL: u32 = 28;
    /// ProcSpell (data id).
    pub const PROC_SPELL: u32 = 55;
}

/// What only the item's description carried (not its properties), kept
/// for the loot rules that read it (the Inventory window's OD): the icon
/// layers (a rare's backdrop among them), its effects, how it is used.
/// All none (zero) in a file written before they were kept.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Described {
    /// Usability bits.
    pub useability: u32,
    /// What it may be used on.
    pub target_type: u32,
    /// The public flags.
    pub public_flags: u32,
    /// The most uses.
    pub maximum_structure: i32,
    /// Its spell (a scroll's, a gem's).
    pub spell_id: u32,
    /// The shared cooldown it belongs to.
    pub shared_cooldown_id: u32,
    /// That cooldown's length, seconds.
    pub cooldown_seconds: f64,
    /// Melee, missile, ammunition, shield.
    pub combat_use: i32,
    /// The ammunition it is or takes.
    pub ammo_type: u32,
    /// The largest stack.
    pub maximum_stack_size: i32,
    /// The items it holds.
    pub items_capacity: i32,
    /// The containers it holds.
    pub containers_capacity: i32,
    /// Its icon effects (the magical highlight among them).
    pub effects: u32,
    /// The icon's underlay (a full icon id).
    pub icon_underlay_id: u32,
    /// The icon's overlay (a full icon id).
    pub icon_overlay_id: u32,
    /// How near, metres, it must be to be used.
    pub use_radius: f32,
    /// What a hook takes.
    pub hook_item_types: u32,
    /// The name for more than one.
    pub plural_name: String,
}

impl Described {
    /// What `item`'s description carried.
    #[must_use]
    pub fn of(item: &InventoryItem) -> Self {
        Self {
            useability: item.useability,
            target_type: item.target_type,
            public_flags: item.public_flags,
            maximum_structure: item.maximum_structure,
            spell_id: item.spell_id,
            shared_cooldown_id: item.shared_cooldown_id,
            cooldown_seconds: item.cooldown_seconds,
            combat_use: item.combat_use,
            ammo_type: item.ammo_type,
            maximum_stack_size: item.maximum_stack_size,
            items_capacity: item.items_capacity,
            containers_capacity: item.containers_capacity,
            effects: item.effects,
            icon_underlay_id: item.icon_underlay_id,
            icon_overlay_id: item.icon_overlay_id,
            use_radius: item.use_radius,
            hook_item_types: item.hook_item_types,
            plural_name: item.plural_name.clone(),
        }
    }

    /// As one JSON object of the members that are not none.
    fn json_value(&self) -> JsonValue {
        let mut members: Vec<(&str, JsonValue)> = Vec::new();
        let mut unsigned = |name, value: u32| {
            if value != 0 {
                members.push((name, u(value)));
            }
        };
        unsigned("useability", self.useability);
        unsigned("target_type", self.target_type);
        unsigned("public_flags", self.public_flags);
        unsigned("spell_id", self.spell_id);
        unsigned("shared_cooldown_id", self.shared_cooldown_id);
        unsigned("ammo_type", self.ammo_type);
        unsigned("effects", self.effects);
        unsigned("icon_underlay_id", self.icon_underlay_id);
        unsigned("icon_overlay_id", self.icon_overlay_id);
        unsigned("hook_item_types", self.hook_item_types);
        for (name, value) in [
            ("maximum_structure", self.maximum_structure),
            ("combat_use", self.combat_use),
            ("maximum_stack_size", self.maximum_stack_size),
            ("items_capacity", self.items_capacity),
            ("containers_capacity", self.containers_capacity),
        ] {
            if value != 0 {
                members.push((name, i(value)));
            }
        }
        if self.cooldown_seconds != 0.0 {
            members.push(("cooldown_seconds", f64_value(self.cooldown_seconds)));
        }
        if self.use_radius != 0.0 {
            members.push(("use_radius", f32_value(self.use_radius)));
        }
        if !self.plural_name.is_empty() {
            members.push(("plural_name", JsonValue::String(self.plural_name.clone())));
        }
        JsonValue::object(members)
    }

    /// Read from its JSON object; a member absent is none. `None` when a
    /// member is of the wrong kind.
    fn from_json(value: &JsonValue) -> Option<Self> {
        let unsigned = |name| value.get(name).map_or(Some(0), read_u32);
        let signed = |name| value.get(name).map_or(Some(0), read_i32);
        Some(Self {
            useability: unsigned("useability")?,
            target_type: unsigned("target_type")?,
            public_flags: unsigned("public_flags")?,
            maximum_structure: signed("maximum_structure")?,
            spell_id: unsigned("spell_id")?,
            shared_cooldown_id: unsigned("shared_cooldown_id")?,
            cooldown_seconds: value.get("cooldown_seconds").map_or(Some(0.0), read_f64)?,
            combat_use: signed("combat_use")?,
            ammo_type: unsigned("ammo_type")?,
            maximum_stack_size: signed("maximum_stack_size")?,
            items_capacity: signed("items_capacity")?,
            containers_capacity: signed("containers_capacity")?,
            effects: unsigned("effects")?,
            icon_underlay_id: unsigned("icon_underlay_id")?,
            icon_overlay_id: unsigned("icon_overlay_id")?,
            use_radius: value.get("use_radius").map_or(Some(0.0), read_f32)?,
            hook_item_types: unsigned("hook_item_types")?,
            plural_name: match value.get("plural_name") {
                Some(name) => name.as_str()?.to_owned(),
                None => String::new(),
            },
        })
    }
}

/// One owned item: where it is, what its description and its appraisal
/// said, and its spells split as Decal split them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemRecord {
    /// Its object id.
    pub id: u32,
    /// Its name.
    pub name: String,
    /// What kind of thing it is.
    pub object_class: ObjectClass,
    /// Its icon (a full icon id).
    pub icon: u32,
    /// Its weenie class.
    pub weenie_class_id: u32,
    /// Item type bits.
    pub item_type: u32,
    /// The container holding it, 0 for none.
    pub container_id: u32,
    /// Its place in that container among its kind; -1 when in none.
    pub container_slot: i32,
    /// The creature wielding it, 0 for none.
    pub wielder_id: u32,
    /// Where it is worn or wielded (bits), 0 when not.
    pub equipped_location: u32,
    /// Where it can be worn or wielded (EquipMask bits), as its
    /// description carried it; 0 in a file written before it was kept.
    pub valid_locations: u32,
    /// The body parts it covers (coverage bits), as its description
    /// carried it; 0 in a file written before it was kept.
    pub coverage_mask: u32,
    /// How many in its stack.
    pub stack_size: i32,
    /// Value, as its description carried it.
    pub value: i32,
    /// Burden, as its description carried it.
    pub burden: i32,
    /// Uses left, as its description carried it.
    pub structure: i32,
    /// Its material, as its description carried it.
    pub material_type: u32,
    /// Its workmanship, as its description carried it.
    pub workmanship: f32,
    /// Which optional values its description carried; none when no
    /// description was seen.
    pub weenie_header_flags: Option<u32>,
    /// It had been appraised (`HasIdData`).
    pub has_id_data: bool,
    /// When that appraisal was taken in, Unix milliseconds; 0 for never.
    pub last_id_time: i64,
    /// Its properties by game property id, with the appraised profiles.
    pub properties: ItemProperties,
    /// The appraisal's spells without the active mark (`Spells`).
    pub spells: Vec<u32>,
    /// The appraisal's spells active on the item, unmarked
    /// (`ActiveSpells`).
    pub active_spells: Vec<u32>,
    /// What only its description carried, for the loot rules.
    pub described: Described,
}

impl ItemRecord {
    /// `MyWorldObjectCreator.Create`: the record of an owned item as the
    /// client has it now. `object` is the world's record of it (whether it
    /// was appraised); `id_time_ms` is the time its appraisal counts as
    /// taken in, kept only when it was appraised.
    #[must_use]
    pub fn from_live(
        item: &InventoryItem,
        object: Option<&WorldObject>,
        properties: &ItemProperties,
        id_time_ms: i64,
    ) -> Self {
        let has_id_data = object.is_some_and(|object| object.has_appraisal_data);
        let mut spells = Vec::new();
        let mut active_spells = Vec::new();
        for &id in &item.appraised_spell_ids {
            if id & ACTIVE_ON_ITEM != 0 {
                active_spells.push(id & !ACTIVE_ON_ITEM);
            } else {
                spells.push(id);
            }
        }
        Self {
            id: item.object_id,
            name: item.name.clone(),
            object_class: item.object_class,
            icon: item.icon_id,
            weenie_class_id: item.weenie_class_id,
            item_type: item.item_type,
            container_id: item.container_id,
            container_slot: item.container_slot,
            wielder_id: item.wielder_id,
            equipped_location: item.equipped_location,
            valid_locations: item.valid_locations,
            coverage_mask: item.coverage_mask,
            stack_size: item.stack_size,
            value: item.value,
            burden: item.burden,
            structure: item.structure,
            material_type: item.material_type,
            workmanship: item.workmanship,
            weenie_header_flags: item.header.as_ref().and_then(|h| h.weenie_header_flags),
            has_id_data,
            last_id_time: if has_id_data { id_time_ms } else { 0 },
            properties: properties.clone(),
            spells,
            active_spells,
            described: Described::of(item),
        }
    }

    /// The item as the client listed it, rebuilt from the record: what the
    /// item info line reads ([`crate::item_info::item_info`]), and every
    /// number the loot rules read, listed as the client lists a live item:
    /// from the kept properties (the weapon numbers from the appraisal's
    /// weapon profile when it has one), and what only the description
    /// carried from [`Described`]. Its palettes are not kept: colour rules
    /// do not match a logged item.
    #[must_use]
    pub fn item(&self) -> InventoryItem {
        let mut appraised_spell_ids = self.spells.clone();
        appraised_spell_ids.extend(self.active_spells.iter().map(|id| id | ACTIVE_ON_ITEM));
        let properties = &self.properties;
        let int = |id: u32| properties.int(id).unwrap_or(0);
        let float = |id: u32| {
            properties
                .floats
                .iter()
                .find(|(key, _)| *key == id)
                .map_or(0.0, |(_, value)| *value)
        };
        let boolean = |id: u32| {
            properties
                .bools
                .iter()
                .find(|(key, _)| *key == id)
                .is_some_and(|(_, value)| *value)
        };
        let data_id = |id: u32| {
            properties
                .data_ids
                .iter()
                .find(|(key, _)| *key == id)
                .map_or(0, |(_, value)| *value)
        };
        let weapon = properties.weapon.as_ref();
        let damage = weapon.map_or_else(|| int(key::DAMAGE), |weapon| weapon.damage);
        let described = &self.described;
        InventoryItem {
            useability: described.useability,
            target_type: described.target_type,
            public_flags: described.public_flags,
            maximum_structure: described.maximum_structure,
            spell_id: match described.spell_id {
                0 => data_id(key::SPELL),
                id => id,
            },
            pet_class: int(key::PET_CLASS),
            summoning_mastery: int(key::SUMMONING_MASTERY),
            shared_cooldown_id: described.shared_cooldown_id,
            cooldown_seconds: described.cooldown_seconds,
            proc_spell_id: data_id(key::PROC_SPELL),
            proc_spell_self_targeted: boolean(key::PROC_SPELL_SELF_TARGETED),
            proc_spell_rate: float(key::PROC_SPELL_RATE),
            weapon_skill: weapon.map_or_else(
                || int(key::WEAPON_SKILL),
                |weapon| weapon.weapon_skill.cast_signed(),
            ),
            damage_type: weapon.map_or_else(|| int(key::DAMAGE_TYPE), |weapon| weapon.damage_type),
            damage,
            damage_variance: weapon.map_or_else(
                || float(key::DAMAGE_VARIANCE),
                |weapon| weapon.damage_variance,
            ),
            use_requires_skill: int(key::USE_REQUIRES_SKILL),
            use_requires_skill_level: int(key::USE_REQUIRES_SKILL_LEVEL),
            use_requires_skill_specialized: int(key::USE_REQUIRES_SKILL_SPEC),
            combat_use: described.combat_use,
            item_spellcraft: int(key::ITEM_SPELLCRAFT),
            wield_requirements: int(key::WIELD_REQUIREMENTS),
            wield_skill_type: int(key::WIELD_SKILLTYPE),
            wield_difficulty: int(key::WIELD_DIFFICULTY),
            attack_type: int(key::ATTACK_TYPE),
            weapon_type: int(key::WEAPON_TYPE),
            ammo_type: match described.ammo_type {
                0 => int(key::AMMO_TYPE).max(0).cast_unsigned(),
                ammo => ammo,
            },
            booster_vital: int(key::BOOSTER_ENUM),
            boost_value: int(key::BOOST_VALUE),
            heal_kit_modifier: float(key::HEALKIT_MOD),
            gear_damage: int(key::GEAR_DAMAGE),
            gear_damage_resistance: int(key::GEAR_DAMAGE_RESIST),
            gear_critical_chance: int(key::GEAR_CRIT),
            gear_critical_resistance: int(key::GEAR_CRIT_RESIST),
            gear_critical_damage: int(key::GEAR_CRIT_DAMAGE),
            gear_critical_damage_resistance: int(key::GEAR_CRIT_DAMAGE_RESIST),
            maximum_stack_size: described.maximum_stack_size,
            items_capacity: described.items_capacity,
            containers_capacity: described.containers_capacity,
            item_current_mana: int(key::ITEM_CUR_MANA),
            item_maximum_mana: int(key::ITEM_MAX_MANA),
            salvage_workmanship: f64::from(self.workmanship),
            num_times_tinkered: int(key::NUM_TIMES_TINKERED),
            imbued_effect: int(key::IMBUED_EFFECT),
            armor_level: int(key::ARMOR_LEVEL),
            max_damage: damage.max(0),
            wand_elemental_damage_type: int(key::DAMAGE_TYPE),
            retained: boolean(key::RETAINED),
            effects: described.effects,
            icon_underlay_id: described.icon_underlay_id,
            icon_overlay_id: described.icon_overlay_id,
            plural_name: described.plural_name.clone(),
            use_radius: described.use_radius,
            hook_item_types: described.hook_item_types,
            object_id: self.id,
            weenie_class_id: self.weenie_class_id,
            name: self.name.clone(),
            item_type: self.item_type,
            container_id: self.container_id,
            wielder_id: self.wielder_id,
            equipped_location: self.equipped_location,
            valid_locations: self.valid_locations,
            coverage_mask: self.coverage_mask,
            stack_size: self.stack_size,
            structure: self.structure,
            appraised_spell_ids,
            container_slot: self.container_slot,
            burden: self.burden,
            value: self.value,
            workmanship: self.workmanship,
            material_type: self.material_type,
            object_class: self.object_class,
            icon_id: self.icon,
            header: self.weenie_header_flags.map(|flags| ObjectHeader {
                weenie_header_flags: Some(flags),
                ..ObjectHeader::default()
            }),
            ..InventoryItem::default()
        }
    }

    /// Its properties by game property id.
    #[must_use]
    pub fn properties(&self) -> &ItemProperties {
        &self.properties
    }

    /// The record as one compact JSON object (one element of the file's
    /// `items`).
    #[must_use]
    pub fn to_json(&self) -> String {
        // Only non-finite numbers fail, and none is written as a number.
        system_text_json::to_string(&self.json_value(), &system_text_json::Options::COMPACT)
            .unwrap_or_default()
    }

    fn json_value(&self) -> JsonValue {
        let p = &self.properties;
        JsonValue::object([
            ("id", u(self.id)),
            ("name", JsonValue::String(self.name.clone())),
            ("object_class", u(u32::from(self.object_class.tag()))),
            ("icon", u(self.icon)),
            ("weenie_class_id", u(self.weenie_class_id)),
            ("item_type", u(self.item_type)),
            ("container_id", u(self.container_id)),
            ("container_slot", i(self.container_slot)),
            ("wielder_id", u(self.wielder_id)),
            ("equipped_location", u(self.equipped_location)),
            ("valid_locations", u(self.valid_locations)),
            ("coverage_mask", u(self.coverage_mask)),
            ("stack_size", i(self.stack_size)),
            ("value", i(self.value)),
            ("burden", i(self.burden)),
            ("structure", i(self.structure)),
            ("material_type", u(self.material_type)),
            ("workmanship", f32_value(self.workmanship)),
            (
                "weenie_header_flags",
                self.weenie_header_flags.map_or(JsonValue::Null, u),
            ),
            ("has_id_data", JsonValue::Bool(self.has_id_data)),
            (
                "last_id_time",
                JsonValue::Number(JsonNumber::I64(self.last_id_time)),
            ),
            ("ints", map(&p.ints, |v| i(*v))),
            (
                "int64s",
                map(&p.int64s, |v| JsonValue::Number(JsonNumber::I64(*v))),
            ),
            ("floats", map(&p.floats, |v| f64_value(*v))),
            ("bools", map(&p.bools, |v| JsonValue::Bool(*v))),
            ("strings", map(&p.strings, |v| JsonValue::String(v.clone()))),
            ("data_ids", map(&p.data_ids, |v| u(*v))),
            ("instance_ids", map(&p.instance_ids, |v| u(*v))),
            (
                "weapon",
                p.weapon.as_ref().map_or(JsonValue::Null, weapon_json),
            ),
            (
                "armor",
                p.armor.as_ref().map_or(JsonValue::Null, armor_json),
            ),
            (
                "spells",
                JsonValue::array(self.spells.iter().map(|id| u(*id))),
            ),
            (
                "active_spells",
                JsonValue::array(self.active_spells.iter().map(|id| u(*id))),
            ),
            ("described", self.described.json_value()),
        ])
    }

    /// The record one element of the file's `items` holds; `None` when a
    /// member is missing or of the wrong kind.
    #[must_use]
    pub fn from_json(value: &JsonValue) -> Option<Self> {
        let properties = ItemProperties {
            ints: read_map(value.get("ints")?, read_i32)?,
            int64s: read_map(value.get("int64s")?, JsonValue::as_i64)?,
            bools: read_map(value.get("bools")?, JsonValue::as_bool)?,
            floats: read_map(value.get("floats")?, read_f64)?,
            strings: read_map(value.get("strings")?, |v| v.as_str().map(str::to_owned))?,
            data_ids: read_map(value.get("data_ids")?, read_u32)?,
            instance_ids: read_map(value.get("instance_ids")?, read_u32)?,
            weapon: optional(value.get("weapon")?, read_weapon)?,
            armor: optional(value.get("armor")?, read_armor)?,
            // An owned item has no creature profile; the log keeps none.
            creature: None,
        };
        let class = u8::try_from(member(value, "object_class", read_u32)?).ok()?;
        Some(Self {
            id: member(value, "id", read_u32)?,
            name: value.get("name")?.as_str()?.to_owned(),
            object_class: ObjectClass::from_tag(class)?,
            icon: member(value, "icon", read_u32)?,
            weenie_class_id: member(value, "weenie_class_id", read_u32)?,
            item_type: member(value, "item_type", read_u32)?,
            container_id: member(value, "container_id", read_u32)?,
            container_slot: member(value, "container_slot", read_i32)?,
            wielder_id: member(value, "wielder_id", read_u32)?,
            equipped_location: member(value, "equipped_location", read_u32)?,
            // Kept since the Inventory filters; none in an older file.
            valid_locations: value.get("valid_locations").map_or(Some(0), read_u32)?,
            coverage_mask: value.get("coverage_mask").map_or(Some(0), read_u32)?,
            stack_size: member(value, "stack_size", read_i32)?,
            value: member(value, "value", read_i32)?,
            burden: member(value, "burden", read_i32)?,
            structure: member(value, "structure", read_i32)?,
            material_type: member(value, "material_type", read_u32)?,
            workmanship: member(value, "workmanship", read_f32)?,
            weenie_header_flags: optional(value.get("weenie_header_flags")?, read_u32)?,
            has_id_data: value.get("has_id_data")?.as_bool()?,
            last_id_time: value.get("last_id_time")?.as_i64()?,
            properties,
            spells: read_ids(value.get("spells")?)?,
            active_spells: read_ids(value.get("active_spells")?)?,
            // Kept since the Weapon OD; none in an older file.
            described: match value.get("described") {
                Some(described) => Described::from_json(described)?,
                None => Described::default(),
            },
        })
    }
}

/// `MyWorldObjectCreator.Combine(older, newer)`: a fresh appraisal, or an
/// older record never appraised, gives the live record. Otherwise the
/// older appraised record stays, every value the live item has
/// overwriting or joining its own (`AddTo`): where it is and what its
/// description says now, its property keys; the keys only the appraisal
/// had stay, and so do its id time, class and spells.
#[must_use]
pub fn combine(older: ItemRecord, newer: ItemRecord) -> ItemRecord {
    if !older.has_id_data || newer.has_id_data {
        return newer;
    }
    let mut kept = older;
    let ItemRecord {
        name,
        icon,
        weenie_class_id,
        item_type,
        container_id,
        container_slot,
        wielder_id,
        equipped_location,
        valid_locations,
        coverage_mask,
        stack_size,
        value,
        burden,
        structure,
        material_type,
        workmanship,
        described,
        weenie_header_flags,
        properties,
        ..
    } = newer;
    kept.name = name;
    kept.icon = icon;
    kept.weenie_class_id = weenie_class_id;
    kept.item_type = item_type;
    kept.container_id = container_id;
    kept.container_slot = container_slot;
    kept.wielder_id = wielder_id;
    kept.equipped_location = equipped_location;
    kept.valid_locations = valid_locations;
    kept.coverage_mask = coverage_mask;
    kept.stack_size = stack_size;
    kept.value = value;
    kept.burden = burden;
    kept.structure = structure;
    kept.material_type = material_type;
    kept.workmanship = workmanship;
    kept.described = described;
    if weenie_header_flags.is_some() {
        kept.weenie_header_flags = weenie_header_flags;
    }
    let p = &mut kept.properties;
    add_to(&mut p.ints, properties.ints);
    add_to(&mut p.int64s, properties.int64s);
    add_to(&mut p.bools, properties.bools);
    add_to(&mut p.floats, properties.floats);
    add_to(&mut p.strings, properties.strings);
    add_to(&mut p.data_ids, properties.data_ids);
    add_to(&mut p.instance_ids, properties.instance_ids);
    if properties.weapon.is_some() {
        p.weapon = properties.weapon;
    }
    if properties.armor.is_some() {
        p.armor = properties.armor;
    }
    kept
}

/// `AddTo`: each of `from`'s keys overwrites `into`'s value or is added.
fn add_to<V>(into: &mut Vec<(u32, V)>, from: Vec<(u32, V)>) {
    for (key, value) in from {
        match into.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => into.push((key, value)),
        }
    }
}

/// `DumpInventoryToFile`'s records: each live item combined with the first
/// previous record of the same id and class; items not owned now are not
/// kept.
#[must_use]
pub fn merge(previous: &[ItemRecord], live: Vec<ItemRecord>) -> Vec<ItemRecord> {
    live.into_iter()
        .map(
            |newer| match previous.iter().find(|p| same_item(p, &newer)) {
                Some(older) => combine(older.clone(), newer),
                None => newer,
            },
        )
        .collect()
}

/// Two records are of the same item: the same id and class.
#[must_use]
pub fn same_item(a: &ItemRecord, b: &ItemRecord) -> bool {
    a.id == b.id && a.object_class == b.object_class
}

fn u(v: u32) -> JsonValue {
    JsonValue::Number(JsonNumber::U64(u64::from(v)))
}

fn i(v: i32) -> JsonValue {
    JsonValue::Number(JsonNumber::I64(i64::from(v)))
}

/// A double, `null` when it is not finite (JSON has no such number). The
/// text is the shortest that reads back as the same value (Rust's own,
/// which costs a small part of the .NET-exact formatting the plugin kit
/// offers; nothing reads this file but this plugin).
fn f64_value(v: f64) -> JsonValue {
    if v.is_finite() {
        JsonValue::Number(JsonNumber::Text(format!("{v:?}")))
    } else {
        JsonValue::Null
    }
}

fn f32_value(v: f32) -> JsonValue {
    if v.is_finite() {
        JsonValue::Number(JsonNumber::Text(format!("{v:?}")))
    } else {
        JsonValue::Null
    }
}

fn map<V>(entries: &[(u32, V)], value: impl Fn(&V) -> JsonValue) -> JsonValue {
    JsonValue::Object(
        entries
            .iter()
            .map(|(key, v)| (key.to_string(), value(v)))
            .collect(),
    )
}

fn weapon_json(w: &WeaponProfile) -> JsonValue {
    JsonValue::object([
        ("damage_type", i(w.damage_type)),
        ("weapon_time", i(w.weapon_time)),
        ("weapon_skill", u(w.weapon_skill)),
        ("damage", i(w.damage)),
        ("damage_variance", f64_value(w.damage_variance)),
        ("damage_mod", f64_value(w.damage_mod)),
        ("weapon_length", f64_value(w.weapon_length)),
        ("max_velocity", f64_value(w.max_velocity)),
        ("weapon_offense", f64_value(w.weapon_offense)),
        ("max_velocity_estimated", i(w.max_velocity_estimated)),
    ])
}

fn armor_json(a: &ArmorProfile) -> JsonValue {
    JsonValue::object([
        ("armor_level", i(a.armor_level)),
        ("slash_mod", f32_value(a.slash_mod)),
        ("pierce_mod", f32_value(a.pierce_mod)),
        ("bludgeon_mod", f32_value(a.bludgeon_mod)),
        ("cold_mod", f32_value(a.cold_mod)),
        ("fire_mod", f32_value(a.fire_mod)),
        ("acid_mod", f32_value(a.acid_mod)),
        ("nether_mod", f32_value(a.nether_mod)),
        ("electric_mod", f32_value(a.electric_mod)),
    ])
}

fn member<T>(value: &JsonValue, name: &str, read: impl Fn(&JsonValue) -> Option<T>) -> Option<T> {
    read(value.get(name)?)
}

fn read_u32(v: &JsonValue) -> Option<u32> {
    u32::try_from(v.as_i64()?).ok()
}

fn read_i32(v: &JsonValue) -> Option<i32> {
    i32::try_from(v.as_i64()?).ok()
}

/// A double; `null` (a non-finite one written) reads as NaN.
fn read_f64(v: &JsonValue) -> Option<f64> {
    if v.is_null() {
        return Some(f64::NAN);
    }
    v.as_f64()
}

#[allow(clippy::cast_possible_truncation)]
fn read_f32(v: &JsonValue) -> Option<f32> {
    read_f64(v).map(|v| v as f32)
}

/// `null` is none; anything else must read.
fn optional<T>(v: &JsonValue, read: impl Fn(&JsonValue) -> Option<T>) -> Option<Option<T>> {
    if v.is_null() {
        Some(None)
    } else {
        read(v).map(Some)
    }
}

fn read_map<V>(v: &JsonValue, read: impl Fn(&JsonValue) -> Option<V>) -> Option<Vec<(u32, V)>> {
    v.as_object()?
        .iter()
        .map(|(key, value)| Some((key.parse().ok()?, read(value)?)))
        .collect()
}

fn read_ids(v: &JsonValue) -> Option<Vec<u32>> {
    v.as_array()?.iter().map(read_u32).collect()
}

fn read_weapon(v: &JsonValue) -> Option<WeaponProfile> {
    Some(WeaponProfile {
        damage_type: member(v, "damage_type", read_i32)?,
        weapon_time: member(v, "weapon_time", read_i32)?,
        weapon_skill: member(v, "weapon_skill", read_u32)?,
        damage: member(v, "damage", read_i32)?,
        damage_variance: member(v, "damage_variance", read_f64)?,
        damage_mod: member(v, "damage_mod", read_f64)?,
        weapon_length: member(v, "weapon_length", read_f64)?,
        max_velocity: member(v, "max_velocity", read_f64)?,
        weapon_offense: member(v, "weapon_offense", read_f64)?,
        max_velocity_estimated: member(v, "max_velocity_estimated", read_i32)?,
    })
}

fn read_armor(v: &JsonValue) -> Option<ArmorProfile> {
    Some(ArmorProfile {
        armor_level: member(v, "armor_level", read_i32)?,
        slash_mod: member(v, "slash_mod", read_f32)?,
        pierce_mod: member(v, "pierce_mod", read_f32)?,
        bludgeon_mod: member(v, "bludgeon_mod", read_f32)?,
        cold_mod: member(v, "cold_mod", read_f32)?,
        fire_mod: member(v, "fire_mod", read_f32)?,
        acid_mod: member(v, "acid_mod", read_f32)?,
        nether_mod: member(v, "nether_mod", read_f32)?,
        electric_mod: member(v, "electric_mod", read_f32)?,
    })
}
