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
    assert!(said(&world, "Last focus decision: none yet."));
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

/// The readout's UB switch is `TeamOverlay`, both ways.
#[test]
fn network_ui_enabled_is_the_team_overlay_switch() {
    let (mut world, mut plugin) = fresh("Teamer");

    world.vt(&mut plugin, "opt set TeamOverlay true");
    world.command(&mut plugin, "ub", "opt get NetworkUI.Enabled");
    assert!(said(&world, "NetworkUI.Enabled (Profile) = True"));

    world.command(&mut plugin, "ub", "opt set NetworkUI.Enabled false");
    world.vt(&mut plugin, "opt get TeamOverlay");
    assert!(said(&world, "Option TeamOverlay = False"));
    assert!(!world.ui_bool(&mut plugin, "TeamOverlayEnabled"));
}

fn option_is(world: &mut World, plugin: &mut MossTank, name: &str, value: &str) -> bool {
    world.clear_chat();
    world.vt(plugin, &format!("opt get {name}"));
    said(world, &format!("Option {name} = {value}"))
}

/// `setinall` reaches every settings profile for the team switches kept in
/// the side-car too.
#[test]
fn setinall_reaches_every_profile_for_the_side_car_team_switches() {
    for name in ["HelpNetworkPeers", "TeamFocusDebuffed", "TeamOverlay"] {
        let (mut world, mut plugin) = fresh("Teamer");
        let start = if name == "TeamOverlay" {
            "false"
        } else {
            "true"
        };
        let end = if name == "TeamOverlay" {
            "True"
        } else {
            "False"
        };
        let end_lower = end.to_lowercase();
        world.vt(&mut plugin, &format!("opt set {name} {start}"));
        world.vt(&mut plugin, "settings save First");
        world.vt(&mut plugin, "settings save Second");

        world.vt(&mut plugin, &format!("opt setinall {name} {end_lower}"));
        assert!(said(
            &world,
            &format!("Done saving setting {name} to all profiles.")
        ));
        world.vt(&mut plugin, "settings load First");

        assert!(option_is(&mut world, &mut plugin, name, end), "{name}");
    }
}

/// `setinall HelpFellows` is not undone by an older `HelpFellows` another
/// profile's side-car still carries.
#[test]
fn setinall_help_fellows_is_not_undone_by_an_older_override() {
    let (mut world, mut plugin) = fresh("Teamer");
    world.vt(&mut plugin, "opt set HelpFellows false");
    world.vt(&mut plugin, "settings save First");
    world.vt(&mut plugin, "settings save Second");

    world.vt(&mut plugin, "opt setinall HelpFellows true");
    world.vt(&mut plugin, "settings load First");

    assert!(option_is(&mut world, &mut plugin, "HelpFellows", "True"));
    assert!(option_is(&mut world, &mut plugin, "DoHelp", "True"));
}

/// A value an existing UB settings file holds for `NetworkUI.Enabled`
/// becomes `TeamOverlay` once for a character; after that the option is
/// the character's own.
#[test]
fn a_saved_network_ui_switch_carries_over_to_the_team_overlay_once() {
    let mut world = World::with_character("Teamer");
    world.profiles.insert(
        "mosstank/ub/settings.json",
        "{\n  \"NetworkUI.Enabled\": \"True\"\n}",
    );
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 3);
    assert!(option_is(&mut world, &mut plugin, "TeamOverlay", "True"));

    world.vt(&mut plugin, "opt set TeamOverlay false");
    world.disable(&mut plugin);
    let mut again = world.enable();
    world.ticks(&mut again, 3);

    assert!(option_is(&mut world, &mut again, "TeamOverlay", "False"));
}
