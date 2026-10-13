//! Weapons: which element one strikes with, what kind of launcher it is,
//! and which profiled weapon a rule that names none reaches for.

use ac_plugin_abi::items::EquipmentItem;
use ac_plugin_abi::world::ObjectClass;

use crate::damage::MonsterDamageType;

/// What kind of missile weapon an item is (the ammunition table's launcher
/// numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissileKind {
    /// Not a missile weapon, or one of no kind the table knows.
    None = 0,
    /// A bow.
    Bow = 5,
    /// A crossbow.
    Crossbow = 6,
    /// An atlatl.
    Atlatl = 7,
    /// A thrown weapon.
    Thrown = 8,
}

/// The item's missile kind: its class must be a missile weapon, and its
/// ammunition type exactly one the table knows.
#[must_use]
pub fn missile_kind(item: &EquipmentItem) -> MissileKind {
    if item.object_class != ObjectClass::MissileWeapon {
        return MissileKind::None;
    }
    match item.ammo_type {
        1 => MissileKind::Bow,
        2 => MissileKind::Crossbow,
        4 => MissileKind::Atlatl,
        0 => MissileKind::Thrown,
        _ => MissileKind::None,
    }
}

/// The launcher type the ammunition table files arrows under: the missile
/// kind of a bow, crossbow or atlatl, 0 for anything else.
#[must_use]
pub fn launcher_type(item: &EquipmentItem) -> i32 {
    match missile_kind(item) {
        kind @ (MissileKind::Bow | MissileKind::Crossbow | MissileKind::Atlatl) => kind as i32,
        _ => 0,
    }
}

/// Whether the item is a wand, staff or orb.
#[must_use]
pub fn is_caster(item: &EquipmentItem) -> bool {
    item.object_class == ObjectClass::WandStaffOrb
}

/// The element a weapon strikes with: the imbue burned into it, else the
/// resistance it cleaves, else the damage it plainly deals; `None` when none
/// of the three says anything.
pub mod element {
    use super::MonsterDamageType;

    const CRITICAL_STRIKE: i32 = 0x0001;
    const CRIPPLING_BLOW: i32 = 0x0002;
    const ARMOR_REND: i32 = 0x0004;
    const SLASH_REND: i32 = 0x0008;
    const PIERCE_REND: i32 = 0x0010;
    const BLUDGEON_REND: i32 = 0x0020;
    const ACID_REND: i32 = 0x0040;
    const COLD_REND: i32 = 0x0080;
    const LIGHTNING_REND: i32 = 0x0100;
    const FIRE_REND: i32 = 0x0200;
    const REND_MASK: i32 = CRITICAL_STRIKE
        | CRIPPLING_BLOW
        | ARMOR_REND
        | SLASH_REND
        | PIERCE_REND
        | BLUDGEON_REND
        | ACID_REND
        | COLD_REND
        | LIGHTNING_REND
        | FIRE_REND;

    const SLASH: i32 = 0x0001;
    const PIERCE: i32 = 0x0002;
    const BLUDGEON: i32 = 0x0004;
    const COLD: i32 = 0x0008;
    const FIRE: i32 = 0x0010;
    const ACID: i32 = 0x0020;
    const LIGHTNING: i32 = 0x0040;
    const NETHER: i32 = 0x0400;

    /// The element. A weapon carrying two rends, or a rend beside a critical
    /// bonus, names no single element by its imbue and falls through.
    #[must_use]
    pub fn resolve(
        imbued_effect: i32,
        resistance_cleaving: i32,
        damage_type: i32,
    ) -> MonsterDamageType {
        match imbued_effect & REND_MASK {
            ACID_REND => return MonsterDamageType::Acid,
            BLUDGEON_REND => return MonsterDamageType::Bludgeon,
            COLD_REND => return MonsterDamageType::Cold,
            FIRE_REND => return MonsterDamageType::Fire,
            LIGHTNING_REND => return MonsterDamageType::Electric,
            PIERCE_REND => return MonsterDamageType::Pierce,
            SLASH_REND => return MonsterDamageType::Slash,
            _ => {}
        }
        let cleaving = [
            (ACID, MonsterDamageType::Acid),
            (BLUDGEON, MonsterDamageType::Bludgeon),
            (COLD, MonsterDamageType::Cold),
            (FIRE, MonsterDamageType::Fire),
            (LIGHTNING, MonsterDamageType::Electric),
            (PIERCE, MonsterDamageType::Pierce),
            (SLASH, MonsterDamageType::Slash),
        ];
        if let Some(&(_, element)) = cleaving
            .iter()
            .find(|(bit, _)| resistance_cleaving & bit != 0)
        {
            return element;
        }
        // The plain damage is asked in a different order: slash before pierce.
        let plain = [
            (ACID, MonsterDamageType::Acid),
            (BLUDGEON, MonsterDamageType::Bludgeon),
            (COLD, MonsterDamageType::Cold),
            (FIRE, MonsterDamageType::Fire),
            (LIGHTNING, MonsterDamageType::Electric),
            (SLASH, MonsterDamageType::Slash),
            (PIERCE, MonsterDamageType::Pierce),
            (NETHER, MonsterDamageType::VoidBasic),
        ];
        plain
            .iter()
            .find(|(bit, _)| damage_type & bit != 0)
            .map_or(MonsterDamageType::None, |&(_, element)| element)
    }
}

/// Which weapon a rule that names none reaches for: a slayer for what is
/// being fought wins outright, then the best-rated weapon of an element the
/// monster is already vulnerable to, then one whose imbue or cleave gives it
/// a wanted element, then the best-rated one, and only at the very end
/// anything at all that can be wielded.
pub mod ladder {
    use std::collections::HashMap;

    use ac_plugin_abi::items::EquipmentItem;

    use super::{MonsterDamageType, element, is_caster, launcher_type};

    /// Only these slots hold something to fight with.
    const WEAPON_READY_MASK: u32 = 0x0350_0000;
    /// The damage bit that makes a wand a void caster.
    const NETHER_DAMAGE: i32 = 0x0400;

    const ARMOR_REND_SCORE: i32 = 4000;
    const CRITICAL_STRIKE_SCORE: i32 = 2000;
    const CRIPPLING_BLOW_SCORE: i32 = 1000;
    const ARMOR_CLEAVING_SCORE: i32 = 400;
    const CRUSHING_BLOW_SCORE: i32 = 200;
    const BITING_STRIKE_SCORE: i32 = 100;

    const ARMOR_REND_BIT: i32 = 0x0004;
    const CRITICAL_STRIKE_BIT: i32 = 0x0001;
    const CRIPPLING_BLOW_BIT: i32 = 0x0002;

    /// What a weapon's imbue and quest bonuses add up to; zero brings
    /// nothing beyond its element.
    #[must_use]
    pub fn rating_of(item: &EquipmentItem) -> i32 {
        let mut score = 0;
        if item.imbued_effect & ARMOR_REND_BIT != 0 {
            score += ARMOR_REND_SCORE;
        }
        if item.imbued_effect & CRITICAL_STRIKE_BIT != 0 {
            score += CRITICAL_STRIKE_SCORE;
        }
        if item.imbued_effect & CRIPPLING_BLOW_BIT != 0 {
            score += CRIPPLING_BLOW_SCORE;
        }
        if item.armor_cleaving {
            score += ARMOR_CLEAVING_SCORE;
        }
        if item.crushing_blow {
            score += CRUSHING_BLOW_SCORE;
        }
        if item.biting_strike {
            score += BITING_STRIKE_SCORE;
        }
        score
    }

    /// What [`select`] is asked.
    #[allow(missing_debug_implementations)]
    pub struct Request<'a> {
        /// Everything the character owns.
        pub items: &'a [EquipmentItem],
        /// Whether an item is on the Items page.
        pub is_profiled: &'a dyn Fn(&EquipmentItem) -> bool,
        /// The elements the fight wants, best first.
        pub wanted: &'a [MonsterDamageType],
        /// The monster's species, or -1 when unknown.
        pub species: i32,
        /// Whether a weapon of this kind can put this element on the
        /// monster (only a launcher can answer no, for want of ammunition).
        pub can_deliver: &'a dyn Fn(&EquipmentItem, MonsterDamageType) -> bool,
        /// Whether the monster already carries that element's vulnerability.
        pub already_vulnerable: &'a dyn Fn(MonsterDamageType) -> bool,
        /// Whether the character can cast war magic.
        pub war_trained: bool,
        /// Whether the character can cast void magic.
        pub void_trained: bool,
        /// An item the rule already took as its off-hand pick, or 0.
        pub exclude_object_id: u32,
    }

    /// The pick and whether it was the last resort (nothing on the page
    /// suited the monster, so the last item was taken regardless; the
    /// reference says so once).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Selection {
        /// The weapon's object id, 0 for none.
        pub object_id: u32,
        /// Whether this was the last resort.
        pub last_resort: bool,
    }

    /// `Dictionary.TryGetValue` after a `Record` that only appends an
    /// element to its order the first time the map lacks it.
    fn record(
        map: &mut HashMap<MonsterDamageType, u32>,
        order: &mut Vec<MonsterDamageType>,
        element: MonsterDamageType,
        object_id: u32,
    ) {
        if !map.contains_key(&element) {
            order.push(element);
        }
        map.insert(element, object_id);
    }

    fn first_common(
        wanted: &[MonsterDamageType],
        available: &[MonsterDamageType],
    ) -> Option<MonsterDamageType> {
        wanted.iter().copied().find(|c| available.contains(c))
    }

    /// Picks a weapon out of the profiled ones.
    #[must_use]
    pub fn select(request: &Request<'_>) -> Selection {
        let wanted = request.wanted;
        let can_deliver = request.can_deliver;
        let mut by_imbue: HashMap<MonsterDamageType, u32> = HashMap::new();
        let mut imbue_order = Vec::new();
        let mut by_rating: HashMap<MonsterDamageType, u32> = HashMap::new();
        let mut rating_of_element: HashMap<MonsterDamageType, i32> = HashMap::new();
        let mut rating_order = Vec::new();
        let mut by_element: HashMap<MonsterDamageType, u32> = HashMap::new();
        let mut element_order = Vec::new();

        let mut slayer = 0u32;
        let mut caster_fallback = 0u32;
        let mut any_launcher = 0u32;
        let mut last = 0u32;
        let mut last_unusable = 0u32;

        for item in request.items {
            if !(request.is_profiled)(item)
                || item.valid_locations & WEAPON_READY_MASK == 0
                || (request.exclude_object_id != 0 && item.object_id == request.exclude_object_id)
            {
                continue;
            }
            let launcher = launcher_type(item) != 0;
            let caster = is_caster(item);

            if caster {
                // Remembered as the last resort even when its school is
                // untrained; it stands behind any weapon that can attack.
                last_unusable = item.object_id;
                if (request.war_trained || request.void_trained)
                    && request.species >= 0
                    && item.slayer_creature_type == request.species
                {
                    slayer = item.object_id;
                }
                let nether = item.damage_type & NETHER_DAMAGE != 0;
                if if nether {
                    !request.void_trained
                } else {
                    !request.war_trained
                } {
                    continue;
                }
            }
            if launcher {
                any_launcher = item.object_id;
            }
            if request.species >= 0 && item.slayer_creature_type == request.species {
                slayer = item.object_id;
            }

            let imbued = element::resolve(item.imbued_effect, item.resistance_cleaving, 0);
            if imbued != MonsterDamageType::None && can_deliver(item, imbued) {
                record(&mut by_imbue, &mut imbue_order, imbued, item.object_id);
            }

            let mut strikes = element::resolve(
                item.imbued_effect,
                item.resistance_cleaving,
                item.damage_type,
            );
            let elementless = strikes == MonsterDamageType::None;
            if elementless && launcher {
                strikes = MonsterDamageType::Pierce;
            }
            if strikes != MonsterDamageType::None && can_deliver(item, strikes) {
                record(&mut by_element, &mut element_order, strikes, item.object_id);
            }

            let rating = rating_of(item);
            if rating > 0 {
                if strikes != MonsterDamageType::None
                    && can_deliver(item, strikes)
                    && rating > rating_of_element.get(&strikes).copied().unwrap_or(0)
                {
                    // As in the C#: the map is set before the order is
                    // recorded, so the order only ever learns of elements
                    // the map already holds (it stays empty).
                    by_rating.insert(strikes, item.object_id);
                    rating_of_element.insert(strikes, rating);
                    record(&mut by_rating, &mut rating_order, strikes, item.object_id);
                }
                if launcher && elementless {
                    // A plain bow shoots whatever the quiver holds, so a
                    // well-rated one stands in for every element it can feed.
                    for &candidate in wanted {
                        if candidate == strikes
                            || !can_deliver(item, candidate)
                            || rating <= rating_of_element.get(&candidate).copied().unwrap_or(0)
                        {
                            continue;
                        }
                        by_rating.insert(candidate, item.object_id);
                        rating_of_element.insert(candidate, rating);
                        record(&mut by_rating, &mut rating_order, candidate, item.object_id);
                    }
                }
                if caster && elementless {
                    caster_fallback = item.object_id;
                }
            }
            last = item.object_id;
        }

        let pick = |object_id: u32| Selection {
            object_id,
            last_resort: false,
        };
        if slayer != 0 {
            return pick(slayer);
        }
        for &candidate in wanted {
            if (request.already_vulnerable)(candidate)
                && let Some(&vulnerable) = by_rating.get(&candidate)
            {
                return pick(vulnerable);
            }
        }
        if let Some(hit) = first_common(wanted, &imbue_order) {
            return pick(by_imbue[&hit]);
        }
        if let Some(&void_weapon) = by_imbue.get(&MonsterDamageType::VoidBasic) {
            return pick(void_weapon);
        }
        if let Some(hit) = first_common(wanted, &rating_order) {
            return pick(by_rating[&hit]);
        }
        if caster_fallback != 0 {
            return pick(caster_fallback);
        }
        if let Some(hit) = first_common(wanted, &element_order) {
            return pick(by_element[&hit]);
        }
        if any_launcher != 0 {
            return pick(any_launcher);
        }
        Selection {
            object_id: if last != 0 { last } else { last_unusable },
            last_resort: last != 0 || last_unusable != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::items::EquipmentItem;
    use ac_plugin_abi::world::ObjectClass;

    use super::ladder::{Request, rating_of, select};
    use super::*;

    fn weapon(id: u32, name: &str, damage_type: i32) -> EquipmentItem {
        EquipmentItem {
            object_id: id,
            name: name.to_owned(),
            item_type: 0x1,
            valid_locations: 0x0010_0000,
            equipped_location: 0,
            container_id: 1,
            wielder_id: 0,
            combat_use: 1,
            damage_type,
            weapon_skill: 44,
            damage: 20,
            damage_variance: 0.25,
            object_class: ObjectClass::MeleeWeapon,
            ..EquipmentItem::default()
        }
    }

    fn caster(id: u32, name: &str, damage_type: i32) -> EquipmentItem {
        EquipmentItem {
            item_type: 0x0000_8000,
            object_class: ObjectClass::WandStaffOrb,
            ..weapon(id, name, damage_type)
        }
    }

    fn launcher(ammo_type: u32) -> EquipmentItem {
        EquipmentItem {
            object_id: 1,
            name: "Launcher".to_owned(),
            item_type: 0x100,
            valid_locations: 0x0010_0000,
            equipped_location: 0,
            container_id: 1,
            wielder_id: 0,
            combat_use: 2,
            damage_type: 0,
            weapon_skill: 47,
            damage: 1,
            damage_variance: 0.1,
            object_class: ObjectClass::MissileWeapon,
            ammo_type,
            ..EquipmentItem::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        items: &[EquipmentItem],
        is_profiled: &dyn Fn(&EquipmentItem) -> bool,
        wanted: &[MonsterDamageType],
        species: i32,
        already_vulnerable: &dyn Fn(MonsterDamageType) -> bool,
        war_trained: bool,
        void_trained: bool,
        exclude_object_id: u32,
    ) -> u32 {
        select(&Request {
            items,
            is_profiled,
            wanted,
            species,
            can_deliver: &|_, _| true,
            already_vulnerable,
            war_trained,
            void_trained,
            exclude_object_id,
        })
        .object_id
    }

    fn pick(items: &[EquipmentItem], wanted: MonsterDamageType) -> u32 {
        run(items, &|_| true, &[wanted], -1, &|_| false, true, true, 0)
    }

    fn pick_with_schools(
        items: &[EquipmentItem],
        war_trained: bool,
        void_trained: bool,
        wanted: MonsterDamageType,
    ) -> u32 {
        run(
            items,
            &|_| true,
            &[wanted],
            -1,
            &|_| false,
            war_trained,
            void_trained,
            0,
        )
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: AnImbuedWeaponOfTheWantedElementBeatsAPlainerHarderOne
    #[test]
    fn an_imbued_weapon_of_the_wanted_element_beats_a_plainer_harder_one() {
        let plain = EquipmentItem {
            damage: 90,
            ..weapon(1, "Heavy Sword", 0x0001)
        };
        let imbued = EquipmentItem {
            damage: 20,
            imbued_effect: 0x0200,
            ..weapon(2, "Fire Rending Dagger", 0x0002)
        };
        assert_eq!(pick(&[imbued, plain], MonsterDamageType::Fire), 2);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: AWeaponWhoseElementComesOnlyFromACleaveStillQualifies
    #[test]
    fn a_weapon_whose_element_comes_only_from_a_cleave_still_qualifies() {
        let plain = EquipmentItem {
            damage: 90,
            ..weapon(1, "Heavy Sword", 0x0001)
        };
        let cleaver = EquipmentItem {
            damage: 20,
            resistance_cleaving: 0x0020,
            ..weapon(2, "Acid Cleaver", 0x0001)
        };
        assert_eq!(pick(&[cleaver, plain], MonsterDamageType::Acid), 2);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: ASlayerForTheSpeciesBeingFoughtWinsOutright
    #[test]
    fn a_slayer_for_the_species_being_fought_wins_outright() {
        let fire = EquipmentItem {
            imbued_effect: 0x0200,
            ..weapon(1, "Fire Rending Dagger", 0x0010)
        };
        let slayer = EquipmentItem {
            slayer_creature_type: 12,
            ..weapon(2, "Olthoi Slayer", 0x0001)
        };
        let items = [fire, slayer];
        let fire_only = [MonsterDamageType::Fire];
        assert_eq!(
            run(&items, &|_| true, &fire_only, 12, &|_| false, true, true, 0),
            2
        );
        // Fighting something else, the slayer means nothing.
        assert_eq!(
            run(&items, &|_| true, &fire_only, 25, &|_| false, true, true, 0),
            1
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: TheRatingWeightsAreTheOnesTheReferenceMacroUses
    #[test]
    fn the_rating_weights_are_the_ones_the_reference_macro_uses() {
        assert_eq!(rating_of(&weapon(1, "Plain", 0x0001)), 0);
        let with_imbue = |imbue| EquipmentItem {
            imbued_effect: imbue,
            ..weapon(1, "Imbued", 0x0001)
        };
        assert_eq!(rating_of(&with_imbue(0x0004)), 4000);
        assert_eq!(rating_of(&with_imbue(0x0001)), 2000);
        assert_eq!(rating_of(&with_imbue(0x0002)), 1000);
        assert_eq!(
            rating_of(&EquipmentItem {
                armor_cleaving: true,
                crushing_blow: true,
                biting_strike: true,
                ..weapon(1, "Quest", 0x0001)
            }),
            700
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: AnElementTheMonsterAlreadyCarriesTakesTheBestRatedWeapon
    #[test]
    fn an_element_the_monster_already_carries_takes_the_best_rated_weapon() {
        let imbued = EquipmentItem {
            imbued_effect: 0x0200,
            ..weapon(1, "Fire Rending Dagger", 0x0002)
        };
        let rated = EquipmentItem {
            imbued_effect: 0x0004,
            ..weapon(2, "Rending Axe", 0x0010)
        };
        let items = [imbued, rated];
        let fire = [MonsterDamageType::Fire];
        assert_eq!(
            run(
                &items,
                &|_| true,
                &fire,
                -1,
                &|e| e == MonsterDamageType::Fire,
                true,
                true,
                0
            ),
            2
        );
        assert_eq!(
            run(&items, &|_| true, &fire, -1, &|_| false, true, true, 0),
            1
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: OnlyProfiledWeaponsAreConsidered
    #[test]
    fn only_profiled_weapons_are_considered() {
        let profiled = weapon(1, "Listed", 0x0010);
        let stranger = EquipmentItem {
            imbued_effect: 0x0200,
            ..weapon(2, "Unlisted", 0x0010)
        };
        assert_eq!(
            run(
                &[profiled, stranger],
                &|item| item.name == "Listed",
                &[MonsterDamageType::Fire],
                -1,
                &|_| false,
                true,
                true,
                0
            ),
            1
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: AWandNeedsTheSchoolItCastsFrom
    #[test]
    fn a_wand_needs_the_school_it_casts_from() {
        let wand = caster(1, "Fire Wand", 0x0010);
        let sword = weapon(3, "Heavy Sword", 0x0001);
        let both = [wand.clone(), sword.clone()];
        assert_eq!(
            pick_with_schools(&both, false, false, MonsterDamageType::Fire),
            3
        );
        assert_eq!(
            pick_with_schools(&both, true, false, MonsterDamageType::Fire),
            1
        );

        let void_wand = caster(2, "Void Wand", 0x0400);
        let void_pair = [void_wand, sword];
        assert_eq!(
            pick_with_schools(&void_pair, true, false, MonsterDamageType::VoidBasic),
            3
        );
        assert_eq!(
            pick_with_schools(&void_pair, false, true, MonsterDamageType::VoidBasic),
            2
        );

        assert_eq!(
            pick_with_schools(&[wand], false, false, MonsterDamageType::Fire),
            1
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankWeaponLadderTests.cs :: TheRulesOffHandPickIsNotTakenByTheMainHandToo
    #[test]
    fn the_rules_off_hand_pick_is_not_taken_by_the_main_hand_too() {
        let best = EquipmentItem {
            imbued_effect: 0x0200,
            ..weapon(1, "Fire Rending Dagger", 0x0010)
        };
        let other = weapon(2, "Plain Dagger", 0x0010);
        let items = [best, other];
        assert_eq!(pick(&items, MonsterDamageType::Fire), 1);
        assert_eq!(
            run(
                &items,
                &|_| true,
                &[MonsterDamageType::Fire],
                -1,
                &|_| false,
                true,
                true,
                1
            ),
            2
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: LauncherTypeFollowsTheAmmoType
    #[test]
    fn launcher_type_follows_the_ammo_type() {
        assert_eq!(launcher_type(&launcher(1)), 5);
        assert_eq!(launcher_type(&launcher(2)), 6);
        assert_eq!(launcher_type(&launcher(4)), 7);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: LauncherKindRequiresMissileClassAndAnExactAmmoType
    #[test]
    fn launcher_kind_requires_missile_class_and_an_exact_ammo_type() {
        assert_eq!(
            launcher_type(&EquipmentItem {
                object_class: ObjectClass::MeleeWeapon,
                ..launcher(1)
            }),
            0
        );
        assert_eq!(launcher_type(&launcher(0x80)), 0);
        assert_eq!(missile_kind(&launcher(0)), MissileKind::Thrown);
    }
}
