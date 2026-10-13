//! The UB settings files: where each tier lands, the order a profile file
//! is looked for in, what happens to a file that does not read, and the
//! alias lists beside them. Expected values are the C# tests'.
//!
//! The store keeps its tiers in memory and writes a changed one at
//! `flush`, which the plugin calls before its call returns (register row
//! STORE-1); the tests flush where the C# store would already have written.

use mosstank_settings::ub::{
    UbSettingCatalog, UbSettingScope, UbSettingValue, UbSettingValues, definitions,
};
use mosstank_stores::storage::{MemoryStorage, NoStorage, NullLog, RecordingLog};
use mosstank_stores::ub_store::UbSettingStore;

fn bound(storage: &mut MemoryStorage, server: &str, character: &str) -> UbSettingStore {
    let mut store = UbSettingStore::new();
    store.bind(storage, &mut NullLog, Some(server), Some(character));
    store
}

fn store(storage: &mut MemoryStorage) -> UbSettingStore {
    bound(storage, "Coldeve", "Acdream")
}

fn write(
    store: &mut UbSettingStore,
    storage: &mut MemoryStorage,
    scope: UbSettingScope,
    name: &str,
    value: UbSettingValue,
) {
    store.write(scope, name, value);
    store.flush(storage, &mut NullLog);
}

fn has_key(storage: &MemoryStorage, key: &str) -> bool {
    storage.keys().any(|k| k == key)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: EachTierLandsInTheFolderItsLayoutSays
#[test]
fn each_tier_lands_in_the_folder_its_layout_says() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);

    store.write(
        UbSettingScope::Global,
        "Nametags.Enabled",
        UbSettingValue::from_bool(false),
    );
    store.write(
        UbSettingScope::Profile,
        "AutoVendor.Tries",
        UbSettingValue::from_int(9),
    );
    store.write(
        UbSettingScope::Character,
        "Networking.Tags",
        UbSettingValue::from_collection(&["tank"]),
    );
    store.flush(&mut storage, &mut NullLog);

    assert!(has_key(&storage, "mosstank/ub/settings.json"));
    assert!(has_key(
        &storage,
        "mosstank/ub/profiles/default.settings.json"
    ));
    assert!(has_key(
        &storage,
        "mosstank/ub/Coldeve/Acdream/settings.json"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AValueWrittenInOneSessionComesBackInTheNext
#[test]
fn a_value_written_in_one_session_comes_back_in_the_next() {
    let mut storage = MemoryStorage::new();
    let mut first = store(&mut storage);
    write(
        &mut first,
        &mut storage,
        UbSettingScope::Profile,
        "AutoVendor.Tries",
        UbSettingValue::from_int(9),
    );

    let again = store(&mut storage).read(UbSettingScope::Profile, "AutoVendor.Tries");

    assert_eq!(again.unwrap().as_int32(), 9);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: TheNarrowestFileThatHasAValueWins
#[test]
fn the_narrowest_file_that_has_a_value_wins() {
    let mut storage = MemoryStorage::new();
    let mut catalog = UbSettingCatalog::new(store(&mut storage), Vec::new());
    let row = catalog.require("AutoVendor.Tries").unwrap();

    assert_eq!(catalog.get(&row).as_int32(), 4);

    for (scope, value) in [
        (UbSettingScope::Global, 1),
        (UbSettingScope::Profile, 2),
        (UbSettingScope::Character, 3),
    ] {
        catalog
            .values_mut()
            .write(scope, "AutoVendor.Tries", UbSettingValue::from_int(value));
        catalog.values_mut().flush(&mut storage, &mut NullLog);
        assert_eq!(catalog.get(&row).as_int32(), value);
    }

    assert!(
        catalog
            .values_mut()
            .clear(UbSettingScope::Character, "AutoVendor.Tries")
    );
    assert_eq!(catalog.get(&row).as_int32(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AnotherCharacterOnTheSameAccountKeepsItsOwnFile
#[test]
fn another_character_on_the_same_account_keeps_its_own_file() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);
    write(
        &mut store,
        &mut storage,
        UbSettingScope::Character,
        "Networking.Tags",
        UbSettingValue::from_collection(&["tank"]),
    );

    store.bind(&mut storage, &mut NullLog, Some("Coldeve"), Some("Horan"));

    assert!(
        store
            .read(UbSettingScope::Character, "Networking.Tags")
            .is_none()
    );
    write(
        &mut store,
        &mut storage,
        UbSettingScope::Character,
        "Networking.Tags",
        UbSettingValue::from_collection(&["healer"]),
    );
    assert!(has_key(&storage, "mosstank/ub/Coldeve/Horan/settings.json"));

    store.bind(&mut storage, &mut NullLog, Some("Coldeve"), Some("Acdream"));
    assert_eq!(
        store
            .read(UbSettingScope::Character, "Networking.Tags")
            .unwrap()
            .items,
        vec!["tank".to_owned()]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ANamedProfileIsItsOwnFileAndTheChoiceIsRememberedPerCharacter
#[test]
fn a_named_profile_is_its_own_file_and_the_choice_is_remembered_per_character() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);

    store.select_profile(&mut storage, &mut NullLog, "raid");
    write(
        &mut store,
        &mut storage,
        UbSettingScope::Profile,
        "AutoVendor.Tries",
        UbSettingValue::from_int(9),
    );
    assert!(has_key(&storage, "mosstank/ub/profiles/raid.settings.json"));

    store.select_profile(&mut storage, &mut NullLog, "default");
    assert!(
        store
            .read(UbSettingScope::Profile, "AutoVendor.Tries")
            .is_none()
    );

    // The next session on this character opens the same profile.
    store.select_profile(&mut storage, &mut NullLog, "raid");
    let next = self::store(&mut storage);
    assert_eq!(next.profile_name(), "raid");
    assert_eq!(
        next.read(UbSettingScope::Profile, "AutoVendor.Tries")
            .unwrap()
            .as_int32(),
        9
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AProfileMadeHereIsAFileAndIsListedFromThenOn
#[test]
fn a_profile_made_here_is_a_file_and_is_listed_from_then_on() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);

    let (made, notice) = store
        .create_profile(&mut storage, &mut NullLog, "raid", false)
        .unwrap();
    assert!(made);
    assert!(notice.contains("raid"));
    assert_eq!(store.profile_name(), "raid");
    assert!(has_key(&storage, "mosstank/ub/profiles/raid.settings.json"));
    assert_eq!(
        store.available_profiles(&mut storage).unwrap(),
        vec!["default".to_owned(), "raid".to_owned()]
    );
    assert_eq!(
        self::store(&mut storage)
            .available_profiles(&mut storage)
            .unwrap(),
        vec!["default".to_owned(), "raid".to_owned()]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AProfileCopiedHereCarriesTheOpenOnesValuesAndLeavesItAlone
#[test]
fn a_profile_copied_here_carries_the_open_ones_values_and_leaves_it_alone() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);
    write(
        &mut store,
        &mut storage,
        UbSettingScope::Profile,
        "AutoVendor.Tries",
        UbSettingValue::from_int(9),
    );

    assert!(
        store
            .create_profile(&mut storage, &mut NullLog, "raid", true)
            .unwrap()
            .0
    );

    assert_eq!(
        store
            .read(UbSettingScope::Profile, "AutoVendor.Tries")
            .unwrap()
            .as_int32(),
        9
    );
    write(
        &mut store,
        &mut storage,
        UbSettingScope::Profile,
        "AutoVendor.Tries",
        UbSettingValue::from_int(11),
    );
    store.select_profile(&mut storage, &mut NullLog, "default");
    assert_eq!(
        store
            .read(UbSettingScope::Profile, "AutoVendor.Tries")
            .unwrap()
            .as_int32(),
        9
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AProfileNeedsANameAndCannotTakeOneAlreadyInUse
#[test]
fn a_profile_needs_a_name_and_cannot_take_one_already_in_use() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);

    let (made, unnamed) = store
        .create_profile(&mut storage, &mut NullLog, "  ", false)
        .unwrap();
    assert!(!made);
    assert!(unnamed.to_lowercase().contains("name"));

    assert!(
        store
            .create_profile(&mut storage, &mut NullLog, "raid", false)
            .unwrap()
            .0
    );
    let (made, taken) = store
        .create_profile(&mut storage, &mut NullLog, "RAID", false)
        .unwrap();
    assert!(!made);
    assert!(taken.contains("already"));
    assert!(
        !store
            .create_profile(&mut storage, &mut NullLog, "default", false)
            .unwrap()
            .0
    );
    assert_eq!(store.profile_name(), "raid");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: TheProfileChoiceIsNotAnOrdinarySettingAndIsNeverListedAsOne
#[test]
fn the_profile_choice_is_not_an_ordinary_setting_and_is_never_listed_as_one() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);
    store.select_profile(&mut storage, &mut NullLog, "raid");

    let document = storage
        .get("mosstank/ub/Coldeve/Acdream/settings.json")
        .unwrap();
    assert!(document.contains("raid"));
    assert!(
        !definitions::all()
            .iter()
            .any(|definition| definition.name.starts_with('$'))
    );
    assert!(store.read(UbSettingScope::Character, "$profile").is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ASettingThisVersionDoesNotKnowSurvivesAWrite
#[test]
fn a_setting_this_version_does_not_know_survives_a_write() {
    let mut storage = MemoryStorage::new();
    storage.insert(
        "mosstank/ub/settings.json",
        "{\n  \"FromTheFuture.Something\": \"42\"\n}",
    );

    let mut store = store(&mut storage);
    write(
        &mut store,
        &mut storage,
        UbSettingScope::Global,
        "Nametags.Enabled",
        UbSettingValue::from_bool(false),
    );

    assert!(
        storage
            .get("mosstank/ub/settings.json")
            .unwrap()
            .contains("FromTheFuture.Something")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ADamagedFileIsReportedAndLeavesTheDefaultsStanding
#[test]
fn a_damaged_file_is_reported_and_leaves_the_defaults_standing() {
    let mut storage = MemoryStorage::new();
    storage.insert("mosstank/ub/settings.json", "{ this is not json");
    let mut log = RecordingLog::default();

    let mut store = UbSettingStore::new();
    store.bind(&mut storage, &mut log, Some("Coldeve"), Some("Acdream"));

    assert!(
        store
            .read(UbSettingScope::Global, "Nametags.Enabled")
            .is_none()
    );
    assert!(
        log.warnings
            .iter()
            .any(|warning| warning.contains("mosstank/ub/settings.json"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AFileThatWouldNotReadAtAllIsNotWrittenOverEither
#[test]
fn a_file_that_would_not_read_at_all_is_not_written_over_either() {
    let mut storage = MemoryStorage::new();
    let original = "{\n  \"Nametags.Enabled\": \"False\"\n}";
    storage.insert("mosstank/ub/settings.json", original);
    storage
        .unreadable
        .push("mosstank/ub/settings.json".to_owned());
    let mut log = RecordingLog::default();

    let mut store = UbSettingStore::new();
    store.bind(&mut storage, &mut log, Some("Coldeve"), Some("Acdream"));
    store.write(
        UbSettingScope::Global,
        "Nametags.MaxRange",
        UbSettingValue::from_single(20.0),
    );
    store.flush(&mut storage, &mut log);

    assert_eq!(storage.get("mosstank/ub/settings.json"), Some(original));
    assert!(!store.is_writable(UbSettingScope::Global));
    assert!(
        log.warnings
            .iter()
            .any(|warning| warning.contains("mosstank/ub/settings.json"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ATierSaysWhetherAWriteToItReachesAFile
#[test]
fn a_tier_says_whether_a_write_to_it_reaches_a_file() {
    let mut storage = MemoryStorage::new();
    storage.insert(
        "mosstank/ub/profiles/default.settings.json",
        "{ this is not json",
    );
    let store = store(&mut storage);

    assert!(store.is_writable(UbSettingScope::Global));
    assert!(store.is_writable(UbSettingScope::Character));
    assert!(!store.is_writable(UbSettingScope::Profile));

    // Nowhere to write at all, and no character yet, are the same answer.
    let mut unavailable = UbSettingStore::new();
    unavailable.load(&mut NoStorage, &mut NullLog);
    assert!(!unavailable.is_writable(UbSettingScope::Global));
    let mut unbound = UbSettingStore::new();
    unbound.load(&mut MemoryStorage::new(), &mut NullLog);
    assert!(!unbound.is_writable(UbSettingScope::Character));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ASettingsFileCopiedFromTheOriginalIsTheDamagedFileCase
#[test]
fn a_settings_file_copied_from_the_original_is_the_damaged_file_case() {
    let copied_in = "{\n  \"$type\": \"UtilityBelt.Settings, UtilityBelt\",\n  \"Plugin\": { \"SettingsProfile\": \"[character]\" },\n  \"DungeonMaps\": { \"Display\": { \"Walls\": { \"Color\": -16777089 } } }\n}";
    let mut storage = MemoryStorage::new();
    storage.insert("mosstank/ub/Coldeve/Acdream/settings.json", copied_in);
    let mut log = RecordingLog::default();

    let mut store = UbSettingStore::new();
    store.bind(&mut storage, &mut log, Some("Coldeve"), Some("Acdream"));
    store.write(
        UbSettingScope::Character,
        "NetworkUI.SelectedTag",
        UbSettingValue::from_text("raid"),
    );
    store.flush(&mut storage, &mut log);

    assert!(!store.is_writable(UbSettingScope::Character));
    assert_eq!(
        storage.get("mosstank/ub/Coldeve/Acdream/settings.json"),
        Some(copied_in)
    );
    assert!(
        log.warnings
            .iter()
            .any(|warning| warning.contains("mosstank/ub/Coldeve/Acdream/settings.json"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: WithNowhereToWriteEverythingStillWorksForTheSession
#[test]
fn with_nowhere_to_write_everything_still_works_for_the_session() {
    let mut store = UbSettingStore::new();
    store.bind(
        &mut NoStorage,
        &mut NullLog,
        Some("Coldeve"),
        Some("Acdream"),
    );
    let mut catalog = UbSettingCatalog::new(store, Vec::new());
    let row = catalog.require("AutoVendor.Tries").unwrap();

    catalog.set(&row, UbSettingValue::from_int(9));
    catalog.values_mut().flush(&mut NoStorage, &mut NullLog);

    assert_eq!(catalog.get(&row).as_int32(), 9);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: BeforeLoginThereIsNoCharacterFolderAndTheWiderTiersStillRead
#[test]
fn before_login_there_is_no_character_folder_and_the_wider_tiers_still_read() {
    let mut storage = MemoryStorage::new();
    let mut store = UbSettingStore::new();
    store.load(&mut storage, &mut NullLog);
    store.write(
        UbSettingScope::Global,
        "Nametags.Enabled",
        UbSettingValue::from_bool(false),
    );
    store.write(
        UbSettingScope::Character,
        "Networking.Tags",
        UbSettingValue::from_collection(&["x"]),
    );
    store.flush(&mut storage, &mut NullLog);

    assert!(has_key(&storage, "mosstank/ub/settings.json"));
    assert!(
        !storage
            .keys()
            .any(|key| key.starts_with("mosstank/ub/Coldeve/"))
    );
    assert!(
        !store
            .read(UbSettingScope::Global, "Nametags.Enabled")
            .unwrap()
            .boolean
    );
    assert_eq!(
        store
            .read(UbSettingScope::Character, "Networking.Tags")
            .unwrap()
            .items,
        vec!["x".to_owned()]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AProfileFileIsLookedForFromTheNarrowestFolderOutwards
#[test]
fn a_profile_file_is_looked_for_from_the_narrowest_folder_outwards() {
    let mut storage = MemoryStorage::new();
    let store = store(&mut storage);
    let resolve = |storage: &mut MemoryStorage| {
        store
            .resolve_profile_key(storage, "autovendor", "shops.utl")
            .unwrap()
    };

    assert_eq!(resolve(&mut storage), "mosstank/ub/autovendor/shops.utl");

    for (file, expected) in [
        (
            "mosstank/ub/autovendor/default.utl",
            "mosstank/ub/autovendor/default.utl",
        ),
        (
            "mosstank/ub/Coldeve/autovendor/default.utl",
            "mosstank/ub/Coldeve/autovendor/default.utl",
        ),
        (
            "mosstank/ub/Coldeve/Acdream/autovendor/default.utl",
            "mosstank/ub/Coldeve/Acdream/autovendor/default.utl",
        ),
        // A file under its own name beats every default, from the widest
        // folder up.
        (
            "mosstank/ub/autovendor/shops.utl",
            "mosstank/ub/autovendor/shops.utl",
        ),
        (
            "mosstank/ub/Coldeve/autovendor/shops.utl",
            "mosstank/ub/Coldeve/autovendor/shops.utl",
        ),
        (
            "mosstank/ub/Coldeve/Acdream/autovendor/shops.utl",
            "mosstank/ub/Coldeve/Acdream/autovendor/shops.utl",
        ),
    ] {
        storage.insert(file, "");
        assert_eq!(resolve(&mut storage), expected);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ANameWithACharacterNoFolderCanHoldIsMadeSafe
#[test]
fn a_name_with_a_character_no_folder_can_hold_is_made_safe() {
    let mut storage = MemoryStorage::new();
    let mut store = bound(&mut storage, "Coldeve", "Bad:Name");

    write(
        &mut store,
        &mut storage,
        UbSettingScope::Character,
        "Networking.Tags",
        UbSettingValue::from_collection(&["x"]),
    );

    assert!(has_key(
        &storage,
        "mosstank/ub/Coldeve/Bad_Name/settings.json"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: TheNewTreeNeverTouchesTheLayoutAlreadyThere
#[test]
fn the_new_tree_never_touches_the_layout_already_there() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);
    store.write(
        UbSettingScope::Global,
        "Nametags.Enabled",
        UbSettingValue::from_bool(false),
    );
    store.write(
        UbSettingScope::Profile,
        "AutoVendor.Tries",
        UbSettingValue::from_int(9),
    );
    store.write(
        UbSettingScope::Character,
        "Networking.Tags",
        UbSettingValue::from_collection(&["x"]),
    );
    store.flush(&mut storage, &mut NullLog);

    assert!(storage.keys().all(|key| key.starts_with("mosstank/ub/")));
}

// --- The alias file -------------------------------------------------------------

fn alias_store(storage: &mut MemoryStorage) -> UbSettingStore {
    bound(storage, "Coldeve", "Acdream")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: TheAliasProfileNamesTheFile
#[test]
fn the_alias_profile_names_the_file() {
    for (profile, key) in [
        ("[character]", "mosstank/ub/Coldeve/Acdream/aliases.txt"),
        ("[CHARACTER]", "mosstank/ub/Coldeve/Acdream/aliases.txt"),
        ("  [character]  ", "mosstank/ub/Coldeve/Acdream/aliases.txt"),
        ("shared", "mosstank/ub/profiles/shared.aliases.txt"),
        ("my:aliases", "mosstank/ub/profiles/my_aliases.aliases.txt"),
    ] {
        assert_eq!(
            alias_store(&mut MemoryStorage::ordinal())
                .alias_file_key(Some(profile))
                .as_deref(),
            Some(key)
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: ABlankProfileIsTheCharactersOwnFile
#[test]
fn a_blank_profile_is_the_characters_own_file() {
    assert_eq!(
        alias_store(&mut MemoryStorage::ordinal())
            .alias_file_key(Some(""))
            .as_deref(),
        Some("mosstank/ub/Coldeve/Acdream/aliases.txt")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: TheCharacterFileNeedsACharacter
#[test]
fn the_character_file_needs_a_character() {
    let store = UbSettingStore::new();
    assert_eq!(store.alias_file_key(Some("[character]")), None);
    assert_eq!(
        store.alias_file_key(Some("shared")).as_deref(),
        Some("mosstank/ub/profiles/shared.aliases.txt")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: LinesRoundTripThroughTheFile
#[test]
fn lines_round_trip_through_the_file() {
    let mut storage = MemoryStorage::ordinal();
    let mut store = alias_store(&mut storage);
    let key = store.alias_file_key(Some("[character]")).unwrap();

    assert!(
        store
            .read_lines(&mut storage, &mut NullLog, Some(&key))
            .is_empty()
    );
    assert!(store.write_lines(
        &mut storage,
        &mut NullLog,
        Some(&key),
        &["^/a$ = /b", "[eat] ^/c$ -> 1"]
    ));

    assert_eq!(storage.get(&key), Some("^/a$ = /b\n[eat] ^/c$ -> 1\n"));
    let expected = vec!["^/a$ = /b".to_owned(), "[eat] ^/c$ -> 1".to_owned()];
    assert_eq!(
        store.read_lines(&mut storage, &mut NullLog, Some(&key)),
        expected
    );
    assert_eq!(
        alias_store(&mut storage).read_lines(&mut storage, &mut NullLog, Some(&key)),
        expected
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AHandWrittenFileReads
#[test]
fn a_hand_written_file_reads() {
    let mut storage = MemoryStorage::ordinal();
    storage.insert(
        "mosstank/ub/profiles/shared.aliases.txt",
        "^/a$ = /b\r\n\r\n  ^/c$ = /d  \r\n",
    );
    assert_eq!(
        alias_store(&mut storage).read_lines(
            &mut storage,
            &mut NullLog,
            Some("mosstank/ub/profiles/shared.aliases.txt")
        ),
        vec!["^/a$ = /b".to_owned(), "^/c$ = /d".to_owned()]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: AWriteWithNoFileIsRefused
#[test]
fn a_write_with_no_file_is_refused() {
    let mut storage = MemoryStorage::ordinal();
    let mut store = UbSettingStore::new();
    assert!(!store.write_lines(&mut storage, &mut NullLog, None, &["^/a$ = /b"]));
    assert!(
        store
            .read_lines(&mut storage, &mut NullLog, None)
            .is_empty()
    );
    let mut nowhere = UbSettingStore::new();
    nowhere.bind(
        &mut NoStorage,
        &mut NullLog,
        Some("Coldeve"),
        Some("Acdream"),
    );
    assert!(!nowhere.write_lines(
        &mut NoStorage,
        &mut NullLog,
        Some("mosstank/ub/Coldeve/Acdream/aliases.txt"),
        &["^/a$ = /b"]
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbSettingStoreTests.cs :: RebindingForgetsWhatWasRead
#[test]
fn rebinding_forgets_what_was_read() {
    let mut storage = MemoryStorage::ordinal();
    let mut store = alias_store(&mut storage);
    let key = store.alias_file_key(Some("[character]")).unwrap();
    store.write_lines(&mut storage, &mut NullLog, Some(&key), &["^/a$ = /b"]);
    storage.insert(&key, "^/x$ = /y\n");
    assert_eq!(
        store.read_lines(&mut storage, &mut NullLog, Some(&key)),
        vec!["^/a$ = /b".to_owned()]
    );

    store.bind(&mut storage, &mut NullLog, Some("Coldeve"), Some("Horan"));
    store.bind(&mut storage, &mut NullLog, Some("Coldeve"), Some("Acdream"));
    assert_eq!(
        store.read_lines(&mut storage, &mut NullLog, Some(&key)),
        vec!["^/x$ = /y".to_owned()]
    );
}

/// The file is the C#'s byte for byte: System.Text.Json, indented, Windows
/// line ends, names in ordinal order, non-ASCII escaped.
#[test]
fn the_file_is_written_as_system_text_json_writes_it() {
    let mut storage = MemoryStorage::new();
    let mut store = store(&mut storage);
    store.write(
        UbSettingScope::Global,
        "b.Second",
        UbSettingValue::from_text("x"),
    );
    store.write(
        UbSettingScope::Global,
        "A.First",
        UbSettingValue::from_text("caf\u{e9} <1>"),
    );
    store.flush(&mut storage, &mut NullLog);

    assert_eq!(
        storage.get("mosstank/ub/settings.json"),
        Some("{\r\n  \"A.First\": \"caf\\u00E9 \\u003C1\\u003E\",\r\n  \"b.Second\": \"x\"\r\n}")
    );
}
