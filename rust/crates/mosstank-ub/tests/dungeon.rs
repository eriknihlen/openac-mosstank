//! The dungeon map's model and its controller, against
//! `DungeonObjectTrackerTests`, `DungeonMapPictureTests` and
//! `DungeonMapControllerTests`.

use std::collections::HashMap;

use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::{
    DungeonCell, DungeonLayer, DungeonWall, Floorplan, ObjectClass, Position, WorldObject,
};
use mosstank_stores::storage::PluginStorage;
use mosstank_ub::dungeon::camera::{DungeonMapCamera, MAX_SCALE};
use mosstank_ub::dungeon::controller::{
    CANVAS_HEIGHT, CANVAS_ID, CANVAS_WIDTH, DungeonMapController, DungeonMapSettings,
    DungeonMapSettingsSource, MapCanvasSpec, MapImage, MapPainter, MapUi, PAN_STEP_PIXELS,
    SETTINGS_POLL_SECONDS, WORLD_READ_SECONDS,
};
use mosstank_ub::dungeon::markers::{self, DungeonMarkerKind};
use mosstank_ub::dungeon::picture::{DungeonMapLine, DungeonMapPicture, DungeonMapRect};
use mosstank_ub::dungeon::shade::DungeonLayerShade;
use mosstank_ub::dungeon::tracker::{DungeonObjectTracker, UPDATE_DISTANCE_METERS};
use mosstank_ub::dungeon::visited::DungeonVisitedCells;
use mosstank_ub::dungeon::{Vec2, Vec3, to_landblock_local};
use mosstank_ub::testing::{FakeUbHost, SharedStorage};

const LANDBLOCK: u32 = 0x01A9_0000;
const SELF_ID: u32 = 0x5000_0001;
const SELF_NAME: &str = "Acdream";

fn close(expected: f64, actual: f64, places: i32) -> bool {
    let tolerance = 0.5 * 10f64.powi(-places);
    (expected - actual).abs() <= tolerance
}

macro_rules! assert_close {
    ($expected:expr, $actual:expr, $places:expr) => {{
        let (expected, actual) = (f64::from($expected), f64::from($actual));
        assert!(
            close(expected, actual, $places),
            "expected {expected}, got {actual}"
        );
    }};
}

/// A navigation position that lands on the given landblock-local metres:
/// the inverse of the plan's own conversion.
fn at(landblock: u32, x: f32, y: f32, z: f32, heading: f32) -> Position {
    let lbx = f64::from((landblock >> 24) & 0xFF);
    let lby = f64::from((landblock >> 16) & 0xFF);
    Position {
        cell_id: landblock | 0x100,
        east_west: (lbx * 192.0 + f64::from(x) - 24468.0) / 240.0,
        north_south: (lby * 192.0 + f64::from(y) - 24468.0) / 240.0,
        elevation: f64::from(z) / 240.0,
        heading_degrees: heading,
        is_outdoor: false,
    }
}

fn object(
    id: u32,
    name: &str,
    object_class: ObjectClass,
    position: Option<Position>,
    wielder: u32,
    container: u32,
) -> WorldObject {
    WorldObject {
        object_id: id,
        name: name.to_owned(),
        object_class,
        container_id: container,
        wielder_id: wielder,
        position,
        icon_id: 0x0600_1000_u32.wrapping_add(id),
        ..WorldObject::default()
    }
}

fn placed(id: u32, name: &str, object_class: ObjectClass, position: Position) -> WorldObject {
    object(id, name, object_class, Some(position), 0, 0)
}

/// Two ten-metre rooms side by side on the ground storey, a third room
/// north of them one storey up, and the walls around the west room.
fn two_rooms() -> Floorplan {
    let west = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let east = vec![(10.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0)];
    let upper = vec![(0.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0)];
    Floorplan {
        landblock_id: LANDBLOCK,
        layers: vec![
            DungeonLayer {
                z: 0.0,
                floors: vec![west, east],
                walls: vec![
                    DungeonWall {
                        start: (0.0, 0.0),
                        end: (0.0, 10.0),
                    },
                    DungeonWall {
                        start: (0.0, 10.0),
                        end: (10.0, 10.0),
                    },
                ],
            },
            DungeonLayer {
                z: 6.0,
                floors: vec![upper],
                walls: Vec::new(),
            },
        ],
        cells: vec![
            DungeonCell {
                cell_id: LANDBLOCK | 0x100,
                center: (5.0, 5.0, 0.0),
                layer_z: 0.0,
            },
            DungeonCell {
                cell_id: LANDBLOCK | 0x101,
                center: (15.0, 5.0, 0.0),
                layer_z: 0.0,
            },
            DungeonCell {
                cell_id: LANDBLOCK | 0x102,
                center: (5.0, 15.0, 6.0),
                layer_z: 6.0,
            },
        ],
        bounds_min: (0.0, 0.0, 0.0),
        bounds_max: (20.0, 20.0, 6.0),
    }
}

// ---- DungeonObjectTrackerTests ------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: TheTestPositionHelperRoundTripsThroughThePlan
#[test]
fn the_test_position_helper_round_trips_through_the_plan() {
    let local = to_landblock_local(&at(LANDBLOCK, 15.0, 5.0, 2.0, 0.0));
    assert_close!(15.0, local.x, 3);
    assert_close!(5.0, local.y, 3);
    assert_close!(2.0, local.z, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: ObjectsFallIntoTheMarkerGroups
#[test]
fn objects_fall_into_the_marker_groups() {
    for (class, id, name, expected) in [
        (
            ObjectClass::Player,
            SELF_ID,
            "Acdream",
            DungeonMarkerKind::You,
        ),
        (
            ObjectClass::Player,
            0x5000_0002,
            "Horan",
            DungeonMarkerKind::Others,
        ),
        (
            ObjectClass::Monster,
            0x8000_0001,
            "Drudge",
            DungeonMarkerKind::Monsters,
        ),
        (
            ObjectClass::Npc,
            0x8000_0002,
            "Guard",
            DungeonMarkerKind::NPCs,
        ),
        (
            ObjectClass::Vendor,
            0x8000_0003,
            "Smith",
            DungeonMarkerKind::NPCs,
        ),
        (
            ObjectClass::Portal,
            0x8000_0004,
            "Portal to Holtburg",
            DungeonMarkerKind::Portals,
        ),
        (
            ObjectClass::Corpse,
            0x8000_0005,
            "Corpse of Acdream",
            DungeonMarkerKind::MyCorpse,
        ),
        (
            ObjectClass::Corpse,
            0x8000_0006,
            "Corpse of Drudge",
            DungeonMarkerKind::OtherCorpses,
        ),
        (
            ObjectClass::Door,
            0x8000_0007,
            "Door",
            DungeonMarkerKind::Doors,
        ),
        (
            ObjectClass::Container,
            0x8000_0008,
            "Chest",
            DungeonMarkerKind::Containers,
        ),
        (
            ObjectClass::Armor,
            0x8000_0009,
            "Breastplate",
            DungeonMarkerKind::Items,
        ),
        (
            ObjectClass::Misc,
            0x8000_000A,
            "Pyreal Mote",
            DungeonMarkerKind::Items,
        ),
        (
            ObjectClass::Lifestone,
            0x8000_000B,
            "Lifestone",
            DungeonMarkerKind::EverythingElse,
        ),
        (
            ObjectClass::Unknown,
            0x8000_000C,
            "Thing",
            DungeonMarkerKind::EverythingElse,
        ),
    ] {
        let subject = placed(id, name, class, at(LANDBLOCK, 5.0, 5.0, 0.0, 0.0));
        assert_eq!(
            expected,
            markers::classify(&subject, SELF_ID, SELF_NAME),
            "{name}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: GroupNamesMatchTheSettingsRows
#[test]
fn group_names_match_the_settings_rows() {
    assert_eq!("NPCs", markers::group_name(DungeonMarkerKind::NPCs));
    assert_eq!("MyCorpse", markers::group_name(DungeonMarkerKind::MyCorpse));
    assert_eq!(
        "OtherCorpses",
        markers::group_name(DungeonMarkerKind::OtherCorpses)
    );
    assert_eq!(
        "EverythingElse",
        markers::group_name(DungeonMarkerKind::EverythingElse)
    );
    assert_eq!("You", markers::group_name(DungeonMarkerKind::You));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: PortalAndNpcLabelsLoseThePortalWords
#[test]
fn portal_and_npc_labels_lose_the_portal_words() {
    for (class, name, label) in [
        (ObjectClass::Portal, "Portal to Holtburg", "Holtburg"),
        (ObjectClass::Portal, "Holtburg Portal", "Holtburg"),
        (ObjectClass::Npc, "Portal to Somewhere", "Somewhere"),
        (
            ObjectClass::Monster,
            "Portal to Somewhere",
            "Portal to Somewhere",
        ),
    ] {
        let subject = placed(1, name, class, at(LANDBLOCK, 5.0, 5.0, 0.0, 0.0));
        assert_eq!(label, markers::label_of(&subject));
    }
}

fn tracked(
    tracker: &DungeonObjectTracker,
    id: u32,
) -> mosstank_ub::dungeon::tracker::DungeonTrackedObject {
    let found: Vec<_> = tracker.objects().filter(|o| o.id == id).cloned().collect();
    assert_eq!(1, found.len());
    found[0].clone()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: ObjectsArePlacedOnTheStoreyTheirHeightFallsIn
#[test]
fn objects_are_placed_on_the_storey_their_height_falls_in() {
    let mut tracker = DungeonObjectTracker::new();
    let plan = two_rooms();
    let drudge = placed(
        0x8000_0001,
        "Drudge",
        ObjectClass::Monster,
        at(LANDBLOCK, 5.0, 15.0, 5.0, 0.0),
    );
    let chest = placed(
        0x8000_0002,
        "Chest",
        ObjectClass::Container,
        at(LANDBLOCK, 15.0, 5.0, 2.0, 0.0),
    );
    let own = at(LANDBLOCK, 5.0, 5.0, 0.0, 90.0);

    let changed = tracker.update(
        &[drudge.clone(), chest.clone()],
        &plan,
        &own,
        SELF_ID,
        SELF_NAME,
    );

    assert!(changed);
    assert_eq!(3, tracker.len());
    let you: Vec<_> = tracker
        .objects()
        .filter(|o| o.kind == DungeonMarkerKind::You)
        .cloned()
        .collect();
    assert_eq!(1, you.len());
    assert_eq!(SELF_ID, you[0].id);
    assert_eq!(Vec3::new(5.0, 5.0, 0.0), you[0].position);
    assert_eq!(0.0, you[0].layer_z);
    assert!(you[0].is_mover);
    let monster = tracked(&tracker, 0x8000_0001);
    assert_eq!(6.0, monster.layer_z);
    assert!(monster.is_mover);
    assert_eq!(0x0600_1000_u32.wrapping_add(0x8000_0001), monster.icon_id);
    let container = tracked(&tracker, 0x8000_0002);
    assert_eq!(0.0, container.layer_z);
    assert!(!container.is_mover);

    assert!(!tracker.update(&[drudge, chest], &plan, &own, SELF_ID, SELF_NAME));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: MoversRePlacePastTheThresholdAndStaticsKeepTheirFirstPlace
#[test]
fn movers_re_place_past_the_threshold_and_statics_keep_their_first_place() {
    let mut tracker = DungeonObjectTracker::new();
    let plan = two_rooms();
    let own = at(LANDBLOCK, 5.0, 5.0, 0.0, 0.0);
    let drudge = placed(
        0x8000_0001,
        "Drudge",
        ObjectClass::Monster,
        at(LANDBLOCK, 15.0, 5.0, 0.0, 0.0),
    );
    let chest = placed(
        0x8000_0002,
        "Chest",
        ObjectClass::Container,
        at(LANDBLOCK, 15.0, 8.0, 0.0, 0.0),
    );
    tracker.update(
        &[drudge.clone(), chest.clone()],
        &plan,
        &own,
        SELF_ID,
        SELF_NAME,
    );

    let mut nudged = drudge.clone();
    nudged.position = Some(at(LANDBLOCK, 15.05, 5.0, 0.0, 0.0));
    assert!(!tracker.update(&[nudged, chest.clone()], &plan, &own, SELF_ID, SELF_NAME));
    assert_close!(15.0, tracked(&tracker, drudge.object_id).position.x, 3);

    let mut walked = drudge.clone();
    walked.position = Some(at(LANDBLOCK, 15.2, 5.0, 0.0, 0.0));
    assert!(tracker.update(
        &[walked.clone(), chest.clone()],
        &plan,
        &own,
        SELF_ID,
        SELF_NAME
    ));
    assert_close!(15.2, tracked(&tracker, drudge.object_id).position.x, 3);

    let mut drifted = chest.clone();
    drifted.position = Some(at(LANDBLOCK, 5.0, 15.0, 6.0, 0.0));
    assert!(!tracker.update(
        &[walked.clone(), drifted.clone()],
        &plan,
        &own,
        SELF_ID,
        SELF_NAME
    ));
    assert_eq!(
        Vec3::new(15.0, 8.0, 0.0),
        tracked(&tracker, chest.object_id).position
    );

    assert!(tracker.update(
        &[walked, drifted],
        &plan,
        &at(LANDBLOCK, 5.0, 5.2, 0.0, 0.0),
        SELF_ID,
        SELF_NAME
    ));
    assert_close!(0.104, UPDATE_DISTANCE_METERS, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: CarriedUnplacedAndForeignObjectsAreNotTrackedAndDeparturesAreAChange
#[test]
fn carried_unplaced_and_foreign_objects_are_not_tracked_and_departures_are_a_change() {
    let mut tracker = DungeonObjectTracker::new();
    let plan = two_rooms();
    let own = at(LANDBLOCK, 5.0, 5.0, 0.0, 0.0);
    let here_position = at(LANDBLOCK, 5.0, 5.0, 0.0, 0.0);
    let wielded = object(
        1,
        "Sword",
        ObjectClass::MeleeWeapon,
        Some(here_position.clone()),
        SELF_ID,
        0,
    );
    let packed = object(
        2,
        "Gem",
        ObjectClass::Gem,
        Some(here_position.clone()),
        0,
        SELF_ID,
    );
    let nowhere = object(3, "Ghost", ObjectClass::Monster, None, 0, 0);
    let abroad = placed(
        4,
        "Drudge",
        ObjectClass::Monster,
        at(0x01AA_0000, 5.0, 5.0, 0.0, 0.0),
    );
    let me = placed(SELF_ID, SELF_NAME, ObjectClass::Player, here_position);
    let here = placed(
        5,
        "Chest",
        ObjectClass::Container,
        at(LANDBLOCK, 15.0, 5.0, 0.0, 0.0),
    );

    tracker.update(
        &[wielded, packed, nowhere, abroad, me, here],
        &plan,
        &own,
        SELF_ID,
        SELF_NAME,
    );

    let mut ids: Vec<u32> = tracker.objects().map(|o| o.id).collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(vec![SELF_ID, 5], ids);
    assert!(tracker.update(&[], &plan, &own, SELF_ID, SELF_NAME));
    let ids: Vec<u32> = tracker.objects().map(|o| o.id).collect();
    assert_eq!(vec![SELF_ID], ids);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: VisitedCellsAreKeptPerLandblockAcrossSessions
#[test]
fn visited_cells_are_kept_per_landblock_across_sessions() {
    let mut storage = SharedStorage::new();
    let mut visited = DungeonVisitedCells::new();
    visited.bind(&mut storage, LANDBLOCK);

    assert!(visited.mark(&mut storage, LANDBLOCK | 0x100).unwrap());
    assert!(!visited.mark(&mut storage, LANDBLOCK | 0x100).unwrap());
    assert!(visited.mark(&mut storage, LANDBLOCK | 0x101).unwrap());
    assert!(visited.contains(LANDBLOCK | 0x101));
    assert!(!visited.contains(LANDBLOCK | 0x102));
    assert_eq!(2, visited.len());
    assert!(
        storage
            .keys()
            .iter()
            .any(|key| key.starts_with("mosstank/ub/") && key.contains("01A9"))
    );

    let mut again = DungeonVisitedCells::new();
    again.bind(&mut storage, LANDBLOCK);
    assert!(again.contains(LANDBLOCK | 0x100));
    assert!(again.contains(LANDBLOCK | 0x101));
    assert_eq!(2, again.len());

    again.bind(&mut storage, 0x01AA_0000);
    assert_eq!(0, again.len());
    assert!(!again.mark(&mut storage, LANDBLOCK | 0x100).unwrap());
    assert_eq!(0, again.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonObjectTrackerTests.cs :: ADamagedVisitedFileReadsAsEmpty
#[test]
fn a_damaged_visited_file_reads_as_empty() {
    let mut storage = SharedStorage::new();
    storage
        .write_text(
            "mosstank/ub/dungeonmaps/visited/01A9.txt",
            "not hex\n\n01A90100\n",
        )
        .unwrap();
    let mut visited = DungeonVisitedCells::new();
    visited.bind(&mut storage, LANDBLOCK);

    assert_eq!(1, visited.len());
    assert!(visited.contains(LANDBLOCK | 0x100));
    assert!(visited.mark(&mut storage, LANDBLOCK | 0x101).unwrap());
}

// ---- DungeonMapPictureTests ---------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: FloorsRasteriseToOneRectanglePerCellAtFivePixelsPerMetre
#[test]
fn floors_rasterise_to_one_rectangle_per_cell_at_five_pixels_per_metre() {
    let picture = DungeonMapPicture::build(&two_rooms());

    assert!(!picture.is_empty());
    assert_eq!(100, picture.width);
    assert_eq!(100, picture.height);
    assert_eq!(2, picture.layers.len());

    let ground = &picture.layers[0];
    assert_eq!(0.0, ground.z);
    assert_eq!(2, ground.floors.len());
    let rect = |x, y, width, height| DungeonMapRect {
        x,
        y,
        width,
        height,
    };
    assert!(
        ground
            .floors
            .iter()
            .any(|run| run.rect == rect(0, 50, 50, 50) && run.cell_index == 0)
    );
    assert!(
        ground
            .floors
            .iter()
            .any(|run| run.rect == rect(50, 50, 50, 50) && run.cell_index == 1)
    );

    let first = &picture.layers[1];
    assert_eq!(6.0, first.z);
    assert_eq!(1, first.floors.len());
    assert_eq!(rect(0, 0, 50, 50), first.floors[0].rect);
    assert_eq!(2, first.floors[0].cell_index);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: WallsAreMappedNorthUpWithEastToTheRight
#[test]
fn walls_are_mapped_north_up_with_east_to_the_right() {
    let picture = DungeonMapPicture::build(&two_rooms());

    let line = |x1, y1, x2, y2| DungeonMapLine { x1, y1, x2, y2 };
    assert_eq!(line(0.0, 100.0, 0.0, 50.0), picture.layers[0].walls[0]);
    assert_eq!(line(0.0, 50.0, 50.0, 50.0), picture.layers[0].walls[1]);

    assert_eq!(
        Vec2::new(75.0, 75.0),
        picture.map_pixel_from_local(Vec3::new(15.0, 5.0, 0.0))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: ThePixelOriginIsTheNorthWestCornerOfTheBounds
#[test]
fn the_pixel_origin_is_the_north_west_corner_of_the_bounds() {
    let room = vec![(-5.0, 3.0), (5.0, 3.0), (5.0, 23.0), (-5.0, 23.0)];
    let plan = Floorplan {
        landblock_id: LANDBLOCK,
        layers: vec![DungeonLayer {
            z: 0.0,
            floors: vec![room],
            walls: Vec::new(),
        }],
        cells: vec![DungeonCell {
            cell_id: LANDBLOCK | 0x100,
            center: (0.0, 13.0, 0.0),
            layer_z: 0.0,
        }],
        bounds_min: (-5.0, 3.0, 0.0),
        bounds_max: (5.0, 23.0, 0.0),
    };

    let picture = DungeonMapPicture::build(&plan);

    assert_eq!(
        Vec2::new(0.0, 0.0),
        picture.map_pixel_from_local(Vec3::new(-5.0, 23.0, 0.0))
    );
    assert_eq!(
        Vec2::new(50.0, 100.0),
        picture.map_pixel_from_local(Vec3::new(5.0, 3.0, 0.0))
    );
    assert_eq!(50, picture.width);
    assert_eq!(100, picture.height);
    assert_eq!(Some(0), picture.find_cell_index(LANDBLOCK | 0x100));
    assert_eq!(None, picture.find_cell_index(LANDBLOCK | 0x101));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: TheEmptyPlanIsAnEmptyPicture
#[test]
fn the_empty_plan_is_an_empty_picture() {
    let picture = DungeonMapPicture::build(&Floorplan::default());

    assert!(picture.is_empty());
    assert!(picture.layers.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: StoreysAreSkippedWashedOrDimmedByHeightAboveThePlayer
#[test]
fn storeys_are_skipped_washed_or_dimmed_by_height_above_the_player() {
    for (layer_z, visible, brightness, opacity) in [
        (12.0, false, 0.0, 0.0),
        (6.0, true, 151.0 / 255.0, 121.0 / 255.0),
        (0.0, true, 1.0, 1.0),
        (-6.0, true, 0.6, 1.0),
        (-15.0, true, 0.0, 1.0),
        (-24.0, true, 0.0, 1.0),
        (-30.0, false, 0.0, 0.0),
    ] {
        let shade = DungeonLayerShade::of(0.0, layer_z, false);

        assert_eq!(visible, shade.visible, "{layer_z}");
        if visible {
            assert_close!(brightness, shade.brightness, 3);
            assert_close!(opacity, shade.opacity, 3);
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: TheCurrentStoreyIsTheSixMetreBandAroundTheDrawHeight
#[test]
fn the_current_storey_is_the_six_metre_band_around_the_draw_height() {
    assert_eq!(1.0, DungeonLayerShade::of(2.9, 0.0, false).brightness);
    assert_eq!(1.0, DungeonLayerShade::of(-2.9, 0.0, false).brightness);
    assert_ne!(1.0, DungeonLayerShade::of(3.5, 0.0, false).brightness);
    assert_ne!(1.0, DungeonLayerShade::of(-3.5, 0.0, false).opacity);
    assert_eq!(6.0, DungeonLayerShade::band_of(5.0));
    assert_eq!(0.0, DungeonLayerShade::band_of(2.9));
    assert_eq!(-6.0, DungeonLayerShade::band_of(-3.1));

    // The live Town Network entry: standing at -5.995 the storey at -6 is
    // current and plain, the hall at 0 is the storey above.
    assert_eq!(-6.0, DungeonLayerShade::band_of(-5.995));
    assert_eq!(
        DungeonLayerShade::PLAIN,
        DungeonLayerShade::of(-5.995, -6.0, false)
    );
    let hall = DungeonLayerShade::of(-5.995, 0.0, false);
    assert!(hall.visible);
    assert_close!(151.0 / 255.0, hall.brightness, 3);
    assert_close!(121.0 / 255.0, hall.opacity, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: ShowAllLayersDrawsEveryStoreyUntinted
#[test]
fn show_all_layers_draws_every_storey_untinted() {
    for z in [40.0, 6.0, 0.0, -30.0] {
        let shade = DungeonLayerShade::of(0.0, z, true);
        assert!(shade.visible);
        assert_eq!(1.0, shade.brightness);
        assert_eq!(1.0, shade.opacity);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapPictureTests.cs :: TheHeightSliderSpansTheDungeonsDepth
#[test]
fn the_height_slider_spans_the_dungeons_depth() {
    assert_eq!(-12.0, DungeonLayerShade::draw_z_from_slider(-12.0, 6.0, 0));
    assert_eq!(-3.0, DungeonLayerShade::draw_z_from_slider(-12.0, 6.0, 50));
    assert_eq!(6.0, DungeonLayerShade::draw_z_from_slider(-12.0, 6.0, 100));
    assert_eq!(50, DungeonLayerShade::slider_from_draw_z(-12.0, 6.0, -3.0));
    assert_eq!(0, DungeonLayerShade::slider_from_draw_z(0.0, 0.0, 5.0));
}

// ---- DungeonMapControllerTests: the camera ----------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: FollowingCentresOnThePlayerAndTurnsWhatIsAheadToTheTop
#[test]
fn following_centres_on_the_player_and_turns_what_is_ahead_to_the_top() {
    let picture = DungeonMapPicture::build(&two_rooms());
    let mut camera = DungeonMapCamera::new();
    camera.set_scale(1.0);

    camera.follow(Vec2::new(5.0, 5.0), 0.0);
    let north = camera.screen_from_local(&picture, Vec3::new(5.0, 15.0, 0.0), 200.0, 200.0);
    let east = camera.screen_from_local(&picture, Vec3::new(15.0, 5.0, 0.0), 200.0, 200.0);
    assert_close!(100.0, north.x, 3);
    assert_close!(50.0, north.y, 3);
    assert_close!(150.0, east.x, 3);
    assert_close!(100.0, east.y, 3);

    camera.follow(Vec2::new(5.0, 5.0), 90.0);
    let east = camera.screen_from_local(&picture, Vec3::new(15.0, 5.0, 0.0), 200.0, 200.0);
    let north = camera.screen_from_local(&picture, Vec3::new(5.0, 15.0, 0.0), 200.0, 200.0);
    assert_close!(100.0, east.x, 3);
    assert_close!(50.0, east.y, 3);
    assert_close!(50.0, north.x, 3);
    assert_close!(100.0, north.y, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheScaleIsClampedAndTheSettingIsFiveMinusIt
#[test]
fn the_scale_is_clamped_and_the_setting_is_five_minus_it() {
    let mut camera = DungeonMapCamera::new();
    camera.set_scale(10.0);
    assert_eq!(5.0, camera.scale());
    camera.set_scale(0.1);
    assert_eq!(0.3, camera.scale());
    camera.set_scale(4.9);
    camera.zoom_in();
    assert_eq!(5.0, camera.scale());
    camera.zoom_out();
    assert_close!(4.8, camera.scale(), 3);
    assert_close!(0.8, DungeonMapCamera::scale_from_setting(4.2), 3);
    assert_close!(4.2, DungeonMapCamera::setting_from_scale(0.8), 3);
    assert_close!(0.3, DungeonMapCamera::scale_from_setting(9.0), 3);

    // The shipped zoom is four screen pixels to the metre: a ten-metre
    // cell is forty pixels wide.
    let shipped = DungeonMapCamera::new();
    let west = shipped.screen_from_map(Vec2::new(0.0, 0.0), Vec2::new(25.0, 25.0), 320.0, 280.0);
    let east = shipped.screen_from_map(Vec2::new(50.0, 0.0), Vec2::new(25.0, 25.0), 320.0, 280.0);
    assert_close!(40.0, east.x - west.x, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: PanningMovesTheMiddleAgainstTheDragThroughTheRotationAndStopsFollowing
#[test]
fn panning_moves_the_middle_against_the_drag_through_the_rotation_and_stops_following() {
    let mut camera = DungeonMapCamera::new();
    camera.set_scale(1.0);
    camera.follow(Vec2::new(5.0, 5.0), 0.0);

    camera.pan_by(50.0, 0.0);
    assert!(!camera.following());
    assert_close!(-5.0, camera.center_meters().x, 3);
    assert_close!(5.0, camera.center_meters().y, 3);
    camera.pan_by(0.0, 50.0);
    assert_close!(15.0, camera.center_meters().y, 3);

    camera.resume_following();
    camera.follow(Vec2::new(5.0, 5.0), 90.0);
    assert_eq!(Vec2::new(5.0, 5.0), camera.center_meters());
    camera.pan_by(50.0, 0.0);
    assert_close!(5.0, camera.center_meters().x, 3);
    assert_close!(15.0, camera.center_meters().y, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: RotatingByDragIsADegreePerPixelAndStopsFollowing
#[test]
fn rotating_by_drag_is_a_degree_per_pixel_and_stops_following() {
    let mut camera = DungeonMapCamera::new();
    camera.follow(Vec2::default(), 30.0);
    camera.rotate_by_drag(-10.0, -5.0);
    assert!(!camera.following());
    assert_close!(45.0, camera.rotation_degrees(), 3);
    camera.follow(Vec2::default(), 90.0);
    assert_close!(45.0, camera.rotation_degrees(), 3);
    camera.rotate_by(-50.0);
    assert_close!(355.0, camera.rotation_degrees(), 3);
}

// ---- DungeonMapControllerTests: the controller ----------------------------------

#[derive(Debug, Clone)]
struct FakeCanvas {
    handle: u32,
    spec: MapCanvasSpec,
    visible: bool,
    invalidations: usize,
    disposed: bool,
}

#[derive(Debug, Default)]
struct FakeUi {
    canvases: Vec<FakeCanvas>,
    object_icons: HashMap<u32, MapImage>,
    released: usize,
}

impl MapUi for FakeUi {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        let handle = u32::try_from(self.canvases.len()).unwrap() + 1;
        self.canvases.push(FakeCanvas {
            handle,
            spec: spec.clone(),
            visible: spec.start_visible,
            invalidations: 0,
            disposed: false,
        });
        Some(handle)
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvas_mut(canvas).visible = visible;
    }

    fn invalidate_canvas(&mut self, canvas: u32) {
        self.canvas_mut(canvas).invalidations += 1;
    }

    fn remove_canvas(&mut self, canvas: u32) {
        self.canvas_mut(canvas).disposed = true;
    }

    fn object_icon(&mut self, object_id: u32) -> MapImage {
        self.object_icons
            .get(&object_id)
            .copied()
            .unwrap_or_default()
    }

    fn release_image(&mut self, _image: MapImage) {
        self.released += 1;
    }
}

impl FakeUi {
    fn canvas_mut(&mut self, handle: u32) -> &mut FakeCanvas {
        self.canvases
            .iter_mut()
            .find(|canvas| canvas.handle == handle)
            .unwrap()
    }
}

/// One line painted: its ends, colour and thickness.
type PaintedLine = ((f64, f64), (f64, f64), Color, f32);

/// Records every line with its ends, colour and thickness, and every text
/// with its place.
#[derive(Debug, Default)]
struct Painter {
    lines: Vec<PaintedLine>,
    texts: Vec<(String, (f64, f64), Color)>,
    images: Vec<(MapImage, (f64, f64, f64, f64))>,
    clip_depth: i32,
}

impl MapPainter for Painter {
    fn width(&self) -> i32 {
        CANVAS_WIDTH
    }

    fn height(&self) -> i32 {
        CANVAS_HEIGHT
    }

    fn clear(&mut self, _color: &Color) {}

    fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), color: &Color, thickness: f32) {
        self.lines.push((from, to, color.clone(), thickness));
    }

    fn draw_text(&mut self, text: &str, at: (f64, f64), color: &Color, _outline: bool) {
        self.texts.push((text.to_owned(), at, color.clone()));
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        (text.len() as f64 * 6.0, 12.0)
    }

    fn draw_image(&mut self, image: MapImage, rect: (f64, f64, f64, f64), _tint: &Color) {
        self.images.push((image, rect));
    }

    fn push_clip(&mut self, _rect: (f64, f64, f64, f64)) {
        self.clip_depth += 1;
    }

    fn pop_clip(&mut self) {
        self.clip_depth -= 1;
    }
}

#[derive(Debug, Default)]
struct Settings {
    current: DungeonMapSettings,
    saved_zoom: Vec<f32>,
}

impl DungeonMapSettingsSource for Settings {
    fn read(&self) -> DungeonMapSettings {
        self.current.clone()
    }

    fn save_map_zoom(&mut self, zoom: f32) {
        self.saved_zoom.push(zoom);
        self.current.map_zoom = zoom;
    }
}

struct Rig {
    host: FakeUbHost,
    ui: FakeUi,
    settings: Settings,
    storage: SharedStorage,
    controller: DungeonMapController,
}

fn snapshot_at(x: f32, y: f32, z: f32, heading: f32) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: SELF_ID,
        position: at(LANDBLOCK, x, y, z, heading),
        ..NavigationSnapshot::default()
    }
}

impl Rig {
    fn new() -> Self {
        let mut host = FakeUbHost::new();
        host.all_sealed = true;
        host.floorplans.insert(LANDBLOCK, two_rooms());
        let storage = SharedStorage::new();
        host.meta.storage = storage.clone();
        host.meta.navigation = snapshot_at(5.0, 5.0, 0.0, 0.0);
        {
            let character = host.meta.character_mut();
            character.id = SELF_ID;
            SELF_NAME.clone_into(&mut character.name);
        }
        let settings = Settings::default();
        let controller = DungeonMapController::new(&settings);
        Self {
            host,
            ui: FakeUi::default(),
            settings,
            storage,
            controller,
        }
    }

    fn set_plan(&mut self, plan: Floorplan) {
        self.host.floorplans.insert(LANDBLOCK, plan);
    }

    fn move_to(&mut self, x: f32, y: f32, z: f32, heading: f32) {
        self.host.meta.navigation = snapshot_at(x, y, z, heading);
    }

    fn tick_by(&mut self, seconds: f64) {
        self.controller
            .on_tick(&mut self.host, &self.settings, &mut self.ui, seconds);
    }

    /// One whole frame: past the world-read interval, so the world is read.
    fn tick(&mut self) {
        self.tick_by(WORLD_READ_SECONDS + 0.001);
    }

    fn canvas(&self) -> &FakeCanvas {
        assert_eq!(1, self.ui.canvases.len());
        &self.ui.canvases[0]
    }

    fn paint(&mut self) -> Painter {
        let mut painter = Painter::default();
        let player = self.host.meta.navigation.clone();
        self.controller
            .paint(&mut painter, &mut self.ui, Some(&player));
        assert_eq!(0, painter.clip_depth);
        painter
    }

    fn pointer(&mut self, kind: u8, x: f64, y: f64, button: u8, modifiers: u8, wheel: i32) {
        assert!(self.canvas().spec.accepts_pointer_input);
        #[allow(clippy::cast_possible_truncation)]
        let event = PointerEvent {
            canvas: self.canvas().handle,
            kind,
            x: x as f32,
            y: y as f32,
            button,
            modifiers,
            wheel_delta: wheel,
        };
        self.controller.on_pointer(&mut self.settings, &event);
    }

    fn down(&mut self, x: f64, y: f64) {
        self.pointer(0, x, y, 1, 0, 0);
    }

    fn move_pointer(&mut self, x: f64, y: f64) {
        self.pointer(2, x, y, 1, 0, 0);
    }

    fn up(&mut self, x: f64, y: f64) {
        self.pointer(1, x, y, 1, 0, 0);
    }

    fn cancel(&mut self, x: f64, y: f64) {
        self.pointer(4, x, y, 1, 0, 0);
    }

    fn wheel(&mut self, x: f64, y: f64, notches: i32) {
        self.pointer(3, x, y, 0, 0, notches);
    }

    fn drag(&mut self, from: (f64, f64), to: (f64, f64), shift: bool) {
        let modifiers = u8::from(shift);
        self.pointer(0, from.0, from.1, 1, modifiers, 0);
        self.pointer(2, to.0, to.1, 1, modifiers, 0);
        self.pointer(1, to.0, to.1, 1, modifiers, 0);
    }
}

fn monster(id: u32, name: &str, class: ObjectClass, x: f32, y: f32, z: f32) -> WorldObject {
    placed(id, name, class, at(LANDBLOCK, x, y, z, 0.0))
}

fn near(point: (f64, f64), x: f64, y: f64) -> bool {
    (point.0 - x).abs() < 0.01 && (point.1 - y).abs() < 0.01
}

const CX: f64 = CANVAS_WIDTH as f64 / 2.0;
const CY: f64 = CANVAS_HEIGHT as f64 / 2.0;

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheCanvasRegistersInsideASealedDungeonAndHidesOutside
#[test]
fn the_canvas_registers_inside_a_sealed_dungeon_and_hides_outside() {
    let mut rig = Rig::new();
    rig.host.all_sealed = false;
    rig.tick();
    assert!(rig.ui.canvases.is_empty());

    rig.host.all_sealed = true;
    rig.tick();
    assert_eq!(CANVAS_ID, rig.canvas().spec.canvas_id);
    assert!(rig.canvas().visible);
    assert_eq!(1, rig.host.floorplan_requests);

    rig.host.all_sealed = false;
    rig.tick();
    assert!(!rig.canvas().visible);
    assert!(!rig.canvas().disposed);

    rig.host.all_sealed = true;
    rig.settings.current.draw_when_closed = false;
    rig.tick_by(SETTINGS_POLL_SECONDS + 1.0);
    assert!(!rig.canvas().visible);
    rig.controller.set_shown(true);
    rig.tick();
    assert!(rig.canvas().visible);
    assert_eq!(1, rig.host.floorplan_requests);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: ARepaintIsAskedOnlyWhenSomethingDrawnChanged
#[test]
fn a_repaint_is_asked_only_when_something_drawn_changed() {
    let mut rig = Rig::new();
    rig.host.meta.objects.push(monster(
        0x8000_0001,
        "Drudge",
        ObjectClass::Monster,
        15.0,
        5.0,
        0.0,
    ));
    rig.tick();
    assert_eq!(1, rig.canvas().invalidations);
    rig.tick();
    rig.tick();
    assert_eq!(1, rig.canvas().invalidations);

    rig.move_to(5.05, 5.0, 0.0, 0.0);
    rig.tick();
    assert_eq!(1, rig.canvas().invalidations);

    rig.move_to(6.0, 5.0, 0.0, 0.0);
    rig.tick();
    assert_eq!(2, rig.canvas().invalidations);

    rig.move_to(6.0, 5.0, 0.0, 45.0);
    rig.tick();
    assert_eq!(3, rig.canvas().invalidations);

    rig.controller.zoom_in(&mut rig.settings);
    rig.tick();
    assert_eq!(4, rig.canvas().invalidations);
    assert_eq!(
        vec![DungeonMapCamera::setting_from_scale(1.0)],
        rig.settings.saved_zoom
    );

    rig.host.meta.objects[0] =
        monster(0x8000_0001, "Drudge", ObjectClass::Monster, 15.05, 5.0, 0.0);
    rig.tick();
    assert_eq!(4, rig.canvas().invalidations);
    rig.host.meta.objects[0] = monster(0x8000_0001, "Drudge", ObjectClass::Monster, 16.0, 5.0, 0.0);
    rig.tick();
    assert_eq!(5, rig.canvas().invalidations);

    let mut snapshot = snapshot_at(6.0, 5.0, 0.0, 45.0);
    snapshot.position.cell_id = LANDBLOCK | 0x101;
    rig.host.meta.navigation = snapshot;
    rig.tick();
    assert_eq!(6, rig.canvas().invalidations);
    rig.tick();
    assert_eq!(6, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: ADragPansTheMapByItsDeltaAndStopsFollowing
#[test]
fn a_drag_pans_the_map_by_its_delta_and_stops_following() {
    let mut rig = Rig::new();
    rig.tick();
    assert!(rig.canvas().spec.accepts_pointer_input);
    assert!(rig.controller.camera().following());

    rig.down(100.0, 100.0);
    rig.move_pointer(140.0, 100.0);
    rig.move_pointer(140.0, 60.0);
    rig.up(140.0, 60.0);
    assert!(!rig.controller.camera().following());
    assert_eq!("Follow: off", rig.controller.follow_text());
    assert_close!(-5.0, rig.controller.camera().center_meters().x, 3);
    assert_close!(-5.0, rig.controller.camera().center_meters().y, 3);
    assert_close!(0.0, rig.controller.camera().rotation_degrees(), 3);

    rig.tick();
    assert_eq!(2, rig.canvas().invalidations);

    // The player walks on; the map stays where it was dragged.
    rig.move_to(8.0, 5.0, 0.0, 0.0);
    rig.tick();
    assert_close!(-5.0, rig.controller.camera().center_meters().x, 3);
    assert_close!(-5.0, rig.controller.camera().center_meters().y, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: AShiftDragTurnsTheMapADegreePerPixel
#[test]
fn a_shift_drag_turns_the_map_a_degree_per_pixel() {
    let mut rig = Rig::new();
    rig.move_to(5.0, 5.0, 0.0, 30.0);
    rig.tick();

    rig.drag((100.0, 100.0), (90.0, 95.0), true);
    assert!(!rig.controller.camera().following());
    assert_close!(45.0, rig.controller.camera().rotation_degrees(), 3);
    assert_eq!(Vec2::new(5.0, 5.0), rig.controller.camera().center_meters());
    rig.tick();
    assert_eq!(2, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheWheelZoomsAboutTheCursorKeepingTheSpotUnderItFixed
#[test]
fn the_wheel_zooms_about_the_cursor_keeping_the_spot_under_it_fixed() {
    let mut rig = Rig::new();
    rig.tick();
    let picture = DungeonMapPicture::build(&two_rooms());
    let width = f64::from(CANVAS_WIDTH);
    let height = f64::from(CANVAS_HEIGHT);

    rig.drag((100.0, 100.0), (120.0, 110.0), false);
    rig.tick();
    assert_eq!(2, rig.canvas().invalidations);

    let spot = Vec3::new(12.0, 8.0, 0.0);
    let before = rig
        .controller
        .camera()
        .screen_from_local(&picture, spot, width, height);
    rig.wheel(f64::from(before.x), f64::from(before.y), 1);
    assert_close!(1.0, rig.controller.camera().scale(), 3);
    let after = rig
        .controller
        .camera()
        .screen_from_local(&picture, spot, width, height);
    assert_close!(before.x, after.x, 2);
    assert_close!(before.y, after.y, 2);
    assert_eq!(
        vec![DungeonMapCamera::setting_from_scale(1.0)],
        rig.settings.saved_zoom
    );
    rig.tick();
    assert_eq!(3, rig.canvas().invalidations);

    rig.wheel(f64::from(before.x), f64::from(before.y), -2);
    assert_close!(0.6, rig.controller.camera().scale(), 3);
    let after = rig
        .controller
        .camera()
        .screen_from_local(&picture, spot, width, height);
    assert_close!(before.x, after.x, 2);
    assert_close!(before.y, after.y, 2);

    rig.controller.toggle_follow();
    rig.tick();
    rig.wheel(10.0, 10.0, 1);
    assert_close!(0.8, rig.controller.camera().scale(), 3);
    assert!(rig.controller.camera().following());
    rig.tick();
    assert_eq!(Vec2::new(5.0, 5.0), rig.controller.camera().center_meters());

    rig.wheel(10.0, 10.0, 30);
    assert_eq!(MAX_SCALE, rig.controller.camera().scale());
    rig.tick();
    let invalidations = rig.canvas().invalidations;
    let saves = rig.settings.saved_zoom.len();
    rig.wheel(10.0, 10.0, 1);
    rig.tick();
    assert_eq!(invalidations, rig.canvas().invalidations);
    assert_eq!(saves, rig.settings.saved_zoom.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: ACancelledPressEndsTheDrag
#[test]
fn a_cancelled_press_ends_the_drag() {
    let mut rig = Rig::new();
    rig.tick();

    rig.down(100.0, 100.0);
    rig.cancel(100.0, 100.0);
    rig.move_pointer(140.0, 100.0);
    assert!(rig.controller.camera().following());
    assert_eq!(Vec2::new(5.0, 5.0), rig.controller.camera().center_meters());
    rig.tick();
    assert_eq!(1, rig.canvas().invalidations);

    rig.down(200.0, 100.0);
    rig.move_pointer(240.0, 100.0);
    assert_close!(-5.0, rig.controller.camera().center_meters().x, 3);
    assert_close!(5.0, rig.controller.camera().center_meters().y, 3);
}

fn rgba(color: &Color) -> (u8, u8, u8, u8) {
    (color.r, color.g, color.b, color.a)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: FloorsArePaintedAsTurnedRectanglesInTheFloorColourAndWallsAsLines
#[test]
#[allow(clippy::float_cmp)]
fn floors_are_painted_as_turned_rectangles_in_the_floor_colour_and_walls_as_lines() {
    let mut rig = Rig::new();
    rig.settings.current.map_zoom = 4.0;
    rig.tick();
    let painter = rig.paint();

    let floors: Vec<_> = painter
        .lines
        .iter()
        .filter(|(from, to, color, thickness)| {
            *thickness == 50.0
                && color.g == 127
                && near(*from, CX - 25.0, CY)
                && near(*to, CX + 25.0, CY)
        })
        .collect();
    assert_eq!(1, floors.len());
    assert_eq!((0, 127, 191, 204), rgba(&floors[0].2));
    let walls: Vec<_> = painter
        .lines
        .iter()
        .filter(|(from, to, _, thickness)| {
            *thickness == 1.0
                && near(*from, CX - 25.0, CY + 25.0)
                && near(*to, CX - 25.0, CY - 25.0)
        })
        .collect();
    assert_eq!(1, walls.len());
    let wall = rgba(&walls[0].2);
    assert_eq!((0, 0, 127), (wall.0, wall.1, wall.2));

    rig.move_to(5.0, 5.0, 0.0, 90.0);
    rig.tick();
    let painter = rig.paint();
    let turned = painter
        .lines
        .iter()
        .filter(|(from, to, color, thickness)| {
            *thickness == 50.0
                && color.g == 127
                && near(*from, CX, CY + 25.0)
                && near(*to, CX, CY - 25.0)
        })
        .count();
    assert_eq!(1, turned);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheStoreyAboveIsWashedAndAllLayersPaintsItPlain
#[test]
#[allow(clippy::float_cmp)]
fn the_storey_above_is_washed_and_all_layers_paints_it_plain() {
    let mut rig = Rig::new();
    rig.settings.current.map_zoom = 4.0;
    rig.tick();
    let painter = rig.paint();

    let above: Vec<_> = painter
        .lines
        .iter()
        .filter(|(_, _, color, thickness)| *thickness == 50.0 && color.a < 200)
        .collect();
    assert_eq!(1, above.len());
    assert_eq!((0, 75, 113, 97), rgba(&above[0].2));
    let plain = |painter: &Painter| {
        painter
            .lines
            .iter()
            .filter(|(_, _, color, thickness)| {
                *thickness == 50.0 && color.a == 204 && color.g == 127
            })
            .count()
    };
    assert_eq!(2, plain(&painter));

    rig.controller.set_show_all_layers(true);
    rig.tick();
    let painter = rig.paint();
    assert_eq!(3, plain(&painter));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: AtTheSunkenEntryOnlyTheCurrentStoreyIsSolidAndLabelled
#[test]
fn at_the_sunken_entry_only_the_current_storey_is_solid_and_labelled() {
    let entry = vec![
        (60.0, -165.0),
        (70.0, -165.0),
        (70.0, -155.0),
        (60.0, -155.0),
    ];
    let hall = vec![
        (50.0, -150.0),
        (90.0, -150.0),
        (90.0, -100.0),
        (50.0, -100.0),
    ];
    let plan = Floorplan {
        landblock_id: LANDBLOCK,
        layers: vec![
            DungeonLayer {
                z: -6.0,
                floors: vec![entry],
                walls: vec![DungeonWall {
                    start: (60.0, -165.0),
                    end: (60.0, -155.0),
                }],
            },
            DungeonLayer {
                z: 0.0,
                floors: vec![hall],
                walls: Vec::new(),
            },
        ],
        cells: vec![
            DungeonCell {
                cell_id: LANDBLOCK | 0x100,
                center: (65.0, -160.0, -6.0),
                layer_z: -6.0,
            },
            DungeonCell {
                cell_id: LANDBLOCK | 0x10A,
                center: (70.0, -125.0, 0.0),
                layer_z: 0.0,
            },
        ],
        bounds_min: (50.0, -165.0, -6.0),
        bounds_max: (90.0, -100.0, 0.0),
    };
    let mut rig = Rig::new();
    rig.set_plan(plan);
    rig.settings.current.map_zoom = 4.2;
    rig.settings.current.opacity = 16;
    rig.host.meta.objects.push(monster(
        0x8000_0010,
        "Portal to Yaraq",
        ObjectClass::Portal,
        68.0,
        -158.0,
        -6.0,
    ));
    rig.host.meta.objects.push(monster(
        0x8000_0011,
        "Greenspire",
        ObjectClass::Portal,
        55.0,
        -120.0,
        0.0,
    ));
    rig.move_to(60.03, -160.03, -5.995, 0.0);
    rig.tick();
    let painter = rig.paint();

    assert_eq!(-1.0, ((rig.controller.draw_z() + 3.0) / 6.0).floor());
    let floors: Vec<_> = painter
        .lines
        .iter()
        .filter(|(_, _, color, thickness)| {
            (thickness - 40.0).abs() < 0.01 && color.g == 127 && color.a == 204
        })
        .collect();
    assert_eq!(1, floors.len());
    let floor = rgba(&floors[0].2);
    assert_eq!((0, 127, 191), (floor.0, floor.1, floor.2));
    assert_close!(40.0, floors[0].1.0 - floors[0].0.0, 2);
    let above: Vec<_> = painter
        .lines
        .iter()
        .filter(|(_, _, _, thickness)| (thickness - 200.0).abs() < 0.01)
        .collect();
    assert_eq!(1, above.len());
    assert_eq!((0, 75, 113, 97), rgba(&above[0].2));
    let count =
        |painter: &Painter, text: &str| painter.texts.iter().filter(|t| t.0 == text).count();
    assert_eq!(1, count(&painter, "Yaraq"));
    assert_eq!(0, count(&painter, "Greenspire"));

    rig.controller.set_show_all_layers(true);
    rig.tick();
    let painter = rig.paint();
    assert_eq!(1, count(&painter, "Yaraq"));
    assert_eq!(1, count(&painter, "Greenspire"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: MarksAreIconsOrDotsAndLabelsAreWrittenUprightAfterTheFloor
#[test]
#[allow(clippy::float_cmp)]
fn marks_are_icons_or_dots_and_labels_are_written_upright_after_the_floor() {
    let mut rig = Rig::new();
    rig.settings.current.map_zoom = 4.0;
    rig.host.meta.objects.push(monster(
        0x8000_0001,
        "Drudge",
        ObjectClass::Monster,
        15.0,
        5.0,
        0.0,
    ));
    rig.host.meta.objects.push(monster(
        0x8000_0002,
        "Chest",
        ObjectClass::Container,
        15.0,
        8.0,
        0.0,
    ));
    rig.host.meta.objects.push(monster(
        0x5000_0002,
        "Horan",
        ObjectClass::Player,
        5.0,
        8.0,
        0.0,
    ));
    rig.host.meta.objects.push(monster(
        0x8000_0003,
        "Lifestone",
        ObjectClass::Lifestone,
        2.0,
        2.0,
        0.0,
    ));
    let drudge_icon = MapImage {
        handle: 7,
        width: 32,
        height: 32,
    };
    rig.ui.object_icons.insert(0x8000_0001, drudge_icon);
    rig.tick();
    let painter = rig.paint();

    assert_eq!(1, painter.images.len());
    let (image, place) = painter.images[0];
    assert_eq!(drudge_icon, image);
    assert_close!(CX + 50.0, place.0 + (place.2 / 2.0), 2);
    assert_close!(CY, place.1 + (place.3 / 2.0), 2);

    assert!(painter.lines.iter().any(|(from, to, color, thickness)| {
        *thickness == 1.0
            && rgba(color).0 == 244
            && color.g == 164
            && color.b == 96
            && (from.1 - (CY - 15.0)).abs() < 4.0
            && from.0 > CX + 40.0
            && to.0 < CX + 60.0
    }));
    assert!(painter.lines.iter().any(|(from, _, color, thickness)| {
        *thickness == 1.0
            && color.r == 255
            && color.g == 0
            && color.b == 0
            && (from.1 - CY).abs() < 4.0
            && (from.0 - CX).abs() < 4.0
    }));

    let horan: Vec<_> = painter.texts.iter().filter(|t| t.0 == "Horan").collect();
    assert_eq!(1, horan.len());
    let (_, place, color) = horan[0];
    assert_close!(CX - 15.0, place.0, 2);
    assert!(place.1 > CY - 15.0 && place.1 < CY);
    assert_eq!(255, color.r);
    assert!(!painter.texts.iter().any(|t| t.0 == "Drudge"));
    assert!(!painter.texts.iter().any(|t| t.0 == "Lifestone"));
    assert!(
        !painter
            .lines
            .iter()
            .any(|(_, _, color, _)| color.r == 245 && color.g == 245 && color.b == 245)
    );

    rig.move_to(5.0, 5.0, 0.0, 90.0);
    rig.tick();
    let painter = rig.paint();
    let horan: Vec<_> = painter.texts.iter().filter(|t| t.0 == "Horan").collect();
    assert_eq!(1, horan.len());
    let turned = horan[0].1;
    assert_close!(CX - 15.0 - 15.0, turned.0, 2);
    assert!(turned.1 > CY);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheCellStoodInIsShadedInTheVisitedColourAndKept
#[test]
#[allow(clippy::float_cmp)]
fn the_cell_stood_in_is_shaded_in_the_visited_colour_and_kept() {
    let mut rig = Rig::new();
    rig.settings.current.map_zoom = 4.0;
    rig.tick();
    let painter = rig.paint();

    let visited: Vec<_> = painter
        .lines
        .iter()
        .filter(|(_, _, color, thickness)| {
            *thickness == 50.0 && color.r == 255 && color.g == 150 && color.b == 255
        })
        .collect();
    assert_eq!(1, visited.len());
    assert!(near(visited[0].0, CX - 25.0, CY));
    assert!(
        rig.storage
            .keys()
            .iter()
            .any(|key| key == "mosstank/ub/dungeonmaps/visited/01A9.txt")
    );

    rig.settings.current.show_visited_tiles = false;
    rig.tick_by(SETTINGS_POLL_SECONDS + 1.0);
    assert_eq!(2, rig.canvas().invalidations);
    let painter = rig.paint();
    assert!(!painter.lines.iter().any(|(_, _, color, _)| color.g == 150));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheNameAndTheCompassAreDrawnWhenTheirRowsSay
#[test]
fn the_name_and_the_compass_are_drawn_when_their_rows_say() {
    let mut rig = Rig::new();
    rig.move_to(5.0, 5.0, 7.0, 90.0);
    rig.tick();
    let painter = rig.paint();

    assert!(painter.texts.iter().any(|t| t.0 == "Dungeon 01A9 (Z:1)"));
    let tips: Vec<_> = painter.texts.iter().filter(|t| t.0 == "N").collect();
    assert_eq!(1, tips.len());
    assert!(tips[0].1.0 < f64::from(CANVAS_WIDTH) - 18.0);

    rig.settings.current.show_compass = false;
    rig.settings.current.dungeon_name.enabled = false;
    rig.tick_by(SETTINGS_POLL_SECONDS + 1.0);
    let painter = rig.paint();
    assert!(painter.texts.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheSliderAndAPanStopFollowingAndFollowBringsItBack
#[test]
fn the_slider_and_a_pan_stop_following_and_follow_brings_it_back() {
    let mut rig = Rig::new();
    rig.tick();
    assert!(rig.controller.camera().following());
    assert_eq!("Follow: on", rig.controller.follow_text());

    rig.controller.set_height_percent(100);
    rig.tick();
    assert!(!rig.controller.camera().following());
    assert_close!(6.0, rig.controller.draw_z(), 3);
    assert_eq!(100, rig.controller.height_percent());
    let painter = rig.paint();
    let names: Vec<_> = painter
        .texts
        .iter()
        .filter(|t| t.0.starts_with("Dungeon"))
        .collect();
    assert_eq!(1, names.len());
    assert!(names[0].0.contains("Z:1"));

    rig.controller.toggle_follow();
    rig.tick();
    assert!(rig.controller.camera().following());
    assert_close!(0.0, rig.controller.draw_z(), 3);
    assert_eq!(0, rig.controller.height_percent());

    rig.controller.pan(f64::from(PAN_STEP_PIXELS), 0.0);
    assert!(!rig.controller.camera().following());
    rig.controller.toggle_follow();
    rig.tick();
    assert_eq!(Vec2::new(5.0, 5.0), rig.controller.camera().center_meters());

    rig.host.meta.objects.push(monster(
        0x8000_0001,
        "Drudge",
        ObjectClass::Monster,
        15.0,
        5.0,
        0.0,
    ));
    rig.ui.object_icons.insert(
        0x8000_0001,
        MapImage {
            handle: 7,
            width: 32,
            height: 32,
        },
    );
    rig.tick();
    rig.paint();
    rig.controller.dispose(&mut rig.ui);
    assert_eq!(1, rig.ui.released);
    assert!(rig.canvas().disposed);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DungeonMapControllerTests.cs :: TheStatusSaysWhichDungeonAndHowMuchOfItIsWalked
#[test]
fn the_status_says_which_dungeon_and_how_much_of_it_is_walked() {
    let mut rig = Rig::new();
    rig.host.all_sealed = false;
    rig.tick();
    assert_eq!("Not in a dungeon.", rig.controller.status());
    rig.host.all_sealed = true;
    rig.tick();
    assert_eq!(
        "Dungeon 01A9: 2 storeys, 1 of 3 cells walked.",
        rig.controller.status()
    );
}
