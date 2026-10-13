//! The fight's choice between monsters, the power bar, and the first magic
//! arms (`CombatControllerTests`, second part).

mod common;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use ac_plugin_abi::projectile::{ProjectilePathResult, ProjectilePathStatus};
use ac_plugin_abi::social::SelectionAction;
use ac_plugin_abi::ui::{MapPoint, ProjectileSample};
use common::{
    FakeAutomation, Fight, fire_attack_rule, in_mode, nav_object, navigation_at, physical,
    physical_with, profile_fixture_weapon, rule, spell, target, wielded_caster,
    wielded_planned_weapon,
};
use mosstank_settings::model::{CombatSettings, TargetSelectionMethod};

fn swinging(targets: Vec<ac_plugin_abi::combat::CombatTarget>) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: physical(),
        targets,
        equipment_items: vec![wielded_planned_weapon(991)],
        ..FakeAutomation::default()
    }
}

fn settings_with(method: TargetSelectionMethod, maximum_range: f64, scan: f64) -> CombatSettings {
    let mut settings = CombatSettings {
        maximum_range,
        selection_method: method,
        scan_interval_seconds: scan,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    settings
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: HigherPriorityRuleWinsEvenWhenTargetIsFarther
#[test]
fn higher_priority_rule_wins_even_when_target_is_farther() {
    let mut settings = settings_with(TargetSelectionMethod::RANGE, 20.0, 0.25);
    settings.rules.insert(0, rule("name#^Olthoi", 4));
    let mut fight = Fight::new(
        swinging(vec![
            target(10, "Drudge", 2.0, 0.0),
            target(20, "Olthoi Soldier", 12.0, 30.0),
        ]),
        settings,
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 20);
    assert!(fight.controller.target_text().contains("Olthoi Soldier"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SamePriorityTargetsDoNotFlipFlopBetweenScans
#[test]
fn same_priority_targets_do_not_flip_flop_between_scans() {
    let mut fight = Fight::new(
        swinging(vec![
            target(10, "Drudge", 3.0, 0.0),
            target(20, "Drudge", 9.0, 0.0),
        ]),
        settings_with(TargetSelectionMethod::RANGE, 20.0, 0.05),
    );

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.combat_snapshot = physical();
    fight.surface.targets = vec![
        target(10, "Drudge", 9.0, 0.0),
        target(20, "Drudge", 2.0, 0.0),
    ];
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(fight.controller.target_text().contains("9.0m"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TargetTextDistanceIsInvariantUnderASwedishCulture
#[test]
fn target_text_distance_is_invariant_under_a_swedish_culture() {
    // The Rust conversion formats invariantly always (COMPAT-1).
    let mut fight = Fight::new(
        swinging(vec![target(10, "Drudge", 9.0, 0.0)]),
        settings_with(TargetSelectionMethod::RANGE, 20.0, 0.05),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.controller.target_text().contains("9.0m"));
    assert!(!fight.controller.target_text().contains("9,0m"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: StickyTargetStillYieldsToAHigherPriorityRule
#[test]
fn sticky_target_still_yields_to_a_higher_priority_rule() {
    let mut settings = settings_with(TargetSelectionMethod::RANGE, 20.0, 0.05);
    settings.rules.insert(0, rule("name#^Olthoi", 4));
    let mut fight = Fight::new(swinging(vec![target(10, "Drudge", 3.0, 0.0)]), settings);

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.combat_snapshot = physical();
    fight.surface.targets = vec![
        target(10, "Drudge", 3.0, 0.0),
        target(20, "Olthoi Soldier", 15.0, 60.0),
    ];
    fight.tick(1.0);

    assert_eq!(fight.surface.last_begin_target, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TargetLockKeepsCurrentTargetAcrossRescan
#[test]
fn target_lock_keeps_current_target_across_rescan() {
    let mut settings = CombatSettings {
        target_lock: true,
        scan_interval_seconds: 0.1,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(swinging(vec![target(10, "First", 4.0, 20.0)]), settings);
    fight.toggle();
    fight.tick(0.1);

    fight.surface.combat_snapshot = physical();
    fight.surface.targets = vec![
        target(10, "First", 4.0, 20.0),
        target(11, "Closer", 1.0, 0.0),
    ];
    fight.tick(0.1);

    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(fight.controller.target_text().contains("First"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: PreviousValidTargetWinsAngleTieBreakWithoutTargetLock
#[test]
fn previous_valid_target_wins_angle_tie_break_without_target_lock() {
    let mut settings = CombatSettings {
        selection_method: TargetSelectionMethod::ANGLE,
        target_lock: false,
        scan_interval_seconds: 0.1,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(
        swinging(vec![
            target(10, "First", 4.0, 1.0),
            target(11, "Second", 4.0, 20.0),
        ]),
        settings,
    );
    fight.toggle();
    fight.tick(0.1);

    fight.surface.combat_snapshot = physical();
    fight.surface.targets = vec![
        target(10, "First", 4.0, 20.0),
        target(11, "Second", 4.0, 1.0),
    ];
    fight.tick(0.1);

    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(fight.controller.target_text().contains("First"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: PreviousTargetDoesNotBeatNewHigherPriorityRule
#[test]
fn previous_target_does_not_beat_new_higher_priority_rule() {
    let mut settings = CombatSettings {
        selection_method: TargetSelectionMethod::RANGE,
        scan_interval_seconds: 0.1,
        ..CombatSettings::default()
    };
    settings.rules.insert(0, rule("name#^Olthoi", 4));
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(swinging(vec![target(10, "Drudge", 2.0, 0.0)]), settings);
    fight.toggle();
    fight.tick(0.1);

    fight.surface.combat_snapshot = physical();
    fight.surface.targets = vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Olthoi Soldier", 4.0, 20.0),
    ];
    fight.tick(1.0);

    assert_eq!(fight.surface.last_begin_target, 20);
    assert!(fight.controller.target_text().contains("Olthoi Soldier"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: BothSelectionUsesAngleForNearTargetsAndRangeWhenNoneAreNear
#[test]
fn both_selection_uses_angle_for_near_targets_and_range_when_none_are_near() {
    let mut settings = CombatSettings {
        maximum_range: 20.0,
        selection_method: TargetSelectionMethod::BOTH,
        target_select_angle_range: 10.0,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(
        swinging(vec![
            target(10, "Near side", 3.0, 80.0),
            target(11, "Near ahead", 8.0, 5.0),
            target(12, "Far", 20.0, 0.0),
        ]),
        settings,
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 11);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: PhysicalAttackWaitsForConfiguredPowerBeforeRelease
#[test]
fn physical_attack_waits_for_configured_power_before_release() {
    let mut settings = CombatSettings {
        attack_power: 0.6,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(swinging(vec![target(10, "Drudge", 2.0, 0.0)]), settings);
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.begin_count, 1);

    let mut snapshot = physical_with(true, true, 0.59);
    snapshot.desired_power = 0.6;
    fight.surface.combat_snapshot = snapshot;
    fight.tick(0.01);
    assert_eq!(fight.surface.release_count, 0);

    let mut snapshot = physical_with(true, true, 0.60);
    snapshot.desired_power = 0.6;
    fight.surface.combat_snapshot = snapshot;
    fight.tick(0.01);
    assert_eq!(fight.surface.release_count, 1);
}

/// `PetDevice`: a frost pet essence.
fn pet_device(id: u32, wcid: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: wcid,
        name: "Frost Pet".to_owned(),
        container_id: 1,
        stack_size: 1,
        structure: 50,
        maximum_structure: 50,
        pet_class: 0,
        summoning_mastery: 3,
        use_requires_skill: 54,
        use_requires_skill_level: 100,
        ..InventoryItem::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheAttackNeitherSummonsNorWaitsOnAPet
#[test]
fn the_attack_neither_summons_nor_waits_on_a_pet() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        item_entries: vec![pet_device(88, 49387)],
        ..FakeAutomation::default()
    };
    let mut settings = CombatSettings {
        summon_pets: true,
        ..CombatSettings::default()
    };
    settings.combat_item_object_ids.insert(88);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    assert_ne!(fight.surface.last_used_item, 88);
    assert!(!fight.status().contains("Summoning"));
}

fn magic_surface(attack: SpellInfo) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        known_attack_spells: vec![attack],
        equipment_items: vec![wielded_caster(990)],
        ..FakeAutomation::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: MagicModeCastsBestProjectedAttackOnExplicitTarget
#[test]
fn magic_mode_casts_best_projected_attack_on_explicit_target() {
    let mut fight = Fight::new(
        magic_surface(spell(100, "Incantation of Flame Bolt")),
        fire_attack_rule(CombatSettings::default()),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(fight.status().contains("Flame Bolt"));
}

fn turning_surface() -> FakeAutomation {
    let mut surface = magic_surface(SpellInfo {
        requires_turn_to: true,
        ..spell(100, "Incantation of Flame Bolt")
    });
    surface.navigation_snapshot = navigation_at(0.0);
    surface
        .navigation_objects
        .insert(10, nav_object(10, "Drudge", 0.1, 0.0));
    surface
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: BreakableTurnFacesTargetBeforeDispatchingTargetedSpell
#[test]
fn breakable_turn_faces_target_before_dispatching_targeted_spell() {
    let mut fight = Fight::new(
        turning_surface(),
        fire_attack_rule(CombatSettings {
            use_breakable_turn_to: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.movement_intents.is_empty());
    assert_eq!(fight.surface.faced_headings, [90.0]);
    assert!(fight.surface.cast_spell_ids.is_empty());

    fight.surface.navigation_snapshot = navigation_at(90.0);
    fight.tick(0.25);

    assert_eq!(fight.surface.cast_spell_ids, [100]);
    assert_eq!(fight.surface.clear_movement_count, 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: BreakableTurnDoesNotReissueFaceHeadingInsideSevenTenthsOfASecond
#[test]
fn breakable_turn_does_not_reissue_face_heading_inside_seven_tenths_of_a_second() {
    let mut fight = Fight::new(
        turning_surface(),
        fire_attack_rule(CombatSettings {
            use_breakable_turn_to: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.293);
    fight.tick(0.293);

    assert_eq!(fight.surface.faced_headings, [90.0]);
    assert!(fight.surface.cast_spell_ids.is_empty());

    fight.tick(0.293);
    assert_eq!(fight.surface.faced_headings.len(), 1);
    fight.tick(0.293);
    assert_eq!(fight.surface.faced_headings.len(), 2);
}

fn projectile_surface(path: ProjectilePathResult) -> FakeAutomation {
    let mut surface = magic_surface(SpellInfo {
        is_projectile: true,
        ..spell(100, "Incantation of Flame Bolt")
    });
    surface.projectile_path = path;
    surface
}

fn path(status: ProjectilePathStatus) -> ProjectilePathResult {
    ProjectilePathResult {
        status,
        ..ProjectilePathResult::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ProjectileAwarenessBlocksSpellBeforeCastDispatch
#[test]
fn projectile_awareness_blocks_spell_before_cast_dispatch() {
    let mut fight = Fight::new(
        projectile_surface(ProjectilePathResult {
            collision_checks: 3,
            blocking_object_id: 0x5000_0001,
            ..path(ProjectilePathStatus::Blocked)
        }),
        fire_attack_rule(CombatSettings {
            use_projectile_awareness: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.is_empty());
    assert_eq!(fight.surface.last_projectile_target, 10);
    assert!(!fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ProjectileAwarenessCanBeExplicitlyDisabled
#[test]
fn projectile_awareness_can_be_explicitly_disabled() {
    let mut fight = Fight::new(
        projectile_surface(path(ProjectilePathStatus::Blocked)),
        fire_attack_rule(CombatSettings {
            use_projectile_awareness: false,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.contains(&100));
    assert_eq!(fight.surface.last_projectile_target, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AClientThatCannotTestAFlightDoesNotStopTheCast
#[test]
fn a_client_that_cannot_test_a_flight_does_not_stop_the_cast() {
    let mut fight = Fight::new(
        projectile_surface(path(ProjectilePathStatus::Unavailable)),
        fire_attack_rule(CombatSettings {
            use_projectile_awareness: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.contains(&100));
    assert!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("cannot test projectile paths"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: CollisionDebugPublishesDiagnosticSamplesToTheGraphicalHost
#[test]
fn collision_debug_publishes_diagnostic_samples_to_the_graphical_host() {
    let samples = vec![ProjectileSample {
        position: MapPoint {
            east_west: 1.0,
            north_south: 2.0,
            elevation: 3.0,
        },
        is_clear: false,
        radius: 0.4,
    }];
    let mut fight = Fight::new(
        projectile_surface(ProjectilePathResult {
            collision_checks: 1,
            debug_samples: samples.clone(),
            ..path(ProjectilePathStatus::Blocked)
        }),
        fire_attack_rule(CombatSettings {
            use_projectile_awareness: true,
            show_collision_debug: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.shown_projectile_debug_samples, samples);
}

fn jiggle_fight() -> Fight {
    let attack = SpellInfo {
        is_projectile: false,
        school: 34,
        difficulty: 300,
        ..spell(100, "Incantation of Flame Bolt")
    };
    let mut fight = Fight::new(
        magic_surface(attack),
        CombatSettings {
            use_projectile_awareness: false,
            do_jiggle: true,
            ..CombatSettings::default()
        },
    );
    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = CastCompletion {
        revision: 1,
        spell_id: 100,
        target_id: 10,
        weenie_error: 0,
    };
    fight
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: DoJiggleUsesAuthenticSelectionCycleInsteadOfMovingTheCharacter
#[test]
fn do_jiggle_uses_authentic_selection_cycle_instead_of_moving_the_character() {
    let mut fight = jiggle_fight();

    fight.tick(0.01);
    fight.tick(0.131);

    assert_eq!(
        fight.surface.selection_actions,
        [
            SelectionAction::PreviousSelection,
            SelectionAction::NextPlayer,
            SelectionAction::PreviousPlayer,
        ]
    );
    assert!(fight.surface.movement_intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheNudgeStopsAfterItsOwnWindow
#[test]
fn the_nudge_stops_after_its_own_window() {
    let mut fight = jiggle_fight();
    fight.tick(0.01);

    for _ in 0..10 {
        fight.tick(0.131);
    }
    let after_the_window = fight.surface.selection_actions.len();

    for _ in 0..20 {
        fight.tick(0.131);
    }

    assert_eq!(fight.surface.selection_actions.len(), after_the_window);
    assert!((2..=9).contains(&after_the_window), "{after_the_window}");
}
