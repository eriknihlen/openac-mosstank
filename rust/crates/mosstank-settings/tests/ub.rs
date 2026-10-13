//! The UB settings catalogue, its values and its coverage of the tools it
//! was ported from, against the C# tests' cases and their inventory tables.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use mosstank_dotnet::compat::number::to_string_f32;
use mosstank_settings::defaults::VtankEnumValue;
use mosstank_settings::ub::{
    UbSettingBinding, UbSettingCatalog, UbSettingDefinition, UbSettingKind, UbSettingScope,
    UbSettingValue, UbSettingValueBag, UbSettingValues, definitions,
};
use openac_plugin_compat::text::casing;

fn catalog() -> UbSettingCatalog<UbSettingValueBag> {
    UbSettingCatalog::new(UbSettingValueBag::default(), Vec::new())
}

fn display(catalog: &UbSettingCatalog<UbSettingValueBag>, name: &str) -> String {
    catalog.display(&catalog.require(name).unwrap())
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: EveryDefinitionHasADottedNameASummaryAndACategory
#[test]
fn every_definition_has_a_dotted_name_a_summary_and_a_category() {
    assert!(!definitions::all().is_empty());
    for definition in definitions::all() {
        assert!(definition.name.contains('.'), "{}", definition.name);
        assert!(
            !casing::is_white_space_only(&definition.summary),
            "{} has no summary",
            definition.name
        );
        assert!(definition.summary.ends_with('.'), "{}", definition.name);
        assert!(!definition.category().is_empty());
        let dot = definition.name.find('.').unwrap();
        assert_eq!(&definition.name[..dot], definition.category());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: NoTwoDefinitionsShareAName
#[test]
fn no_two_definitions_share_a_name() {
    let names: HashSet<&str> = definitions::all().iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names.len(), definitions::all().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: EveryDefaultMatchesTheDeclaredValueShape
#[test]
fn every_default_matches_the_declared_value_shape() {
    for definition in definitions::all() {
        assert_eq!(
            definition.kind, definition.default.kind,
            "{}",
            definition.name
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: EveryEnumSettingOffersItsChoicesAndEveryOtherSettingOffersNone
#[test]
fn every_enum_setting_offers_its_choices_and_every_other_setting_offers_none() {
    for definition in definitions::all() {
        assert_eq!(
            definition.kind == UbSettingKind::Enum,
            !definition.choices.is_empty(),
            "{}",
            definition.name
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: AllThreeScopeTiersAreUsed
#[test]
fn all_three_scope_tiers_are_used() {
    let scopes: HashSet<UbSettingScope> = definitions::all().iter().map(|d| d.scope).collect();
    for scope in UbSettingScope::ALL {
        assert!(scopes.contains(&scope), "{scope:?}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: TheCharacterTierHoldsOnlyWhoTheCharacterIs
#[test]
fn the_character_tier_holds_only_who_the_character_is() {
    let mut character_scoped: Vec<&str> = definitions::all()
        .iter()
        .filter(|d| d.scope == UbSettingScope::Character)
        .map(|d| d.name.as_str())
        .collect();
    character_scoped.sort_unstable();
    assert_eq!(
        character_scoped,
        [
            "Aliases.Profile",
            "AutoTinker.CharmedSmith",
            "GameEvents.Profile",
            "NetworkUI.SelectedTag",
            "Networking.ReceiveCommands",
            "Networking.Tags",
            "Sharing.CastTag",
            "Sharing.Vitals",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: TheNestedDisplayGroupsAreFlattenedToDottedNames
#[test]
fn the_nested_display_groups_are_flattened_to_dotted_names() {
    let all = definitions::all();
    let count = |prefix: &str| all.iter().filter(|d| d.name.starts_with(prefix)).count();
    assert!(all.iter().any(|d| d.name == "Nametags.Monster.TagColor"));
    assert!(
        all.iter()
            .any(|d| d.name == "DungeonMaps.Display.Markers.Portals.Color")
    );
    assert_eq!(count("Nametags.Player."), 5);
    assert_eq!(count("DungeonMaps.Display.Markers.Doors."), 5);
    assert_eq!(count("DungeonMaps.Display.Walls."), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: EveryCategoryIsOneOfTheInScopeTools
#[test]
fn every_category_is_one_of_the_in_scope_tools() {
    let mut categories: Vec<&str> = definitions::all()
        .iter()
        .map(UbSettingDefinition::category)
        .collect();
    categories.sort_unstable();
    categories.dedup();
    assert_eq!(
        categories,
        [
            "Aliases",
            "AutoTinker",
            "AutoVendor",
            "AutoXp",
            "DungeonMaps",
            "EquipmentManager",
            "GameEvents",
            "ItemGiver",
            "Jumper",
            "LandscapeMaps",
            "Nametags",
            "NetworkUI",
            "Networking",
            "Plugin",
            "Sharing",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: ABoundRowReadsAndWritesThroughItsGetSetPair
#[test]
fn a_bound_row_reads_and_writes_through_its_get_set_pair() {
    let mut catalog = catalog();
    let row = catalog.require("AutoVendor.Tries").unwrap();
    assert_eq!(catalog.get(&row).as_int32(), 4);

    catalog.set(&row, UbSettingValue::from_int(9));
    let again = catalog.require("AutoVendor.Tries").unwrap();
    assert_eq!(catalog.get(&again).as_int32(), 9);
    assert_eq!(
        catalog
            .values()
            .read(UbSettingScope::Profile, "AutoVendor.Tries")
            .unwrap()
            .as_int32(),
        9
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: EveryRowIsWrittenToTheTierItsScopeDeclaresAndToNoOther
#[test]
fn every_row_is_written_to_the_tier_its_scope_declares_and_to_no_other() {
    let mut catalog = catalog();
    for row in catalog.settings().to_vec() {
        let value = catalog.get(&row);
        catalog.set(&row, value);
        for scope in UbSettingScope::ALL {
            let written = catalog.values().read(scope, row.name());
            assert_eq!(
                written.is_some(),
                scope == row.scope(),
                "{} declares {:?}; {scope:?}",
                row.name(),
                row.scope()
            );
        }
    }
    let count = |scope| {
        catalog
            .settings()
            .iter()
            .filter(|r| r.scope() == scope)
            .count()
    };
    assert_eq!(count(UbSettingScope::Global), 6);
    assert_eq!(count(UbSettingScope::Profile), 154);
    assert_eq!(count(UbSettingScope::Character), 8);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: AnUnwrittenRowReadsItsDefault
#[test]
fn an_unwritten_row_reads_its_default() {
    let catalog = catalog();
    assert!(
        catalog
            .get(&catalog.require("Jumper.PauseNav").unwrap())
            .boolean
    );
    assert_eq!(
        catalog
            .get(&catalog.require("Nametags.Player.TagColor").unwrap())
            .as_color(),
        0xFF00_FFFF
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: AnOverriddenRowReachesItsLiveOwnerInsteadOfTheBag
#[test]
fn an_overridden_row_reaches_its_live_owner_instead_of_the_bag() {
    let live = Rc::new(Cell::new(3));
    let (read, write) = (Rc::clone(&live), Rc::clone(&live));
    let mut catalog = UbSettingCatalog::new(
        UbSettingValueBag::default(),
        vec![(
            "Jumper.Attempts".to_owned(),
            UbSettingBinding::new(
                move || UbSettingValue::from_int(read.get()),
                move |value| write.set(value.as_int32()),
            ),
        )],
    );

    let row = catalog.require("Jumper.Attempts").unwrap();
    catalog.set(&row, UbSettingValue::from_int(7));

    assert_eq!(live.get(), 7);
    assert_eq!(catalog.get(&row).as_int32(), 7);
    assert!(
        catalog
            .values()
            .read(UbSettingScope::Profile, "Jumper.Attempts")
            .is_none()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: EveryShippedDecimalDefaultReadsAsTheNumberItIs
#[test]
fn every_shipped_decimal_default_reads_as_the_number_it_is() {
    let catalog = catalog();
    for (name, expected) in [
        ("Nametags.Player.TagSize", "0.15"),
        ("Nametags.Player.TickerSize", "0.1"),
        ("Nametags.Monster.TagSize", "0.15"),
        ("Nametags.Monster.TickerSize", "0.1"),
        ("DungeonMaps.MapZoom", "4.2"),
        ("Nametags.MaxRange", "35"),
        ("ItemGiver.Range", "15"),
        ("ItemGiver.Delay", "0"),
    ] {
        assert_eq!(display(&catalog, name), expected, "{name}");
    }
    let singles: Vec<&UbSettingDefinition> = definitions::all()
        .iter()
        .filter(|d| d.kind == UbSettingKind::Single)
        .collect();
    assert_eq!(singles.len(), 17);
    for definition in singles {
        assert_eq!(
            to_string_f32(definition.default.as_single()),
            definition.default.to_storage_string()
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: RequireRefusesAnUnknownName
#[test]
fn require_refuses_an_unknown_name() {
    let catalog = catalog();
    assert!(catalog.try_get("Nope.Missing").is_none());
    assert_eq!(
        catalog.require("Nope.Missing").unwrap_err().to_string(),
        "No UB setting is named 'Nope.Missing'."
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCatalogTests.cs :: CategoriesComeBackInOrderWithTheirRows
#[test]
fn categories_come_back_in_order_with_their_rows() {
    let catalog = catalog();
    assert_eq!(catalog.categories().len(), 15);
    assert_eq!(catalog.categories()[0], "Aliases");
    assert_eq!(catalog.settings().len(), 168);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: EachShapeKeepsItsOwnKindAndPayload
#[test]
#[allow(clippy::float_cmp)]
fn each_shape_keeps_its_own_kind_and_payload() {
    assert!(UbSettingValue::from_bool(true).boolean);
    assert_eq!(UbSettingValue::from_bool(true).kind, UbSettingKind::Bool);
    assert_eq!(UbSettingValue::from_int(7).as_int32(), 7);
    assert_eq!(UbSettingValue::from_single(1.5).as_single(), 1.5);
    assert_eq!(UbSettingValue::from_double(2.25).as_double(), 2.25);
    assert_eq!(UbSettingValue::from_text("hello").text, "hello");
    assert_eq!(UbSettingValue::from_choice(3).as_int32(), 3);
    assert_eq!(
        UbSettingValue::from_color(0xFF00_FF00).as_color(),
        0xFF00_FF00
    );
    assert_eq!(
        UbSettingValue::from_collection(&["a", "b"]).items,
        ["a", "b"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: TextIsTheSameInEveryLocale
#[test]
#[allow(clippy::float_cmp)]
fn text_is_the_same_in_every_locale() {
    // There is no current culture here: every text is the invariant one.
    assert_eq!(
        UbSettingValue::from_single(0.15).to_storage_string(),
        "0.15"
    );
    assert_eq!(
        UbSettingValue::from_double(0.05).to_storage_string(),
        "0.05"
    );
    assert_eq!(
        UbSettingValue::from_color(0xFF00_FF00).to_storage_string(),
        "#FF00FF00"
    );
    let parsed = UbSettingValue::try_parse(UbSettingKind::Double, "0.05").unwrap();
    assert_eq!(parsed.as_double(), 0.05);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: EveryShapeReadsAsTheValueItIsAndRoundTripsThroughThatText
#[test]
fn every_shape_reads_as_the_value_it_is_and_round_trips_through_that_text() {
    let values = [
        (UbSettingValue::from_bool(true), "True"),
        (UbSettingValue::from_bool(false), "False"),
        (UbSettingValue::from_int(-12), "-12"),
        (UbSettingValue::from_single(4.2), "4.2"),
        (UbSettingValue::from_single(0.15), "0.15"),
        (UbSettingValue::from_single(0.1), "0.1"),
        (UbSettingValue::from_single(35.0), "35"),
        (
            UbSettingValue::from_double(0.020_833_333_333_333_3),
            "0.0208333333333333",
        ),
        (UbSettingValue::from_double(15.0), "15"),
        (
            UbSettingValue::from_text("a name with spaces"),
            "a name with spaces",
        ),
        (UbSettingValue::from_choice(2), "2"),
        (UbSettingValue::from_color(0xFFF4_A460), "#FFF4A460"),
        (
            UbSettingValue::from_collection(&["one", "two", "three"]),
            "one\ntwo\nthree",
        ),
        (UbSettingValue::from_collection::<&str>(&[]), ""),
    ];
    for (value, text) in values {
        assert_eq!(value.to_storage_string(), text);
        let back = UbSettingValue::try_parse(value.kind, text)
            .unwrap_or_else(|| panic!("{:?} '{text}' did not read back.", value.kind));
        assert_eq!(back.kind, value.kind);
        assert_eq!(back.to_storage_string(), text);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: AColourReadsFromEverySpellingItIsWrittenIn
#[test]
fn a_colour_reads_from_every_spelling_it_is_written_in() {
    for text in [
        "#FF00FFFF",
        "FF00FFFF",
        "#00FFFF",
        "-16711681",
        "4278255615",
    ] {
        let value = UbSettingValue::try_parse(UbSettingKind::Color, text).unwrap();
        assert_eq!(value.as_color(), 0xFF00_FFFF, "{text}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: ABareNumberIsADecimalAndHexNeedsAHashOrALetter
#[test]
fn a_bare_number_is_a_decimal_and_hex_needs_a_hash_or_a_letter() {
    for (text, expected) in [
        ("16777215", 0x00FF_FFFF),
        ("123456", 0x0001_E240),
        ("#123456", 0xFF12_3456),
        ("#12345678", 0x1234_5678),
        ("FF00FFFF", 0xFF00_FFFF),
        ("00FFFF", 0xFF00_FFFF),
    ] {
        let value = UbSettingValue::try_parse(UbSettingKind::Color, text).unwrap();
        assert_eq!(value.as_color(), expected, "{text}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: ABadValueIsRefusedRatherThanGuessedAt
#[test]
fn a_bad_value_is_refused_rather_than_guessed_at() {
    assert!(UbSettingValue::try_parse(UbSettingKind::Int, "seven").is_none());
    assert!(UbSettingValue::try_parse(UbSettingKind::Color, "#GGGGGG").is_none());
    assert!(UbSettingValue::try_parse(UbSettingKind::Bool, "maybe").is_none());
    assert!(UbSettingValue::try_parse(UbSettingKind::Double, "1,5").is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: BooleansReadBackFromEitherSpelling
#[test]
fn booleans_read_back_from_either_spelling() {
    assert!(
        UbSettingValue::try_parse(UbSettingKind::Bool, "True")
            .unwrap()
            .boolean
    );
    assert!(
        !UbSettingValue::try_parse(UbSettingKind::Bool, "false")
            .unwrap()
            .boolean
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingValueTests.cs :: TheValueColumnShowsTheChoicesLabelRatherThanItsNumber
#[test]
fn the_value_column_shows_the_choices_label_rather_than_its_number() {
    let definition = UbSettingDefinition {
        choices: vec![
            VtankEnumValue {
                value: 0,
                label: "Forward".to_owned(),
            },
            VtankEnumValue {
                value: 1,
                label: "Backward".to_owned(),
            },
        ],
        ..UbSettingDefinition::new(
            "Jumper.Direction",
            "Which way the jump is aimed.",
            UbSettingKind::Enum,
            UbSettingValue::from_choice(1),
            UbSettingScope::Profile,
        )
    };
    assert_eq!(
        definition.display(&UbSettingValue::from_choice(1)),
        "Backward"
    );
    assert_eq!(definition.display(&UbSettingValue::from_choice(2)), "2");
    assert_eq!(definition.display(&UbSettingValue::from_bool(true)), "True");
    assert_eq!(
        definition.display(&UbSettingValue::from_collection(&["one", "two"])),
        "one, two"
    );
}

// --- Coverage: the inventory of every leaf value the in-scope tools
// declare, written down as data (from UbSettingCoverageTests.cs), and
// reconciled against the catalogue in both directions.

const FLAT_SETTINGS: &[(&str, &str)] = &[
    ("Nametags", "Enabled"),
    ("Nametags", "MaxRange"),
    ("DungeonMaps", "Enabled"),
    ("DungeonMaps", "Debug"),
    ("DungeonMaps", "DrawWhenClosed"),
    ("DungeonMaps", "ShowVisitedTiles"),
    ("DungeonMaps", "VisitedTilesColor"),
    ("DungeonMaps", "ShowCompass"),
    ("DungeonMaps", "Opacity"),
    ("DungeonMaps", "MapZoom"),
    ("DungeonMaps", "LabelFontSize"),
    ("DungeonMaps", "MapWindowX"),
    ("DungeonMaps", "MapWindowY"),
    ("DungeonMaps", "MapWindowWidth"),
    ("DungeonMaps", "MapWindowHeight"),
    ("LandscapeMaps", "Enabled"),
    ("LandscapeMaps", "Opacity"),
    ("LandscapeMaps", "MapWindowX"),
    ("LandscapeMaps", "MapWindowY"),
    ("LandscapeMaps", "MapWindowWidth"),
    ("LandscapeMaps", "MapWindowHeight"),
    ("NetworkUI", "Enabled"),
    ("NetworkUI", "ShowHudWhenClosed"),
    ("NetworkUI", "TrackedItems"),
    ("NetworkUI", "SelectedTag"),
    ("NetworkUI", "WindowPositionX"),
    ("NetworkUI", "WindowPositionY"),
    ("Networking", "Tags"),
    ("AutoVendor", "Enabled"),
    ("AutoVendor", "EnableBuying"),
    ("AutoVendor", "EnableSelling"),
    ("AutoVendor", "TestMode"),
    ("AutoVendor", "Think"),
    ("AutoVendor", "ShowMerchantInfo"),
    ("AutoVendor", "OnlyFromMainPack"),
    ("AutoVendor", "Tries"),
    ("AutoVendor", "TriesTime"),
    ("AutoXp", "StopBeforeMax"),
    ("AutoXp", "TriesTime"),
    ("AutoXp", "MaxXpChunk"),
    ("AutoXp", "Policy"),
    ("Aliases", "Enabled"),
    ("Aliases", "Profile"),
    ("Aliases", "DefinedAliases"),
    ("EquipmentManager", "Think"),
    ("GameEvents", "Enabled"),
    ("GameEvents", "Profile"),
    ("GameEvents", "GameEventHandlers"),
    ("AutoTinker", "MinPercentage"),
    ("AutoTinker", "MaxTinks"),
    ("Jumper", "PauseNav"),
    ("Jumper", "ThinkComplete"),
    ("Jumper", "ThinkFail"),
    ("Jumper", "Attempts"),
    ("Plugin", "Debug"),
    ("Plugin", "GenericMessageDisplay.Enabled"),
    ("Plugin", "GenericMessageDisplay.Color"),
    ("Plugin", "DebugMessageDisplay.Enabled"),
    ("Plugin", "DebugMessageDisplay.Color"),
    ("Plugin", "ExpressionMessageDisplay.Enabled"),
    ("Plugin", "ExpressionMessageDisplay.Color"),
    ("Plugin", "ErrorMessageDisplay.Enabled"),
    ("Plugin", "ErrorMessageDisplay.Color"),
    ("InventoryManager", "AutoCram"),
    ("InventoryManager", "AutoStack"),
    ("InventoryManager", "IGThink"),
    ("InventoryManager", "IGFailure"),
    ("InventoryManager", "IGBusyCount"),
    ("InventoryManager", "IGRange"),
    ("InventoryManager", "IGUIEnabled"),
    ("InventoryManager", "IGWindowX"),
    ("InventoryManager", "IGWindowY"),
    ("InventoryManager", "TreatStackAsSingleItem"),
    ("InventoryManager", "WatchLootProfile"),
    ("VTankControl", "VitalSharing"),
    ("VTankControl", "PatchExpressionEngine"),
    ("VTankControl", "MetaTickRateOverride"),
    ("VTankControl", "FixPortalLoops"),
    ("VTankControl", "PortalLoopCount"),
];

const NAMETAG_FIELDS: [&str; 5] = [
    "Enabled",
    "TagColor",
    "TagSize",
    "TickerColor",
    "TickerSize",
];
const NAMETAG_GROUPS: [&str; 7] = [
    "Player",
    "Pet",
    "AllegiancePlayer",
    "Portal",
    "Npc",
    "Vendor",
    "Monster",
];
const MAP_COLOUR_FIELDS: [&str; 2] = ["Enabled", "Color"];
const MAP_COLOUR_GROUPS: [&str; 8] = [
    "DungeonName",
    "Walls",
    "InnerWalls",
    "RampedWalls",
    "Stairs",
    "Floors",
    "VisualNavStickyPoint",
    "VisualNavLines",
];
const MAP_MARKER_FIELDS: [&str; 5] = ["Enabled", "ShowLabel", "UseIcon", "Color", "Size"];
const MAP_MARKER_GROUPS: [&str; 11] = [
    "You",
    "Others",
    "Items",
    "Monsters",
    "NPCs",
    "MyCorpse",
    "OtherCorpses",
    "Portals",
    "Containers",
    "Doors",
    "EverythingElse",
];

const NAMED_HERE: [(&str, &str); 6] = [
    ("GameEvents.GameEventHandlers", "GameEvents.Handlers"),
    ("InventoryManager.IGThink", "ItemGiver.Think"),
    ("InventoryManager.IGFailure", "ItemGiver.FailureLimit"),
    ("InventoryManager.IGBusyCount", "ItemGiver.BusyRetryLimit"),
    ("InventoryManager.IGRange", "ItemGiver.Range"),
    ("VTankControl.VitalSharing", "Sharing.Vitals"),
];

const SAME_SWITCH: &str = "The same switch is already on the Options page and in the option catalogue; a second row would be a second copy of one value.";

const EXCLUDED: [(&str, &str); 21] = [
    (
        "DungeonMaps.MapWindowX",
        "The host keeps where a window sits.",
    ),
    (
        "DungeonMaps.MapWindowY",
        "The host keeps where a window sits.",
    ),
    (
        "DungeonMaps.MapWindowWidth",
        "The host keeps how big a window is.",
    ),
    (
        "DungeonMaps.MapWindowHeight",
        "The host keeps how big a window is.",
    ),
    (
        "LandscapeMaps.MapWindowX",
        "The host keeps where a window sits.",
    ),
    (
        "LandscapeMaps.MapWindowY",
        "The host keeps where a window sits.",
    ),
    (
        "LandscapeMaps.MapWindowWidth",
        "The host keeps how big a window is.",
    ),
    (
        "LandscapeMaps.MapWindowHeight",
        "The host keeps how big a window is.",
    ),
    (
        "NetworkUI.WindowPositionX",
        "The host keeps where a window sits.",
    ),
    (
        "NetworkUI.WindowPositionY",
        "The host keeps where a window sits.",
    ),
    (
        "InventoryManager.IGWindowX",
        "The host keeps where a window sits.",
    ),
    (
        "InventoryManager.IGWindowY",
        "The host keeps where a window sits.",
    ),
    ("InventoryManager.AutoCram", SAME_SWITCH),
    ("InventoryManager.AutoStack", SAME_SWITCH),
    (
        "InventoryManager.IGUIEnabled",
        "The host decides whether a plugin panel is shown, and there is no separate hand-over window here to show.",
    ),
    (
        "InventoryManager.TreatStackAsSingleItem",
        "A hand-over count here is always in units and splits the last stack, so there is no second behaviour to pick.",
    ),
    (
        "InventoryManager.WatchLootProfile",
        "Plugin storage gives no file-change notification; a loot profile is re-read when it is selected.",
    ),
    (
        "VTankControl.PatchExpressionEngine",
        "It swaps the macro plugin's expression evaluator for the original's at run time. There is one expression engine here, this plugin's own, and it is always the one used, so there is no foreign evaluator to replace.",
    ),
    (
        "VTankControl.MetaTickRateOverride",
        "It pokes the macro plugin's meta loop from a frame handler to make it tick faster than the rate built into it. The meta engine here is this plugin's own and runs on its own schedule, so there is no foreign loop to poke.",
    ),
    (
        "VTankControl.FixPortalLoops",
        "It counts portal exits into the same landcell and then calls a repair that the original leaves unwritten -- the method is a comment and a bare return -- so the switch turns on a counter and nothing else.",
    ),
    (
        "VTankControl.PortalLoopCount",
        "It is how many exits that counter waits for, and it goes with the switch above it.",
    ),
];

const ADDED_HERE: [(&str, &str); 4] = [
    (
        "Networking.ReceiveCommands",
        "Per-character local command reception, enabled by default.",
    ),
    (
        "ItemGiver.Delay",
        "A pause between one hand-over and the next, which the original has no setting for; it was added here so a run can be slowed to what the server accepts.",
    ),
    (
        "AutoTinker.CharmedSmith",
        "Whether the character carries the Charmed Smith augmentation, which adds five points to an imbue's chance. The original reads the augmentation off the character; this client exposes no augmentations, so the only way the planned percentage can match what the server rolls is for the player to tick it.",
    ),
    (
        "Sharing.CastTag",
        "Which of the other clients' casts to take in, by the tag their client was started with. The original takes every connected client's casts; the tags already exist on the peer record here, so a character can listen to its own group alone.",
    ),
];

const FLAT_DEFAULTS: &[(&str, &str)] = &[
    ("Nametags.Enabled", "True"),
    ("Nametags.MaxRange", "35"),
    ("DungeonMaps.Enabled", "True"),
    ("DungeonMaps.Debug", "False"),
    ("DungeonMaps.DrawWhenClosed", "True"),
    ("DungeonMaps.ShowVisitedTiles", "True"),
    ("DungeonMaps.VisitedTilesColor", "#FFFF96FF"),
    ("DungeonMaps.ShowCompass", "True"),
    ("DungeonMaps.Opacity", "16"),
    ("DungeonMaps.MapZoom", "4.2"),
    ("DungeonMaps.LabelFontSize", "10"),
    ("LandscapeMaps.Enabled", "True"),
    ("LandscapeMaps.Opacity", "16"),
    ("NetworkUI.Enabled", "False"),
    ("NetworkUI.ShowHudWhenClosed", "True"),
    ("NetworkUI.SelectedTag", "All"),
    ("Aliases.Enabled", "True"),
    ("Aliases.Profile", "[character]"),
    ("AutoVendor.Enabled", "True"),
    ("AutoVendor.EnableBuying", "True"),
    ("AutoVendor.EnableSelling", "True"),
    ("AutoVendor.TestMode", "False"),
    ("AutoVendor.Think", "False"),
    ("AutoVendor.ShowMerchantInfo", "True"),
    ("AutoVendor.OnlyFromMainPack", "False"),
    ("AutoVendor.Tries", "4"),
    ("AutoVendor.TriesTime", "5000"),
    ("AutoXp.StopBeforeMax", "10"),
    ("AutoXp.TriesTime", "300"),
    ("AutoXp.MaxXpChunk", "1000000000"),
    ("EquipmentManager.Think", "False"),
    ("GameEvents.Enabled", "True"),
    ("GameEvents.Profile", "[character]"),
    ("ItemGiver.Range", "15"),
    ("ItemGiver.Delay", "0"),
    ("ItemGiver.BusyRetryLimit", "10"),
    ("ItemGiver.FailureLimit", "3"),
    ("ItemGiver.Think", "False"),
    ("AutoTinker.MinPercentage", "99.5"),
    ("AutoTinker.MaxTinks", "10"),
    ("AutoTinker.CharmedSmith", "False"),
    ("Jumper.PauseNav", "True"),
    ("Jumper.ThinkComplete", "False"),
    ("Jumper.ThinkFail", "False"),
    ("Jumper.Attempts", "3"),
    ("Plugin.Debug", "False"),
    ("Plugin.GenericMessageDisplay.Enabled", "True"),
    ("Plugin.GenericMessageDisplay.Color", "5"),
    ("Plugin.DebugMessageDisplay.Enabled", "True"),
    ("Plugin.DebugMessageDisplay.Color", "14"),
    ("Plugin.ExpressionMessageDisplay.Enabled", "True"),
    ("Plugin.ExpressionMessageDisplay.Color", "5"),
    ("Plugin.ErrorMessageDisplay.Enabled", "True"),
    ("Plugin.ErrorMessageDisplay.Color", "15"),
    ("Sharing.Vitals", "False"),
    ("Networking.ReceiveCommands", "True"),
    ("Sharing.CastTag", ""),
];

const NAMETAG_DEFAULTS: [(&str, &str); 7] = [
    ("Player", "#FF00FFFF"),
    ("Pet", "#FF00FFFF"),
    ("AllegiancePlayer", "#FF00FF00"),
    ("Portal", "#FF00FF00"),
    ("Npc", "#FFFFFF00"),
    ("Vendor", "#FFFF00FF"),
    ("Monster", "#FFFF0000"),
];

const MAP_COLOUR_DEFAULTS: [(&str, &str); 8] = [
    ("DungeonName", "#FFFFFFFF"),
    ("Walls", "#FF00007F"),
    ("InnerWalls", "#FF7FBFFF"),
    ("RampedWalls", "#FF4EA6FF"),
    ("Stairs", "#FF003F7F"),
    ("Floors", "#FF007FBF"),
    ("VisualNavStickyPoint", "#FFADFF2F"),
    ("VisualNavLines", "#FFFF00FF"),
];

/// Group, enabled, use icon, show label, colour.
const MAP_MARKER_DEFAULTS: [(&str, bool, bool, bool, &str); 11] = [
    ("You", true, false, false, "#FFFF0000"),
    ("Others", true, true, true, "#FFFFFFFF"),
    ("Items", true, true, true, "#FFFFFFFF"),
    ("Monsters", true, true, false, "#FFFFA500"),
    ("NPCs", true, true, false, "#FFFFFF00"),
    ("MyCorpse", true, true, true, "#FFFF0000"),
    ("OtherCorpses", false, true, false, "#FFF5F5F5"),
    ("Portals", true, true, true, "#FFFFF0FF"),
    ("Containers", true, true, false, "#FFF4A460"),
    ("Doors", true, false, false, "#FFA52A2A"),
    ("EverythingElse", false, true, false, "#FFF5F5F5"),
];

const LIST_DEFAULTS: [(&str, usize); 4] = [
    ("NetworkUI.TrackedItems", 0),
    ("Networking.Tags", 0),
    ("Aliases.DefinedAliases", 0),
    ("GameEvents.Handlers", 0),
];

const DEFAULT_PINNED_ELSEWHERE: [(&str, &str); 1] = [(
    "AutoXp.Policy",
    "Its default is a weighting table with one line per target, which is neither a one-line value nor a count.",
)];

fn flat_settings() -> impl Iterator<Item = (&'static str, &'static str)> {
    FLAT_SETTINGS.iter().copied()
}

fn flat_defaults() -> &'static [(&'static str, &'static str)] {
    FLAT_DEFAULTS
}

fn shipped_defaults() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = flat_defaults()
        .iter()
        .map(|&(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    let bool_text = |value: bool| if value { "True" } else { "False" }.to_owned();
    for (group, color) in NAMETAG_DEFAULTS {
        let prefix = format!("Nametags.{group}.");
        rows.push((format!("{prefix}Enabled"), "True".to_owned()));
        rows.push((format!("{prefix}TagColor"), color.to_owned()));
        rows.push((format!("{prefix}TagSize"), "0.15".to_owned()));
        rows.push((format!("{prefix}TickerColor"), color.to_owned()));
        rows.push((format!("{prefix}TickerSize"), "0.1".to_owned()));
    }
    for (group, color) in MAP_COLOUR_DEFAULTS {
        rows.push((
            format!("DungeonMaps.Display.{group}.Enabled"),
            "True".to_owned(),
        ));
        rows.push((
            format!("DungeonMaps.Display.{group}.Color"),
            color.to_owned(),
        ));
    }
    for (group, enabled, icon, label, color) in MAP_MARKER_DEFAULTS {
        let prefix = format!("DungeonMaps.Display.Markers.{group}.");
        rows.push((format!("{prefix}Enabled"), bool_text(enabled)));
        rows.push((format!("{prefix}ShowLabel"), bool_text(label)));
        rows.push((format!("{prefix}UseIcon"), bool_text(icon)));
        rows.push((format!("{prefix}Color"), color.to_owned()));
        rows.push((format!("{prefix}Size"), "3".to_owned()));
    }
    rows
}

fn inventory() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = flat_settings()
        .map(|(tool, field)| (tool.to_owned(), field.to_owned()))
        .collect();
    for group in NAMETAG_GROUPS {
        for field in NAMETAG_FIELDS {
            rows.push(("Nametags".to_owned(), format!("{group}.{field}")));
        }
    }
    for group in MAP_COLOUR_GROUPS {
        for field in MAP_COLOUR_FIELDS {
            rows.push(("DungeonMaps".to_owned(), format!("Display.{group}.{field}")));
        }
    }
    for group in MAP_MARKER_GROUPS {
        for field in MAP_MARKER_FIELDS {
            rows.push((
                "DungeonMaps".to_owned(),
                format!("Display.Markers.{group}.{field}"),
            ));
        }
    }
    rows
}

fn inventory_keys() -> Vec<String> {
    inventory()
        .into_iter()
        .map(|(tool, field)| format!("{tool}.{field}"))
        .collect()
}

fn on_the_tab() -> HashSet<String> {
    definitions::all().iter().map(|d| d.name.clone()).collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: TheInventoryIsTheOneHundredAndEightyFiveLeafValuesTheToolsDeclare
#[test]
fn the_inventory_is_the_one_hundred_and_eighty_five_leaf_values_the_tools_declare() {
    let keys = inventory_keys();
    assert_eq!(keys.len(), 185);
    assert_eq!(keys.iter().collect::<HashSet<_>>().len(), keys.len());
    assert!(keys.iter().all(|key| key.contains('.')));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: EverySettingTheInScopeToolsDeclareIsOnTheTabOrExcludedWithAReason
#[test]
fn every_setting_the_in_scope_tools_declare_is_on_the_tab_or_excluded_with_a_reason() {
    let on_the_tab = on_the_tab();
    let excluded: HashMap<&str, &str> = EXCLUDED.into_iter().collect();
    let named_here: HashMap<&str, &str> = NAMED_HERE.into_iter().collect();
    let added_here: HashMap<&str, &str> = ADDED_HERE.into_iter().collect();

    let mut should_be_on_the_tab: HashMap<String, String> = HashMap::new();
    for declared in inventory_keys() {
        if excluded.contains_key(declared.as_str()) {
            continue;
        }
        let here = named_here
            .get(declared.as_str())
            .map_or_else(|| declared.clone(), |&here| here.to_owned());
        should_be_on_the_tab.insert(here, declared);
    }

    let mut missing: Vec<String> = should_be_on_the_tab
        .iter()
        .filter(|(here, _)| !on_the_tab.contains(*here))
        .map(|(here, declared)| format!("{declared} (looked for as {here})"))
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "declared but neither on the tab nor excluded: {missing:?}"
    );

    let mut unaccounted: Vec<&String> = on_the_tab
        .iter()
        .filter(|name| {
            !should_be_on_the_tab.contains_key(*name) && !added_here.contains_key(name.as_str())
        })
        .collect();
    unaccounted.sort();
    assert!(
        unaccounted.is_empty(),
        "on the tab but not declared: {unaccounted:?}"
    );

    // 185 declared, 21 excluded, our own additions: the arithmetic is stated.
    assert_eq!(excluded.len(), 21);
    assert_eq!(should_be_on_the_tab.len(), 164);
    assert_eq!(
        should_be_on_the_tab.len() + added_here.len(),
        on_the_tab.len()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: EveryExclusionNamesASettingThatExistsAndGivesAReason
#[test]
fn every_exclusion_names_a_setting_that_exists_and_gives_a_reason() {
    let declared: HashSet<String> = inventory_keys().into_iter().collect();
    for (key, reason) in EXCLUDED {
        assert!(
            declared.contains(key),
            "{key} is excluded but no tool declares it"
        );
        assert!(!casing::is_white_space_only(reason));
        assert!(reason.trim().ends_with('.'), "{key}");
    }
    for (key, reason) in ADDED_HERE {
        assert!(!declared.contains(key), "{key} is declared by a tool");
        assert!(!casing::is_white_space_only(reason));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: EveryRenameNamesASettingThatExistsAndARowThatExists
#[test]
fn every_rename_names_a_setting_that_exists_and_a_row_that_exists() {
    let declared: HashSet<String> = inventory_keys().into_iter().collect();
    let on_the_tab = on_the_tab();
    for (there, here) in NAMED_HERE {
        assert!(declared.contains(there), "{there}");
        assert!(on_the_tab.contains(here), "{here}");
        assert_ne!(there, here);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: EveryShippedDefaultIsTheValueItWasPortedFrom
#[test]
fn every_shipped_default_is_the_value_it_was_ported_from() {
    let catalog = catalog();
    for (name, expected) in shipped_defaults() {
        assert_eq!(
            catalog
                .require(&name)
                .unwrap()
                .definition
                .default
                .to_storage_string(),
            expected,
            "{name}"
        );
    }
    for (name, lines) in LIST_DEFAULTS {
        assert_eq!(
            catalog
                .require(name)
                .unwrap()
                .definition
                .default
                .items
                .len(),
            lines,
            "{name}"
        );
    }
    assert_eq!(
        shipped_defaults()
            .iter()
            .filter(|(_, value)| value.starts_with('#'))
            .count(),
        34
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: TheExperiencePolicyShipsWithTheWeightsItWasPortedFrom
#[test]
fn the_experience_policy_ships_with_the_weights_it_was_ported_from() {
    let catalog = catalog();
    let policy = catalog
        .get(&catalog.require("AutoXp.Policy").unwrap())
        .items;

    assert_eq!(policy.len(), 47);
    assert_eq!(policy[0], "Strength = 1");
    assert_eq!(policy[5], "Self = 1");
    assert_eq!(policy[6], "Health = 1.4");
    assert_eq!(policy[7], "Stamina = 0.1");
    assert_eq!(policy[8], "Mana = 0.1");
    assert_eq!(policy[9], "Alchemy = 0");
    assert_eq!(policy[25], "Jump = 0.02");
    assert_eq!(policy[34], "MeleeDefense = 5");
    assert_eq!(policy[46], "WeaponTinkering = 0");
    assert_eq!(policy.iter().filter(|l| l.ends_with(" = 10")).count(), 7);
    assert_eq!(policy.iter().filter(|l| l.ends_with(" = 0")).count(), 10);
    // One target and one weight a line, with a decimal point:
    // ^[A-Za-z]+ = (0|[1-9][0-9]*)(\.[0-9]+)?$
    for line in &policy {
        let (target, weight) = line.split_once(" = ").unwrap();
        assert!(
            !target.is_empty() && target.chars().all(|c| c.is_ascii_alphabetic()),
            "{line}"
        );
        let (whole, fraction) = weight.split_once('.').unwrap_or((weight, ""));
        assert!(
            whole == "0"
                || (!whole.starts_with('0')
                    && whole.chars().all(|c| c.is_ascii_digit())
                    && !whole.is_empty()),
            "{line}"
        );
        assert!(
            weight.contains('.') != fraction.is_empty()
                && fraction.chars().all(|c| c.is_ascii_digit()),
            "{line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: EveryRowOnTheTabHasItsDefaultWrittenDownSomewhere
#[test]
fn every_row_on_the_tab_has_its_default_written_down_somewhere() {
    let mut pinned: HashSet<String> = shipped_defaults()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    pinned.extend(LIST_DEFAULTS.iter().map(|(name, _)| (*name).to_owned()));
    pinned.extend(
        DEFAULT_PINNED_ELSEWHERE
            .iter()
            .map(|(name, _)| (*name).to_owned()),
    );
    let on_the_tab = on_the_tab();
    let mut unpinned: Vec<&String> = on_the_tab.iter().filter(|n| !pinned.contains(*n)).collect();
    unpinned.sort();
    assert!(unpinned.is_empty(), "no default written down: {unpinned:?}");
    let mut stale: Vec<&String> = pinned.iter().filter(|n| !on_the_tab.contains(*n)).collect();
    stale.sort();
    assert!(
        stale.is_empty(),
        "defaults for rows that do not exist: {stale:?}"
    );
    for (_, reason) in DEFAULT_PINNED_ELSEWHERE {
        assert!(reason.trim().ends_with('.'));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingCoverageTests.cs :: TheShapeOfTheCatalogueIsWhatTheDisplayToolsImply
#[test]
fn the_shape_of_the_catalogue_is_what_the_display_tools_imply() {
    let all = definitions::all();
    assert_eq!(
        all.iter()
            .filter(|d| d.kind == UbSettingKind::Color)
            .count(),
        34
    );
    assert_eq!(
        all.iter()
            .filter(|d| d.kind == UbSettingKind::Collection)
            .count(),
        5
    );
    assert!(
        !all.iter()
            .any(|d| casing::to_upper_invariant(&d.name).contains("WINDOW"))
    );
}
