//! Focus on what the team debuffed (`TeamFocusDebuffed`): a fighter on a
//! monster the team has not debuffed switches to one in range that it has,
//! and then stays there while that debuff holds. New behaviour (owner,
//! 2026-11-14), not a port.

mod common;

use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::peers::PeerCast;
use common::{
    FakeAutomation, Fight, debuff, physical, profile_fixture_weapon, rule, target,
    wielded_planned_weapon,
};
use mosstank_settings::model::{CombatSettings, TargetSelectionMethod};

const FESTER: u32 = 80;
const IMPERIL: u32 = 83;

fn swinging(targets: Vec<CombatTarget>) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: physical(),
        targets,
        equipment_items: vec![wielded_planned_weapon(991)],
        spell_lookup: vec![
            debuff(FESTER, "Fester Other VII"),
            debuff(IMPERIL, "Imperil Other VII"),
        ],
        ..FakeAutomation::default()
    }
}

fn settings(focus: bool) -> CombatSettings {
    let mut settings = CombatSettings {
        maximum_range: 20.0,
        selection_method: TargetSelectionMethod::RANGE,
        scan_interval_seconds: 0.05,
        team_focus_debuffed: focus,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    settings
}

/// A teammate's landed debuff on a monster, heard through the hub.
fn landed(fight: &mut Fight, target_id: u32, spell_id: u32, seconds: f64) {
    let sequence = i64::try_from(fight.surface.peers.casts.len()).unwrap() + 1;
    fight.surface.peers.casts.push(PeerCast {
        sequence,
        client_id: 7,
        caster_id: 0x5000_0002,
        target_id,
        spell_id,
        effective_skill: 300,
        seconds_remaining: seconds,
        landed: true,
        ..PeerCast::default()
    });
    fight.observe_remote_casts();
}

/// One pass with the monsters where they now stand.
fn pass(fight: &mut Fight, targets: Vec<CombatTarget>) -> u32 {
    fight.surface.combat_snapshot = physical();
    fight.surface.targets = targets;
    fight.tick(0.25);
    fight.controller.target_id()
}

fn two_drudges(near: f32, far: f32) -> Vec<CombatTarget> {
    vec![
        target(10, "Drudge", near, 0.0),
        target(20, "Drudge", far, 0.0),
    ]
}

/// (a) On an undebuffed monster, a debuffed one in range takes over.
#[test]
fn an_undebuffed_target_gives_way_to_a_debuffed_monster_in_range() {
    let mut fight = Fight::new(swinging(two_drudges(3.0, 9.0)), settings(true));
    fight.toggle();
    assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 10);

    landed(&mut fight, 20, FESTER, 60.0);

    assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 20);
    assert!(
        fight
            .controller
            .last_focus_decision()
            .unwrap()
            .to_string()
            .contains("debuffed")
    );
}

/// (b) Two debuffed monsters, the second gaining a debuff that makes it the
/// usual order's choice (an Imperil this fighter knows ranks a monster
/// first for a melee weapon), distances jittering for fifty passes: one
/// target throughout.
#[test]
fn two_debuffed_monsters_never_trade_places() {
    let mut surface = swinging(two_drudges(3.0, 9.0));
    surface.known_combat_spells = vec![debuff(IMPERIL, "Imperil Other VII")];
    let mut fight = Fight::new(surface, settings(true));
    fight.toggle();
    landed(&mut fight, 10, FESTER, 60.0);
    landed(&mut fight, 20, FESTER, 60.0);
    let first = pass(&mut fight, two_drudges(3.0, 9.0));
    assert_eq!(first, 10);
    // A stronger debuff on the other one, and it comes nearer.
    landed(&mut fight, 20, IMPERIL, 60.0);

    for step in 0..50 {
        let jitter = if step % 2 == 0 { 2.5 } else { -2.5 };
        let chosen = pass(&mut fight, two_drudges(5.0 + jitter, 5.0 - jitter));
        assert_eq!(chosen, first, "pass {step}");
    }
}

/// (c) The debuff on the target runs out while the other monster's holds:
/// one switch, and a teammate recasting on the first (with a debuff that
/// makes it the usual order's choice again) never brings the fighter back.
#[test]
fn a_debuff_running_out_and_recast_does_not_bounce_the_fighter() {
    let mut surface = swinging(two_drudges(3.0, 9.0));
    surface.known_combat_spells = vec![debuff(IMPERIL, "Imperil Other VII")];
    let mut fight = Fight::new(surface, settings(true));
    fight.toggle();
    landed(&mut fight, 10, FESTER, 2.0);
    landed(&mut fight, 20, FESTER, 60.0);
    let mut seen = vec![pass(&mut fight, two_drudges(3.0, 9.0))];
    assert_eq!(seen[0], 10);

    for step in 0..40 {
        if step == 12 {
            landed(&mut fight, 10, FESTER, 60.0);
            landed(&mut fight, 10, IMPERIL, 60.0);
        }
        seen.push(pass(&mut fight, two_drudges(3.0, 9.0)));
    }

    let switches = seen.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert_eq!(switches, 1, "{seen:?}");
    assert_eq!(*seen.last().unwrap(), 20);
}

/// (c) With no other debuffed monster, the expiry and the recast leave the
/// fighter where it is.
#[test]
fn a_lone_debuffed_target_is_kept_through_its_expiry_and_recast() {
    let mut fight = Fight::new(swinging(two_drudges(3.0, 9.0)), settings(true));
    fight.toggle();
    landed(&mut fight, 10, FESTER, 2.0);
    for step in 0..30 {
        if step == 12 {
            landed(&mut fight, 10, FESTER, 60.0);
        }
        assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 10, "pass {step}");
    }
}

/// (d) Focus off: the debuff does not pull the fighter off its target.
#[test]
fn with_focus_off_a_debuffed_monster_does_not_take_over() {
    let mut fight = Fight::new(swinging(two_drudges(3.0, 9.0)), settings(false));
    fight.toggle();
    assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 10);

    landed(&mut fight, 20, FESTER, 60.0);

    assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 10);
}

fn with_olthoi(settings: CombatSettings) -> CombatSettings {
    let mut settings = settings;
    settings.rules.insert(0, rule("name#^Olthoi", 4));
    settings
}

fn drudge_then_olthoi() -> Vec<CombatTarget> {
    vec![
        target(10, "Drudge", 3.0, 0.0),
        target(30, "Olthoi Soldier", 15.0, 60.0),
    ]
}

/// A higher-priority monster arriving mid-fight does not pull the fighter
/// off a debuffed one while focus is on.
#[test]
fn a_higher_priority_arrival_does_not_pull_the_fighter_off_a_debuffed_monster() {
    let mut fight = Fight::new(
        swinging(vec![target(10, "Drudge", 3.0, 0.0)]),
        with_olthoi(settings(true)),
    );
    fight.toggle();
    landed(&mut fight, 10, FESTER, 60.0);
    assert_eq!(pass(&mut fight, vec![target(10, "Drudge", 3.0, 0.0)]), 10);

    assert_eq!(pass(&mut fight, drudge_then_olthoi()), 10);
}

/// Focus off, the arrival wins as it always has; and with focus on, it
/// wins too when nothing is debuffed.
#[test]
fn a_higher_priority_arrival_still_wins_without_a_team_debuff_or_with_focus_off() {
    for (focus, debuffed) in [(false, true), (true, false)] {
        let mut fight = Fight::new(
            swinging(vec![target(10, "Drudge", 3.0, 0.0)]),
            with_olthoi(settings(focus)),
        );
        fight.toggle();
        if debuffed {
            landed(&mut fight, 10, FESTER, 60.0);
        }
        assert_eq!(pass(&mut fight, vec![target(10, "Drudge", 3.0, 0.0)]), 10);

        assert_eq!(
            pass(&mut fight, drudge_then_olthoi()),
            30,
            "{focus} {debuffed}"
        );
    }
}

/// A claim (a teammate's attempt) alone is not a team debuff.
#[test]
fn a_claim_alone_does_not_draw_the_fighter() {
    let mut fight = Fight::new(swinging(two_drudges(3.0, 9.0)), settings(true));
    fight.toggle();
    assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 10);

    fight.surface.peers.casts.push(PeerCast {
        sequence: 1,
        client_id: 7,
        caster_id: 0x5000_0002,
        target_id: 20,
        spell_id: FESTER,
        landed: false,
        ..PeerCast::default()
    });
    fight.observe_remote_casts();

    assert_eq!(pass(&mut fight, two_drudges(3.0, 9.0)), 10);
}
