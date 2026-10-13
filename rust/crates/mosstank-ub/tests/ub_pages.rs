//! The UB tab's pages, against `MossTankPanelTests.UbSettingsTab`,
//! `MossTankPanelTests.UbMainTab` and `InterfaceFontMetricsTests`. The tab
//! row (which tab is selected) and the markup's own shape belong to the
//! panel and are ported with it; these drive the pages' view of the
//! catalogue over the real UB store.

mod common;

use ac_plugin_abi::callback::InterceptDecision;
use common::page::PageRig;
use mosstank_settings::ub::definitions;
use mosstank_ub::alias::AliasController;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::font;
use mosstank_ub::main_page::{self, SWITCH_OFF_COLOR, SWITCH_ON_COLOR, SWITCHES};
use mosstank_ub::testing::SharedStorage;

// ---- InterfaceFontMetricsTests ---------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/InterfaceFontMetricsTests.cs :: TheMeasureIsTheFontsAdvance
#[test]
#[allow(clippy::float_cmp)]
fn the_measure_is_the_fonts_advance() {
    for (text, width) in [
        ("F", 6.0),
        ("WC", 19.0),
        ("DC", 17.0),
        ("Cs", 13.0),
        ("I", 3.0),
        ("", 0.0),
        ("é", 0.0),
    ] {
        assert_eq!(width, font::measure(text), "{text}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InterfaceFontMetricsTests.cs :: WordsWrapWholeAndNoLineIsWiderThanAsked
#[test]
fn words_wrap_whole_and_no_line_is_wider_than_asked() {
    let text = "Draw a name above the creatures and objects around you.";
    let lines = font::wrap(text, 120.0);
    assert!(lines.len() > 1);
    for line in &lines {
        assert!(font::measure(line) <= 120.0, "{line}");
    }
    assert_eq!(text, lines.join(" "));
    let next = lines[1].split(' ').next().unwrap();
    assert!(font::measure(&format!("{} {next}", lines[0])) > 120.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InterfaceFontMetricsTests.cs :: AWordWiderThanTheWholeWidthIsBrokenWhereTheWidthRunsOut
#[test]
fn a_word_wider_than_the_whole_width_is_broken_where_the_width_runs_out() {
    let lines = font::wrap("ab WWWWWWWWWW cd", 40.0);
    for line in &lines {
        assert!(font::measure(line) <= 40.0, "{line}");
    }
    assert_eq!("abWWWWWWWWWWcd", lines.concat().replace(' ', ""));
    assert!(font::wrap("   ", 40.0).is_empty());
}

// ---- the settings page ----------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheUbSettingsPageOpensWithEverySettingAndEveryToolListed
#[test]
fn the_ub_settings_page_opens_with_every_setting_and_every_tool_listed() {
    let rig = PageRig::new(&SharedStorage::new());
    assert_eq!(168, rig.names().len());
    assert_eq!(rig.names().len(), rig.values().len());
    assert_eq!("(all)", rig.page.category_names()[0]);
    assert_eq!(16, rig.page.category_names().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheCategoryListNarrowsTheSettingList
#[test]
fn the_category_list_narrows_the_setting_list() {
    let mut rig = PageRig::new(&SharedStorage::new());
    let jumper = rig
        .page
        .category_names()
        .iter()
        .position(|name| name == "Jumper")
        .unwrap();
    rig.page
        .select_category(&rig.catalog, i64::try_from(jumper).unwrap());
    assert_eq!(4, rig.names().len());
    assert!(rig.names().iter().all(|name| name.starts_with("Jumper.")));
    rig.page.select_category(&rig.catalog, 0);
    assert_eq!(168, rig.names().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheFilterFieldReachesBothTheNameAndTheDescription
#[test]
fn the_filter_field_reaches_both_the_name_and_the_description() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("markers.doors");
    assert_eq!(5, rig.names().len());

    rig.filter("staircase");
    let mut names = rig.names();
    names.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        vec![
            "DungeonMaps.Display.Stairs.Enabled",
            "DungeonMaps.Display.Stairs.Color"
        ],
        names
    );

    rig.filter("");
    assert_eq!(168, rig.names().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ASwitchFlipsWhereItStandsAndASavedValueSurvivesTheSession
#[test]
fn a_switch_flips_where_it_stands_and_a_saved_value_survives_the_session() {
    let storage = SharedStorage::new();
    let mut rig = PageRig::new(&storage);
    rig.filter("Jumper.PauseNav");
    assert_eq!("True", rig.value(0));
    rig.click(0);
    assert_eq!("False", rig.value(0));
    assert!(rig.notice().contains("Jumper.PauseNav"));

    let mut next = PageRig::new(&storage);
    next.filter("Jumper.PauseNav");
    assert_eq!("False", next.value(0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ANumberIsTypedIntoTheSharedFieldAndCommittedOnSubmit
#[test]
fn a_number_is_typed_into_the_shared_field_and_committed_on_submit() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("AutoVendor.Tries");
    rig.select(0);
    assert_eq!("4", rig.page.value_draft());
    rig.submit("9");
    assert_eq!("9", rig.value(0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ARefusedValueLeavesTheSettingAloneAndSaysWhy
#[test]
fn a_refused_value_leaves_the_setting_alone_and_says_why() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("nine");
    assert_eq!("4", rig.value(0));
    assert!(rig.notice().contains("unchanged"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: PuttingARowBackToItsDefaultTakesTheSavedValueOutOfItsFile
#[test]
fn putting_a_row_back_to_its_default_takes_the_saved_value_out_of_its_file() {
    let storage = SharedStorage::new();
    let mut rig = PageRig::new(&storage);
    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("9");
    let profile = "mosstank/ub/profiles/default.settings.json";
    assert!(rig.file(profile).contains("\"AutoVendor.Tries\""));

    rig.reset();

    assert_eq!("4", rig.value(0));
    assert!(!rig.file(profile).contains("\"AutoVendor.Tries\""));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheDescriptionAndTheFileAnEditLandsInAreBothOnThePage
#[test]
fn the_description_and_the_file_an_edit_lands_in_are_both_on_the_page() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Enabled");
    rig.select(0);
    assert!(
        rig.page
            .description_line(0)
            .starts_with("Nametags.Enabled:")
    );
    assert_eq!("Saved for this installation.", rig.scope_text());

    rig.filter("Networking.Tags");
    rig.select(0);
    assert_eq!("Saved for this character only.", rig.scope_text());

    rig.filter("AutoVendor.Tries");
    rig.select(0);
    assert_eq!("Saved in profile 'default'.", rig.scope_text());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AnEditLandsInTheOneFileTheScopeSentenceNames
#[test]
fn an_edit_lands_in_the_one_file_the_scope_sentence_names() {
    for (name, typed, file) in [
        ("Nametags.Enabled", "False", "mosstank/ub/settings.json"),
        (
            "AutoVendor.Tries",
            "9",
            "mosstank/ub/profiles/default.settings.json",
        ),
        (
            "NetworkUI.SelectedTag",
            "raid",
            "mosstank/ub/Coldeve/Acdream/settings.json",
        ),
    ] {
        let storage = SharedStorage::new();
        let mut rig = PageRig::new(&storage);
        rig.filter(name);
        rig.select(0);
        rig.submit(typed);

        let quoted = format!("\"{name}\"");
        assert!(rig.file(file).contains(&quoted), "{name}");
        for key in storage.keys() {
            if key == file || !key.starts_with("mosstank/ub/") {
                continue;
            }
            assert!(
                !storage.get(&key).unwrap().contains(&quoted),
                "{name} in {key}"
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AnEditToAFileThatDidNotReadAppliesButSaysItIsNotSaved
#[test]
fn an_edit_to_a_file_that_did_not_read_applies_but_says_it_is_not_saved() {
    let damaged = "{ this is not json";
    let storage = SharedStorage::new();
    storage.insert("mosstank/ub/profiles/default.settings.json", damaged);
    let mut rig = PageRig::new(&storage);
    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("9");

    assert_eq!("9", rig.value(0));
    assert!(rig.notice().contains("this session only"));
    assert!(rig.notice().contains("repaired"));
    assert_eq!(
        damaged,
        rig.file("mosstank/ub/profiles/default.settings.json")
    );

    rig.filter("Jumper.PauseNav");
    rig.click(0);
    assert!(rig.notice().contains("this session only"));

    rig.filter("NetworkUI.TrackedItems");
    rig.click(0);
    rig.add_list_entry("Prismatic Taper");
    assert!(rig.notice().contains("this session only"));
    rig.page.hide_list_editor();

    rig.filter("Nametags.Monster.TagColor");
    rig.click(0);
    rig.page.set_color_channel(8, 128.0);
    rig.apply_color();
    assert!(rig.notice().contains("this session only"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AnEditToAFileThatReadSaysNothingAboutSessions
#[test]
fn an_edit_to_a_file_that_read_says_nothing_about_sessions() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("9");
    assert_eq!("AutoVendor.Tries is now 9.", rig.notice());
}

// ---- the colour sub-page ------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ClickingAColourOpensTheSubPageLoadedWithThatColour
#[test]
#[allow(clippy::float_cmp)]
fn clicking_a_colour_opens_the_sub_page_loaded_with_that_colour() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Monster.TagColor");
    assert!(!rig.page.color_visible());
    rig.click(0);
    assert!(rig.page.color_visible());
    assert_eq!("#FFFF0000", rig.page.color_hex());
    assert_eq!(255.0, rig.page.color_channel(24));
    assert_eq!(255.0, rig.page.color_channel(16));
    assert_eq!(0.0, rig.page.color_channel(8));
    assert!(
        rig.page
            .color_target()
            .contains("Nametags.Monster.TagColor")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheSlidersAndTheHexFieldEachDriveTheOther
#[test]
#[allow(clippy::float_cmp)]
fn the_sliders_and_the_hex_field_each_drive_the_other() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Monster.TagColor");
    rig.click(0);

    rig.page.set_color_channel(8, 128.0);
    assert_eq!("#FFFF8000", rig.page.color_hex());
    assert_eq!("128", rig.page.color_channel_text(8));

    rig.page.set_color_hex("#8000FF00");
    assert_eq!(128.0, rig.page.color_channel(24));
    assert_eq!(0.0, rig.page.color_channel(16));
    assert_eq!(255.0, rig.page.color_channel(8));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheSwatchShowsTheDraftColourAsItIsEdited
#[test]
fn the_swatch_shows_the_draft_colour_as_it_is_edited() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Monster.TagColor");
    rig.click(0);
    assert_eq!(0xFFFF_0000, rig.page.color_swatch());

    rig.page.set_color_channel(8, 128.0);
    assert_eq!(0xFFFF_8000, rig.page.color_swatch());
    assert_eq!("#FFFF0000", rig.value(0));

    rig.page.set_color_hex("#8000FF00");
    assert_eq!(0x8000_FF00, rig.page.color_swatch());

    rig.page.reset_color_to_default(&rig.catalog);
    assert_eq!(0xFFFF_0000, rig.page.color_swatch());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AColourIsOnlySavedWhenTheSubPageIsSaved
#[test]
fn a_colour_is_only_saved_when_the_sub_page_is_saved() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Monster.TagColor");
    rig.click(0);
    rig.page.set_color_channel(8, 128.0);

    rig.page.hide_color_editor();
    assert!(!rig.page.color_visible());
    assert_eq!("#FFFF0000", rig.value(0));

    rig.click(0);
    rig.page.set_color_channel(8, 128.0);
    rig.apply_color();
    assert!(!rig.page.color_visible());
    assert_eq!("#FFFF8000", rig.value(0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheColourButtonSaysSoWhenTheRowIsNotAColour
#[test]
fn the_colour_button_says_so_when_the_row_is_not_a_colour() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Jumper.Attempts");
    rig.select(0);
    rig.page.show_color_editor(&rig.catalog);
    assert!(!rig.page.color_visible());
    assert!(rig.notice().contains("not a colour"));
}

// ---- the list sub-page --------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: OneListSubPageServesEveryListSetting
#[test]
fn one_list_sub_page_serves_every_list_setting() {
    let mut rig = PageRig::new(&SharedStorage::new());
    for name in [
        "AutoXp.Policy",
        "NetworkUI.TrackedItems",
        "Networking.Tags",
        "Aliases.DefinedAliases",
        "GameEvents.Handlers",
    ] {
        rig.filter(name);
        rig.click(0);
        assert!(
            rig.page.list_visible(),
            "{name} did not open the list page."
        );
        assert!(rig.page.list_title().starts_with(name));
        rig.page.hide_list_editor();
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ListEntriesAreAddedReplacedAndRemovedAndSavedAsTheyGo
#[test]
fn list_entries_are_added_replaced_and_removed_and_saved_as_they_go() {
    let storage = SharedStorage::new();
    let mut rig = PageRig::new(&storage);
    rig.filter("Networking.Tags");
    rig.click(0);

    rig.add_list_entry("tank");
    rig.page.set_list_draft("healer");
    rig.add_list_entry_from_draft();
    assert_eq!(vec!["tank", "healer"], rig.page.list_rows());
    assert_eq!("tank, healer", rig.value(0));

    rig.page.select_list_row(0);
    rig.page.set_list_draft("main tank");
    rig.replace_list_entry();
    assert_eq!(vec!["main tank", "healer"], rig.page.list_rows());

    rig.page.select_list_row(1);
    rig.remove_list_entry();
    assert_eq!(vec!["main tank"], rig.page.list_rows());

    let mut next = PageRig::new(&storage);
    next.filter("Networking.Tags");
    assert_eq!("main tank", next.value(0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AnEmptyListReadsAsEmptyRatherThanAsNothing
#[test]
fn an_empty_list_reads_as_empty_rather_than_as_nothing() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Networking.Tags");
    assert_eq!("(empty)", rig.value(0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheListButtonSaysSoWhenTheRowIsNotAList
#[test]
fn the_list_button_says_so_when_the_row_is_not_a_list() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Jumper.Attempts");
    rig.select(0);
    rig.page.show_list_editor(&rig.catalog);
    assert!(!rig.page.list_visible());
    assert!(rig.notice().contains("not a list"));
}

// ---- profiles -------------------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ANewProfileIsMadeOnThePageAndIsWhereTheNextEditLands
#[test]
fn a_new_profile_is_made_on_the_page_and_is_where_the_next_edit_lands() {
    let storage = SharedStorage::new();
    let mut rig = PageRig::new(&storage);
    assert_eq!(vec!["default"], rig.profile_names());

    rig.page.set_profile_name_draft("raid");
    rig.create_profile();

    assert_eq!("raid", rig.selected_profile());
    assert_eq!(vec!["default", "raid"], rig.profile_names());

    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("9");

    assert!(
        rig.file("mosstank/ub/profiles/raid.settings.json")
            .contains("\"AutoVendor.Tries\"")
    );
    assert!(rig.scope_text().contains("raid"));

    let mut next = PageRig::new(&storage);
    assert_eq!("raid", next.selected_profile());
    assert_eq!(vec!["default", "raid"], next.profile_names());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheProfileNameFieldMakesTheProfileOnEnter
#[test]
fn the_profile_name_field_makes_the_profile_on_enter() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.create_named_profile("raid");
    assert_eq!("raid", rig.selected_profile());
    assert_eq!(vec!["default", "raid"], rig.profile_names());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: CopyingMakesANewProfileHoldingWhatTheOpenOneHolds
#[test]
fn copying_makes_a_new_profile_holding_what_the_open_one_holds() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("9");

    rig.page.set_profile_name_draft("raid");
    rig.copy_profile();

    assert_eq!("raid", rig.selected_profile());
    assert_eq!("9", rig.value(0));
    assert!(
        rig.file("mosstank/ub/profiles/raid.settings.json")
            .contains("\"AutoVendor.Tries\"")
    );

    rig.submit("11");
    assert!(
        rig.file("mosstank/ub/profiles/raid.settings.json")
            .contains("\"11\"")
    );
    assert!(
        rig.file("mosstank/ub/profiles/default.settings.json")
            .contains("\"9\"")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ANewProfileNeedsANameAndWillNotTakeOneAlreadyInUse
#[test]
fn a_new_profile_needs_a_name_and_will_not_take_one_already_in_use() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.create_profile();
    assert_eq!("default", rig.selected_profile());
    assert!(rig.notice().to_lowercase().contains("name"));

    rig.page.set_profile_name_draft("default");
    rig.create_profile();
    assert_eq!("default", rig.selected_profile());
    assert!(rig.notice().contains("already"));
    assert_eq!(vec!["default"], rig.profile_names());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AProfileChosenFromTheMenuIsWhereTheNextEditLands
#[test]
fn a_profile_chosen_from_the_menu_is_where_the_next_edit_lands() {
    let storage = SharedStorage::new();
    let mut rig = PageRig::new(&storage);
    rig.page.set_profile_name_draft("raid");
    rig.create_profile();

    rig.select_profile("default");
    assert_eq!("default", rig.selected_profile());
    rig.select_profile("raid");
    assert_eq!("raid", rig.selected_profile());

    rig.filter("AutoVendor.Tries");
    rig.select(0);
    rig.submit("9");

    assert!(
        storage
            .keys()
            .contains(&"mosstank/ub/profiles/raid.settings.json".to_owned())
    );
    assert!(rig.scope_text().contains("raid"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ChangingTheSubPageShutsTheOtherOne
#[test]
fn changing_the_sub_page_shuts_the_other_one() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Monster.TagColor");
    rig.click(0);
    assert!(rig.page.color_visible());

    rig.filter("Networking.Tags");
    rig.click(0);
    assert!(rig.page.list_visible());
    assert!(!rig.page.color_visible());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AnOutOfRangeValueIsHeldToTheBoundTheMacroUses
#[test]
fn an_out_of_range_value_is_held_to_the_bound_the_macro_uses() {
    for (name, typed, held) in [
        ("Jumper.Attempts", "999", "20"),
        ("Jumper.Attempts", "0", "1"),
        ("ItemGiver.Range", "5000", "1000"),
        ("ItemGiver.Delay", "-4", "0"),
        ("ItemGiver.BusyRetryLimit", "0", "1"),
        ("ItemGiver.FailureLimit", "900", "100"),
    ] {
        let mut rig = PageRig::new(&SharedStorage::new());
        rig.filter(name);
        rig.select(0);
        rig.submit(typed);
        assert_eq!(held, rig.value(0), "{name} {typed}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: LeavingTheTabShutsBothSubPages
#[test]
fn leaving_the_tab_shuts_both_sub_pages() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("Nametags.Monster.TagColor");
    rig.click(0);
    rig.page.leave();
    assert!(!rig.page.color_visible());
    assert!(!rig.page.list_visible());
}

// ---- the alias list on the page -----------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheAliasListOnThePageRewritesWhatIsTyped
#[test]
fn the_alias_list_on_the_page_rewrites_what_is_typed() {
    let mut rig = PageRig::new(&SharedStorage::new());
    let mut host = mosstank_ub::testing::FakeUbHost::new();
    let mut runtime = mosstank_meta::runtime::ExpressionRuntime::new(
        &mut host,
        Some(1),
        std::rc::Rc::new(mosstank_expr::FixedClock::default()),
    );
    let mut controller = AliasController::new();
    controller.attach(&mut host).unwrap();
    assert_eq!(1, host.interceptors.len());
    let mut intercept = |rig: &PageRig, line: &str| {
        let catalog: &dyn UbCatalog = &rig.catalog;
        let enabled = catalog.flag("Aliases.Enabled");
        let lines = catalog.value("Aliases.DefinedAliases").items;
        let mut services = mosstank_meta::services::NoServices;
        let mut expressions = mosstank_ub::expressions::RuntimeExpressions {
            runtime: &mut runtime,
            services: &mut services,
        };
        controller.intercept(&mut host, &mut expressions, enabled, &lines, line)
    };

    assert!(matches!(intercept(&rig, "/ubpd"), InterceptDecision::Pass));

    rig.filter("Aliases.DefinedAliases");
    rig.click(0);
    rig.add_list_entry("^/ubpd$ = /ub propertydump");

    assert_eq!(
        InterceptDecision::Rewrite("/ub propertydump".to_owned()),
        intercept(&rig, "/ubpd")
    );

    rig.filter("Aliases.Enabled");
    rig.click(0);
    assert!(matches!(intercept(&rig, "/ubpd"), InterceptDecision::Pass));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheAliasListIsTheFileTheProfileRowNames
#[test]
fn the_alias_list_is_the_file_the_profile_row_names() {
    let storage = SharedStorage::new();
    let mut rig = PageRig::new(&storage);
    rig.filter("Aliases.DefinedAliases");
    rig.click(0);
    rig.add_list_entry("^/mine$ = /say mine");
    assert_eq!(
        "^/mine$ = /say mine\n",
        rig.file("mosstank/ub/Coldeve/Acdream/aliases.txt")
    );
    for key in storage.keys() {
        if key.ends_with(".json") {
            assert!(
                !storage
                    .get(&key)
                    .unwrap()
                    .contains("Aliases.DefinedAliases")
            );
        }
    }
    rig.page.hide_list_editor();

    storage.insert(
        "mosstank/ub/profiles/shared.aliases.txt",
        "^/ours$ = /say ours\n",
    );
    rig.filter("Aliases.Profile");
    rig.select(0);
    rig.submit("shared");

    rig.filter("Aliases.DefinedAliases");
    assert_eq!("^/ours$ = /say ours", rig.value(0));
    rig.click(0);
    assert_eq!(vec!["^/ours$ = /say ours"], rig.page.list_rows());
    rig.add_list_entry("^/more$ = /say more");
    assert_eq!(
        "^/ours$ = /say ours\n^/more$ = /say more\n",
        rig.file("mosstank/ub/profiles/shared.aliases.txt")
    );
    assert_eq!(
        "^/mine$ = /say mine\n",
        rig.file("mosstank/ub/Coldeve/Acdream/aliases.txt")
    );

    // Another character on the same computer, pointed at the same name.
    let mut other = PageRig::for_character(&storage, "Horan");
    other.filter("Aliases.Profile");
    other.select(0);
    other.submit("shared");
    other.filter("Aliases.DefinedAliases");
    assert_eq!("^/ours$ = /say ours, ^/more$ = /say more", other.value(0));
}

fn open_alias_list(storage: &SharedStorage) -> PageRig {
    let mut rig = PageRig::new(storage);
    rig.filter("Aliases.DefinedAliases");
    rig.click(0);
    assert!(rig.page.list_visible());
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AnAliasLineRoundTripsThroughThePage
#[test]
fn an_alias_line_round_trips_through_the_page() {
    let line = "[eat] ^/lsr$ -> actiontrycastbyid[1635]";
    let storage = SharedStorage::new();
    let mut rig = open_alias_list(&storage);

    rig.add_list_entry(line);
    assert_eq!(vec![line], rig.page.list_rows());
    assert_eq!("", rig.page.list_notice());
    assert_eq!(
        format!("{line}\n"),
        rig.file("mosstank/ub/Coldeve/Acdream/aliases.txt")
    );

    rig.page.hide_list_editor();
    rig.click(0);
    assert_eq!(vec![line], rig.page.list_rows());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: AMalformedAliasIsRefusedWithTheReasonOnThePage
#[test]
fn a_malformed_alias_is_refused_with_the_reason_on_the_page() {
    let storage = SharedStorage::new();
    let mut rig = open_alias_list(&storage);
    let key = "mosstank/ub/Coldeve/Acdream/aliases.txt";

    rig.add_list_entry("^(/x$ = /y");
    assert!(rig.page.list_rows().is_empty());
    assert!(rig.page.list_notice().contains("regular expression"));
    assert!(storage.get(key).is_none());

    rig.add_list_entry("just words");
    assert!(rig.page.list_rows().is_empty());
    assert!(rig.page.list_notice().contains("pattern = command"));

    rig.add_list_entry("^/ok$ = /say ok");
    assert_eq!(vec!["^/ok$ = /say ok"], rig.page.list_rows());
    assert_eq!("", rig.page.list_notice());

    rig.page.select_list_row(0);
    rig.page.set_list_draft("^(/x$ = /y");
    rig.replace_list_entry();
    assert_eq!(vec!["^/ok$ = /say ok"], rig.page.list_rows());
    assert!(rig.page.list_notice().contains("regular expression"));
    assert_eq!("^/ok$ = /say ok\n", rig.file(key));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ABadLineAlreadyInTheFileIsShownAndNamed
#[test]
fn a_bad_line_already_in_the_file_is_shown_and_named() {
    let storage = SharedStorage::new();
    storage.insert(
        "mosstank/ub/Coldeve/Acdream/aliases.txt",
        "^/ok$ = /say ok\nnot an alias\n",
    );
    let mut rig = open_alias_list(&storage);

    assert_eq!(
        vec!["^/ok$ = /say ok", "not an alias"],
        rig.page.list_rows()
    );
    assert!(rig.page.list_notice().contains("Line 2"));

    rig.page.select_list_row(1);
    rig.page.set_list_draft("^/fixed$ = /say fixed");
    rig.replace_list_entry();
    assert_eq!("", rig.page.list_notice());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: OtherListsAreNotHeldToTheAliasFormat
#[test]
fn other_lists_are_not_held_to_the_alias_format() {
    let mut rig = PageRig::new(&SharedStorage::new());
    rig.filter("NetworkUI.TrackedItems");
    rig.click(0);
    rig.add_list_entry("just words");
    assert_eq!(vec!["just words"], rig.page.list_rows());
    assert_eq!("", rig.page.list_notice());
}

/// The description labels' widths in the C# markup the page is drawn from.
fn description_label_widths() -> Vec<f64> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../src/AcDream.Plugins.MossTank/mosstank.xml"
    );
    let markup = std::fs::read_to_string(path).expect("the C# markup is beside the port");
    markup
        .lines()
        .filter(|line| line.contains("<label") && line.contains("text=\"{UbSettingDescription"))
        .map(|line| {
            let at = line.find(" w=\"").expect("a label has a width") + 4;
            let rest = &line[at..];
            rest[..rest.find('"').unwrap()].parse().unwrap()
        })
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: EverySettingDescriptionFitsTheLabelsBesideTheList
#[test]
fn every_setting_description_fits_the_labels_beside_the_list() {
    let widths = description_label_widths();
    assert_eq!(4, widths.len());
    let width = widths.iter().copied().fold(f64::MAX, f64::min);

    let mut rig = PageRig::new(&SharedStorage::new());
    let mut longest = String::new();
    let mut longest_width = 0.0;
    let mut longest_lines = 0;
    for definition in definitions::all() {
        rig.filter(&definition.name);
        let index = rig
            .names()
            .iter()
            .position(|name| *name == definition.name)
            .expect("the row is listed");
        rig.select(i64::try_from(index).unwrap());

        let lines: Vec<String> = (0..widths.len())
            .map(|at| rig.page.description_line(at))
            .collect();
        let whole = format!("{}: {}", definition.name, definition.summary);
        let shown: Vec<&str> = lines
            .iter()
            .filter(|line| !line.is_empty())
            .map(String::as_str)
            .collect();
        assert_eq!(whole, shown.join(" "));
        for line in &lines {
            assert!(
                font::measure(line) <= width,
                "{}: '{line}' is {} px in a {width} px label.",
                definition.name,
                font::measure(line)
            );
        }
        let whole_width = font::measure(&whole);
        if whole_width > longest_width {
            longest.clone_from(&definition.name);
            longest_width = whole_width;
            longest_lines = shown.len();
        }
    }
    assert_eq!("GameEvents.Handlers", longest);
    assert_eq!(3, longest_lines);
    assert!(longest_lines <= widths.len());
}

// ---- the UB page -------------------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: PressingAToolButtonFlipsTheRowTheSettingsPageLists
#[test]
fn pressing_a_tool_button_flips_the_row_the_settings_page_lists() {
    for button in &SWITCHES {
        let mut rig = PageRig::new(&SharedStorage::new());
        let before = rig.row_value(button.row);
        assert!(before == "True" || before == "False", "{}", button.row);

        main_page::press(&mut rig.catalog, &mut rig.page, button);
        let after = rig.row_value(button.row);
        assert_ne!(before, after, "{}", button.row);
        assert!(after == "True" || after == "False");

        main_page::press(&mut rig.catalog, &mut rig.page, button);
        assert_eq!(before, rig.row_value(button.row));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: ACaptionFollowsTheRowWhenTheSettingsPageChangesIt
#[test]
fn a_caption_follows_the_row_when_the_settings_page_changes_it() {
    let mut rig = PageRig::new(&SharedStorage::new());
    let button = main_page::find("ToggleUbNametags").unwrap();
    let before = main_page::caption(&rig.catalog, button);
    let was_on = before.ends_with(": On");
    assert_eq!(
        if was_on {
            "Name Tags: On"
        } else {
            "Name Tags: Off"
        },
        before
    );

    rig.filter("Nametags.Enabled");
    rig.click(0);

    assert_eq!(
        if was_on {
            "Name Tags: Off"
        } else {
            "Name Tags: On"
        },
        main_page::caption(&rig.catalog, button)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: TheColourOfAButtonAgreesWithItsCaption
#[test]
fn the_colour_of_a_button_agrees_with_its_caption() {
    let rig = PageRig::new(&SharedStorage::new());
    let pairs: Vec<(String, u32)> = SWITCHES
        .iter()
        .filter(|button| button.action != "ToggleUbReceiveCommands")
        .map(|button| {
            (
                main_page::caption(&rig.catalog, button),
                main_page::color(&rig.catalog, button),
            )
        })
        .collect();
    assert!(pairs.iter().any(|(caption, _)| caption.ends_with("On")));
    assert!(pairs.iter().any(|(caption, _)| caption.ends_with("Off")));
    for (caption, colour) in pairs {
        let on = caption.ends_with("On");
        assert_eq!(
            if on {
                SWITCH_ON_COLOR
            } else {
                SWITCH_OFF_COLOR
            },
            colour,
            "{caption}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: FlippingASwitchHereReportsItLikeTheSettingsPageDoes
#[test]
fn flipping_a_switch_here_reports_it_like_the_settings_page_does() {
    let mut rig = PageRig::new(&SharedStorage::new());
    let button = main_page::find("ToggleUbAliases").unwrap();
    main_page::press(&mut rig.catalog, &mut rig.page, button);
    assert!(rig.notice().starts_with("Aliases.Enabled is now "));
}
