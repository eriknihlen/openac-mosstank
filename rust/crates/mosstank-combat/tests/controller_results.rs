//! The fight's elements, its answers from the server, the weapon it
//! wields and its ammunition (`CombatControllerTests`, fifth part).

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use ac_plugin_abi::combat::{CombatMode, CombatTarget};
use ac_plugin_abi::items::{EquipmentItem, InventoryItem, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use ac_plugin_abi::projectile::{ProjectilePathResult, ProjectilePathStatus};
use ac_plugin_abi::types::ChatLine;
use common::{
    AMMUNITION_SLOT, FakeAutomation, Fight, ammo_game_info, debuff, debuff_only, equipment,
    fire_attack_rule, in_mode, inventory_item, magic_spell, missile_skill, only_rule, peaceful,
    physical, physical_with, rule_with, single_rule, spell, target, wielded_caster,
};
use mosstank_automation::cast_tracker::{CastRequest, SpellCastTracker};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable};
use mosstank_combat::failure::{CombatFailureTracker, CombatSuppressionReason};
use mosstank_combat::gate::NO_WAND_NOTICE;
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;
use mosstank_settings::model::CombatSettings;
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions};

fn magic(targets: Vec<CombatTarget>, known: Vec<SpellInfo>) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets,
        known_combat_spells: known,
        equipment_items: vec![wielded_caster(990)],
        ..FakeAutomation::default()
    }
}

fn drudge() -> Vec<CombatTarget> {
    vec![target(10, "Drudge", 5.0, 0.0)]
}

fn ranged() -> CombatSettings {
    CombatSettings {
        maximum_range: 40.0,
        ..CombatSettings::default()
    }
}

fn row(flags: MonsterActionFlags, damage_type: MonsterDamageType) -> CombatSettings {
    only_rule(ranged(), single_rule(flags, damage_type))
}

fn weapon_row(actions: MonsterRuleActions) -> CombatSettings {
    only_rule(CombatSettings::default(), rule_with("DEFAULT", actions))
}

fn cast_line(text: &str) -> ChatLine {
    ChatLine {
        sequence: 1,
        text: text.to_owned(),
        log_text_type: 0x07,
        ..ChatLine::default()
    }
}

fn index_of(log: &[String], entry: &str) -> Option<usize> {
    log.iter().position(|call| call == entry)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AFistsRowCastsTuskerFistsBeforeTheEnchantmentIsUp
#[test]
fn a_fists_row_casts_tusker_fists_before_the_enchantment_is_up() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![
                magic_spell(0x0B76, "Tusker Fists", 300),
                magic_spell(105, "Shock Wave VII", 300),
            ],
        ),
        row(MonsterActionFlags::ATTACK, MonsterDamageType::Fists),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (0x0B76, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AFistsRowWithoutTuskerFistsAttacksWithBludgeon
#[test]
fn a_fists_row_without_tusker_fists_attacks_with_bludgeon() {
    let mut fight = Fight::new(
        magic(drudge(), vec![magic_spell(105, "Shock Wave VII", 300)]),
        row(MonsterActionFlags::ATTACK, MonsterDamageType::Fists),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (105, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: UnknownMonsterFallsToAuthenticUnlistedElementAndCastsNoExtraVuln
#[test]
fn unknown_monster_falls_to_authentic_unlisted_element_and_casts_no_extra_vuln() {
    let mut fight = Fight::new(
        magic(
            vec![target(10, "New Server Creature", 5.0, 0.0)],
            vec![
                magic_spell(100, "Force Bolt VII", 300),
                debuff(85, "Piercing Vulnerability Other VII"),
            ],
        ),
        only_rule(
            ranged(),
            rule_with(
                "DEFAULT",
                MonsterRuleActions {
                    flags: MonsterActionFlags::ATTACK,
                    damage_type: MonsterDamageType::Auto,
                    // Ex. Vuln = Auto resolves through the damage table,
                    // which has nothing for this monster: step 9 never runs.
                    extra_vulnerability: MonsterDamageType::Auto,
                    ..MonsterRuleActions::default()
                },
            ),
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    // Pierce is the first entry of the element walk.
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&85));
    assert!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("Using unlisted damage type: Pierce"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AMonsterGivenUpForWantOfASpellIsToldWhy
#[test]
fn a_monster_given_up_for_want_of_a_spell_is_told_why() {
    let mut surface = magic(
        vec![
            target(10, "Drudge", 5.0, 0.0),
            target(11, "Golem", 6.0, 0.0),
        ],
        vec![
            magic_spell(100, "Flame Bolt VII", 300),
            magic_spell(27, "Flame Bolt I", 1),
        ],
    );
    surface.missing_component_spell_ids.insert(100);
    surface.missing_component_spell_ids.insert(27);
    let mut fight = Fight::new(surface, fire_attack_rule(ranged()));

    fight.toggle();
    fight.tick(0.25);
    fight.tick(0.25);

    let why = "No usable attack spell (Flame Bolt VII and 1 lower: missing components)";
    let said: Vec<&String> = fight
        .surface
        .posted_system_messages
        .iter()
        .filter(|message| message.contains("Not attacking"))
        .collect();
    assert_eq!(
        said,
        [
            &format!("[MossTank] Not attacking Drudge: {why}"),
            &format!("[MossTank] Not attacking Golem: {why}"),
        ]
    );
    let wanted = format!("Attack: Drudge yielded nothing this pass ({why}), choosing again");
    assert!(
        fight.log_texts().contains(&wanted),
        "{:?}",
        fight.log_texts()
    );
}

/// `TimeOutACast`: one cast driven all the way to the result timeout, and
/// the tracker's events handed to the fight.
fn time_out_a_cast(fight: &mut Fight, target_id: u32, spell_id: u32, name: &str, saying: &str) {
    let tracker = fight.controller.cast_tracker();
    {
        let mut tracker = tracker.borrow_mut();
        tracker.reset();
        tracker.begin(&CastRequest {
            saying,
            ..CastRequest::new(spell_id, name, target_id, "", false)
        });
        tracker.observe_chat(0, saying, true, 0x11);
        tracker.advance(SpellCastTracker::RESULT_TIMEOUT_SECONDS + 0.1);
    }
    fight.with(|controller, cx| controller.drain_cast_events(cx));
}

fn bolt_fight(settings: CombatSettings) -> Fight {
    Fight::new(
        magic(drudge(), vec![magic_spell(100, "Flame Bolt VII", 300)]),
        fire_attack_rule(settings),
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASelfCastThatTimesOutIsNotAnnouncedAsAnUnhittableTarget
#[test]
fn a_self_cast_that_times_out_is_not_announced_as_an_unhittable_target() {
    let mut fight = bolt_fight(CombatSettings {
        maximum_range: 40.0,
        scan_interval_seconds: 0.05,
        blacklist_monster_attempt_count: 0,
        blacklist_monster_timeout_seconds: 120.0,
        ..CombatSettings::default()
    });
    fight.toggle();
    fight.tick(0.25);

    let own = 1_342_177_290;
    time_out_a_cast(
        &mut fight,
        own,
        0x1131,
        "Incantation of Flame Bane",
        "MalarQuaTak",
    );
    assert!(
        !fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("Cannot hit"))
    );

    time_out_a_cast(&mut fight, 10, 100, "Flame Bolt VII", "ZojakQuazael");
    assert!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("Cannot hit Drudge (10) \u{2014} blacklisted for")),
        "{:?}",
        fight.surface.posted_system_messages
    );
}

fn hear_own_saying(fight: &mut Fight) {
    let saying = fight.surface.known_combat_spells[0].saying.clone();
    fight
        .controller
        .cast_tracker()
        .borrow_mut()
        .observe_chat(0, &saying, true, 0x11);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: PermanentFailResultTextForceBlacklistsTheTarget
#[test]
fn permanent_fail_result_text_force_blacklists_the_target() {
    let mut fight = bolt_fight(CombatSettings {
        maximum_range: 40.0,
        scan_interval_seconds: 0.05,
        blacklist_monster_timeout_seconds: 300.0,
        ..CombatSettings::default()
    });

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));

    hear_own_saying(&mut fight);
    fight.surface.chat_messages = vec![cast_line("Drudge is an invalid target.")];
    fight.tick(0.25);
    fight.tick(0.25);

    assert!(fight.status().contains("Waiting for a target"));
    assert!(!fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: PermanentFailIsIgnoredWhileAMultiTargetSpellIsInFlight
#[test]
fn permanent_fail_is_ignored_while_a_multi_target_spell_is_in_flight() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![SpellInfo {
                family: 638,
                ..magic_spell(100, "Flame Bolt VII", 300)
            }],
        ),
        fire_attack_rule(CombatSettings {
            maximum_range: 40.0,
            scan_interval_seconds: 0.05,
            blacklist_monster_timeout_seconds: 300.0,
            ..CombatSettings::default()
        }),
    );

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));

    fight.surface.chat_messages = vec![cast_line("Drudge is an invalid target.")];
    fight.tick(0.25);
    fight.tick(0.25);

    assert!(fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AKillLineEndsTheTargetBeforeTheWorldRemovesIt
#[test]
fn a_kill_line_ends_the_target_before_the_world_removes_it() {
    let mut fight = bolt_fight(CombatSettings {
        maximum_range: 40.0,
        scan_interval_seconds: 0.05,
        ..CombatSettings::default()
    });

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    let casts_before_the_kill = fight.surface.cast_spell_ids.len();

    hear_own_saying(&mut fight);
    fight.surface.chat_messages = vec![ChatLine {
        sequence: 1,
        text: "You killed Drudge!".to_owned(),
        ..ChatLine::default()
    }];
    fight.tick(0.25);
    fight.tick(0.25);

    assert!(!fight.has_target());
    assert_eq!(fight.surface.cast_spell_ids.len(), casts_before_the_kill);
    assert!(fight.status().contains("Waiting for a target"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheResultWaitTimeoutBumpsTheBlacklistAttemptCount
#[test]
fn the_result_wait_timeout_bumps_the_blacklist_attempt_count() {
    let mut fight = bolt_fight(CombatSettings {
        maximum_range: 40.0,
        scan_interval_seconds: 0.05,
        // No attempts allowed, so the first one that records trips.
        blacklist_monster_attempt_count: 0,
        blacklist_monster_timeout_seconds: 300.0,
        ..CombatSettings::default()
    });

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));

    hear_own_saying(&mut fight);
    assert!(fight.has_target());

    // 4 x 907 ms of the result timer.
    fight.controller.cast_tracker().borrow_mut().advance(3.7);
    fight.tick(0.25);

    assert!(!fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AServerRefusedCastDoesNotCountAgainstTheTarget
#[test]
fn a_server_refused_cast_does_not_count_against_the_target() {
    let mut fight = bolt_fight(CombatSettings {
        maximum_range: 40.0,
        scan_interval_seconds: 0.05,
        blacklist_monster_attempt_count: 1,
        blacklist_monster_timeout_seconds: 300.0,
        ..CombatSettings::default()
    });

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));

    fight.surface.last_cast_completion = CastCompletion {
        revision: 1,
        spell_id: 100,
        target_id: 10,
        weenie_error: 0x1D,
    };
    fight.tick(0.25);
    fight.tick(0.25);

    assert!(fight.has_target());
}

fn twenty_or_eight(count: u32) -> Vec<CombatTarget> {
    (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let distance = 3.0 + i as f32;
            target(10 + i, "Drudge", distance, 0.0)
        })
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ATwentyCandidateScanDoesNotRebuildThePlanPerCandidate
#[test]
fn a_twenty_candidate_scan_does_not_rebuild_the_plan_per_candidate() {
    let mut fight = Fight::new(
        magic(
            twenty_or_eight(20),
            vec![
                magic_spell(100, "Flame Bolt VII", 300),
                debuff(80, "Fester Other VII"),
                debuff(81, "Broadside of a Barn"),
                debuff(82, "Fire Vulnerability Other VII"),
                debuff(83, "Imperil Other VII"),
                debuff(84, "Magic Yield Other VII"),
            ],
        ),
        row(
            MonsterActionFlags::ATTACK
                | MonsterActionFlags::YIELD
                | MonsterActionFlags::IMPERIL
                | MonsterActionFlags::VULNERABILITY
                | MonsterActionFlags::BROADSIDE
                | MonsterActionFlags::FESTER,
            MonsterDamageType::Auto,
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    // Measured in the C#: 2 and 145 with the memos, 169 and 211 without.
    assert!(
        (1..=20).contains(&fight.surface.capture_owned_equipment_count),
        "{}",
        fight.surface.capture_owned_equipment_count
    );
    assert!(
        (1..=180).contains(&fight.surface.known_combat_spell_reads),
        "{}",
        fight.surface.known_combat_spell_reads
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: APassThatChoosesAgainStillCapturesTheCharacterOnlyOnce
#[test]
fn a_pass_that_chooses_again_still_captures_the_character_only_once() {
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: twenty_or_eight(8),
        known_attack_spells: vec![SpellInfo {
            is_projectile: true,
            ..spell(100, "Incantation of Flame Bolt")
        }],
        equipment_items: vec![wielded_caster(990)],
        // Nothing can be reached, so every attempt turns a column off and
        // the pass chooses again until it runs out of monsters.
        projectile_path: ProjectilePathResult {
            status: ProjectilePathStatus::Blocked,
            ..ProjectilePathResult::default()
        },
        ..FakeAutomation::default()
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

    assert!(
        fight.surface.projectile_path_checks >= 2,
        "the pass never had to choose again, so nothing is being measured"
    );
    assert!((1..=2).contains(&fight.surface.capture_owned_equipment_count));
    assert!((1..=2).contains(&fight.surface.capture_owned_items_count));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: KillAndSuccessResultTextResetTheAttemptCountOnly
#[test]
fn kill_and_success_result_text_reset_the_attempt_count_only() {
    let settings = CombatSettings {
        blacklist_monster_timeout_seconds: 300.0,
        ..CombatSettings::default()
    };
    let mut tracker = CombatFailureTracker::new();

    tracker.observe_targets(&drudge(), 0.0, &settings);
    tracker.force_blacklist(10, 0.0, &settings);
    assert_eq!(
        tracker.reason(10, 1.0),
        CombatSuppressionReason::Blacklisted
    );

    tracker.reset_attempts(10);
    assert_eq!(
        tracker.reason(10, 1.0),
        CombatSuppressionReason::Blacklisted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: RingOnlyRuleCastsUntargetedRingWithOneNearbyRingTarget
#[test]
fn ring_only_rule_casts_untargeted_ring_with_one_nearby_ring_target() {
    let ring = SpellInfo {
        is_offensive: true,
        target_mask: 0,
        description: "Shoots waves of fire outward from the caster.".to_owned(),
        ..spell(110, "Cassius' Ring of Fire")
    };
    let mut fight = Fight::new(
        magic(vec![target(10, "Drudge", 3.0, 0.0)], vec![ring]),
        only_rule(
            CombatSettings::default(),
            single_rule(MonsterActionFlags::RING, MonsterDamageType::Fire),
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_untargeted_cast, 110);
    assert_eq!(fight.surface.last_targeted_cast, (0, 0));
}

fn equips(log: &[String]) -> Vec<String> {
    log.iter()
        .filter(|entry| entry.starts_with("Equip:"))
        .cloned()
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TwoWandsOfOneNameDoNotTakeTurnsBeingWielded
#[test]
fn two_wands_of_one_name_do_not_take_turns_being_wielded() {
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![
            equipment(0x8000_0A4C, "Wand", 0).item_type(0x8000).build(),
            equipment(0x8000_0B34, "Wand", 0).item_type(0x8000).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = row(MonsterActionFlags::ATTACK, MonsterDamageType::Auto);
    settings.maximum_range = CombatSettings::default().maximum_range;
    settings.combat_item_names.insert("Wand".to_owned());
    settings.combat_item_order.push("Wand".to_owned());
    settings.combat_item_order_ids.push(0x8000_0A4C);
    settings.combat_item_order_ids.push(0x8000_0B34);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    let first = equips(&fight.surface.call_log);
    assert_eq!(first.len(), 1);

    fight.tick(0.25);
    fight.tick(0.25);

    let mut every = equips(&fight.surface.call_log);
    every.dedup();
    let mut distinct = Vec::new();
    for entry in every {
        if !distinct.contains(&entry) {
            distinct.push(entry);
        }
    }
    assert_eq!(first, distinct);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ExplicitMonsterWeaponUsesCanonicalEquipmentCommandBeforeAttack
#[test]
fn explicit_monster_weapon_uses_canonical_equipment_command_before_attack() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![equipment(700, "Fire Sword", 0x10).build()],
        ..FakeAutomation::default()
    };
    let mut settings = weapon_row(MonsterActionFlags::ATTACK.with_weapon(700));
    settings.combat_item_object_ids.insert(700);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 700);
    assert_eq!(fight.surface.begin_count, 0);
}

/// Builds rule actions from flags and a weapon id.
trait WithWeapon {
    fn with_weapon(self, weapon_object_id: u32) -> MonsterRuleActions;
}

impl WithWeapon for MonsterActionFlags {
    fn with_weapon(self, weapon_object_id: u32) -> MonsterRuleActions {
        MonsterRuleActions {
            flags: self,
            weapon_object_id,
            ..MonsterRuleActions::default()
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ExplicitMonsterWeaponResolvesDurableNameAfterRelogChangesObjectId
#[test]
fn explicit_monster_weapon_resolves_durable_name_after_relog_changes_object_id() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![equipment(900, "Fire Sword", 0x10).build()],
        ..FakeAutomation::default()
    };
    let mut settings = weapon_row(MonsterRuleActions {
        weapon_name: "Fire Sword".to_owned(),
        ..MonsterActionFlags::ATTACK.with_weapon(700)
    });
    // An older profile lists its items by name alone, which is exactly
    // what still matches the same sword under a new object id.
    settings.combat_item_names.insert("Fire Sword".to_owned());
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 900);
    assert_eq!(fight.surface.begin_count, 0);
}

fn listed_and_unlisted() -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![
            equipment(700, "Unlisted Blade", 0x10).build(),
            equipment(701, "Listed Sword", 0x10).build(),
        ],
        ..FakeAutomation::default()
    }
}

fn fire_with_weapon(weapon_object_id: u32) -> CombatSettings {
    weapon_row(MonsterRuleActions {
        damage_type: MonsterDamageType::Fire,
        ..MonsterActionFlags::ATTACK.with_weapon(weapon_object_id)
    })
}

fn not_listed_notices(surface: &FakeAutomation) -> usize {
    surface
        .posted_system_messages
        .iter()
        .filter(|line| line.contains("not in the Items list"))
        .count()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnUnlistedRuleWeaponIsNeverWieldedAndTheAutomaticChoiceAnswers
#[test]
fn an_unlisted_rule_weapon_is_never_wielded_and_the_automatic_choice_answers() {
    let mut settings = fire_with_weapon(700);
    settings.combat_item_object_ids.insert(701);
    let mut fight = Fight::new(listed_and_unlisted(), settings);

    fight.toggle();
    for _ in 0..8 {
        fight.tick(0.25);
    }

    assert!(
        !fight
            .surface
            .call_log
            .contains(&"Equip:000002BC".to_owned())
    );
    assert_eq!(fight.surface.last_equip_object_id, 701);
    assert_eq!(not_listed_notices(&fight.surface), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AListedRuleWeaponIsStillWieldedByName
#[test]
fn a_listed_rule_weapon_is_still_wielded_by_name() {
    let mut settings = fire_with_weapon(700);
    settings.combat_item_object_ids.insert(700);
    settings.combat_item_object_ids.insert(701);
    let mut fight = Fight::new(listed_and_unlisted(), settings);

    fight.toggle();
    for _ in 0..8 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 700);
    assert_eq!(not_listed_notices(&fight.surface), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AutomaticDamageSelectionChoosesStrongestMatchingWeapon
#[test]
fn automatic_damage_selection_chooses_strongest_matching_weapon() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![
            equipment(700, "Weak fire", 0x10).damage(20).build(),
            equipment(701, "Strong fire", 0x10).damage(35).build(),
            equipment(702, "Acid", 0x20).damage(99).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        CombatSettings::default(),
        single_rule(MonsterActionFlags::ATTACK, MonsterDamageType::Fire),
    );
    settings.combat_item_names.insert("Strong fire".to_owned());
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 701);
    assert_eq!(fight.surface.begin_count, 0);
}

fn attack_only() -> CombatSettings {
    weapon_row(MonsterRuleActions {
        flags: MonsterActionFlags::ATTACK,
        ..MonsterRuleActions::default()
    })
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AttackRoutesTheCombatModeThroughTheSharedGateNotEnterDefaultMode
#[test]
fn attack_routes_the_combat_mode_through_the_shared_gate_not_enter_default_mode() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![
            equipment(700, "War Wand", 0)
                .equipped(0x0010_0000)
                .item_type(0x8000)
                .build(),
        ],
        ..FakeAutomation::default()
    };
    let mut fight = Fight::new(surface, attack_only());

    fight.toggle();
    fight.tick(0.25);
    fight.tick(0.25);

    // The wand is already wielded; only the mode is wrong. The gate
    // recomputes the mode the wielded item implies and asks for it.
    assert!(index_of(&fight.surface.call_log, "EnterMode:Magic").is_some());
    assert_eq!(fight.surface.combat_snapshot.mode, CombatMode::Magic);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AttackWithNothingWieldedAndAnEmptyItemsProfileStopsWithTheWandNotice
#[test]
fn attack_with_nothing_wielded_and_an_empty_items_profile_stops_with_the_wand_notice() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        ..FakeAutomation::default()
    };
    let mut fight = Fight::new(surface, attack_only());

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    let notices: Vec<&String> = fight
        .surface
        .posted_system_messages
        .iter()
        .filter(|message| message.contains(NO_WAND_NOTICE))
        .collect();
    assert_eq!(notices, [&format!("[MossTank] {NO_WAND_NOTICE}")]);
    assert!(!fight.controller.enabled());
    assert!(index_of(&fight.surface.call_log, "EnterDefaultMode").is_none());
    assert!(index_of(&fight.surface.call_log, "EnterMode:Melee").is_none());
    assert_eq!(fight.surface.begin_count, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AttackWithNothingWieldedWieldsTheProfiledWandAndRequestsMagic
#[test]
fn attack_with_nothing_wielded_wields_the_profiled_wand_and_requests_magic() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![equipment(700, "War Wand", 0).item_type(0x8000).build()],
        ..FakeAutomation::default()
    };
    let mut settings = attack_only();
    settings.combat_item_names.insert("War Wand".to_owned());
    settings.combat_item_order_ids.push(700);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    let log = &fight.surface.call_log;
    let equip = index_of(log, "Equip:000002BC").expect("the profiled wand was never wielded");
    let magic = index_of(log, "EnterMode:Magic");
    assert!(
        magic.is_some_and(|magic| magic > equip),
        "Magic was requested before the wand: {}",
        log.join(" | ")
    );
    assert!(fight.controller.enabled());
    assert!(
        !fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains(NO_WAND_NOTICE))
    );
    assert!(index_of(log, "EnterDefaultMode").is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AttackWithAWieldedSwordButNoProfiledWeaponSaysWhatIsMissing
#[test]
fn attack_with_a_wielded_sword_but_no_profiled_weapon_says_what_is_missing() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        // Wielded, but in no profile.
        equipment_items: vec![
            equipment(900, "Unprofiled Sword", 0x0001)
                .equipped(0x0010_0000)
                .build(),
        ],
        ..FakeAutomation::default()
    };
    let mut fight = Fight::new(surface, attack_only());

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    // The character is told what is missing, once, and the attack steps
    // aside; the weapon in its hand is left alone.
    assert_eq!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .filter(|message| message.contains("Add the weapon you fight with to the Items list."))
            .count(),
        1
    );
    assert!(
        !fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains(NO_WAND_NOTICE))
    );
    assert!(fight.controller.enabled());
    assert!(index_of(&fight.surface.call_log, "EnterDefaultMode").is_none());
    assert!(index_of(&fight.surface.call_log, "EnterMode:Magic").is_none());
    assert_eq!(fight.surface.begin_count, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AttackWithAProfiledMeleeWeaponTakesThePhysicalArmThroughTheGate
#[test]
fn attack_with_a_profiled_melee_weapon_takes_the_physical_arm_through_the_gate() {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![equipment(900, "Fire Sword", 0x10).build()],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        CombatSettings::default(),
        single_rule(MonsterActionFlags::ATTACK, MonsterDamageType::Fire),
    );
    settings.combat_item_names.insert("Fire Sword".to_owned());
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..6 {
        fight.tick(0.25);
    }

    let log = &fight.surface.call_log;
    let equip = index_of(log, "Equip:00000384").expect("the planned weapon was never wielded");
    let melee = index_of(log, "EnterMode:Melee");
    assert!(
        melee.is_some_and(|melee| melee > equip),
        "Melee was requested before the weapon: {}",
        log.join(" | ")
    );
    assert!(fight.controller.enabled());
    assert!(
        !fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains(NO_WAND_NOTICE))
    );
    assert!(index_of(log, "EnterDefaultMode").is_none());
}

fn fire_bow() -> EquipmentItem {
    equipment(700, "Fire Bow", 0x10)
        .item_type(0x100)
        .equipped(0x0010_0000)
        .ammo(1, 1)
        .build()
}

fn arrows(id: u32, stack: i32) -> common::Gear {
    equipment(id, "Deadly Fire Arrow", 0x10)
        .combat_use(3)
        .ammo(1, stack)
        .valid(AMMUNITION_SLOT)
}

fn arrow_item(id: u32, name: &str, stack: i32) -> InventoryItem {
    let mut item = inventory_item(id, name, 0x100, 0, false);
    item.stack_size = stack;
    item
}

fn bow_row() -> CombatSettings {
    let mut settings = weapon_row(MonsterRuleActions {
        damage_type: MonsterDamageType::Fire,
        ..MonsterActionFlags::ATTACK.with_weapon(700)
    });
    settings.combat_item_object_ids.insert(700);
    settings
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ManualEquipmentCommandReadsAmmunitionAddedBetweenCalls
#[test]
fn manual_equipment_command_reads_ammunition_added_between_calls() {
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        character_skills: vec![missile_skill()],
        equipment_items: vec![fire_bow()],
        ..FakeAutomation::default()
    };
    let mut fight = Fight::with_game_info(surface, bow_row(), ammo_game_info());

    assert!(!fight.equip_one_step("Fixture"));
    assert_eq!(fight.surface.last_equip_object_id, 0);

    fight.surface.equipment_items = vec![fire_bow(), arrows(801, 20).build()];
    fight.surface.item_entries = vec![arrow_item(801, "Deadly Fire Arrow", 20)];

    assert!(!fight.equip_one_step("Fixture"));
    assert_eq!(fight.surface.last_equip_object_id, 801);
}

/// `BoundAmmunitionGate`: the fight over a database, answering the gate's
/// two ammunition questions.
fn ammunition_fight(surface: FakeAutomation, game_info: Option<VtankGameInfoDatabase>) -> Fight {
    Fight::with_game_info(
        surface,
        CombatSettings::default(),
        game_info.unwrap_or_else(ammo_game_info),
    )
}

fn ammunition_stale(fight: &mut Fight) -> bool {
    fight.with(|controller, cx| controller.ammunition_stale(cx, 700, MonsterDamageType::Fire))
}

fn wield_ammunition(fight: &mut Fight) -> bool {
    fight.with(|controller, cx| controller.wield_ammunition(cx, MonsterDamageType::Fire))
}

fn bow_in_peace(
    equipment_items: Vec<EquipmentItem>,
    item_entries: Vec<InventoryItem>,
) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        character_skills: vec![missile_skill()],
        equipment_items,
        item_entries,
        ..FakeAutomation::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionUsesExactQuiverSlotAndLargestMatchingStack
#[test]
fn ammunition_uses_exact_quiver_slot_and_largest_matching_stack() {
    let mut fight = ammunition_fight(
        bow_in_peace(
            vec![
                fire_bow(),
                equipment(801, "Deadly Fire Arrow", 0x10)
                    .combat_use(3)
                    .equipped(0x0010_0000)
                    .ammo(0, 5)
                    .valid(AMMUNITION_SLOT)
                    .build(),
                equipment(802, "Deadly Fire Arrow", 0x10)
                    .combat_use(3)
                    .ammo(0, 10)
                    .valid(AMMUNITION_SLOT)
                    .build(),
            ],
            vec![
                inventory_item(801, "Deadly Fire Arrow", 0x100, 0, false),
                inventory_item(802, "Deadly Fire Arrow", 0x100, 0, false),
            ],
        ),
        None,
    );

    assert!(ammunition_stale(&mut fight));
    assert!(wield_ammunition(&mut fight));
    assert_eq!(fight.surface.last_equip_object_id, 802);
}

/// `IsAmmunitionSelectionPending`.
fn is_ammunition_selection_pending(name: &str, count: i32) -> bool {
    let mut fight = ammunition_fight(
        bow_in_peace(vec![fire_bow()], vec![arrow_item(801, name, count)]),
        None,
    );
    ammunition_stale(&mut fight)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionAvailabilityRequiresExactCase
#[test]
fn ammunition_availability_requires_exact_case() {
    assert!(!is_ammunition_selection_pending("deadly fire arrow", 1));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionAvailabilityRequiresPositiveCount
#[test]
fn ammunition_availability_requires_positive_count() {
    assert!(!is_ammunition_selection_pending("Deadly Fire Arrow", 0));
}

fn bow_and_quiver() -> FakeAutomation {
    bow_in_peace(
        vec![fire_bow(), arrows(801, 20).ammo(0, 20).build()],
        vec![inventory_item(801, "Deadly Fire Arrow", 0x100, 0, false)],
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionApplicationUsesTheEvaluatedPrimary
#[test]
fn ammunition_application_uses_the_evaluated_primary() {
    let mut fight = ammunition_fight(bow_and_quiver(), None);

    assert!(ammunition_stale(&mut fight));
    assert!(wield_ammunition(&mut fight));
    assert_eq!(fight.surface.last_equip_object_id, 801);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmoGateDoesNotSubstituteBundledOptionsForMissingRows
#[test]
fn ammo_gate_does_not_substitute_bundled_options_for_missing_rows() {
    for database in [
        VtankGameInfoDatabase::empty(),
        VtankGameInfoDatabase::load_default(),
    ] {
        let mut fight = ammunition_fight(bow_and_quiver(), Some(database));
        assert!(!ammunition_stale(&mut fight));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnUnavailableAmmunitionSelectionIsSaidOnce
#[test]
fn an_unavailable_ammunition_selection_is_said_once() {
    let mut fight = ammunition_fight(
        bow_in_peace(vec![fire_bow()], Vec::new()),
        Some(VtankGameInfoDatabase::load_default()),
    );

    assert!(!ammunition_stale(&mut fight));
    assert!(!ammunition_stale(&mut fight));

    assert_eq!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .filter(|message| message.to_lowercase().contains("ammunition"))
            .count(),
        1,
        "{:?}",
        fight.surface.posted_system_messages
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: MissingAmmunitionIsNotAnActionInProgress
#[test]
fn missing_ammunition_is_not_an_action_in_progress() {
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        equipment_items: vec![fire_bow()],
        ..FakeAutomation::default()
    };
    let mut fight = ammunition_fight(surface, None);

    assert!(!ammunition_stale(&mut fight));
    assert!(!wield_ammunition(&mut fight));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionSelectionReadsADatabaseReplacedMidSession
#[test]
fn ammunition_selection_reads_a_database_replaced_mid_session() {
    let mut fight = ammunition_fight(
        bow_and_quiver(),
        Some(VtankGameInfoDatabase::load_default()),
    );
    assert!(!ammunition_stale(&mut fight));

    fight.controller.replace_game_info(ammo_game_info());

    assert!(ammunition_stale(&mut fight));
    assert!(wield_ammunition(&mut fight));
    assert_eq!(fight.surface.last_equip_object_id, 801);
}

fn bow_fight(combat_snapshot: ac_plugin_abi::combat::CombatSnapshot) -> Fight {
    let surface = FakeAutomation {
        combat_snapshot,
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        character_skills: vec![missile_skill()],
        equipment_items: vec![fire_bow(), arrows(801, 20).build()],
        item_entries: vec![inventory_item(801, "Deadly Fire Arrow", 0x100, 0, false)],
        ..FakeAutomation::default()
    };
    Fight::with_game_info(surface, bow_row(), ammo_game_info())
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: MissileLauncherSelectsOfficialBestAvailableAmmunition
#[test]
fn missile_launcher_selects_official_best_available_ammunition() {
    let mut fight = bow_fight(physical());

    fight.toggle();
    for _ in 0..4 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 801);
    assert_eq!(fight.surface.begin_count, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionWieldGoesThroughAuthenticDropToPeacePrologue
#[test]
fn ammunition_wield_goes_through_authentic_drop_to_peace_prologue() {
    let mut fight = bow_fight(in_mode(CombatMode::Missile));

    fight.toggle();
    for _ in 0..6 {
        fight.tick(0.25);
    }

    // Peace is asked for BEFORE the arrow is wielded, never after.
    let log = &fight.surface.call_log;
    let peace = index_of(log, "EnterMode:Peace")
        .unwrap_or_else(|| panic!("Peace was never requested: {}", log.join(" | ")));
    let equip = index_of(log, "Equip:00000321");
    assert!(
        equip.is_some_and(|equip| equip > peace),
        "The arrow was wielded before Peace: {}",
        log.join(" | ")
    );
    assert_eq!(fight.surface.last_equip_object_id, 801);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AmmunitionStillWieldsThroughThePanelsExternallyBoundGate
#[test]
fn ammunition_still_wields_through_the_panels_externally_bound_gate() {
    let mut fight = bow_fight(physical());

    // Exactly what the panel builds: the one shared gate, injected.
    let gate = Rc::new(RefCell::new(mosstank_combat::gate::CombatModeGate::new()));
    fight.controller.bind_combat_mode_gate(gate);

    fight.toggle();
    for _ in 0..6 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 801);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: StuckCombatModeUsesProfiledCasterAfterAuthenticRetryCount
#[test]
fn stuck_combat_mode_uses_profiled_caster_after_authentic_retry_count() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        ignore_mode_changes: true,
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![
            equipment(700, "Fire Sword", 0x10).build(),
            equipment(800, "Recovery Wand", 0).item_type(0x8000).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = weapon_row(MonsterActionFlags::ATTACK.with_weapon(700));
    settings
        .combat_item_names
        .insert("Recovery Wand".to_owned());
    settings.combat_item_order_ids.push(800);
    let mut fight = Fight::new(surface, settings);
    fight.vitals.drop_to_peace_mode_retry_count = 2;

    fight.toggle();
    fight.tick(0.25);
    fight.tick(0.25);

    assert_eq!(fight.surface.mode_change_requests, 1);
    assert_eq!(fight.surface.last_used_item, 800);
    assert_eq!(fight.surface.last_equip_object_id, 0);
    assert!(
        fight
            .status()
            .to_lowercase()
            .contains("bugged combat state"),
        "{}",
        fight.status()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SessionLossDisablesAndAborts
#[test]
fn session_loss_disables_and_aborts() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        ..FakeAutomation::default()
    };
    let mut fight = Fight::new(surface, CombatSettings::default());
    fight.toggle();
    fight.surface.is_available = false;

    fight.tick(0.1);

    assert!(!fight.controller.enabled());
    assert_eq!(fight.surface.abort_count, 1);
    assert_eq!(fight.status(), "Session ended");
}

fn lens_fight() -> Fight {
    let imperil = SpellInfo {
        school: 31,
        is_debuff: true,
        is_offensive: true,
        duration_seconds: 60.0,
        ..spell(90, "Imperil Other VII")
    };
    let mut lens = inventory_item(800, "Imperil Lens", 0x8000, 90, true);
    lens.item_spellcraft = 400;
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: drudge(),
        spell_lookup: vec![imperil],
        item_entries: vec![lens],
        ..FakeAutomation::default()
    };
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.combat_item_object_ids.insert(800);
    Fight::new(surface, settings)
}

fn lens_answers(fight: &mut Fight) {
    fight.surface.last_item_completion = ItemUseCompletion {
        revision: 1,
        source_id: 800,
        target_id: 10,
        weenie_error: 0,
    };
    fight.surface.chat_messages = vec![cast_line("You cast Imperil Other VII on Drudge.")];
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: CasterItemDebuffWaitsForUseDoneAndConfirmedCastChat
#[test]
fn caster_item_debuff_waits_for_use_done_and_confirmed_cast_chat() {
    let mut fight = lens_fight();

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_applied_item, (800, 10));
    assert_eq!(fight.surface.begin_count, 0);
    assert!(fight.controller.has_pending_item_debuff());

    lens_answers(&mut fight);
    fight.tick(0.25);

    assert_eq!(fight.surface.apply_count, 1);
    assert!(fight.status().contains("Waiting for a target"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AWandDebuffHoldsTheItemSlotForItsWholeCast
#[test]
fn a_wand_debuff_holds_the_item_slot_for_its_whole_cast() {
    let mut fight = lens_fight();
    let locks = Rc::new(RefCell::new(ActionLockTable::new()));
    fight
        .controller
        .bind_action_locks(locks.clone(), Box::new(|| false));

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_applied_item, (800, 10));
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    // Still held most of the way through the cast window.
    locks.borrow_mut().advance(11.0);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    lens_answers(&mut fight);
    fight.tick(0.25);

    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ProcWeaponChargesAtZeroAndRequiresCastChatNotAttackDone
#[test]
fn proc_weapon_charges_at_zero_and_requires_cast_chat_not_attack_done() {
    let imperil = SpellInfo {
        school: 31,
        is_debuff: true,
        is_offensive: true,
        duration_seconds: 60.0,
        target_mask: 0x10,
        ..spell(91, "Imperil Other VII")
    };
    let mut sword = inventory_item(801, "Imperil Sword", 1, 0, true);
    sword.item_spellcraft = 400;
    sword.appraised_spell_ids = vec![91];
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: drudge(),
        spell_lookup: vec![imperil],
        item_entries: vec![sword],
        ..FakeAutomation::default()
    };
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.combat_item_object_ids.insert(801);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(fight.surface.last_begin_power.abs() < f32::EPSILON);

    fight.surface.combat_snapshot = physical_with(true, true, 0.0);
    fight.tick(0.1);
    assert_eq!(fight.surface.release_count, 1);

    let mut done = physical();
    done.completion_revision = 1;
    done.completion_weenie_error = 0;
    fight.surface.combat_snapshot = done;
    fight.tick(0.1);
    assert!(!fight.status().contains("Waiting for a target"));

    fight.surface.chat_messages = vec![cast_line("You cast Imperil Other VII on Drudge.")];
    fight.tick(0.1);
    assert!(fight.status().contains("Waiting for a target"));
}
