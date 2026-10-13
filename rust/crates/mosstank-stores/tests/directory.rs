//! The shared VTank profile folder's names, pickers and binding file,
//! against `VtankProfileDirectoryTests`.

use mosstank_formats::fixtures;
use mosstank_formats::usd::VtankDatabase;
use mosstank_stores::directory::{
    self, BY_CHARACTER_LABEL, DEFAULT_LABEL, NONE_LABEL, ProfileEntry,
};
use mosstank_stores::storage::{MemoryStorage, NoStorage, PluginStorage};

/// Keys matched exactly, as the C# test's storage matches them.
fn storage_with(files: &[&str]) -> MemoryStorage {
    let mut storage = MemoryStorage::ordinal();
    for key in files {
        storage.write_text(key, "1\r\n").unwrap();
    }
    storage
}

fn settings(
    storage: &mut dyn PluginStorage,
    mine_only: bool,
    current: Option<&str>,
) -> Vec<ProfileEntry> {
    directory::list_settings_profiles(storage, "Barris", "Coldeve", mine_only, current).unwrap()
}

fn has_file(entries: &[ProfileEntry], file: &str) -> bool {
    entries.iter().any(|e| e.file_name == file)
}

fn has_display(entries: &[ProfileEntry], display: &str) -> bool {
    entries.iter().any(|e| e.display_name == display)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: AutoCharacterFileNameMatchesRealInstalledConvention
#[test]
fn auto_character_file_name_matches_real_installed_convention() {
    assert_eq!(
        "--Barris_Coldeve.usd",
        directory::auto_character_file_name("Barris", "Coldeve", "usd")
    );
    assert_eq!(
        "--Barris_Coldeve.usd",
        directory::auto_character_file_name("Barris", "Coldeve", ".usd")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: SubProfilesAreDisplayedAsCharBracketSuffix
#[test]
fn sub_profiles_are_displayed_as_char_bracket_suffix() {
    assert_eq!(
        Some("[Char] Base".to_owned()),
        directory::try_display_name("--Barris_Coldeve_Base.usd", "Barris", "Coldeve")
    );
    assert_eq!(
        Some("[Char] viridian".to_owned()),
        directory::try_display_name("--Barris_Coldeve_viridian.usd", "Barris", "Coldeve")
    );
    assert_eq!(
        None,
        directory::try_display_name("--Barris_Coldeve.usd", "Barris", "Coldeve")
    );
    assert_eq!(
        None,
        directory::try_display_name("--Someone_Coldeve_Base.usd", "Barris", "Coldeve")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: OtherCharactersHiddenPrefixedFilesAreInvisible
#[test]
fn other_characters_hidden_prefixed_files_are_invisible() {
    assert!(directory::is_hidden_from_other_characters(
        "--Someone_Coldeve.usd",
        "Barris",
        "Coldeve"
    ));
    assert!(!directory::is_hidden_from_other_characters(
        "--Barris_Coldeve_Base.usd",
        "Barris",
        "Coldeve"
    ));
    assert!(!directory::is_hidden_from_other_characters(
        "MyNamedProfile.usd",
        "Barris",
        "Coldeve"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: ListSettingsProfilesSeedsDefaultAndByCharFirst
#[test]
fn list_settings_profiles_seeds_default_and_by_char_first() {
    let mut storage = storage_with(&[
        "mosstank/profiles/Shared.usd",
        "mosstank/profiles/--Barris_Coldeve.usd",
        "mosstank/profiles/--Barris_Coldeve_Base.usd",
        "mosstank/profiles/--Someone_Coldeve.usd",
    ]);

    let entries = settings(&mut storage, false, None);

    assert_eq!(DEFAULT_LABEL, entries[0].display_name);
    assert_eq!(BY_CHARACTER_LABEL, entries[1].display_name);
    assert!(has_display(&entries, "Shared.usd"));
    assert!(has_display(&entries, "[Char] Base"));
    assert!(!has_file(&entries, "--Someone_Coldeve.usd"));
    assert!(!has_file(&entries, "--Barris_Coldeve.usd"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: ListNavigationProfilesFiltersBothReservedPrefixesWithinNavsFolder
#[test]
fn list_navigation_profiles_filters_both_reserved_prefixes_within_navs_folder() {
    let mut storage = storage_with(&[
        "mosstank/navs/Hunt.af",
        "mosstank/navs/--Barris_Coldeve.af",
        "mosstank/navs/~~backup.af",
    ]);

    let entries = directory::list_navigation_profiles(&mut storage).unwrap();

    assert_eq!(NONE_LABEL, entries[0].display_name);
    assert_eq!(BY_CHARACTER_LABEL, entries[1].display_name);
    assert!(has_display(&entries, "Hunt.af"));
    assert!(has_file(&entries, "mosstank/navs/Hunt.af"));
    assert!(!entries.iter().any(|e| e.display_name.starts_with("--")));
    assert!(!entries.iter().any(|e| e.display_name.starts_with("~~")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: NavigationAndMetaPickersEachSeeOnlyTheirOwnFolder
#[test]
fn navigation_and_meta_pickers_each_see_only_their_own_folder() {
    let mut storage = storage_with(&[
        "mosstank/navs/Hunt.af",
        // The same bare name, in the other folder.
        "mosstank/metas/Hunt.af",
        "mosstank/navs/--Barris_Coldeve.af",
        "mosstank/metas/--Barris_Coldeve.af",
    ]);

    let nav_entries = directory::list_navigation_profiles(&mut storage).unwrap();
    let meta_entries = directory::list_meta_profiles(&mut storage).unwrap();

    assert!(has_file(&nav_entries, "mosstank/navs/Hunt.af"));
    assert!(!has_file(&nav_entries, "mosstank/metas/Hunt.af"));
    assert!(!nav_entries.iter().any(|e| e.display_name.starts_with("--")));

    assert!(has_file(&meta_entries, "mosstank/metas/Hunt.af"));
    assert!(!has_file(&meta_entries, "mosstank/navs/Hunt.af"));
    assert!(
        !meta_entries
            .iter()
            .any(|e| e.display_name.starts_with("--"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: AnotherCharactersAutoRouteIsHiddenFromTheNavPicker
#[test]
fn another_characters_auto_route_is_hidden_from_the_nav_picker() {
    let key = format!(
        "mosstank/navs/{}",
        directory::auto_character_file_name("Someone", "Coldeve", "af")
    );
    let mut storage = storage_with(&[&key]);

    let nav_entries = directory::list_navigation_profiles(&mut storage).unwrap();
    let meta_entries = directory::list_meta_profiles(&mut storage).unwrap();

    assert!(!nav_entries.iter().any(|e| e.file_name.contains("Someone")));
    assert!(!meta_entries.iter().any(|e| e.file_name.contains("Someone")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: ListingsOnUnavailableStorageOnlySeedTheBuiltInEntries
#[test]
fn listings_on_unavailable_storage_only_seed_the_built_in_entries() {
    let entries = settings(&mut NoStorage, false, None);
    assert_eq!(2, entries.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: NestedPathKeysAreNotTreatedAsProfileFiles
#[test]
fn nested_path_keys_are_not_treated_as_profile_files() {
    let mut storage = storage_with(&["subdir/Nested.usd", "mosstank/profiles/Flat.usd"]);

    let entries = settings(&mut storage, false, None);

    assert!(has_file(&entries, "mosstank/profiles/Flat.usd"));
    assert!(!entries.iter().any(|e| e.file_name.contains("subdir")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: MineOnlyKeepsTheCurrentlySelectedSharedFileVisible
#[test]
fn mine_only_keeps_the_currently_selected_shared_file_visible() {
    let mut storage = storage_with(&[
        "mosstank/profiles/Shared.usd",
        "mosstank/profiles/OtherShared.usd",
    ]);

    let without_current = settings(&mut storage, true, None);
    assert!(!has_file(&without_current, "mosstank/profiles/Shared.usd"));
    assert!(!has_file(
        &without_current,
        "mosstank/profiles/OtherShared.usd"
    ));

    let with_current = settings(&mut storage, true, Some("mosstank/profiles/Shared.usd"));
    assert!(has_file(&with_current, "mosstank/profiles/Shared.usd"));
    assert!(!has_file(
        &with_current,
        "mosstank/profiles/OtherShared.usd"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: MineOnlyUncheckedIgnoresCurrentFileAndKeepsEverything
#[test]
fn mine_only_unchecked_ignores_current_file_and_keeps_everything() {
    let mut storage = storage_with(&["mosstank/profiles/Shared.usd"]);

    let entries = settings(&mut storage, false, None);

    assert!(has_file(&entries, "mosstank/profiles/Shared.usd"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: AstFileNameHasNoHiddenPrefixAndNameServerOrder
#[test]
fn ast_file_name_has_no_hidden_prefix_and_name_server_order() {
    assert_eq!(
        "Barris_Coldeve.ast",
        directory::ast_file_name("Barris", "Coldeve")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: CdfFileNameUsesServerNameOrderReversedFromAst
#[test]
fn cdf_file_name_uses_server_name_order_reversed_from_ast() {
    assert_eq!(
        "mosstank/profiles/Coldeve_Barris.cdf",
        directory::cdf_file_name("Barris", "Coldeve")
    );
}

fn binding_storage(text: &str) -> MemoryStorage {
    let mut storage = MemoryStorage::ordinal();
    storage
        .write_text("mosstank/profiles/Coldeve_Barris.cdf", text)
        .unwrap();
    storage
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: TryReadCharacterBindingParsesAllFourLines
#[test]
fn try_read_character_binding_parses_all_four_lines() {
    let mut storage = binding_storage(
        "uTank2 CDF 1.0\r\n--Barris_Coldeve.usd\r\nLoot.utl\r\n--Barris_Coldeve.nav\r\nHunt.met\r\n",
    );

    let binding = directory::try_read_character_binding(&mut storage, "Barris", "Coldeve")
        .unwrap()
        .expect("a binding");

    assert_eq!(
        "mosstank/profiles/--Barris_Coldeve.usd",
        binding.settings_file_name
    );
    assert_eq!("mosstank/loot/Loot.utl", binding.loot_file_name);
    assert_eq!("mosstank/navs/--Barris_Coldeve.nav", binding.nav_file_name);
    assert_eq!(
        Some("mosstank/metas/Hunt.met".to_owned()),
        binding.meta_file_name
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: TryReadCharacterBindingWithoutMetaLineLeavesMetaNull
#[test]
fn try_read_character_binding_without_meta_line_leaves_meta_null() {
    let mut storage = binding_storage(
        "uTank2 CDF 1.0\r\n--Barris_Coldeve.usd\r\nLoot.utl\r\n--Barris_Coldeve.nav\r\n",
    );

    let binding = directory::try_read_character_binding(&mut storage, "Barris", "Coldeve")
        .unwrap()
        .expect("a binding");

    assert_eq!(None, binding.meta_file_name);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: TryReadCharacterBindingRewritesLegacyUtsSettingsExtension
#[test]
fn try_read_character_binding_rewrites_legacy_uts_settings_extension() {
    let mut storage = binding_storage(
        "uTank2 CDF 1.0\r\n--Barris_Coldeve.uts\r\nLoot.utl\r\n--Barris_Coldeve.nav\r\n",
    );

    let binding = directory::try_read_character_binding(&mut storage, "Barris", "Coldeve")
        .unwrap()
        .expect("a binding");

    assert_eq!(
        "mosstank/profiles/--Barris_Coldeve.usd",
        binding.settings_file_name
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: TryReadCharacterBindingReturnsNullOnHeaderMismatch
#[test]
fn try_read_character_binding_returns_null_on_header_mismatch() {
    let mut storage = binding_storage(
        "uTank2 CDF 0.9\r\n--Barris_Coldeve.usd\r\nLoot.utl\r\n--Barris_Coldeve.nav\r\n",
    );

    assert_eq!(
        None,
        directory::try_read_character_binding(&mut storage, "Barris", "Coldeve").unwrap()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: TryReadCharacterBindingReturnsNullWhenFileMissing
#[test]
fn try_read_character_binding_returns_null_when_file_missing() {
    assert_eq!(
        None,
        directory::try_read_character_binding(&mut MemoryStorage::ordinal(), "Barris", "Coldeve")
            .unwrap()
    );
}

const AST_FIXTURES: [&str; 3] = ["owner-a.ast", "owner-b.ast", "owner-c.ast"];

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: RealAstFixturesParseAsTheSpellsTable
#[test]
fn real_ast_fixtures_parse_as_the_spells_table() {
    for file_name in AST_FIXTURES {
        let text = fixtures::read_vtank(file_name);

        let database = VtankDatabase::parse(&text).unwrap();

        let spells = database.find("Spells").expect("a Spells table");
        assert_eq!(
            vec!["SpellID", "EndTime", "Target", "CastTime"],
            spells.column_names,
            "{file_name}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankProfileDirectoryTests.cs :: RealAstFixturesRoundTripByteIdentical
#[test]
fn real_ast_fixtures_round_trip_byte_identical() {
    for file_name in AST_FIXTURES {
        let original = fixtures::read_vtank(file_name);

        let rewritten = VtankDatabase::parse(&original).unwrap().render();

        assert_eq!(original, rewritten, "{file_name}");
    }
}
