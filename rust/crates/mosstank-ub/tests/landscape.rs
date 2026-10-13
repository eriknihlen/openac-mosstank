//! The countryside map, against `LandscapeMapViewTests`,
//! `LandscapeMarkerCatalogTests` and `LandscapeMapControllerTests`.

use std::collections::HashSet;

use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::Position;
use mosstank_stores::ub_store;
use mosstank_ub::canvas::{
    BUTTON_LEFT, MapCanvasSpec, MapImage, MapPainter, MapUi, POINTER_CANCELLED, POINTER_DOWN,
    POINTER_MOVE, POINTER_UP, POINTER_WHEEL,
};
use mosstank_ub::landscape::controller::{
    CANVAS_HEIGHT, CANVAS_ID, CANVAS_WIDTH, LandscapeMapController, LandscapeMapSettings,
    MAP_ART_SURFACE_ID,
};
use mosstank_ub::landscape::markers::{
    self, FOLDER, LandscapeMarker, LandscapeMarkerCatalog, LandscapeMarkerKind, SHIPPED_FILE,
};
use mosstank_ub::landscape::placer::{MapLabelPlacer, Rect};
use mosstank_ub::landscape::view::{self, LandscapeMapView, MapRulerTick};
use mosstank_ub::testing::SharedStorage;

fn close(expected: f64, actual: f64, places: i32) -> bool {
    (expected - actual).abs() <= 0.5 * 10f64.powi(-places)
}

macro_rules! assert_close {
    ($expected:expr, $actual:expr, $places:expr) => {{
        let (expected, actual): (f64, f64) = ($expected, $actual);
        assert!(
            close(expected, actual, $places),
            "expected {expected}, got {actual}"
        );
    }};
}

// ---- LandscapeMapViewTests ---------------------------------------------------

fn view_320() -> LandscapeMapView {
    LandscapeMapView::new(320, 280, 2048, 2048)
}

/// The test painter's measure: six pixels a character, twelve high.
#[allow(clippy::cast_precision_loss)]
fn measure(text: &str) -> (f64, f64) {
    (text.len() as f64 * 6.0, 12.0)
}

fn widest(ticks: &[MapRulerTick]) -> f64 {
    ticks
        .iter()
        .map(|tick| measure(&tick.label).0)
        .fold(0.0, f64::max)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: APositionRoundTripsThroughTheCanvasAndNorthIsUp
#[test]
fn a_position_round_trips_through_the_canvas_and_north_is_up() {
    let mut view = view_320();
    view.set_zoom(0.5);
    view.center_on(33.6, 42.1);

    let (x, y) = view.to_canvas(33.6, 42.1);
    assert_close!(160.0, x, 6);
    assert_close!(140.0, y, 6);

    let (east_west, north_south) = view.to_world(x, y);
    assert_close!(33.6, east_west, 6);
    assert_close!(42.1, north_south, 6);

    let (_, north_y) = view.to_canvas(33.6, 60.0);
    assert!(
        north_y < y,
        "a position further north draws higher on the canvas"
    );
    let (east_x, _) = view.to_canvas(50.0, 42.1);
    assert!(east_x > x, "a position further east draws further right");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: ZoomingAboutAPointKeepsThePositionUnderIt
#[test]
fn zooming_about_a_point_keeps_the_position_under_it() {
    let mut view = view_320();
    view.set_zoom(0.3);
    view.center_on(10.0, -20.0);
    let (east_west, north_south) = view.to_world(250.0, 60.0);

    view.zoom_about(250.0, 60.0, view::ZOOM_STEP);
    let (x, y) = view.to_canvas(east_west, north_south);
    assert_close!(250.0, x, 6);
    assert_close!(60.0, y, 6);

    view.zoom_about(250.0, 60.0, -3.0 * view::ZOOM_STEP);
    let (x, y) = view.to_canvas(east_west, north_south);
    assert_close!(250.0, x, 6);
    assert_close!(60.0, y, 6);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: TheZoomStaysBetweenZeroAndOne
#[test]
fn the_zoom_stays_between_zero_and_one() {
    let mut view = view_320();
    view.zoom_about(0.0, 0.0, -5.0);
    assert_close!(0.0, view.zoom(), 12);
    assert!(view.scale() > 0.0);
    view.zoom_about(0.0, 0.0, 5.0);
    assert_close!(1.0, view.zoom(), 12);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: PanningSlidesTheMapByCanvasPixelsAndStopsFollowing
#[test]
fn panning_slides_the_map_by_canvas_pixels_and_stops_following() {
    let mut view = view_320();
    view.set_zoom(0.5);
    view.follow = true;
    view.center_on(0.0, 0.0);
    let (east_west, north_south) = view.to_world(160.0, 140.0);

    view.pan_by(40.0, -10.0);
    assert!(!view.follow);
    let (x, y) = view.to_canvas(east_west, north_south);
    assert_close!(200.0, x, 6);
    assert_close!(130.0, y, 6);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: TheRulerStepFollowsTheZoom
#[test]
fn the_ruler_step_follows_the_zoom() {
    for (zoom, increment) in [
        (0.0, 50.0),
        (0.04, 20.0),
        (0.1, 10.0),
        (0.24, 5.0),
        (0.4, 2.0),
        (0.6, 1.0),
        (0.8, 0.5),
        (0.9, 0.2),
        (1.0, 0.1),
    ] {
        assert_close!(increment, LandscapeMapView::coordinate_increment(zoom), 12);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: RulerTicksSitOnMultiplesOfTheStepInsideTheCanvas
#[test]
fn ruler_ticks_sit_on_multiples_of_the_step_inside_the_canvas() {
    let mut view = view_320();
    view.set_zoom(0.6);
    view.center_on(33.6, 42.1);
    let increment = view.horizontal_increment(&mut measure);
    assert_close!(
        LandscapeMapView::coordinate_increment(view.zoom()),
        increment,
        12
    );

    let horizontal = view.horizontal_ticks(&mut measure);
    assert!(!horizontal.is_empty());
    for tick in &horizontal {
        assert!((0.0..=320.0).contains(&tick.pixel));
        let east_west = view.to_world(tick.pixel, 0.0).0;
        assert_close!(
            0.0,
            (east_west / increment - (east_west / increment).round()).abs(),
            6
        );
        assert!(tick.label.ends_with('E'));
    }

    let vertical = view.vertical_ticks(&mut measure);
    assert!(!vertical.is_empty());
    for tick in &vertical {
        assert!((0.0..=280.0).contains(&tick.pixel));
        assert!(tick.label.ends_with('N'));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: RowZeroIsTheClientsNorthEdgeAtOneHundredTwoPointThree
#[test]
fn row_zero_is_the_clients_north_edge_at_one_hundred_two_point_three() {
    let view = view_320();

    let (x, y) = view.to_image(-102.4, 102.3);
    assert_close!(0.0, x, 6);
    assert_close!(0.0, y, 6);

    let (_, south) = view.to_image(0.0, -102.5);
    assert_close!(2048.0, south, 6);

    let (_, north_south) = view.to_world(view.canvas_top(), view.canvas_top());
    assert_close!(102.3, north_south, 6);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: TheReadoutWritesNorthSouthFirstWithTheHemisphereAfterTheNumber
#[test]
fn the_readout_writes_north_south_first_with_the_hemisphere_after_the_number() {
    assert_eq!("42.10N, 33.60E", view::position_text(33.6, 42.1));
    assert_eq!("12.50S, 7.25W", view::position_text(-7.25, -12.5));
    assert_eq!("0.00, 0.00", view::position_text(0.0, 0.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: TheEastWestStepWidensUntilTheLabelsFitAndTheNorthSouthStepNeedNot
#[test]
fn the_east_west_step_widens_until_the_labels_fit_and_the_north_south_step_need_not() {
    let mut view = LandscapeMapView::new(320, 280, 1024, 1024);
    view.set_zoom(0.06);
    view.center_on(0.0, 0.0);
    assert_close!(
        10.0,
        LandscapeMapView::coordinate_increment(view.zoom()),
        12
    );

    assert_close!(20.0, view.horizontal_increment(&mut measure), 12);
    assert_close!(10.0, view.vertical_increment(&mut measure), 12);

    let horizontal = view.horizontal_ticks(&mut measure);
    let widest = widest(&horizontal);
    for pair in horizontal.windows(2) {
        assert!(
            pair[1].pixel - pair[0].pixel >= widest + view::LABEL_GAP,
            "ticks {} and {} crowd",
            pair[0].label,
            pair[1].label
        );
    }

    let vertical = view.vertical_ticks(&mut measure);
    for pair in vertical.windows(2) {
        assert!(pair[0].pixel - pair[1].pixel >= 12.0 + view::LABEL_GAP);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapViewTests.cs :: TheEastWestStepIsTheNarrowestWhoseLabelsDoNotOverlap
#[test]
fn the_east_west_step_is_the_narrowest_whose_labels_do_not_overlap() {
    for zoom in [0.0, 0.04, 0.1, 0.24, 0.4, 0.6, 0.8, 0.9, 1.0] {
        let mut view = LandscapeMapView::new(320, 280, 1024, 1024);
        view.set_zoom(zoom);
        view.center_on(33.6, 42.1);
        let ticks = view.horizontal_ticks(&mut measure);
        let widest = widest(&ticks);
        for pair in ticks.windows(2) {
            assert!(pair[1].pixel - pair[0].pixel >= widest + view::LABEL_GAP);
        }

        let chosen = view.horizontal_increment(&mut measure);
        let own = LandscapeMapView::coordinate_increment(zoom);
        assert!(chosen >= own);
        if chosen > own {
            // One step narrower would have crowded.
            let narrower = LandscapeMapView::narrower_increment(chosen);
            assert!(narrower * view.pixels_per_unit_x() < widest + view::LABEL_GAP);
        }
    }
}

// ---- LandscapeMarkerCatalogTests -----------------------------------------------

#[test]
fn the_marker_folder_sits_under_the_ub_tree() {
    assert_eq!(FOLDER, format!("{}maps/", ub_store::ROOT));
    assert_eq!(SHIPPED_FILE, format!("{FOLDER}markers.csv"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: AnEmptyFolderGetsTheShippedFileWithTheFormatWrittenInIt
#[test]
fn an_empty_folder_gets_the_shipped_file_with_the_format_written_in_it() {
    let mut storage = SharedStorage::new();
    let mut catalog = LandscapeMarkerCatalog::new();

    assert!(!catalog.load(&mut storage).unwrap());
    let shipped = storage.get(SHIPPED_FILE).expect("the shipped file");
    assert!(shipped.contains("kind, name, north-south, east-west"));
    assert!(catalog.markers().is_empty());
    assert_eq!(0, catalog.revision());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: EveryCsvUnderTheFolderIsReadAndAChangeBumpsTheRevision
#[test]
fn every_csv_under_the_folder_is_read_and_a_change_bumps_the_revision() {
    let mut storage = SharedStorage::new();
    storage.insert(
        &format!("{FOLDER}towns.csv"),
        "# towns\nTown, Holtburg, 42.1, 33.6\n",
    );
    storage.insert(
        &format!("{FOLDER}portals.csv"),
        "portal, \"Holtburg, Town Portal\", 42.5, 33.9\n",
    );
    storage.insert(&format!("{FOLDER}notes.txt"), "Town, Nope, 1, 1");
    let mut catalog = LandscapeMarkerCatalog::new();

    assert!(catalog.load(&mut storage).unwrap());
    assert_eq!(1, catalog.revision());
    assert_eq!(
        vec![
            LandscapeMarker::new(
                LandscapeMarkerKind::Portal,
                "Holtburg, Town Portal",
                42.5,
                33.9
            ),
            LandscapeMarker::new(LandscapeMarkerKind::Town, "Holtburg", 42.1, 33.6),
        ],
        catalog.markers()
    );

    assert!(!catalog.load(&mut storage).unwrap());
    assert_eq!(1, catalog.revision());

    storage.insert(
        &format!("{FOLDER}towns.csv"),
        "Town, Holtburg, 42.1, 33.6\nLifestone, Holtburg Lifestone, 42.3, 33.7\n",
    );
    assert!(catalog.load(&mut storage).unwrap());
    assert_eq!(2, catalog.revision());
    assert_eq!(3, catalog.markers().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: ALineIsAMarkerOnlyWhenAllFourFieldsParse
#[test]
fn a_line_is_a_marker_only_when_all_four_fields_parse() {
    for (line, parses) in [
        ("Town, Holtburg, 42.1", false),
        ("Castle, Holtburg, 42.1, 33.6", false),
        ("Unknown, Holtburg, 42.1, 33.6", false),
        ("Town, , 42.1, 33.6", false),
        ("Town, Holtburg, 142.1, 33.6", false),
        ("Town, Holtburg, forty, 33.6", false),
        ("VENDOR, Aun Tanua, -31.1, 13.7", true),
        ("Dungeon,\"Halls, of Metos\",-3.5,88.2", true),
    ] {
        assert_eq!(parses, markers::try_parse_line(line).is_some(), "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: ParsingAFileSkipsCommentsBlanksAndBadLines
#[test]
fn parsing_a_file_skips_comments_blanks_and_bad_lines() {
    let parsed = markers::parse(
        "# header\n\nTown, Holtburg, 42.1, 33.6\r\nnonsense\nNpc, Ulgrim, 42.0, 33.5\n",
    );
    assert_eq!(2, parsed.len());
    assert_eq!("Ulgrim", parsed[1].name);
    assert_eq!(LandscapeMarkerKind::Npc, parsed[1].kind);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: EachKindShowsItsIconAndThenItsNameFromItsOwnZoom
#[test]
fn each_kind_shows_its_icon_and_then_its_name_from_its_own_zoom() {
    for (kind, zoom, icon, label) in [
        (LandscapeMarkerKind::Town, 0.0, true, true),
        (LandscapeMarkerKind::Portal, 0.05, false, false),
        (LandscapeMarkerKind::Portal, 0.2, true, false),
        (LandscapeMarkerKind::Portal, 0.32, true, true),
        (LandscapeMarkerKind::Lifestone, 0.3, true, false),
        (LandscapeMarkerKind::Vendor, 0.5, true, false),
        (LandscapeMarkerKind::Vendor, 0.8, true, true),
    ] {
        let display = markers::display_of(kind);
        assert_eq!(icon, display.shows_marker_at(zoom), "{kind:?} at {zoom}");
        assert_eq!(label, display.shows_label_at(zoom), "{kind:?} at {zoom}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: ANameHasEightCandidatePlacesWalkedClockwiseFromAbove
#[test]
fn a_name_has_eight_candidate_places_walked_clockwise_from_above() {
    let candidates = MapLabelPlacer::candidates(100.0, 100.0, 40.0, 12.0);
    assert_eq!(
        [
            Rect::new(80.0, 78.0, 40.0, 12.0),
            Rect::new(104.0, 80.0, 40.0, 12.0),
            Rect::new(108.0, 93.0, 40.0, 12.0),
            Rect::new(108.0, 104.0, 40.0, 12.0),
            Rect::new(80.0, 106.0, 40.0, 12.0),
            Rect::new(50.0, 102.0, 40.0, 12.0),
            Rect::new(50.0, 93.0, 40.0, 12.0),
            Rect::new(54.0, 82.0, 40.0, 12.0),
        ],
        candidates
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: ANameStepsAroundWhatIsAlreadyPlacedAndGivesUpWhenNothingIsFree
#[test]
fn a_name_steps_around_what_is_already_placed_and_gives_up_when_nothing_is_free() {
    let mut placer = MapLabelPlacer::new();
    let first = placer.try_place(100.0, 100.0, 40.0, 12.0);
    assert_eq!(Some(Rect::new(80.0, 78.0, 40.0, 12.0)), first);
    assert_eq!(
        Some(Rect::new(108.0, 93.0, 40.0, 12.0)),
        placer.try_place(100.0, 100.0, 40.0, 12.0)
    );
    assert_eq!(
        Some(Rect::new(80.0, 106.0, 40.0, 12.0)),
        placer.try_place(100.0, 100.0, 40.0, 12.0)
    );
    assert_eq!(
        Some(Rect::new(50.0, 93.0, 40.0, 12.0)),
        placer.try_place(100.0, 100.0, 40.0, 12.0)
    );
    assert_eq!(None, placer.try_place(100.0, 100.0, 40.0, 12.0));

    placer.reset();
    assert!(placer.occupied().is_empty());
    assert_eq!(first, placer.try_place(100.0, 100.0, 40.0, 12.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: AClaimedIconPushesANameToTheNextFreePlace
#[test]
fn a_claimed_icon_pushes_a_name_to_the_next_free_place() {
    let mut placer = MapLabelPlacer::new();
    placer.occupy(Rect::new(80.0, 78.0, 40.0, 12.0));
    assert_eq!(
        Some(Rect::new(108.0, 93.0, 40.0, 12.0)),
        placer.try_place(100.0, 100.0, 40.0, 12.0)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMarkerCatalogTests.cs :: TouchingEdgesDoNotIntersect
#[test]
fn touching_edges_do_not_intersect() {
    assert!(!MapLabelPlacer::intersects(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(10.0, 0.0, 10.0, 10.0)
    ));
    assert!(MapLabelPlacer::intersects(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(9.0, 9.0, 10.0, 10.0)
    ));
}

// ---- LandscapeMapControllerTests -----------------------------------------------

#[derive(Debug)]
struct FakeCanvas {
    spec: MapCanvasSpec,
    visible: bool,
    invalidations: usize,
    disposed: bool,
}

#[derive(Debug, Default)]
struct FakeUi {
    canvases: Vec<FakeCanvas>,
    client_art: HashSet<u32>,
    requested: Vec<u32>,
    released: usize,
}

impl MapUi for FakeUi {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        self.canvases.push(FakeCanvas {
            spec: spec.clone(),
            visible: spec.start_visible,
            invalidations: 0,
            disposed: false,
        });
        Some(u32::try_from(self.canvases.len()).unwrap())
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvases[usize::try_from(canvas).unwrap() - 1].visible = visible;
    }

    fn invalidate_canvas(&mut self, canvas: u32) {
        self.canvases[usize::try_from(canvas).unwrap() - 1].invalidations += 1;
    }

    fn remove_canvas(&mut self, canvas: u32) {
        self.canvases[usize::try_from(canvas).unwrap() - 1].disposed = true;
    }

    fn object_icon(&mut self, _object_id: u32) -> MapImage {
        MapImage::default()
    }

    fn client_art(&mut self, surface_id: u32) -> MapImage {
        self.requested.push(surface_id);
        if self.client_art.contains(&surface_id) {
            MapImage {
                handle: (surface_id & 0xFFFF) + 1,
                width: 1024,
                height: 1024,
            }
        } else {
            MapImage::default()
        }
    }

    fn release_image(&mut self, _image: MapImage) {
        self.released += 1;
    }
}

/// Records every call so a paint can be read back as a list of what it drew.
#[derive(Debug, Default)]
struct RecordingPainter {
    ops: Vec<String>,
    texts: Vec<String>,
    images: Vec<MapImage>,
    clip_depth: i32,
}

impl MapPainter for RecordingPainter {
    fn width(&self) -> i32 {
        CANVAS_WIDTH
    }

    fn height(&self) -> i32 {
        CANVAS_HEIGHT
    }

    fn clear(&mut self, color: &Color) {
        self.ops.push(format!("clear a={}", color.a));
    }

    fn fill_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color) {
        self.ops.push("fill".to_owned());
    }

    fn stroke_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color, _thickness: f32) {
        self.ops.push("stroke".to_owned());
    }

    fn draw_line(&mut self, _from: (f64, f64), _to: (f64, f64), color: &Color, _thickness: f32) {
        self.ops
            .push(format!("line {},{},{}", color.r, color.g, color.b));
    }

    fn draw_text(&mut self, text: &str, _at: (f64, f64), _color: &Color, _outline: bool) {
        self.ops.push("text".to_owned());
        self.texts.push(text.to_owned());
    }

    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        measure(text)
    }

    fn draw_image(&mut self, image: MapImage, _rect: (f64, f64, f64, f64), _tint: &Color) {
        self.ops.push("image".to_owned());
        self.images.push(image);
    }

    fn push_clip(&mut self, _rect: (f64, f64, f64, f64)) {
        self.clip_depth += 1;
    }

    fn pop_clip(&mut self) {
        self.clip_depth -= 1;
    }
}

struct Rig {
    ui: FakeUi,
    settings: LandscapeMapSettings,
    player: Option<Position>,
    controller: LandscapeMapController,
}

impl Rig {
    fn new() -> Self {
        Self::with(LandscapeMapSettings::default())
    }

    fn with(settings: LandscapeMapSettings) -> Self {
        Self {
            ui: FakeUi::default(),
            settings,
            player: Some(Position {
                cell_id: 0xA9B4_0001,
                east_west: 33.6,
                north_south: 42.1,
                elevation: 0.0,
                heading_degrees: 90.0,
                is_outdoor: true,
            }),
            controller: LandscapeMapController::new(&settings),
        }
    }

    fn canvas(&self) -> &FakeCanvas {
        assert_eq!(1, self.ui.canvases.len());
        &self.ui.canvases[0]
    }

    fn paint(&mut self) -> RecordingPainter {
        let mut painter = RecordingPainter::default();
        self.controller.paint(&mut painter, &mut self.ui);
        assert_eq!(0, painter.clip_depth);
        painter
    }

    fn ticks(&mut self, count: usize) {
        for _ in 0..count {
            self.controller
                .on_tick(&self.settings, &mut self.ui, self.player.clone(), 0.015);
        }
    }

    fn toggle_follow(&mut self) {
        self.controller.toggle_follow(self.player.clone());
    }

    fn send(&mut self, kind: u8, x: f64, y: f64, button: u8, wheel_delta: i32) {
        assert!(self.canvas().spec.accepts_pointer_input);
        #[allow(clippy::cast_possible_truncation)]
        self.controller.on_pointer(&PointerEvent {
            canvas: 1,
            kind,
            x: x as f32,
            y: y as f32,
            button,
            modifiers: 0,
            wheel_delta,
        });
    }

    fn down(&mut self, x: f64, y: f64) {
        self.send(POINTER_DOWN, x, y, BUTTON_LEFT, 0);
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.send(POINTER_MOVE, x, y, BUTTON_LEFT, 0);
    }

    fn up(&mut self, x: f64, y: f64) {
        self.send(POINTER_UP, x, y, BUTTON_LEFT, 0);
    }

    fn wheel(&mut self, x: f64, y: f64, notches: i32) {
        self.send(POINTER_WHEEL, x, y, 0, notches);
    }

    fn cancel(&mut self, x: f64, y: f64) {
        self.send(POINTER_CANCELLED, x, y, BUTTON_LEFT, 0);
    }

    fn set_player(&mut self, change: impl FnOnce(&mut Position)) {
        let mut player = self.player.clone().expect("a player");
        change(&mut player);
        self.player = Some(player);
    }
}

fn marker_storage(text: &str) -> SharedStorage {
    let storage = SharedStorage::new();
    storage.insert(&format!("{FOLDER}a.csv"), text);
    storage
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: TheCanvasIsRegisteredOnceWhenTheWindowFirstOpensAndShownOnlyWhileItIsOpen
#[test]
fn the_canvas_is_registered_once_when_the_window_first_opens_and_shown_only_while_it_is_open() {
    let mut rig = Rig::new();
    rig.ticks(3);
    assert!(rig.ui.canvases.is_empty());

    rig.controller.set_shown(true);
    rig.ticks(1);
    assert_eq!(CANVAS_ID, rig.canvas().spec.canvas_id);
    assert!(rig.canvas().visible);
    assert_eq!(1, rig.canvas().invalidations);

    rig.controller.set_shown(false);
    rig.ticks(1);
    assert!(!rig.canvas().visible);

    rig.controller.set_shown(true);
    rig.ticks(1);
    assert!(rig.canvas().visible);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: TheEnabledRowOnThePageHidesTheCanvas
#[test]
fn the_enabled_row_on_the_page_hides_the_canvas() {
    let mut rig = Rig::with(LandscapeMapSettings {
        enabled: false,
        opacity: 16,
    });
    rig.controller.set_shown(true);
    rig.ticks(1);
    assert!(rig.ui.canvases.is_empty());

    rig.settings = LandscapeMapSettings {
        enabled: true,
        opacity: 16,
    };
    rig.ticks(20);
    assert!(rig.canvas().visible);

    rig.settings = LandscapeMapSettings {
        enabled: false,
        opacity: 16,
    };
    rig.ticks(20);
    assert!(!rig.canvas().visible);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: ARepaintIsAskedForOnlyWhenSomethingDrawnChanged
#[test]
fn a_repaint_is_asked_for_only_when_something_drawn_changed() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.ticks(1);
    assert_eq!(1, rig.canvas().invalidations);

    rig.ticks(60);
    assert_eq!(1, rig.canvas().invalidations);

    rig.controller.pan(10.0, 0.0);
    rig.ticks(1);
    assert_eq!(2, rig.canvas().invalidations);

    rig.controller.zoom_in();
    rig.ticks(1);
    assert_eq!(3, rig.canvas().invalidations);

    rig.set_player(|player| player.heading_degrees = 91.0);
    rig.ticks(1);
    assert_eq!(4, rig.canvas().invalidations);

    // A step too small to move the character's canvas pixel.
    rig.set_player(|player| player.east_west = 33.6001);
    rig.ticks(1);
    assert_eq!(4, rig.canvas().invalidations);

    // A step that does, while following.
    rig.toggle_follow();
    rig.ticks(1);
    assert_eq!(5, rig.canvas().invalidations);
    rig.set_player(|player| player.east_west = 34.6);
    rig.ticks(1);
    assert_eq!(6, rig.canvas().invalidations);

    let mut storage = marker_storage("Town, Holtburg, 42.1, 33.6\n");
    rig.controller.load_markers(&mut storage).unwrap();
    rig.ticks(1);
    assert_eq!(7, rig.canvas().invalidations);

    rig.settings = LandscapeMapSettings {
        enabled: true,
        opacity: 200,
    };
    rig.ticks(20);
    assert_eq!(8, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: ADragPansTheMapByItsDeltaAndStopsFollowing
#[test]
fn a_drag_pans_the_map_by_its_delta_and_stops_following() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.toggle_follow();
    rig.ticks(1);
    assert!(rig.canvas().spec.accepts_pointer_input);
    assert!(rig.controller.view().follow);
    let offset_x = rig.controller.view().offset_x();
    let offset_y = rig.controller.view().offset_y();
    let scale = rig.controller.view().scale();

    rig.down(100.0, 100.0);
    rig.move_to(140.0, 100.0);
    rig.move_to(140.0, 110.0);
    rig.up(140.0, 110.0);
    assert!(!rig.controller.view().follow);
    assert_eq!("Follow: off", rig.controller.follow_text());
    assert_close!(offset_x - 40.0 / scale, rig.controller.view().offset_x(), 6);
    assert_close!(offset_y - 10.0 / scale, rig.controller.view().offset_y(), 6);

    rig.ticks(1);
    assert_eq!(2, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: TheWheelZoomsAboutTheCursorKeepingThePositionUnderItFixed
#[test]
fn the_wheel_zooms_about_the_cursor_keeping_the_position_under_it_fixed() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.ticks(1);

    let (east_west, north_south) = rig.controller.view().to_world(200.0, 150.0);
    rig.wheel(200.0, 150.0, 1);
    assert_close!(0.08, rig.controller.view().zoom(), 6);
    let (east_after, north_after) = rig.controller.view().to_world(200.0, 150.0);
    assert_close!(east_west, east_after, 6);
    assert_close!(north_south, north_after, 6);
    rig.ticks(1);
    assert_eq!(2, rig.canvas().invalidations);

    rig.wheel(200.0, 150.0, -2);
    assert_close!(0.0, rig.controller.view().zoom(), 6);
    let (east_after, north_after) = rig.controller.view().to_world(200.0, 150.0);
    assert_close!(east_west, east_after, 6);
    assert_close!(north_south, north_after, 6);
    rig.ticks(1);
    assert_eq!(3, rig.canvas().invalidations);

    rig.wheel(200.0, 150.0, -1);
    rig.ticks(1);
    assert_eq!(3, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: TheReadoutFollowsTheCursorWhileItIsPressedOverTheMap
#[test]
fn the_readout_follows_the_cursor_while_it_is_pressed_over_the_map() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.ticks(1);
    let middle = rig.controller.readout();

    rig.down(40.0, 60.0);
    let (east_west, north_south) = rig.controller.view().to_world(40.0, 60.0);
    let under = view::position_text(east_west, north_south);
    assert_ne!(middle, under);
    assert_eq!(under, rig.controller.readout());
    rig.ticks(1);
    assert_eq!(2, rig.canvas().invalidations);
    assert!(rig.paint().texts.contains(&under));

    rig.up(40.0, 60.0);
    assert_eq!(middle, rig.controller.readout());
    rig.ticks(1);
    assert_eq!(3, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: ACancelledPressEndsTheDrag
#[test]
fn a_cancelled_press_ends_the_drag() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.ticks(1);
    let middle = rig.controller.readout();
    let offset_x = rig.controller.view().offset_x();

    rig.down(100.0, 100.0);
    rig.cancel(100.0, 100.0);
    assert_eq!(middle, rig.controller.readout());
    rig.move_to(140.0, 100.0);
    assert_close!(offset_x, rig.controller.view().offset_x(), 12);
    assert_eq!(middle, rig.controller.readout());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: ThePaintDrawsTheClientsMapArtWhenTheClientHasIt
#[test]
fn the_paint_draws_the_clients_map_art_when_the_client_has_it() {
    let mut rig = Rig::new();
    rig.ui.client_art.insert(MAP_ART_SURFACE_ID);
    rig.controller.set_shown(true);
    rig.ticks(1);

    let painter = rig.paint();
    assert_eq!("clear a=16", painter.ops[0]);
    assert_eq!(1, painter.images.len());
    assert!(rig.ui.requested.contains(&MAP_ART_SURFACE_ID));
    assert_eq!(1024, rig.controller.view().image_width());
    assert!(painter.ops.contains(&"line 255,0,255".to_owned()));
    assert!(painter.texts.contains(&rig.controller.readout()));
    assert!(painter.texts.iter().any(|text| text.ends_with('E')));
    assert!(painter.texts.iter().any(|text| text.ends_with('N')));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: ThePaintDrawsAGridWhenTheClientHasNoMapArt
#[test]
fn the_paint_draws_a_grid_when_the_client_has_no_map_art() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.ticks(1);

    let painter = rig.paint();
    assert!(painter.images.is_empty());
    assert!(painter.ops.contains(&"line 70,80,110".to_owned()));
    assert!(painter.texts.contains(&rig.controller.readout()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: MarkersDrawTheirIconsAndNamesInTheirOwnZoomBands
#[test]
fn markers_draw_their_icons_and_names_in_their_own_zoom_bands() {
    let mut rig = Rig::new();
    rig.ui
        .client_art
        .insert(markers::display_of(LandscapeMarkerKind::Vendor).icon_surface_id);
    let mut storage = marker_storage("Vendor, Aun Tanua, 42.1, 33.6\nTown, Far Away, -90, -90\n");
    rig.controller.load_markers(&mut storage).unwrap();
    rig.controller.set_shown(true);
    rig.toggle_follow();
    rig.ticks(1);

    let world = rig.paint();
    assert!(!world.texts.contains(&"Aun Tanua".to_owned()));
    assert!(!world.texts.contains(&"Far Away".to_owned()));
    assert!(world.images.is_empty());

    rig.controller.view_mut().set_zoom(0.5);
    let middle = rig.paint();
    assert_eq!(1, middle.images.len());
    assert!(!middle.texts.contains(&"Aun Tanua".to_owned()));

    rig.controller.view_mut().set_zoom(0.85);
    let near = rig.paint();
    assert_eq!(1, near.images.len());
    assert!(near.texts.contains(&"Aun Tanua".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: ASessionResetDropsTheImageHandlesAndAsksAgainOnTheNextPaint
#[test]
fn a_session_reset_drops_the_image_handles_and_asks_again_on_the_next_paint() {
    let mut rig = Rig::new();
    rig.ui.client_art.insert(MAP_ART_SURFACE_ID);
    rig.controller.set_shown(true);
    rig.ticks(1);
    rig.paint();
    rig.paint();
    assert_eq!(1, rig.ui.requested.len());

    rig.controller.reset();
    rig.paint();
    assert_eq!(2, rig.ui.requested.len());

    rig.controller.dispose(&mut rig.ui);
    assert!(rig.canvas().disposed);
    assert_eq!(1, rig.ui.released);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LandscapeMapControllerTests.cs :: FollowCentresOnTheCharacterAndAPanLetsGo
#[test]
fn follow_centres_on_the_character_and_a_pan_lets_go() {
    let mut rig = Rig::new();
    rig.controller.set_shown(true);
    rig.ticks(1);
    assert_eq!("Follow: off", rig.controller.follow_text());

    rig.toggle_follow();
    assert_eq!("Follow: on", rig.controller.follow_text());
    assert_close!(view::FOLLOW_ZOOM, rig.controller.view().zoom(), 12);
    assert_eq!("42.10N, 33.60E", rig.controller.readout());

    rig.controller.pan(40.0, 0.0);
    assert_eq!("Follow: off", rig.controller.follow_text());
    assert_ne!("42.10N, 33.60E", rig.controller.readout());
}
