//! The `/vt` and `/ub` commands whose point is another slice of the
//! plugin: help, the older jump grammar, an option flipped by its plain
//! name, a route point typed in and a route moved to
//! another landblock (ported from the C#'s `MossTankPanelTests.UbCommands`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::{
    MoveDirection, MovePace, MoveRequest, MoveUnit, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;

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

/// `new FakeAutomation { NavigationSnapshot = NavigationAt(heading) }`,
/// taking every movement.
fn standing(heading: f32) -> (World, MossTank) {
    let mut world = World::panel_character("Test Character");
    world.set_reply::<calls::NavigationSnapshot>(&navigation_at(heading));
    let accepted = NavigationStatus::Accepted;
    world.set_reply::<calls::NavigationJump>(&accepted);
    world.set_reply::<calls::NavigationMove>(&accepted);
    world.set_reply::<calls::NavigationSetIntent>(&accepted);
    world.set_reply::<calls::NavigationClearIntent>(&accepted);
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

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: HelpForAnUnknownCommandSaysSoRatherThanPrintingEverything
#[test]
fn help_for_an_unknown_command_says_so_rather_than_printing_everything() {
    let mut world = World::panel_character("Test Character");
    let mut plugin = world.enable();
    world.clear_chat();

    world.vt(&mut plugin, "help notacommand");

    assert_eq!(
        world.chat(),
        ["[MossTank] No help found for command: notacommand"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheOlderJumpGrammarStillStrafes
/// A second word of true or false cannot be a hold time, so routes and
/// macros written against the older grammar keep working.
#[test]
fn the_older_jump_grammar_still_strafes() {
    let (mut world, mut plugin) = standing(90.0);

    world.vt(&mut plugin, "jump 90 true 800 strafeleft");
    tick(&mut world, &mut plugin, 0.1);

    assert_eq!(
        world.requests::<calls::NavigationMove>(),
        [MoveRequest {
            direction: MoveDirection::StrafeLeft,
            pace: MovePace::Walk,
            amount: 0.0,
            unit: MoveUnit::MetersOrDegrees,
        }]
    );
    assert_eq!(world.requests::<calls::NavigationJump>(), [0.8]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ToggleFlipsTheMacrosOwnOptionByItsPlainName
/// Where the two groups could both answer a name, the macro's own option
/// wins.
#[test]
fn toggle_flips_the_macros_own_option_by_its_plain_name() {
    let mut world = World::panel_character("Test Character");
    let mut plugin = world.enable();
    let before = plugin.tank().unwrap().option("EnableCombat").is_truthy();
    world.clear_chat();

    world.vt(&mut plugin, "opt toggle EnableCombat");

    assert_eq!(
        plugin.tank().unwrap().option("EnableCombat").is_truthy(),
        !before
    );
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.starts_with("[MossTank] Set option EnableCombat")),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AddNavPointTakesTheCoordinatePairOutOfTheWholeArgument
/// The north/south and east/west pair is found anywhere in the argument,
/// and the point is added at ground height.
#[test]
fn add_nav_point_takes_the_coordinate_pair_out_of_the_whole_argument() {
    for line in [
        "addnavpt 41.20S, 44.00E, 0.41Z",
        "addnavpt 41.2S 44E",
        "addnavpt 41.20S, 44.00E,",
    ] {
        let (mut world, mut plugin) = standing(0.0);

        world.vt(&mut plugin, line);
        assert_eq!(
            world.chat().last().map(String::as_str),
            Some("[MossTank] Added navigation point."),
            "{line}"
        );
        world.vt(&mut plugin, "navaf save pair-only");

        let saved: Vec<&str> = world
            .profiles
            .keys()
            .filter(|key| key.contains("pair-only"))
            .collect();
        assert_eq!(saved.len(), 1, "{line}: {saved:?}");
        let text = world.profiles.get(saved[0]).unwrap();
        assert!(text.contains("pnt 44 -41.2 0"), "{line}: {text}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TranslateRouteShiftsEveryPointByTheLandblockDistance
#[test]
fn translate_route_shifts_every_point_by_the_landblock_distance() {
    let (mut world, mut plugin) = standing(0.0);
    let p = &mut plugin;
    world.vt(p, "addnavpt 10.0N, 20.0E");
    world.vt(p, "nav save eo-east");
    world.clear_chat();

    // One landblock step east is 192 m, which is 0.8 coordinates. The
    // east/west block is the top byte of the landblock id.
    ub(
        &mut world,
        p,
        "translateroute 0x00640371 eo-east.nav 0x01640371 eo-main.nav",
    );

    // Both routes are .nav files: a route saved by a bare name is one, and
    // the translation is written in the form its name asks for.
    let saved: Vec<&str> = world
        .profiles
        .keys()
        .filter(|key| key.contains("eo-main"))
        .collect();
    assert_eq!(saved, ["mosstank/navs/eo-main.nav"]);
    let text = world.profiles.get(saved[0]).unwrap();
    assert!(text.contains("\r\n20.8\r\n10\r\n0\r\n0\r\n"), "{text}");
    // A translation that worked is said only in the reference's debug
    // output, so with Plugin.Debug off nothing is printed.
    assert!(world.chat().is_empty(), "{:?}", world.chat());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TranslateRouteRefusesToOverwriteWithoutTheForceFlag
#[test]
fn translate_route_refuses_to_overwrite_without_the_force_flag() {
    let (mut world, mut plugin) = standing(0.0);
    let p = &mut plugin;
    world.vt(p, "addnavpt 10.0N, 20.0E");
    world.vt(p, "nav save eo-east");
    world.vt(p, "nav save eo-main");
    world.clear_chat();

    ub(
        &mut world,
        p,
        "translateroute 0x00640371 eo-east.nav 0x00650371 eo-main.nav",
    );

    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.starts_with("[UB] Error: VTank: Output path already exists!")),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ListGvarsWithADamagedVariableFileFailsAsOneCommand
/// The global variables live in the plugin's storage, one file each; one
/// damaged file fails the whole listing with its reason, and nothing of the
/// list is printed.
#[test]
fn list_gvars_with_a_damaged_variable_file_fails_as_one_command() {
    let mut world = World::panel_character("Test Character");
    let mut plugin = world.enable();
    world.vt(&mut plugin, "mexec setgvar[damaged,1]");
    let keys: Vec<String> = world
        .plugin_data
        .keys()
        .filter(|key| key.starts_with("expressions/global/"))
        .map(str::to_owned)
        .collect();
    assert_eq!(keys.len(), 1, "{keys:?}");
    world.vt(&mut plugin, "mexec setgvar[fine,2]");
    world.plugin_data.insert(&keys[0], "{ not json");
    world.clear_chat();

    ub(&mut world, &mut plugin, "listgvars");

    let chat = world.chat();
    assert!(
        !chat
            .iter()
            .any(|line| line == "[UB] Defined global variables:"),
        "{chat:?}"
    );
    assert!(
        chat.iter()
            .any(|line| line.starts_with("[UB] Error: Command failed: A global variable file")),
        "{chat:?}"
    );
}
