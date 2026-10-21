//! Buff consumables and the profile's AssistItems: the gem a GemFood row
//! names, the appraised spell of a gem added by hand, and the imported
//! consumable rows a save hands back as it found them (ported from the C#'s
//! `MossTankPanelTests`, over its `FakeAutomation`).

mod common;

use ac_plugin_abi::items::InventoryItem;
use common::World;
use common::automation::FakeAutomation;
use mosstank::MossTank;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};
use mosstank_loot::crafting::{ALL_PEAS, pea_consumable_names};
use mosstank_settings::model::{AllSettings, ConsumableCategory};

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn run(world: &mut World, plugin: &mut MossTank, count: usize) {
    world.act(plugin, "ToggleCombat");
    for _ in 0..count {
        tick(world, plugin, 0.3);
    }
}

/// A gem whose appraisal names `spells`.
fn gem(object_id: u32, name: &str, spells: &[u32]) -> InventoryItem {
    let mut item = common::item(object_id, name, 0x800);
    item.appraised_spell_ids = spells.to_vec();
    item
}

/// `GemFoodAutomation`: full vitals, two buffs, and the gem `name` whose
/// appraisal names spell 3810.
fn gem_food_automation(name: &str) -> FakeAutomation {
    let mut automation = FakeAutomation::with_full_vitals();
    let mut appraised = common::spell(3810, 518, "An appraised effect.");
    appraised.duration_seconds = 600.0;
    automation.set_known_self_buffs(vec![
        appraised,
        common::spell(3811, 519, "A configured effect."),
    ]);
    automation.item_entries = vec![gem(101, name, &[3810])];
    automation
}

/// `SettingsWithGemFood`: the defaults with these GemFood rows.
fn settings_with_gem_food(entries: &[(&str, u32)]) -> String {
    let mut database = mosstank_settings::defaults::create();
    let table = database.find_mut("GemFoodItems").unwrap();
    let name = table.column_index("Name").unwrap();
    let spell = table.column_index("Spell").unwrap();
    table.rows.clear();
    for &(item_name, spell_id) in entries {
        let mut row = VtankRow {
            cells: vec![VtankCell::int(0); table.column_names.len()],
        };
        row.cells[name] = VtankCell::string(item_name);
        row.cells[spell] = VtankCell::int(i32::try_from(spell_id).unwrap());
        table.rows.push(row);
    }
    database.render()
}

/// `SettingsWithoutGemFood`: the defaults with no GemFood table at all.
fn settings_without_gem_food() -> String {
    let mut database = mosstank_settings::defaults::create();
    database
        .tables
        .retain(|(table_name, _)| table_name != "GemFoodItems");
    database.render()
}

fn buff_status(world: &mut World, plugin: &mut MossTank) -> String {
    world.ui_text(plugin, "BuffStatus")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LoadedGemFoodUsesItsConfiguredSpellWithoutAConsumablesProfileRow
#[test]
fn loaded_gem_food_uses_its_configured_spell_without_a_consumables_profile_row() {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.name = "Gem Tester".to_owned();
    automation.set_known_self_buffs(vec![
        common::spell(3810, 518, "An unconfigured effect."),
        common::spell(3811, 519, "A configured effect."),
    ]);
    automation.item_entries = vec![
        gem(100, "Unknown Gem", &[3810]),
        gem(101, "Blackmoor's Favor", &[3810]),
    ];
    let mut world = World::with_automation(automation);
    world.profiles.insert(
        "mosstank/profiles/GemFood.usd",
        &settings_with_gem_food(&[("Unknown Gem", 999_999), ("Blackmoor's Favor", 3811)]),
    );
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load GemFood");
    run(&mut world, &mut plugin, 8);

    assert_eq!(world.automation().used_item_ids, [101]);
    assert!(buff_status(&mut world, &mut plugin).contains("Spell 3811"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SwitchingToAProfileWithoutGemFoodClearsItsOwnedConsumables
#[test]
fn switching_to_a_profile_without_gem_food_clears_its_owned_consumables() {
    let mut world = World::with_automation(gem_food_automation("Blackmoor's Favor"));
    world.profiles.insert(
        "mosstank/profiles/GemFood.usd",
        &settings_with_gem_food(&[("Blackmoor's Favor", 3811)]),
    );
    world.profiles.insert(
        "mosstank/profiles/NoGemFood.usd",
        &settings_without_gem_food(),
    );
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load GemFood");
    world.vt(&mut plugin, "settings load NoGemFood");
    run(&mut world, &mut plugin, 8);

    assert!(world.automation().used_item_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UiAuthoredBuffConsumableStillUsesItsAppraisedSpell
#[test]
fn ui_authored_buff_consumable_still_uses_its_appraised_spell() {
    let mut world = World::with_automation(gem_food_automation("Ui Authored Gem"));
    let mut plugin = world.enable();

    world.select_object(101);
    world.act(&mut plugin, "AddSelectedConsumable");
    run(&mut world, &mut plugin, 8);

    assert_eq!(world.automation().used_item_ids, [101]);
    assert!(buff_status(&mut world, &mut plugin).contains("Spell 3810"));
}

/// The defaults with an `Extension` column on AssistItems, as a profile
/// written by a newer VTank has, and these rows.
fn assist_items_with_extension(rows: &[(&str, i32, &str)]) -> VtankDatabase {
    let mut database = mosstank_settings::defaults::create();
    let table = database.find_mut("AssistItems").unwrap();
    table.column_names.push("Extension".to_owned());
    table.index_flags.push(false);
    for &(name, kind, extension) in rows {
        table.rows.push(VtankRow {
            cells: vec![
                VtankCell::string(name),
                VtankCell::int(kind),
                VtankCell::string(extension),
            ],
        });
    }
    database
}

/// The defaults with these two-cell AssistItems rows.
fn assist_items(rows: &[(&str, i32)]) -> VtankDatabase {
    let mut database = mosstank_settings::defaults::create();
    let table = database.find_mut("AssistItems").unwrap();
    for &(name, kind) in rows {
        table.rows.push(VtankRow {
            cells: vec![VtankCell::string(name), VtankCell::int(kind)],
        });
    }
    database
}

/// A world with nothing owned and `database` in the profile folder as
/// `<name>.usd`, loaded.
fn load_profile(name: &str, database: &VtankDatabase) -> (World, MossTank) {
    let mut world = World::with_automation(FakeAutomation::default());
    world
        .profiles
        .insert(&format!("mosstank/profiles/{name}.usd"), &database.render());
    let mut plugin = world.enable();
    world.vt(&mut plugin, &format!("settings load {name}"));
    (world, plugin)
}

/// The AssistItems table of the selected profile as saved
/// (`SettingsKey(panel.SelectedMacroProfile)`).
fn saved_assist_items(world: &mut World, plugin: &mut MossTank) -> VtankTable {
    let selected = world.ui_text(plugin, "SelectedMacroProfile");
    let key = format!("mosstank/profiles/{selected}");
    VtankDatabase::parse(world.profiles.get(&key).unwrap())
        .unwrap()
        .find("AssistItems")
        .unwrap()
        .clone()
}

fn cell(table: &VtankTable, row: &VtankRow, column: &str) -> VtankCell {
    row.cells[table.column_index(column).unwrap()].clone()
}

fn rows_named<'t>(table: &'t VtankTable, name: &str) -> Vec<&'t VtankRow> {
    table
        .rows
        .iter()
        .filter(|row| cell(table, row, "Object").as_string() == name)
        .collect()
}

fn live_peas(plugin: &MossTank) -> Vec<String> {
    pea_consumable_names(&plugin.tank().unwrap().settings().combat)
        .iter()
        .cloned()
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedAssistItemsMapKindsAndPreserveUnknownCustomRowsOnSave
#[test]
fn imported_assist_items_map_kinds_and_preserve_unknown_custom_rows_on_save() {
    let mut database = assist_items_with_extension(&[
        ("Health Kit", 0, "keep-health"),
        ("Bread", 1, "keep-food"),
        ("Unknown", 99, "keep-unknown"),
    ]);
    let mut settings = AllSettings::default();
    mosstank_settings::profile::load(&database.render(), &mut settings, None).unwrap();
    mosstank_settings::items::assist_items::apply(&mut settings.combat);

    let combat = &mut settings.combat;
    assert_eq!(
        combat.consumable_categories.get("Health Kit"),
        Some(&ConsumableCategory::HealthKit)
    );
    assert_eq!(
        combat.consumable_categories.get("Bread"),
        Some(&ConsumableCategory::HealthFood)
    );
    assert!(!combat.consumable_names.contains("Unknown"));

    combat.consumable_names.remove("Health Kit");
    combat.consumable_categories.remove("Health Kit");
    combat
        .imported_assist_items
        .retain(|item| item.name != "Health Kit");
    mosstank_settings::profile::save(&mut database, &mut settings).unwrap();
    let saved = database.find("AssistItems").unwrap();
    assert!(rows_named(saved, "Health Kit").is_empty());
    let food = rows_named(saved, "Bread");
    assert_eq!(food.len(), 1);
    assert_eq!(cell(saved, food[0], "Type").as_int().unwrap(), 1);
    assert_eq!(cell(saved, food[0], "Extension").as_string(), "keep-food");
    let unknown = rows_named(saved, "Unknown");
    assert_eq!(unknown.len(), 1);
    assert_eq!(cell(saved, unknown[0], "Type").as_int().unwrap(), 99);
    assert_eq!(
        cell(saved, unknown[0], "Extension").as_string(),
        "keep-unknown"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RemovingAnImportedConsumableDeletesOnlyItsExactAssistRow
#[test]
fn removing_an_imported_consumable_deletes_only_its_exact_assist_row() {
    let (mut world, mut plugin) = load_profile(
        "AssistRemove",
        &assist_items_with_extension(&[
            ("Bread", 1, "bread-custom"),
            ("Mana Cake", 5, "cake-custom"),
        ]),
    );
    // The profile's gems and food come first.
    assert_eq!(
        world.ui_list(&mut plugin, "ConsumableRows"),
        [
            "Asheron's Benediction",
            "Blackmoor's Favor",
            "Bread",
            "Mana Cake"
        ]
    );
    world.act_int(&mut plugin, "SelectConsumableRow", 2);

    let saved = saved_assist_items(&mut world, &mut plugin);
    assert!(rows_named(&saved, "Bread").is_empty());
    let cake = rows_named(&saved, "Mana Cake");
    assert_eq!(cake.len(), 1);
    assert_eq!(
        cell(&saved, cake[0], "Extension").as_string(),
        "cake-custom"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SwitchingToAnEmptyAssistItemsTableClearsImportedConsumables
#[test]
fn switching_to_an_empty_assist_items_table_clears_imported_consumables() {
    let (mut world, mut plugin) = load_profile("Assist", &assist_items(&[("Bread", 1)]));
    world.profiles.insert(
        "mosstank/profiles/NoAssist.usd",
        &mosstank_settings::defaults::create().render(),
    );
    assert!(
        world
            .ui_list(&mut plugin, "ConsumableRows")
            .contains(&"Bread".to_owned())
    );

    world.vt(&mut plugin, "settings load NoAssist");

    assert!(
        !world
            .ui_list(&mut plugin, "ConsumableRows")
            .contains(&"Bread".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddingAllPeasOverAnImportedSinglePeaRowActsAndPersists
#[test]
fn adding_all_peas_over_an_imported_single_pea_row_acts_and_persists() {
    let (mut world, mut plugin) = load_profile(
        "AssistAllPeas",
        &assist_items_with_extension(&[(ALL_PEAS, 9, "keep-custom")]),
    );
    assert!(!live_peas(&plugin).iter().any(|name| name == ALL_PEAS));

    world.act(&mut plugin, "AddAllPeas");

    assert!(live_peas(&plugin).iter().any(|name| name == ALL_PEAS));
    let saved = saved_assist_items(&mut world, &mut plugin);
    let rows = rows_named(&saved, ALL_PEAS);
    assert_eq!(rows.len(), 1);
    assert_eq!(cell(&saved, rows[0], "Type").as_int().unwrap(), 11);
    assert_eq!(
        cell(&saved, rows[0], "Extension").as_string(),
        "keep-custom"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MismatchedPeaNameAndKindPairsAuthorizeNothing
#[test]
fn mismatched_pea_name_and_kind_pairs_authorize_nothing() {
    // The wildcard name carrying the single-pea kind, and a single pea
    // carrying the wildcard kind.
    let (mut world, mut plugin) = load_profile(
        "AssistMismatch",
        &assist_items(&[(ALL_PEAS, 9), ("Gold Pea", 11)]),
    );

    let rows = world.ui_list(&mut plugin, "ConsumableRows");
    assert!(rows.contains(&ALL_PEAS.to_owned()), "{rows:?}");
    assert!(rows.contains(&"Gold Pea".to_owned()), "{rows:?}");
    assert!(live_peas(&plugin).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DuplicateImportedRowsAndCustomCellsSurviveAnUnrelatedSave
#[test]
fn duplicate_imported_rows_and_custom_cells_survive_an_unrelated_save() {
    let (mut world, mut plugin) = load_profile(
        "AssistDuplicate",
        &assist_items_with_extension(&[
            ("Bread", 1, "first-custom"),
            ("Bread", 5, "second-custom"),
        ]),
    );

    world.act(&mut plugin, "ToggleAutoStack");

    let saved = saved_assist_items(&mut world, &mut plugin);
    let bread = rows_named(&saved, "Bread");
    assert_eq!(bread.len(), 2);
    let kinds: Vec<i32> = bread
        .iter()
        .map(|row| cell(&saved, row, "Type").as_int().unwrap())
        .collect();
    assert_eq!(kinds, [1, 5]);
    let extensions: Vec<String> = bread
        .iter()
        .map(|row| cell(&saved, row, "Extension").as_string().to_owned())
        .collect();
    assert_eq!(extensions, ["first-custom", "second-custom"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnExplicitKindReKindsEveryRowForTheNameAndKeepsThem
#[test]
fn an_explicit_kind_re_kinds_every_row_for_the_name_and_keeps_them() {
    let (mut world, mut plugin) = load_profile(
        "AssistTwoPeaRows",
        &assist_items_with_extension(&[
            (ALL_PEAS, 9, "first-custom"),
            (ALL_PEAS, 1, "second-custom"),
        ]),
    );

    world.act(&mut plugin, "AddAllPeas");

    let saved = saved_assist_items(&mut world, &mut plugin);
    let peas = rows_named(&saved, ALL_PEAS);
    assert_eq!(peas.len(), 2);
    let kinds: Vec<i32> = peas
        .iter()
        .map(|row| cell(&saved, row, "Type").as_int().unwrap())
        .collect();
    assert_eq!(kinds, [11, 11]);
    let extensions: Vec<String> = peas
        .iter()
        .map(|row| cell(&saved, row, "Extension").as_string().to_owned())
        .collect();
    assert_eq!(extensions, ["first-custom", "second-custom"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ThePeriodicSweepDoesNotOverruleAnImportedConsumableKind
#[test]
fn the_periodic_sweep_does_not_overrule_an_imported_consumable_kind() {
    let mut automation = FakeAutomation::default();
    let mut bread = common::item(77, "Bread", 0x20);
    bread.booster_vital = 2;
    automation.item_entries = vec![bread];
    let mut world = World::with_automation(automation);
    world.profiles.insert(
        "mosstank/profiles/AssistBread.usd",
        &assist_items(&[("Bread", 5)]).render(),
    );
    let mut plugin = world.enable();

    world.vt(&mut plugin, "settings load AssistBread");
    for _ in 0..4 {
        tick(&mut world, &mut plugin, 0.6);
    }

    world.act(&mut plugin, "ToggleAutoStack");
    let saved = saved_assist_items(&mut world, &mut plugin);
    let rows = rows_named(&saved, "Bread");
    assert_eq!(rows.len(), 1);
    assert_eq!(cell(&saved, rows[0], "Type").as_int().unwrap(), 5);
}
