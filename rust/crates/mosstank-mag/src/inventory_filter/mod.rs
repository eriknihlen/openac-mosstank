//! The Inventory window's filters: a port of the owner's MosswartOverlord
//! inventory search's filters (its Go service's `search.go`,
//! `spell_filter.go`, `suit_cd.go`; the page's `constants.ts`), read
//! against each row's facts ([`RowFacts`], worked out once when the row is
//! read).
//!
//! Where it differs from the website (the spec's fixes): slots by the real
//! EquipMask names ([`slots`]); a cantrip by its exact name at its tier or
//! better (the website: Legendary only, and any spell whose name shared
//! the label's words); crit, nether and life resist the larger of the base
//! and the Gear property; both wield requirement triples; no Healing
//! rating (a character property). An item never appraised is left out by
//! a cantrip, spell, rating, set, Armor type or armor crit damage
//! filter and counted
//! ([`Verdict::NotIdentified`]). The filters window's own state is
//! [`window::FilterWindow`].

pub mod cantrips;
pub mod facts;
pub mod slots;
pub mod window;

use std::collections::BTreeSet;

use ac_plugin_abi::world::ObjectClass;

use crate::item_info::names;

pub use cantrips::{CantripTier, GROUPS, cantrip_index, cantrip_name, cantrip_of, spell_matches};
pub use facts::{Rating, RowFacts};
pub use slots::slot_name;

/// Worn or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Equipped {
    /// Either.
    #[default]
    Any,
    /// Worn or wielded.
    Yes,
    /// Not.
    No,
}

/// The Item tab's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemType {
    /// Every item.
    #[default]
    All,
    /// Armor.
    Armor,
    /// Jewelry.
    Jewelry,
    /// Weapons.
    Weapons,
    /// Clothing.
    Clothing,
    /// Shirts.
    Shirts,
    /// Pants.
    Pants,
}

/// A kind of weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WeaponKind {
    /// Heavy weapons.
    Heavy,
    /// Light weapons.
    Light,
    /// Finesse weapons.
    Finesse,
    /// Two-handed combat.
    TwoHanded,
    /// Missile weapons.
    Missile,
    /// War magic casters.
    War,
    /// Void magic casters.
    Void,
}

/// A slot checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// A ring.
    Ring,
    /// A bracelet.
    Bracelet,
    /// A necklace.
    Neck,
    /// A trinket.
    Trinket,
    /// A cloak.
    Cloak,
    /// Head.
    Head,
    /// Chest.
    Chest,
    /// Abdomen.
    Abdomen,
    /// Upper arms.
    UpperArms,
    /// Lower arms.
    LowerArms,
    /// Hands.
    Hands,
    /// Upper legs.
    UpperLegs,
    /// Lower legs.
    LowerLegs,
    /// Feet.
    Feet,
    /// A shield.
    Shield,
}

/// What a filter makes of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verdict {
    /// Shown.
    #[default]
    Pass,
    /// Left out.
    Fail,
    /// Left out only because it was never appraised.
    NotIdentified,
}

/// Every filter of the filter window. The default keeps every row.
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryFilter {
    /// Worn or not.
    pub equipped: Equipped,
    /// Bonded only.
    pub bonded: bool,
    /// Attuned only.
    pub attuned: bool,
    /// Rares only.
    pub rare: bool,
    /// The type.
    pub item_type: ItemType,
    /// The kinds of weapon (any of them; none for every weapon), with
    /// [`ItemType::Weapons`] only.
    pub weapon_kinds: BTreeSet<WeaponKind>,
    /// The slots (any of them; none for every slot).
    pub slots: BTreeSet<Slot>,
    /// The cantrips the item must all have, each at its tier or better
    /// (by [`cantrip_index`]).
    pub cantrips: Vec<(usize, CantripTier)>,
    /// Text one of its spells' names contains, ignoring case.
    pub spell_contains: String,
    /// The ratings' minimums, by [`Rating::index`].
    pub ratings: [Option<i32>; Rating::ALL.len()],
    /// The crit damage tiers armor may have (CD0, CD1, CD2); all or none
    /// is no filter.
    pub allowed_cd: [bool; 3],
    /// The equipment set.
    pub set: Option<i32>,
    /// The highest wield level.
    pub max_wield: Option<i32>,
    /// The lowest value.
    pub min_value: Option<i32>,
    /// The lowest workmanship.
    pub min_workmanship: Option<i32>,
    /// The highest burden.
    pub max_burden: Option<i32>,
    /// The characters searched; none for every one.
    pub characters: Option<BTreeSet<String>>,
}

impl Default for InventoryFilter {
    fn default() -> Self {
        Self {
            equipped: Equipped::Any,
            bonded: false,
            attuned: false,
            rare: false,
            item_type: ItemType::All,
            weapon_kinds: BTreeSet::new(),
            slots: BTreeSet::new(),
            cantrips: Vec::new(),
            spell_contains: String::new(),
            ratings: [None; Rating::ALL.len()],
            allowed_cd: [true; 3],
            set: None,
            max_wield: None,
            min_value: None,
            min_workmanship: None,
            max_burden: None,
            characters: None,
        }
    }
}

impl InventoryFilter {
    /// A rating's minimum.
    #[must_use]
    pub fn rating(&self, rating: Rating) -> Option<i32> {
        self.ratings[rating.index()]
    }

    /// Sets a rating's minimum (none: no filter).
    pub fn set_rating(&mut self, rating: Rating, minimum: Option<i32>) {
        self.ratings[rating.index()] = minimum;
    }

    /// No filter is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.summary_parts().is_empty()
    }

    /// The same filter for every character: what the rows are tested
    /// against (the characters are chosen per owner, not per row).
    #[must_use]
    pub fn for_rows(&self) -> Self {
        Self {
            characters: None,
            ..self.clone()
        }
    }

    /// The row passes.
    #[must_use]
    pub fn matches(&self, facts: &RowFacts) -> bool {
        self.verdict(facts) == Verdict::Pass
    }

    /// What the filter makes of the row: the filters that need no
    /// appraisal first; an item never appraised fails the cantrip, spell,
    /// rating, set, Armor type (its armor level) and armor crit damage
    /// filters as [`Verdict::NotIdentified`].
    #[must_use]
    pub fn verdict(&self, facts: &RowFacts) -> Verdict {
        if !self.plain_matches(facts) {
            return Verdict::Fail;
        }
        if !self.needs_appraisal(facts) {
            return Verdict::Pass;
        }
        if !facts.identified {
            return Verdict::NotIdentified;
        }
        if self.appraised_matches(facts) {
            Verdict::Pass
        } else {
            Verdict::Fail
        }
    }

    /// The filters that need no appraisal pass.
    fn plain_matches(&self, facts: &RowFacts) -> bool {
        let equipped = match self.equipped {
            Equipped::Any => true,
            Equipped::Yes => facts.equipped,
            Equipped::No => !facts.equipped,
        };
        let at_most = |limit: Option<i32>, value: Option<i32>| {
            limit.is_none_or(|limit| value.is_some_and(|v| v <= limit))
        };
        let at_least = |limit: Option<i32>, value: Option<i32>| {
            limit.is_none_or(|limit| value.is_some_and(|v| v >= limit))
        };
        equipped
            && (!self.bonded || facts.bonded)
            && (!self.attuned || facts.attuned)
            && (!self.rare || facts.rare)
            && self.type_matches(facts)
            && (self.slots.is_empty() || self.slots.iter().any(|slot| slot.matches(&facts.slot)))
            && self
                .max_wield
                .is_none_or(|limit| facts.wield_level.is_none_or(|level| level <= limit))
            && at_least(self.min_value, facts.value)
            && at_least(self.min_workmanship, facts.workmanship)
            && at_most(self.max_burden, facts.burden)
    }

    /// A filter that reads the appraisal applies to the row.
    fn needs_appraisal(&self, facts: &RowFacts) -> bool {
        !self.cantrips.is_empty()
            || !self.spell_needle().is_empty()
            || self.ratings.iter().any(Option::is_some)
            || self.set.is_some()
            || (self.item_type == ItemType::Armor && facts.class == ObjectClass::Armor)
            || (self.cd_active() && slots::is_armor_slot(&facts.slot))
    }

    /// The filters that read the appraisal pass.
    fn appraised_matches(&self, facts: &RowFacts) -> bool {
        let cantrips = self.cantrips.iter().all(|&(index, tier)| {
            facts
                .cantrips
                .iter()
                .any(|&(found, found_tier)| found == index && found_tier >= tier)
        });
        let needle = self.spell_needle();
        let spell = needle.is_empty() || facts.spells.iter().any(|name| name.contains(&needle));
        let ratings = Rating::ALL.iter().all(|&rating| {
            self.rating(rating)
                .is_none_or(|minimum| facts.rating(rating).is_some_and(|v| v >= minimum))
        });
        let cd = !self.cd_active()
            || !slots::is_armor_slot(&facts.slot)
            || self.allowed_cd[usize::from(facts.cd_tier)];
        let set = self.set.is_none_or(|set| facts.set_id == Some(set));
        let armor =
            self.item_type != ItemType::Armor || facts.armor_level.is_some_and(|level| level > 0);
        cantrips && spell && ratings && cd && set && armor
    }

    /// The type and the weapon kinds pass.
    fn type_matches(&self, facts: &RowFacts) -> bool {
        let class = facts.class;
        let name = facts.name.as_str();
        let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
        let not_outer = !has(&["robe", "cloak", "pallium", "armet"]);
        let coverage = facts.coverage;
        match self.item_type {
            ItemType::All => true,
            // Its armor level is the appraisal's.
            ItemType::Armor => class == ObjectClass::Armor,
            ItemType::Jewelry => class == ObjectClass::Jewelry,
            ItemType::Weapons => {
                matches!(
                    class,
                    ObjectClass::MeleeWeapon
                        | ObjectClass::MissileWeapon
                        | ObjectClass::WandStaffOrb
                ) && (self.weapon_kinds.is_empty()
                    || self.weapon_kinds.iter().any(|kind| kind.matches(facts)))
            }
            ItemType::Clothing => {
                class == ObjectClass::Clothing
                    && not_outer
                    && has(&["shirt", "pants", "breeches", "baggy", "tunic"])
            }
            ItemType::Shirts => {
                class == ObjectClass::Clothing
                    && coverage & 8 != 0
                    && coverage & 6 != 6
                    && not_outer
                    && !has(&["pants", "breeches"])
            }
            ItemType::Pants => class == ObjectClass::Clothing && coverage & 2 == 2 && not_outer,
        }
    }

    /// The spell text, lowercase and trimmed.
    fn spell_needle(&self) -> String {
        self.spell_contains.trim().to_lowercase()
    }

    /// The allowed crit damage tiers filter: some but not all of them.
    fn cd_active(&self) -> bool {
        self.allowed_cd.iter().any(|allowed| *allowed)
            && !self.allowed_cd.iter().all(|allowed| *allowed)
    }

    /// One line naming every filter set, joined by ` · `; empty for none.
    #[must_use]
    pub fn summary(&self) -> String {
        self.summary_parts().join(" · ")
    }

    fn summary_parts(&self) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        match self.equipped {
            Equipped::Any => {}
            Equipped::Yes => parts.push("Equipped".to_owned()),
            Equipped::No => parts.push("Not equipped".to_owned()),
        }
        for (set, name) in [
            (self.bonded, "Bonded"),
            (self.attuned, "Attuned"),
            (self.rare, "Rare"),
        ] {
            if set {
                parts.push(name.to_owned());
            }
        }
        match self.item_type {
            ItemType::All => {}
            ItemType::Weapons if !self.weapon_kinds.is_empty() => {
                let kinds: Vec<&str> = self.weapon_kinds.iter().map(|kind| kind.label()).collect();
                parts.push(format!("Weapons: {}", kinds.join(", ")));
            }
            other => parts.push(other.label().to_owned()),
        }
        if !self.slots.is_empty() {
            let slots: Vec<&str> = self.slots.iter().map(|slot| slot.label()).collect();
            parts.push(slots.join(", "));
        }
        for &(index, tier) in &self.cantrips {
            if let Some(name) = cantrip_name(index) {
                parts.push(format!("{} {name}", tier.name()));
            }
        }
        let needle = self.spell_contains.trim();
        if !needle.is_empty() {
            parts.push(format!("Spell \"{needle}\""));
        }
        for rating in Rating::ALL {
            if let Some(minimum) = self.rating(rating) {
                parts.push(format!("{} ≥ {minimum}", rating.label()));
            }
        }
        if self.cd_active() {
            let tiers: Vec<String> = (0..3)
                .filter(|tier| self.allowed_cd[*tier])
                .map(|tier| format!("CD{tier}"))
                .collect();
            parts.push(tiers.join(", "));
        }
        if let Some(set) = self.set {
            parts.push(set_name(set));
        }
        for (value, label) in [
            (self.max_wield, "Wield ≤"),
            (self.min_value, "Value ≥"),
            (self.min_workmanship, "Work ≥"),
            (self.max_burden, "Burden ≤"),
        ] {
            if let Some(value) = value {
                parts.push(format!("{label} {value}"));
            }
        }
        match &self.characters {
            None => {}
            Some(names) if names.len() == 1 => {
                parts.extend(names.iter().cloned());
            }
            Some(names) => parts.push(format!("{} characters", names.len())),
        }
        parts
    }
}

/// An equipment set's name, `Set <id>` when the table has none.
#[must_use]
pub fn set_name(id: i32) -> String {
    names::attribute_set(id).map_or_else(|| format!("Set {id}"), str::to_owned)
}

impl Equipped {
    /// Every choice, in the Item tab's order.
    pub const ALL: [Self; 3] = [Self::Any, Self::Yes, Self::No];
}

impl ItemType {
    /// Every type, in the Item tab's order.
    pub const ALL: [Self; 7] = [
        Self::All,
        Self::Armor,
        Self::Jewelry,
        Self::Weapons,
        Self::Clothing,
        Self::Shirts,
        Self::Pants,
    ];

    /// Its label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Armor => "Armor",
            Self::Jewelry => "Jewelry",
            Self::Weapons => "Weapons",
            Self::Clothing => "Clothing",
            Self::Shirts => "Shirts",
            Self::Pants => "Pants",
        }
    }
}

impl WeaponKind {
    /// Every kind, in the Item tab's order.
    pub const ALL: [Self; 7] = [
        Self::Heavy,
        Self::Light,
        Self::Finesse,
        Self::TwoHanded,
        Self::Missile,
        Self::War,
        Self::Void,
    ];

    /// Its label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Heavy => "Heavy",
            Self::Light => "Light",
            Self::Finesse => "Finesse",
            Self::TwoHanded => "Two-handed",
            Self::Missile => "Missile",
            Self::War => "War Magic",
            Self::Void => "Void Magic",
        }
    }

    /// The weapon is of this kind: a melee or missile weapon by its weapon
    /// skill, a caster by its wield skill.
    fn matches(self, facts: &RowFacts) -> bool {
        let (class, skill) = match self {
            Self::Heavy => (ObjectClass::MeleeWeapon, 44),
            Self::Light => (ObjectClass::MeleeWeapon, 45),
            Self::Finesse => (ObjectClass::MeleeWeapon, 46),
            Self::TwoHanded => (ObjectClass::MeleeWeapon, 41),
            Self::Missile => (ObjectClass::MissileWeapon, 47),
            Self::War => (ObjectClass::WandStaffOrb, 34),
            Self::Void => (ObjectClass::WandStaffOrb, 43),
        };
        facts.class == class && facts.weapon_skill == skill
    }
}

impl Slot {
    /// Every slot, in the Item tab's order: the jewelry, then the armor.
    pub const ALL: [Self; 15] = [
        Self::Ring,
        Self::Bracelet,
        Self::Neck,
        Self::Trinket,
        Self::Cloak,
        Self::Head,
        Self::Chest,
        Self::Abdomen,
        Self::UpperArms,
        Self::LowerArms,
        Self::Hands,
        Self::UpperLegs,
        Self::LowerLegs,
        Self::Feet,
        Self::Shield,
    ];

    /// Its label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ring => "Ring",
            Self::Bracelet => "Bracelet",
            Self::Neck => "Neck",
            Self::Trinket => "Trinket",
            Self::Cloak => "Cloak",
            Self::Head => "Head",
            Self::Chest => "Chest",
            Self::Abdomen => "Abdomen",
            Self::UpperArms => "Upper Arms",
            Self::LowerArms => "Lower Arms",
            Self::Hands => "Hands",
            Self::UpperLegs => "Upper Legs",
            Self::LowerLegs => "Lower Legs",
            Self::Feet => "Feet",
            Self::Shield => "Shield",
        }
    }

    /// An item of this slot name is worn here: a jewelry slot by a word of
    /// it (either ring, either wrist), an armor slot by one of its parts.
    fn matches(self, slot: &str) -> bool {
        match self {
            Self::Ring => slot.contains("Ring"),
            Self::Bracelet => slot.contains("Wrist") || slot.contains("Bracelet"),
            Self::Neck => slot.contains("Neck"),
            Self::Trinket => slot.contains("Trinket"),
            Self::Cloak => slot.contains("Cloak"),
            other => slots::parts(slot).any(|part| part == other.label()),
        }
    }
}
