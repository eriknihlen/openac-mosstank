//! The other clients: the peer features a meta asks for, and the clients
//! readout, against `PeerFeatureSubscriptionsTests` and
//! `NetworkHudControllerTests`.

use std::collections::HashMap;

use ac_plugin_abi::peers::NetworkClient;
use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use mosstank_formats::meta::{
    MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile, MetaRule,
};
use mosstank_ub::canvas::{MapCanvasSpec, MapImage, MapPainter, MapUi};
use mosstank_ub::hud::{
    self, HUD_WIDTH, NetworkHudController, NetworkHudSettings, NetworkHudSettingsSource, ROW_HEIGHT,
};
use mosstank_ub::network::{PeerFeatureSubscriptions, capabilities, meta_peer_requirements};
use mosstank_ub::testing::FakeUbHost;

// ---- PeerFeatureSubscriptionsTests ---------------------------------------------

fn active(host: &FakeUbHost) -> usize {
    host.peer_leases.len() - host.peer_unsubscribed.len()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PeerFeatureSubscriptionsTests.cs :: ClientsOwnTheirRequestsAndDisabledFeaturesReleaseThem
#[test]
fn clients_own_their_requests_and_disabled_features_release_them() {
    let mut first = FakeUbHost::new();
    let second = FakeUbHost::new();
    let mut one = PeerFeatureSubscriptions::new();
    let _two = PeerFeatureSubscriptions::new();
    one.update(&mut first, capabilities::NONE);
    assert!(first.peer_leases.is_empty());
    one.update(&mut first, capabilities::CASTS);
    one.update(&mut first, capabilities::CASTS);
    assert_eq!(1, first.peer_leases.len());
    assert!(second.peer_leases.is_empty());
    one.update(&mut first, capabilities::CASTS | capabilities::COMMANDS);
    assert_eq!(1, active(&first));
    one.update(&mut first, capabilities::NONE);
    assert_eq!(0, active(&first));
}

fn expression_condition(text: &str) -> MetaCondition {
    MetaCondition {
        kind: MetaConditionKind::Expression,
        text: text.to_owned(),
        ..MetaCondition::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PeerFeatureSubscriptionsTests.cs :: MetaRequirementsFollowEnabledNestedCallsRatherThanSimilarWords
#[test]
fn meta_requirements_follow_enabled_nested_calls_rather_than_similar_words() {
    let mut profile = MetaProfile::default();
    profile.rules.push(MetaRule {
        condition: expression_condition("netclients []"),
        action: MetaAction {
            children: vec![MetaAction {
                kind: MetaActionKind::ExpressionAction,
                text: "netcasts[tag]".to_owned(),
                ..MetaAction::default()
            }],
            ..MetaAction::default()
        },
        enabled: true,
        ..MetaRule::default()
    });
    profile.rules.push(MetaRule {
        condition: expression_condition("netclients[]"),
        enabled: false,
        ..MetaRule::default()
    });
    assert_eq!(
        capabilities::CLIENT_STATE | capabilities::CASTS,
        meta_peer_requirements(&profile)
    );
    profile.rules[0].enabled = false;
    assert_eq!(capabilities::NONE, meta_peer_requirements(&profile));
    profile.rules.push(MetaRule {
        condition: expression_condition("mynetclients[]"),
        enabled: true,
        ..MetaRule::default()
    });
    assert_eq!(capabilities::NONE, meta_peer_requirements(&profile));
}

// ---- NetworkHudControllerTests -------------------------------------------------

const SELF_ID: u32 = 0x5000_0001;

fn at(east_west: f64, north_south: f64, heading: f32) -> Position {
    Position {
        cell_id: 0xA9B4_0001,
        east_west,
        north_south,
        elevation: 0.0,
        heading_degrees: heading,
        is_outdoor: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn client_with(
    client_id: u32,
    player_id: u32,
    name: &str,
    position: Position,
    health: u32,
    max_health: u32,
    stamina: u32,
    mana: u32,
    tags: &[&str],
) -> NetworkClient {
    NetworkClient {
        client_id,
        player_id,
        name: name.to_owned(),
        world_name: "Coldeve".to_owned(),
        heading: position.heading_degrees,
        position,
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        current_health: health,
        current_mana: mana,
        current_stamina: stamina,
        max_health,
        max_mana: 100,
        max_stamina: 100,
        is_remote: false,
    }
}

fn client(client_id: u32, player_id: u32, name: &str, position: Position) -> NetworkClient {
    client_with(client_id, player_id, name, position, 100, 100, 50, 25, &[])
}

#[derive(Debug, Clone)]
struct FakeCanvas {
    handle: u32,
    spec: MapCanvasSpec,
    visible: bool,
    invalidations: usize,
}

#[derive(Debug, Default)]
struct FakeUi {
    canvases: Vec<FakeCanvas>,
    object_icons: HashMap<u32, MapImage>,
}

impl MapUi for FakeUi {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        let handle = u32::try_from(self.canvases.len()).unwrap() + 1;
        self.canvases.push(FakeCanvas {
            handle,
            spec: spec.clone(),
            visible: spec.start_visible,
            invalidations: 0,
        });
        Some(handle)
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvases[usize::try_from(canvas).unwrap() - 1].visible = visible;
    }

    fn invalidate_canvas(&mut self, canvas: u32) {
        self.canvases[usize::try_from(canvas).unwrap() - 1].invalidations += 1;
    }

    fn remove_canvas(&mut self, _canvas: u32) {}

    fn client_art(&mut self, _surface_id: u32) -> MapImage {
        MapImage::default()
    }

    fn object_icon(&mut self, object_id: u32) -> MapImage {
        self.object_icons
            .get(&object_id)
            .copied()
            .unwrap_or_default()
    }

    fn release_image(&mut self, _image: MapImage) {}
}

/// `LandscapeMapControllerTests.RecordingPainter`: the operations by name,
/// the texts and the images.
#[derive(Debug, Default)]
struct RecordingPainter {
    ops: Vec<String>,
    texts: Vec<String>,
    images: Vec<MapImage>,
    clip_depth: i32,
}

impl MapPainter for RecordingPainter {
    fn width(&self) -> i32 {
        HUD_WIDTH
    }

    fn height(&self) -> i32 {
        hud::CANVAS_HEIGHT
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

    fn draw_line(&mut self, _from: (f64, f64), _to: (f64, f64), _color: &Color, _thickness: f32) {
        self.ops.push("line".to_owned());
    }

    fn draw_text(&mut self, text: &str, _at: (f64, f64), _color: &Color, _outline: bool) {
        self.ops.push("text".to_owned());
        self.texts.push(text.to_owned());
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        (text.len() as f64 * 6.0, 12.0)
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

struct Settings(NetworkHudSettings);

impl NetworkHudSettingsSource for Settings {
    fn read(&self) -> NetworkHudSettings {
        self.0.clone()
    }
}

struct Rig {
    host: FakeUbHost,
    ui: FakeUi,
    settings: Settings,
    controller: NetworkHudController,
}

impl Rig {
    fn new() -> Self {
        Self::with(NetworkHudSettings {
            enabled: true,
            ..NetworkHudSettings::default()
        })
    }

    fn with(settings: NetworkHudSettings) -> Self {
        let mut host = FakeUbHost::new();
        {
            let me = host.meta.character_mut();
            me.id = SELF_ID;
            "Acdream".clone_into(&mut me.name);
            me.health = (100, 100, 100);
            me.stamina = (50, 100, 100);
            me.mana = (25, 100, 100);
        }
        host.meta.navigation.is_available = true;
        host.meta.navigation.position = at(0.0, 0.0, 0.0);
        let settings = Settings(settings);
        let controller = NetworkHudController::new(&settings);
        Self {
            host,
            ui: FakeUi::default(),
            settings,
            controller,
        }
    }

    fn ticks(&mut self, count: usize) {
        for _ in 0..count {
            self.controller
                .on_tick(&mut self.host, &self.settings, &mut self.ui, 0.015);
        }
    }

    fn canvas(&self) -> &FakeCanvas {
        assert_eq!(1, self.ui.canvases.len());
        &self.ui.canvases[0]
    }

    fn paint(&self) -> RecordingPainter {
        let mut painter = RecordingPainter::default();
        self.controller.paint(&mut painter);
        assert_eq!(0, painter.clip_depth);
        painter
    }

    fn names(&self) -> Vec<&str> {
        self.controller
            .rows()
            .iter()
            .map(|row| row.name.as_str())
            .collect()
    }

    fn press(&mut self, y: f64, shift: bool) {
        assert!(self.canvas().spec.accepts_pointer_input);
        #[allow(clippy::cast_possible_truncation)]
        let event = PointerEvent {
            canvas: self.canvas().handle,
            kind: 0,
            x: 50.0,
            y: y as f32,
            button: 1,
            modifiers: u8::from(shift),
            wheel_delta: 0,
        };
        self.controller.on_pointer(&mut self.host, &event);
    }

    fn selected(&self) -> Vec<u32> {
        self.host.selections.clone()
    }
}

fn owned(id: u32, name: &str, stack: i32, is_owned: bool) -> WorldObject {
    WorldObject {
        object_id: id,
        weenie_class_id: 1,
        name: name.to_owned(),
        object_class: ObjectClass::SpellComponent,
        is_owned,
        stack_size: stack,
        ..WorldObject::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: RowsAreTheClientsCarryingTheSelectedTagAndAllListsEveryOne
#[test]
fn rows_are_the_clients_carrying_the_selected_tag_and_all_lists_every_one() {
    let mut rig = Rig::new();
    rig.host.meta.peers.push(client_with(
        1,
        0x5000_0002,
        "Sigrid",
        at(0.01, 0.0, 0.0),
        100,
        100,
        50,
        25,
        &["tanks"],
    ));
    rig.host.meta.peers.push(client_with(
        2,
        0x5000_0003,
        "Ulf",
        at(0.02, 0.0, 0.0),
        100,
        100,
        50,
        25,
        &["healers"],
    ));
    rig.ticks(1);
    assert_eq!(vec!["Acdream", "Sigrid", "Ulf"], rig.names());

    "tanks".clone_into(&mut rig.settings.0.selected_tag);
    rig.ticks(20);
    assert_eq!(vec!["Acdream", "Sigrid"], rig.names());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: TheSelectedTagMatchesRegardlessOfCase
#[test]
fn the_selected_tag_matches_regardless_of_case() {
    let mut rig = Rig::new();
    rig.host.meta.peers.push(client_with(
        1,
        0x5000_0002,
        "Sigrid",
        at(0.01, 0.0, 0.0),
        100,
        100,
        50,
        25,
        &["Tanks"],
    ));
    "tanks".clone_into(&mut rig.settings.0.selected_tag);
    rig.ticks(20);
    assert_eq!(vec!["Acdream", "Sigrid"], rig.names());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: TheReadoutRepaintsOnlyWhenARowChangedAndAtMostTwentyTimesASecond
#[test]
fn the_readout_repaints_only_when_a_row_changed_and_at_most_twenty_times_a_second() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .peers
        .push(client(1, 0x5000_0002, "Sigrid", at(0.01, 0.0, 0.0)));
    rig.ticks(1);
    assert_eq!(1, rig.canvas().invalidations);
    let captures = rig.host.peer_capture_count;

    // A second of 15 ms frames: a rebuild every fourth frame, never more
    // often than twenty a second, never so rarely as ten.
    rig.ticks(66);
    assert_eq!(1, rig.canvas().invalidations);
    let rebuilt = rig.host.peer_capture_count - captures;
    assert!((11..=20).contains(&rebuilt), "{rebuilt}");

    rig.host.meta.peers[0] = client_with(
        1,
        0x5000_0002,
        "Sigrid",
        at(0.01, 0.0, 0.0),
        90,
        100,
        50,
        25,
        &[],
    );
    rig.ticks(1);
    rig.host.meta.peers[0] = client_with(
        1,
        0x5000_0002,
        "Sigrid",
        at(0.01, 0.0, 0.0),
        80,
        100,
        50,
        25,
        &[],
    );
    rig.ticks(1);
    assert_eq!(2, rig.canvas().invalidations);
    assert_eq!("80%", rig.controller.rows()[1].health_text);

    rig.ticks(4);
    assert_eq!(2, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: TheArrowIsTheBearingLessTheFacing
#[test]
fn the_arrow_is_the_bearing_less_the_facing() {
    for (facing, east, north, degrees) in [
        (0.0, 1.0, 0.0, 90.0),
        (90.0, 1.0, 0.0, 0.0),
        (0.0, 0.0, 1.0, 0.0),
        (0.0, 0.0, -1.0, 180.0),
        (180.0, 0.0, -1.0, 0.0),
        (0.0, -1.0, 0.0, 270.0),
    ] {
        let radians = hud::arrow_radians(&at(0.0, 0.0, facing), &at(east, north, 0.0));
        let got = radians * 180.0 / std::f64::consts::PI;
        assert!(
            (got - degrees).abs() < 5e-7,
            "{facing} {east} {north}: {got}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: TheTintLosesTwoPerMetreAndStopsAtRed
#[test]
fn the_tint_loses_two_per_metre_and_stops_at_red() {
    for (meters, tint) in [(0.0, 255), (50.0, 155), (127.5, 0), (500.0, 0)] {
        assert_eq!(tint, hud::distance_tint(meters));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: NumbersAreWholeUpToAThousandAndThousandsBeyond
#[test]
fn numbers_are_whole_up_to_a_thousand_and_thousands_beyond() {
    for (number, text) in [
        (0.0, "0"),
        (999.4, "999"),
        (1500.0, "1.5k"),
        (12345.0, "12k"),
    ] {
        assert_eq!(text, hud::format_number(number));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: ARowCarriesTheVitalsAndTheDistanceAndTheOwnRowNoDistance
#[test]
#[allow(clippy::float_cmp)]
fn a_row_carries_the_vitals_and_the_distance_and_the_own_row_no_distance() {
    let mut rig = Rig::new();
    rig.host.meta.navigation.position = at(0.0, 0.0, 90.0);
    // One map unit east is 240 m.
    rig.host.meta.peers.push(client_with(
        1,
        0x5000_0002,
        "Sigrid",
        at(1.0, 0.0, 0.0),
        40,
        80,
        50,
        25,
        &[],
    ));
    rig.host
        .meta
        .peers
        .push(client(2, SELF_ID, "Me", at(0.0, 0.0, 0.0)));
    rig.ticks(1);

    assert_eq!(2, rig.controller.rows().len());
    let sigrid = &rig.controller.rows()[1];
    assert_eq!(0.5, sigrid.health_fraction);
    assert_eq!("50%", sigrid.health_text);
    assert_eq!(0.5, sigrid.stamina_fraction);
    assert_eq!("50%", sigrid.stamina_text);
    assert_eq!(0.25, sigrid.mana_fraction);
    assert_eq!("25%", sigrid.mana_text);
    assert_eq!("240", sigrid.distance_text);
    assert!(sigrid.arrow_radians.abs() < 5e-7);
    assert_eq!(0, sigrid.distance_tint);
    assert!(!sigrid.is_self);

    let me = &rig.controller.rows()[0];
    assert!(me.is_self);
    assert_eq!("Acdream", me.name);
    assert_eq!("", me.distance_text);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: TheOwnRowCountsTrackedItemsFromTheInventoryWithTheirIcons
#[test]
fn the_own_row_counts_tracked_items_from_the_inventory_with_their_icons() {
    let mut rig = Rig::new();
    "Prismatic Taper\n\nHealing Kit\n".clone_into(&mut rig.settings.0.tracked_items);
    rig.host
        .meta
        .objects
        .push(owned(0x8000_0010, "Prismatic Taper", 40, true));
    rig.host
        .meta
        .objects
        .push(owned(0x8000_0011, "Prismatic Taper", 12, true));
    rig.host
        .meta
        .objects
        .push(owned(0x8000_0012, "Prismatic Taper", 99, false));
    rig.ui.object_icons.insert(
        0x8000_0010,
        MapImage {
            handle: 7,
            width: 32,
            height: 32,
        },
    );
    rig.host
        .meta
        .peers
        .push(client(1, 0x5000_0002, "Sigrid", at(0.01, 0.0, 0.0)));
    rig.ticks(1);

    let me = &rig.controller.rows()[0];
    assert!(me.is_self);
    assert_eq!(2, me.tracked.len());
    assert_eq!("52", me.tracked[0].count);
    assert_eq!(7, me.tracked[0].icon.handle);
    assert_eq!("0", me.tracked[1].count);
    assert!(!me.tracked[1].icon.is_valid());
    assert!(rig.controller.rows()[1].tracked.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: SelectingARowSelectsThatCharacter
#[test]
fn selecting_a_row_selects_that_character() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .peers
        .push(client(1, 0x5000_0002, "Sigrid", at(0.01, 0.0, 0.0)));
    rig.ticks(1);

    assert_eq!(
        Some("Sigrid".to_owned()),
        rig.controller.select_row(&mut rig.host, 1)
    );
    assert_eq!(vec![0x5000_0002], rig.selected());
    assert_eq!(
        Some("Acdream".to_owned()),
        rig.controller.select_row(&mut rig.host, 0)
    );
    assert_eq!(vec![0x5000_0002, SELF_ID], rig.selected());
    assert_eq!(None, rig.controller.select_row(&mut rig.host, 2));
    assert_eq!(2, rig.selected().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: AShiftClickOnARowSelectsThatCharacter
#[test]
fn a_shift_click_on_a_row_selects_that_character() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .peers
        .push(client(1, 0x5000_0002, "Sigrid", at(0.01, 0.0, 0.0)));
    rig.host
        .meta
        .peers
        .push(client(2, 0x5000_0003, "Ulf", at(0.02, 0.0, 0.0)));
    rig.ticks(1);
    assert!(rig.canvas().spec.accepts_pointer_input);
    assert_eq!(1, rig.canvas().invalidations);

    rig.press(25.0, true);
    assert_eq!(vec![0x5000_0002], rig.selected());

    rig.press(5.0, false);
    assert_eq!(1, rig.selected().len());

    rig.press(f64::from(ROW_HEIGHT) * 3.0 + 1.0, true);
    assert_eq!(1, rig.selected().len());

    rig.press(19.9, true);
    assert_eq!(vec![0x5000_0002, SELF_ID], rig.selected());

    rig.ticks(1);
    assert_eq!(1, rig.canvas().invalidations);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: TheCanvasShowsWhileOnAndEitherOpenOrAllowedToStay
#[test]
fn the_canvas_shows_while_on_and_either_open_or_allowed_to_stay() {
    let mut rig = Rig::with(NetworkHudSettings::default());
    rig.ticks(1);
    assert!(rig.ui.canvases.is_empty());

    rig.settings.0.enabled = true;
    rig.settings.0.show_hud_when_closed = true;
    rig.ticks(20);
    assert!(rig.canvas().visible);

    rig.settings.0.show_hud_when_closed = false;
    rig.ticks(20);
    assert!(!rig.canvas().visible);

    rig.controller.set_shown(true);
    rig.ticks(1);
    assert!(rig.canvas().visible);

    rig.settings.0.enabled = false;
    rig.ticks(20);
    assert!(!rig.canvas().visible);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: ThePaintDrawsBarsTextArrowsAndTrackedCounts
#[test]
fn the_paint_draws_bars_text_arrows_and_tracked_counts() {
    let mut rig = Rig::new();
    "Prismatic Taper".clone_into(&mut rig.settings.0.tracked_items);
    rig.host
        .meta
        .objects
        .push(owned(0x8000_0010, "Prismatic Taper", 40, true));
    rig.ui.object_icons.insert(
        0x8000_0010,
        MapImage {
            handle: 7,
            width: 32,
            height: 32,
        },
    );
    rig.host
        .meta
        .peers
        .push(client(1, 0x5000_0002, "Sigrid", at(0.01, 0.0, 0.0)));
    rig.ticks(1);
    let both = rig.paint();
    assert_eq!(12, both.ops.iter().filter(|op| *op == "fill").count());
    assert_eq!(
        3,
        both.ops.iter().filter(|op| op.starts_with("line")).count()
    );
    for text in ["Acdream", "Sigrid", "100%", "50%", "25%", "2", "40"] {
        assert!(both.texts.iter().any(|t| t == text), "{text}");
    }
    assert_eq!(1, both.images.len());
    assert_eq!(7, both.images[0].handle);

    {
        let me = rig.host.meta.character_mut();
        me.id = 0;
        me.name.clear();
    }
    rig.ticks(4);
    let peer_only = rig.paint();
    assert_eq!(6, peer_only.ops.iter().filter(|op| *op == "fill").count());
    assert_eq!(
        3,
        peer_only
            .ops
            .iter()
            .filter(|op| op.starts_with("line"))
            .count()
    );
    assert!(!peer_only.texts.iter().any(|t| t == "Acdream"));
    assert!(!peer_only.texts.iter().any(|t| t == "40"));
    assert!(peer_only.images.is_empty());
}

// Not a port yet: its C# counterpart (NetworkHudControllerTests.cs,
// AnotherClientsRowIsLabelledWithTheNameAlone) comes with the C# line's fix.
/// Another client's row is labelled with the character's name and nothing
/// else: the client's number in front of it read as a list of ids before
/// the names.
#[test]
fn another_clients_row_is_labelled_with_the_name_alone() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .peers
        .push(client(3, 0x5000_0002, "Sigrid", at(0.01, 0.0, 0.0)));
    rig.ticks(1);

    let painter = rig.paint();
    assert!(painter.texts.iter().any(|t| t == "Sigrid"));
    assert!(!painter.texts.iter().any(|t| t.ends_with(" Sigrid")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NetworkHudControllerTests.cs :: AloneTheReadoutIsOneRowWithTheLocalName
#[test]
#[allow(clippy::float_cmp)]
fn alone_the_readout_is_one_row_with_the_local_name() {
    let mut rig = Rig::new();
    "Prismatic Taper".clone_into(&mut rig.settings.0.tracked_items);
    rig.host
        .meta
        .objects
        .push(owned(0x8000_0010, "Prismatic Taper", 40, true));
    rig.host.peers_connected = false;
    rig.ticks(1);

    assert_eq!(1, rig.controller.rows().len());
    let me = &rig.controller.rows()[0];
    assert_eq!("Acdream", me.name);
    assert!(me.is_self);
    assert_eq!(SELF_ID, me.player_id);
    assert_eq!(1.0, me.health_fraction);
    assert_eq!("50%", me.stamina_text);
    assert_eq!("25%", me.mana_text);
    assert_eq!("", me.distance_text);
    assert_eq!(1, me.tracked.len());
    assert_eq!("40", me.tracked[0].count);

    let painter = rig.paint();
    assert!(painter.texts.iter().any(|t| t == "Acdream"));
    assert!(!painter.texts.iter().any(|t| t.ends_with(" Acdream")));
}
