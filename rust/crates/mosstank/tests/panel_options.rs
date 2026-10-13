//! The panel's option pages and the advanced options window, driven
//! through the window bindings (ported from the C#'s `MossTankPanelTests`).

mod common;

use common::World;
use mosstank::MossTank;
use mosstank_expr::ExpressionValue;
use mosstank_settings::options::{self, CATEGORY_BITS, VtankSettingValueType};

fn fresh() -> (World, MossTank) {
    let mut world = World::with_character("Option Tester");
    let plugin = world.enable();
    (world, plugin)
}

fn index_of(world: &mut World, plugin: &mut MossTank, name: &str) -> i32 {
    let index = world.ui_index(plugin, "AdvancedOptionNames", name);
    assert!(index >= 0, "{name}");
    index
}

fn value_at(world: &mut World, plugin: &mut MossTank, index: i32) -> String {
    world.ui_list(plugin, "AdvancedOptionValueColumn")[usize::try_from(index).unwrap()].clone()
}

fn pascal(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ToggleAdvancedOptionsAndLootEditorVisibilityFlipBothWays
#[test]
fn the_advanced_options_and_loot_editor_toggles_flip_both_ways() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert!(!world.ui_bool(p, "AdvancedOptionsVisible"));
    world.act(p, "ToggleAdvancedOptionsVisible");
    assert!(world.ui_bool(p, "AdvancedOptionsVisible"));
    world.act(p, "HideAdvancedOptions");
    assert!(!world.ui_bool(p, "AdvancedOptionsVisible"));
    world.act(p, "HideAdvancedOptions");
    assert!(!world.ui_bool(p, "AdvancedOptionsVisible"));
    world.act(p, "ToggleAdvancedOptionsVisible");
    assert!(world.ui_bool(p, "AdvancedOptionsVisible"));
    world.act(p, "ToggleAdvancedOptionsVisible");
    assert!(!world.ui_bool(p, "AdvancedOptionsVisible"));

    assert!(!world.ui_bool(p, "LootEditorVisible"));
    world.act(p, "ToggleLootEditorVisible");
    assert!(world.ui_bool(p, "LootEditorVisible"));
    world.act(p, "ToggleLootEditorVisible");
    assert!(!world.ui_bool(p, "LootEditorVisible"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AdvancedOptionCategoryNamesShowRealNamesNotHexBitmasks
#[test]
fn the_category_names_are_real_names_not_bitmasks() {
    let (mut world, mut plugin) = fresh();
    let names = world.ui_list(&mut plugin, "AdvancedOptionCategoryNames");
    assert_eq!(
        names,
        [
            "Misc",
            "Recharge",
            "MeleeCombat",
            "SpellCombat",
            "Ranges",
            "Navigation",
            "Buffing",
            "Crafting",
            "Looting",
        ]
    );
    assert!(!names.iter().any(|name| name.starts_with("0x")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AdvancedOptionCategoryFilterHidesNonMatchingSettings
#[test]
fn the_category_filter_hides_settings_of_other_categories() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let names = world.ui_list(p, "AdvancedOptionNames");
    assert!(names.iter().any(|n| n == "EnableLooting"));
    assert!(names.iter().any(|n| n == "EnableNav"));

    let count = world.ui_bools(p, "AdvancedOptionCategoryEnabled").len();
    for index in 0..count {
        if CATEGORY_BITS[index] != 0x100 {
            world.act_int(
                p,
                "ToggleAdvancedOptionCategoryAt",
                i32::try_from(index).unwrap(),
            );
        }
    }
    let names = world.ui_list(p, "AdvancedOptionNames");
    assert!(names.iter().any(|n| n == "EnableLooting"));
    assert!(!names.iter().any(|n| n == "EnableNav"));

    let enabled = world.ui_bools(p, "AdvancedOptionCategoryEnabled");
    for (index, on) in enabled.iter().enumerate() {
        if !on {
            world.act_int(
                p,
                "ToggleAdvancedOptionCategoryAt",
                i32::try_from(index).unwrap(),
            );
        }
    }
    assert!(
        world
            .ui_list(p, "AdvancedOptionNames")
            .iter()
            .any(|n| n == "EnableNav")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AdvancedOptionListHidesTStringSettingsButKeepsTheirEnumCounterparts
#[test]
fn the_list_hides_text_settings_but_keeps_their_choice_counterparts() {
    let (mut world, mut plugin) = fresh();
    let names = world.ui_list(&mut plugin, "AdvancedOptionNames");
    for hidden in [
        "BuffProfile-Prots",
        "BuffProfile-Banes",
        "BlacklistedSpellComps",
    ] {
        assert!(!names.iter().any(|n| n == hidden), "{hidden}");
    }
    for shown in ["BuffProfile_Prots", "BuffProfile_Banes"] {
        assert!(names.iter().any(|n| n == shown), "{shown}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ASettingNothingReadsHasNoRowButIsAnOption
#[test]
fn a_setting_nothing_reads_has_no_row_but_is_an_option() {
    let (mut world, mut plugin) = fresh();
    let names = world.ui_list(&mut plugin, "AdvancedOptionNames");
    assert!(!names.iter().any(|n| n == "WhoYouGonnaCall"));
    assert!(options::NAMES.contains(&"WhoYouGonnaCall"));
    let set = world
        .with_tank(&mut plugin, |tank, client| {
            tank.set_meta_option(client, "WhoYouGonnaCall", &ExpressionValue::Boolean(false))
        })
        .unwrap();
    assert!(set);
    assert!(
        !world
            .with_tank(&mut plugin, |tank, _| tank.option("WhoYouGonnaCall"))
            .is_truthy()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AdvancedOptionListOffersOnlyThePlainValueKinds
#[test]
fn the_list_offers_only_the_plain_value_kinds() {
    let (mut world, mut plugin) = fresh();
    let names = world.ui_list(&mut plugin, "AdvancedOptionNames");
    assert!(!names.iter().any(|n| n == "RechargeHandlerSet"));
    for name in names {
        assert!(
            matches!(
                options::declared_type(&name),
                VtankSettingValueType::Bool
                    | VtankSettingValueType::Enum
                    | VtankSettingValueType::Int
                    | VtankSettingValueType::Double
            ),
            "{name}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AdvancedOptionDescriptionSurfacesRealAuthenticHelpText
#[test]
fn the_description_shows_the_authentic_help_text() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let index = index_of(&mut world, p, "DoHelp");
    world.act_int(p, "SelectAdvancedOption", index);
    let description = world.ui_text(p, "AdvancedOptionDescription");
    assert!(description.starts_with("DoHelp:"), "{description}");
    assert!(description.to_lowercase().contains("fellowship"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AdvancedOptionValueColumnMirrorsTheLiveSettingValue
#[test]
fn the_value_column_mirrors_the_live_setting() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let index = index_of(&mut world, p, "EnableCombat");
    let before = value_at(&mut world, p, index);
    assert_eq!(before, pascal(world.ui_bool(p, "CombatEnabled")));
    world.act(p, "ToggleCombatEnabled");
    world.act_int(p, "SelectAdvancedOption", index);
    let after = value_at(&mut world, p, index);
    assert_ne!(before, after);
    assert_eq!(after, pascal(world.ui_bool(p, "CombatEnabled")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ShowAdvancedOptionsRefreshesValuesChangedWhileThePopupWasClosed
#[test]
fn showing_the_window_refreshes_values_changed_while_it_was_closed() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let index = index_of(&mut world, p, "EnableCombat");
    let before = value_at(&mut world, p, index);
    world.act(p, "ToggleCombatEnabled");
    world.act(p, "ShowAdvancedOptions");
    let after = value_at(&mut world, p, index);
    assert_ne!(before, after);
    assert_eq!(after, pascal(world.ui_bool(p, "CombatEnabled")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ClickAdvancedOptionValue_OnBoolRow_FlipsInPlace
#[test]
fn clicking_a_switch_rows_value_flips_it_in_place() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let index = index_of(&mut world, p, "EnableCombat");
    let before = world.ui_bool(p, "CombatEnabled");
    world.act_int(p, "ClickAdvancedOptionValue", index);
    assert_eq!(world.ui_bool(p, "CombatEnabled"), !before);
    assert_eq!(value_at(&mut world, p, index), pascal(!before));
    world.act_int(p, "ClickAdvancedOptionValue", index);
    assert_eq!(world.ui_bool(p, "CombatEnabled"), before);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ClickAdvancedOptionValue_OnEnumRow_CyclesToTheNextLabelAndWraps
#[test]
fn clicking_a_choice_rows_value_cycles_and_wraps() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let index = index_of(&mut world, p, "UseArcs");
    world.act_int(p, "SelectAdvancedOption", index);
    world.act_text(p, "SubmitAdvancedOption", "1");
    assert_eq!(value_at(&mut world, p, index), "No");
    for expected in ["At Range", "Yes", "No"] {
        world.act_int(p, "ClickAdvancedOptionValue", index);
        assert_eq!(value_at(&mut world, p, index), expected);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ClickAdvancedOptionValue_OnNumericRow_SelectsAndLoadsTheEditField
#[test]
fn clicking_a_number_rows_value_selects_it_and_loads_the_field() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let index = index_of(&mut world, p, "SpellDiffExcessThreshold-Hunt");
    world.act_int(p, "ClickAdvancedOptionValue", index);
    assert_eq!(world.ui_int(p, "SelectedAdvancedOptionIndex"), index);
    assert_eq!(
        world.ui_text(p, "AdvancedOptionValueDraft"),
        value_at(&mut world, p, index)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CombatMacroRunningReflectsTheSameStateAsCombatButtonText
#[test]
fn the_running_switch_and_the_button_caption_agree() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert!(!world.ui_bool(p, "CombatMacroRunning"));
    assert_eq!(world.ui_text(p, "CombatButtonText"), "Run Macro");
    world.act(p, "ToggleCombat");
    assert!(world.ui_bool(p, "CombatMacroRunning"));
    assert_eq!(world.ui_text(p, "CombatButtonText"), "Stop Macro");
}
