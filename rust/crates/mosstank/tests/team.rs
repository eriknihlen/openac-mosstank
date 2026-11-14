//! Team play from the shell: the four team switches under `/vt opt` and
//! on the Vitals tab, and `/vt team`. New behaviour (owner, 2026-11-14),
//! not a port.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::peers::NetworkClient;
use common::World;
use mosstank::MossTank;

fn fresh(name: &str) -> (World, MossTank) {
    let mut world = World::with_character(name);
    let plugin = world.enable();
    world.clear_chat();
    (world, plugin)
}

fn said(world: &World, text: &str) -> bool {
    world.chat().iter().any(|line| line.contains(text))
}

fn restart(world: &mut World, plugin: &mut MossTank) -> MossTank {
    world.disable(plugin);
    world.clear_chat();
    world.enable()
}

#[test]
fn the_team_switches_are_options_with_their_defaults() {
    let (mut world, mut plugin) = fresh("Teamer");

    world.vt(&mut plugin, "opt list");
    for name in [
        "HelpFellows",
        "HelpNetworkPeers",
        "TeamFocusDebuffed",
        "TeamOverlay",
    ] {
        assert!(said(&world, name), "{name}");
    }
    for (name, value) in [
        ("HelpFellows", "True"),
        ("HelpNetworkPeers", "True"),
        ("TeamFocusDebuffed", "True"),
        ("TeamOverlay", "False"),
    ] {
        world.vt(&mut plugin, &format!("opt get {name}"));
        assert!(said(&world, &format!("Option {name} = {value}")), "{name}");
    }
}

/// `HelpFellows` is VTank's `DoHelp`: one value under two names, on the
/// panel too, and it survives a reload whichever name wrote it last.
#[test]
fn help_fellows_is_do_help_and_survives_a_reload() {
    let (mut world, mut plugin) = fresh("Teamer");

    world.vt(&mut plugin, "opt set DoHelp false");
    world.vt(&mut plugin, "opt set HelpFellows true");
    world.vt(&mut plugin, "opt toggle helpfellows");
    world.vt(&mut plugin, "opt get DoHelp");
    assert!(said(&world, "Option DoHelp = False"));
    assert!(!world.ui_bool(&mut plugin, "HelpOthersEnabled"));

    let mut restarted = restart(&mut world, &mut plugin);
    world.vt(&mut restarted, "opt get HelpFellows");
    assert!(said(&world, "Option HelpFellows = False"));

    // The panel's switch writes the same option.
    world.act(&mut restarted, "ToggleHelpOthers");
    let mut again = restart(&mut world, &mut restarted);
    world.vt(&mut again, "opt get DoHelp");
    assert!(said(&world, "Option DoHelp = True"));
}

#[test]
fn the_vitals_tab_switches_follow_the_options_and_survive_a_reload() {
    let (mut world, mut plugin) = fresh("Teamer");
    assert!(world.ui_bool(&mut plugin, "HelpNetworkPeersEnabled"));
    assert!(world.ui_bool(&mut plugin, "TeamFocusDebuffedEnabled"));
    assert!(!world.ui_bool(&mut plugin, "TeamOverlayEnabled"));

    world.act(&mut plugin, "ToggleHelpNetworkPeers");
    world.act(&mut plugin, "ToggleTeamFocusDebuffed");
    world.act(&mut plugin, "ToggleTeamOverlay");
    world.vt(&mut plugin, "opt get HelpNetworkPeers");
    assert!(said(&world, "Option HelpNetworkPeers = False"));

    let mut restarted = restart(&mut world, &mut plugin);
    assert!(!world.ui_bool(&mut restarted, "HelpNetworkPeersEnabled"));
    assert!(!world.ui_bool(&mut restarted, "TeamFocusDebuffedEnabled"));
    assert!(world.ui_bool(&mut restarted, "TeamOverlayEnabled"));
}

fn peer(client_id: u32, player_id: u32, name: &str, remote: bool) -> NetworkClient {
    NetworkClient {
        client_id,
        player_id,
        name: name.to_owned(),
        world_name: "Coldeve".to_owned(),
        current_health: 40,
        max_health: 100,
        is_remote: remote,
        ..NetworkClient::default()
    }
}

#[test]
fn vt_team_lists_the_switches_the_members_and_the_last_decisions() {
    let (mut world, mut plugin) = fresh("Teamer");
    world.set_reply::<calls::PeersIsConnected>(&true);
    world.set_reply::<calls::PeersClients>(&vec![
        peer(2, 0x5000_0002, "Healer", false),
        peer(3, 0x5000_0003, "Faraway", true),
    ]);

    world.vt(&mut plugin, "team");

    assert!(said(
        &world,
        "Team: HelpFellows on, HelpNetworkPeers on, TeamFocusDebuffed on, TeamOverlay off."
    ));
    assert!(said(
        &world,
        "Healer (network peer, this computer): health 40%"
    ));
    assert!(said(&world, "Faraway (network peer, imported): health 40%"));
    assert!(said(&world, "Claims: none."));
    assert!(said(&world, "Team debuffs: none in force."));
    assert!(said(&world, "Last helper decision: none yet."));
}

#[test]
fn vt_team_alone_says_so() {
    let (mut world, mut plugin) = fresh("Teamer");
    world.vt(&mut plugin, "team");
    assert!(said(&world, "Nobody else in the team"));
}

#[test]
fn vt_team_overlay_sets_the_overlay_switch() {
    let (mut world, mut plugin) = fresh("Teamer");

    world.vt(&mut plugin, "team overlay on");
    assert!(said(&world, "Team overlay on."));
    world.vt(&mut plugin, "opt get TeamOverlay");
    assert!(said(&world, "Option TeamOverlay = True"));
    world.vt(&mut plugin, "team overlay off");
    assert!(said(&world, "Team overlay off."));
    world.vt(&mut plugin, "team overlay");
    world.vt(&mut plugin, "opt get TeamOverlay");
    assert!(said(&world, "Option TeamOverlay = True"));
    world.vt(&mut plugin, "team nonsense");
    assert!(said(&world, "Syntax: /vt team [overlay [on|off]]"));
}

#[test]
fn vt_help_lists_team() {
    let (mut world, mut plugin) = fresh("Teamer");
    world.vt(&mut plugin, "help");
    assert!(said(&world, "opt team"));
    world.vt(&mut plugin, "help team");
    assert!(said(&world, "Syntax: /vt team [overlay [on|off]]"));
}
