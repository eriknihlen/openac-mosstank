//! Item enchantments as the buff pass casts them: the auras an added item
//! brings, the character's banes and Impenetrability, the BuffedItems rows a
//! VTank profile imports, and the ledger that covers what was cast (ported
//! from the C#'s `MossTankPanelTests`, over its `FakeAutomation`).

mod common;

use ac_plugin_abi::magic::SpellInfo;
use common::World;
use common::automation::{
    FakeAutomation, ITEM_ENCHANTMENT_SCHOOL, active, component, named_spell, skill,
};
use mosstank::MossTank;
use mosstank_expr::ExpressionValue;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};

const PROFILE_KEY: &str = "mosstank/profiles/Imported.usd";

/// An aura of the Item Enchantment school the character casts on itself.
fn self_aura(spell_id: u32, family: u32, name: &str) -> SpellInfo {
    let mut spell = named_spell(spell_id, family, name, ITEM_ENCHANTMENT_SCHOOL);
    spell.is_self_targeted = true;
    spell
}

/// A wand that goes in the wand slot (`Item(10, "War Wand", 0x8000,
/// validLocations: 0x01000000)`).
fn war_wand(object_id: u32) -> ac_plugin_abi::items::InventoryItem {
    let mut item = common::item(object_id, "War Wand", 0x8000);
    item.valid_locations = 0x0100_0000;
    item
}

/// `ItemEnchantAutomation`: three wand auras and the wand. Every test that
/// wires the fake's `CurrentSelection` to the host's selection gets
/// [`selection_wired`].
fn item_enchant_automation() -> FakeAutomation {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.object_id = 0x5000_0001;
    automation.set_known_self_buffs(vec![
        self_aura(101, 201, "Aura of Defender Self I"),
        self_aura(102, 202, "Aura of Hermetic Link Self I"),
        self_aura(103, 203, "Aura of Spirit Drinker Self I"),
    ]);
    automation.item_entries = vec![war_wand(10)];
    automation
}

/// `automation.CurrentSelection = () => host.Selection.SelectedObjectId`.
fn selection_wired(mut automation: FakeAutomation) -> FakeAutomation {
    automation.casts_see_selection = true;
    automation
}

fn open(automation: FakeAutomation) -> (World, MossTank) {
    let mut world = World::with_automation(automation);
    let plugin = world.enable();
    (world, plugin)
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn ticks(world: &mut World, plugin: &mut MossTank, count: usize, seconds: f64) {
    for _ in 0..count {
        tick(world, plugin, seconds);
    }
}

fn add_selected(world: &mut World, plugin: &mut MossTank, object_id: u32) {
    world.select_object(object_id);
    world.act(plugin, "AddSelectedItem");
}

/// Adds the selected item, runs the macro and lets `count` frames pass.
fn add_and_run(world: &mut World, plugin: &mut MossTank, object_id: u32, count: usize) {
    add_selected(world, plugin, object_id);
    world.act(plugin, "ToggleCombat");
    ticks(world, plugin, count, 0.3);
}

fn run(world: &mut World, plugin: &mut MossTank, count: usize) {
    world.act(plugin, "ToggleCombat");
    ticks(world, plugin, count, 0.3);
}

fn set_option(world: &mut World, plugin: &mut MossTank, name: &str, value: ExpressionValue) {
    world
        .with_tank(plugin, |tank, client| {
            tank.set_meta_option(client, name, &value)
        })
        .unwrap();
}

fn chat_contains(world: &World, text: &str) -> bool {
    world.chat().iter().any(|message| message.contains(text))
}

/// A VTank settings profile with these `(Object, Spell)` BuffedItems rows,
/// in the profile folder as `Imported.usd`.
fn write_imported(world: &mut World, rows: &[(i32, i32)]) {
    let mut database = mosstank_settings::defaults::create();
    let table = database.find_mut("BuffedItems").unwrap();
    for &(object, spell) in rows {
        table.rows.push(VtankRow {
            cells: vec![VtankCell::int(object), VtankCell::int(spell)],
        });
    }
    world.profiles.insert(PROFILE_KEY, &database.render());
}

/// The `(Object, Spell)` pairs of the saved Imported profile.
fn imported_pairs(world: &World) -> Vec<(i32, i32)> {
    let database = VtankDatabase::parse(world.profiles.get(PROFILE_KEY).unwrap()).unwrap();
    let table = database.find("BuffedItems").unwrap();
    let object = table.column_index("Object").unwrap();
    let spell = table.column_index("Spell").unwrap();
    table
        .rows
        .iter()
        .map(|row| {
            (
                row.cells[object].as_int().unwrap(),
                row.cells[spell].as_int().unwrap(),
            )
        })
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddingAWandPopulatesItsThreeDefaultAurasAndCastsThemAtTheItem
#[test]
fn adding_a_wand_populates_its_three_default_auras_and_casts_them_at_the_item() {
    let (mut world, mut plugin) = open(selection_wired(item_enchant_automation()));

    add_and_run(&mut world, &mut plugin, 10, 12);

    assert_eq!(world.cast_spell_ids(), [101, 102, 103]);
    assert_eq!(world.automation().selected, Some(10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedBuffedItemSpellTargetsItsExactSameNameObject
#[test]
fn imported_buffed_item_spell_targets_its_exact_same_name_object() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.item_entries = vec![war_wand(10), war_wand(11)];
    let mut world = World::with_automation(automation);
    write_imported(&mut world, &[(11, 101)]);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load Imported");
    world.select_object(10);
    run(&mut world, &mut plugin, 6);

    assert_eq!(world.cast_spell_ids(), [101]);
    assert_eq!(world.automation().cast_selection_ids, [11]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedBuffedItemWeaponSentinelUsesTheEquippedWeapon
#[test]
fn imported_buffed_item_weapon_sentinel_uses_the_equipped_weapon() {
    let mut automation = selection_wired(item_enchant_automation());
    let mut coat = common::item(10, "Worn Coat", 2);
    coat.equipped_location = 0x0000_0001;
    let mut wand = common::item(11, "War Wand", 0x8000);
    wand.equipped_location = 0x0100_0000;
    automation.item_entries = vec![coat, wand];
    let mut world = World::with_automation(automation);
    write_imported(&mut world, &[(-1, 101)]);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load Imported");
    run(&mut world, &mut plugin, 6);

    assert_eq!(world.cast_spell_ids(), [101]);
    assert_eq!(world.automation().cast_selection_ids, [11]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedUntargetedBuffedItemSpellJoinsTheNormalBuffPlan
#[test]
fn imported_untargeted_buffed_item_spell_joins_the_normal_buff_plan() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.skills = vec![skill(32, "Item Enchantment", 300)];
    let mut untargeted = self_aura(101, 201, "Untargeted Aura I");
    untargeted.is_untargeted = true;
    automation.set_known_self_buffs(vec![untargeted]);
    let mut world = World::with_automation(automation);
    write_imported(&mut world, &[(10, 101)]);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load Imported");
    run(&mut world, &mut plugin, 6);

    assert_eq!(world.cast_spell_ids(), [101]);
    assert_eq!(world.automation().cast_selection_ids, [0]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UnknownAndEmptyImportedBuffedItemSpellsStayInert
#[test]
fn unknown_and_empty_imported_buffed_item_spells_stay_inert() {
    let mut world = World::with_automation(item_enchant_automation());
    write_imported(&mut world, &[(10, 999), (10, -1)]);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load Imported");
    run(&mut world, &mut plugin, 6);

    assert!(world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedBuffedItemUiDeletesOneSpellPairAndKeepsItsSiblings
#[test]
fn imported_buffed_item_ui_deletes_one_spell_pair_and_keeps_its_siblings() {
    let mut world = World::with_automation(selection_wired(item_enchant_automation()));
    write_imported(&mut world, &[(10, -1), (10, 101), (10, 102)]);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load Imported");
    assert_eq!(
        world.ui_list(&mut plugin, "ItemRows"),
        [
            "War Wand",
            "War Wand \u{2014} Aura of Defender Self I",
            "War Wand \u{2014} Aura of Hermetic Link Self I"
        ]
    );

    world.act_int(&mut plugin, "DeleteItemRowAt", 1);

    assert_eq!(
        world.ui_list(&mut plugin, "ItemRows"),
        ["War Wand", "War Wand \u{2014} Aura of Hermetic Link Self I"]
    );
    run(&mut world, &mut plugin, 8);

    assert_eq!(world.cast_spell_ids(), [102]);
    let pairs = imported_pairs(&world);
    assert!(pairs.contains(&(10, -1)), "{pairs:?}");
    assert!(!pairs.contains(&(10, 101)), "{pairs:?}");
    assert!(pairs.contains(&(10, 102)), "{pairs:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedBuffedItemNumericExemplarResolvesKnownTierByFamily
#[test]
fn imported_buffed_item_numeric_exemplar_resolves_known_tier_by_family() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.skills = vec![skill(ITEM_ENCHANTMENT_SCHOOL, "Item Enchantment", 300)];
    let mut higher = self_aura(101, 201, "Renamed Higher Aura VI");
    higher.tier = 6;
    automation.set_known_self_buffs(vec![higher]);
    automation.catalog_spells = vec![self_aura(100, 201, "Old Exemplar I")];
    let mut world = World::with_automation(automation);
    write_imported(&mut world, &[(10, 100)]);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load Imported");
    assert_eq!(
        world.ui_list(&mut plugin, "ItemRows"),
        ["War Wand \u{2014} Old Exemplar I"]
    );
    run(&mut world, &mut plugin, 6);

    assert_eq!(world.cast_spell_ids(), [101]);
    assert_eq!(world.automation().cast_selection_ids, [10]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddingAShieldPopulatesTheSevenBanesAndImpenetrability
#[test]
fn adding_a_shield_populates_the_seven_banes_and_impenetrability() {
    let mut automation = selection_wired(item_enchant_automation());
    let mut shield = common::item(20, "Tower Shield", 2);
    shield.valid_locations = 0x0020_0000;
    automation.item_entries = vec![shield];
    automation.set_known_self_buffs(vec![
        named_spell(201, 301, "Blade Bane I", ITEM_ENCHANTMENT_SCHOOL),
        named_spell(202, 302, "Impenetrability I", ITEM_ENCHANTMENT_SCHOOL),
    ]);
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 20, 12);

    assert_eq!(world.cast_spell_ids(), [202, 201, 201, 202]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ShieldBaneRowsFollowTheDamageElementEnumOrder
#[test]
fn shield_bane_rows_follow_the_damage_element_enum_order() {
    let mut automation = selection_wired(item_enchant_automation());
    let mut shield = common::item(20, "Tower Shield", 2);
    shield.valid_locations = 0x0020_0000;
    automation.item_entries = vec![shield];
    automation.set_known_self_buffs(vec![
        named_spell(210, 310, "Piercing Bane I", ITEM_ENCHANTMENT_SCHOOL),
        named_spell(211, 311, "Acid Bane I", ITEM_ENCHANTMENT_SCHOOL),
        named_spell(212, 312, "Flame Bane I", ITEM_ENCHANTMENT_SCHOOL),
    ]);
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 20, 12);

    assert_eq!(world.cast_spell_ids(), [211, 212, 210, 210, 211, 212]);
}

/// `Bane(id, family, name)`: an armor bane, cast at the character.
fn bane(spell_id: u32, family: u32, name: &str) -> SpellInfo {
    let mut spell = named_spell(spell_id, family, name, ITEM_ENCHANTMENT_SCHOOL);
    spell.description = "Increases a shield or piece of armor's resistance by 10%. \
                         Target yourself to cast this spell on all of your equipped armor."
        .to_owned();
    spell
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UntrainedItemEnchantmentOverTheLevelThresholdBuildsNoItemRows
#[test]
fn untrained_item_enchantment_over_the_level_threshold_builds_no_item_rows() {
    let mut automation = selection_wired(item_enchant_automation());
    // No Item Enchantment skill at all, and well past
    // BuffWithUntrained-Item's default 80.
    automation.level = 100;
    let mut with_banes = automation.known_self_buffs().to_vec();
    with_banes.push(bane(341, 441, "Impenetrability I"));
    with_banes.push(bane(342, 442, "Acid Bane I"));
    automation.set_known_self_buffs(with_banes);
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 12);

    assert!(world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UntrainedItemEnchantmentUnderTheLevelThresholdStillBuildsRows
#[test]
fn untrained_item_enchantment_under_the_level_threshold_still_builds_rows() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.level = 80;
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 12);

    assert_eq!(world.cast_spell_ids(), [101, 102, 103]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AWandAddedWithNoBuffsCastsNothingAtIt
#[test]
fn a_wand_added_with_no_buffs_casts_nothing_at_it() {
    let (mut world, mut plugin) = open(selection_wired(item_enchant_automation()));

    world.select_object(10);
    world.act(&mut plugin, "AddSelectedItemNoBuffs");
    run(&mut world, &mut plugin, 12);

    assert!(world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemAlreadyEnchantedForLongerThanTheThresholdIsNotDue
#[test]
fn an_item_already_enchanted_for_longer_than_the_threshold_is_not_due() {
    let mut automation = selection_wired(item_enchant_automation());
    // The auras are self-targeted: the character carries their timers.
    automation.active_enchantments = vec![active(101, 201, 1, 1800.0), active(102, 202, 1, 1800.0)];
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 12);

    assert_eq!(world.cast_spell_ids(), [103]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ForceBuffAlsoForcesTheItemEnchantRowsBecauseEqIEndsOnDmD
#[test]
fn force_buff_also_forces_the_item_enchant_rows() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.active_enchantments = vec![
        active(101, 201, 1, 1800.0),
        active(102, 202, 1, 1800.0),
        active(103, 203, 1, 1800.0),
    ];
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 4);
    assert!(world.cast_spell_ids().is_empty());

    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 12, 0.3);

    assert_eq!(world.cast_spell_ids(), [101, 102, 103]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: EverySelfBuffIsCastBeforeAnyItemEnchantment
#[test]
fn every_self_buff_is_cast_before_any_item_enchantment() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.skills = vec![skill(1, "Life Magic", 300)];
    let mut known = automation.known_self_buffs().to_vec();
    known.push(common::spell(
        1,
        10,
        "Increases the caster's Life Magic skill by 10 points.",
    ));
    automation.set_known_self_buffs(known);
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 12);

    assert_eq!(world.cast_spell_ids(), [1, 101, 102, 103]);
}

/// `BaneAutomation`: Item Enchantment at 300 and the given spells.
fn bane_automation(known: Vec<SpellInfo>) -> FakeAutomation {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.object_id = 0x5000_0001;
    automation.skills = vec![skill(ITEM_ENCHANTMENT_SCHOOL, "Item Enchantment", 300)];
    automation.set_known_self_buffs(known);
    selection_wired(automation)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ACharacterTargetedBaneIsCastOnceAndThenCoveredByTheLedger
#[test]
fn a_character_targeted_bane_is_cast_once_and_then_covered_by_the_ledger() {
    let (mut world, mut plugin) = open(bane_automation(vec![bane(301, 401, "Blade Bane I")]));

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [301]);
    let automation = world.automation();
    assert!(automation.active_enchantments.is_empty());
    assert!(
        automation
            .item_enchantments
            .get(&automation.object_id)
            .is_none_or(Vec::is_empty),
        "a bane never lands on the character itself"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImpenetrabilityIsTheFirstCharacterRow
#[test]
fn impenetrability_is_the_first_character_row() {
    let (mut world, mut plugin) = open(bane_automation(vec![
        bane(311, 411, "Acid Bane I"),
        bane(312, 412, "Impenetrability I"),
    ]));

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [312, 311]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: BaneRowsFollowTheProfileLettersInReadingOrder
#[test]
fn bane_rows_follow_the_profile_letters_in_reading_order() {
    let (mut world, mut plugin) = open(bane_automation(vec![
        bane(321, 421, "Bludgeon Bane I"),
        bane(322, 422, "Lightning Bane I"),
    ]));
    set_option(
        &mut world,
        &mut plugin,
        "BuffProfile_Banes",
        ExpressionValue::Number(1.0),
    );
    set_option(
        &mut world,
        &mut plugin,
        "BuffProfile-Banes",
        ExpressionValue::String("LB".into()),
    );

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [322, 321]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CharacterRowsClimbTheFamilyPastTheRenamedTiers
#[test]
fn character_rows_climb_the_family_past_the_renamed_tiers() {
    let tier_one = bane(331, 431, "Acid Bane I");
    let mut olthoi = tier_one.clone();
    olthoi.spell_id = 0x082C;
    olthoi.name = "Olthoi's Bane".to_owned();
    olthoi.tier = 7;
    olthoi.difficulty = 300;
    let mut incantation = tier_one.clone();
    incantation.spell_id = 0x1127;
    incantation.name = "Incantation of Acid Bane".to_owned();
    incantation.tier = 8;
    incantation.difficulty = 400;
    let mut automation = bane_automation(vec![tier_one, olthoi, incantation]);
    automation.skills = vec![skill(ITEM_ENCHANTMENT_SCHOOL, "Item Enchantment", 410)];
    let (mut world, mut plugin) = open(automation);
    set_option(
        &mut world,
        &mut plugin,
        "BuffProfile_Banes",
        ExpressionValue::Number(8.0),
    );

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [0x1127]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CharacterRowsAreAcceptedAgainstTheirOwnTierOneSpell
#[test]
fn character_rows_are_accepted_against_their_own_tier_one_spell() {
    let mut tier_one = bane(331, 431, "Acid Bane I");
    tier_one.component_set = (7, 34, 42, 57);
    let mut decoy = tier_one.clone();
    decoy.spell_id = 900;
    decoy.name = "Acid Ward Self I".to_owned();
    decoy.is_self_targeted = true;
    decoy.difficulty = 5;
    decoy.component_set = (7, 34, 42, 61);
    let mut incantation = tier_one.clone();
    incantation.spell_id = 0x1127;
    incantation.name = "Incantation of Acid Bane".to_owned();
    incantation.tier = 8;
    incantation.difficulty = 400;
    let mut automation = bane_automation(vec![tier_one, decoy, incantation]);
    automation.skills = vec![skill(ITEM_ENCHANTMENT_SCHOOL, "Item Enchantment", 410)];
    let (mut world, mut plugin) = open(automation);
    set_option(
        &mut world,
        &mut plugin,
        "BuffProfile_Banes",
        ExpressionValue::Number(8.0),
    );

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [0x1127]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheBanePresetsAreTheBaneEnumsNotTheProtectionEnums
#[test]
fn the_bane_presets_are_the_bane_enums_not_the_protection_enums() {
    let (mut world, mut plugin) = open(bane_automation(vec![
        bane(331, 431, "Acid Bane I"),
        bane(332, 432, "Blade Bane I"),
    ]));
    set_option(
        &mut world,
        &mut plugin,
        "BuffProfile_Banes",
        ExpressionValue::Number(8.0),
    );

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [331]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ASelfCastBaneLandsOnArmorAndStampsTheLedgerAtTheCharacter
#[test]
fn a_self_cast_bane_lands_on_armor_and_stamps_the_ledger_at_the_character() {
    let mut automation = bane_automation(vec![bane(351, 451, "Blade Bane I")]);
    automation.cast_result_text =
        Some("You cast Blade Bane I on Alduressa Boots, refreshing Blade Bane I".to_owned());
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "log Misc on");

    run(&mut world, &mut plugin, 30);

    assert_eq!(world.cast_spell_ids(), [351]);
    let object_id = world.automation().object_id;
    let name = world.automation().name.clone();
    assert!(
        chat_contains(
            &world,
            &format!("Cast Blade Bane I on {object_id} ending at")
        ),
        "{:?}",
        world.chat()
    );
    assert!(
        chat_contains(
            &world,
            &format!("Buffing: character row {name} ({object_id}) \u{2192} Blade Bane I")
        ),
        "{:?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemRowWhoseSelfAuraLandsOnTheCharacterIsCoveredNextPass
#[test]
fn an_item_row_whose_self_aura_lands_on_the_character_is_covered_next_pass() {
    let mut automation = selection_wired(item_enchant_automation());
    // One resolvable row, so the loop would be unmistakable.
    automation.set_known_self_buffs(vec![self_aura(101, 201, "Aura of Defender Self I")]);
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 20);

    assert_eq!(world.cast_spell_ids(), [101]);
    let automation = world.automation();
    assert!(
        automation
            .active_enchantments
            .iter()
            .any(|held| held.spell_id == 101)
    );
    assert!(
        automation
            .item_enchantments
            .get(&10)
            .is_none_or(Vec::is_empty),
        "the server never enchants the item with a Self aura"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemEnchantRowFallsToTheCastableLowerTierLikeTheSelfWalk
#[test]
fn an_item_enchant_row_falls_to_the_castable_lower_tier_like_the_self_walk() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.skills = vec![skill(ITEM_ENCHANTMENT_SCHOOL, "Item Enchantment", 300)];
    let mut sixth = self_aura(106, 201, "Aura of Defender Self VI");
    sixth.tier = 6;
    sixth.difficulty = 200;
    sixth.formula_component_ids = vec![7];
    let mut first = self_aura(101, 201, "Aura of Defender Self I");
    first.tier = 1;
    first.difficulty = 10;
    first.formula_component_ids = vec![8];
    automation.set_known_self_buffs(vec![sixth, first]);
    automation.components.insert(7, component(7, "Lead Scarab"));
    automation
        .components
        .insert(8, component(8, "Pyreal Scarab"));
    automation.item_entries = vec![war_wand(10), common::item(50, "Pyreal Scarab", 1)];
    let (mut world, mut plugin) = open(automation);

    add_and_run(&mut world, &mut plugin, 10, 12);

    let casts = world.cast_spell_ids();
    assert!(!casts.is_empty());
    assert!(casts.iter().all(|&id| id == 101), "{casts:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemRowPickAndItsLedgerWriteBothSayWhatHappenedOnMisc
#[test]
fn an_item_row_pick_and_its_ledger_write_both_say_what_happened_on_misc() {
    let mut automation = selection_wired(item_enchant_automation());
    automation.set_known_self_buffs(vec![self_aura(101, 201, "Aura of Defender Self I")]);
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "log Misc on");

    add_selected(&mut world, &mut plugin, 10);
    world.clear_chat();
    run(&mut world, &mut plugin, 12);

    assert!(
        chat_contains(
            &world,
            "Buffing: item row War Wand (10) \u{2192} Aura of Defender Self I"
        ),
        "{:?}",
        world.chat()
    );
    assert!(
        world.chat().iter().any(|message| {
            message.contains("Cast Aura of Defender Self I on 10 ending at")
                && message.contains("OVERRIDDEN")
        }),
        "{:?}",
        world.chat()
    );
}
