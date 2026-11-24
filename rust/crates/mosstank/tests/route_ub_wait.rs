//! A route's chat waypoint that starts a UB tool waits for it: the next
//! waypoint begins only once the tool has picked the line up and finished.
//!
//! Not a port of a C# test. The live bug (2026-11-24): a `/ub givep` waypoint
//! started a hand-over of 41 items that took 7 s; the route's next waypoint, a
//! 5 s pause, ran out under it, and the next `/ub givep` was refused with
//! "Already running" and lost. With 1 ms pauses between three gives the
//! second and the third were always refused.
//!
//! The client runs a line the plugin submits after the call that submitted
//! it; a line naming the plugin's own verb comes back as the plugin's own
//! command before its next tick. [`Rig::frame`] does that.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus};
use ac_plugin_abi::navigation::{NavigationSnapshot, NavigationStatus};
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use common::World;
use mosstank::MossTank;
use mosstank_formats::route::{NavigationSettings, RouteMode, RouteWaypoint, RouteWaypointType};

const PYREAL: u32 = 0x8000_0010;
const TAPER: u32 = 0x8000_0011;
const MOTE: u32 = 0x8000_0012;
const ALREADY_RUNNING: &str = "[UB] Error: InventoryManager: Already running.  Please wait until it completes or use /ub give stop to quit previous session";

fn here() -> Position {
    Position {
        cell_id: 0x0001_0001,
        is_outdoor: true,
        ..Position::default()
    }
}

fn bare(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: 0x5000_0001,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn chat(text: &str) -> RouteWaypoint {
    RouteWaypoint {
        kind: RouteWaypointType::ChatCommand,
        position: here(),
        text: text.to_owned(),
        ..RouteWaypoint::default()
    }
}

fn pause(milliseconds: i32) -> RouteWaypoint {
    RouteWaypoint {
        kind: RouteWaypointType::Pause,
        position: here(),
        duration_milliseconds: milliseconds,
        ..RouteWaypoint::default()
    }
}

/// The plugin with a route loaded and the macro running, a player to give
/// to standing beside the character, and the client taking every give.
struct Rig {
    world: World,
    plugin: MossTank,
    /// How many of the submitted lines the client has run.
    handed_back: usize,
    /// What the character carries.
    owned: Vec<InventoryItem>,
}

impl Rig {
    fn new(waypoints: Vec<RouteWaypoint>, owned: Vec<InventoryItem>) -> Self {
        let mut world = World::panel_character("Test Character");
        world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
            is_available: true,
            local_object_id: 1,
            position: here(),
            ..NavigationSnapshot::default()
        });
        let accepted = NavigationStatus::Accepted;
        world.set_reply::<calls::NavigationSetIntent>(&accepted);
        world.set_reply::<calls::NavigationClearIntent>(&accepted);
        world.set_reply::<calls::NavigationFace>(&accepted);
        let mule = WorldObject {
            object_id: 0x5000_0ABC,
            name: "Mule".to_owned(),
            object_class: ObjectClass::Player,
            position: Some(here()),
            ..WorldObject::default()
        };
        world.set_reply::<calls::WorldObjects>(&vec![mule.clone()]);
        world.set_reply::<calls::WorldObject>(&Some(mule));
        world.set_reply::<calls::ItemsOwned>(&owned);
        world.set_reply::<calls::ItemsGive>(&ItemResult::of(ItemStatus::Started));
        let route = NavigationSettings {
            enabled: true,
            mode: RouteMode::Once,
            waypoints,
            ..NavigationSettings::default()
        };
        let saved = mosstank_formats::nav::save(&route).unwrap();
        world.profiles.insert("mosstank/navs/Gives.nav", &saved);
        let mut plugin = world.enable();
        world.vt(&mut plugin, "nav load Gives");
        world.vt(&mut plugin, "opt set EnableNav true");
        world.with_tank(&mut plugin, |tank, client| {
            tank.set_macro_running(client, true);
        });
        world.clear_chat();
        Self {
            world,
            plugin,
            handed_back: 0,
            owned,
        }
    }

    /// One pass of the client: the lines submitted since the last one run
    /// (a `/ub` line comes back as the plugin's command), then the tick.
    fn frame(&mut self) {
        let submitted = self.world.requests::<calls::ChatSubmit>();
        for line in &submitted[self.handed_back..] {
            if let Some(arguments) = line.strip_prefix("/ub ") {
                self.world.command(&mut self.plugin, "ub", arguments);
            }
        }
        self.handed_back = submitted.len();
        self.world.tick(&mut self.plugin);
    }

    /// Frames for so many seconds.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn run_for(&mut self, seconds: f64) {
        let frames = (seconds / ac_plugin_abi::TICK_SECONDS).round() as usize;
        for _ in 0..frames {
            self.frame();
        }
    }

    /// Frames until `done`, at most `seconds` of them; whether it came.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn run_until(&mut self, seconds: f64, mut done: impl FnMut(&Self) -> bool) -> bool {
        let frames = (seconds / ac_plugin_abi::TICK_SECONDS).round() as usize;
        for _ in 0..frames {
            if done(self) {
                return true;
            }
            self.frame();
        }
        done(self)
    }

    /// The `/ub` lines the plugin submitted.
    fn submitted(&self) -> Vec<String> {
        self.world
            .requests::<calls::ChatSubmit>()
            .into_iter()
            .filter(|line| line.starts_with("/ub "))
            .collect()
    }

    fn given(&self) -> Vec<u32> {
        self.world
            .requests::<calls::ItemsGive>()
            .iter()
            .map(|give| give.object_id)
            .collect()
    }

    /// The character no longer has `object_id`: the give landed.
    fn hand_over(&mut self, object_id: u32) {
        self.owned.retain(|item| item.object_id != object_id);
        self.world.set_reply::<calls::ItemsOwned>(&self.owned);
    }

    fn refused(&self) -> bool {
        self.world.chat().iter().any(|line| line == ALREADY_RUNNING)
    }

    /// Every item asked for has landed: the client gives at once.
    fn hand_over_what_was_given(&mut self) {
        for object_id in self.given() {
            self.hand_over(object_id);
        }
    }

    /// Frames for at most `seconds`, the client giving at once, until
    /// `done`; whether it came.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn give_until(&mut self, seconds: f64, mut done: impl FnMut(&Self) -> bool) -> bool {
        let frames = (seconds / ac_plugin_abi::TICK_SECONDS).round() as usize;
        for _ in 0..frames {
            if done(self) {
                return true;
            }
            self.frame();
            self.hand_over_what_was_given();
        }
        done(self)
    }

    /// How many walks forward the route asked the client for.
    fn forward_walks(&self) -> usize {
        self.world
            .requests::<calls::NavigationSetIntent>()
            .iter()
            .filter(|intent| intent.forward)
            .count()
    }

    /// How often the plugin let go of the movement keys.
    fn stops(&self) -> usize {
        self.world.requests::<calls::NavigationClearIntent>().len()
    }

    fn set_macro_running(&mut self, running: bool) {
        self.world.with_tank(&mut self.plugin, |tank, client| {
            tank.set_macro_running(client, running);
        });
    }
}

/// A point the character never reaches.
fn far_point() -> RouteWaypoint {
    RouteWaypoint {
        kind: RouteWaypointType::Point,
        position: Position {
            north_south: 60.0,
            ..here()
        },
        ..RouteWaypoint::default()
    }
}

/// Two gives with a pause of `milliseconds` between them: the second is
/// sent only after the first has finished, and neither is refused.
fn chained_gives_wait_for_each_other(milliseconds: i32) {
    let mut rig = Rig::new(
        vec![
            chat("/ub give Pyreal to Mule"),
            pause(milliseconds),
            chat("/ub give Taper to Mule"),
        ],
        vec![bare(PYREAL, "Pyreal"), bare(TAPER, "Taper")],
    );

    assert!(rig.run_until(2.0, |rig| rig.given() == [PYREAL]));
    // The first give is under way: the pyreal has not left the packs yet.
    rig.run_for(3.0);
    assert_eq!(
        rig.submitted(),
        ["/ub give Pyreal to Mule"],
        "{:?}",
        rig.world.chat()
    );
    assert!(!rig.refused(), "{:?}", rig.world.chat());

    rig.hand_over(PYREAL);
    assert!(rig.run_until(3.0, |rig| rig.given() == [PYREAL, TAPER]));
    assert_eq!(
        rig.submitted(),
        ["/ub give Pyreal to Mule", "/ub give Taper to Mule"]
    );
    assert!(!rig.refused(), "{:?}", rig.world.chat());
}

#[test]
fn two_gives_with_no_pause_between_them_wait_for_each_other() {
    chained_gives_wait_for_each_other(0);
}

#[test]
fn two_gives_one_millisecond_apart_wait_for_each_other() {
    chained_gives_wait_for_each_other(1);
}

/// The live case: a give of 7 s, then a 5 s pause, then the next give. The
/// pause counts only once the give is done, so the next give is sent 5 s
/// after it, and not refused.
#[test]
fn a_pause_after_a_long_give_counts_from_the_end_of_the_give() {
    let mut rig = Rig::new(
        vec![
            chat("/ub give Pyreal to Mule"),
            pause(5000),
            chat("/ub give Taper to Mule"),
        ],
        vec![bare(PYREAL, "Pyreal"), bare(TAPER, "Taper")],
    );

    assert!(rig.run_until(2.0, |rig| rig.given() == [PYREAL]));
    rig.run_for(7.0);
    assert_eq!(rig.submitted().len(), 1, "{:?}", rig.world.chat());
    rig.hand_over(PYREAL);

    // The pause runs its whole 5 s from here.
    rig.run_for(4.5);
    assert_eq!(rig.submitted().len(), 1, "{:?}", rig.submitted());
    assert!(rig.run_until(1.5, |rig| rig.given() == [PYREAL, TAPER]));
    assert!(!rig.refused(), "{:?}", rig.world.chat());
}

/// `/ub give stop` ends the give, and the route goes on at once.
#[test]
fn stopping_the_give_lets_the_route_go_on() {
    let mut rig = Rig::new(
        vec![
            chat("/ub give Pyreal to Mule"),
            pause(0),
            chat("/ub give Mote to Mule"),
        ],
        vec![bare(PYREAL, "Pyreal"), bare(MOTE, "Mote")],
    );

    assert!(rig.run_until(2.0, |rig| rig.given() == [PYREAL]));
    rig.run_for(2.0);
    assert_eq!(rig.submitted().len(), 1);

    rig.world.command(&mut rig.plugin, "ub", "give stop");
    assert!(rig.run_until(1.0, |rig| rig.given() == [PYREAL, MOTE]));
    assert!(!rig.refused(), "{:?}", rig.world.chat());
}

/// UtilityBelt locks the route's navigation while a give runs: a give typed
/// by hand (or started by a meta) stops a route walking to its point, and
/// the route walks on once the give is done.
#[test]
fn a_give_typed_by_hand_stops_the_route_walking_until_it_is_done() {
    let mut rig = Rig::new(
        vec![far_point()],
        vec![bare(PYREAL, "Pyreal"), bare(TAPER, "Taper")],
    );
    assert!(rig.run_until(2.0, |rig| rig.forward_walks() > 0));
    let stops = rig.stops();

    rig.world
        .command(&mut rig.plugin, "ub", "give Pyreal to Mule");
    rig.run_for(0.5);
    assert!(rig.stops() > stops, "the walk is stopped for the give");
    let walks = rig.forward_walks();
    rig.run_for(3.0);
    assert_eq!(rig.forward_walks(), walks, "no walk while the give runs");

    rig.hand_over(PYREAL);
    assert!(
        rig.run_until(3.0, |rig| rig.forward_walks() > walks),
        "the route walks on after the give"
    );
}

/// The live case's hand-over by profile: 41 items given one at a time, and
/// the give after them waits for the last.
#[test]
fn a_profile_give_of_41_items_is_waited_for() {
    use mosstank_formats::utl::{self, LootAction, LootRule};

    let mut owned: Vec<InventoryItem> = (0..41)
        .map(|index| bare(0x8000_0100 + index, "Mote"))
        .collect();
    owned.push(bare(TAPER, "Taper"));
    let mut rig = Rig::new(
        vec![
            chat("/ub ig Motes to Mule"),
            pause(1),
            chat("/ub give Taper to Mule"),
        ],
        owned,
    );
    // The profile keeps the motes and not the taper.
    let mut keep = LootRule::named("Motes", "name ~= Mote");
    keep.action = LootAction::Keep;
    rig.world.profiles.insert(
        "mosstank/ub/itemgiver/Motes.utl",
        &utl::serialize_rules(&[keep]),
    );

    assert!(rig.give_until(60.0, |rig| rig.given().contains(&TAPER)));
    let given = rig.given();
    assert_eq!(given.len(), 42, "{given:x?}");
    assert_eq!(given.last(), Some(&TAPER));
    assert_eq!(
        rig.submitted(),
        ["/ub ig Motes to Mule", "/ub give Taper to Mule"]
    );
    assert!(!rig.refused(), "{:?}", rig.world.chat());
}

/// Another `/ub` line run before the client hands the route's own back (a
/// meta's, in the same frame) does not let the route go on: it walks to the
/// point after the give only once the give is done.
#[test]
fn another_ub_line_in_the_same_frame_does_not_end_the_wait() {
    let mut rig = Rig::new(
        vec![chat("/ub give Pyreal to Mule"), far_point()],
        vec![bare(PYREAL, "Pyreal")],
    );
    assert!(rig.run_until(2.0, |rig| !rig.submitted().is_empty()));
    // The route's line is out; another runs first, and the plugin ticks
    // before the client hands the route's line back.
    rig.handed_back = rig.world.requests::<calls::ChatSubmit>().len();
    rig.world.command(&mut rig.plugin, "ub", "listvars");
    for _ in 0..20 {
        rig.world.tick(&mut rig.plugin);
    }
    assert_eq!(rig.forward_walks(), 0, "the route waits for its own line");

    rig.world
        .command(&mut rig.plugin, "ub", "give Pyreal to Mule");
    rig.run_for(2.0);
    assert_eq!(rig.forward_walks(), 0, "and then for the give");
    rig.hand_over(PYREAL);
    assert!(rig.run_until(3.0, |rig| rig.forward_walks() > 0));
}

/// A `/ub delay` line holds the route until the line it put off has run, and
/// the give that line starts is waited for.
#[test]
fn a_delayed_give_is_waited_for() {
    let mut rig = Rig::new(
        vec![
            chat("/ub delay 500 /ub give Pyreal to Mule"),
            pause(0),
            chat("/ub give Taper to Mule"),
        ],
        vec![bare(PYREAL, "Pyreal"), bare(TAPER, "Taper")],
    );
    assert!(rig.run_until(2.0, |rig| rig.given() == [PYREAL]));
    rig.run_for(2.0);
    assert_eq!(rig.submitted().len(), 2, "{:?}", rig.submitted());
    rig.hand_over(PYREAL);
    assert!(rig.run_until(2.0, |rig| rig.given() == [PYREAL, TAPER]));
    assert!(!rig.refused(), "{:?}", rig.world.chat());
}

/// The macro stopped and started again while a chat waypoint waits on its
/// give: the line is not sent again, and the route goes on after the give.
#[test]
fn a_waypoint_waiting_across_a_macro_stop_does_not_send_its_line_again() {
    let mut rig = Rig::new(
        vec![
            chat("/ub give Pyreal to Mule"),
            pause(0),
            chat("/ub give Taper to Mule"),
        ],
        vec![bare(PYREAL, "Pyreal"), bare(TAPER, "Taper")],
    );
    assert!(rig.run_until(2.0, |rig| rig.given() == [PYREAL]));
    rig.set_macro_running(false);
    rig.run_for(1.0);
    rig.set_macro_running(true);
    rig.run_for(2.0);
    assert_eq!(rig.submitted(), ["/ub give Pyreal to Mule"]);

    rig.hand_over(PYREAL);
    assert!(rig.run_until(3.0, |rig| rig.given() == [PYREAL, TAPER]));
    assert_eq!(
        rig.submitted(),
        ["/ub give Pyreal to Mule", "/ub give Taper to Mule"]
    );
    assert!(!rig.refused(), "{:?}", rig.world.chat());
}
