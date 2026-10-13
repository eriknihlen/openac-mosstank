//! The Items, Consumables, Buffs and Vitals pages, driven through the
//! window bindings (ported from the C#'s `MossTankPanelTests`).

mod common;

use common::{World, item, spell};
use mosstank::MossTank;
use mosstank_formats::usd::VtankDatabase;

fn fresh() -> (World, MossTank) {
    let mut world = World::panel_character("Item Tester");
    let plugin = world.enable();
    (world, plugin)
}

fn add(world: &mut World, plugin: &mut MossTank, handler: &str, object_id: u32) {
    world.select_object(object_id);
    world.act(plugin, handler);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ItemsAndConsumablesTabsAddTheSelectedOwnedItemToRealProfiles
#[test]
fn the_items_and_consumables_tabs_add_the_selection_to_real_profiles() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.own_items(vec![
        item(10, "Imperil Lens", 0x8000),
        item(11, "Iron Phial of Imperil", 0x100),
        item(12, "Black Marrow Pea", 0x20),
    ]);
    add(&mut world, p, "AddSelectedItem", 10);
    assert!(world.ui_text(p, "ItemProfileText").contains("Imperil Lens"));
    add(&mut world, p, "AddSelectedConsumable", 11);
    world.act(p, "AddAllPeas");
    let consumables = world.ui_text(p, "ConsumableProfileText");
    assert!(consumables.contains("Iron Phial of Imperil"));
    assert!(consumables.contains(mosstank_loot::crafting::ALL_PEAS));
    assert!(
        world
            .ui_list(p, "ItemRows")
            .iter()
            .any(|row| row == "Imperil Lens")
    );
    world.act(p, "RemoveSelectedItem");
    assert!(
        !world
            .ui_list(p, "ItemRows")
            .iter()
            .any(|row| row == "Imperil Lens")
    );
    assert_eq!(world.ui_list(p, "ConsumableRows").len(), 2);
    world.act(p, "RemoveSelectedConsumable");
    assert_eq!(world.ui_list(p, "ConsumableRows").len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ItemsGridNameClickDeletesAndHandsClickCyclesHandedness
#[test]
fn the_items_grid_deletes_on_the_name_and_cycles_hands() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.own_items(vec![item(10, "Fire Sword", 1), item(11, "Ice Wand", 1)]);
    add(&mut world, p, "AddSelectedItem", 10);
    add(&mut world, p, "AddSelectedItem", 11);
    assert_eq!(
        world.ui_list(p, "ItemNameColumn"),
        ["Fire Sword", "Ice Wand"]
    );
    assert_eq!(world.ui_list(p, "ItemHandsColumn")[0], "Auto");
    for expected in ["1-Handed", "2-Handed", "Auto"] {
        world.act_int(p, "CycleItemHandsAt", 0);
        assert_eq!(world.ui_list(p, "ItemHandsColumn")[0], expected);
    }
    assert_eq!(world.ui_list(p, "ItemHandsColumn")[1], "Auto");
    world.act_int(p, "DeleteItemRowAt", 0);
    assert_eq!(world.ui_list(p, "ItemNameColumn"), ["Ice Wand"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ItemHandsColumnDoesNotReallocateOnEveryReadAndNoBuffSuffixNeverLeaks
/// The allocation half has no meaning for pushed values; the caption half
/// is kept.
#[test]
fn the_no_buffs_suffix_stays_on_the_name_and_out_of_the_hands() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.own_items(vec![item(10, "Fire Sword", 1)]);
    add(&mut world, p, "AddSelectedItemNoBuffs", 10);
    assert_eq!(
        world.ui_list(p, "ItemNameColumn"),
        ["Fire Sword   [no buffs]"]
    );
    assert_eq!(world.ui_list(p, "ItemHandsColumn")[0], "Auto");
    world.act_int(p, "CycleItemHandsAt", 0);
    assert_eq!(world.ui_list(p, "ItemHandsColumn")[0], "1-Handed");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ConsumablesLeftListRowClickRemovesTheRowDirectly
#[test]
fn a_click_on_a_consumable_row_removes_it() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.own_items(vec![
        item(20, "Iron Phial of Imperil", 0x100),
        item(21, "Iron Phial of Vulnerability", 0x100),
    ]);
    add(&mut world, p, "AddSelectedConsumable", 20);
    add(&mut world, p, "AddSelectedConsumable", 21);
    assert_eq!(world.ui_list(p, "ConsumableRows").len(), 2);
    world.act_int(p, "SelectConsumableRow", 0);
    assert_eq!(world.ui_list(p, "ConsumableRows").len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ExcludedComponentsGridAddsBySelectionAndDeletesByAnyCellClick
#[test]
fn the_excluded_components_grid_adds_the_selection_and_deletes_on_a_click() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let mut scarab = item(20, "Charged Yellow Scarab", 0x20);
    scarab.icon_id = 0x0600_1234;
    world.own_items(vec![scarab]);
    assert!(world.ui_list(p, "ExcludedComponentRows").is_empty());
    add(&mut world, p, "AddSelectedComponent", 20);
    assert_eq!(
        world.ui_list(p, "ExcludedComponentRows"),
        ["Charged Yellow Scarab"]
    );
    assert_eq!(world.ui_uints(p, "ExcludedComponentIcons")[0], 0x0600_1234);
    world.act(p, "AddSelectedComponent");
    assert_eq!(
        world.ui_list(p, "ExcludedComponentRows"),
        ["Charged Yellow Scarab"]
    );
    world.act_int(p, "DeleteExcludedComponentAt", 0);
    assert!(world.ui_list(p, "ExcludedComponentRows").is_empty());
}

fn two_buffs(world: &mut World) {
    world.know_spells(vec![
        spell(1, 10, "Increases the caster's Strength by 10 points."),
        spell(2, 20, "Increases the caster's Focus by 10 points."),
    ]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ExtraBuffAndBlacklistedFamilyNamesPersistAcrossSessions
#[test]
fn extra_buffs_and_blacklisted_families_persist_across_sessions() {
    let mut world = World::panel_character("Persist Check");
    two_buffs(&mut world);
    let mut first = world.enable();
    let p = &mut first;
    world.act(p, "ShowExtraBuffPicker");
    world.act_int(p, "PickBuffAt", 0);
    world.act(p, "ShowBlacklistedBuffPicker");
    world.act_int(p, "PickBuffAt", 1);
    assert_eq!(world.ui_list(p, "ExtraBuffRows"), ["Spell 1"]);
    assert_eq!(world.ui_list(p, "BlacklistedBuffFamilyRows"), ["Spell 2"]);

    let usd: Vec<String> = world
        .profiles
        .keys()
        .filter(|key| key.ends_with(".usd"))
        .map(str::to_owned)
        .collect();
    assert_eq!(usd.len(), 1);
    let document = VtankDatabase::parse(world.profiles.get(&usd[0]).unwrap()).unwrap();
    let extra = &document.find("ExtraBuffSpells").unwrap().rows;
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0].cells[0].as_int().unwrap(), 1);
    let anti = &document.find("AntiExtraBuffSpells").unwrap().rows;
    assert_eq!(anti.len(), 1);
    assert_eq!(anti[0].cells[0].as_int().unwrap(), 2);
    world.disable(p);

    let mut second = world.enable();
    let p = &mut second;
    assert_eq!(world.ui_list(p, "ExtraBuffRows"), ["Spell 1"]);
    assert_eq!(world.ui_list(p, "BlacklistedBuffFamilyRows"), ["Spell 2"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: BuffPickerAddsToTheRequestedListAndAnyCellClickDeletes
#[test]
fn the_buff_picker_adds_to_the_list_it_was_opened_for() {
    let mut world = World::panel_character("Picker");
    two_buffs(&mut world);
    let mut plugin = world.enable();
    let p = &mut plugin;
    assert!(!world.ui_bool(p, "BuffPickerVisible"));
    world.act(p, "ShowExtraBuffPicker");
    assert!(world.ui_bool(p, "BuffPickerVisible"));
    assert_eq!(world.ui_list(p, "BuffPickerRows").len(), 2);
    world.act_text(p, "SetBuffPickerSearchText", "Spell 1");
    assert_eq!(world.ui_list(p, "BuffPickerRows"), ["Spell 1"]);
    world.act_int(p, "PickBuffAt", 0);
    assert!(!world.ui_bool(p, "BuffPickerVisible"));
    assert_eq!(world.ui_list(p, "ExtraBuffRows"), ["Spell 1"]);
    assert!(world.ui_list(p, "BlacklistedBuffFamilyRows").is_empty());

    world.act(p, "ShowBlacklistedBuffPicker");
    world.act_text(p, "SetBuffPickerSearchText", "");
    world.act_int(p, "PickBuffAt", 1);
    assert_eq!(world.ui_list(p, "BlacklistedBuffFamilyRows"), ["Spell 2"]);
    world.act_int(p, "DeleteExtraBuffAt", 0);
    assert!(world.ui_list(p, "ExtraBuffRows").is_empty());
    world.act_int(p, "DeleteBlacklistedBuffFamilyAt", 0);
    assert!(world.ui_list(p, "BlacklistedBuffFamilyRows").is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ItemProfilesPersistThroughHostScopedStorage
#[test]
fn item_profiles_persist_through_the_plugins_storage() {
    let (mut world, mut first) = fresh();
    world.own_items(vec![item(10, "Imperil Lens", 0x8000)]);
    add(&mut world, &mut first, "AddSelectedItem", 10);
    world.disable(&mut first);
    let mut second = world.enable();
    assert!(
        world
            .ui_text(&mut second, "ItemProfileText")
            .contains("Imperil Lens")
    );
    assert!(
        world
            .plugin_data
            .keys()
            .any(|key| key.starts_with("profiles/macro/"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VitalThresholdsPersistWithTheProfile
#[test]
fn vital_thresholds_persist_with_the_profile() {
    let (mut world, mut first) = fresh();
    let p = &mut first;
    world.act_float(p, "SetNormalHealth", 0.42);
    world.act_float(p, "SetNoTargetMana", 0.88);
    world.act(p, "ToggleHelpOthers");
    world.disable(p);
    let mut second = world.enable();
    let p = &mut second;
    assert!((world.ui_float(p, "NormalHealthValue") - 0.42).abs() < 0.005);
    assert!((world.ui_float(p, "NoTargetManaValue") - 0.88).abs() < 0.005);
    assert!(!world.ui_bool(p, "HelpOthersEnabled"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AutoStackAndAutoCramUseVtankDefaultsAndPersistPerProfile
#[test]
fn auto_stack_and_cram_use_vtanks_defaults_and_persist() {
    let (mut world, mut first) = fresh();
    let p = &mut first;
    assert!(world.ui_bool(p, "AutoStackEnabled"));
    assert!(!world.ui_bool(p, "AutoCramEnabled"));
    assert!(world.ui_bool(p, "AutoCraftItemsEnabled"));
    assert!(world.ui_bool(p, "RefillWornManaEnabled"));
    for toggle in [
        "ToggleAutoStack",
        "ToggleAutoCram",
        "ToggleAutoCraftItems",
        "ToggleRefillWornMana",
    ] {
        world.act(p, toggle);
    }
    world.act_float(p, "SetRefillWornMana", 0.44);
    world.disable(p);
    let mut second = world.enable();
    let p = &mut second;
    assert!(!world.ui_bool(p, "AutoStackEnabled"));
    assert!(world.ui_bool(p, "AutoCramEnabled"));
    assert!(!world.ui_bool(p, "AutoCraftItemsEnabled"));
    assert!(!world.ui_bool(p, "RefillWornManaEnabled"));
    assert!((world.ui_float(p, "RefillWornManaValue") - 0.44).abs() < 0.005);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VitalsPercentWrappersReadAndWriteTheSameFieldAsTheZeroToOnePair
#[test]
fn the_percent_sliders_read_and_write_the_same_field_as_the_fractions() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act_float(p, "SetNormalHealth", 0.42);
    assert!((world.ui_float(p, "NormalHealthPercent") - 42.0).abs() < 0.005);
    world.act_float(p, "SetHelperManaPercent", 65.0);
    assert!((world.ui_float(p, "HelperManaValue") - 0.65).abs() < 0.0005);
    assert!((world.ui_float(p, "HelperManaPercent") - 65.0).abs() < 0.005);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FastCastProjectileAwarenessAndDebuffFallbackToggleTheirVtankDefaultsAndPersist
#[test]
fn fast_cast_wall_awareness_and_debuff_fallback_toggle_and_persist() {
    let (mut world, mut first) = fresh();
    let p = &mut first;
    assert!(!world.ui_bool(p, "FastCastBuffsEnabled"));
    assert!(world.ui_bool(p, "DontShootAtWallsEnabled"));
    assert!(!world.ui_bool(p, "DebuffFallbackEnabled"));
    for toggle in [
        "ToggleFastCastBuffs",
        "ToggleDontShootAtWalls",
        "ToggleDebuffFallback",
    ] {
        world.act(p, toggle);
    }
    assert!(world.ui_bool(p, "FastCastBuffsEnabled"));
    assert!(!world.ui_bool(p, "DontShootAtWallsEnabled"));
    assert!(world.ui_bool(p, "DebuffFallbackEnabled"));
    world.disable(p);
    let mut second = world.enable();
    let p = &mut second;
    assert!(world.ui_bool(p, "FastCastBuffsEnabled"));
    assert!(!world.ui_bool(p, "DontShootAtWallsEnabled"));
    assert!(world.ui_bool(p, "DebuffFallbackEnabled"));
}

/// The vitals and coverage panel's character (`UpdateTick_...`,
/// `RetainedLabelReads_...`): hurt a little, two attributes or one, the
/// skills given, and one self buff.
fn vitals_character(
    skills: Vec<ac_plugin_abi::character::Skill>,
    attributes: Vec<ac_plugin_abi::character::Attribute>,
) -> common::automation::FakeAutomation {
    let mut automation = common::automation::FakeAutomation::default();
    automation.health = (90, 100);
    automation.stamina = (80, 110);
    automation.mana = (70, 120);
    automation.skills = skills;
    automation.attributes = attributes;
    automation.set_known_self_buffs(vec![spell(
        1,
        10,
        "Increases the caster's Life Magic skill by 10 points.",
    )]);
    automation
}

fn trained(
    skill_id: u32,
    name: &str,
    current: u32,
    training: ac_plugin_abi::character::Training,
) -> ac_plugin_abi::character::Skill {
    ac_plugin_abi::character::Skill {
        training,
        ..common::automation::skill(skill_id, name, current)
    }
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UpdateTick_RefreshesRareCoverageAndChangedVitals
/// The vitals line follows a change on the next tick; the coverage line
/// (attributes, trained skills, buff lines) is refreshed more rarely, and
/// a change to the known self buffs refreshes it at once.
#[test]
fn the_tick_refreshes_rare_coverage_and_changed_vitals() {
    use ac_plugin_abi::character::Training;
    let automation = vitals_character(
        vec![trained(1, "Life Magic", 300, Training::Trained)],
        vec![common::automation::attribute(0, "Strength", 100)],
    );
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();
    tick(&mut world, &mut plugin, 0.0);

    world.automation().health = (75, 100);
    world.automation().skills = vec![
        trained(1, "Life Magic", 300, Training::Trained),
        trained(2, "War Magic", 350, Training::Specialized),
    ];
    tick(&mut world, &mut plugin, 0.5);

    assert!(
        world
            .ui_text(&mut plugin, "Vitals")
            .starts_with("Health 75/100")
    );
    assert_eq!(
        world.ui_text(&mut plugin, "Coverage"),
        "1 attributes, 1 trained skills, 1 buff lines"
    );

    tick(&mut world, &mut plugin, 0.5);
    assert_eq!(
        world.ui_text(&mut plugin, "Coverage"),
        "1 attributes, 2 trained skills, 1 buff lines"
    );

    world.automation().set_known_self_buffs(vec![
        spell(
            1,
            10,
            "Increases the caster's Life Magic skill by 10 points.",
        ),
        spell(
            2,
            11,
            "Increases the caster's War Magic skill by 10 points.",
        ),
    ]);
    tick(&mut world, &mut plugin, 0.0);

    assert_eq!(
        world.ui_text(&mut plugin, "Coverage"),
        "1 attributes, 2 trained skills, 2 buff lines"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RetainedLabelReads_UseUpdateSideSnapshotsWithoutAllocating
/// The vitals and coverage lines are worked out on the tick, not on each
/// read: reading them again and again gives the same text and never asks
/// the client. (The C#'s zero-allocation and same-reference checks are
/// .NET mechanics; a window binding here is a value worked out by the
/// shell, so what carries over is that a read does not reach the client.
/// The C#'s count of one self-buff read in all is not kept: the plugin
/// pushes its windows after it starts, and the buff picker's rows read the
/// self buffs as the C#'s retained UI does on every frame, UI-1.)
#[test]
fn the_vitals_and_coverage_lines_are_read_from_the_tick_snapshot() {
    use ac_plugin_abi::character::Training;
    let automation = vitals_character(
        vec![
            trained(1, "Life Magic", 300, Training::Trained),
            trained(2, "War Magic", 350, Training::Specialized),
            trained(3, "Run", 100, Training::Untrained),
        ],
        vec![
            common::automation::attribute(0, "Strength", 100),
            common::automation::attribute(1, "Endurance", 100),
        ],
    );
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();

    tick(&mut world, &mut plugin, 0.0);

    let vitals = world.ui_text(&mut plugin, "Vitals");
    let coverage = world.ui_text(&mut plugin, "Coverage");
    assert_eq!(vitals, "Health 90/100   Stam 80/110   Mana 70/120");
    assert_eq!(coverage, "2 attributes, 2 trained skills, 1 buff lines");
    let calls = world.fake.calls.len();
    let buff_reads = world.automation().known_self_buff_reads;
    for _ in 0..100 {
        assert_eq!(world.ui_text(&mut plugin, "Vitals"), vitals);
        assert_eq!(world.ui_text(&mut plugin, "Coverage"), coverage);
    }
    assert_eq!(world.automation().known_self_buff_reads, buff_reads);
    assert!(
        world.fake.calls[calls..].iter().all(|import| !matches!(
            import,
            ac_plugin_abi::Import::CharacterInfo
                | ac_plugin_abi::Import::CharacterSkills
                | ac_plugin_abi::Import::CharacterAttributes
                | ac_plugin_abi::Import::SpellsKnown
        )),
        "a read asked the client"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ExcludedComponentIconsDoesNotScanLiveInventoryOnEveryRead
/// The excluded components' icons are kept with the rows: reading them
/// does not read the inventory again.
#[test]
fn the_excluded_component_icons_do_not_scan_the_inventory_on_every_read() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let mut scarab = item(20, "Charged Yellow Scarab", 0x20);
    scarab.icon_id = 0x0600_1234;
    world.own_items(vec![scarab]);
    add(&mut world, p, "AddSelectedComponent", 20);
    assert_eq!(world.ui_uints(p, "ExcludedComponentIcons")[0], 0x0600_1234);

    let reads_after_add = world.requests::<ac_plugin_abi::calls::ItemsOwned>().len();
    for _ in 0..5 {
        let _ = world.ui_uints(p, "ExcludedComponentIcons");
    }

    assert_eq!(
        world.requests::<ac_plugin_abi::calls::ItemsOwned>().len(),
        reads_after_add
    );
}
