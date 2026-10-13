//! The peer features the macro asks the other clients for
//! (`MossTankPanel.Peers`): the UB tab's switches, the meta's own calls and
//! an expression's, and the meta waiting for a hub that is not connected.
//! The C# has no panel test of these; the cases follow its code.

mod common;

use ac_plugin_abi::calls;
use common::World;
use mosstank::MossTank;

const CLIENT_STATE: u32 = 1;
const CASTS: u32 = 2;
const COMMANDS: u32 = 4;

/// A meta whose first pass calls `netclients[]` and moves to Walk.
const PEER_META: &str = "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tDoExpr {netclients[]}\r\n\t\t\t\tSetState {Walk}\r\nSTATE: {Walk} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n";

/// In the world, on a host with a peer hub that hands out lease 7, with
/// the UB tab's command switch (on by default) turned off so each test
/// starts from nothing asked for.
fn hub_world() -> (World, MossTank) {
    let mut world = World::with_character("Barris");
    world.set_reply::<calls::PeersSubscribe>(&Some(7));
    world.set_reply::<calls::PeersUnsubscribe>(&true);
    let mut plugin = world.enable();
    world.command(
        &mut plugin,
        "ub",
        "opt set Networking.ReceiveCommands false",
    );
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), 0);
    world.fake.requests.clear();
    (world, plugin)
}

fn requested(plugin: &MossTank) -> u32 {
    plugin.tank().unwrap().requested_peer_features()
}

#[test]
fn sharing_vitals_asks_for_the_clients_state_and_casts() {
    let (mut world, mut plugin) = hub_world();

    world.command(&mut plugin, "ub", "opt set Sharing.Vitals true");
    world.tick(&mut plugin);

    assert_eq!(requested(&plugin), CLIENT_STATE | CASTS);
    assert_eq!(
        world.requests::<calls::PeersSubscribe>(),
        vec![CLIENT_STATE | CASTS]
    );
}

#[test]
fn receiving_commands_asks_for_the_broadcasts_and_turning_it_off_lets_go() {
    let (mut world, mut plugin) = hub_world();
    world.command(&mut plugin, "ub", "opt set Networking.ReceiveCommands true");
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), COMMANDS);

    world.command(
        &mut plugin,
        "ub",
        "opt set Networking.ReceiveCommands false",
    );
    world.tick(&mut plugin);

    assert_eq!(requested(&plugin), 0);
    assert_eq!(world.requests::<calls::PeersUnsubscribe>(), vec![7]);
}

#[test]
fn the_meta_waits_for_the_hub_its_functions_need() {
    let (mut world, mut plugin) = hub_world();
    world.profiles.insert("mosstank/metas/Peers.af", PEER_META);
    world.vt(&mut plugin, "meta load Peers");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    world.set_reply::<calls::PeersIsConnected>(&false);

    world.with_tank(&mut plugin, |tank, client| tank.on_tick(client, 0.3));

    assert_eq!(requested(&plugin), CLIENT_STATE);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Default");

    world.set_reply::<calls::PeersIsConnected>(&true);
    world.with_tank(&mut plugin, |tank, client| tank.on_tick(client, 0.3));

    assert_eq!(plugin.tank().unwrap().meta_state(), "Walk");
}

#[test]
fn a_host_without_a_hub_never_holds_the_meta() {
    let mut world = World::with_character("Barris");
    world.set_reply::<calls::PeersSubscribe>(&None);
    world.set_reply::<calls::PeersIsConnected>(&false);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), COMMANDS);
    world.profiles.insert("mosstank/metas/Peers.af", PEER_META);
    world.vt(&mut plugin, "meta load Peers");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });

    world.with_tank(&mut plugin, |tank, client| tank.on_tick(client, 0.3));

    assert_eq!(plugin.tank().unwrap().meta_state(), "Walk");
}

#[test]
fn an_expressions_call_keeps_its_peers_for_five_seconds_while_the_meta_is_off() {
    let (mut world, mut plugin) = hub_world();
    world.fake.clock.utc_unix_ms = 1_000_000;
    world.vt(&mut plugin, "mexec netcasts[]");
    assert_eq!(requested(&plugin), CASTS);

    world.fake.clock.utc_unix_ms += 4_999;
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), CASTS);

    world.fake.clock.utc_unix_ms += 1;
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), 0);
}

#[test]
fn the_lease_goes_back_when_the_session_ends_and_with_the_plugin() {
    let (mut world, mut plugin) = hub_world();
    world.command(&mut plugin, "ub", "opt set Sharing.Vitals true");
    world.tick(&mut plugin);
    world.fake.character = None;
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), 0);
    assert_eq!(world.requests::<calls::PeersUnsubscribe>(), vec![7]);

    world.fake.character = Some(common::character("Barris"));
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), CLIENT_STATE | CASTS);
    world.disable(&mut plugin);

    assert_eq!(world.requests::<calls::PeersUnsubscribe>(), vec![7, 7]);
}

// ---- The labels row on the shell's frame ----------------------------------
//
// Not ports: the C# tests of the labels row (`TheLabelsRowIsPushedToThe
// ClientWhenItChanges`, `ALabelsRowSavedEarlierIsPushedAsThePageIsBuilt`)
// are ported against the publisher in mosstank-ub; these check that the
// shell builds and ticks it where the panel does.

fn tag_pushes(world: &World) -> Vec<Vec<String>> {
    world.requests::<calls::PeersSetTags>()
}

#[test]
fn the_shell_pushes_the_labels_row_when_it_changes() {
    let mut world = World::with_character("Barris");
    world.set_reply::<calls::PeersSetTags>(&true);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    assert!(tag_pushes(&world).is_empty());

    world.command(&mut plugin, "ub", "opt set Networking.Tags add tank");
    world.tick(&mut plugin);
    world.tick(&mut plugin);

    assert_eq!(vec![vec!["tank".to_owned()]], tag_pushes(&world));
}

#[test]
fn the_shell_pushes_a_labels_row_saved_earlier_as_it_starts() {
    let mut world = World::with_character("Barris");
    world.set_reply::<calls::PeersSetTags>(&true);
    let mut first = world.enable();
    world.command(&mut first, "ub", "opt set Networking.Tags add tank");
    world.disable(&mut first);
    world.fake.requests.clear();

    let _next = world.enable();

    assert_eq!(vec![vec!["tank".to_owned()]], tag_pushes(&world));
}

// ---- The network readout's window -----------------------------------------
//
// Not a port: the C# has no panel test of it. `ShouldShow` asks for the
// clients' state while the readout is on and its window is open, or while
// it may stay on screen with the window shut.

#[test]
fn the_open_clients_window_asks_for_the_clients_state() {
    let (mut world, mut plugin) = hub_world();
    world.set_reply::<calls::HostWindowAttached>(&true);
    world.command(
        &mut plugin,
        "ub",
        "opt set NetworkUI.ShowHudWhenClosed false",
    );
    world.command(&mut plugin, "ub", "opt set NetworkUI.Enabled true");
    // The readout re-reads its switches on its own frame every quarter
    // second, after the peers have asked.
    world.ticks(&mut plugin, 20);
    assert_eq!(requested(&plugin), 0);

    world.act(&mut plugin, "ShowUbClients");
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), CLIENT_STATE);

    world.act(&mut plugin, "HideUbClients");
    world.tick(&mut plugin);
    assert_eq!(requested(&plugin), 0);
}
