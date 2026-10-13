//! The folder tree, its preparer, the profile folder's pickers and binding
//! names, and the two one-time copies (the flat layout into `mosstank/`, the
//! UB tree into the shared folder). Expected values are the C# tests'.

use std::cell::RefCell;
use std::rc::Rc;

use mosstank_stores::directory::{self, ProfileEntry};
use mosstank_stores::layout::{self, StorageFolderPreparer};
use mosstank_stores::layout_migration::{self, LayoutMigrationResult};
use mosstank_stores::storage::{MemoryStorage, NoStorage, PluginStorage};
use mosstank_stores::ub_store;
use mosstank_stores::ub_tree_migration::{self, UbTreeMigrationResult};
use openac_plugin_compat::datetime::DateTime;

type Lines = Rc<RefCell<Vec<String>>>;

fn build() -> (StorageFolderPreparer, Lines, Lines) {
    let log: Lines = Rc::default();
    let chat: Lines = Rc::default();
    let (log_sink, chat_sink) = (Rc::clone(&log), Rc::clone(&chat));
    let preparer = StorageFolderPreparer::new(
        move |line| log_sink.borrow_mut().push(line.to_owned()),
        move |line| chat_sink.borrow_mut().push(line.to_owned()),
    );
    (preparer, log, chat)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_start_makes_the_shared_profile_folders
#[test]
fn a_start_makes_the_shared_profile_folders() {
    let mut shared = MemoryStorage::new();
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_start(&mut shared, &mut storage);

    assert_eq!(
        shared.directories,
        strings(&[
            "mosstank/profiles",
            "mosstank/metas",
            "mosstank/navs",
            "mosstank/loot",
            "mosstank/ub/",
            "mosstank/ub/profiles",
            "mosstank/ub/autovendor",
            "mosstank/ub/equip",
            "mosstank/ub/itemgiver",
            "mosstank/ub/maps",
            "mosstank/ub/dungeonmaps",
            "mosstank/ub/dungeonmaps/visited",
        ])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_start_makes_the_plugin_storage_folders
#[test]
fn a_start_makes_the_plugin_storage_folders() {
    let mut shared = MemoryStorage::new();
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_start(&mut shared, &mut storage);

    assert_eq!(
        storage.directories,
        strings(&[
            "expressions/global",
            "expressions/persistent",
            "profiles/macro",
            "profiles/macro/sidecar",
        ])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_start_runs_once
#[test]
fn a_start_runs_once() {
    let mut shared = MemoryStorage::new();
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_start(&mut shared, &mut storage);
    let made = storage.directories.len();
    preparer.prepare_start(&mut shared, &mut storage);

    assert_eq!(storage.directories.len(), made);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_login_makes_the_server_and_character_tiers
#[test]
fn a_login_makes_the_server_and_character_tiers() {
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Acdream"));

    assert_eq!(
        storage.directories,
        strings(&[
            "mosstank/ub/Frostfell/",
            "mosstank/ub/Frostfell/autovendor",
            "mosstank/ub/Frostfell/equip",
            "mosstank/ub/Frostfell/Acdream/",
            "mosstank/ub/Frostfell/Acdream/autovendor",
            "mosstank/ub/Frostfell/Acdream/equip",
        ])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_name_a_folder_cannot_hold_is_filed_under_the_safe_spelling
#[test]
fn a_name_a_folder_cannot_hold_is_filed_under_the_safe_spelling() {
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_character(&mut storage, Some("Dark:Side"), Some("Ac/dream?"));

    assert_eq!(
        storage.directories,
        strings(&[
            "mosstank/ub/Dark_Side/",
            "mosstank/ub/Dark_Side/autovendor",
            "mosstank/ub/Dark_Side/equip",
            "mosstank/ub/Dark_Side/Ac_dream_/",
            "mosstank/ub/Dark_Side/Ac_dream_/autovendor",
            "mosstank/ub/Dark_Side/Ac_dream_/equip",
        ])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: The_same_login_twice_makes_nothing_again
#[test]
fn the_same_login_twice_makes_nothing_again() {
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Acdream"));
    storage.directories.clear();
    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Acdream"));

    assert!(storage.directories.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: Another_character_on_the_same_server_makes_only_its_own_tier
#[test]
fn another_character_on_the_same_server_makes_only_its_own_tier() {
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Acdream"));
    storage.directories.clear();
    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Horan"));

    assert_eq!(
        storage.directories,
        strings(&[
            "mosstank/ub/Frostfell/Horan/",
            "mosstank/ub/Frostfell/Horan/autovendor",
            "mosstank/ub/Frostfell/Horan/equip",
        ])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_login_without_a_world_names_nothing
#[test]
fn a_login_without_a_world_names_nothing() {
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_character(&mut storage, Some(""), Some("Acdream"));

    assert!(storage.directories.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_world_without_a_character_stops_at_the_server_tier
#[test]
fn a_world_without_a_character_stops_at_the_server_tier() {
    let mut storage = MemoryStorage::new();
    let (mut preparer, _, _) = build();

    preparer.prepare_character(&mut storage, Some("Frostfell"), None);

    assert_eq!(
        storage.directories,
        strings(&[
            "mosstank/ub/Frostfell/",
            "mosstank/ub/Frostfell/autovendor",
            "mosstank/ub/Frostfell/equip",
        ])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_refused_folder_is_logged_with_its_path_and_said_once
#[test]
fn a_refused_folder_is_logged_with_its_path_and_said_once() {
    let mut storage = MemoryStorage::new();
    storage
        .refuse_directories
        .push("mosstank/ub/maps".to_owned());
    let (mut preparer, log, chat) = build();

    preparer.prepare_start(&mut storage, &mut MemoryStorage::new());
    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Acdream"));

    let log = log.borrow();
    assert_eq!(log.len(), 1);
    // Path.Combine("C:\\memory", "mosstank", "ub", "maps") on Windows.
    assert!(log[0].contains("C:\\memory\\mosstank\\ub\\maps"));
    assert_eq!(chat.borrow().len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_refused_folder_does_not_stop_the_folders_after_it
#[test]
fn a_refused_folder_does_not_stop_the_folders_after_it() {
    let mut storage = MemoryStorage::new();
    storage
        .throw_directories
        .push("mosstank/ub/profiles".to_owned());
    let (mut preparer, log, _) = build();

    preparer.prepare_start(&mut storage, &mut MemoryStorage::new());

    assert!(
        storage
            .directories
            .contains(&"mosstank/ub/dungeonmaps/visited".to_owned())
    );
    let log = log.borrow();
    assert_eq!(log.len(), 1);
    assert!(log[0].contains("is not writable"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_refused_folder_is_tried_again_on_the_next_login
#[test]
fn a_refused_folder_is_tried_again_on_the_next_login() {
    let mut storage = MemoryStorage::new();
    storage
        .refuse_directories
        .push("mosstank/ub/Frostfell/equip".to_owned());
    let (mut preparer, _, chat) = build();

    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Acdream"));
    storage.directories.clear();
    preparer.prepare_character(&mut storage, Some("Frostfell"), Some("Horan"));

    assert!(
        storage
            .directories
            .contains(&"mosstank/ub/Frostfell/equip".to_owned())
    );
    assert_eq!(chat.borrow().len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: A_session_with_no_storage_is_not_a_failure
#[test]
fn a_session_with_no_storage_is_not_a_failure() {
    let (mut preparer, log, chat) = build();

    preparer.prepare_start(&mut NoStorage, &mut NoStorage);
    preparer.prepare_character(&mut NoStorage, Some("Frostfell"), Some("Acdream"));

    assert!(log.borrow().is_empty());
    assert!(chat.borrow().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: The_layout_is_the_one_the_settings_store_reads_from
#[test]
fn the_layout_is_the_one_the_settings_store_reads_from() {
    assert!(layout::vtank_profile_folders().contains(&format!("{}profiles", ub_store::ROOT)));
    assert!(
        layout::character_folders(Some("Frostfell"), Some("Acdream"))
            .contains(&format!("{}Frostfell/Acdream/", ub_store::ROOT))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/StorageLayoutTests.cs :: No_UB_folder_is_laid_out_in_the_plugins_own_storage
#[test]
fn no_ub_folder_is_laid_out_in_the_plugins_own_storage() {
    assert!(
        !layout::PLUGIN_STORAGE_FOLDERS
            .iter()
            .any(|folder| folder.to_ascii_lowercase().contains("ub/"))
    );
    for folder in layout::character_folders(Some("Frostfell"), Some("Acdream")) {
        assert!(folder.starts_with(&format!("{}/", directory::ROOT)));
    }
}

// --- The profile folder ------------------------------------------------------

fn displayed(entries: &[ProfileEntry]) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| !entry.file_name.is_empty())
        .map(|entry| entry.display_name.clone())
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: CanonicalCharacterKeyDropsTheServersDisplayMarker
#[test]
fn canonical_character_key_drops_the_servers_display_marker() {
    assert_eq!(
        directory::canonical_character_key(Some("+Acdream")),
        "Acdream"
    );
    assert_eq!(
        directory::canonical_character_key(Some("Acdream")),
        "Acdream"
    );
    assert_eq!(
        directory::canonical_character_key(Some("  +Acdream  ")),
        "Acdream"
    );
    assert_eq!(directory::canonical_character_key(None), "");
    assert_eq!(directory::canonical_character_key(Some("+")), "");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: EveryKindHasItsOwnFolderUnderOneRoot
#[test]
fn every_kind_has_its_own_folder_under_one_root() {
    assert_eq!(directory::ROOT, "mosstank");
    assert_eq!(directory::SETTINGS_FOLDER, "mosstank/profiles");
    assert_eq!(directory::NAV_FOLDER, "mosstank/navs");
    assert_eq!(directory::META_FOLDER, "mosstank/metas");
    assert_eq!(directory::LOOT_FOLDER, "mosstank/loot");
}

fn flat_storage(files: &[(&str, &str)]) -> MemoryStorage {
    let mut storage = MemoryStorage::ordinal();
    storage.root = None;
    for (key, text) in files {
        storage.insert(key, text);
    }
    storage
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: SettingsPickerListsOnlyUsdFromTheProfilesFolder
#[test]
fn settings_picker_lists_only_usd_from_the_profiles_folder() {
    let mut storage = flat_storage(&[
        ("mosstank/profiles/Shared.usd", "1\r\n"),
        ("mosstank/profiles/--Barris_Coldeve.usd", "1\r\n"),
        ("mosstank/profiles/Barris_Coldeve.cdf", "1\r\n"),
        ("mosstank/loot/Shared.utl", "1\r\n"),
        ("mosstank/navs/Shared.af", "1\r\n"),
        ("Elsewhere.usd", "1\r\n"),
    ]);

    let entries =
        directory::list_settings_profiles(&mut storage, "Barris", "Coldeve", false, None).unwrap();

    assert_eq!(displayed(&entries), strings(&["Shared.usd"]));
    assert_eq!(
        entries
            .iter()
            .find(|entry| !entry.file_name.is_empty())
            .unwrap()
            .file_name,
        "mosstank/profiles/Shared.usd"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: NavigationPickerListsAfAndNavFromTheNavsFolder
#[test]
fn navigation_picker_lists_af_and_nav_from_the_navs_folder() {
    let mut storage = flat_storage(&[
        ("mosstank/navs/Beta.af", "1\r\n"),
        ("mosstank/navs/alpha.nav", "1\r\n"),
        ("mosstank/navs/Beta.af.bak", "1\r\n"),
        ("mosstank/metas/Hunt.af", "1\r\n"),
        ("mosstank/loot/Keep.utl", "1\r\n"),
    ]);

    assert_eq!(
        displayed(&directory::list_navigation_profiles(&mut storage).unwrap()),
        strings(&["alpha.nav", "Beta.af"])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MetaPickerListsAfAndMetFromTheMetasFolder
#[test]
fn meta_picker_lists_af_and_met_from_the_metas_folder() {
    let mut storage = flat_storage(&[
        ("mosstank/metas/Hunt.af", "1\r\n"),
        ("mosstank/metas/dropped.met", "1\r\n"),
        ("mosstank/metas/Hunt.af.tmp", "1\r\n"),
        ("mosstank/navs/Route.af", "1\r\n"),
    ]);

    assert_eq!(
        displayed(&directory::list_meta_profiles(&mut storage).unwrap()),
        strings(&["dropped.met", "Hunt.af"])
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: LootPickerListsOnlyUtlFromTheLootFolder
#[test]
fn loot_picker_lists_only_utl_from_the_loot_folder() {
    let mut storage = flat_storage(&[
        ("mosstank/loot/Keep.utl", "1\r\n"),
        ("mosstank/loot/Keep.utl.twin-2026-01-01", "1\r\n"),
        ("mosstank/profiles/Shared.usd", "1\r\n"),
        ("Elsewhere.utl", "1\r\n"),
    ]);

    assert_eq!(
        displayed(&directory::list_loot_profiles(&mut storage).unwrap()),
        strings(&["Keep.utl"])
    );
}

/// 2026-09-21 12:00 UTC, the C# tests' `Now`.
fn now() -> DateTime {
    DateTime::utc_from_unix_millis(1_789_992_000_000)
}

fn trimmed(storage: &mut MemoryStorage, key: &str) -> Option<String> {
    storage
        .read_text(key)
        .unwrap()
        .map(|text| text.trim().to_owned())
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationCopiesTheOldFlatLayoutIntoItsFolders
#[test]
fn migration_copies_the_old_flat_layout_into_its_folders() {
    let mut storage = flat_storage(&[
        ("--Barris_Coldeve.usd", "settings\r\n"),
        ("Coldeve_Barris.cdf", "binding\r\n"),
        ("Keep.utl", "loot\r\n"),
        ("navs/Hunt.af", "route\r\n"),
        ("metas/Buffing.af", "meta\r\n"),
        ("nav_Legacy.af", "flat route\r\n"),
        ("Flat.af", "flat meta\r\n"),
    ]);

    let result = layout_migration::run(&mut storage, now()).unwrap();

    let read = |storage: &mut MemoryStorage, key| trimmed(storage, key);
    assert_eq!(
        read(&mut storage, "mosstank/profiles/--Barris_Coldeve.usd").as_deref(),
        Some("settings")
    );
    assert_eq!(
        read(&mut storage, "mosstank/profiles/Coldeve_Barris.cdf").as_deref(),
        Some("binding")
    );
    assert_eq!(
        read(&mut storage, "mosstank/loot/Keep.utl").as_deref(),
        Some("loot")
    );
    assert_eq!(
        read(&mut storage, "mosstank/navs/Hunt.af").as_deref(),
        Some("route")
    );
    assert_eq!(
        read(&mut storage, "mosstank/metas/Buffing.af").as_deref(),
        Some("meta")
    );
    assert_eq!(
        read(&mut storage, "mosstank/navs/Legacy.af").as_deref(),
        Some("flat route")
    );
    assert_eq!(
        read(&mut storage, "mosstank/metas/Flat.af").as_deref(),
        Some("flat meta")
    );
    assert_eq!(result.total(), 7);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationTellsFlatRoutesFromFlatMetasAndDropsTheRouteMarker
#[test]
fn migration_tells_flat_routes_from_flat_metas_and_drops_the_route_marker() {
    let mut storage = flat_storage(&[
        ("nav_Hunt.af", "named route\r\n"),
        ("--nav_Barris_Coldeve.af", "character route\r\n"),
        ("--Barris_Coldeve.af", "character meta\r\n"),
    ]);

    layout_migration::run(&mut storage, now()).unwrap();

    assert_eq!(
        trimmed(&mut storage, "mosstank/navs/Hunt.af").as_deref(),
        Some("named route")
    );
    assert_eq!(
        trimmed(&mut storage, "mosstank/navs/--Barris_Coldeve.af").as_deref(),
        Some("character route")
    );
    assert_eq!(
        trimmed(&mut storage, "mosstank/metas/--Barris_Coldeve.af").as_deref(),
        Some("character meta")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationNeverRemovesOrRewritesWhatItCopiedFrom
#[test]
fn migration_never_removes_or_rewrites_what_it_copied_from() {
    let mut storage = flat_storage(&[
        ("--Barris_Coldeve.usd", "settings\r\n"),
        ("navs/Hunt.af", "route\r\n"),
    ]);

    layout_migration::run(&mut storage, now()).unwrap();

    assert_eq!(
        trimmed(&mut storage, "--Barris_Coldeve.usd").as_deref(),
        Some("settings")
    );
    assert_eq!(
        trimmed(&mut storage, "navs/Hunt.af").as_deref(),
        Some("route")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationLeavesAnotherToolsFilesAlone
#[test]
fn migration_leaves_another_tools_files_alone() {
    let mut storage = flat_storage(&[
        ("GameInfo.ugd", "database\r\n"),
        ("someone-else/Their.usd", "theirs\r\n"),
        ("Backup.usd.bak", "backup\r\n"),
    ]);

    let result = layout_migration::run(&mut storage, now()).unwrap();

    assert_eq!(result.total(), 0);
    assert!(!storage.keys().any(|key| key.starts_with("mosstank/")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationRunsOnceAndLeavesAlreadyCopiedFilesUntouched
#[test]
fn migration_runs_once_and_leaves_already_copied_files_untouched() {
    let mut storage = flat_storage(&[
        ("Keep.utl", "old\r\n"),
        ("mosstank/loot/Keep.utl", "already here\r\n"),
    ]);

    let first = layout_migration::run(&mut storage, now()).unwrap();
    let second = layout_migration::run(&mut storage, now()).unwrap();

    assert_eq!(first.total(), 0);
    assert_eq!(second.total(), 0);
    assert_eq!(
        trimmed(&mut storage, "mosstank/loot/Keep.utl").as_deref(),
        Some("already here")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationKeepsTheMarkedTwinAndSetsTheOtherAside
#[test]
fn migration_keeps_the_marked_twin_and_sets_the_other_aside() {
    let mut storage = flat_storage(&[
        (
            "--Acdream_sawato.usd",
            "written before the object arrived\r\n",
        ),
        ("--+Acdream_sawato.usd", "written after it arrived\r\n"),
        ("sawato_Acdream.cdf", "early binding\r\n"),
        ("sawato_+Acdream.cdf", "later binding\r\n"),
    ]);

    let result = layout_migration::run(&mut storage, now()).unwrap();

    assert_eq!(
        trimmed(&mut storage, "mosstank/profiles/--Acdream_sawato.usd").as_deref(),
        Some("written after it arrived")
    );
    assert_eq!(
        trimmed(
            &mut storage,
            "mosstank/profiles/--Acdream_sawato.usd.twin-2026-09-21"
        )
        .as_deref(),
        Some("written before the object arrived")
    );
    assert_eq!(
        trimmed(&mut storage, "mosstank/profiles/sawato_Acdream.cdf").as_deref(),
        Some("later binding")
    );
    assert_eq!(result.twins_kept, 2);
    assert!(
        !storage
            .keys()
            .any(|key| key.starts_with("mosstank/") && key.contains('+'))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationNeverReplacesAProfileTheNewLayoutAlreadyHas
#[test]
fn migration_never_replaces_a_profile_the_new_layout_already_has() {
    let mut storage = flat_storage(&[
        (
            "mosstank/profiles/--Mossy_sawato.usd",
            "the live profile\r\n",
        ),
        ("mosstank/profiles/sawato_Mossy.cdf", "the live binding\r\n"),
        ("--+Mossy_sawato.usd", "a stale blank profile\r\n"),
        ("sawato_+Mossy.cdf", "a stale binding\r\n"),
    ]);

    layout_migration::run(&mut storage, now()).unwrap();
    let tomorrow = now().add_ticks(864_000_000_000);
    layout_migration::run(&mut storage, tomorrow).unwrap();

    assert_eq!(
        trimmed(&mut storage, "mosstank/profiles/--Mossy_sawato.usd").as_deref(),
        Some("the live profile")
    );
    assert_eq!(
        trimmed(&mut storage, "mosstank/profiles/sawato_Mossy.cdf").as_deref(),
        Some("the live binding")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankFileLayoutTests.cs :: MigrationSaysWhatItCopiedAndWhere
#[test]
fn migration_says_what_it_copied_and_where() {
    let result = LayoutMigrationResult {
        settings: 2,
        loot: 1,
        navs: 0,
        metas: 0,
        twins_kept: 1,
    };

    let line = result.describe(Some("/data/vtank")).unwrap();

    assert!(line.contains("2 settings"));
    assert!(line.contains("1 loot"));
    assert!(line.contains("mosstank"));
    assert!(line.contains("duplicate"));
    assert_eq!(LayoutMigrationResult::default().describe(None), None);
}

// --- The UB tree --------------------------------------------------------------

fn ub_storage(files: &[(&str, &str)]) -> MemoryStorage {
    flat_storage(files)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbTreeMigrationTests.cs :: EveryFileArrivesAtTheSameRelativePathUnderTheNewRoot
#[test]
fn every_file_arrives_at_the_same_relative_path_under_the_new_root() {
    let mut plugin = ub_storage(&[
        ("ub/settings.json", "installation"),
        ("ub/maps/markers.csv", "Town, Holtburg, 42.1, 33.6"),
        ("ub/Coldeve/Acdream/settings.json", "character"),
        ("ub/Coldeve/Acdream/equip/raid.utl", "rules"),
        ("expressions/global/keep.json", "not UB"),
    ]);
    let mut vtank = ub_storage(&[]);

    let result = ub_tree_migration::run(&mut plugin, &mut vtank).unwrap();

    assert_eq!(result.copied, 4);
    assert_eq!(result.already_present, 0);
    assert_eq!(vtank.get("mosstank/ub/settings.json"), Some("installation"));
    assert_eq!(
        vtank.get("mosstank/ub/maps/markers.csv"),
        Some("Town, Holtburg, 42.1, 33.6")
    );
    assert_eq!(
        vtank.get("mosstank/ub/Coldeve/Acdream/settings.json"),
        Some("character")
    );
    assert_eq!(
        vtank.get("mosstank/ub/Coldeve/Acdream/equip/raid.utl"),
        Some("rules")
    );
    assert!(!vtank.contains("mosstank/ub/expressions/global/keep.json"));
    assert_eq!(vtank.keys().count(), 4);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbTreeMigrationTests.cs :: TheOriginalsAreLeftWhereTheyWere
#[test]
fn the_originals_are_left_where_they_were() {
    let mut plugin = ub_storage(&[("ub/settings.json", "installation")]);

    ub_tree_migration::run(&mut plugin, &mut ub_storage(&[])).unwrap();

    assert_eq!(plugin.get("ub/settings.json"), Some("installation"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbTreeMigrationTests.cs :: AFileAlreadyAtItsNewNameIsLeftAlone
#[test]
fn a_file_already_at_its_new_name_is_left_alone() {
    let mut plugin = ub_storage(&[
        ("ub/settings.json", "old"),
        ("ub/maps/markers.csv", "markers"),
    ]);
    let mut vtank = ub_storage(&[("mosstank/ub/settings.json", "edited since")]);

    let first = ub_tree_migration::run(&mut plugin, &mut vtank).unwrap();

    assert_eq!(first.copied, 1);
    assert_eq!(first.already_present, 1);
    assert_eq!(vtank.get("mosstank/ub/settings.json"), Some("edited since"));

    let second = ub_tree_migration::run(&mut plugin, &mut vtank).unwrap();

    assert_eq!(second.copied, 0);
    assert_eq!(second.already_present, 2);
    assert_eq!(second.describe(Some("C:\\memory")), None);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbTreeMigrationTests.cs :: TheCopyIsAnnouncedInOneLineNamingTheFolder
#[test]
fn the_copy_is_announced_in_one_line_naming_the_folder() {
    let mut plugin = ub_storage(&[("ub/settings.json", "a"), ("ub/maps/markers.csv", "b")]);

    let result = ub_tree_migration::run(&mut plugin, &mut ub_storage(&[])).unwrap();

    // Path.Combine("C:\\memory", "mosstank", "ub") on Windows.
    assert_eq!(
        result.describe(Some("C:\\memory")).unwrap(),
        "MossTank copied 2 UtilityBelt file(s) into C:\\memory\\mosstank\\ub (the originals were left where they were)."
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbTreeMigrationTests.cs :: NothingToCopySaysNothing
#[test]
fn nothing_to_copy_says_nothing() {
    let mut empty = ub_storage(&[]);
    assert_eq!(
        ub_tree_migration::run(&mut empty, &mut ub_storage(&[]))
            .unwrap()
            .describe(Some("C:\\memory")),
        None
    );

    let mut plugin = ub_storage(&[("ub/settings.json", "a")]);
    assert_eq!(
        ub_tree_migration::run(&mut plugin, &mut MemoryStorage::unavailable()).unwrap(),
        UbTreeMigrationResult::default()
    );
    assert_eq!(
        ub_tree_migration::run(&mut MemoryStorage::unavailable(), &mut ub_storage(&[])).unwrap(),
        UbTreeMigrationResult::default()
    );
}
