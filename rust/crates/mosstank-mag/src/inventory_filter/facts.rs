//! What the filters read of one item ([`RowFacts`]), worked out once when
//! its row is read.

use std::rc::Rc;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass};

use super::cantrips::{CantripTier, cantrip_of};
use super::slots::slot_name;
use crate::item_info::{Values, key as item_key};

/// A rating the Ratings tab filters on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rating {
    /// Damage rating.
    DamageRating,
    /// Critical damage rating.
    CritDamage,
    /// Healing boost rating.
    HealBoost,
    /// Vitality (the Gear maximum health).
    Vitality,
    /// Armor level.
    ArmorLevel,
    /// Damage resistance rating.
    DamageResist,
    /// Critical resistance rating.
    CritResist,
    /// Critical damage resistance rating.
    CritDamageResist,
    /// Healing resistance rating.
    HealingResist,
    /// Nether resistance rating.
    NetherResist,
    /// Damage over time resistance rating.
    DotResist,
    /// Life resistance rating.
    LifeResist,
    /// Sneak attack rating.
    SneakAttack,
    /// Recklessness rating.
    Recklessness,
    /// Deception rating.
    Deception,
    /// Player-killer damage rating.
    PkDamage,
    /// Player-killer damage resistance rating.
    PkDamageResist,
    /// Gear player-killer damage rating.
    GearPkDamage,
    /// Gear player-killer damage resistance rating.
    GearPkResist,
    /// Times tinkered.
    Tinks,
}

impl Rating {
    /// Every rating, in the Ratings tab's order.
    pub const ALL: [Self; 20] = [
        Self::DamageRating,
        Self::CritDamage,
        Self::HealBoost,
        Self::Vitality,
        Self::ArmorLevel,
        Self::DamageResist,
        Self::CritResist,
        Self::CritDamageResist,
        Self::HealingResist,
        Self::NetherResist,
        Self::DotResist,
        Self::LifeResist,
        Self::SneakAttack,
        Self::Recklessness,
        Self::Deception,
        Self::PkDamage,
        Self::PkDamageResist,
        Self::GearPkDamage,
        Self::GearPkResist,
        Self::Tinks,
    ];

    /// Its place in [`Self::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    /// Its label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::DamageRating => "Damage rating",
            Self::CritDamage => "Crit damage",
            Self::HealBoost => "Heal boost",
            Self::Vitality => "Vitality",
            Self::ArmorLevel => "Armor level",
            Self::DamageResist => "Damage resist",
            Self::CritResist => "Crit resist",
            Self::CritDamageResist => "Crit dmg resist",
            Self::HealingResist => "Healing resist",
            Self::NetherResist => "Nether resist",
            Self::DotResist => "DoT resist",
            Self::LifeResist => "Life resist",
            Self::SneakAttack => "Sneak attack",
            Self::Recklessness => "Recklessness",
            Self::Deception => "Deception",
            Self::PkDamage => "PK damage",
            Self::PkDamageResist => "PK dmg resist",
            Self::GearPkDamage => "Gear PK dmg",
            Self::GearPkResist => "Gear PK resist",
            Self::Tinks => "Tinks",
        }
    }
}

/// One item's facts, as the filters read them.
#[derive(Debug, Clone, Default)]
pub struct RowFacts {
    /// It is worn or wielded.
    pub equipped: bool,
    /// Bonded.
    pub bonded: bool,
    /// Attuned.
    pub attuned: bool,
    /// A rare.
    pub rare: bool,
    /// What kind of thing it is.
    pub class: ObjectClass,
    /// Its name, lowercase (without its material).
    pub name: String,
    /// Its armor level.
    pub armor_level: Option<i32>,
    /// The body parts it covers.
    pub coverage: u32,
    /// The skill it is used with: a melee or missile weapon's weapon
    /// skill, a caster's wield skill; 0 for none.
    pub weapon_skill: i32,
    /// Its slot's name.
    pub slot: String,
    /// The character level it needs, when it needs one.
    pub wield_level: Option<i32>,
    /// Its value.
    pub value: Option<i32>,
    /// Its workmanship.
    pub workmanship: Option<i32>,
    /// Its burden.
    pub burden: Option<i32>,
    /// Its equipment set.
    pub set_id: Option<i32>,
    /// Its ratings, by [`Rating::index`].
    pub ratings: [Option<i32>; Rating::ALL.len()],
    /// Its critical damage rating as a tier, 0 to 2.
    pub cd_tier: u8,
    /// Its spells' names, lowercase.
    pub spells: Vec<Rc<str>>,
    /// The cantrips among them, with their tiers.
    pub cantrips: Vec<(usize, CantripTier)>,
    /// It was appraised.
    pub identified: bool,
}

impl RowFacts {
    /// The facts of `item`: `identified` when it was appraised;
    /// `spell_names` its spells' names, lowercase.
    #[must_use]
    pub fn new(
        item: &InventoryItem,
        properties: &ItemProperties,
        identified: bool,
        spell_names: Vec<Rc<str>>,
    ) -> Self {
        let values = Values::new(item, properties);
        let int = |id: u32| properties.int(id);
        let positive = |id: u32| int(id).is_some_and(|v| v > 0);
        let class = item.object_class;
        let valid_locations = match item.valid_locations {
            0 => int(key::VALID_LOCATIONS).map_or(0, i32::cast_unsigned),
            v => v,
        };
        let coverage = match item.coverage_mask {
            0 => int(key::COVERAGE).map_or(0, i32::cast_unsigned),
            v => v,
        };
        let has_material = values.long(item_key::MATERIAL) > 0;
        let weapon_skill = if class == ObjectClass::WandStaffOrb {
            int(item_key::WIELD_REQ_ATTRIBUTE).unwrap_or(0)
        } else {
            properties
                .weapon
                .as_ref()
                .map(|weapon| weapon.weapon_skill.cast_signed())
                .filter(|skill| *skill != 0)
                .or_else(|| (item.weapon_skill != 0).then_some(item.weapon_skill))
                .or_else(|| int(key::WEAPON_SKILL))
                .unwrap_or(0)
        };
        let workmanship = int(item_key::WORKMANSHIP).or_else(|| {
            // The description's, before an appraisal (whole for loot).
            #[allow(clippy::cast_possible_truncation)]
            let described = item.workmanship.floor() as i32;
            (described > 0).then_some(described)
        });
        let ratings =
            Rating::ALL.map(|rating| rating.property_ids().iter().filter_map(|id| int(*id)).max());
        let crit_damage = ratings[Rating::CritDamage.index()].unwrap_or(0);
        let cantrips = spell_names
            .iter()
            .filter_map(|name| cantrip_of(name))
            .collect();
        Self {
            equipped: item.equipped_location != 0 || positive(key::CURRENT_WIELDED_LOCATION),
            bonded: positive(key::BONDED),
            attuned: positive(key::ATTUNED),
            rare: positive(key::RARE_ID),
            class,
            name: item.name.to_lowercase(),
            armor_level: int(item_key::ARMOR_LEVEL),
            coverage,
            weapon_skill,
            slot: slot_name(valid_locations, coverage, has_material),
            wield_level: wield_level(&int),
            value: values.long_opt(item_key::VALUE),
            workmanship,
            burden: values.long_opt(item_key::BURDEN),
            set_id: int(item_key::EQUIPMENT_SET).filter(|id| *id != 0),
            ratings,
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            cd_tier: crit_damage.clamp(0, 2) as u8,
            spells: spell_names,
            cantrips,
            identified,
        }
    }

    /// The Value column's text: blank when it has none.
    #[must_use]
    pub fn value_text(&self) -> String {
        self.value
            .map_or_else(String::new, |value| crate::fmt::grouped(i64::from(value)))
    }

    /// The Wield column's text: blank for no level requirement.
    #[must_use]
    pub fn wield_text(&self) -> String {
        self.wield_level
            .map_or_else(String::new, |level| level.to_string())
    }

    /// One of its ratings.
    #[must_use]
    pub fn rating(&self, rating: Rating) -> Option<i32> {
        self.ratings[rating.index()]
    }
}

/// The level requirement: the first requirement triple that asks for a
/// character level (requirement 7 on attribute 1), the first then the
/// second.
fn wield_level(int: &impl Fn(u32) -> Option<i32>) -> Option<i32> {
    [
        (
            item_key::WIELD_REQ_TYPE,
            item_key::WIELD_REQ_ATTRIBUTE,
            item_key::WIELD_REQ_VALUE,
        ),
        (
            key::WIELD_REQ_TYPE_2,
            key::WIELD_REQ_ATTRIBUTE_2,
            key::WIELD_REQ_VALUE_2,
        ),
    ]
    .into_iter()
    .find(|(kind, attribute, _)| {
        int(*kind) == Some(LEVEL_REQUIREMENT) && int(*attribute) == Some(1)
    })
    .and_then(|(_, _, value)| int(value))
}

/// The wield requirement kind of a character level.
const LEVEL_REQUIREMENT: i32 = 7;

/// The game property ids the filters read beyond the item info line's.
mod key {
    /// ClothingPriority: the coverage bits.
    pub const COVERAGE: u32 = 4;
    /// ValidLocations: the EquipMask bits.
    pub const VALID_LOCATIONS: u32 = 9;
    /// CurrentWieldedLocation.
    pub const CURRENT_WIELDED_LOCATION: u32 = 10;
    /// RareId.
    pub const RARE_ID: u32 = 17;
    /// Bonded.
    pub const BONDED: u32 = 33;
    /// WeaponSkill.
    pub const WEAPON_SKILL: u32 = 48;
    /// Attuned.
    pub const ATTUNED: u32 = 114;
    /// WieldRequirements2.
    pub const WIELD_REQ_TYPE_2: u32 = 270;
    /// WieldSkillType2.
    pub const WIELD_REQ_ATTRIBUTE_2: u32 = 271;
    /// WieldDifficulty2.
    pub const WIELD_REQ_VALUE_2: u32 = 272;
}

impl Rating {
    /// The properties it reads (the larger is taken): the base and the
    /// Gear property where both exist.
    fn property_ids(self) -> &'static [u32] {
        match self {
            Self::DamageRating => &[307, 370],
            Self::CritDamage => &[314, 374],
            Self::HealBoost => &[323, 376],
            Self::Vitality => &[379],
            Self::ArmorLevel => &[item_key::ARMOR_LEVEL],
            Self::DamageResist => &[308, 371],
            Self::CritResist => &[315, 373],
            Self::CritDamageResist => &[316, 375],
            Self::HealingResist => &[317],
            Self::NetherResist => &[331, 377],
            Self::DotResist => &[350],
            Self::LifeResist => &[351, 378],
            Self::SneakAttack => &[356],
            Self::Recklessness => &[357],
            Self::Deception => &[358],
            Self::PkDamage => &[381],
            Self::PkDamageResist => &[382],
            Self::GearPkDamage => &[383],
            Self::GearPkResist => &[384],
            Self::Tinks => &[item_key::TINKS],
        }
    }
}
