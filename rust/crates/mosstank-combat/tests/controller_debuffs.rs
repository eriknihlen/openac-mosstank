//! The fight's debuff chain, its elements and the casts other clients on
//! this computer report (`CombatControllerTests`, fourth part).

mod common;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::{CombatMode, CombatTarget};
use ac_plugin_abi::items::{EquipmentItem, EquipmentObservation, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::projectile::{ProjectilePathResult, ProjectilePathStatus};
use ac_plugin_abi::types::ChatLine;
use common::{
    FakeAutomation, Fight, debuff, debuff_only, equipment, game_info, in_mode, inventory_item,
    magic_spell, only_rule, peaceful, physical, rule_with, single_rule, skill, spell, target,
    wielded_caster, wielded_planned_weapon,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::model::{CombatSettings, TargetSelectionMethod};
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

fn completion(revision: i64, spell_id: u32, target_id: u32, error: u32) -> CastCompletion {
    CastCompletion {
        revision,
        spell_id,
        target_id,
        weenie_error: error,
    }
}

fn row(flags: MonsterActionFlags, damage_type: MonsterDamageType) -> CombatSettings {
    only_rule(ranged(), single_rule(flags, damage_type))
}

/// `FireVulnerabilityRule`: attack with fire, and tick the vulnerability
/// column.
fn fire_vulnerability_rule() -> CombatSettings {
    row(
        MonsterActionFlags::ATTACK | MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Fire,
    )
}

/// `PeerCast`, with the C# helper's defaults.
fn peer_cast(sequence: i64, spell_id: u32) -> PeerCast {
    PeerCast {
        sequence,
        client_id: 7,
        caster_id: 0x5000_0002,
        target_id: 10,
        spell_id,
        effective_skill: 300,
        seconds_remaining: 45.0,
        landed: true,
        ..PeerCast::default()
    }
}

/// `NetworkClient`.
fn network_client(client_id: u32, name: &str, tags: &[&str]) -> NetworkClient {
    NetworkClient {
        client_id,
        player_id: 0x5000_0000 + client_id,
        name: name.to_owned(),
        world_name: "Coldeve".to_owned(),
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        current_health: 100,
        current_mana: 100,
        current_stamina: 100,
        max_health: 100,
        max_mana: 100,
        max_stamina: 100,
        heading: 0.0,
        ..NetworkClient::default()
    }
}

/// The fake's own character id (`FakeAutomation.ObjectId`).
const SELF_ID: u32 = 1;

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: DebuffsGoOutInAuthenticTwelveStepOrder
#[test]
fn debuffs_go_out_in_authentic_twelve_step_order() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![
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
            MonsterDamageType::Fire,
        ),
    );
    fight.toggle();

    let mut revision = 0;
    for expected in [84, 83, 82, 81, 80] {
        fight.tick(0.25);
        assert_eq!(fight.surface.last_targeted_cast, (expected, 10));
        revision += 1;
        fight.surface.last_cast_completion = completion(revision, expected, 10, 0);
        fight.tick(0.25);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheDebuffLeadDecidesWhenACurseIsRenewed
#[test]
fn the_debuff_lead_decides_when_a_curse_is_renewed() {
    for (precast_seconds, renews) in [(5.0, false), (20.0, true)] {
        let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
        settings.maximum_range = 40.0;
        settings.debuff_precast_seconds = precast_seconds;
        let mut fight = Fight::new(
            magic(drudge(), vec![debuff(83, "Imperil Other VII")]),
            settings,
        );
        fight.toggle();

        // The curse goes out and the server answers it: sixty seconds on it.
        fight.tick(0.25);
        assert_eq!(fight.surface.cast_spell_ids, [83]);
        fight.surface.last_cast_completion = completion(1, 83, 10, 0);
        fight.tick(0.25);

        // Forty-five seconds on, with fifteen seconds of curse left.
        for _ in 0..180 {
            fight.tick(0.25);
        }

        assert_eq!(
            fight.surface.cast_spell_ids.len() > 1,
            renews,
            "{precast_seconds}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: OnlyOneDebuffKindIsDispatchedPerPass
#[test]
fn only_one_debuff_kind_is_dispatched_per_pass() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![
                debuff(83, "Imperil Other VII"),
                debuff(84, "Magic Yield Other VII"),
            ],
        ),
        row(
            MonsterActionFlags::ATTACK | MonsterActionFlags::YIELD | MonsterActionFlags::IMPERIL,
            MonsterDamageType::Fire,
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.cast_spell_ids, [84]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: BlockedDebuffPathDropsThatColumnAndTheChainMovesOn
#[test]
fn blocked_debuff_path_drops_that_column_and_the_chain_moves_on() {
    let mut surface = magic(
        drudge(),
        vec![
            // The first step of the chain, and the only spell here whose
            // family declares a flight (117 is a bolt family).
            SpellInfo {
                family: 117,
                ..debuff(84, "Magic Yield Other VII")
            },
            // Step 7.
            debuff(83, "Imperil Other VII"),
        ],
    );
    surface.projectile_path = ProjectilePathResult {
        status: ProjectilePathStatus::Blocked,
        collision_checks: 3,
        blocking_object_id: 0x5000_0001,
        ..ProjectilePathResult::default()
    };
    let settings = CombatSettings {
        maximum_range: 40.0,
        use_projectile_awareness: true,
        ..CombatSettings::default()
    };
    assert!(!settings.allow_debuff_fallback);
    let mut fight = Fight::new(
        surface,
        only_rule(
            settings,
            single_rule(
                MonsterActionFlags::ATTACK
                    | MonsterActionFlags::YIELD
                    | MonsterActionFlags::IMPERIL,
                MonsterDamageType::Fire,
            ),
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (83, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&84));
}

fn wand_at(id: u32, name: &'static str, damage_type: i32) -> EquipmentItem {
    equipment(id, name, damage_type).item_type(0x8000).build()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ALearnedDebuffWieldsAWandBeforeItIsCast
#[test]
fn a_learned_debuff_wields_a_wand_before_it_is_cast() {
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Melee),
        targets: drudge(),
        known_combat_spells: vec![debuff(83, "Imperil Other VII")],
        equipment_items: vec![wielded_planned_weapon(991), wand_at(990, "Fixture Wand", 0)],
        ..FakeAutomation::default()
    };
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.maximum_range = 40.0;
    settings.combat_item_names.insert("Fixture Wand".to_owned());
    settings.combat_item_order_ids.push(990);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    // Peace first, then the wand: nothing is cast while the sword is in
    // hand.
    for _ in 0..4 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_equip_object_id, 990);
    assert!(fight.surface.cast_spell_ids.is_empty());

    // The server confirms the swap: the sword is away, the wand is in.
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
    for _ in 0..4 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_targeted_cast, (83, 10));
}

/// `DebuffWandFor`.
fn debuff_wand_for(switch_wands_to_debuff: bool) -> u32 {
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: drudge(),
        known_combat_spells: vec![debuff(83, "Imperil Other VII")],
        equipment_items: vec![
            wand_at(991, "Attack Wand", 0x0010),
            wand_at(990, "Spare Wand", 0),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        CombatSettings {
            maximum_range: 40.0,
            switch_wands_to_debuff,
            ..CombatSettings::default()
        },
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::IMPERIL,
                weapon_name: "Attack Wand".to_owned(),
                ..MonsterRuleActions::default()
            },
        ),
    );
    settings.combat_item_names.insert("Attack Wand".to_owned());
    settings.combat_item_names.insert("Spare Wand".to_owned());
    settings.combat_item_order.push("Spare Wand".to_owned());
    settings.combat_item_order_ids.push(990);
    settings.combat_item_order_ids.push(991);
    let mut fight = Fight::new(surface, settings);
    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_equip_object_id
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SwitchWandsToDebuffDebuffsWithTheCasterAttackWeapon
#[test]
fn switch_wands_to_debuff_debuffs_with_the_caster_attack_weapon() {
    assert_eq!(debuff_wand_for(true), 991);
    assert_eq!(debuff_wand_for(false), 990);
}

/// `DebuffAtDistance`.
fn debuff_at_distance(reach: f32, distance: f32) -> (u32, u32) {
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.maximum_range = 40.0;
    settings.spell_range_fudge = 0.0;
    let mut fight = Fight::new(
        magic(
            vec![target(10, "Drudge", distance, 0.0)],
            vec![SpellInfo {
                base_range_constant: reach,
                ..debuff(83, "Imperil Other VII")
            }],
        ),
        settings,
    );
    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_targeted_cast
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ADebuffSpellOutOfReachIsNotChosen
#[test]
fn a_debuff_spell_out_of_reach_is_not_chosen() {
    assert_eq!(debuff_at_distance(4.0, 6.0), (0, 0));
    assert_eq!(debuff_at_distance(20.0, 6.0), (83, 10));
}

fn imperil_90() -> SpellInfo {
    SpellInfo {
        school: 31,
        is_debuff: true,
        is_offensive: true,
        duration_seconds: 60.0,
        ..spell(90, "Imperil Other VII")
    }
}

fn imperil_lens() -> ac_plugin_abi::items::InventoryItem {
    let mut lens = inventory_item(800, "Imperil Lens", 0x8000, 90, true);
    lens.item_spellcraft = 400;
    lens
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnOutOfReachSpellLeavesTheItemAsTheDebuffSource
#[test]
fn an_out_of_reach_spell_leaves_the_item_as_the_debuff_source() {
    let imperil = SpellInfo {
        base_range_constant: 4.0,
        ..imperil_90()
    };
    let lens = imperil_lens();
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: vec![target(10, "Drudge", 6.0, 0.0)],
        known_combat_spells: vec![imperil.clone()],
        spell_lookup: vec![imperil],
        item_entries: vec![lens.clone()],
        // The learned spell would out-rank the lens on skill; only its
        // reach keeps it out of the choice.
        character_skills: vec![skill(31, "Creature", Training::Trained, 500)],
        ..FakeAutomation::default()
    };
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.maximum_range = 40.0;
    settings.spell_range_fudge = 0.0;
    settings.combat_item_object_ids.insert(lens.object_id);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_applied_item, (800, 10));
    assert!(fight.surface.cast_spell_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: BlockedDebuffPathStillLetsTheAttackGoOutOnStockSettings
#[test]
fn blocked_debuff_path_still_lets_the_attack_go_out_on_stock_settings() {
    let mut surface = magic(
        drudge(),
        vec![
            SpellInfo {
                family: 117,
                ..debuff(84, "Magic Yield Other VII")
            },
            magic_spell(100, "Flame Bolt VII", 300),
        ],
    );
    surface.projectile_path = ProjectilePathResult {
        status: ProjectilePathStatus::Blocked,
        ..ProjectilePathResult::default()
    };
    let mut fight = Fight::new(
        surface,
        only_rule(
            CombatSettings {
                maximum_range: 40.0,
                use_projectile_awareness: true,
                ..CombatSettings::default()
            },
            single_rule(
                MonsterActionFlags::ATTACK | MonsterActionFlags::YIELD,
                MonsterDamageType::Fire,
            ),
        ),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnUnknownVulnScoresNoUrgencyForTheTargetItCannotDebuff
#[test]
fn an_unknown_vuln_scores_no_urgency_for_the_target_it_cannot_debuff() {
    let mut settings = CombatSettings {
        maximum_range: 40.0,
        selection_method: TargetSelectionMethod::RANGE,
        ..CombatSettings::default()
    };
    settings.rules.clear();
    settings
        .rules
        .push(single_rule_named("name#^Drudge", MonsterDamageType::Fire));
    settings
        .rules
        .push(single_rule_named("name#^Rat", MonsterDamageType::Acid));
    let mut fight = Fight::new(
        magic(
            vec![target(10, "Drudge", 9.0, 0.0), target(20, "Rat", 2.0, 0.0)],
            vec![
                magic_spell(100, "Flame Bolt VII", 300),
                magic_spell(101, "Acid Stream VII", 300),
                // The Fire Vuln line is deliberately absent.
                debuff(85, "Acid Vulnerability Other VII"),
            ],
        ),
        settings,
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (101, 20));
}

fn single_rule_named(
    expression: &str,
    damage_type: MonsterDamageType,
) -> mosstank_settings::monster::MonsterRule {
    rule_with(
        expression,
        MonsterRuleActions {
            flags: MonsterActionFlags::ATTACK,
            damage_type,
            ..MonsterRuleActions::default()
        },
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ThePlannedWeaponsOwnElementPreemptsTheDamageTable
#[test]
fn the_planned_weapons_own_element_preempts_the_damage_table() {
    let fire_wand = equipment(990, "Flame Wand", 0x0010)
        .item_type(0x8000)
        .equipped(0x0010_0000)
        .build();
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: vec![target(10, "Magma Golem", 5.0, 0.0)],
        known_combat_spells: vec![
            magic_spell(100, "Flame Bolt VII", 300),
            magic_spell(102, "Frost Bolt VII", 300),
        ],
        equipment_items: vec![fire_wand],
        ..FakeAutomation::default()
    };
    let settings = only_rule(
        ranged(),
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                damage_type: MonsterDamageType::Auto,
                weapon_name: "Flame Wand".to_owned(),
                ..MonsterRuleActions::default()
            },
        ),
    );
    let mut fight = Fight::with_game_info(surface, settings, game_info());

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnImbuedMeleeWeaponKeepsItsOwnElementForALifeOnlyCaster
#[test]
fn an_imbued_melee_weapon_keeps_its_own_element_for_a_life_only_caster() {
    let mut sword = equipment(991, "Imbued Sword", 0x0001)
        .equipped(0x0010_0000)
        .build();
    // Plainly a slashing sword; its imbue rends fire.
    sword.imbued_effect = 0x0200;
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: drudge(),
        known_combat_spells: vec![
            debuff(70, "Fire Vulnerability Other VII"),
            debuff(71, "Cold Vulnerability Other VII"),
        ],
        equipment_items: vec![wand_at(990, "Spare Wand", 0), sword],
        // Life magic only: no war, no void.
        character_skills: vec![skill(33, "Life Magic", Training::Trained, 300)],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        ranged(),
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::VULNERABILITY,
                damage_type: MonsterDamageType::Auto,
                extra_vulnerability: MonsterDamageType::None,
                weapon_name: "Imbued Sword".to_owned(),
                ..MonsterRuleActions::default()
            },
        ),
    );
    settings.combat_item_names.insert("Spare Wand".to_owned());
    settings.combat_item_names.insert("Imbued Sword".to_owned());
    settings.combat_item_order_ids.push(990);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    for _ in 0..5 {
        fight.tick(0.25);
    }

    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ABowWithAnEmptyPackCanDeliverNoElementAtAll
#[test]
fn a_bow_with_an_empty_pack_can_deliver_no_element_at_all() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Magma Golem", 5.0, 0.0)],
        equipment_items: vec![
            equipment(900, "Yumi", 0)
                .item_type(0x100)
                .equipped(0x0010_0000)
                .ammo(0x001, 1)
                .build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = row(MonsterActionFlags::ATTACK, MonsterDamageType::Auto);
    settings.combat_item_names.insert("Yumi".to_owned());
    let mut fight = Fight::with_game_info(surface, settings, game_info());

    fight.toggle();
    fight.tick(0.25);

    assert!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("No ammunition available at all!")),
        "{:?}",
        fight.surface.posted_system_messages
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheLoadedGameInfoDatabaseDrivesTheAutoAttackElement
#[test]
fn the_loaded_game_info_database_drives_the_auto_attack_element() {
    let mut fight = Fight::with_game_info(
        magic(
            vec![target(10, "Magma Golem", 5.0, 0.0)],
            vec![
                magic_spell(103, "Force Bolt VII", 300),
                magic_spell(105, "Shock Wave VII", 300),
                // The Magma Golem's first listed weakness is cold.
                magic_spell(104, "Frost Bolt VII", 300),
            ],
        ),
        row(MonsterActionFlags::ATTACK, MonsterDamageType::Auto),
        game_info(),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (104, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ADefaultRowWithNoDebuffColumnsCastsNoVulnerability
#[test]
fn a_default_row_with_no_debuff_columns_casts_no_vulnerability() {
    let mut fight = Fight::with_game_info(
        magic(
            vec![target(10, "Magma Golem", 5.0, 0.0)],
            vec![
                // The Magma Golem's first listed weakness is cold, so the
                // character has both an attack and a debuff for it.
                magic_spell(104, "Frost Bolt VII", 300),
                debuff(85, "Cold Vulnerability Other VII"),
            ],
        ),
        row(MonsterActionFlags::ATTACK, MonsterDamageType::Auto),
        game_info(),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (104, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&85));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: VulnAndAnExtraVulnerabilityCastTheAttackElementFirst
#[test]
fn vuln_and_an_extra_vulnerability_cast_the_attack_element_first() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![
                magic_spell(100, "Flame Bolt VII", 300),
                debuff(70, "Fire Vulnerability Other VII"),
                debuff(71, "Acid Vulnerability Other VII"),
            ],
        ),
        only_rule(
            ranged(),
            rule_with(
                "DEFAULT",
                MonsterRuleActions {
                    flags: MonsterActionFlags::ATTACK | MonsterActionFlags::VULNERABILITY,
                    damage_type: MonsterDamageType::Fire,
                    extra_vulnerability: MonsterDamageType::Acid,
                    ..MonsterRuleActions::default()
                },
            ),
        ),
    );

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast.0, 70);

    // The first debuff is still in flight, so let it finish.
    fight.surface.last_cast_completion = completion(1, 70, 10, 0);
    fight.tick(0.25);
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast.0, 71);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ALureIsNeverChosenAsACreatureVulnerability
#[test]
fn a_lure_is_never_chosen_as_a_creature_vulnerability() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![
                magic_spell(100, "Flame Bolt VII", 300),
                debuff(80, "Flame Lure III"),
                debuff(81, "Fire Vulnerability Other VII"),
            ],
        ),
        fire_vulnerability_rule(),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (81, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&80));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: KnowingOnlyLuresCastsNothingAtTheCreature
#[test]
fn knowing_only_lures_casts_nothing_at_the_creature() {
    let mut fight = Fight::new(
        magic(
            drudge(),
            vec![
                magic_spell(100, "Flame Bolt VII", 300),
                debuff(80, "Flame Lure III"),
            ],
        ),
        fire_vulnerability_rule(),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&80));
}

// ---- Casts other clients on this computer reported ----

fn bind_sharing(fight: &mut Fight, enabled: bool, tag: Option<&'static str>) {
    fight.controller.bind_cast_sharing(
        Box::new(move || enabled),
        Box::new(move || tag.map(str::to_owned)),
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: RejectedOnlyPeerBatchAdvancesCursorWithoutReadingTrustSnapshot
#[test]
fn rejected_only_peer_batch_advances_cursor_without_reading_trust_snapshot() {
    let mut surface = FakeAutomation::default();
    surface.peers.casts.push(PeerCast {
        caster_id: SELF_ID,
        ..peer_cast(1, 70)
    });
    surface.peers.casts.push(PeerCast {
        landed: false,
        ..peer_cast(2, 70)
    });
    surface.peers.casts.push(PeerCast {
        seconds_remaining: 0.0,
        ..peer_cast(3, 70)
    });
    surface.peers.casts.push(PeerCast {
        seconds_remaining: -1.0,
        ..peer_cast(4, 70)
    });
    surface.peers.casts.push(PeerCast {
        seconds_remaining: f64::NAN,
        ..peer_cast(5, 70)
    });
    let mut fight = Fight::new(surface, fire_vulnerability_rule());
    let tag_reads = std::rc::Rc::new(std::cell::Cell::new(0));
    let reads = tag_reads.clone();
    fight.controller.bind_cast_sharing(
        Box::new(|| true),
        Box::new(move || {
            reads.set(reads.get() + 1);
            Some("debuffs".to_owned())
        }),
    );

    fight.observe_remote_casts();
    fight.observe_remote_casts();

    assert_eq!(fight.surface.peers.capture_requests, [0, 5]);
    assert_eq!(fight.surface.peers.client_capture_count, 0);
    assert_eq!(tag_reads.get(), 0);
    assert!(fight.surface.reported.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: MixedPeerBatchTakesOneTrustSnapshotAndNextBatchRechecksTags
#[test]
fn mixed_peer_batch_takes_one_trust_snapshot_and_next_batch_rechecks_tags() {
    let mut surface = FakeAutomation {
        spell_lookup: vec![debuff(70, "Fire Vulnerability Other VII")],
        ..FakeAutomation::default()
    };
    surface
        .peers
        .clients
        .push(network_client(7, "Healer", &["heals"]));
    surface
        .peers
        .clients
        .push(network_client(8, "Debuffer", &["Debuffs"]));
    surface.peers.casts.push(PeerCast {
        landed: false,
        ..peer_cast(1, 70)
    });
    surface.peers.casts.push(peer_cast(2, 70));
    surface.peers.casts.push(PeerCast {
        client_id: 8,
        ..peer_cast(3, 70)
    });
    surface.peers.casts.push(PeerCast {
        client_id: 8,
        ..peer_cast(4, 70)
    });
    let mut fight = Fight::new(surface, fire_vulnerability_rule());
    bind_sharing(&mut fight, true, Some("debuffs"));

    fight.observe_remote_casts();
    assert_eq!(fight.surface.peers.client_capture_count, 1);
    assert_eq!(fight.surface.reported.len(), 2);
    fight.surface.peers.clients.clear();
    fight.surface.peers.casts.push(PeerCast {
        client_id: 8,
        ..peer_cast(5, 70)
    });
    fight.observe_remote_casts();
    fight.observe_remote_casts();
    assert_eq!(fight.surface.peers.client_capture_count, 2);
    assert_eq!(fight.surface.reported.len(), 2);
    assert_eq!(fight.surface.peers.capture_requests, [0, 4, 5]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: UntaggedPeerBatchReadsTagOnceAndDoesNotCaptureClients
#[test]
fn untagged_peer_batch_reads_tag_once_and_does_not_capture_clients() {
    let mut surface = FakeAutomation {
        spell_lookup: vec![debuff(70, "Fire Vulnerability Other VII")],
        ..FakeAutomation::default()
    };
    surface.peers.casts.push(peer_cast(1, 70));
    surface.peers.casts.push(peer_cast(2, 70));
    let mut fight = Fight::new(surface, fire_vulnerability_rule());
    let tag_reads = std::rc::Rc::new(std::cell::Cell::new(0));
    let reads = tag_reads.clone();
    fight.controller.bind_cast_sharing(
        Box::new(|| true),
        Box::new(move || {
            reads.set(reads.get() + 1);
            None
        }),
    );

    fight.observe_remote_casts();

    assert_eq!(tag_reads.get(), 1);
    assert_eq!(fight.surface.peers.client_capture_count, 0);
    assert_eq!(fight.surface.reported.len(), 2);
}

fn bolt_and_fire_vuln() -> FakeAutomation {
    magic(
        drudge(),
        vec![
            magic_spell(100, "Flame Bolt VII", 300),
            debuff(70, "Fire Vulnerability Other VII"),
        ],
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AVulnerabilityAnotherClientLandedStandsTheDebuffChainDown
#[test]
fn a_vulnerability_another_client_landed_stands_the_debuff_chain_down() {
    let mut surface = bolt_and_fire_vuln();
    surface.peers.casts.push(peer_cast(1, 70));
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.toggle();
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&70));
    assert_eq!(fight.surface.reported, [(10, 70, 45.0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AVulnerabilityAnotherClientLandedMakesTheElementChoiceSeeItAsAlreadyThere
#[test]
fn a_vulnerability_another_client_landed_makes_the_element_choice_see_it_as_already_there() {
    fn surface() -> FakeAutomation {
        let mut frost = equipment(700, "Frost Blade", 0x0008).build();
        frost.crushing_blow = true;
        let mut slashing = equipment(701, "Slashing Blade", 0x0001).build();
        slashing.crushing_blow = true;
        FakeAutomation {
            combat_snapshot: physical(),
            targets: vec![target(10, "Magma Golem", 2.0, 0.0)],
            known_combat_spells: vec![debuff(72, "Blade Vulnerability Other VII")],
            equipment_items: vec![frost, slashing],
            ..FakeAutomation::default()
        }
    }
    fn settings() -> CombatSettings {
        let mut settings = row(MonsterActionFlags::ATTACK, MonsterDamageType::Auto);
        settings.combat_item_names.insert("Frost Blade".to_owned());
        settings
            .combat_item_names
            .insert("Slashing Blade".to_owned());
        settings
    }

    let mut alone = Fight::with_game_info(surface(), settings(), game_info());
    alone.toggle();
    for _ in 0..4 {
        alone.tick(0.25);
    }
    assert_eq!(alone.surface.last_equip_object_id, 700);

    let mut helped_surface = surface();
    helped_surface.peers.casts.push(peer_cast(1, 72));
    let mut helped = Fight::with_game_info(helped_surface, settings(), game_info());
    helped.toggle();
    helped.observe_remote_casts();
    for _ in 0..4 {
        helped.tick(0.25);
    }
    assert_eq!(helped.surface.last_equip_object_id, 701);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ACastFromThisCharacterIsIgnored
#[test]
fn a_cast_from_this_character_is_ignored() {
    let mut surface = bolt_and_fire_vuln();
    surface.peers.casts.push(PeerCast {
        caster_id: SELF_ID,
        ..peer_cast(1, 70)
    });
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.toggle();
    fight.observe_remote_casts();
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
    assert!(fight.surface.reported.is_empty());
    assert_eq!(fight.surface.peers.capture_requests, [0, 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARemoteSpellThatIsNotATrackedDebuffIsDroppedFromTheDebuffTable
#[test]
fn a_remote_spell_that_is_not_a_tracked_debuff_is_dropped_from_the_debuff_table() {
    let strength = SpellInfo {
        is_debuff: false,
        is_offensive: false,
        is_beneficial: true,
        ..debuff(200, "Strength Other VII")
    };
    let mut surface = bolt_and_fire_vuln();
    surface.spell_lookup = vec![strength];
    surface.peers.casts.push(PeerCast {
        seconds_remaining: 60.0,
        ..peer_cast(1, 200)
    });
    surface.peers.casts.push(PeerCast {
        seconds_remaining: 60.0,
        ..peer_cast(2, 9999)
    });
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.toggle();
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
    assert_eq!(fight.surface.reported, [(10, 200, 60.0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheCursorAdvancesAndACastIsNeverAppliedTwice
#[test]
fn the_cursor_advances_and_a_cast_is_never_applied_twice() {
    let mut surface = magic(
        drudge(),
        vec![
            magic_spell(100, "Flame Bolt VII", 300),
            debuff(70, "Fire Vulnerability Other VII"),
            debuff(71, "Acid Vulnerability Other VII"),
        ],
    );
    surface.peers.casts.push(peer_cast(3, 70));
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.toggle();
    fight.observe_remote_casts();
    fight.observe_remote_casts();
    fight.observe_remote_casts();
    assert_eq!(fight.surface.peers.capture_requests, [0, 3, 3]);
    assert_eq!(fight.surface.reported.len(), 1);
    assert_eq!(
        fight
            .log_texts()
            .iter()
            .filter(|line| line.starts_with("Peer cast:"))
            .count(),
        1
    );

    fight.surface.peers.casts.push(peer_cast(5, 71));
    fight.observe_remote_casts();
    assert_eq!(fight.surface.peers.capture_requests, [0, 3, 3, 3]);
    assert_eq!(fight.surface.reported.len(), 2);
    fight.observe_remote_casts();
    assert_eq!(fight.surface.peers.capture_requests.last(), Some(&5));
    assert_eq!(fight.surface.reported.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: RemoteCastsAreTakenOnlyFromPeersCarryingTheChosenTag
#[test]
fn remote_casts_are_taken_only_from_peers_carrying_the_chosen_tag() {
    let mut surface = bolt_and_fire_vuln();
    surface
        .peers
        .clients
        .push(network_client(7, "Healer", &["heals"]));
    surface
        .peers
        .clients
        .push(network_client(8, "Debuffer", &["Debuffs", "mules"]));
    for (sequence, client_id, seconds) in [(1, 7, 20.0), (2, 8, 30.0), (3, 9, 40.0)] {
        surface.peers.casts.push(PeerCast {
            client_id,
            seconds_remaining: seconds,
            ..peer_cast(sequence, 70)
        });
    }
    let mut fight = Fight::new(surface, fire_vulnerability_rule());
    bind_sharing(&mut fight, true, Some("debuffs"));

    fight.toggle();
    fight.observe_remote_casts();

    assert_eq!(fight.surface.reported, [(10, 70, 30.0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: WithSharingOffNothingIsTakenIn
#[test]
fn with_sharing_off_nothing_is_taken_in() {
    let mut surface = bolt_and_fire_vuln();
    surface.peers.casts.push(peer_cast(1, 70));
    let mut fight = Fight::new(surface, fire_vulnerability_rule());
    bind_sharing(&mut fight, false, None);

    fight.toggle();
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert!(fight.surface.peers.capture_requests.is_empty());
    assert!(fight.surface.reported.is_empty());
    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ACastSeenBeforeTheMacroStartedIsTakenInWhenItStarts
#[test]
fn a_cast_seen_before_the_macro_started_is_taken_in_when_it_starts() {
    let mut surface = bolt_and_fire_vuln();
    surface.peers.casts.push(peer_cast(1, 70));
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.observe_remote_casts();
    assert_eq!(fight.surface.peers.capture_requests, [0]);
    assert_eq!(fight.surface.reported.len(), 1);
    assert!(
        !fight
            .log_texts()
            .iter()
            .any(|line| line.starts_with("Peer cast:"))
    );

    fight.toggle();
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert_eq!(fight.surface.peers.capture_requests, [0, 0]);
    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&70));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ALearnedDebuffAnnouncesItsAttemptWithTheSkillAndItsSuccessWithTheDuration
#[test]
fn a_learned_debuff_announces_its_attempt_with_the_skill_and_its_success_with_the_duration() {
    let mut surface = bolt_and_fire_vuln();
    surface.character_skills = vec![skill(
        31,
        "Creature Enchantment",
        Training::Specialized,
        320,
    )];
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
    assert_eq!(fight.surface.peers.attempts, [(10, 70, 320)]);
    assert!(fight.surface.peers.successes.is_empty());

    // The first try fails: nothing landed, so nothing is announced.
    fight.surface.last_cast_completion = completion(1, 70, 10, 0x0402);
    fight.tick(0.25);
    assert!(fight.surface.peers.successes.is_empty());

    // The second try goes out and lands.
    fight.tick(0.25);
    assert_eq!(fight.surface.peers.attempts.len(), 2);
    fight.surface.last_cast_completion = completion(2, 70, 10, 0);
    fight.tick(0.25);
    assert_eq!(fight.surface.peers.successes, [(10, 70, 320, 60.0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASuccessReadOnTheFrameIsAnnouncedOnce
#[test]
fn a_success_read_on_the_frame_is_announced_once() {
    let mut fight = Fight::new(bolt_and_fire_vuln(), fire_vulnerability_rule());

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.peers.attempts, [(10, 70, 0)]);

    fight.surface.last_cast_completion = completion(1, 70, 10, 0);
    fight
        .controller
        .observe_learned_debuff_receipt(&mut fight.surface, 0.1);
    fight.tick(0.25);
    fight
        .controller
        .observe_learned_debuff_receipt(&mut fight.surface, 0.1);

    assert_eq!(fight.surface.peers.successes, [(10, 70, 0, 60.0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AWandDebuffAnnouncesItsAttemptAndItsConfirmedSuccess
#[test]
fn a_wand_debuff_announces_its_attempt_and_its_confirmed_success() {
    let lens = imperil_lens();
    let surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: drudge(),
        spell_lookup: vec![imperil_90()],
        item_entries: vec![lens.clone()],
        character_skills: vec![skill(31, "Creature Enchantment", Training::Trained, 280)],
        ..FakeAutomation::default()
    };
    let mut settings = debuff_only(MonsterActionFlags::IMPERIL);
    settings.combat_item_object_ids.insert(lens.object_id);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_applied_item, (800, 10));
    assert_eq!(fight.surface.peers.attempts, [(10, 90, 280)]);
    assert!(fight.surface.peers.successes.is_empty());

    fight.surface.last_item_completion = ItemUseCompletion {
        revision: 1,
        source_id: 800,
        target_id: 10,
        weenie_error: 0,
    };
    fight.surface.chat_messages = vec![ChatLine {
        sequence: 1,
        text: "You cast Imperil Other VII on Drudge.".to_owned(),
        log_text_type: 0x07,
        ..ChatLine::default()
    }];
    fight.tick(0.25);

    assert_eq!(fight.surface.peers.successes, [(10, 90, 280, 60.0)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: WithSharingOffNothingIsAnnounced
#[test]
fn with_sharing_off_nothing_is_announced() {
    let mut fight = Fight::new(bolt_and_fire_vuln(), fire_vulnerability_rule());
    bind_sharing(&mut fight, false, None);

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, 70, 10, 0);
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.contains(&70));
    assert!(fight.surface.peers.attempts.is_empty());
    assert!(fight.surface.peers.successes.is_empty());
}
