//! The Inventory window's filters: each row's facts, the cantrip table, and
//! the filter itself.
//!
//! Source: the owner's MosswartOverlord inventory search (its Go service's
//! `slotname.go`, `search.go`, `spell_filter.go`, `suit_cd.go` and their
//! tests; the page's `constants.ts`), with the spec's fixes: the real
//! EquipMask table for slots, cantrips by their exact name at a tier or
//! better, the larger of a rating's base and Gear property for crit,
//! nether and life resist, both wield requirement triples.

use std::collections::BTreeSet;
use std::rc::Rc;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WeaponProfile};
use mosstank_mag::inventory_filter::{
    CantripTier, Equipped, GROUPS, InventoryFilter, ItemType, Rating, RowFacts, Slot, Verdict,
    WeaponKind, cantrip_index, cantrip_of, slot_name, spell_matches,
};

// --- Slots (slotname.go) ---

#[test]
fn a_slot_is_named_from_the_equip_mask_table() {
    assert_eq!(slot_name(0, 0, false), "-");
    assert_eq!(slot_name(1, 0, false), "Head");
    assert_eq!(slot_name(32_768, 0, false), "Neck");
    assert_eq!(slot_name(30, 0, false), "Shirt");
    assert_eq!(slot_name(1_048_576, 0, false), "Melee Weapon");
    assert_eq!(slot_name(4_194_304, 0, false), "Missile Weapon");
    assert_eq!(slot_name(67_108_864, 0, false), "Trinket");
    assert_eq!(slot_name(134_217_728, 0, false), "Cloak");
    // The website's filter called this "Shield"; it is two-handed.
    assert_eq!(slot_name(33_554_432, 0, false), "Two-Handed");
    assert_eq!(slot_name(2_097_152, 0, false), "Shield");
    assert_eq!(slot_name(268_435_456, 0, false), "Aetheria Blue");
    assert_eq!(slot_name(1_073_741_824, 0, false), "Aetheria Red");
}

#[test]
fn either_finger_or_either_wrist_reads_as_one_slot() {
    assert_eq!(slot_name(262_144 | 524_288, 0, false), "Ring");
    assert_eq!(slot_name(65_536 | 131_072, 0, false), "Bracelet");
    assert_eq!(slot_name(262_144, 0, false), "Left Ring");
}

#[test]
fn lower_legs_and_feet_read_as_feet() {
    assert_eq!(slot_name(384, 0, false), "Feet");
    assert_eq!(slot_name(384, 0, true), "Feet");
}

#[test]
fn several_armor_bits_without_a_material_are_named_bit_by_bit() {
    let hauberk = 512 | 1024 | 2048 | 4096;
    let coverage = 1024 | 2048 | 4096 | 8192;
    assert_eq!(
        slot_name(hauberk, coverage, false),
        "Chest, Abdomen, Upper Arms, Lower Arms"
    );
}

#[test]
fn loot_armor_is_reduced_by_its_coverage() {
    // Outerwear chest, abdomen, upper and lower arms: a chest piece.
    assert_eq!(
        slot_name(512 | 1024 | 2048 | 4096, 1024 | 2048 | 4096 | 8192, true),
        "Chest"
    );
    // Upper and lower arms: either.
    assert_eq!(
        slot_name(2048 | 4096, 4096 | 8192, true),
        "Upper Arms, Lower Arms"
    );
    // Abdomen, upper and lower legs: any of them.
    assert_eq!(
        slot_name(1024 | 8192 | 16_384, 2048 | 256 | 512, true),
        "Abdomen, Upper Legs, Lower Legs"
    );
    // Abdomen and upper legs: abdomen.
    assert_eq!(slot_name(1024 | 8192, 2048 | 256, true), "Abdomen");
    // Lower legs and feet coverage: feet.
    assert_eq!(slot_name(16_384 | 256, 512 | 65_536, true), "Feet");
    // One coverage bit: its slot.
    assert_eq!(slot_name(512 | 1024, 1024, true), "Chest");
}

// --- Cantrips (constants.ts, spell_filter.go) ---

#[test]
fn the_cantrip_table_is_the_websites_six_groups_and_47_names() {
    let names: Vec<&str> = GROUPS.iter().map(|group| group.name).collect();
    assert_eq!(
        names,
        [
            "Attributes",
            "Weapon skills",
            "Magic",
            "Utility",
            "Defense",
            "Wards & Banes"
        ]
    );
    let count: usize = GROUPS.iter().map(|group| group.cantrips.len()).sum();
    assert_eq!(count, 47);
    assert_eq!(GROUPS[0].cantrips[0], "Strength");
    assert_eq!(GROUPS[5].cantrips[8], "Storm Bane");
    assert_eq!(cantrip_index("Strength"), Some(0));
    assert_eq!(cantrip_index("Storm Bane"), Some(46));
    assert_eq!(cantrip_index("Prowess"), None);
}

#[test]
fn a_cantrip_matches_its_exact_name_at_the_tier_or_better() {
    let strength = cantrip_index("Strength").unwrap();
    // Legendary Strength, spell 6107.
    assert!(spell_matches(
        "Legendary Strength",
        strength,
        CantripTier::Epic
    ));
    assert!(spell_matches(
        "legendary strength",
        strength,
        CantripTier::Legendary
    ));
    assert!(spell_matches("Epic Strength", strength, CantripTier::Epic));
    assert!(!spell_matches(
        "Major Strength",
        strength,
        CantripTier::Epic
    ));
    assert!(spell_matches(
        "Minor Strength",
        strength,
        CantripTier::Minor
    ));
    assert!(!spell_matches(
        "Strength Self VI",
        strength,
        CantripTier::Minor
    ));
    assert_eq!(
        cantrip_of("Epic Invulnerability"),
        Some((cantrip_index("Invulnerability").unwrap(), CantripTier::Epic))
    );
}

#[test]
fn a_spell_sharing_a_word_with_a_cantrip_does_not_match_it() {
    // "Prowess" (2391), "Blood Thirst" (2486), "Flame Ward" (2433) and
    // "Legendary Armor Tinkering Expertise" (6042) were the website's
    // false positives.
    for index in 0..47 {
        assert!(!spell_matches("Prowess", index, CantripTier::Minor));
    }
    assert_eq!(cantrip_of("Prowess"), None);
    assert_eq!(cantrip_of("Blood Thirst"), None);
    assert_eq!(cantrip_of("Flame Ward"), None);
    let armor = cantrip_index("Armor").unwrap();
    assert!(!spell_matches(
        "Legendary Armor Tinkering Expertise",
        armor,
        CantripTier::Minor
    ));
    assert!(spell_matches("Legendary Armor", armor, CantripTier::Minor));
}

// --- Row facts ---

fn item(class: ObjectClass) -> InventoryItem {
    InventoryItem {
        object_id: 0x8000_0001,
        name: "Thing".to_owned(),
        object_class: class,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn props(ints: &[(u32, i32)]) -> ItemProperties {
    ItemProperties {
        ints: ints.to_vec(),
        ..ItemProperties::default()
    }
}

fn facts(item: &InventoryItem, ints: &[(u32, i32)]) -> RowFacts {
    RowFacts::new(item, &props(ints), true, Vec::new())
}

fn spells(names: &[&str]) -> Vec<Rc<str>> {
    names
        .iter()
        .map(|name| Rc::from(name.to_lowercase()))
        .collect()
}

#[test]
fn equipped_is_a_wielded_location() {
    let mut ring = item(ObjectClass::Jewelry);
    assert!(!facts(&ring, &[]).equipped);
    assert!(facts(&ring, &[(10, 262_144)]).equipped);
    ring.equipped_location = 262_144;
    assert!(facts(&ring, &[]).equipped);
}

#[test]
fn bonded_attuned_and_rare_are_their_properties() {
    let ring = item(ObjectClass::Jewelry);
    let plain = facts(&ring, &[]);
    assert!(!plain.bonded && !plain.attuned && !plain.rare);
    let marked = facts(&ring, &[(33, 1), (114, 1), (17, 2_045)]);
    assert!(marked.bonded && marked.attuned && marked.rare);
    assert!(!facts(&ring, &[(33, 0), (114, 0), (17, 0)]).bonded);
}

#[test]
fn the_class_armor_level_and_coverage_are_kept() {
    let mut coat = item(ObjectClass::Armor);
    coat.coverage_mask = 1024 | 2048;
    let facts = facts(&coat, &[(28, 412)]);
    assert_eq!(facts.class, ObjectClass::Armor);
    assert_eq!(facts.armor_level, Some(412));
    assert_eq!(facts.coverage, 1024 | 2048);
    let saved = RowFacts::new(
        &item(ObjectClass::Armor),
        &props(&[(4, 1024)]),
        true,
        vec![],
    );
    assert_eq!(
        saved.coverage, 1024,
        "the coverage property when not described"
    );
}

#[test]
fn the_weapon_skill_is_the_profiles_and_a_casters_its_wield_skill() {
    let sword = item(ObjectClass::MeleeWeapon);
    let profile = ItemProperties {
        weapon: Some(WeaponProfile {
            weapon_skill: 44,
            ..WeaponProfile::default()
        }),
        ..ItemProperties::default()
    };
    assert_eq!(
        RowFacts::new(&sword, &profile, true, vec![]).weapon_skill,
        44
    );
    assert_eq!(facts(&sword, &[(48, 46)]).weapon_skill, 46);
    let mut described = item(ObjectClass::MissileWeapon);
    described.weapon_skill = 47;
    assert_eq!(facts(&described, &[]).weapon_skill, 47);
    let wand = item(ObjectClass::WandStaffOrb);
    assert_eq!(
        facts(&wand, &[(158, 2), (159, 43), (160, 355)]).weapon_skill,
        43
    );
    assert_eq!(facts(&wand, &[]).weapon_skill, 0);
}

#[test]
fn the_slot_is_named_from_the_valid_locations() {
    let mut ring = item(ObjectClass::Jewelry);
    ring.valid_locations = 262_144 | 524_288;
    assert_eq!(facts(&ring, &[]).slot, "Ring");
    // Saved: the property.
    assert_eq!(
        facts(&item(ObjectClass::Jewelry), &[(9, 32_768)]).slot,
        "Neck"
    );
    // Loot armor with its material, reduced by its coverage.
    let mut coat = item(ObjectClass::Armor);
    coat.valid_locations = 512 | 1024 | 2048 | 4096;
    coat.coverage_mask = 1024 | 2048 | 4096 | 8192;
    assert_eq!(
        facts(&coat, &[]).slot,
        "Chest, Abdomen, Upper Arms, Lower Arms"
    );
    assert_eq!(facts(&coat, &[(131, 61)]).slot, "Chest");
    assert_eq!(facts(&item(ObjectClass::Gem), &[]).slot, "-");
}

#[test]
fn the_wield_level_reads_both_requirement_triples() {
    let sword = item(ObjectClass::MeleeWeapon);
    assert_eq!(facts(&sword, &[]).wield_level, None);
    assert_eq!(
        facts(&sword, &[(158, 7), (159, 1), (160, 180)]).wield_level,
        Some(180)
    );
    // A skill requirement first, the level second.
    assert_eq!(
        facts(
            &sword,
            &[
                (158, 2),
                (159, 44),
                (160, 375),
                (270, 7),
                (271, 1),
                (272, 150)
            ]
        )
        .wield_level,
        Some(150)
    );
    // A skill requirement alone is no level.
    assert_eq!(
        facts(&sword, &[(158, 2), (159, 44), (160, 375)]).wield_level,
        None
    );
}

#[test]
fn value_workmanship_burden_and_set_are_their_properties() {
    let mut ring = item(ObjectClass::Jewelry);
    let facts_ = facts(&ring, &[(19, 7_500), (105, 6), (5, 30), (265, 13)]);
    assert_eq!(facts_.value, Some(7_500));
    assert_eq!(facts_.workmanship, Some(6));
    assert_eq!(facts_.burden, Some(30));
    assert_eq!(facts_.set_id, Some(13));
    let none = facts(&ring, &[]);
    assert_eq!(
        (none.value, none.workmanship, none.burden, none.set_id),
        (None, None, None, None)
    );
    // The object's description, before an appraisal.
    ring.value = 900;
    ring.burden = 20;
    ring.workmanship = 7.0;
    let described = facts(&ring, &[]);
    assert_eq!(
        (described.value, described.burden, described.workmanship),
        (Some(900), Some(20), Some(7))
    );
}

#[test]
fn a_rating_is_the_larger_of_its_base_and_gear_property() {
    let amulet = item(ObjectClass::Jewelry);
    // Every rating's ids, the base or the Gear property the larger.
    let facts = facts(
        &amulet,
        &[
            (307, 2),
            (370, 3),
            (323, 5),
            (376, 4),
            (379, 21),
            (28, 300),
            (308, 1),
            (371, 6),
            (315, 1),
            (373, 4),
            (316, 7),
            (375, 2),
            (317, 8),
            (331, 5),
            (377, 2),
            (350, 9),
            (378, 6),
            (356, 10),
            (357, 11),
            (358, 12),
            (381, 13),
            (382, 14),
            (383, 15),
            (384, 16),
            (171, 17),
        ],
    );
    let expected = [
        (Rating::DamageRating, Some(3)),
        (Rating::CritDamage, None),
        (Rating::HealBoost, Some(5)),
        (Rating::Vitality, Some(21)),
        (Rating::ArmorLevel, Some(300)),
        (Rating::DamageResist, Some(6)),
        (Rating::CritResist, Some(4)),
        (Rating::CritDamageResist, Some(7)),
        (Rating::HealingResist, Some(8)),
        (Rating::NetherResist, Some(5)),
        (Rating::DotResist, Some(9)),
        (Rating::LifeResist, Some(6)),
        (Rating::SneakAttack, Some(10)),
        (Rating::Recklessness, Some(11)),
        (Rating::Deception, Some(12)),
        (Rating::PkDamage, Some(13)),
        (Rating::PkDamageResist, Some(14)),
        (Rating::GearPkDamage, Some(15)),
        (Rating::GearPkResist, Some(16)),
        (Rating::Tinks, Some(17)),
    ];
    assert_eq!(expected.map(|(rating, _)| rating), Rating::ALL);
    for (rating, value) in expected {
        assert_eq!(facts.rating(rating), value, "{rating:?}");
    }
    assert_eq!(Rating::ALL.len(), 20, "Healing rating (342) is not one");
    assert_eq!(Rating::CritDamageResist.label(), "Crit dmg resist");
}

#[test]
fn the_crit_damage_tier_is_the_rating_clamped_to_0_to_2() {
    let gauntlets = item(ObjectClass::Armor);
    // suit_cd_test.go's TestCritTier.
    for (rating, tier) in [(-1, 0), (0, 0), (1, 1), (2, 2), (3, 2), (5, 2)] {
        assert_eq!(
            facts(&gauntlets, &[(374, rating)]).cd_tier,
            tier,
            "{rating}"
        );
    }
    assert_eq!(facts(&gauntlets, &[]).cd_tier, 0);
    assert_eq!(facts(&gauntlets, &[(314, 1), (374, 2)]).cd_tier, 2);
}

#[test]
fn the_spells_are_kept_lowercase_with_their_cantrips() {
    let ring = item(ObjectClass::Jewelry);
    let facts = RowFacts::new(
        &ring,
        &props(&[]),
        true,
        spells(&["Legendary Strength", "Epic Focus", "Prowess"]),
    );
    assert_eq!(
        facts.spells.iter().map(|s| &**s).collect::<Vec<_>>(),
        ["legendary strength", "epic focus", "prowess"]
    );
    assert_eq!(
        facts.cantrips,
        [
            (cantrip_index("Strength").unwrap(), CantripTier::Legendary),
            (cantrip_index("Focus").unwrap(), CantripTier::Epic)
        ]
    );
    assert!(facts.identified);
    assert!(!RowFacts::new(&ring, &props(&[]), false, vec![]).identified);
}

// --- The filter ---

fn appraised(item: &InventoryItem, ints: &[(u32, i32)], spell_names: &[&str]) -> RowFacts {
    RowFacts::new(item, &props(ints), true, spells(spell_names))
}

fn named(class: ObjectClass, name: &str, coverage: u32) -> InventoryItem {
    InventoryItem {
        name: name.to_owned(),
        coverage_mask: coverage,
        ..item(class)
    }
}

#[test]
fn an_empty_filter_keeps_everything() {
    let filter = InventoryFilter::default();
    assert!(filter.is_empty());
    assert_eq!(filter.summary(), "");
    assert!(filter.matches(&facts(&item(ObjectClass::Gem), &[])));
    let unidentified = RowFacts::new(&item(ObjectClass::Gem), &props(&[]), false, vec![]);
    assert!(filter.matches(&unidentified));
}

#[test]
fn the_equipped_choice() {
    let mut worn = item(ObjectClass::Jewelry);
    worn.equipped_location = 32_768;
    let worn = facts(&worn, &[]);
    let carried = facts(&item(ObjectClass::Jewelry), &[]);
    let mut filter = InventoryFilter {
        equipped: Equipped::Yes,
        ..InventoryFilter::default()
    };
    assert!(filter.matches(&worn) && !filter.matches(&carried));
    filter.equipped = Equipped::No;
    assert!(!filter.matches(&worn) && filter.matches(&carried));
    assert_eq!(filter.summary(), "Not equipped");
}

#[test]
fn bonded_attuned_and_rare_each_need_their_property() {
    let ring = item(ObjectClass::Jewelry);
    let plain = facts(&ring, &[]);
    for filter in [
        InventoryFilter {
            bonded: true,
            ..InventoryFilter::default()
        },
        InventoryFilter {
            attuned: true,
            ..InventoryFilter::default()
        },
        InventoryFilter {
            rare: true,
            ..InventoryFilter::default()
        },
    ] {
        assert!(!filter.matches(&plain));
        assert!(filter.matches(&facts(&ring, &[(33, 1), (114, 1), (17, 1)])));
    }
}

#[test]
fn the_item_types() {
    let armor = facts(&item(ObjectClass::Armor), &[(28, 200)]);
    let armor_without_level = facts(&item(ObjectClass::Armor), &[]);
    let ring = facts(&item(ObjectClass::Jewelry), &[]);
    let filter = |item_type| InventoryFilter {
        item_type,
        ..InventoryFilter::default()
    };
    assert!(filter(ItemType::Armor).matches(&armor));
    assert!(!filter(ItemType::Armor).matches(&armor_without_level));
    assert!(!filter(ItemType::Armor).matches(&ring));
    assert!(filter(ItemType::Jewelry).matches(&ring));
    assert!(!filter(ItemType::Jewelry).matches(&armor));
    // Clothing: a shirt or pants by name, not a robe.
    let shirt = facts(&named(ObjectClass::Clothing, "Flared Shirt", 8 | 16), &[]);
    let pants = facts(
        &named(ObjectClass::Clothing, "Baggy Pants", 2 | 4 | 16),
        &[],
    );
    let robe = facts(&named(ObjectClass::Clothing, "Robe of the Shirt", 8), &[]);
    let breeches = facts(&named(ObjectClass::Clothing, "Breeches", 2), &[]);
    assert!(filter(ItemType::Clothing).matches(&shirt));
    assert!(filter(ItemType::Clothing).matches(&pants));
    assert!(!filter(ItemType::Clothing).matches(&robe));
    // Shirts: chest underwear, not both leg bits, not pants by name.
    assert!(filter(ItemType::Shirts).matches(&shirt));
    assert!(!filter(ItemType::Shirts).matches(&pants));
    assert!(!filter(ItemType::Shirts).matches(&robe));
    // Pants: upper leg underwear.
    assert!(filter(ItemType::Pants).matches(&pants));
    assert!(filter(ItemType::Pants).matches(&breeches));
    assert!(!filter(ItemType::Pants).matches(&shirt));
}

#[test]
fn weapons_and_their_kinds() {
    let sword = facts(&item(ObjectClass::MeleeWeapon), &[(48, 44)]);
    let dagger = facts(&item(ObjectClass::MeleeWeapon), &[(48, 46)]);
    let bow = facts(&item(ObjectClass::MissileWeapon), &[(48, 47)]);
    let orb = facts(&item(ObjectClass::WandStaffOrb), &[(159, 43)]);
    let ring = facts(&item(ObjectClass::Jewelry), &[]);
    let mut filter = InventoryFilter {
        item_type: ItemType::Weapons,
        ..InventoryFilter::default()
    };
    for weapon in [&sword, &dagger, &bow, &orb] {
        assert!(filter.matches(weapon));
    }
    assert!(!filter.matches(&ring));
    filter.weapon_kinds = BTreeSet::from([WeaponKind::Heavy, WeaponKind::Void]);
    assert!(filter.matches(&sword));
    assert!(!filter.matches(&dagger));
    assert!(!filter.matches(&bow));
    assert!(filter.matches(&orb));
    assert_eq!(filter.summary(), "Weapons: Heavy, Void Magic");
    // The kinds count only with Weapons.
    filter.item_type = ItemType::All;
    assert!(filter.matches(&ring));
}

#[test]
fn the_slots_are_ored() {
    let slot = |valid| {
        let mut thing = item(ObjectClass::Jewelry);
        thing.valid_locations = valid;
        facts(&thing, &[])
    };
    let ring = slot(786_432);
    let bracelet = slot(196_608);
    let neck = slot(32_768);
    let shield = slot(2_097_152);
    let two_handed = slot(33_554_432);
    let leggings = slot(1024 | 8192 | 16_384);
    let filter = |slots: &[Slot]| InventoryFilter {
        slots: slots.iter().copied().collect(),
        ..InventoryFilter::default()
    };
    assert!(filter(&[Slot::Ring]).matches(&ring));
    assert!(!filter(&[Slot::Ring]).matches(&neck));
    assert!(filter(&[Slot::Bracelet]).matches(&bracelet));
    assert!(filter(&[Slot::Ring, Slot::Neck]).matches(&neck));
    assert!(filter(&[Slot::Shield]).matches(&shield));
    assert!(!filter(&[Slot::Shield]).matches(&two_handed));
    assert!(filter(&[Slot::UpperLegs]).matches(&leggings));
    assert!(!filter(&[Slot::UpperArms]).matches(&leggings));
}

#[test]
fn every_checked_cantrip_is_needed_at_its_tier_or_better() {
    let ring = item(ObjectClass::Jewelry);
    let both = appraised(&ring, &[], &["Legendary Strength", "Epic Invulnerability"]);
    let one = appraised(&ring, &[], &["Legendary Strength"]);
    let strength = cantrip_index("Strength").unwrap();
    let invulnerability = cantrip_index("Invulnerability").unwrap();
    let filter = InventoryFilter {
        cantrips: vec![
            (strength, CantripTier::Legendary),
            (invulnerability, CantripTier::Epic),
        ],
        ..InventoryFilter::default()
    };
    assert!(filter.matches(&both));
    assert!(!filter.matches(&one));
    let legendary_only = InventoryFilter {
        cantrips: vec![(invulnerability, CantripTier::Legendary)],
        ..InventoryFilter::default()
    };
    assert!(!legendary_only.matches(&both));
    assert_eq!(
        filter.summary(),
        "Legendary Strength · Epic Invulnerability"
    );
}

#[test]
fn spell_contains_is_any_spell_ignoring_case() {
    let ring = item(ObjectClass::Jewelry);
    let facts = appraised(&ring, &[], &["Epic Invulnerability"]);
    let filter = |text: &str| InventoryFilter {
        spell_contains: text.to_owned(),
        ..InventoryFilter::default()
    };
    assert!(filter("INVULN").matches(&facts));
    assert!(!filter("frostbite").matches(&facts));
    assert!(filter("   ").is_empty(), "whitespace is no filter");
}

#[test]
fn a_rating_needs_at_least_its_minimum_and_missing_fails() {
    let amulet = item(ObjectClass::Jewelry);
    let mut filter = InventoryFilter::default();
    filter.set_rating(Rating::CritDamage, Some(2));
    assert!(filter.matches(&facts(&amulet, &[(374, 2)])));
    assert!(filter.matches(&facts(&amulet, &[(314, 3)])));
    assert!(!filter.matches(&facts(&amulet, &[(374, 1)])));
    assert!(!filter.matches(&facts(&amulet, &[])));
    assert_eq!(filter.rating(Rating::CritDamage), Some(2));
    assert_eq!(filter.summary(), "Crit damage ≥ 2");
}

#[test]
fn allowed_crit_damage_drops_armor_of_other_tiers_only() {
    let piece = |valid, cd| {
        let mut thing = item(ObjectClass::Armor);
        thing.valid_locations = valid;
        facts(&thing, &[(374, cd)])
    };
    let mut filter = InventoryFilter {
        allowed_cd: [true, true, false],
        ..InventoryFilter::default()
    };
    assert!(filter.matches(&piece(1, 1)));
    assert!(!filter.matches(&piece(1, 2)));
    assert!(!filter.matches(&piece(1024 | 8192, 3)));
    // Jewelry is never dropped.
    assert!(filter.matches(&piece(32_768, 2)));
    assert_eq!(filter.summary(), "CD0, CD1");
    // All three or none: no filter (suit_cd.go).
    filter.allowed_cd = [false; 3];
    assert!(filter.is_empty());
    filter.allowed_cd = [true; 3];
    assert!(filter.is_empty());
}

#[test]
fn the_set_and_the_requirements() {
    let ring = item(ObjectClass::Jewelry);
    let soldier = facts(&ring, &[(265, 13), (19, 5_000), (105, 8), (5, 40)]);
    let level = facts(&ring, &[(158, 7), (159, 1), (160, 180)]);
    let set = InventoryFilter {
        set: Some(13),
        ..InventoryFilter::default()
    };
    assert!(set.matches(&soldier) && !set.matches(&level));
    assert_eq!(set.summary(), "Soldier's Set");
    let wield = InventoryFilter {
        max_wield: Some(150),
        ..InventoryFilter::default()
    };
    assert!(wield.matches(&soldier), "no level requirement passes");
    assert!(!wield.matches(&level));
    let value = InventoryFilter {
        min_value: Some(5_000),
        min_workmanship: Some(8),
        max_burden: Some(40),
        ..InventoryFilter::default()
    };
    assert!(value.matches(&soldier));
    assert!(!value.matches(&level), "missing values fail");
    assert_eq!(value.summary(), "Value ≥ 5000 · Work ≥ 8 · Burden ≤ 40");
}

#[test]
fn an_unidentified_item_fails_the_appraisal_filters_and_says_so() {
    let ring = item(ObjectClass::Jewelry);
    let unidentified = RowFacts::new(&ring, &props(&[(374, 3)]), false, vec![]);
    let mut ratings = InventoryFilter::default();
    ratings.set_rating(Rating::CritDamage, Some(1));
    assert_eq!(ratings.verdict(&unidentified), Verdict::NotIdentified);
    let cantrip = InventoryFilter {
        cantrips: vec![(0, CantripTier::Minor)],
        ..InventoryFilter::default()
    };
    assert_eq!(cantrip.verdict(&unidentified), Verdict::NotIdentified);
    let spell = InventoryFilter {
        spell_contains: "x".to_owned(),
        ..InventoryFilter::default()
    };
    assert_eq!(spell.verdict(&unidentified), Verdict::NotIdentified);
    // Left out by another filter first: a plain fail.
    let jewelry_and_rating = InventoryFilter {
        item_type: ItemType::Armor,
        ..ratings.clone()
    };
    assert_eq!(jewelry_and_rating.verdict(&unidentified), Verdict::Fail);
    // Filters that need no appraisal still pass it.
    let jewelry = InventoryFilter {
        item_type: ItemType::Jewelry,
        ..InventoryFilter::default()
    };
    assert_eq!(jewelry.verdict(&unidentified), Verdict::Pass);
}

#[test]
fn several_filters_together_and_their_summary() {
    let mut amulet = item(ObjectClass::Jewelry);
    amulet.valid_locations = 32_768;
    let good = appraised(
        &amulet,
        &[(374, 2), (265, 13), (33, 1)],
        &["Legendary Strength"],
    );
    let mut filter = InventoryFilter {
        bonded: true,
        item_type: ItemType::Jewelry,
        slots: BTreeSet::from([Slot::Neck]),
        cantrips: vec![(0, CantripTier::Legendary)],
        set: Some(13),
        ..InventoryFilter::default()
    };
    filter.set_rating(Rating::CritDamage, Some(2));
    assert!(filter.matches(&good));
    filter.set_rating(Rating::CritDamage, Some(3));
    assert!(!filter.matches(&good));
    assert_eq!(
        filter.summary(),
        "Bonded · Jewelry · Neck · Legendary Strength · Crit damage ≥ 3 · Soldier's Set"
    );
    filter.characters = Some(BTreeSet::from(["Alt One".to_owned()]));
    assert!(filter.summary().ends_with(" · Alt One"));
    assert!(!filter.is_empty());
    assert!(InventoryFilter::default().is_empty());
}

// --- The review's fixes ---

#[test]
fn loot_armor_without_coverage_bits_is_named_by_its_own_slots() {
    assert_eq!(slot_name(512 | 1024, 0, true), "Chest, Abdomen");
    assert_eq!(slot_name(2048 | 4096, 0, true), "Upper Arms, Lower Arms");
}

#[test]
fn any_ready_slot_is_no_slot() {
    // CanGoInReadySlot: every bit below the top one.
    assert_eq!(slot_name(0x7FFF_FFFF, 0, false), "-");
    assert_eq!(slot_name(0x7FFF_FFFF, 1024, true), "-");
}

#[test]
fn the_armor_type_and_the_set_need_an_appraisal() {
    let coat = item(ObjectClass::Armor);
    let ring = item(ObjectClass::Jewelry);
    let unidentified = |item: &InventoryItem| RowFacts::new(item, &props(&[]), false, vec![]);
    let armor = InventoryFilter {
        item_type: ItemType::Armor,
        ..InventoryFilter::default()
    };
    assert_eq!(armor.verdict(&unidentified(&coat)), Verdict::NotIdentified);
    assert_eq!(
        armor.verdict(&unidentified(&ring)),
        Verdict::Fail,
        "not armor at all"
    );
    assert_eq!(
        armor.verdict(&facts(&coat, &[])),
        Verdict::Fail,
        "appraised, no level"
    );
    assert_eq!(armor.verdict(&facts(&coat, &[(28, 200)])), Verdict::Pass);
    let set = InventoryFilter {
        set: Some(13),
        ..InventoryFilter::default()
    };
    assert_eq!(set.verdict(&unidentified(&ring)), Verdict::NotIdentified);
    assert_eq!(set.verdict(&facts(&ring, &[])), Verdict::Fail);
    assert_eq!(set.verdict(&facts(&ring, &[(265, 13)])), Verdict::Pass);
}
