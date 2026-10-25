//! A meta's own `/vt` and `/ub` lines run inside its pass, before the next
//! rule is looked at, as the C#'s `IPluginChat.Submit` runs the plugin's
//! verbs before it returns (ported from the C#'s
//! `MossTankPanelTests.MetaLines`). Those tests are not yet in this
//! branch's C# tree, so they are named in "C# source" lines the census
//! does not read; make them Source lines when the C# tree catches up.

mod common;

use common::World;
use mosstank::MossTank;

/// A one-point circular route: never empty.
const QUESTS_ROUTE: &str = "NAV: nav0 circular ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n~~ }\r\n";

/// States the metas below may end in; each holds a rule that never fires.
const END_STATES: &str = "STATE: {Loaded} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\nSTATE: {Skipped} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\nSTATE: {Other} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\n";

fn meta(default_rules: &str) -> String {
    format!("STATE: {{Default}} ~~ {{\r\n{default_rules}~~ }}\r\n{END_STATES}")
}

/// The meta loaded and switched on, the macro started, and one pass.
fn run_one_pass(meta_text: &str) -> (World, MossTank) {
    let mut world = World::with_character("Barris");
    world.profiles.insert("mosstank/metas/Lines.af", meta_text);
    world
        .profiles
        .insert("mosstank/navs/Quests.af", QUESTS_ROUTE);
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Lines");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    // Only what the pass says is kept.
    world.clear_chat();
    world.with_tank(&mut plugin, |tank, client| tank.on_tick(client, 0.3));
    (world, plugin)
}

fn meta_state(plugin: &MossTank) -> String {
    plugin.tank().unwrap().meta_state().to_owned()
}

fn state_lines(world: &World) -> Vec<String> {
    world
        .chat()
        .into_iter()
        .filter(|line| line.contains("Meta state:"))
        .collect()
}

// C# source: MossTankPanelTests.MetaLines.cs :: ARouteAMetaLineLoadsIsThereForTheNextRule
#[test]
fn a_route_a_meta_line_loads_is_there_for_the_next_rule() {
    let (world, plugin) = run_one_pass(&meta(
        "\tIF:\tAlways\r\n\t\tDO:\tChat {/vt nav load Quests}\r\n\tIF:\tNavEmpty\r\n\t\tDO:\tSetState {Skipped}\r\n\tIF:\tAlways\r\n\t\tDO:\tSetState {Loaded}\r\n",
    ));

    assert_eq!(meta_state(&plugin), "Loaded", "{:#?}", world.chat());
    assert_eq!(plugin.tank().unwrap().route_waypoint_count(), 1);
}

// C# source: MossTankPanelTests.MetaLines.cs :: ARouteAChatboxLineLoadsIsThereForTheNextRule
#[test]
fn a_route_a_chatbox_line_loads_is_there_for_the_next_rule() {
    let (world, plugin) = run_one_pass(&meta(
        "\tIF:\tAlways\r\n\t\tDO:\tDoExpr {chatbox[`/vt nav load Quests`]}\r\n\tIF:\tNavEmpty\r\n\t\tDO:\tSetState {Skipped}\r\n\tIF:\tAlways\r\n\t\tDO:\tSetState {Loaded}\r\n",
    ));

    assert_eq!(meta_state(&plugin), "Loaded", "{:#?}", world.chat());
}

// C# source: MossTankPanelTests.MetaLines.cs :: ARouteADelayedChatboxLineLoadsIsThereBeforeThePass
#[test]
fn a_route_a_delayed_chatbox_line_loads_is_there_before_the_pass() {
    let mut text = meta(
        "\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tDoExpr {delayexec[0,`chatbox[\\`/vt nav load Quests\\`]`]}\r\n\t\t\t\tSetState {Waiting}\r\n",
    );
    text.push_str("STATE: {Waiting} ~~ {\r\n\tIF:\tNavEmpty\r\n\t\tDO:\tSetState {Skipped}\r\n\tIF:\tAlways\r\n\t\tDO:\tSetState {Loaded}\r\n~~ }\r\n");
    let (mut world, mut plugin) = run_one_pass(&text);
    assert_eq!(meta_state(&plugin), "Waiting", "{:#?}", world.chat());

    // The delayed expression runs at the start of the next tick, before
    // that tick's pass.
    world.with_tank(&mut plugin, |tank, client| tank.on_tick(client, 0.3));

    assert_eq!(meta_state(&plugin), "Loaded", "{:#?}", world.chat());
}

// C# source: MossTankPanelTests.MetaLines.cs :: AStateAMetaLineSetsEndsThePass
#[test]
fn a_state_a_meta_line_sets_ends_the_pass() {
    let (world, plugin) = run_one_pass(&meta(
        "\tIF:\tAlways\r\n\t\tDO:\tChat {/vt setmetastate Other}\r\n\tIF:\tAlways\r\n\t\tDO:\tSetState {Skipped}\r\n",
    ));

    assert_eq!(meta_state(&plugin), "Other");
    assert_eq!(
        state_lines(&world),
        vec!["[MossTank] Meta state: Default -> Other.".to_owned()]
    );
}

// C# source: MossTankPanelTests.MetaLines.cs :: AVariableAUbLineSetsIsThereForTheNextRule
#[test]
fn a_variable_a_ub_line_sets_is_there_for_the_next_rule() {
    let (world, plugin) = run_one_pass(&meta(
        "\tIF:\tAlways\r\n\t\tDO:\tChat {/ub mexec setvar[seen, 1]}\r\n\tIF:\tExpr {getvar[seen]==1}\r\n\t\tDO:\tSetState {Loaded}\r\n\tIF:\tAlways\r\n\t\tDO:\tSetState {Skipped}\r\n",
    ));

    assert_eq!(meta_state(&plugin), "Loaded", "{:#?}", world.chat());
}
