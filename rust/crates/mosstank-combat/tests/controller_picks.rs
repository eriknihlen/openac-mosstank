//! The retry over monsters, the spell tier and the wand the fight picks,
//! the kill hold, and the listed weapons (`CombatControllerTests`, last
//! part).

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::{CombatMode, CombatTarget};
use ac_plugin_abi::items::EquipmentObservation;
use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use ac_plugin_abi::projectile::{ProjectilePathResult, ProjectilePathStatus};
use ac_plugin_abi::world::ObjectClass;
use common::{
    FakeAutomation, Fight, debuff, debuff_only, equipment, fire_attack_rule, in_mode, magic_spell,
    nav_object, navigation_at, only_rule, peaceful, physical, profile_fixture_weapon, rule_with,
    single_rule, skill, spell, target, target_with_health, wielded_caster, wielded_planned_weapon,
};
use mosstank_automation::cast_tracker::{CastRequest, SpellCastTracker};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable};
use mosstank_combat::CombatModeGate;
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::model::{CombatSettings, LootSettings, TargetSelectionMethod};
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions};

fn magic(targets: Vec<CombatTarget>) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets,
        equipment_items: vec![wielded_caster(990)],
        ..FakeAutomation::default()
    }
}

fn projectile_bolt() -> SpellInfo {
    SpellInfo {
        is_projectile: true,
        ..spell(100, "Incantation of Flame Bolt")
    }
}

fn blocked() -> ProjectilePathResult {
    ProjectilePathResult {
        status: ProjectilePathStatus::Blocked,
        collision_checks: 3,
        blocking_object_id: 0x5000_0001,
        ..ProjectilePathResult::default()
    }
}

fn ranged() -> CombatSettings {
    CombatSettings {
        maximum_range: 40.0,
        ..CombatSettings::default()
    }
}

fn has(log: &[String], entry: &str) -> bool {
    log.iter().any(|call| call == entry)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AMonsterBehindCoverIsPassedOverForAReachableOneInTheSamePass
#[test]
fn a_monster_behind_cover_is_passed_over_for_a_reachable_one_in_the_same_pass() {
    let mut surface = magic(vec![
        target(10, "Drudge", 3.0, 0.0),
        target(20, "Mosswart", 6.0, 10.0),
    ]);
    surface.known_attack_spells = vec![projectile_bolt()];
    // The near one cannot be reached; the far one can.
    surface.projectile_paths.insert(10, blocked());
    let mut fight = Fight::new(
        surface,
        fire_attack_rule(CombatSettings {
            maximum_range: 40.0,
            selection_method: TargetSelectionMethod::RANGE,
            use_projectile_awareness: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheWalkDoesNotInheritTheColumnTheAttackTurnedOff
#[test]
fn the_walk_does_not_inherit_the_column_the_attack_turned_off() {
    let mut surface = magic(vec![
        target(10, "Drudge", 4.0, 0.0),
        target(20, "Mosswart", 15.0, 10.0),
    ]);
    surface.known_attack_spells = vec![projectile_bolt()];
    surface.navigation_snapshot = navigation_at(0.0);
    // Nothing can be thrown at the near one.
    surface.projectile_paths.insert(10, blocked());
    // Somewhere for the walk to go if it wrongly picks the far one.
    surface
        .navigation_objects
        .insert(20, nav_object(20, "Mosswart", 0.1, 0.0));
    let mut fight = Fight::new(
        surface,
        fire_attack_rule(CombatSettings {
            maximum_range: 5.0,
            approach_distance: 20.0,
            selection_method: TargetSelectionMethod::RANGE,
            use_projectile_awareness: true,
            scan_interval_seconds: 0.05,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    // The attack pass ran and found nothing it could do.
    assert_eq!(fight.surface.last_targeted_cast, (0, 0));

    // Same pass, the walk's turn: the near monster is back in the running
    // and already close enough, so there is nowhere to walk.
    assert!(!fight.approach(true));
    assert!(fight.surface.movement_intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheRetryStopsAsSoonAsNothingIsLeftToChooseFrom
#[test]
fn the_retry_stops_as_soon_as_nothing_is_left_to_choose_from() {
    let mut surface = magic(vec![target(10, "Drudge", 3.0, 0.0)]);
    surface.known_attack_spells = vec![projectile_bolt()];
    surface.projectile_path = ProjectilePathResult {
        status: ProjectilePathStatus::Blocked,
        ..ProjectilePathResult::default()
    };
    let mut fight = Fight::new(
        surface,
        fire_attack_rule(CombatSettings {
            maximum_range: 40.0,
            use_projectile_awareness: true,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.is_empty());
    assert!(!fight.has_target());
    // Three shapes tried against the one monster, then nothing is left.
    assert!((1..=8).contains(&fight.surface.projectile_path_checks));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ATierThePackCannotPayForIsNotPicked
#[test]
fn a_tier_the_pack_cannot_pay_for_is_not_picked() {
    let mut surface = magic(vec![target(10, "Drudge", 5.0, 0.0)]);
    surface.known_combat_spells = vec![
        magic_spell(100, "Flame Bolt VII", 300),
        magic_spell(101, "Flame Bolt IV", 150),
    ];
    surface.missing_component_spell_ids.insert(100);
    let mut fight = Fight::new(surface, fire_attack_rule(ranged()));

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (101, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: NoTierIsPickedWhenThePackHasNoComponentsAtAll
#[test]
fn no_tier_is_picked_when_the_pack_has_no_components_at_all() {
    let mut surface = magic(vec![target(10, "Drudge", 5.0, 0.0)]);
    surface.known_combat_spells = vec![magic_spell(100, "Flame Bolt VII", 300)];
    surface.missing_component_spell_ids.insert(100);
    let mut fight = Fight::new(surface, fire_attack_rule(ranged()));

    fight.toggle();
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AWeaponsStanceComesFromItsClassNotItsNumbers
#[test]
fn a_weapons_stance_comes_from_its_class_not_its_numbers() {
    let thrown = equipment(1, "Throwing Dagger", 0x0002)
        .item_type(0x100)
        .ammo(0, 1)
        .build();
    let mut fists = equipment(2, "Training Wraps", 0x0001)
        .damage(0)
        .item_type(0x1)
        .build();
    fists.weapon_skill = 0;

    assert_eq!(CombatModeGate::mode_for(&thrown), CombatMode::Missile);
    assert_eq!(CombatModeGate::mode_for(&fists), CombatMode::Melee);
    let mut unknown = thrown;
    unknown.object_class = ObjectClass::Unknown;
    assert_eq!(CombatModeGate::mode_for(&unknown), CombatMode::Magic);
    assert!(!CombatModeGate::is_caster(&unknown));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARolledElementAdvancesEvenOnAPassThatOnlyDebuffs
#[test]
fn a_rolled_element_advances_even_on_a_pass_that_only_debuffs() {
    let mut surface = magic(vec![target(10, "Drudge", 5.0, 0.0)]);
    surface.known_combat_spells = vec![
        debuff(70, "Piercing Vulnerability Other VII"),
        debuff(71, "Bludgeoning Vulnerability Other VII"),
    ];
    let mut fight = Fight::new(
        surface,
        only_rule(
            ranged(),
            rule_with(
                "DEFAULT",
                MonsterRuleActions {
                    flags: MonsterActionFlags::ATTACK | MonsterActionFlags::VULNERABILITY,
                    damage_type: MonsterDamageType::Random,
                    extra_vulnerability: MonsterDamageType::None,
                    ..MonsterRuleActions::default()
                },
            ),
        ),
    );

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast.0, 70);

    // The first debuff is still in flight, so let it finish.
    fight.surface.last_cast_completion = CastCompletion {
        revision: 1,
        spell_id: 70,
        target_id: 10,
        weenie_error: 0,
    };
    fight.tick(0.25);
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast.0, 71);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AZeroWeaponColumnMeansAWandAndSelectsNothing
#[test]
fn a_zero_weapon_column_means_a_wand_and_selects_nothing() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        known_combat_spells: vec![magic_spell(100, "Flame Bolt VII", 300)],
        equipment_items: vec![
            equipment(990, "Fixture Wand", 0).item_type(0x8000).build(),
            equipment(991, "Fire Sword", 0x0010).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        ranged(),
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                damage_type: MonsterDamageType::Fire,
                weapon_to_use_raw: 0,
                ..MonsterRuleActions::default()
            },
        ),
    );
    settings.combat_item_names.insert("Fixture Wand".to_owned());
    settings.combat_item_names.insert("Fire Sword".to_owned());
    settings.combat_item_order_ids.push(990);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_equip_object_id, 990);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheFirstProfiledWandWinsWithoutAnAlphabeticalTieBreak
#[test]
fn the_first_profiled_wand_wins_without_an_alphabetical_tie_break() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        known_combat_spells: vec![debuff(83, "Imperil Other VII")],
        equipment_items: vec![
            equipment(990, "Zephyr Wand", 0).item_type(0x8000).build(),
            equipment(991, "Acid Wand", 0).item_type(0x8000).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.maximum_range = 40.0;
    for name in ["Zephyr Wand", "Acid Wand"] {
        settings.combat_item_names.insert(name.to_owned());
        settings.combat_item_order.push(name.to_owned());
    }
    settings.combat_item_order_ids.push(990);
    settings.combat_item_order_ids.push(991);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_equip_object_id, 990);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARolledElementThrowsTheFirstRungOfItsWarFamily
#[test]
fn a_rolled_element_throws_the_first_rung_of_its_war_family() {
    let mut surface = magic(vec![target(10, "Drudge", 5.0, 0.0)]);
    surface.known_combat_spells = vec![
        magic_spell(100, "Force Bolt I", 50),
        magic_spell(101, "Force Bolt VII", 300),
    ];
    let mut fight = Fight::new(
        surface,
        only_rule(
            ranged(),
            single_rule(MonsterActionFlags::ATTACK, MonsterDamageType::Random),
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ACastTearsDownAnArmedSwingFirst
#[test]
fn a_cast_tears_down_an_armed_swing_first() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        known_combat_spells: vec![magic_spell(100, "Flame Bolt VII", 300)],
        equipment_items: vec![
            equipment(990, "Fixture Wand", 0).item_type(0x8000).build(),
            wielded_planned_weapon(991),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = fire_attack_rule(ranged());
    profile_fixture_weapon(&mut settings);
    settings.combat_item_names.insert("Fixture Wand".to_owned());
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.begin_count, 1);
    let aborts_after_the_swing = fight.surface.abort_count;

    // The rule flips to magic; the swing must be torn down as the cast goes
    // out.
    fight.surface.combat_snapshot.mode = CombatMode::Magic;
    fight.surface.equipment_items = vec![
        equipment(990, "Fixture Wand", 0)
            .item_type(0x8000)
            .equipped(0x0010_0000)
            .build(),
    ];
    fight.surface.emit_placement(EquipmentObservation {
        object_id: 991,
        equipped_location: 0,
        is_removal: true,
        ..EquipmentObservation::default()
    });
    fight.surface.emit_placement(EquipmentObservation {
        object_id: 990,
        equipped_location: 0x0010_0000,
        is_removal: false,
        ..EquipmentObservation::default()
    });
    fight.settings.rules[0] = rule_with(
        "DEFAULT",
        MonsterRuleActions {
            flags: MonsterActionFlags::ATTACK,
            damage_type: MonsterDamageType::Fire,
            weapon_name: "Fixture Wand".to_owned(),
            ..MonsterRuleActions::default()
        },
    );
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(fight.surface.abort_count > aborts_after_the_swing);
}

/// A war spell's kill heard by the shared tracker, with the locks bound and
/// the route hold answering `holds`.
fn spell_kill(holds: bool) -> Rc<RefCell<ActionLockTable>> {
    let locks = Rc::new(RefCell::new(ActionLockTable::new()));
    let mut fight = Fight::new(FakeAutomation::default(), CombatSettings::default());
    fight
        .controller
        .bind_action_locks(locks.clone(), Box::new(move || holds));
    let tracker = fight.controller.cast_tracker();
    {
        let mut tracker = tracker.borrow_mut();
        tracker.begin(&CastRequest {
            saying: "testwords",
            school: SpellCastTracker::WAR_MAGIC_SCHOOL,
            can_kill: true,
            ..CastRequest::new(1, "Flame Bolt VII", 30, "Drudge", false)
        });
        tracker.observe_chat(0, "testwords", true, 0x11);
        tracker.observe_chat(1, "You killed Drudge!", false, 0);
    }
    fight.with(|controller, cx| controller.drain_cast_events(cx));
    locks
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASpellKillHoldsNavigationForThreeSecondsWhileLootingIsOn
#[test]
fn a_spell_kill_holds_navigation_for_three_seconds_while_looting_is_on() {
    let locks = spell_kill(true);

    assert!(locks.borrow_mut().is_locked(ActionLockKind::Navigation));

    locks.borrow_mut().advance(2.9);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::Navigation));

    locks.borrow_mut().advance(0.2);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASpellKillDoesNotHoldNavigationWhileLootingIsOff
#[test]
fn a_spell_kill_does_not_hold_navigation_while_looting_is_off() {
    let locks = spell_kill(false);

    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASpellKillDoesNotHoldNavigationWhileLootingOnlyRares
#[test]
fn a_spell_kill_does_not_hold_navigation_while_looting_only_rares() {
    let loot = LootSettings {
        enabled: true,
        loot_only_rare_corpses: true,
        ..LootSettings::default()
    };
    let locks = spell_kill(loot.holds_route_after_kill());

    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheKillHoldFollowsLootingAndRareOnly
#[test]
fn the_kill_hold_follows_looting_and_rare_only() {
    for (enabled, rare_only, holds) in [
        (true, false, true),
        (true, true, false),
        (false, false, false),
        (false, true, false),
    ] {
        let loot = LootSettings {
            enabled,
            loot_only_rare_corpses: rare_only,
            ..LootSettings::default()
        };
        assert_eq!(
            loot.holds_route_after_kill(),
            holds,
            "{enabled} {rare_only}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnUnlistedCopyOfAListedWeaponIsNeverChosen
#[test]
fn an_unlisted_copy_of_a_listed_weapon_is_never_chosen() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![
            target(10, "Plasma Golem", 1.8, 0.0),
            target(11, "Banderling Savage", 2.1, 8.0),
        ],
        equipment_items: vec![
            equipment(700, "Wings of Rakhil", 0)
                .item_type(0x8000)
                .build(),
            // In hand. Same name as the listed-but-absent row 999, a
            // different object, and on no list of its own.
            equipment(900, "Decapitator's Blade", 0x0001)
                .equipped(0x0200_0000)
                .valid(0x0200_0000)
                .build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        CombatSettings {
            maximum_range: 8.0,
            ..CombatSettings::default()
        },
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                ..MonsterRuleActions::default()
            },
        ),
    );
    settings
        .combat_item_names
        .insert("Wings of Rakhil".to_owned());
    settings
        .combat_item_names
        .insert("Decapitator's Blade".to_owned());
    settings.combat_item_object_ids.insert(700);
    settings.combat_item_order_ids.push(700);
    // Listed, and no longer in the pack.
    settings.combat_item_object_ids.insert(999);
    settings.combat_item_order_ids.push(999);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..8 {
        fight.tick(0.25);
    }

    assert!(!has(&fight.surface.call_log, "Equip:00000384"));
    assert!(!has(&fight.surface.call_log, "Equip:000002BC"));
    assert_eq!(fight.surface.begin_count, 0);
    assert_eq!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .filter(|message| message.contains("Add the weapon you fight with to the Items list."))
            .count(),
        1
    );
    // The fight steps aside; it does not stop the macro.
    assert!(fight.controller.enabled());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AListedWeaponIsKeptBetweenKillsRatherThanSwappingToTheWand
#[test]
fn a_listed_weapon_is_kept_between_kills_rather_than_swapping_to_the_wand() {
    let mut rated_wand = equipment(700, "Wings of Rakhil", 0)
        .item_type(0x8000)
        .build();
    rated_wand.imbued_effect = 0x0001;
    let held_blade = equipment(900, "Decapitator's Blade", 0x0001)
        .equipped(0x0200_0000)
        .valid(0x0200_0000)
        .build();
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        // War magic is trained, so the wand is a weapon the walk will reach
        // for: the last rung, answering for every monster no listed weapon's
        // element suits.
        character_skills: vec![skill(34, "War Magic", Training::Trained, 300)],
        targets: vec![
            target(10, "Plasma Golem", 1.8, 0.0),
            target(11, "Banderling Savage", 2.1, 8.0),
        ],
        equipment_items: vec![rated_wand, held_blade],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        CombatSettings {
            maximum_range: 8.0,
            selection_method: TargetSelectionMethod::RANGE,
            scan_interval_seconds: 0.05,
            ..CombatSettings::default()
        },
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                ..MonsterRuleActions::default()
            },
        ),
    );
    for id in [700, 900] {
        settings.combat_item_object_ids.insert(id);
        settings.combat_item_order_ids.push(id);
    }
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    // The first monster dies with the second still at arm's length.
    fight.surface.targets = vec![
        target_with_health(10, "Plasma Golem", 1.8, 0.0, 0.0),
        target(11, "Banderling Savage", 2.1, 8.0),
    ];
    fight.surface.unattackable_targets.insert(10);
    fight.surface.begin_targets.clear();
    for _ in 0..4 {
        fight.tick(0.5);
    }

    assert!(!has(&fight.surface.call_log, "Equip:000002BC"));
    assert!(!has(&fight.surface.call_log, "EnterMode:Magic"));
    assert_eq!(fight.surface.last_begin_target, 11);
}
