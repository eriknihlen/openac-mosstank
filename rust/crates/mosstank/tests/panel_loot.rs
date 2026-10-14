//! The loot editor and the loot profile picker and commands, driven through
//! the window bindings (ported from the C#'s `MossTankPanelTests`).

mod common;

use common::World;
use mosstank::MossTank;
use mosstank_stores::directory;
use mosstank_stores::loot_store::NO_ACTIVE_PROFILE;

const MALFORMED: &str = "UTL\r\n1\r\n1\r\nunfinished";

fn looter() -> (World, MossTank) {
    let mut world = World::with_character("Looter");
    let plugin = world.enable();
    (world, plugin)
}

fn cdf(world: &World) -> String {
    world
        .profiles
        .get(&directory::cdf_file_name("Looter", "Coldeve"))
        .unwrap()
        .to_owned()
}

fn rows(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "LootRuleRows")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootingUsesVtankDefaultsAndPersistsTheOrderedRuleEditor
#[test]
fn looting_uses_vtanks_defaults_and_persists_the_rule_editor() {
    let mut world = World::panel_character("Loot Editor");
    let mut first = world.enable();
    let p = &mut first;
    assert!(!world.ui_bool(p, "LootEnabled"));
    assert!(!world.ui_bool(p, "LootPriorityBoostEnabled"));
    assert!(rows(&mut world, p).is_empty());
    world.act(p, "ToggleLooting");
    world.act(p, "ToggleLootPriorityBoost");
    world.act(p, "AddLootRule");
    world.act_text(p, "SetLootExpressionDraft", "name ~= coin && value >= 10");
    world.act(p, "ApplyLootRule");
    world.act_text(p, "SelectLootAction", "KeepUpTo");
    world.act(p, "LootKeepCountUp");
    world.act(p, "LootPriorityUp");
    world.act(p, "LootRangeDown");
    world.disable(p);

    let mut second = world.enable();
    let p = &mut second;
    assert!(world.ui_bool(p, "LootEnabled"));
    assert!(world.ui_bool(p, "LootPriorityBoostEnabled"));
    let rules = rows(&mut world, p);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].contains("KeepUpTo"), "{rules:?}");
    assert!(rules[0].contains("name ~= coin"), "{rules:?}");
    assert_eq!(world.ui_text(p, "LootKeepCountText"), "Keep up to 2");
    assert_eq!(world.ui_text(p, "LootPriorityText"), "Priority 1");
    assert_eq!(world.ui_text(p, "LootRangeText"), "Corpse range 0m");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootProfilesAreIndependentNamedDocuments
#[test]
fn loot_profiles_are_independent_named_documents() {
    let mut world = World::panel_character("Looter");
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.act(p, "AddLootRule");
    world.act_text(p, "SetLootExpressionDraft", "name ~= coin");
    world.act(p, "ApplyLootRule");
    world.vt(p, "loot save Currency");
    world.act(p, "AddLootRule");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Currency");
    assert_eq!(rows(&mut world, p).len(), 2);
    world.act_text(p, "SelectLootProfile", "By char");
    assert_eq!(rows(&mut world, p).len(), 1);
    world.act_text(p, "SelectLootProfile", "Currency");
    assert_eq!(rows(&mut world, p).len(), 2);
    assert!(world.profiles.contains("mosstank/loot/Currency.utl"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootProfilePickerRejectsMalformedCandidateWithoutChangingActiveProfileOrFiles
#[test]
fn the_picker_rejects_a_malformed_profile_without_changing_anything() {
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    world.vt(p, "loot new Safe");
    world.act(p, "AddLootRule");
    let safe = rows(&mut world, p);
    world.profiles.insert("mosstank/loot/Loot5.utl", MALFORMED);
    let before = cdf(&world);
    world.act_text(p, "SelectLootProfile", "Loot5");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Safe");
    assert_eq!(rows(&mut world, p), safe);
    assert_eq!(cdf(&world), before);
    assert_eq!(
        world.profiles.get("mosstank/loot/Loot5.utl"),
        Some(MALFORMED)
    );
    let notice = world.ui_text(p, "LootEditorNotice");
    assert!(notice.contains("Loot5"), "{notice}");
    assert!(notice.contains("Active profile remains Safe"), "{notice}");
    assert!(
        !world
            .infos()
            .iter()
            .any(|line| line.contains("Loaded loot profile Loot5"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootSaveCommandRefusesToCopyAnIncompleteActiveProfile
#[test]
fn loot_save_refuses_to_copy_an_incomplete_profile() {
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    let partial = "UTL\r\n1\r\n2\r\nKeep\r\n\r\n0;1\r\nunfinished";
    world.profiles.insert("mosstank/loot/Partial.utl", partial);
    world.vt(p, "loot load Partial");
    world.act(p, "AddLootRule");
    let before_rows = rows(&mut world, p);
    let before = cdf(&world);
    world.vt(p, "loot save Copy");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Partial");
    assert_eq!(rows(&mut world, p), before_rows);
    assert_eq!(
        world.profiles.get("mosstank/loot/Partial.utl"),
        Some(partial)
    );
    assert!(!world.profiles.contains("mosstank/loot/Copy.utl"));
    assert_eq!(cdf(&world), before);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.to_lowercase().contains("incomplete"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootSaveCommandRefusesWithAnInactiveStartupProfile
#[test]
fn loot_save_refuses_with_an_inactive_startup_profile() {
    let (mut world, mut first) = looter();
    world.vt(&mut first, "loot new Broken");
    world.profiles.insert("mosstank/loot/Broken.utl", MALFORMED);
    let before = cdf(&world);
    world.disable(&mut first);
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.clear_chat();
    world.vt(p, "loot save Copy");
    assert_eq!(world.ui_text(p, "LootProfileName"), NO_ACTIVE_PROFILE);
    assert_eq!(
        world.profiles.get("mosstank/loot/Broken.utl"),
        Some(MALFORMED)
    );
    assert!(!world.profiles.contains("mosstank/loot/Copy.utl"));
    assert_eq!(cdf(&world), before);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.to_lowercase().contains("no complete active profile"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootLoadCommandReportsMalformedCandidateWithoutAFalseLoadedMessage
#[test]
fn loot_load_reports_a_malformed_file_without_claiming_a_load() {
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    world.vt(p, "loot new Safe");
    world.act(p, "AddLootRule");
    world.profiles.insert("mosstank/loot/Loot5.utl", MALFORMED);
    world.clear_chat();
    world.vt(p, "loot load Loot5.utl");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Safe");
    let chat = world.chat();
    assert!(chat.iter().any(|line| line.contains("Loot5")
        && line.contains("could not be read")
        && line.contains("Active profile remains Safe")));
    assert!(
        chat.iter()
            .any(|line| line.contains("Could not load loot profile Loot5.utl"))
    );
    assert!(!chat.iter().any(|line| line.contains("Loaded loot profile")));
    assert_eq!(
        world.profiles.get("mosstank/loot/Loot5.utl"),
        Some(MALFORMED)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ReloadingActiveLootProfileDoesNotOverwriteExternalMalformedEdit
#[test]
fn reloading_the_active_profile_does_not_overwrite_an_external_bad_edit() {
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    world.vt(p, "loot new Safe");
    world.act(p, "AddLootRule");
    world.profiles.insert("mosstank/loot/Safe.utl", MALFORMED);
    world.vt(p, "loot load Safe");
    world.act(p, "ToggleLootPriorityBoost");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Safe");
    assert_eq!(
        world.profiles.get("mosstank/loot/Safe.utl"),
        Some(MALFORMED)
    );
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("could not be read"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootSaveCommandDoesNotOverwriteMalformedDestination
#[test]
fn loot_save_does_not_overwrite_an_unreadable_destination() {
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    world.vt(p, "loot new Source");
    world.act(p, "AddLootRule");
    world.profiles.insert("mosstank/loot/Target.utl", MALFORMED);
    world.vt(p, "loot save Target");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Source");
    assert_eq!(
        world.profiles.get("mosstank/loot/Target.utl"),
        Some(MALFORMED)
    );
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Cannot overwrite unreadable"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MissingNamedStartupLootBindingIsNotCreatedFromInheritedRules
#[test]
fn a_missing_named_startup_profile_is_not_made_from_inherited_rules() {
    let (mut world, mut first) = looter();
    world.vt(&mut first, "loot new MissingLater");
    world.act(&mut first, "AddLootRule");
    world.disable(&mut first);
    world.profiles.remove("mosstank/loot/MissingLater.utl");
    let before = cdf(&world);
    let mut restarted = world.enable();
    let p = &mut restarted;
    world.act(p, "ToggleLootPriorityBoost");
    assert!(!world.profiles.contains("mosstank/loot/MissingLater.utl"));
    assert_eq!(world.ui_text(p, "LootProfileName"), NO_ACTIVE_PROFILE);
    assert!(rows(&mut world, p).is_empty());
    assert_eq!(cdf(&world), before);
    assert!(
        world
            .errors()
            .iter()
            .any(|line| line.contains("MissingLater") && line.contains("not found"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SettingsSwitchRetainsActiveLootRulesWhenActiveLootFileBecameMalformed
#[test]
fn a_settings_switch_keeps_the_active_rules_when_the_loot_file_went_bad() {
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    world.vt(p, "loot new Safe");
    world.act(p, "AddLootRule");
    world.vt(p, "settings save OneRule");
    world.vt(p, "settings save TwoRules");
    world.act(p, "AddLootRule");
    assert_eq!(rows(&mut world, p).len(), 2);
    world.profiles.insert("mosstank/loot/Safe.utl", MALFORMED);
    world.vt(p, "settings load OneRule");
    world.act(p, "ToggleLootPriorityBoost");
    assert_eq!(rows(&mut world, p).len(), 2);
    assert_eq!(
        world.profiles.get("mosstank/loot/Safe.utl"),
        Some(MALFORMED)
    );
    assert!(
        world
            .ui_text(p, "LootEditorNotice")
            .contains("could not be read")
    );
}

// The Rust conversion reads a large picked profile a step a tick, the frames
// waiting for it (register PROF-1); at the end it is what a pick of a small
// one gives at once.
#[test]
fn a_large_loot_profile_picked_loads_a_step_a_tick() {
    use mosstank_formats::{fixtures, utl};
    let (mut world, mut plugin) = looter();
    let p = &mut plugin;
    let mut profile = utl::try_read(Some(&fixtures::read_vtank("loot-c.utl"))).unwrap();
    let rules = profile.rules.clone();
    while profile.rules.len() < 3000 {
        profile.rules.extend(rules.iter().cloned());
    }
    let text = utl::write(&profile);
    assert!(text.len() > 4 * 64 * 1024, "{}", text.len());
    world.profiles.insert("mosstank/loot/Big.utl", &text);
    let loaded = |world: &World| {
        world
            .infos()
            .iter()
            .any(|line| line.contains("Loaded loot profile Big."))
    };
    world.act_text(p, "SelectLootProfile", "Big");
    assert!(!loaded(&world));
    let mut ticks = 0;
    while !loaded(&world) {
        assert!(ticks < 100, "the profile never finished loading");
        world.tick(p);
        ticks += 1;
    }
    assert!(ticks > 1, "{ticks}");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Big");
    assert_eq!(rows(&mut world, p).len(), profile.rules.len());
}
