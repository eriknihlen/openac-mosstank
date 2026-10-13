//! The route store as the navigation tests drive it (`NavigationTests`,
//! the route-profile half): one storage behind both roots, as the C# test
//! host gives.

use mosstank_formats::af;
use mosstank_formats::fixtures;
use mosstank_formats::route::{
    NavigationSettings, Position, RouteJumpDirection, RouteMode, RouteRecallKind, RouteWaypoint,
    RouteWaypointType,
};
use mosstank_stores::directory;
use mosstank_stores::profile_store::ProfileLoad;
use mosstank_stores::route_store::RouteProfileStore;
use mosstank_stores::storage::{
    MemoryStorage, PluginStorage, RecordingLog, Root, StoreHost, StoreLog,
};
use sha2::{Digest, Sha256};

/// The C# `FakeHost` with one `MemoryStorage` for both its storage and its
/// profiles, on a world with no name.
#[derive(Debug)]
struct Host {
    storage: MemoryStorage,
    log: RecordingLog,
}

impl Host {
    fn new() -> Self {
        Self {
            storage: MemoryStorage::ordinal(),
            log: RecordingLog::default(),
        }
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.storage.get(key)
    }
}

impl StoreHost for Host {
    fn storage(&mut self, _root: Root) -> &mut dyn PluginStorage {
        &mut self.storage
    }

    fn log(&mut self) -> &mut dyn StoreLog {
        &mut self.log
    }

    fn world_name(&mut self) -> String {
        String::new()
    }
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn legacy_key(identity: &str) -> String {
    let hash = Sha256::digest(identity.as_bytes());
    format!("profiles/route/{}.json", hex_upper(&hash))
}

fn legacy_route_by_character_key(character_name: &str) -> String {
    legacy_key(&format!("char:{}", character_name.trim().to_uppercase()))
}

fn legacy_route_named_key(name: &str) -> String {
    legacy_key(&format!("named:{}", name.trim().to_uppercase()))
}

fn bound(host: &mut Host, character: &str) -> RouteProfileStore {
    let mut store = RouteProfileStore::new();
    assert!(store.bind_character(host, Some(character)).unwrap());
    store
}

fn load(store: &mut RouteProfileStore, host: &mut Host) -> (ProfileLoad, NavigationSettings) {
    let mut target = NavigationSettings::default();
    let loaded = store.load_current(host, &mut target, None).unwrap();
    (loaded, target)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteProfilesRoundTripWaypointFieldsButLeaveSettingsOwnedFieldsAlone
#[test]
fn route_profiles_round_trip_waypoint_fields_but_leave_settings_owned_fields_alone() {
    let mut host = Host::new();
    let mut source = NavigationSettings::with_mode(RouteMode::Linear);
    source.waypoints.push(RouteWaypoint {
        kind: RouteWaypointType::Jump,
        position: Position {
            cell_id: 0x7F7F_0001,
            east_west: 1.2,
            north_south: -3.4,
            elevation: 5.6,
            heading_degrees: 78.0,
            is_outdoor: true,
        },
        object_id: 88,
        object_name: "Portal".to_owned(),
        text: "/say hello".to_owned(),
        duration_milliseconds: 1234,
        recall: RouteRecallKind::SecondaryPortalRecall,
        jump_heading_degrees: 271.5,
        jump_hold_shift: true,
        jump_charge_milliseconds: 875,
        jump_direction: RouteJumpDirection::StrafeRight,
        ..RouteWaypoint::default()
    });
    let mut first = bound(&mut host, "Test Character");
    first.save_current(&mut host, &source);

    // Values a settings-profile load would already have set: loading the
    // route leaves every one of them alone.
    let mut target = NavigationSettings {
        enabled: false,
        priority: false,
        minimum_distance_meters: 9.0,
        follow_around_corners: true,
        open_doors: false,
        door_identify_range_meters: 11.0,
        door_open_range_meters: 1.0,
        door_lockpick_excess_threshold: -3,
        ..NavigationSettings::default()
    };
    let mut second = bound(&mut host, "Test Character");
    assert_eq!(
        second.load_current(&mut host, &mut target, None).unwrap(),
        ProfileLoad::Loaded
    );

    assert_eq!(target.mode, RouteMode::Linear);
    assert_eq!(target.waypoints.len(), 1);
    let waypoint = &target.waypoints[0];
    assert_eq!(waypoint.kind, RouteWaypointType::Jump);
    assert_eq!(waypoint.jump_heading_degrees, 271.5);
    assert!(waypoint.jump_hold_shift);
    assert_eq!(waypoint.jump_charge_milliseconds, 875);
    // The direction survives the save: it rides the charge field.
    assert_eq!(waypoint.jump_direction, RouteJumpDirection::StrafeRight);

    assert!(!target.enabled);
    assert!(!target.priority);
    assert_eq!(target.minimum_distance_meters, 9.0);
    assert!(target.follow_around_corners);
    assert!(!target.open_doors);
    assert_eq!(target.door_identify_range_meters, 11.0);
    assert_eq!(target.door_open_range_meters, 1.0);
    assert_eq!(target.door_lockpick_excess_threshold, -3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteStoreMigratesLegacyJsonProfileToAfAndDeletesTheJsonKey
#[test]
fn route_store_migrates_legacy_json_profile_to_af_and_deletes_the_json_key() {
    let mut host = Host::new();
    let legacy = legacy_route_by_character_key("Barris");
    host.storage.insert(
        &legacy,
        "{\n  \"Mode\": 1,\n  \"Waypoints\": [\n    { \"Type\": 0, \"EastWest\": 5.0, \"NorthSouth\": 6.0 }\n  ]\n}",
    );

    let mut store = bound(&mut host, "Barris");
    let (loaded, target) = load(&mut store, &mut host);
    assert_eq!(loaded, ProfileLoad::Loaded);

    assert!(!host.storage.contains(&legacy));
    assert_eq!(target.mode, RouteMode::Linear);
    assert_eq!(target.waypoints.len(), 1);
    assert!((target.waypoints[0].position.east_west - 5.0).abs() < 0.0005);
    assert!((target.waypoints[0].position.north_south - 6.0).abs() < 0.0005);

    // Idempotent second run: nothing left to migrate.
    let mut reopened = bound(&mut host, "Barris");
    let (loaded, reloaded) = load(&mut reopened, &mut host);
    assert_eq!(loaded, ProfileLoad::Loaded);
    assert_eq!(reloaded.waypoints.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteStoreLeavesLegacyJsonUntouchedWhenAfCounterpartExists
#[test]
fn route_store_leaves_legacy_json_untouched_when_af_counterpart_exists() {
    let mut host = Host::new();
    let legacy = legacy_route_by_character_key("Barris");
    host.storage
        .insert(&legacy, "{ \"Mode\": 1, \"Waypoints\": [] }");
    let real_key = format!(
        "mosstank/navs/{}",
        directory::auto_character_file_name("Barris", "", "af")
    );
    let mut real = NavigationSettings::with_mode(RouteMode::Circular);
    real.waypoints.push(RouteWaypoint {
        kind: RouteWaypointType::Point,
        position: Position {
            cell_id: 0x0001_0001,
            east_west: 1.0,
            north_south: 2.0,
            elevation: 0.0,
            heading_degrees: 0.0,
            is_outdoor: true,
        },
        ..RouteWaypoint::default()
    });
    host.storage
        .insert(&real_key, &af::save_nav(&real).unwrap());

    let mut store = bound(&mut host, "Barris");
    let (loaded, target) = load(&mut store, &mut host);
    assert_eq!(loaded, ProfileLoad::Loaded);

    assert!(host.storage.contains(&legacy));
    assert_eq!(target.mode, RouteMode::Circular);
    assert_eq!(target.waypoints.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: LegacyJsonRouteMigratesOldRecallOrdinalToTheRightNewKind
#[test]
fn legacy_json_route_migrates_old_recall_ordinal_to_the_right_new_kind() {
    for (legacy_ordinal, expected) in [
        (0, RouteRecallKind::LifestoneRecall),
        (1, RouteRecallKind::Marketplace),
        (2, RouteRecallKind::PrimaryPortalRecall),
        (3, RouteRecallKind::SecondaryPortalRecall),
    ] {
        let mut host = Host::new();
        let legacy = legacy_route_by_character_key("Barris");
        host.storage.insert(
            &legacy,
            &format!(
                "{{\n  \"Mode\": 1,\n  \"Waypoints\": [\n    {{ \"Type\": 2, \"EastWest\": 1.0, \"NorthSouth\": 2.0, \"Recall\": {legacy_ordinal} }}\n  ]\n}}"
            ),
        );

        let mut store = bound(&mut host, "Barris");
        let (loaded, target) = load(&mut store, &mut host);
        assert_eq!(loaded, ProfileLoad::Loaded);

        assert_eq!(target.waypoints.len(), 1);
        assert_eq!(target.waypoints[0].kind, RouteWaypointType::Recall);

        let file_name = format!(
            "mosstank/navs/{}",
            directory::auto_character_file_name("Barris", "", "af")
        );
        let af = host.text(&file_name).expect("the route was written");
        assert!(
            af.contains(&format!("{{{}}}", expected.display_name())),
            "{legacy_ordinal}: {af}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteRosterSweepConvertsEveryNamedLegacyProfileOnce
#[test]
fn route_roster_sweep_converts_every_named_legacy_profile_once() {
    let mut host = Host::new();
    host.storage.insert(
        "profiles/route/index.json",
        "{ \"Names\": [\"Farming\", \"Buffing\"] }",
    );
    host.storage.insert(
        &legacy_route_named_key("Farming"),
        "{ \"Mode\": 1, \"Waypoints\": [ { \"Type\": 0, \"EastWest\": 1.0, \"NorthSouth\": 2.0 } ] }\n",
    );
    host.storage.insert(
        &legacy_route_named_key("Buffing"),
        "{ \"Mode\": 1, \"Waypoints\": [ { \"Type\": 0, \"EastWest\": 3.0, \"NorthSouth\": 4.0 } ] }\n",
    );

    let mut store = bound(&mut host, "Barris");
    let _ = load(&mut store, &mut host);

    assert!(!host.storage.contains(&legacy_route_named_key("Farming")));
    assert!(!host.storage.contains(&legacy_route_named_key("Buffing")));
    assert!(!host.storage.contains("profiles/route/index.json"));
    let mut farming = NavigationSettings::default();
    af::try_load_nav(
        host.text("mosstank/navs/Farming.af").unwrap(),
        &mut farming,
        None,
    )
    .unwrap();
    assert_eq!(farming.waypoints.len(), 1);
    assert!((farming.waypoints[0].position.east_west - 1.0).abs() < 0.0005);
    let mut buffing = NavigationSettings::default();
    af::try_load_nav(
        host.text("mosstank/navs/Buffing.af").unwrap(),
        &mut buffing,
        None,
    )
    .unwrap();
    assert_eq!(buffing.waypoints.len(), 1);
    assert!((buffing.waypoints[0].position.east_west - 3.0).abs() < 0.0005);

    // Idempotent: a fresh store against the same storage sweeps nothing
    // more (there is no roster key left to read).
    let mut reopened = bound(&mut host, "Barris");
    let _ = load(&mut reopened, &mut host);
    assert!(host.storage.contains("mosstank/navs/Farming.af"));
    assert!(host.storage.contains("mosstank/navs/Buffing.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteStoreLeavesNonMarkedFlatAfFilesForTheMetaStore
#[test]
fn route_store_leaves_non_marked_flat_af_files_for_the_meta_store() {
    let mut host = Host::new();
    host.storage.insert("SharedMeta.af", "1\r\n");

    let mut store = bound(&mut host, "Barris");
    let _ = load(&mut store, &mut host);

    assert!(host.storage.contains("SharedMeta.af"));
    assert!(!host.storage.contains("mosstank/navs/SharedMeta.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ANavDroppedIntoTheNavsFolderLoadsAndSavesBackIntoItself
#[test]
fn a_nav_dropped_into_the_navs_folder_loads_and_saves_back_into_itself() {
    let mut host = Host::new();
    host.storage.insert(
        "mosstank/navs/Dropped.nav",
        &fixtures::read_vtank("nav/nav_ab.nav"),
    );
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Barris")).unwrap();

    assert!(
        store
            .available_names(&mut host)
            .unwrap()
            .iter()
            .any(|name| name == "Dropped")
    );
    assert!(store.select(&mut host, Some("Dropped"), false).unwrap());

    let mut route = NavigationSettings::default();
    assert_eq!(
        store.load_current(&mut host, &mut route, None).unwrap(),
        ProfileLoad::Loaded
    );
    assert!(!route.waypoints.is_empty());

    let dropped = host.text("mosstank/navs/Dropped.nav").unwrap().to_owned();
    assert!(store.save_current(&mut host, &route));

    // A route saves back to the file it came from, in that file's own form,
    // and one saved unchanged is the same bytes; nothing is written beside
    // it.
    assert_eq!(
        host.text("mosstank/navs/Dropped.nav"),
        Some(dropped.as_str())
    );
    assert!(!host.storage.contains("mosstank/navs/Dropped.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteStoreRefusesToLoadAMetaOnlyFileWithNoticeNamingMetasFolder
#[test]
fn route_store_refuses_to_load_a_meta_only_file_with_notice_naming_metas_folder() {
    let mut host = Host::new();
    host.storage.insert(
        "mosstank/navs/Misplaced.af",
        &fixtures::read_vtank("af/bella.af"),
    );

    let mut store = bound(&mut host, "Barris");
    assert!(store.select(&mut host, Some("Misplaced"), false).unwrap());

    let (loaded, _) = load(&mut store, &mut host);

    assert_eq!(loaded, ProfileLoad::Failed);
    let notice = store.recovery_notice().expect("a notice");
    assert!(notice.contains("mosstank/metas/"), "{notice}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: FollowModeRouteRoundTripsTheFollowTargetThroughAf
#[test]
fn follow_mode_route_round_trips_the_follow_target_through_af() {
    let mut host = Host::new();
    let source = NavigationSettings {
        mode: RouteMode::Target,
        follow_target_object_id: 99,
        follow_target_name: "Leader".to_owned(),
        ..NavigationSettings::default()
    };
    let mut first = bound(&mut host, "Test Character");
    first.save_current(&mut host, &source);

    let mut second = bound(&mut host, "Test Character");
    let (loaded, target) = load(&mut second, &mut host);
    assert_eq!(loaded, ProfileLoad::Loaded);

    assert_eq!(target.mode, RouteMode::Target);
    assert_eq!(target.follow_target_object_id, 99);
    assert_eq!(target.follow_target_name, "Leader");
}
