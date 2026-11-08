//! The item info line Mag-Tools prints when an item is identified
//! (`ItemInfo.ToString()`), segment by segment.
//!
//! Source: Mag-Tools `Mag-Tools/ItemInfo/ItemInfo.cs` and
//! `Shared/MyWorldObject.cs` at 43ee2ec. Every expected string is worked
//! out from those by hand (doubles as .NET computes them; `Math.Round`
//! halves to even; `N1`/`N2` take fifteen digits and round half away from
//! zero; a double joined into a string prints fifteen digits). Tests that
//! also pin a test of the C# port (`OpenAC-MagTools`,
//! `ItemInfoFormatterTests`) name it; its tests that pin the C# port's own
//! deviations (protections as floats 13-19, the mastery text `(80)`) are
//! not ported.

use ac_plugin_abi::items::{InventoryItem, ObjectHeader};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{ArmorProfile, ItemProperties, ObjectClass, WeaponProfile};
use mosstank_mag::item_info::{ItemInfoInput, item_info};

/// An item the line reads: a name and a class, nothing else.
fn item(name: &str, class: ObjectClass) -> InventoryItem {
    InventoryItem {
        name: name.to_owned(),
        object_class: class,
        ..InventoryItem::default()
    }
}

/// Properties by game property id.
fn props(ints: &[(u32, i32)], floats: &[(u32, f64)]) -> ItemProperties {
    ItemProperties {
        ints: ints.to_vec(),
        floats: floats.to_vec(),
        ..ItemProperties::default()
    }
}

/// A weapon profile with the values the line reads.
fn weapon(damage: i32, variance: f64, damage_mod: f64, offense: f64) -> WeaponProfile {
    WeaponProfile {
        damage,
        damage_variance: variance,
        damage_mod,
        weapon_offense: offense,
        ..WeaponProfile::default()
    }
}

/// A spell as the client's spell table answers it.
fn spell(id: u32, name: &str, family: u32, difficulty: i32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family,
        difficulty,
        ..SpellInfo::default()
    }
}

/// The line with the settings given and the spells in `catalog`.
fn line_with(
    item: &InventoryItem,
    properties: &ItemProperties,
    show_buffed_values: bool,
    show_value_and_burden: bool,
    catalog: &[SpellInfo],
) -> String {
    let input = ItemInfoInput {
        item,
        properties,
        show_buffed_values,
        show_value_and_burden,
    };
    item_info(&input, &mut |id| {
        catalog.iter().find(|s| s.spell_id == id).cloned()
    })
}

/// The line with both settings off and no spells known.
fn line(item: &InventoryItem, properties: &ItemProperties) -> String {
    line_with(item, properties, false, false, &[])
}

// ---- Segments 1-3: material, name, mastery, set ----

#[test]
fn a_plain_item_is_its_name() {
    // Source: ItemInfo.cs:42
    let ring = item("Test Ring", ObjectClass::Jewelry);
    assert_eq!(line(&ring, &props(&[], &[])), "Test Ring");
}

#[test]
fn material_mastery_and_set_frame_the_name() {
    // Source: ItemInfo.cs:34-60
    // Source: OpenAC-MagTools ItemInfoFormatterTests.MaterialNameAndMasteryAndSetPrefixTheLine
    let sword = item("Long Sword", ObjectClass::Armor);
    let p = props(&[(131, 61), (353, 2), (265, 13)], &[]);
    assert_eq!(line(&sword, &p), "Iron Long Sword (Sword), Soldier's Set");
}

#[test]
fn unknown_material_mastery_and_set_print_their_numbers() {
    // Source: ItemInfo.cs:39, 49, 59 (the C# port printed `(80)`; the
    // original prints `(Unknown mastery 80)`)
    let thing = item("Thing", ObjectClass::Misc);
    let p = props(&[(131, 72), (353, 80), (265, 3)], &[]);
    assert_eq!(
        line(&thing, &p),
        "unknown material 72 Thing (Unknown mastery 80), Unknown set 3"
    );
}

#[test]
fn the_material_comes_from_the_object_description_before_an_appraisal() {
    // Source: ItemInfo.cs:34 (Decal's Material key is filled from the
    // object's description too)
    let mut thing = item("Thing", ObjectClass::Misc);
    thing.material_type = 61;
    assert_eq!(line(&thing, &props(&[], &[])), "Iron Thing");
}

// ---- Segments 5-7: armour level, imbues, tinks ----

#[test]
fn armour_level_imbues_and_tinks() {
    // Source: ItemInfo.cs:62-85
    // Source: OpenAC-MagTools ItemInfoFormatterTests.ArmorLevelAndImbueFlagsAndTinks
    let plate = item("Breastplate", ObjectClass::Armor);
    let p = props(&[(28, 200), (179, 1 | 8 | 8192), (171, 6)], &[]);
    assert_eq!(
        line(&plate, &p),
        "Breastplate, AL 200, CS SlashRend Hematited, Tinks 6"
    );
}

#[test]
fn every_imbue_in_order_and_an_unknown_one_leaves_a_bare_comma() {
    // Source: ItemInfo.cs:65-81
    let all = 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 | 512 | 1024 | 4096 | 8192 | 536_870_912;
    let thing = item("X", ObjectClass::MeleeWeapon);
    assert_eq!(
        line(&thing, &props(&[(179, all)], &[])),
        "X, CS CB AR SlashRend PierceRend BludgeRend AcidRend FrostRend LightRend FireRend \
         MeleeImbue MagicImbue Hematited MagicAbsorb"
    );
    assert_eq!(line(&thing, &props(&[(179, 2048)], &[])), "X,");
}

// ---- Segments 8-16: weapon damage and modifiers ----

#[test]
fn weapon_damage_range_and_every_modifier_in_order() {
    // Source: ItemInfo.cs:87-114. 50 - 50 * 0.2 = 40 -> "40.00";
    // (1.1 - 1) * 100 = 10.000000000000009 -> 10; (1.2 - 1) * 100 ->
    // 20; (1.05 - 1) * 100 -> 5; (1.15 - 1) * 100 = 14.999999999999991
    // -> 15; Math.Round(2.0999999999999908, 1) = 2.1;
    // Math.Round(-9.999999999999998, 1) = -10; 0.5 * 100 = 50.
    // Source: OpenAC-MagTools ItemInfoFormatterTests.PercentageBonusSegmentsFormatInOrder
    // (there with the C# port's float keys; here the weapon profile)
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    let mut p = props(
        &[(204, 5)],
        &[(152, 1.2), (29, 1.15), (150, 1.021), (149, 0.9), (144, 0.5)],
    );
    p.weapon = Some(weapon(50, 0.2, 1.1, 1.05));
    assert_eq!(
        line(&sword, &p),
        "Sword, 40.00-50, +5, +10%, +20%vs. Monsters, +5%a, 15%md, 2.1%mgc.d, -10%msl.d, 50%mc"
    );
}

#[test]
fn damage_without_variance_and_a_negative_elemental_bonus() {
    // Source: ItemInfo.cs:89-93 (a negative bonus prints "+-5")
    // Source: OpenAC-MagTools ItemInfoFormatterTests.DamageSegmentShowsPlainMaxDamageWhenVarianceIsZero
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    let mut p = props(&[(204, -5)], &[]);
    p.weapon = Some(weapon(50, 0.0, 1.0, 1.0));
    assert_eq!(line(&sword, &p), "Sword, 50, +-5");
}

#[test]
fn modifiers_round_half_to_even_and_never_print_a_negative_zero() {
    // Source: ItemInfo.cs:96, 105. (1.125 - 1) * 100 = 12.5 -> 12;
    // (0.996 - 1) * 100 = -0.4 -> -0 -> "0".
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    let mut p = props(&[], &[(29, 0.996)]);
    p.weapon = Some(weapon(0, 0.0, 1.125, 1.0));
    assert_eq!(line(&sword, &p), "Sword, +12%, 0%md");
}

#[test]
fn weapon_values_come_from_the_appraised_weapon_profile() {
    // Source: ItemInfo.cs:87-143. 60 - 60 * 0.2 -> "48.00"; buffed:
    // no material, so no tinks left: DoT(84, 0.2) = 84 * 1.01 = 84.84 ->
    // "84.8"; Math.Round((1.05 - 1) * 100) = 5 -> "5.0/".
    // Source: OpenAC-MagTools ItemInfoFormatterTests.WeaponSegmentsComeFromARealWeaponProfileAppraisal
    let mace = item("War Mace", ObjectClass::MeleeWeapon);
    let mut p = props(&[], &[]);
    p.weapon = Some(weapon(60, 0.2, 1.10, 1.05));
    assert_eq!(
        line_with(&mace, &p, true, false, &[]),
        "War Mace, 48.00-60, +10%, +5%a, (84.8/60 5.0/)"
    );
}

// ---- Segment 17: the buffed block ----

#[test]
fn the_buffed_block_of_a_plain_melee_weapon() {
    // Source: ItemInfo.cs:116-143, MyWorldObject.cs:267-299, 376-379.
    // Not loot generated: no tinks left. DoT(50 + 24, 0.5) = 64.75 ->
    // "64.8" (half away from zero); nothing after the space.
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    let mut p = props(&[], &[]);
    p.weapon = Some(weapon(50, 0.5, 1.0, 1.0));
    assert_eq!(
        line_with(&sword, &p, true, false, &[]),
        "Sword, 25.00-50, (64.8/50 )"
    );
}

#[test]
fn the_buffed_block_of_a_loot_melee_weapon_with_its_spells() {
    // Source: ItemInfo.cs:116-143, MyWorldObject.cs:267-366,
    // Dictionaries.cs:353-358, 402-418.
    // Buffed max damage: 60 - 22 (Infected Caress, active on the item)
    // + 10 (Legendary Blood Thirst) = 48. Loot generated, no tinks, no
    // imbue: 9 tinks left; the loop takes 3 iron and 6 granite: 51 and
    // 0.55 * 0.8^6 = 0.1441792; DoT(75, 0.14417920000000004) =
    // 77.63395200000001 -> "77.6". Attack 1.15 + .09 -> 24 -> "24.0/";
    // defence 1.1 + .09 -> 19.000000000000018 -> 19 -> "19.0".
    let sword = InventoryItem {
        appraised_spell_ids: vec![6089, 6094, 6091, 0x8000_0000 | 2096],
        ..item("Test Sword", ObjectClass::MeleeWeapon)
    };
    let mut p = props(&[(131, 61)], &[(29, 1.1)]);
    p.weapon = Some(weapon(60, 0.55, 1.0, 1.15));
    let catalog = [
        spell(6089, "Legendary Blood Thirst", 0, 0),
        spell(6094, "Legendary Heart Thirst", 0, 0),
        spell(6091, "Legendary Defender", 0, 0),
        spell(2096, "Infected Caress", 154, 300),
    ];
    assert_eq!(
        line_with(&sword, &p, true, false, &catalog),
        "Iron Test Sword, 27.00-60, +15%a, 10%md, (77.6/48 24.0/19.0), \
         Legendary Heart Thirst, Legendary Defender, Legendary Blood Thirst"
    );
}

#[test]
fn the_tinks_left_count_the_tinks_done_and_the_imbue() {
    // Source: MyWorldObject.cs:277-297. 8 tinks done and an imbue: 2
    // left, both granite: DoT(74, 0.32000000000000006) = 70.744 ->
    // "70.7". No imbue: one tink goes to it, 1 left: DoT(74, 0.4) =
    // 68.08000000000001 -> "68.1".
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    let mut p = props(&[(131, 61), (171, 8), (179, 1)], &[]);
    p.weapon = Some(weapon(50, 0.5, 1.0, 1.0));
    assert_eq!(
        line_with(&sword, &p, true, false, &[]),
        "Iron Sword, CS, Tinks 8, 25.00-50, (70.7/50 )"
    );
    p.ints.retain(|(k, _)| *k != 179);
    assert_eq!(
        line_with(&sword, &p, true, false, &[]),
        "Iron Sword, Tinks 8, 25.00-50, (68.1/50 )"
    );
}

#[test]
fn the_buffed_block_of_a_missile_weapon() {
    // Source: ItemInfo.cs:124-125, MyWorldObject.cs:304. 20 +
    // ((1.3 - 1) * 100) / 3 + 10 = 40 -> "40.0"; without key 204 it is -1.
    let bow = item("Test Bow", ObjectClass::MissileWeapon);
    let mut p = props(&[(204, 10)], &[]);
    p.weapon = Some(weapon(20, 0.0, 1.3, 1.0));
    assert_eq!(
        line_with(&bow, &p, true, false, &[]),
        "Test Bow, 20, +10, +30%, (40.0 )"
    );
    p.ints.clear();
    assert_eq!(
        line_with(&bow, &p, true, false, &[]),
        "Test Bow, 20, +30%, (-1.0 )"
    );
}

#[test]
fn the_buffed_block_of_a_caster() {
    // Source: ItemInfo.cs:127-128, 139-140, MyWorldObject.cs:336-366,
    // Dictionaries.cs:433. Hermetic Link's change is not 1, so its bonus is
    // added: 0.3 + 1.3 = 1.6 -> "/160". A caster without key 152 reads 0:
    // (0 - 1) * 100 -> "-100.0".
    let orb = InventoryItem {
        appraised_spell_ids: vec![6087],
        ..item("Test Orb", ObjectClass::WandStaffOrb)
    };
    let catalog = [spell(6087, "Legendary Hermetic Link", 0, 0)];
    let p = props(&[], &[(152, 1.1), (144, 0.3)]);
    assert_eq!(
        line_with(&orb, &p, true, false, &catalog),
        "Test Orb, +10%vs. Monsters, 30%mc, (10.0 /160), Legendary Hermetic Link"
    );
    let plain = item("Test Orb", ObjectClass::WandStaffOrb);
    let p = props(&[], &[(144, 0.3)]);
    assert_eq!(
        line_with(&plain, &p, true, false, &[]),
        "Test Orb, 30%mc, (-100.0 /30)"
    );
}

#[test]
fn the_buffed_block_only_for_weapons_and_casters_and_only_when_on() {
    // Source: ItemInfo.cs:116
    // Source: OpenAC-MagTools ItemInfoFormatterTests.BuffedBlockOnlyAppearsForWeaponClassesWhenEnabled
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    let mut p = props(&[(131, 61)], &[]);
    p.weapon = Some(weapon(50, 0.2, 1.0, 1.0));
    assert!(line_with(&sword, &p, true, false, &[]).contains(", ("));
    assert!(!line_with(&sword, &p, false, false, &[]).contains(", ("));
    let widget = item("Widget", ObjectClass::Misc);
    assert_eq!(
        line_with(&widget, &props(&[], &[]), true, false, &[]),
        "Widget"
    );
}

// ---- Segment 18: the spell list ----

#[test]
fn a_loot_item_hides_its_lesser_spells_and_lists_the_rest_by_id_descending() {
    // Source: ItemInfo.cs:145-251
    // Source: OpenAC-MagTools ItemInfoFormatterTests.LootGeneratedItemFiltersTieredSpellsButKeepsImpenAndAugmented
    let ring = InventoryItem {
        appraised_spell_ids: vec![10, 11, 12, 13, 14],
        ..item("Ring", ObjectClass::Jewelry)
    };
    let catalog = [
        spell(10, "Minor Strength", 0, 0),
        spell(11, "Major Strength III", 0, 0),
        spell(12, "Legendary Impenetrability", 0, 0),
        spell(13, "Augmented Awareness", 0, 0),
        spell(14, "Some Incantation of Foo", 0, 0),
    ];
    assert_eq!(
        line_with(&ring, &props(&[(131, 61)], &[]), false, false, &catalog),
        "Iron Ring, Augmented Awareness, Legendary Impenetrability, Minor Strength"
    );
}

#[test]
fn every_filter_of_a_loot_item_on_an_enchantable_piece() {
    // Source: ItemInfo.cs:166-246: banes, impen and Brogard hidden on an
    // enchantable piece; levels I-VI by name, difficulty 300 and
    // Incantations hidden; anything else shown. Cantrips (the highest ids)
    // come first.
    let ring = InventoryItem {
        appraised_spell_ids: vec![20, 21, 22, 23, 24, 25, 26, 27, 6100],
        ..item("Ring", ObjectClass::Jewelry)
    };
    let catalog = [
        spell(20, "Acid Bane", 0, 0),
        spell(21, "Impenetrability", 0, 0),
        spell(22, "Brogard's Defiance", 0, 0),
        spell(23, "Strength Self I", 0, 0),
        spell(24, "Strength Self VI", 0, 0),
        spell(25, "Might of the Lugians", 1, 300),
        spell(26, "Incantation of Strength Self", 1, 400),
        spell(27, "Concentration", 13, 100),
        spell(6100, "Legendary Strength", 0, 0),
    ];
    assert_eq!(
        line_with(&ring, &props(&[(131, 61)], &[]), false, false, &catalog),
        "Iron Ring, Legendary Strength, Concentration"
    );
}

#[test]
fn an_unenchantable_loot_piece_shows_banes_and_impen() {
    // Source: ItemInfo.cs:173-178 (key 36 is Decal's Unenchantable)
    // Source: OpenAC-MagTools ItemInfoFormatterTests.UnenchantableItemShowsBaneAndImpenSpellsInsteadOfHidingThem
    // Source: OpenAC-MagTools ItemInfoFormatterTests.UnenchantableLootGearStillShowsBanesAndImpen
    let ring = InventoryItem {
        appraised_spell_ids: vec![20, 21, 30],
        ..item("Ring", ObjectClass::Jewelry)
    };
    let catalog = [
        spell(20, "Some Bane", 0, 0),
        spell(21, "Brogard's Ward", 0, 0),
        spell(30, "Impenetrability", 0, 0),
    ];
    assert_eq!(
        line_with(&ring, &props(&[(131, 61)], &[]), false, false, &catalog),
        "Iron Ring"
    );
    assert_eq!(
        line_with(
            &ring,
            &props(&[(131, 61), (36, 1)], &[]),
            false,
            false,
            &catalog
        ),
        "Iron Ring, Impenetrability, Brogard's Ward, Some Bane"
    );
}

#[test]
fn weapon_buffs_show_by_difficulty() {
    // Source: ItemInfo.cs:211-228: families 152-158, 195 and 325; 250
    // hidden, 300 and 400+ shown, anything else hidden.
    // Source: OpenAC-MagTools ItemInfoFormatterTests.WeaponBuffFamiliesFilterByDifficulty
    let sword = InventoryItem {
        appraised_spell_ids: vec![30, 31, 32, 33, 34, 35, 36, 37],
        ..item("Sword", ObjectClass::MeleeWeapon)
    };
    let catalog = [
        spell(30, "Buff 250", 154, 250),
        spell(31, "Buff 300", 152, 300),
        spell(32, "Buff 400", 158, 400),
        spell(33, "Buff 350", 154, 350),
        spell(34, "Mana Buff 300", 195, 300),
        spell(35, "Damage Mod 500", 325, 500),
        spell(36, "Not A Buff 300", 151, 300),
        spell(37, "Not A Buff Either 300", 159, 300),
    ];
    assert_eq!(
        line_with(&sword, &props(&[(131, 61)], &[]), false, false, &catalog),
        "Iron Sword, Damage Mod 500, Mana Buff 300, Buff 400, Buff 300"
    );
}

#[test]
fn an_item_not_loot_generated_lists_every_spell() {
    // Source: ItemInfo.cs:161-163
    // Source: OpenAC-MagTools ItemInfoFormatterTests.NonLootGeneratedItemShowsEverySpellRegardlessOfFilter
    let trinket = InventoryItem {
        appraised_spell_ids: vec![100, 50],
        ..item("Trinket", ObjectClass::Misc)
    };
    let catalog = [
        spell(100, "Some VI Spell", 0, 0),
        spell(50, "Incantation of Something", 999, 300),
    ];
    assert_eq!(
        line_with(&trinket, &props(&[], &[]), false, false, &catalog),
        "Trinket, Some VI Spell, Incantation of Something"
    );
}

#[test]
fn the_description_flag_alone_makes_an_item_loot_generated() {
    // Source: ItemInfo.cs:162 (Decal holds the Material key when the
    // object's description carried one, even 0; the name has no prefix)
    let ring = InventoryItem {
        appraised_spell_ids: vec![11],
        header: Some(ObjectHeader {
            weenie_header_flags: Some(0x8000_0000),
            ..ObjectHeader::default()
        }),
        ..item("Ring", ObjectClass::Jewelry)
    };
    let catalog = [spell(11, "Strength Self III", 0, 0)];
    assert_eq!(
        line_with(&ring, &props(&[], &[]), false, false, &catalog),
        "Ring"
    );
}

#[test]
fn spells_active_on_the_item_are_not_listed() {
    // Source: ItemInfo.cs:151-152 (Decal's Spell(i) list: the appraisal's
    // entries without the active bit)
    let ring = InventoryItem {
        appraised_spell_ids: vec![0x8000_0000 | 12, 13],
        ..item("Ring", ObjectClass::Jewelry)
    };
    let catalog = [spell(12, "Active One", 0, 0), spell(13, "Listed One", 0, 0)];
    assert_eq!(
        line_with(&ring, &props(&[], &[]), false, false, &catalog),
        "Ring, Listed One"
    );
}

#[test]
fn an_unknown_spell_prints_as_unknown_and_the_rest_still_prints() {
    // Source: fix F11 (Mag-Tools threw and printed nothing)
    let ring = InventoryItem {
        appraised_spell_ids: vec![2_000_000, 10],
        ..item("Test Ring", ObjectClass::Jewelry)
    };
    let p = props(&[(131, 61), (370, 3)], &[]);
    let catalog = [spell(10, "Minor Strength", 0, 0)];
    let line = line_with(&ring, &p, false, false, &catalog);
    assert!(line.contains("Unknown spell 2000000"));
    assert!(line.starts_with("Iron Test Ring"));
    assert_eq!(
        line,
        "Iron Test Ring, Unknown spell 2000000, Minor Strength, [D 3]"
    );
    let alone = InventoryItem {
        appraised_spell_ids: vec![2_000_000],
        ..item("Test Ring", ObjectClass::Jewelry)
    };
    let line = line_with(&alone, &props(&[], &[]), false, false, &[]);
    assert!(line.starts_with("Test Ring"));
    assert!(line.contains("Unknown spell 2000000"));
}

// ---- Segments 19-24: requirements ----

#[test]
fn the_wield_requirement_as_a_level_or_a_skill() {
    // Source: ItemInfo.cs:255-267
    // Source: OpenAC-MagTools ItemInfoFormatterTests.WieldRequirementFormatsAsLevelForAttributeTypeSeven
    let sword = item("Sword", ObjectClass::MeleeWeapon);
    assert_eq!(
        line(&sword, &props(&[(158, 7), (159, 1), (160, 180)], &[])),
        "Sword, Wield Lvl 180"
    );
    assert_eq!(
        line(&sword, &props(&[(158, 1), (159, 0xB), (160, 250)], &[])),
        "Sword, Sword 250"
    );
    assert_eq!(
        line(&sword, &props(&[(158, 2), (159, 0x2C), (160, 375)], &[])),
        "Sword, Heavy Weapons 375"
    );
    assert_eq!(
        line(&sword, &props(&[(158, 2), (159, 0x11), (160, 250)], &[])),
        "Sword, Unknown skill: 17 250"
    );
}

#[test]
fn the_activation_requirement_unless_the_wield_one_covers_it() {
    // Source: ItemInfo.cs:275-281
    // Source: OpenAC-MagTools ItemInfoFormatterTests.ActivationRequirementReadsSkillIdFromTheOrdinaryIntProperty
    // Source: OpenAC-MagTools ItemInfoFormatterTests.ActivationRequirementIsSuppressedWhenWieldMatchesAndCovers
    let wand = item("Wand", ObjectClass::WandStaffOrb);
    assert_eq!(
        line(&wand, &props(&[(115, 300), (176, 0x36)], &[])),
        "Wand, Summoning 300 to Activate"
    );
    assert_eq!(
        line(&wand, &props(&[(115, 300), (176, 99)], &[])),
        "Wand, Unknown skill: 99 300 to Activate"
    );
    let covered = props(
        &[(115, 300), (158, 2), (159, 0x36), (160, 300), (176, 0x36)],
        &[],
    );
    assert_eq!(line(&wand, &covered), "Wand, Summoning 300");
    let lower = props(
        &[(115, 300), (158, 2), (159, 0x36), (160, 290), (176, 0x36)],
        &[],
    );
    assert_eq!(
        line(&wand, &lower),
        "Wand, Summoning 290, Summoning 300 to Activate"
    );
}

#[test]
fn summoning_levels_skills_and_the_lore_difficulty() {
    // Source: ItemInfo.cs:270-302
    // Source: OpenAC-MagTools ItemInfoFormatterTests.ManaConversionKeyLoreAndActivationSegments
    // Source: OpenAC-MagTools ItemInfoFormatterTests.UnknownSkillSpecUsesTheOriginalsDistinctLiteral
    let orb = item("Orb", ObjectClass::WandStaffOrb);
    assert_eq!(
        line(
            &orb,
            &props(
                &[(369, 3), (109, 250), (366, 0x36), (367, 5), (368, 0x36)],
                &[]
            )
        ),
        "Orb, Lvl 3, Summoning 5, Spec Summoning 5, Diff 250"
    );
    assert_eq!(
        line(&orb, &props(&[(366, 0x36), (367, 5), (368, 9999)], &[])),
        "Orb, Summoning 5, Unknown skill spec: 9999 5"
    );
    assert_eq!(
        line(&orb, &props(&[(366, 9999), (367, 5)], &[])),
        "Orb, Unknown skill: 9999 5"
    );
}

// ---- Segment 25: workmanship ----

#[test]
fn salvage_workmanship_with_two_decimals_and_craft_otherwise() {
    // Source: ItemInfo.cs:304-313
    // Source: OpenAC-MagTools ItemInfoFormatterTests.SalvageWorkmanshipUsesTwoDecimalsAndCraftUsesRawInt
    let bag = InventoryItem {
        workmanship: 4.0,
        ..item("Salvage Bag", ObjectClass::Salvage)
    };
    assert_eq!(
        line(&bag, &props(&[(105, 4)], &[])),
        "Salvage Bag, Work 4.00"
    );
    let bag = InventoryItem {
        workmanship: 7.25,
        ..item("Salvage Bag", ObjectClass::Salvage)
    };
    assert_eq!(line(&bag, &props(&[], &[])), "Salvage Bag, Work 7.25");
    let sword = InventoryItem {
        workmanship: 8.0,
        ..item("Sword", ObjectClass::MeleeWeapon)
    };
    assert_eq!(line(&sword, &props(&[(105, 8)], &[])), "Sword, Craft 8");
    assert_eq!(
        line(&sword, &props(&[(105, 8), (171, 10)], &[])),
        "Sword, Tinks 10"
    );
}

// ---- Segment 26: protections ----

#[test]
fn an_unenchantable_armour_piece_shows_its_protections() {
    // Source: ItemInfo.cs:315-325 (order slash, pierce, bludgeon, cold,
    // fire, acid, lightning; nether is not shown)
    // Source: OpenAC-MagTools ItemInfoFormatterTests.UnenchantableArmorProtectionsComeFromARealArmorProfileAppraisal
    let plate = item("Plate", ObjectClass::Armor);
    let mut p = props(&[(28, 300), (36, 1)], &[]);
    p.armor = Some(ArmorProfile {
        armor_level: 300,
        slash_mod: 1.1,
        pierce_mod: 1.2,
        bludgeon_mod: 1.3,
        cold_mod: 1.4,
        fire_mod: 1.5,
        acid_mod: 1.6,
        nether_mod: 9.9,
        electric_mod: 1.7,
    });
    assert_eq!(
        line(&plate, &p),
        "Plate, AL 300, [1.1/1.2/1.3/1.4/1.5/1.6/1.7]"
    );
    p.ints.retain(|(k, _)| *k != 36);
    assert_eq!(line(&plate, &p), "Plate, AL 300");
    let ring = item("Ring", ObjectClass::Jewelry);
    assert_eq!(line(&ring, &props(&[(36, 1)], &[])), "Ring");
}

// ---- Segment 27: value and burden ----

#[test]
fn value_and_burden_only_when_the_setting_is_on() {
    // Source: ItemInfo.cs:327-334
    // Source: OpenAC-MagTools ItemInfoFormatterTests.ValueAndBurdenOnlyPrintWhenTheSettingIsOn
    let coin = item("Coin", ObjectClass::Misc);
    let p = props(&[(19, 12345), (5, 3)], &[]);
    assert_eq!(line_with(&coin, &p, true, false, &[]), "Coin");
    assert_eq!(
        line_with(&coin, &p, true, true, &[]),
        "Coin, Value 12,345, BU 3"
    );
}

#[test]
fn value_and_burden_come_from_the_object_description_before_an_appraisal() {
    // Source: ItemInfo.cs:329-333 (Decal fills Value and Burden from the
    // object's description)
    let coin = InventoryItem {
        value: 1_500_000,
        burden: 20,
        ..item("Coin", ObjectClass::Misc)
    };
    assert_eq!(
        line_with(&coin, &props(&[], &[]), false, true, &[]),
        "Coin, Value 1,500,000, BU 20"
    );
}

// ---- Segment 28: ratings ----

#[test]
fn ratings_with_cd_before_cr() {
    // Source: ItemInfo.cs:336-349, MyWorldObject.cs:195-223
    // Source: OpenAC-MagTools ItemInfoFormatterTests.RatingsBlockListsOnlyPresentRatingsInOrder
    let ring = item("Test Ring", ObjectClass::Jewelry);
    let p = props(
        &[
            (370, 5),
            (371, 0),
            (372, 3),
            (373, 4),
            (374, 6),
            (375, 1),
            (376, 2),
            (379, 7),
        ],
        &[],
    );
    assert_eq!(
        line(&ring, &p),
        "Test Ring, [D 5, C 3, CD 6, CR 4, CDR 1, HB 2, V 7]"
    );
    assert_eq!(
        line(&ring, &props(&[(370, 5), (372, 3), (376, 2)], &[])),
        "Test Ring, [D 5, C 3, HB 2]"
    );
    assert_eq!(line(&ring, &props(&[(371, 0)], &[])), "Test Ring");
}

// ---- Segment 29: keyrings ----

#[test]
fn a_keyring_shows_its_keys_and_uses() {
    // Source: ItemInfo.cs:351-352
    // Source: OpenAC-MagTools ItemInfoFormatterTests.KeyringShowsKeysAndUsesWhenNameContainsKeyring
    let ring = item("Burning Sands Keyring", ObjectClass::Misc);
    assert_eq!(
        line(&ring, &props(&[(193, 12), (92, 4)], &[])),
        "Burning Sands Keyring, Keys: 12, Uses: 4"
    );
    let described = InventoryItem {
        structure: 9,
        ..item("Keyring", ObjectClass::Misc)
    };
    assert_eq!(
        line(&described, &props(&[(193, 2)], &[])),
        "Keyring, Keys: 2, Uses: 9"
    );
    let key = item("Keyring", ObjectClass::Key);
    assert_eq!(line(&key, &props(&[(193, 2)], &[])), "Keyring");
}

// ---- The name tables ----

#[test]
fn the_name_tables_are_complete() {
    // Source: Dictionaries.cs:11-325 (51 skills, 11 masteries, 138 sets,
    // 72 materials, with their gaps)
    use mosstank_mag::item_info::names;
    assert_eq!(names::SKILL_INFO.len(), 51);
    assert_eq!(names::MASTERY_INFO.len(), 11);
    assert_eq!(names::ATTRIBUTE_SET_INFO.len(), 138);
    assert_eq!(names::MATERIAL_INFO.len(), 72);
    assert_eq!(names::skill(0x11), None);
    assert_eq!(names::material(72), None);
    assert_eq!(names::attribute_set(23), Some("Hardenend Set"));
    assert_eq!(names::attribute_set(83), Some("Weave of Assess Creature "));
}

// ---- All segments in order ----

#[test]
fn every_segment_in_order() {
    // Source: ItemInfo.cs:30-355. The f32 protections widen to doubles
    // (0.9 -> 0.899999976158142 -> "0.9").
    let coat = InventoryItem {
        appraised_spell_ids: vec![4407, 6095],
        ..item("Amuli Coat", ObjectClass::Armor)
    };
    let mut p = props(
        &[
            (131, 64),
            (265, 13),
            (28, 520),
            (179, 4),
            (171, 3),
            (36, 1),
            (158, 7),
            (159, 1),
            (160, 150),
            (105, 7),
            (19, 8000),
            (5, 900),
            (371, 2),
        ],
        &[],
    );
    p.armor = Some(ArmorProfile {
        armor_level: 520,
        slash_mod: 1.0,
        pierce_mod: 0.8,
        bludgeon_mod: 0.9,
        cold_mod: 0.5,
        fire_mod: 0.4,
        acid_mod: 1.2,
        nether_mod: 0.0,
        electric_mod: 0.6,
    });
    let catalog = [
        spell(4407, "Incantation of Impenetrability", 0, 400),
        spell(6095, "Legendary Impenetrability", 0, 0),
    ];
    assert_eq!(
        line_with(&coat, &p, true, true, &catalog),
        "Steel Amuli Coat, Soldier's Set, AL 520, AR, Tinks 3, Legendary Impenetrability, \
         Incantation of Impenetrability, Wield Lvl 150, Craft 7, \
         [1.0/0.8/0.9/0.5/0.4/1.2/0.6], Value 8,000, BU 900, [DR 2]"
    );
}
