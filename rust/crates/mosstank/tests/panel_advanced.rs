//! The advanced options window: its search, category filters and typed
//! editors, driven through the window's own bindings.

mod common;

use common::World;
use mosstank::MossTank;

fn fresh() -> (World, MossTank) {
    let mut world = World::with_character("Advanced Tester");
    let plugin = world.enable();
    (world, plugin)
}

fn select(world: &mut World, plugin: &mut MossTank, name: &str) {
    let row = world.ui_index(plugin, "AdvancedOptionNames", name);
    world.act_int(plugin, "SelectAdvancedOption", row);
}

fn custom_buff_editor_applies_the_related_setting(mode: &str, custom: &str, other: &str) {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act_text(p, "SetAdvancedOptionSearchText", mode);
    world.act_int(p, "SelectAdvancedOption", 0);
    assert!(!world.ui_bool(p, "AdvancedCustomBuffVisible"));
    world.act_text(p, "SelectAdvancedOptionChoiceText", "Custom");
    assert!(world.ui_bool(p, "AdvancedCustomBuffVisible"));
    assert_eq!(world.ui_text(p, "AdvancedCustomBuffName"), custom);
    assert_eq!(world.ui_text(p, "AdvancedCustomBuffDraft"), "ALFCBPS");

    world.act_text(p, "SubmitAdvancedCustomBuff", "bpa");
    assert_eq!(world.option_text(p, custom), "ABP");
    assert_eq!(world.ui_text(p, "AdvancedCustomBuffDraft"), "ABP");
    assert_eq!(world.option_text(p, other), "ALFCBPS");
    assert_eq!(world.ui_text(p, "AdvancedOptionChoiceText"), "Custom");

    world.act_text(p, "SubmitAdvancedCustomBuff", "123");
    assert_eq!(world.option_text(p, custom), "ABP");
    assert!(
        world
            .ui_text(p, "AdvancedOptionNotice")
            .contains("Use only")
    );
    world.act_text(p, "SubmitAdvancedCustomBuff", "");
    assert_eq!(world.option_text(p, custom), "");

    world.act_text(p, "SelectAdvancedOptionChoiceText", "All");
    assert!(!world.ui_bool(p, "AdvancedCustomBuffVisible"));
    world.act_text(p, "SubmitAdvancedCustomBuff", "L");
    assert_eq!(world.option_text(p, custom), "");
    world.act_text(p, "SelectAdvancedOptionChoiceText", "Custom");
    world.act_text(p, "SubmitAdvancedCustomBuff", "C");
    world.act_text(p, "SetAdvancedOptionSearchText", "no-such-option");
    assert!(!world.ui_bool(p, "AdvancedCustomBuffVisible"));
    world.act_text(p, "SubmitAdvancedCustomBuff", "F");
    assert_eq!(world.option_text(p, custom), "C");
    world.act(p, "ClearAdvancedOptionSearch");
    assert_eq!(world.ui_text(p, "AdvancedOptionName"), mode);
    assert_eq!(world.ui_text(p, "AdvancedCustomBuffDraft"), "C");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.AdvancedSearch.cs :: AdvancedCustomBuffEditorAppliesTheRelatedOriginalStringSetting
#[test]
fn the_custom_buff_editor_applies_the_related_setting() {
    custom_buff_editor_applies_the_related_setting(
        "BuffProfile_Banes",
        "BuffProfile-Banes",
        "BuffProfile-Prots",
    );
    custom_buff_editor_applies_the_related_setting(
        "BuffProfile_Prots",
        "BuffProfile-Prots",
        "BuffProfile-Banes",
    );
}

fn buff_profile_choices_keep_their_labels(name: &str, label: &str, expected: i32) {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    select(&mut world, p, "EnableCombat");
    assert!(world.ui_list(p, "AdvancedOptionChoices").is_empty());

    world.act_text(p, "SetAdvancedOptionSearchText", name);
    select(&mut world, p, name);
    assert_eq!(world.ui_text(p, "AdvancedOptionName"), name);
    assert!(world.ui_bool(p, "AdvancedOptionChoiceVisible"));
    let choices = world.ui_list(p, "AdvancedOptionChoices");
    assert_eq!(choices.len(), 8);
    for choice in ["Custom", "All", "None"] {
        assert!(choices.iter().any(|c| c == choice), "{choice}");
    }

    world.act_text(p, "SelectAdvancedOptionChoiceText", label);
    assert_eq!(world.ui_text(p, "AdvancedOptionChoiceText"), label);
    assert_eq!(
        world.ui_text(p, "AdvancedOptionValueDraft"),
        expected.to_string()
    );
    assert_eq!(
        world.ui_int(p, "SelectedAdvancedOptionChoice"),
        expected - 1
    );

    world.act_text(p, "SelectAdvancedOptionChoiceText", "None");
    assert_eq!(world.ui_text(p, "AdvancedOptionChoiceText"), "None");
    assert_eq!(world.ui_text(p, "AdvancedOptionValueDraft"), "3");
    world.act(p, "ClearAdvancedOptionSearch");
    assert_eq!(world.ui_text(p, "AdvancedOptionName"), name);
    assert_eq!(world.ui_text(p, "AdvancedOptionChoiceText"), "None");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.AdvancedSearch.cs :: AdvancedBuffProfileChoicesKeepTheirOriginalLabelsAndValues
#[test]
fn buff_profile_choices_keep_their_original_labels_and_values() {
    buff_profile_choices_keep_their_labels("BuffProfile_Banes", "BPSAC", 7);
    buff_profile_choices_keep_their_labels("BuffProfile_Prots", "BPSAC", 8);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.AdvancedSearch.cs :: AdvancedSearchMatchesAllWordsAcrossNamesAndDescriptions
#[test]
fn the_search_matches_all_words_across_names_and_descriptions() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    let original = world.ui_list(p, "AdvancedOptionNames");
    world.act_text(p, "SetAdvancedOptionSearchText", "  DOHELP   fellowship  ");
    assert_eq!(world.ui_list(p, "AdvancedOptionNames"), ["DoHelp"]);
    world.act_text(
        p,
        "SetAdvancedOptionSearchText",
        "DoHelp fellowship nonexistent-term",
    );
    assert!(world.ui_list(p, "AdvancedOptionNames").is_empty());
    world.act(p, "ClearAdvancedOptionSearch");
    assert_eq!(world.ui_list(p, "AdvancedOptionNames"), original);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.AdvancedSearch.cs :: AdvancedSearchPreservesSelectionByNameAndNeverChangesValues
#[test]
fn the_search_keeps_the_selection_by_name_and_never_changes_values() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    select(&mut world, p, "EnableCombat");
    let before = world.ui_bool(p, "CombatEnabled");
    world.act_text(p, "SetAdvancedOptionSearchText", "combat");
    assert_eq!(world.ui_text(p, "AdvancedOptionName"), "EnableCombat");
    world.act_text(p, "SetAdvancedOptionSearchText", "DoHelp");
    assert!(!world.ui_bool(p, "AdvancedOptionEditorVisible"));
    assert_eq!(world.ui_int(p, "SelectedAdvancedOptionIndex"), -1);
    world.act_text(
        p,
        "SubmitAdvancedOption",
        if before { "false" } else { "true" },
    );
    assert_eq!(world.ui_bool(p, "CombatEnabled"), before);
    world.act(p, "ClearAdvancedOptionSearch");
    assert_eq!(world.ui_text(p, "AdvancedOptionName"), "EnableCombat");
    assert_eq!(world.ui_bool(p, "CombatEnabled"), before);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.AdvancedSearch.cs :: AdvancedCategoryAndTextFiltersIntersectAndRestoreSelection
#[test]
fn category_and_text_filters_intersect_and_restore_the_selection() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    select(&mut world, p, "EnableNav");
    world.act_text(p, "SetAdvancedOptionSearchText", "EnableNav");
    let categories = world.ui_bools(p, "AdvancedOptionCategoryEnabled").len();
    for row in 0..categories {
        world.act_int(
            p,
            "ToggleAdvancedOptionCategoryAt",
            i32::try_from(row).unwrap(),
        );
    }
    assert!(world.ui_list(p, "AdvancedOptionNames").is_empty());
    assert!(!world.ui_bool(p, "AdvancedOptionEditorVisible"));
    for row in 0..categories {
        world.act_int(
            p,
            "ToggleAdvancedOptionCategoryAt",
            i32::try_from(row).unwrap(),
        );
    }
    assert_eq!(world.ui_list(p, "AdvancedOptionNames"), ["EnableNav"]);
    assert_eq!(world.ui_text(p, "AdvancedOptionName"), "EnableNav");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.AdvancedSearch.cs :: AdvancedTypedEditorsApplyOnlyTheSelectedOriginalSetting
#[test]
fn the_typed_editors_apply_only_the_selected_setting() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    select(&mut world, p, "EnableCombat");
    assert!(world.ui_bool(p, "AdvancedOptionBooleanVisible"));
    assert!(!world.ui_bool(p, "AdvancedOptionChoiceVisible"));
    let before = world.ui_bool(p, "CombatEnabled");
    world.act(p, "ToggleAdvancedOptionBoolean");
    assert_eq!(world.ui_bool(p, "CombatEnabled"), !before);

    select(&mut world, p, "UseArcs");
    assert!(world.ui_bool(p, "AdvancedOptionChoiceVisible"));
    assert_eq!(
        world.ui_list(p, "AdvancedOptionChoices"),
        ["No", "At Range", "Yes"]
    );
    world.act_text(p, "SelectAdvancedOptionChoiceText", "Yes");
    assert_eq!(world.ui_text(p, "AdvancedOptionValueDraft"), "3");
    assert_eq!(world.ui_int(p, "SelectedAdvancedOptionChoice"), 2);
    assert_eq!(world.ui_text(p, "AdvancedOptionChoiceText"), "Yes");
    world.act_text(p, "SelectAdvancedOptionChoiceText", "not a choice");
    world.act_int(p, "SelectAdvancedOptionChoice", -1);
    assert_eq!(world.ui_int(p, "SelectedAdvancedOptionChoice"), 2);
    world.act(p, "ToggleAdvancedOptionBoolean");
    assert_eq!(world.ui_int(p, "SelectedAdvancedOptionChoice"), 2);

    select(&mut world, p, "SpellDiffExcessThreshold-Hunt");
    assert!(world.ui_bool(p, "AdvancedOptionNumberVisible"));
    world.act_text(p, "SetAdvancedOptionValueDraft", "37");
    world.act(p, "ApplyAdvancedOption");
    assert_eq!(world.ui_text(p, "AdvancedOptionValueDraft"), "37");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.OptionParity.cs :: RareCorpseWalkAppearsInLootingAdvancedOptionsAndEditsTheSavedSwitch
#[test]
fn the_rare_corpse_walk_appears_in_the_looting_options_and_edits_the_saved_switch() {
    use mosstank_settings::options::{CATEGORY_BITS, NAMES, WALK_TO_RARE_CORPSE};
    let mut world = World::with_character("Prover");
    let mut plugin = world.enable();
    let p = &mut plugin;

    assert!(!NAMES.contains(&WALK_TO_RARE_CORPSE));
    world.act(p, "ShowAdvancedOptions");
    world.act_text(p, "SetAdvancedOptionSearchText", WALK_TO_RARE_CORPSE);
    assert_eq!(
        world.ui_list(p, "AdvancedOptionNames"),
        [WALK_TO_RARE_CORPSE]
    );
    assert_eq!(world.ui_list(p, "AdvancedOptionValueColumn")[0], "False");
    world.act_int(p, "SelectAdvancedOption", 0);
    assert!(world.ui_bool(p, "AdvancedOptionBooleanVisible"));
    assert!(
        world
            .ui_text(p, "AdvancedOptionDescription")
            .contains("own rare")
    );

    world.act_int(p, "ClickAdvancedOptionValue", 0);
    assert!(world.ui_bool(p, "WalkToOwnRareCorpsesEnabled"));
    assert_eq!(world.ui_list(p, "AdvancedOptionValueColumn")[0], "True");

    world.disable(p);
    let mut restarted = world.enable();
    let r = &mut restarted;
    assert!(world.ui_bool(r, "WalkToOwnRareCorpsesEnabled"));

    world.act_text(r, "SetAdvancedOptionSearchText", "");
    let categories = world.ui_bools(r, "AdvancedOptionCategoryEnabled").len();
    for index in 0..categories {
        if CATEGORY_BITS[index] != 0x100 {
            world.act_int(
                r,
                "ToggleAdvancedOptionCategoryAt",
                i32::try_from(index).unwrap(),
            );
        }
    }
    assert!(
        world
            .ui_list(r, "AdvancedOptionNames")
            .iter()
            .any(|name| name == WALK_TO_RARE_CORPSE)
    );
}
