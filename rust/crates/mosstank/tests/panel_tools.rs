//! The UB tools through the whole shell: the counter, the login roster, the
//! item giver, the jumper and the face command, with what they say and
//! think (ported from the C#'s `MossTankPanelTests`, over its
//! `FakeAutomation`).

mod common;

use ac_plugin_abi::navigation::{MoveReport, NavigationSnapshot, NavigationStatus};
use ac_plugin_abi::social::{LoginCharacter, LoginState};
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use ac_plugin_abi::{Import, Wire, calls};
use common::World;
use common::automation::FakeAutomation;
use mosstank::MossTank;
use mosstank_automation::locks::ActionLockKind;

/// `NavigationAt(heading)`.
fn navigation_at(heading: f32) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x0001_0001,
            heading_degrees: heading,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    }
}

/// `FakeAutomation { NavigationSnapshot = NavigationAt(heading) }`.
fn at(heading: f32) -> FakeAutomation {
    let mut automation = FakeAutomation::default();
    automation.navigation = navigation_at(heading);
    automation
}

/// The world over `automation`, the client taking every movement.
fn open(automation: FakeAutomation) -> (World, MossTank) {
    let mut world = World::with_automation(automation);
    let accepted = NavigationStatus::Accepted;
    world.set_reply::<calls::NavigationJump>(&accepted);
    world.set_reply::<calls::NavigationMove>(&accepted);
    world.set_reply::<calls::NavigationFace>(&accepted);
    let plugin = world.enable();
    (world, plugin)
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn ub(world: &mut World, plugin: &mut MossTank, arguments: &str) {
    world.command(plugin, "ub", arguments);
}

/// `Item(id, name, itemType)`.
fn item(object_id: u32, name: &str, item_type: u32) -> ac_plugin_abi::items::InventoryItem {
    common::item(object_id, name, item_type)
}

/// A player on the landscape at `position`.
fn player(object_id: u32, name: &str, position: Position) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Player,
        is_landscape: true,
        position: Some(position),
        ..WorldObject::default()
    }
}

/// `Carried(id, name)`: an owned, appraised item as the object table holds
/// it.
fn carried(object_id: u32, name: &str) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::SpellComponent,
        container_id: 1,
        is_owned: true,
        has_appraisal_data: true,
        ..WorldObject::default()
    }
}

/// The mule, standing where the character stands.
fn mule_here(name: &str) -> WorldObject {
    player(100, name, navigation_at(0.0).position)
}

/// `/t` lines submitted (`Submitted` starting "/t ").
fn tells(world: &World) -> Vec<String> {
    world
        .fake
        .submitted
        .iter()
        .filter(|line| line.starts_with("/t "))
        .cloned()
        .collect()
}

fn gives(world: &mut World) -> Vec<(u32, u32, u32)> {
    world.automation().gives.clone()
}

// ---- /vt help and mexec -----------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankHelpAndExpressionsUseTheLocalCommandSurface
#[test]
fn vtank_help_and_expressions_use_the_local_command_surface() {
    let (mut world, mut plugin) = open(FakeAutomation::default());
    world.clear_chat();

    world.vt(&mut plugin, "help");
    world.vt(&mut plugin, "mexec 1 + 2 * 3");

    // Four macro verb lists, the documented-command list, the pointer at
    // per-command help, the pointer at /ub, then the two the expression
    // wrote.
    let chat = world.chat();
    assert_eq!(chat.len(), 9, "{chat:?}");
    assert!(chat[0].starts_with("[MossTank] MossTank /vt \u{2014} profiles:"));
    assert_eq!(
        chat[7],
        "[MossTank] MExec evaluating expression: \"1 + 2 * 3\""
    );
    assert_eq!(chat[8], "[MossTank] Result: 7");
}

// ---- The counter ------------------------------------------------------------------

/// A named loot profile keeping the tapers, for a profile count.
fn save_counted_profile(world: &mut World, plugin: &mut MossTank) {
    world.act(plugin, "AddLootRule");
    world.act_text(plugin, "SetLootExpressionDraft", "name ~= taper");
    world.act(plugin, "ApplyLootRule");
    world.vt(plugin, "loot save Counted");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankCountAnswersAsACommandAndAsAnExpression
/// The count reaches the player as a command that writes its answer and as
/// expression functions a meta reads the figure from, both bound to the
/// same counter.
#[test]
fn vtank_count_answers_as_a_command_and_as_an_expression() {
    let mut automation = at(0.0);
    automation.item_entries = vec![
        item(10, "Prismatic Taper", 0x0000_1000),
        item(11, "Prismatic Taper", 0x0000_1000),
        item(12, "Pyreal Mote", 0x0000_1000),
    ];
    automation.world_objects = vec![
        // 24 m away, so inside a 100 m question.
        player(
            20,
            "Near",
            Position {
                cell_id: 0x0001_0001,
                north_south: 0.1,
                is_outdoor: true,
                ..Position::default()
            },
        ),
        // 240 m away, so outside it.
        player(
            21,
            "Far",
            Position {
                cell_id: 0x0001_0001,
                north_south: 1.0,
                is_outdoor: true,
                ..Position::default()
            },
        ),
        // The three carried items, already appraised, so a profile count
        // can decide all three without waiting on anything.
        carried(10, "Prismatic Taper"),
        carried(11, "Prismatic Taper"),
        carried(12, "Pyreal Mote"),
    ];
    let (mut world, mut plugin) = open(automation);
    let p = &mut plugin;
    save_counted_profile(&mut world, p);
    world.clear_chat();

    ub(&mut world, p, "count item taper");
    ub(&mut world, p, "count player 100");
    world.vt(p, "mexec vtcountitem[taper] + vtcountplayer[100]");

    assert_eq!(
        tells(&world)[..3],
        [
            "/t Test Character, Counter: Item Count: Prismatic Taper - 2",
            "/t Test Character, Counter: Total Item Count: 2",
            "/t Test Character, Counter: Player Count: 1",
        ]
    );
    let chat = world.chat();
    assert!(
        !chat.iter().any(|line| line.starts_with("You think")),
        "{chat:?}"
    );
    assert_eq!(
        chat.last().map(String::as_str),
        Some("[MossTank] Result: 3")
    );

    world.vt(p, "mexec vtcountprofile[Counted]");

    // Two tapers kept by the rule, and the mote left out by it.
    assert_eq!(
        world.chat().last().map(String::as_str),
        Some("[MossTank] Result: 2")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankCountStopCancelsAProfileCountThatIsStillWaiting
/// A profile count holds the character while it waits on appraisals; the
/// stop verb takes the character back.
#[test]
fn vtank_count_stop_cancels_a_profile_count_that_is_still_waiting() {
    let mut automation = FakeAutomation::default();
    automation.item_entries = vec![item(10, "Prismatic Taper", 0x0000_1000)];
    automation.unassessed.insert(10);
    let (mut world, mut plugin) = open(automation);
    let p = &mut plugin;
    save_counted_profile(&mut world, p);

    ub(&mut world, p, "count profile Counted");
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.starts_with("[UB] Counter: Items remaining to ID:")),
        "{chat:?}"
    );

    ub(&mut world, p, "count stop");

    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.contains("Item counter stopped: Counted")),
        "{chat:?}"
    );
}

// ---- The login roster -------------------------------------------------------------

fn login_character(object_id: u32, name: &str, active_index: i32) -> LoginCharacter {
    LoginCharacter {
        object_id,
        name: name.to_owned(),
        active_index,
        is_pending_delete: false,
    }
}

/// Mule's account, in the order the list arrived.
fn mule(roster: Vec<LoginCharacter>) -> (World, MossTank) {
    let mut automation = FakeAutomation::default();
    "Mule".clone_into(&mut automation.name);
    let mut world = World::with_automation(automation);
    world.set_reply::<calls::LoginState>(&LoginState {
        is_available: true,
        roster,
        ..LoginState::default()
    });
    world.set_reply::<calls::LoginSetNext>(&true);
    world.set_reply::<calls::LoginClearNext>(&true);
    let plugin = world.enable();
    (world, plugin)
}

/// `NextLoginObjectId`: the last pick the plugin made, 0 once cleared.
fn next_login(world: &World) -> u32 {
    let mut next = 0;
    for (import, bytes) in &world.fake.requests {
        match import {
            Import::LoginSetNext => {
                next = <calls::LoginSetNext as ac_plugin_abi::Call>::Request::from_bytes(bytes)
                    .unwrap();
            }
            Import::LoginClearNext => next = 0,
            _ => {}
        }
    }
    next
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankLoginListsTheRosterAlphabeticallyBesideItsAccountSlots
/// The number beside a character is its alphabetical position, not its
/// slot; the widths, the heading and the decimal id are the reference's.
#[test]
fn vtank_login_lists_the_roster_alphabetically_beside_its_account_slots() {
    let (mut world, mut plugin) = mule(vec![
        login_character(0x5000_0050, "Zeke", 0),
        login_character(0x10, "Mule", 1),
        login_character(0x30, "Aardvark", 2),
    ]);
    world.clear_chat();

    ub(&mut world, &mut plugin, "login list");

    // The character the client is on is marked, and each line ends on the
    // account slot.
    assert_eq!(
        world.chat(),
        [
            "[UB] Listing 3 logins.",
            "Index     Name                          ID                  Filter Index",
            "0         Aardvark                      48                  2",
            "1         **Mule**                      16                  1",
            "2         Zeke                          1342177360          0",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankLoginListOfAnEmptyRosterStillPrintsItsHeading
#[test]
fn vtank_login_list_of_an_empty_roster_still_prints_its_heading() {
    let (mut world, mut plugin) = mule(Vec::new());
    world.clear_chat();

    ub(&mut world, &mut plugin, "login list");

    assert_eq!(
        world.chat(),
        [
            "[UB] Listing 0 logins.",
            "Index     Name                          ID                  Filter Index",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankLoginNextTakesAnIndexARelativeIndexOrPartOfAName
/// An alphabetical index, an index counted from where the character
/// stands, or part of a name; an index off the end refuses and clears the
/// pick unless the wrap was asked for.
#[test]
fn vtank_login_next_takes_an_index_a_relative_index_or_part_of_a_name() {
    let (mut world, mut plugin) = mule(vec![
        login_character(0x50, "Zeke", 0),
        login_character(0x10, "Mule", 1),
        login_character(0x30, "Aardvark", 2),
    ]);
    let p = &mut plugin;

    ub(&mut world, p, "login next 0");
    assert_eq!(next_login(&world), 0x30);

    // One before "Mule" alphabetically is "Aardvark".
    ub(&mut world, p, "login nextr -1");
    assert_eq!(next_login(&world), 0x30);

    ub(&mut world, p, "login nextr 1");
    assert_eq!(next_login(&world), 0x50);

    ub(&mut world, p, "login next zek");
    assert_eq!(next_login(&world), 0x50);

    // Off the end without the loop flag: refused, and the pick is dropped.
    ub(&mut world, p, "login next 7");
    assert_eq!(next_login(&world), 0);

    // Past the end with the loop flag wraps around instead: three steps on
    // from "Mule" in a list of three is "Mule" again.
    ub(&mut world, p, "login nextrl 3");
    assert_eq!(next_login(&world), 0x10);

    ub(&mut world, p, "login next nobody");
    assert_eq!(next_login(&world), 0);

    ub(&mut world, p, "login next 2");
    assert_eq!(next_login(&world), 0x50);
    ub(&mut world, p, "login clear");
    assert_eq!(next_login(&world), 0);
}

// ---- The item giver ---------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankGiveReadsTheMatchModeTheCountAndTheTarget
/// How the item name is matched, how many units go, and to whom.
#[test]
fn vtank_give_reads_the_match_mode_the_count_and_the_target() {
    let mut automation = at(0.0);
    let mut tapers = item(10, "Prismatic Taper", 0x0000_1000);
    tapers.stack_size = 10;
    automation.item_entries = vec![tapers, item(11, "Pyreal Mote", 0x0000_1000)];
    automation.world_objects = vec![mule_here("Taper Mule")];
    let (mut world, mut plugin) = open(automation);
    let p = &mut plugin;

    ub(&mut world, p, "givep 4 prismatic to Taper Mule");
    tick(&mut world, p, 0.1);
    assert_eq!(gives(&mut world), [(10, 100, 4)]);

    ub(&mut world, p, "give stop");
    world.automation().gives.clear();

    ub(&mut world, p, "giver ^pyreal.*$ to Taper Mule");
    tick(&mut world, p, 0.1);
    assert_eq!(gives(&mut world), [(11, 100, 0)]);

    ub(&mut world, p, "give cancel");
    world.automation().gives.clear();

    // An exact match: the partial name alone matches nothing now.
    ub(&mut world, p, "give prismatic to Taper Mule");
    tick(&mut world, p, 0.1);
    assert!(gives(&mut world).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheItemGiverReportsItsEndThroughItsThinkSetting
/// A hand-over says how it ended: a think with ItemGiver.Think on, a plain
/// line with it off, for a run that finishes and for one stopped by hand.
#[test]
fn the_item_giver_reports_its_end_through_its_think_setting() {
    for think in [false, true] {
        let mut automation = at(0.0);
        automation.item_entries = vec![item(10, "Prismatic Taper", 0x0000_1000)];
        automation.world_objects = vec![mule_here("Taper Mule")];
        let (mut world, mut plugin) = open(automation);
        let p = &mut plugin;
        ub(
            &mut world,
            p,
            &format!(
                "opt set ItemGiver.Think {}",
                if think { "True" } else { "False" }
            ),
        );
        world.clear_chat();
        world.fake.submitted.clear();

        // Nothing matches, so the run ends on its first tick.
        ub(&mut world, p, "give Nothing Such to Taper Mule");
        tick(&mut world, p, 0.1);
        ub(&mut world, p, "givep Prismatic to Taper Mule");
        ub(&mut world, p, "give stop");

        // The reference's end line: what, to whom, how long, how many, and
        // the failures.
        let finished =
            "ItemGiver finished: nothing such to Taper Mule. took 0s to give 0 item(s). 0";
        let stopped = "ItemGiver finished: prismatic to Taper Mule. took 0s to give 0 item(s). 0";
        let chat = world.chat();
        if think {
            assert_eq!(
                tells(&world),
                [
                    format!("/t Test Character, {finished}"),
                    format!("/t Test Character, {stopped}"),
                ]
            );
            assert!(
                !chat.iter().any(|line| line.contains("ItemGiver finished")),
                "{chat:?}"
            );
        } else {
            assert!(chat.contains(&format!("[UB] {finished}")), "{chat:?}");
            assert!(chat.contains(&format!("[UB] {stopped}")), "{chat:?}");
            assert!(tells(&world).is_empty());
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AStuckHandOverBailsAfterTenSecondsWithTheReferencesLines
/// A hand-over that does not move on for ten seconds bails: the error, then
/// the finished line, and no second give of the stuck item.
#[test]
fn a_stuck_hand_over_bails_after_ten_seconds_with_the_references_lines() {
    let mut automation = at(0.0);
    automation.item_entries = vec![item(10, "Prismatic Taper", 0x0000_1000)];
    automation.world_objects = vec![mule_here("Taper Mule")];
    let (mut world, mut plugin) = open(automation);
    let p = &mut plugin;
    world.clear_chat();

    ub(&mut world, p, "give Prismatic Taper to Taper Mule");
    for _ in 0..25 {
        tick(&mut world, p, 0.5);
    }

    assert_eq!(gives(&mut world).len(), 1);
    let chat = world.chat();
    let bail = chat
        .iter()
        .position(|line| line == "[UB] Error: InventoryManager: ItemGiver bail, Timeout expired")
        .unwrap_or_else(|| panic!("{chat:?}"));
    let finished = chat
        .iter()
        .position(|line| line.starts_with("[UB] ItemGiver finished: "))
        .unwrap_or_else(|| panic!("{chat:?}"));
    assert_eq!(bail + 1, finished);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ACapitalPOnGiveMatchesPartOfTheTargetsNameAndNotTheItems
/// Little p matches part of the item's name, big P part of the target's.
#[test]
fn a_capital_p_on_give_matches_part_of_the_targets_name_and_not_the_items() {
    let mut automation = at(0.0);
    automation.item_entries = vec![
        item(10, "Prismatic Taper", 0x0000_1000),
        item(11, "Prismatic Taper, Sealed", 0x0000_1000),
    ];
    automation.world_objects = vec![mule_here("Taper Mule")];
    let (mut world, mut plugin) = open(automation);
    let p = &mut plugin;

    ub(&mut world, p, "giveP Prismatic Taper to Taper");
    tick(&mut world, p, 0.1);

    // Part of the target's name found the mule; the whole item name left
    // the sealed taper behind.
    assert_eq!(gives(&mut world), [(10, 100, 0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ProfileLoadCarriesTheGiveRange
/// The give's own numbers ride in the profile's side-car.
#[test]
fn a_profile_load_carries_the_give_range() {
    let mut automation = at(0.0);
    automation.item_entries = vec![item(10, "Prismatic Taper", 0x0000_1000)];
    // Five meters off: inside the default range, outside this one.
    automation.world_objects = vec![player(
        100,
        "Mule",
        Position {
            cell_id: 0x0001_0001,
            east_west: 5.0 / 240.0,
            is_outdoor: true,
            ..Position::default()
        },
    )];
    let mut world = World::with_automation(automation);
    world.profiles.insert(
        "mosstank/profiles/Tight.usd",
        &mosstank_settings::defaults::create().render(),
    );
    world.plugin_data.insert(
        "profiles/macro/sidecar/Tight.usd.json",
        "{ \"InventoryGiveRangeMeters\": 2 }",
    );
    let mut plugin = world.enable();
    let p = &mut plugin;
    tick(&mut world, p, 0.1);

    world.vt(p, "settings load Tight");
    tick(&mut world, p, 0.1);
    world.clear_chat();

    ub(&mut world, p, "give Prismatic Taper to Mule");
    tick(&mut world, p, 0.1);

    assert!(gives(&mut world).is_empty());
    let chat = world.chat();
    assert!(
        chat.iter().any(|message| message.starts_with(
            "[UB] Error: InventoryManager: Mule is 5.00 meters away, IGRange is set to "
        )),
        "{chat:?}"
    );
}

// ---- The jumper and the face command ----------------------------------------------

fn jumps(world: &World) -> usize {
    world.requests::<calls::NavigationJump>().len()
}

fn moves(world: &World) -> usize {
    world.requests::<calls::NavigationMove>().len()
}

/// The client reports the jump it made (`MoveReport.JumpSequence = 1`).
fn report_jump(world: &mut World) {
    world.set_reply::<calls::NavigationMoveReport>(&MoveReport {
        jump_sequence: 1,
        ..MoveReport::default()
    });
}

/// `ExpireCommandJumpAttempt`: one command jump's hold, settle and retry
/// grace, ticked through.
fn expire_command_jump_attempt(world: &mut World, plugin: &mut MossTank) {
    tick(world, plugin, 0.1);
    tick(world, plugin, 0.25);
    tick(world, plugin, 5.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankJumpRetriesAChargeThatNeverBecameAJumpThenGivesUp
/// A charge the client never turns into a jump is tried again, and after
/// the attempt ceiling the command lets the character go.
#[test]
fn vtank_jump_retries_a_charge_that_never_became_a_jump_then_gives_up() {
    let (mut world, mut plugin) = open(at(180.0));
    let p = &mut plugin;

    world.vt(p, "jump 180 false 100");
    tick(&mut world, p, 0.05);
    assert_eq!(jumps(&world), 1);

    for attempt in 2..=3 {
        expire_command_jump_attempt(&mut world, p);
        tick(&mut world, p, 0.05);
        assert_eq!(jumps(&world), attempt);
    }

    expire_command_jump_attempt(&mut world, p);
    let last = world.chat().last().cloned().unwrap_or_default();
    assert!(last.to_lowercase().contains("gave up"), "{last}");

    let (jumped, moved) = (jumps(&world), moves(&world));
    tick(&mut world, p, 1.0);
    assert_eq!(jumps(&world), jumped);
    assert_eq!(moves(&world), moved);

    // The same clock run through with the client reporting a jump: the
    // charge stands, and nothing is tried a second time.
    let (mut reporting, mut second) = open(at(180.0));
    let s = &mut second;
    reporting.vt(s, "jump 180 false 100");
    tick(&mut reporting, s, 0.05);
    tick(&mut reporting, s, 0.1);
    report_jump(&mut reporting);
    expire_command_jump_attempt(&mut reporting, s);
    tick(&mut reporting, s, 0.05);
    assert_eq!(jumps(&reporting), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: UbFaceTurnsToTheHeadingAndStopsWithoutJumping
/// Facing a heading is the jump's turn without the jump.
#[test]
fn ub_face_turns_to_the_heading_and_stops_without_jumping() {
    let (mut world, mut plugin) = open(at(90.0));
    let p = &mut plugin;

    ub(&mut world, p, "face 180");
    tick(&mut world, p, 0.05);
    assert_eq!(
        world.requests::<calls::NavigationFace>().last().copied(),
        Some(180.0)
    );

    world.automation().navigation = navigation_at(180.0);
    tick(&mut world, p, 0.1);
    assert!(
        !world
            .automation()
            .movement_intents
            .iter()
            .any(|intent| intent.jump)
    );

    let asked = world.requests::<calls::NavigationFace>().len();
    tick(&mut world, p, 1.0);
    assert_eq!(world.requests::<calls::NavigationFace>().len(), asked);
    assert!(world.automation().movement_intents.is_empty());
}

fn navigation_locked(plugin: &MossTank) -> bool {
    plugin
        .tank()
        .unwrap()
        .is_action_locked(ActionLockKind::Navigation)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: PauseNavHoldsTheRouteStillForAsLongAsTheJumpLasts
/// Jumper.PauseNav holds the route still while the jump is under way and
/// lets it go the moment the jump is done.
#[test]
fn pause_nav_holds_the_route_still_for_as_long_as_the_jump_lasts() {
    let (mut world, mut plugin) = open(at(180.0));
    let p = &mut plugin;

    ub(&mut world, p, "face 180");
    assert!(navigation_locked(p));

    // Already pointing the right way, so the turn is done on the first tick
    // and the hold goes with it.
    tick(&mut world, p, 0.05);
    assert!(!navigation_locked(p));

    ub(&mut world, p, "opt set Jumper.PauseNav false");
    ub(&mut world, p, "face 180");
    assert!(!navigation_locked(p));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ThinkCompleteThinksTurningSuccessAndJumperSuccess
/// Jumper.ThinkComplete says so to yourself when the turn or the jump
/// lands.
#[test]
fn think_complete_thinks_turning_success_and_jumper_success() {
    let (mut world, mut plugin) = open(at(180.0));
    let p = &mut plugin;
    ub(&mut world, p, "opt set Jumper.ThinkComplete true");

    ub(&mut world, p, "face 180");
    tick(&mut world, p, 0.05);
    assert!(
        world
            .fake
            .submitted
            .contains(&"/t Test Character, Turning Success".to_owned()),
        "{:?}",
        world.fake.submitted
    );

    world.vt(p, "jump 180 false 100");
    tick(&mut world, p, 0.05);
    tick(&mut world, p, 0.1);
    report_jump(&mut world);
    tick(&mut world, p, 0.5);

    assert!(
        world
            .fake
            .submitted
            .contains(&"/t Test Character, Jumper Success".to_owned()),
        "{:?}",
        world.fake.submitted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ThinkFailThinksTheJumpThatNeverHappened
/// Jumper.ThinkFail sends the give-up down the same road.
#[test]
fn think_fail_thinks_the_jump_that_never_happened() {
    let (mut world, mut plugin) = open(at(180.0));
    let p = &mut plugin;
    ub(&mut world, p, "opt set Jumper.ThinkFail true");

    world.vt(p, "jump 180 false 100");
    tick(&mut world, p, 0.05);
    for _ in 2..=3 {
        expire_command_jump_attempt(&mut world, p);
        tick(&mut world, p, 0.05);
    }
    expire_command_jump_attempt(&mut world, p);

    assert!(
        world
            .fake
            .submitted
            .iter()
            .any(|line| line.starts_with("/t Test Character, Jump gave up")),
        "{:?}",
        world.fake.submitted
    );
}
