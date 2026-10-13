//! The fight's magic arm: debuff receipts, rings, drains, ghosts, arcs,
//! the flight check and the streak (`CombatControllerTests`, third part).

mod common;

use ac_plugin_abi::combat::{AttackHeight, CombatMode, CombatTarget};
use ac_plugin_abi::magic::{CastCompletion, CastGate, SpellInfo};
use ac_plugin_abi::projectile::{ProjectilePathKind, ProjectilePathResult, ProjectilePathStatus};
use common::{
    FakeAutomation, Fight, debuff_only, equipment, fire_attack_rule, fixture_facts, game_info,
    in_mode, inventory_item, learned_debuff, magic_spell, only_rule, single_rule, skill, spell,
    target, target_with_health, wielded_caster,
};
use mosstank_automation::cast_tracker::SpellCastTrackerState;
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;
use mosstank_settings::model::{CombatSettings, MeleeAttackHeight, UseArcsMode};
use mosstank_settings::monster::MonsterActionFlags;

use ac_plugin_abi::character::Training;

fn magic(targets: Vec<CombatTarget>, known: Vec<SpellInfo>) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets,
        known_combat_spells: known,
        equipment_items: vec![wielded_caster(990)],
        ..FakeAutomation::default()
    }
}

fn ranged(maximum_range: f64) -> CombatSettings {
    CombatSettings {
        maximum_range,
        ..CombatSettings::default()
    }
}

fn completion(revision: i64, spell_id: u32, target_id: u32, error: u32) -> CastCompletion {
    CastCompletion {
        revision,
        spell_id,
        target_id,
        weenie_error: error,
    }
}

/// The imperil-then-bolt fight the receipt cases share.
fn imperil_then_bolt() -> Fight {
    let mut surface = magic(
        vec![target(10, "Drudge", 5.0, 0.0)],
        vec![learned_debuff(90, "Imperil Other VII")],
    );
    surface.known_attack_spells = vec![spell(100, "Incantation of Flame Bolt")];
    Fight::new(
        surface,
        only_rule(
            CombatSettings::default(),
            single_rule(
                MonsterActionFlags::IMPERIL | MonsterActionFlags::ATTACK,
                MonsterDamageType::Fire,
            ),
        ),
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: MagicRuleDebuffsAndWaitsForServerReceiptBeforeAttack
#[test]
fn magic_rule_debuffs_and_waits_for_server_receipt_before_attack() {
    let mut fight = imperil_then_bolt();

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (90, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&100));

    fight.surface.last_cast_completion = completion(1, 90, 10, 0);
    fight.tick(0.25);
    assert!(fight.surface.cast_spell_ids.contains(&100));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ALearnedDebuffCastIsReportedInFlightUntilTheServerAnswers
#[test]
fn a_learned_debuff_cast_is_reported_in_flight_until_the_server_answers() {
    let mut fight = imperil_then_bolt();
    assert!(!fight.controller.learned_debuff_cast_in_flight());

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (90, 10));
    assert!(fight.controller.learned_debuff_cast_in_flight());

    fight.surface.last_cast_completion = completion(1, 90, 10, 0);
    fight.tick(0.25);
    assert!(!fight.controller.learned_debuff_cast_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheServersAnswerToALearnedDebuffIsReadOnTheFrameWithoutATurn
#[test]
fn the_servers_answer_to_a_learned_debuff_is_read_on_the_frame_without_a_turn() {
    let mut fight = imperil_then_bolt();

    fight.toggle();
    fight.tick(0.25);
    assert!(fight.controller.learned_debuff_cast_in_flight());

    // No tick from here: the pass is held, only frames run.
    fight.surface.last_cast_completion = completion(1, 90, 10, 0);
    fight
        .controller
        .observe_learned_debuff_receipt(&mut fight.surface, 0.05);

    assert!(!fight.controller.learned_debuff_cast_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ABusyRefusalHoldsTheNextDebuffRequestForItsWait
#[test]
fn a_busy_refusal_holds_the_next_debuff_request_for_its_wait() {
    let mut fight = imperil_then_bolt();

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (90, 10));
    let issued = fight.surface.cast_spell_ids.len();

    // The server's answer: refused, 0x1D, "You're too busy!".
    fight.surface.last_cast_completion = completion(1, 90, 10, 0x1D);
    fight
        .controller
        .observe_learned_debuff_receipt(&mut fight.surface, 0.05);
    assert!(!fight.controller.learned_debuff_cast_in_flight());

    // The next pass, a frame later: nothing new goes out.
    fight.tick(0.05);
    assert_eq!(fight.surface.cast_spell_ids.len(), issued);

    // Once the wait is up, the macro is free to try again.
    fight.tick(0.3);
    assert_eq!(fight.surface.cast_spell_ids.len(), issued + 1);
}

fn ring_of_fire() -> SpellInfo {
    SpellInfo {
        target_mask: 0,
        is_untargeted: true,
        ..magic_spell(110, "Cassius' Ring of Fire", 300)
    }
}

/// `CastRingScenario`.
fn cast_ring_scenario(
    known: &[SpellInfo],
    flags: MonsterActionFlags,
    distance: f32,
    ring_distance: f64,
) -> (u32, (u32, u32)) {
    let mut fight = Fight::new(
        magic(vec![target(10, "Drudge", distance, 0.0)], known.to_vec()),
        only_rule(
            CombatSettings {
                maximum_range: 40.0,
                ring_distance,
                ..CombatSettings::default()
            },
            single_rule(flags, MonsterDamageType::Fire),
        ),
    );
    fight.toggle();
    fight.tick(0.25);
    (
        fight.surface.last_untargeted_cast,
        fight.surface.last_targeted_cast,
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: RingArmAlsoRequiresNoStreakColumnAndANonZeroTally
#[test]
fn ring_arm_also_requires_no_streak_column_and_a_non_zero_tally() {
    let known = [
        ring_of_fire(),
        magic_spell(102, "Flame Streak VII", 350),
        magic_spell(100, "Flame Bolt VII", 300),
    ];

    let (untargeted, targeted) = cast_ring_scenario(
        &known,
        MonsterActionFlags::RING | MonsterActionFlags::STREAK,
        3.0,
        5.0,
    );
    assert_eq!(untargeted, 0);
    assert_eq!(targeted.0, 102);

    // Ring only, but the monster is beyond RingDistance so the tally is
    // zero: the ring arm fails and the pass bolts.
    let (untargeted, targeted) = cast_ring_scenario(&known, MonsterActionFlags::RING, 12.0, 5.0);
    assert_eq!(untargeted, 0);
    assert_eq!(targeted.0, 100);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheRingArmAsksForComponentsAndNothingElse
#[test]
fn the_ring_arm_asks_for_components_and_nothing_else() {
    let mut surface = magic(
        vec![target(10, "Drudge", 3.0, 0.0)],
        vec![ring_of_fire(), magic_spell(100, "Flame Bolt VII", 300)],
    );
    // The client is not ready to start the ring this instant.
    surface.cast_gates.insert(110, CastGate::Refused);
    let mut fight = Fight::new(
        surface,
        only_rule(
            CombatSettings {
                maximum_range: 40.0,
                ring_distance: 5.0,
                minimum_ring_targets: 1,
                ..CombatSettings::default()
            },
            single_rule(MonsterActionFlags::RING, MonsterDamageType::Fire),
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_untargeted_cast, 0);
    assert_eq!(fight.surface.last_targeted_cast, (0, 0));
    assert!(fight.status().contains("Cassius' Ring of Fire"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: OnlyAMonsterThePassFollowsAndHasNotGivenUpOnIsPointable
#[test]
fn only_a_monster_the_pass_follows_and_has_not_given_up_on_is_pointable() {
    let mut fight = Fight::new(
        magic(
            vec![target(10, "Drudge", 5.0, 0.0)],
            vec![magic_spell(100, "Flame Bolt VII", 300)],
        ),
        ranged(40.0),
    );

    // Nothing has been scanned yet, so nothing is pointable.
    assert!(!fight.controller.is_tracked_and_not_blacklisted(10));

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.controller.is_tracked_and_not_blacklisted(10));
    assert!(!fight.controller.is_tracked_and_not_blacklisted(11));
    assert!(!fight.controller.is_tracked_and_not_blacklisted(0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: WithNoEquipmentProjectionTheAttackStillWaitsForCombatMode
#[test]
fn with_no_equipment_projection_the_attack_still_waits_for_combat_mode() {
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        equipment_available: false,
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        known_combat_spells: vec![magic_spell(100, "Flame Bolt VII", 300)],
        ..FakeAutomation::default()
    };
    let mut fight = Fight::new(surface, fire_attack_rule(ranged(40.0)));
    fight.toggle();
    fight.tick(0.25);

    // Nothing is cast out of peace mode, and the mode is asked for.
    assert!(fight.surface.cast_spell_ids.is_empty());
    assert!(
        fight
            .surface
            .call_log
            .iter()
            .any(|call| call == "EnterMode:Magic"),
        "{:?}",
        fight.surface.call_log
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheDrainArmCastsWhatThePlannerChose
#[test]
fn the_drain_arm_casts_what_the_planner_chose() {
    let surface = magic(
        vec![CombatTarget {
            weenie_class_id: 1010,
            health_revision: 4,
            ..target_with_health(10, "Olthoi Slasher", 5.0, 0.0, 0.05)
        }],
        vec![
            magic_spell(1237, "Drain Health Other I", 100),
            magic_spell(1238, "Drain Health Other II", 150),
            magic_spell(1239, "Drain Health Other III", 200),
            magic_spell(2760, "Martyr's Hecatomb I", 100),
            magic_spell(2761, "Martyr's Hecatomb II", 150),
            magic_spell(2762, "Martyr's Hecatomb III", 200),
        ],
    );
    let settings = only_rule(
        CombatSettings {
            maximum_range: 40.0,
            monster_facts: fixture_facts(),
            ..CombatSettings::default()
        },
        single_rule(MonsterActionFlags::ATTACK, MonsterDamageType::DrainAuto),
    );
    let mut fight = Fight::with_game_info(surface, settings, game_info());
    fight.toggle();
    fight.tick(0.25);

    // The database lists this monster as unaffectable by magic, so no
    // drain may be planned at all; among the martyrs the plan takes the
    // best monster health taken off per millisecond spent.
    assert_eq!(fight.surface.last_targeted_cast, (2761, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: CastsNoOneEverAnswersGetTheMonsterDeleted
#[test]
fn casts_no_one_ever_answers_get_the_monster_deleted() {
    let settings = fire_attack_rule(CombatSettings {
        maximum_range: 40.0,
        ghost_monster_spell_attempt_count: 3,
        delete_ghost_monsters: true,
        monster_facts: fixture_facts(),
        ..CombatSettings::default()
    });
    let mut fight = Fight::new(
        magic(
            vec![target(10, "Olthoi Slasher", 5.0, 0.0)],
            vec![magic_spell(100, "Flame Bolt VII", 300)],
        ),
        settings,
    );
    fight.toggle();

    // One pass issues the cast; the rule track is then held while it is in
    // flight, which is what the whole re-issue mechanism exists for.
    fight.tick(0.25);
    assert!(fight.surface.cast_spell_ids.contains(&100));
    assert_eq!(
        fight.controller.cast_tracker().borrow().state(),
        SpellCastTrackerState::AwaitingLaunch
    );

    // The server never acknowledges it: the tracker re-sends every 200 ms,
    // and each re-send is one more unanswered attempt. (The tracker's
    // events reach the fight when it next reads them.)
    let tracker = fight.controller.cast_tracker();
    for _ in 0..10 {
        tracker.borrow_mut().advance(0.1);
        fight.with(|controller, cx| controller.drain_cast_events(cx));
    }

    assert_eq!(fight.surface.dismissed_ghosts, [10]);
    assert!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .any(|line| line.contains("Deleting ghost monster Olthoi Slasher"))
    );

    // Deleting it is the whole consequence. There is no permanent verdict
    // on the monster, so the very next pass picks it again.
    fight.surface.cast_spell_ids.clear();
    fight.tick(0.25);
    assert!(fight.surface.cast_spell_ids.contains(&100));
}

/// `StalledHealthScenario`.
fn stalled_health_scenario(
    name: &str,
    by_health_tracker: bool,
    stale_seconds: f64,
) -> FakeAutomation {
    let surface = magic(
        vec![CombatTarget {
            health_revision: 3,
            ..target(10, name, 5.0, 0.0)
        }],
        vec![magic_spell(100, "Flame Bolt VII", 300)],
    );
    let settings = fire_attack_rule(CombatSettings {
        maximum_range: 40.0,
        delete_ghost_monsters_by_health_tracker: by_health_tracker,
        ghost_delete_health_tracker_seconds: stale_seconds,
        monster_facts: fixture_facts(),
        ..CombatSettings::default()
    });
    let mut fight = Fight::new(surface, settings);
    fight.toggle();
    for _ in 0..30 {
        fight.tick(1.0);
    }
    fight.surface
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: OnlyAMonsterWithAKnownHealthCeilingCanGoStaleIntoAGhost
#[test]
fn only_a_monster_with_a_known_health_ceiling_can_go_stale_into_a_ghost() {
    let listed = stalled_health_scenario("Olthoi Slasher", true, 10.0);
    assert!(!listed.dismissed_ghosts.is_empty());
    assert!(listed.dismissed_ghosts.iter().all(|&id| id == 10));
    assert!(
        listed
            .posted_system_messages
            .iter()
            .any(|line| line.contains("due to HP tracker notification"))
    );

    // The database does not list "Drudge", so the client is never told
    // this monster's health in points and its silence means nothing.
    assert!(
        stalled_health_scenario("Drudge", true, 10.0)
            .dismissed_ghosts
            .is_empty()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ForgettingASilentMonsterIsAProfileChoice
#[test]
fn forgetting_a_silent_monster_is_a_profile_choice() {
    for (by_health_tracker, forgets) in [(true, true), (false, false)] {
        let surface = stalled_health_scenario("Olthoi Slasher", by_health_tracker, 10.0);
        assert_eq!(
            !surface.dismissed_ghosts.is_empty(),
            forgets,
            "{by_health_tracker}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheSilenceThatMakesAMonsterAGhostIsTheProfilesOwnLength
#[test]
fn the_silence_that_makes_a_monster_a_ghost_is_the_profiles_own_length() {
    for (stale_seconds, forgets) in [(10.0, true), (60.0, false)] {
        let surface = stalled_health_scenario("Olthoi Slasher", true, stale_seconds);
        assert_eq!(
            !surface.dismissed_ghosts.is_empty(),
            forgets,
            "{stale_seconds}"
        );
    }
}

/// `RingTallyScenario`.
fn ring_tally_scenario(
    known: &[SpellInfo],
    targets: Vec<CombatTarget>,
    minimum_range: f64,
) -> (u32, (u32, u32)) {
    let mut fight = Fight::new(
        magic(targets, known.to_vec()),
        only_rule(
            CombatSettings {
                maximum_range: 40.0,
                ring_distance: 5.0,
                minimum_range,
                minimum_ring_targets: 2,
                ..CombatSettings::default()
            },
            single_rule(
                MonsterActionFlags::RING | MonsterActionFlags::ATTACK,
                MonsterDamageType::Fire,
            ),
        ),
    );
    fight.toggle();
    fight.tick(0.25);
    (
        fight.surface.last_untargeted_cast,
        fight.surface.last_targeted_cast,
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheRingTallyCountsOnlyValidCandidatesStrictlyInsideTheRing
#[test]
fn the_ring_tally_counts_only_valid_candidates_strictly_inside_the_ring() {
    let known = [ring_of_fire(), magic_spell(100, "Flame Bolt VII", 300)];

    // One monster exactly ON the ring boundary, which the strict comparison
    // excludes, so the tally is one and the pass bolts instead.
    assert_eq!(
        ring_tally_scenario(
            &known,
            vec![
                target(10, "Drudge", 3.0, 0.0),
                target(11, "Drudge", 5.0, 0.0)
            ],
            0.0
        )
        .1
        .0,
        100
    );

    // The second monster is nearer than the minimum distance, so it is not
    // a candidate at all and cannot be tallied.
    assert_eq!(
        ring_tally_scenario(
            &known,
            vec![
                target(10, "Drudge", 3.0, 0.0),
                target(11, "Drudge", 0.5, 0.0)
            ],
            1.0
        )
        .1
        .0,
        100
    );

    // Two valid candidates strictly inside the ring: the ring fires.
    assert_eq!(
        ring_tally_scenario(
            &known,
            vec![
                target(10, "Drudge", 3.0, 0.0),
                target(11, "Drudge", 4.0, 0.0)
            ],
            0.0
        )
        .0,
        110
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: StrongerBoltBeatsTheArcEvenWithUseArcsYes
#[test]
fn stronger_bolt_beats_the_arc_even_with_use_arcs_yes() {
    let mut fight = Fight::new(
        magic(
            vec![target(10, "Drudge", 20.0, 0.0)],
            vec![
                magic_spell(100, "Flame Bolt VII", 300),
                magic_spell(101, "Flame Arc IV", 150),
            ],
        ),
        fire_attack_rule(CombatSettings {
            use_arcs: UseArcsMode::YES,
            arc_range: 1.0,
            maximum_range: 40.0,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
}

/// `CastWithUseArcs`.
fn cast_with_use_arcs(
    known: &[SpellInfo],
    mode: UseArcsMode,
    distance: f64,
    arc_range: f64,
) -> (u32, u32) {
    #[allow(clippy::cast_possible_truncation)]
    let distance = distance as f32;
    let mut fight = Fight::new(
        magic(vec![target(10, "Drudge", distance, 0.0)], known.to_vec()),
        fire_attack_rule(CombatSettings {
            use_arcs: mode,
            arc_range,
            maximum_range: 40.0,
            ..CombatSettings::default()
        }),
    );
    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_targeted_cast
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: UseArcsDecidesOnlyAnExactQualityTie
#[test]
fn use_arcs_decides_only_an_exact_quality_tie() {
    let known = [
        magic_spell(100, "Flame Bolt VII", 300),
        magic_spell(101, "Flame Arc VII", 300),
    ];

    assert_eq!(
        cast_with_use_arcs(&known, UseArcsMode::YES, 20.0, 1.0),
        (101, 10)
    );
    assert_eq!(
        cast_with_use_arcs(&known, UseArcsMode::NO, 20.0, 1.0),
        (100, 10)
    );
    // "At range" arcs only from the arc range outwards; inside it the bolt
    // still wins.
    assert_eq!(
        cast_with_use_arcs(&known, UseArcsMode::AT_RANGE, 20.0, 10.0),
        (101, 10)
    );
    assert_eq!(
        cast_with_use_arcs(&known, UseArcsMode::AT_RANGE, 5.0, 10.0),
        (100, 10)
    );
}

fn blocked() -> ProjectilePathResult {
    ProjectilePathResult {
        status: ProjectilePathStatus::Blocked,
        collision_checks: 3,
        blocking_object_id: 0x5000_0001,
        ..ProjectilePathResult::default()
    }
}

fn four_drudges() -> Vec<CombatTarget> {
    vec![
        target(10, "Drudge", 5.0, 0.0),
        target(11, "Drudge", 6.0, 0.0),
        target(12, "Drudge", 7.0, 0.0),
        target(13, "Drudge", 8.0, 0.0),
    ]
}

fn streak_rule(settings: CombatSettings) -> CombatSettings {
    only_rule(
        settings,
        single_rule(MonsterActionFlags::STREAK, MonsterDamageType::Fire),
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ThePerTickBudgetCapsHowManyMonstersOnePassEvaluates
#[test]
fn the_per_tick_budget_caps_how_many_monsters_one_pass_evaluates() {
    for (budget, attempts) in [(2, 2), (4, 4)] {
        let mut surface = magic(
            four_drudges(),
            vec![magic_spell(102, "Flame Streak VII", 350)],
        );
        surface.projectile_path = blocked();
        let mut fight = Fight::new(
            surface,
            streak_rule(CombatSettings {
                maximum_range: 40.0,
                use_projectile_awareness: true,
                maximum_collision_checks_per_tick: budget,
                ..CombatSettings::default()
            }),
        );

        fight.toggle();
        fight.tick(0.25);

        let wanted = format!("Loop iterations: {attempts}");
        assert!(
            fight.log_texts().iter().any(|line| line.ends_with(&wanted)),
            "{budget}: {:?}",
            fight.log_texts()
        );
        assert!(fight.surface.cast_spell_ids.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheBudgetIsIgnoredWhileTheFlightCheckIsOff
#[test]
fn the_budget_is_ignored_while_the_flight_check_is_off() {
    let mut fight = Fight::new(
        magic(four_drudges(), Vec::new()),
        streak_rule(CombatSettings {
            maximum_range: 40.0,
            use_projectile_awareness: false,
            maximum_collision_checks_per_tick: 4,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(
        fight
            .log_texts()
            .iter()
            .any(|line| line.ends_with("Loop iterations: 1")),
        "{:?}",
        fight.log_texts()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AStreakIntoCoverIsRefusedAndTakesItsColumnWithIt
#[test]
fn a_streak_into_cover_is_refused_and_takes_its_column_with_it() {
    let mut surface = magic(
        vec![target(10, "Drudge", 5.0, 0.0)],
        vec![magic_spell(102, "Flame Streak VII", 350)],
    );
    surface.projectile_path = blocked();
    let mut fight = Fight::new(
        surface,
        streak_rule(CombatSettings {
            maximum_range: 40.0,
            use_projectile_awareness: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.is_empty());
    // The monster stops being a candidate once its only column is off, so
    // the pass stops rather than spinning against it.
    assert!(!fight.has_target());
    assert!((1..=4).contains(&fight.surface.projectile_path_checks));
}

/// `ClearanceHeightFor`.
fn clearance_height_for(known: &[SpellInfo], mode: UseArcsMode) -> AttackHeight {
    let mut fight = Fight::new(
        magic(vec![target(10, "Drudge", 5.0, 0.0)], known.to_vec()),
        fire_attack_rule(CombatSettings {
            maximum_range: 40.0,
            use_projectile_awareness: true,
            use_arcs: mode,
            // Deliberately neither of the two shape heights.
            attack_height: MeleeAttackHeight::LOW,
            ..CombatSettings::default()
        }),
    );
    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_projectile_height
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: BoltAndArcClearanceUseTheShapesOwnHeight
#[test]
fn bolt_and_arc_clearance_use_the_shapes_own_height() {
    let known = [
        SpellInfo {
            is_projectile: true,
            ..magic_spell(100, "Flame Bolt VII", 300)
        },
        SpellInfo {
            is_projectile: true,
            ..magic_spell(101, "Flame Arc VII", 300)
        },
    ];

    assert_eq!(
        clearance_height_for(&known, UseArcsMode::NO),
        AttackHeight::Medium
    );
    assert_eq!(
        clearance_height_for(&known, UseArcsMode::YES),
        AttackHeight::High
    );
}

fn imperil_phial_surface() -> FakeAutomation {
    let imperil = SpellInfo {
        school: 31,
        is_debuff: true,
        is_offensive: true,
        duration_seconds: 60.0,
        ..spell(1323, "Imperil Other I")
    };
    let mut phial = inventory_item(200, "Iron Phial of Imperil", 0x100, 0, false);
    phial.combat_use = 0;
    FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Missile),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        spell_lookup: vec![imperil],
        item_entries: vec![phial],
        equipment_items: vec![
            equipment(200, "Iron Phial of Imperil", 0)
                .item_type(0x100)
                .build(),
        ],
        character_skills: vec![skill(38, "Alchemy", Training::Trained, 400)],
        ..FakeAutomation::default()
    }
}

fn phial_settings() -> CombatSettings {
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings
        .consumable_names
        .insert("Iron Phial of Imperil".to_owned());
    settings.use_projectile_awareness = true;
    settings
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GrenadesComeFromTheGameDatabaseInUse
#[test]
fn grenades_come_from_the_game_database_in_use() {
    let mut fight = Fight::with_game_info(
        imperil_phial_surface(),
        phial_settings(),
        VtankGameInfoDatabase::load_default(),
    );

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_projectile_target, 0);

    fight.controller.replace_game_info(game_info());
    fight.tick(0.25);

    assert_eq!(fight.surface.last_projectile_target, 10);
    assert_eq!(
        fight.surface.last_projectile_kind,
        ProjectilePathKind::Missile
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ADebuffsClearanceUsesItsOwnFlightHeight
#[test]
fn a_debuffs_clearance_uses_its_own_flight_height() {
    let mut settings = phial_settings();
    // Deliberately neither of the two shape heights.
    settings.attack_height = MeleeAttackHeight::LOW;
    let mut fight = Fight::with_game_info(imperil_phial_surface(), settings, game_info());

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(
        fight.surface.last_projectile_kind,
        ProjectilePathKind::Missile
    );
    assert_eq!(fight.surface.last_projectile_height, AttackHeight::Low);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheFlightCheckIsAskedForInTheProfilesOwnShape
#[test]
fn the_flight_check_is_asked_for_in_the_profiles_own_shape() {
    for (radius, step) in [(0.4_f64, 0.7_f64), (1.25, 0.2)] {
        let mut surface = magic(
            vec![target(10, "Drudge", 5.0, 0.0)],
            vec![magic_spell(102, "Flame Streak VII", 350)],
        );
        surface.projectile_path = blocked();
        let mut fight = Fight::new(
            surface,
            streak_rule(CombatSettings {
                maximum_range: 40.0,
                use_projectile_awareness: true,
                collision_projectile_radius: radius,
                collision_step_distance: step,
                ..CombatSettings::default()
            }),
        );

        fight.toggle();
        fight.tick(0.25);

        assert!(fight.surface.projectile_path_checks > 0);
        #[allow(clippy::cast_possible_truncation)]
        let (radius, step) = (radius as f32, step as f32);
        assert_eq!(fight.surface.last_projectile_radius, radius);
        assert_eq!(fight.surface.last_projectile_step_distance, step);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: JumpingClearOfAWandCastIsAProfileChoice
#[test]
fn jumping_clear_of_a_wand_cast_is_a_profile_choice() {
    for jumps in [true, false] {
        let imperil = SpellInfo {
            school: 31,
            is_debuff: true,
            is_offensive: true,
            duration_seconds: 60.0,
            ..spell(1323, "Imperil Other VI")
        };
        let surface = FakeAutomation {
            combat_snapshot: in_mode(CombatMode::Magic),
            targets: vec![target(10, "Drudge", 5.0, 0.0)],
            item_entries: vec![inventory_item(
                200,
                "Wand of Imperil",
                0x8000,
                imperil.spell_id,
                true,
            )],
            spell_lookup: vec![imperil],
            equipment_items: vec![
                equipment(200, "Wand of Imperil", 0)
                    .item_type(0x8000)
                    .build(),
            ],
            ..FakeAutomation::default()
        };
        let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
        settings.maximum_range = 40.0;
        settings.jump_out_wand_casting = jumps;
        settings
            .combat_item_names
            .insert("Wand of Imperil".to_owned());
        let mut fight = Fight::new(surface, settings);

        fight.toggle();
        for _ in 0..8 {
            fight.tick(0.25);
        }

        assert_eq!(
            fight
                .surface
                .movement_intents
                .iter()
                .any(|intent| intent.jump),
            jumps,
            "{jumps}"
        );
    }
}

/// `CastAgainstHealth`.
fn cast_against_health(known: &[SpellInfo], health_fraction: f32, name: &str) -> (u32, u32) {
    let surface = magic(
        vec![CombatTarget {
            weenie_class_id: 1010,
            health_revision: 7,
            ..target_with_health(10, name, 5.0, 0.0, health_fraction)
        }],
        known.to_vec(),
    );
    let mut fight = Fight::new(
        surface,
        only_rule(
            CombatSettings {
                maximum_range: 40.0,
                monster_facts: fixture_facts(),
                ..CombatSettings::default()
            },
            single_rule(
                MonsterActionFlags::ATTACK | MonsterActionFlags::STREAK,
                MonsterDamageType::Fire,
            ),
        ),
    );
    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_targeted_cast
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AttackPlusStreakUsesTheStreakOnlyAsAFinishingBlow
#[test]
fn attack_plus_streak_uses_the_streak_only_as_a_finishing_blow() {
    let known = [
        magic_spell(100, "Flame Bolt VII", 300),
        magic_spell(102, "Flame Streak VII", 350),
    ];

    // "Olthoi Slasher" is listed in the fixture database at 2000 health,
    // and the streak's difficulty of 350 sets the bar at 50 points until a
    // real blow is seen: 1800 left bolts, 20 left finishes.
    assert_eq!(cast_against_health(&known, 0.9, "Olthoi Slasher").0, 100);
    assert_eq!(cast_against_health(&known, 0.01, "Olthoi Slasher").0, 102);

    // A monster the database does not list has no health in points at all,
    // so the finishing move can never be chosen for it.
    assert_eq!(cast_against_health(&known, 0.01, "Drudge").0, 100);
}
