//! The fight's debuff chain, its elements and the casts other clients on
//! this computer report (`CombatControllerTests`, fourth part).

mod common;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::{CombatMode, CombatTarget};
use ac_plugin_abi::items::{EquipmentItem, EquipmentObservation, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use ac_plugin_abi::peers::PeerCast;
use ac_plugin_abi::projectile::{ProjectilePathResult, ProjectilePathStatus};
use ac_plugin_abi::types::ChatLine;
use common::{
    FakeAutomation, Fight, debuff, debuff_only, equipment, game_info, in_mode, inventory_item,
    magic_spell, only_rule, peaceful, physical, rule_with, single_rule, skill, spell, target,
    wielded_caster, wielded_planned_weapon,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::model::{CombatSettings, DebuffEachFirst, TargetSelectionMethod};
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
            // The earlier step of the chain, and the only spell here whose
            // family declares a flight (a Corruption is thrown like a
            // bolt).
            debuff(84, "Corruption VII"),
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
                    | MonsterActionFlags::CORRUPTION
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

/// Fire Vulnerability Other VII (82) and VI (85) known; a fire Vuln rule.
fn fire_vulns(skill_current: Option<u32>) -> Fight {
    let mut surface = magic(
        drudge(),
        vec![
            debuff(82, "Fire Vulnerability Other VII"),
            SpellInfo {
                tier: 6,
                difficulty: 225,
                ..debuff(85, "Fire Vulnerability Other VI")
            },
        ],
    );
    if let Some(current) = skill_current {
        // A Vulnerability is life magic.
        surface
            .character_skills
            .push(skill(33, "Life Magic", Training::Specialized, current));
    }
    Fight::new(surface, fire_vulnerability_rule())
}

// The learned debuff is the strongest tier the character can cast, as the
// attack's tier walk picks (owner, 2026-11-12: out of Mana Scarabs the bot
// kept casting the level 7 Vuln and failing on components).
#[test]
fn a_debuff_tier_the_pack_cannot_pay_for_is_not_picked() {
    let mut fight = fire_vulns(None);
    fight.surface.missing_component_spell_ids.insert(82);
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (85, 10));
}

#[test]
fn no_debuff_is_cast_when_the_pack_pays_for_no_tier() {
    let mut fight = fire_vulns(None);
    fight.surface.missing_component_spell_ids.insert(82);
    fight.surface.missing_component_spell_ids.insert(85);
    fight.toggle();
    fight.tick(0.25);
    assert!(!fight.surface.cast_spell_ids.contains(&82));
    assert!(!fight.surface.cast_spell_ids.contains(&85));
}

// The tier also needs the hunt margin over its difficulty (default 25).
#[test]
fn a_debuff_tier_short_of_the_hunt_margin_is_not_picked() {
    // 260 clears VI (225 + 25) but not VII (250 + 25).
    let mut fight = fire_vulns(Some(260));
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (85, 10));

    let mut fight = fire_vulns(Some(275));
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (82, 10));
}

/// `Imperil Other VII` as the C# rig has it, in the spell table's line (an
/// Imperil is life magic).
fn imperil_90() -> SpellInfo {
    mosstank_combat::testing::in_debuff_line(SpellInfo {
        is_debuff: true,
        is_offensive: true,
        duration_seconds: 60.0,
        ..spell(90, "Imperil Other VII")
    })
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
        character_skills: vec![skill(33, "Life Magic", Training::Trained, 500)],
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
            // A Corruption is thrown like a bolt: the one debuff here with
            // a flight to block.
            debuff(84, "Corruption VII"),
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
                MonsterActionFlags::ATTACK | MonsterActionFlags::CORRUPTION,
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

// ---- Casts the team reported ----
//
// The team is the fellowship (`HelpFellows`) and the network peers
// (`HelpNetworkPeers`); there is no tag (owner, 2026-11-14).

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: RejectedOnlyPeerBatchAdvancesCursorWithoutReadingTrustSnapshot
/// The C# read the tag's trust snapshot; with the tag gone, a batch with
/// nothing to take in does not read the fellowship either.
#[test]
fn rejected_only_peer_batch_advances_cursor_without_reading_the_fellowship() {
    let mut surface = FakeAutomation::default();
    surface.peers.casts.push(PeerCast {
        caster_id: SELF_ID,
        ..peer_cast(1, 70)
    });
    // (An attempt, which the C# also skipped, is a teammate's claim now.)
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
    fight.vitals.help_network_peers = false;

    fight.observe_remote_casts();
    fight.observe_remote_casts();

    assert_eq!(fight.surface.peers.capture_requests, [0, 5]);
    assert_eq!(fight.surface.peers.client_capture_count, 0);
    assert_eq!(fight.surface.peers.fellowship_reads, 0);
    assert!(fight.surface.reported.is_empty());
}

/// Two landed casts, one by a fellow and one by a peer outside the
/// fellowship.
fn fellow_and_stranger_casts() -> FakeAutomation {
    let mut surface = FakeAutomation {
        spell_lookup: vec![debuff(70, "Fire Vulnerability Other VII")],
        ..FakeAutomation::default()
    };
    surface.peers.fellows.push(0x5000_0002);
    surface.peers.casts.push(peer_cast(1, 70));
    surface.peers.casts.push(PeerCast {
        client_id: 8,
        caster_id: 0x5000_0009,
        target_id: 11,
        ..peer_cast(2, 70)
    });
    surface
}

/// New: helping the fellows alone takes in the fellows' casts and no one
/// else's, reading the fellowship once for the batch.
#[test]
fn helping_fellows_alone_takes_in_a_fellows_cast_and_not_a_strangers() {
    let mut fight = Fight::new(fellow_and_stranger_casts(), fire_vulnerability_rule());
    fight.vitals.help_network_peers = false;

    fight.observe_remote_casts();

    assert_eq!(fight.surface.reported, [(10, 70, 45.0)]);
    assert_eq!(fight.surface.peers.fellowship_reads, 1);
    assert_eq!(fight.surface.peers.capture_requests, [0]);
}

/// New: helping the network peers takes in every peer's cast, fellow or
/// not, without reading the fellowship.
#[test]
fn helping_network_peers_takes_in_every_peers_cast() {
    let mut fight = Fight::new(fellow_and_stranger_casts(), fire_vulnerability_rule());
    fight.vitals.help_others = false;

    fight.observe_remote_casts();

    assert_eq!(fight.surface.reported, [(10, 70, 45.0), (11, 70, 45.0)]);
    assert_eq!(fight.surface.peers.fellowship_reads, 0);
}

/// New: helping the fellows alone still announces this character's casts.
#[test]
fn helping_the_fellows_alone_announces_the_debuff_attempt() {
    let mut fight = Fight::new(bolt_and_fire_vuln(), fire_vulnerability_rule());
    fight.vitals.help_network_peers = false;

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
    assert_eq!(fight.surface.peers.attempts.len(), 1);
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

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: WithSharingOffNothingIsTakenIn
/// Sharing is off when the character helps neither the fellows nor the
/// network peers.
#[test]
fn with_sharing_off_nothing_is_taken_in() {
    let mut surface = bolt_and_fire_vuln();
    surface.peers.casts.push(peer_cast(1, 70));
    let mut fight = Fight::new(surface, fire_vulnerability_rule());
    fight.vitals.help_others = false;
    fight.vitals.help_network_peers = false;

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
    // A vulnerability is life magic.
    surface.character_skills = vec![skill(33, "Life Magic", Training::Specialized, 320)];
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
        character_skills: vec![skill(33, "Life Magic", Training::Trained, 280)],
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
    fight.vitals.help_others = false;
    fight.vitals.help_network_peers = false;

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, 70, 10, 0);
    fight.tick(0.25);

    assert!(fight.surface.cast_spell_ids.contains(&70));
    assert!(fight.surface.peers.attempts.is_empty());
    assert!(fight.surface.peers.successes.is_empty());
}

// Not applicable: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: FailedTrustReadDoesNotConsumeAnEligiblePeerCast: the client list read cannot fail through the SDK, so there is no failed read to retry (register COMBAT-4)

// The Void curses as the spell table has them: Corrosion (family 637) and
// Corruption (family 638) are enchantment projectiles with no duration of
// their own, and the five bolts of Corruption make it a spell that strikes
// several monsters.
const CORROSION: u32 = 5393;
const CORRUPTION: u32 = 5401;
const INCANTATION_OF_CORRUPTION: u32 = 5402;

/// A fight debuffing with `known`, the rule asking for `flag` only.
fn void_fight(
    flag: MonsterActionFlags,
    known: &[&str],
    targets: Vec<CombatTarget>,
    settings: CombatSettings,
) -> Fight {
    let mut rule = debuff_only(flag);
    rule.maximum_range = 40.0;
    rule.scan_interval_seconds = 0.05;
    rule.blacklist_monster_attempt_count = settings.blacklist_monster_attempt_count;
    rule.blacklist_monster_timeout_seconds = settings.blacklist_monster_timeout_seconds;
    rule.use_projectile_awareness = settings.use_projectile_awareness;
    let known = known
        .iter()
        .map(|name| mosstank_combat::testing::debuff_spell(name))
        .collect();
    Fight::new(magic(targets, known), rule)
}

fn corrosion_fight(settings: CombatSettings) -> Fight {
    void_fight(
        MonsterActionFlags::CORROSION,
        &["Corrosion VII"],
        drudge(),
        settings,
    )
}

fn corruption_fight(settings: CombatSettings) -> Fight {
    void_fight(
        MonsterActionFlags::CORRUPTION,
        &["Corruption VII"],
        drudge(),
        settings,
    )
}

/// Any miss gives the monster up at once: a fight that still has its
/// target counted none.
fn blacklist_on_first_miss() -> CombatSettings {
    CombatSettings {
        blacklist_monster_attempt_count: 0,
        blacklist_monster_timeout_seconds: 300.0,
        ..CombatSettings::default()
    }
}

fn magic_line(sequence: u64, text: &str) -> ChatLine {
    ChatLine {
        sequence,
        text: text.to_owned(),
        log_text_type: 0x07,
        ..ChatLine::default()
    }
}

fn ticks(fight: &mut Fight, count: usize) {
    for _ in 0..count {
        fight.tick(0.25);
    }
}

fn gave_up(fight: &Fight, name: &str) -> bool {
    fight
        .surface
        .posted_system_messages
        .iter()
        .any(|message| message.contains(&format!("Cannot hit {name}")))
}

fn logged(fight: &Fight, text: &str) -> bool {
    fight.log_texts().iter().any(|line| line.contains(text))
}

// The server sends the landing line as the bolt lands, which at a short
// range is before it finishes the cast (a second of recoil later).
#[test]
fn a_landing_line_before_the_cast_finishes_counts_and_corrosion_lasts_sixteen_seconds() {
    let mut fight = corrosion_fight(blacklist_on_first_miss());

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION]);

    fight.surface.chat_messages = vec![magic_line(1, "You cast Corrosion VII on Drudge")];
    fight.tick(0.25);
    assert_eq!(fight.surface.peers.successes, [(10, CORROSION, 0, 16.0)]);

    // The server finishes the cast: nothing more to wait for.
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);
    assert!(!fight.status().contains("land"), "{}", fight.status());

    // Up for its sixteen seconds from the landing (at 0.5 s): no recast,
    // and no miss when the landing budget runs out.
    ticks(&mut fight, 62);
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION]);
    assert!(!gave_up(&fight, "Drudge"));
    assert!(!logged(&fight, "never landed"));
    assert_eq!(fight.surface.peers.successes.len(), 1);

    ticks(&mut fight, 2);
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION, CORROSION]);
}

#[test]
fn a_landing_line_after_the_cast_finishes_counts_and_corruption_lasts_thirty_one_seconds() {
    let mut fight = corruption_fight(CombatSettings::default());

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.cast_spell_ids, [CORRUPTION]);

    // The server finished the cast: the projectile is flying, not landed.
    fight.surface.last_cast_completion = completion(1, CORRUPTION, 10, 0);
    fight.tick(0.25);
    assert!(fight.surface.peers.successes.is_empty());
    assert!(fight.status().contains("land"), "{}", fight.status());

    fight.surface.chat_messages = vec![magic_line(1, "You cast Corruption VII on Drudge")];
    fight.tick(0.25);
    assert_eq!(fight.surface.peers.successes, [(10, CORRUPTION, 0, 31.0)]);

    // Up for thirty-one seconds from the landing (at 0.75 s).
    ticks(&mut fight, 122);
    assert_eq!(fight.surface.cast_spell_ids, [CORRUPTION]);
    ticks(&mut fight, 4);
    assert_eq!(fight.surface.cast_spell_ids, [CORRUPTION, CORRUPTION]);
}

#[test]
fn a_corruption_bolt_landing_on_another_monster_is_not_this_ones() {
    let mut fight = corruption_fight(CombatSettings::default());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORRUPTION, 10, 0);
    fight.tick(0.25);
    fight.surface.chat_messages = vec![magic_line(
        1,
        "You cast Corruption VII on Tusker Guard, refreshing Corruption VII",
    )];
    fight.tick(0.25);

    assert!(fight.surface.peers.successes.is_empty());
    assert!(fight.status().contains("land"), "{}", fight.status());
}

#[test]
fn a_corruption_that_never_lands_is_no_miss() {
    let mut fight = corruption_fight(blacklist_on_first_miss());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORRUPTION, 10, 0);
    // Past the landing budget: given up on, and cast again.
    ticks(&mut fight, 20);

    assert!(
        logged(&fight, "never landed on Drudge"),
        "{:?}",
        fight.log_texts()
    );
    assert!(fight.has_target());
    assert!(!gave_up(&fight, "Drudge"));
    assert_eq!(fight.surface.cast_spell_ids, [CORRUPTION, CORRUPTION]);
}

#[test]
fn a_resisted_projectile_debuff_is_cast_again_without_a_miss() {
    let mut fight = corrosion_fight(blacklist_on_first_miss());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);
    fight.surface.chat_messages = vec![magic_line(1, "Drudge resists your spell")];
    fight.tick(0.25);
    fight.tick(0.25);

    assert!(fight.has_target());
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION, CORROSION]);
    assert!(fight.surface.peers.successes.is_empty());
}

#[test]
fn five_corrosions_that_never_land_give_the_monster_up() {
    let mut fight = corrosion_fight(CombatSettings {
        blacklist_monster_attempt_count: 4,
        blacklist_monster_timeout_seconds: 120.0,
        ..CombatSettings::default()
    });

    fight.toggle();
    for revision in 1..=5 {
        fight.tick(0.25);
        assert!(fight.has_target(), "cast {revision}");
        let cast = usize::try_from(revision).unwrap();
        assert_eq!(fight.surface.cast_spell_ids.len(), cast);
        fight.surface.last_cast_completion = completion(revision, CORROSION, 10, 0);
        // Into the wall: no result line before the landing budget runs out.
        for _ in 0..16 {
            fight.tick(0.25);
            if fight.surface.cast_spell_ids.len() > cast {
                break;
            }
        }
    }

    assert!(!fight.has_target());
    assert!(
        gave_up(&fight, "Drudge (10)"),
        "{:?}",
        fight.surface.posted_system_messages
    );
    assert_eq!(fight.surface.cast_spell_ids.len(), 5);
}

#[test]
fn a_monster_dead_while_the_curse_flies_is_no_miss_and_the_next_is_cursed() {
    let mut fight = void_fight(
        MonsterActionFlags::CORROSION,
        &["Corrosion VII"],
        vec![
            target(10, "Drudge", 5.0, 0.0),
            target(11, "Tusker", 8.0, 0.0),
        ],
        blacklist_on_first_miss(),
    );

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (CORROSION, 10));
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);
    assert!(fight.status().contains("land"), "{}", fight.status());

    // It dies (a fellow's blow) and stays in the scan, dead.
    fight.surface.targets[0].is_dead = true;
    ticks(&mut fight, 4);

    assert_eq!(fight.surface.last_targeted_cast, (CORROSION, 11));
    ticks(&mut fight, 20);
    assert!(!gave_up(&fight, "Drudge"));
    assert!(
        !logged(&fight, "never landed on Drudge"),
        "{:?}",
        fight.log_texts()
    );
}

// The retail client's wording of a refused target. The local server says
// out of range in its finish-cast answer instead, the case of
// `a_cast_the_server_refuses_is_cast_again_with_no_miss`.
#[test]
fn a_projectile_debuff_out_of_range_gives_the_monster_up() {
    let mut fight = corrosion_fight(CombatSettings::default());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.chat_messages = vec![magic_line(1, "Target is out of range")];
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);

    assert!(!fight.has_target());
    assert!(fight.surface.peers.successes.is_empty());
}

// A single-target spell has only its one monster: any resist is that
// spell's, whatever name the line carries.
#[test]
fn a_resist_ends_a_single_target_curses_wait_whoever_it_names() {
    let mut fight = corrosion_fight(blacklist_on_first_miss());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);
    fight.surface.chat_messages = vec![magic_line(1, "Tusker Guard resists your spell")];
    fight.tick(0.25);
    fight.tick(0.25);

    assert!(fight.has_target());
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION, CORROSION]);
}

// Corruption's side bolts can be resisted by others while the aimed one
// is still on its way.
#[test]
fn a_side_bolts_resist_does_not_end_corruptions_wait() {
    let mut fight = corruption_fight(CombatSettings::default());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORRUPTION, 10, 0);
    fight.tick(0.25);
    fight.surface.chat_messages = vec![magic_line(1, "Tusker Guard resists your spell")];
    fight.tick(0.25);

    assert_eq!(fight.surface.cast_spell_ids, [CORRUPTION]);
    assert!(fight.status().contains("land"), "{}", fight.status());
}

// A late line of the cast before is in the chat when this one goes out.
#[test]
fn a_line_from_before_the_cast_is_not_its_landing() {
    let mut fight = corrosion_fight(CombatSettings::default());
    fight.surface.chat_messages = vec![magic_line(1, "You cast Corrosion VII on Drudge")];

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION]);
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);

    assert!(fight.surface.peers.successes.is_empty());
    assert!(fight.status().contains("land"), "{}", fight.status());
}

#[test]
fn a_killing_blow_on_the_monster_ends_the_wait_with_no_miss() {
    let mut fight = corrosion_fight(blacklist_on_first_miss());

    fight.toggle();
    fight.tick(0.25);
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0);
    fight.tick(0.25);
    assert!(fight.status().contains("land"), "{}", fight.status());
    fight.surface.chat_messages = vec![ChatLine {
        sequence: 1,
        text: "You killed Drudge!".to_owned(),
        log_text_type: 0,
        ..ChatLine::default()
    }];
    fight.tick(0.25);

    assert!(!fight.status().contains("land"), "{}", fight.status());
    assert!(!logged(&fight, "never landed"));
    assert!(!gave_up(&fight, "Drudge"));
}

// The server refuses a cast at a monster out of reach in its finish-cast
// answer (0x550, missile out of range): nothing flew, so nothing is awaited
// and it is no miss.
#[test]
fn a_cast_the_server_refuses_is_cast_again_with_no_miss() {
    let mut fight = corrosion_fight(blacklist_on_first_miss());

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION]);
    fight.surface.last_cast_completion = completion(1, CORROSION, 10, 0x550);
    fight.tick(0.25);

    assert!(!fight.status().contains("land"), "{}", fight.status());
    assert_eq!(fight.surface.cast_spell_ids, [CORROSION, CORROSION]);
    assert!(fight.has_target());
    assert!(!gave_up(&fight, "Drudge"));
    assert!(fight.surface.peers.successes.is_empty());
}

#[test]
fn the_incantation_landing_line_marks_the_curse_up() {
    let surface = magic(
        vec![target(10, "Tamed Armoredillo", 5.0, 0.0)],
        vec![mosstank_combat::testing::debuff_spell(
            "Incantation of Corruption",
        )],
    );
    let mut rule = debuff_only(MonsterActionFlags::CORRUPTION);
    rule.maximum_range = 40.0;
    let mut fight = Fight::new(surface, rule);

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.cast_spell_ids, [INCANTATION_OF_CORRUPTION]);
    fight.surface.last_cast_completion = completion(1, INCANTATION_OF_CORRUPTION, 10, 0);
    fight.tick(0.25);
    fight.surface.chat_messages = vec![magic_line(
        1,
        "You cast Incantation of Corruption on Tamed Armoredillo",
    )];
    fight.tick(0.25);

    assert_eq!(
        fight.surface.peers.successes,
        [(10, INCANTATION_OF_CORRUPTION, 0, 31.0)]
    );
    ticks(&mut fight, 40);
    assert_eq!(fight.surface.cast_spell_ids, [INCANTATION_OF_CORRUPTION]);
}

/// A caster with the life and creature skills the real debuffs' reach is
/// worked out from, knowing `known`.
fn debuffer(known: &[&str]) -> FakeAutomation {
    let mut surface = magic(
        drudge(),
        known
            .iter()
            .map(|name| mosstank_combat::testing::debuff_spell(name))
            .collect(),
    );
    surface.character_skills = vec![
        skill(31, "Creature Enchantment", Training::Specialized, 400),
        skill(33, "Life Magic", Training::Specialized, 400),
    ];
    surface
}

/// The rule that asks for one debuff: a vulnerability comes with its
/// element's attack row.
fn asking_for(flag: MonsterActionFlags, element: MonsterDamageType) -> CombatSettings {
    if flag == MonsterActionFlags::VULNERABILITY {
        row(MonsterActionFlags::ATTACK | flag, element)
    } else {
        let mut settings = debuff_only(flag);
        settings.maximum_range = 40.0;
        settings
    }
}

/// Each debuff with the names the game gives its sixth and seventh levels
/// and the seventh level of its self-cast twin.
fn seventh_levels() -> [(
    MonsterActionFlags,
    MonsterDamageType,
    &'static str,
    &'static str,
    &'static str,
); 12] {
    use MonsterActionFlags as F;
    use MonsterDamageType as D;
    [
        (
            F::IMPERIL,
            D::Auto,
            "Imperil Other VI",
            "Gossamer Flesh",
            "Imperil Self VII",
        ),
        (
            F::YIELD,
            D::Auto,
            "Magic Yield Other VI",
            "Futility",
            "Magic Yield Self VII",
        ),
        (
            F::FESTER,
            D::Auto,
            "Fester Other VI",
            "Decrepitude's Grasp",
            "Fester Self VII",
        ),
        (
            F::GRAVITY_WELL,
            D::Auto,
            "Vulnerability Other VI",
            "Gravity Well",
            "Vulnerability Self VII",
        ),
        (
            F::BROADSIDE,
            D::Auto,
            "Defenselessness Other VI",
            "Broadside of a Barn",
            "Defenselessness Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Acid,
            "Acid Vulnerability Other VI",
            "Olthoi's Gift",
            "Acid Vulnerability Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Slash,
            "Blade Vulnerability Other VI",
            "Swordsman's Gift",
            "Blade Vulnerability Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Bludgeon,
            "Bludgeoning Vulnerability Other VI",
            "Tusker's Gift",
            "Bludgeoning Vulnerability Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Cold,
            "Cold Vulnerability Other VI",
            "Gelidite's Gift",
            "Cold Vulnerability Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Fire,
            "Fire Vulnerability Other VI",
            "Inferno's Gift",
            "Fire Vulnerability Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Electric,
            "Lightning Vulnerability Other VI",
            "Astyrrian's Gift",
            "Lightning Vulnerability Self VII",
        ),
        (
            F::VULNERABILITY,
            D::Pierce,
            "Piercing Vulnerability Other VI",
            "Archer's Gift",
            "Piercing Vulnerability Self VII",
        ),
    ]
}

// The game gives the seventh level of a debuff a name of its own: a
// character who knows it throws it, not the sixth.
#[test]
fn the_seventh_level_of_a_debuff_goes_out_under_its_own_name() {
    for (flag, element, sixth, seventh, twin) in seventh_levels() {
        let mut fight = Fight::new(debuffer(&[twin, sixth, seventh]), asking_for(flag, element));
        fight.toggle();
        fight.tick(0.25);

        let expected = mosstank_combat::testing::debuff_spell(seventh).spell_id;
        assert_eq!(
            fight.surface.last_targeted_cast,
            (expected, 10),
            "{seventh}"
        );
    }
}

// The lower levels of the two debuffs a rule names by their seventh level
// are those debuffs too.
#[test]
fn a_debuff_named_for_its_seventh_level_goes_out_at_the_level_known() {
    for (flag, element, sixth, _, twin) in seventh_levels() {
        let mut fight = Fight::new(debuffer(&[twin, sixth]), asking_for(flag, element));
        fight.toggle();
        fight.tick(0.25);

        let expected = mosstank_combat::testing::debuff_spell(sixth).spell_id;
        assert_eq!(fight.surface.last_targeted_cast, (expected, 10), "{sixth}");
    }
}

// The self-cast twin is in the same family and is a debuff the character
// knows: it is never thrown at a monster.
#[test]
fn the_self_cast_twin_of_a_debuff_is_never_thrown_at_a_monster() {
    for (flag, element, _, _, twin) in seventh_levels() {
        let mut fight = Fight::new(debuffer(&[twin]), asking_for(flag, element));
        fight.toggle();
        fight.tick(0.25);

        assert!(fight.surface.cast_spell_ids.is_empty(), "{twin}");
    }
}

// ---- Claims: a teammate's cast attempt ----
//
// A teammate's attempt on a monster counts as that debuff being up there
// for six seconds (at the attempt's tier or below), so this character
// picks another monster or another debuff; the landed cast replaces it.

fn two_drudges_needing_fire_vuln() -> FakeAutomation {
    magic(
        vec![
            target(10, "Drudge", 5.0, 0.0),
            target(11, "Drudge", 8.0, 0.0),
        ],
        vec![
            magic_spell(100, "Flame Bolt VII", 300),
            debuff(70, "Fire Vulnerability Other VII"),
        ],
    )
}

/// What the first bot announced, as the second hears it through the hub.
fn heard_attempts(from: &FakeAutomation, caster_id: u32) -> Vec<PeerCast> {
    from.peers
        .attempts
        .iter()
        .enumerate()
        .map(|(index, &(target_id, spell_id, skill))| PeerCast {
            sequence: i64::try_from(index).unwrap() + 1,
            client_id: 7,
            caster_id,
            target_id,
            spell_id,
            effective_skill: skill,
            seconds_remaining: 0.0,
            landed: false,
            ..PeerCast::default()
        })
        .collect()
}

/// Starts the macro and reads the team's casts once: the backlog read from
/// the start of the list is old, so what is pushed after this is heard as
/// it happens.
fn start_listening(fight: &mut Fight) {
    fight.toggle();
    fight.observe_remote_casts();
}

/// An attempt heard as it happens.
fn attempt(sequence: i64, spell_id: u32) -> PeerCast {
    PeerCast {
        landed: false,
        seconds_remaining: 0.0,
        ..peer_cast(sequence, spell_id)
    }
}

fn debuff_each_of_a_priority() -> CombatSettings {
    CombatSettings {
        debuff_each_first: DebuffEachFirst::PRIORITY,
        ..fire_vulnerability_rule()
    }
}

/// New: two bots, two monsters that both need the same vulnerability. The
/// first casts at the nearer; the second hears the attempt and debuffs the
/// other one.
#[test]
fn a_teammates_attempt_sends_the_second_debuffer_to_the_other_monster() {
    let mut first = Fight::new(two_drudges_needing_fire_vuln(), debuff_each_of_a_priority());
    first.toggle();
    first.tick(0.25);
    assert_eq!(first.surface.last_targeted_cast, (70, 10));

    let mut second = Fight::new(two_drudges_needing_fire_vuln(), debuff_each_of_a_priority());
    start_listening(&mut second);
    second.surface.peers.casts = heard_attempts(&first.surface, 0x5000_0002);
    second.observe_remote_casts();
    second.tick(0.25);

    assert_eq!(second.surface.last_targeted_cast, (70, 11));
    // An attempt is not a landed debuff: the client's tracker is not told.
    assert!(second.surface.reported.is_empty());
}

/// New: debuffing one monster at a time, the second bot leaves the claimed
/// debuff alone and attacks.
#[test]
fn a_teammates_attempt_stands_the_same_debuff_down_on_that_monster() {
    let mut fight = Fight::new(bolt_and_fire_vuln(), fire_vulnerability_rule());

    start_listening(&mut fight);
    fight.surface.peers.casts.push(attempt(1, 70));
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (100, 10));
    assert!(!fight.surface.cast_spell_ids.contains(&70));
}

/// New: a claim holds for six seconds, then the debuff is due again.
#[test]
fn a_claim_runs_out_after_six_seconds() {
    let mut fight = Fight::new(bolt_and_fire_vuln(), fire_vulnerability_rule());
    start_listening(&mut fight);
    fight.surface.peers.casts.push(attempt(1, 70));
    fight.observe_remote_casts();
    fight.tick(0.25);
    assert!(!fight.surface.cast_spell_ids.contains(&70));
    assert_eq!(fight.controller.team_claims().len(), 1);

    for _ in 0..24 {
        fight.tick(0.25);
    }

    assert!(fight.surface.cast_spell_ids.contains(&70));
    assert!(fight.controller.team_claims().is_empty());
}

/// New: a claim at a lower tier does not hold off a stronger debuff.
#[test]
fn a_weaker_claim_does_not_hold_off_a_stronger_debuff() {
    let mut surface = bolt_and_fire_vuln();
    surface.spell_lookup.push(SpellInfo {
        tier: 3,
        ..debuff(69, "Fire Vulnerability Other III")
    });
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    start_listening(&mut fight);
    fight.surface.peers.casts.push(attempt(1, 69));
    fight.observe_remote_casts();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
}

/// New: the landed cast replaces the claim with the real record.
#[test]
fn the_landed_cast_replaces_the_claim() {
    let mut fight = Fight::new(bolt_and_fire_vuln(), fire_vulnerability_rule());
    start_listening(&mut fight);
    fight.surface.peers.casts.push(attempt(1, 70));
    fight.observe_remote_casts();
    assert_eq!(fight.controller.team_claims().len(), 1);

    fight.surface.peers.casts.push(peer_cast(2, 70));
    fight.observe_remote_casts();

    assert!(fight.controller.team_claims().is_empty());
    assert_eq!(fight.controller.team_debuffs().len(), 1);
}

// ---- Focus counts the team's debuffs, never this character's own ----

/// The first debuffer's debuff lands on the monster it claimed: the second,
/// debuffing the other monster, stays there and finishes its job.
#[test]
fn the_second_debuffer_stays_on_its_monster_when_the_firsts_debuff_lands() {
    let mut first = Fight::new(two_drudges_needing_fire_vuln(), debuff_each_of_a_priority());
    first.toggle();
    first.tick(0.25);

    let mut second = Fight::new(two_drudges_needing_fire_vuln(), debuff_each_of_a_priority());
    start_listening(&mut second);
    second.surface.peers.casts = heard_attempts(&first.surface, 0x5000_0002);
    second.observe_remote_casts();
    second.tick(0.25);
    assert_eq!(second.surface.last_targeted_cast, (70, 11));

    let sequence = i64::try_from(second.surface.peers.casts.len()).unwrap() + 1;
    second.surface.peers.casts.push(PeerCast {
        sequence,
        target_id: 10,
        ..peer_cast(sequence, 70)
    });
    second.observe_remote_casts();
    second.tick(0.25);
    second.tick(0.25);
    assert_eq!(second.controller.target_id(), 11);
    assert_eq!(second.surface.last_targeted_cast, (70, 11));
}

/// A debuffer debuffing each monster of a priority first moves on to the
/// next monster once its own debuff is up on the first.
#[test]
fn a_debuffer_moves_on_to_the_next_monster_after_its_own_debuff_lands() {
    let mut fight = Fight::new(two_drudges_needing_fire_vuln(), debuff_each_of_a_priority());
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (70, 10));

    fight.surface.last_cast_completion = completion(1, 70, 10, 0);
    fight.tick(0.25);
    fight.tick(0.25);

    assert_eq!(fight.surface.last_targeted_cast, (70, 11));
}

/// A bot alone, its own debuff up on its monster, yields to a
/// higher-priority arrival exactly as before focus existed.
#[test]
fn a_bot_alone_yields_to_a_higher_priority_arrival_whatever_it_debuffed() {
    let mut settings = fire_vulnerability_rule();
    settings.rules.insert(
        0,
        rule_with(
            "name#^Olthoi",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                damage_type: MonsterDamageType::Fire,
                priority: 4,
                ..MonsterRuleActions::default()
            },
        ),
    );
    let mut fight = Fight::new(bolt_and_fire_vuln(), settings);
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_targeted_cast, (70, 10));
    fight.surface.last_cast_completion = completion(1, 70, 10, 0);
    fight.tick(0.25);
    fight.tick(0.25);
    assert_eq!(fight.controller.target_id(), 10);

    fight.surface.targets = vec![
        target(10, "Drudge", 5.0, 0.0),
        target(30, "Olthoi Soldier", 15.0, 0.0),
    ];
    fight.tick(0.25);

    assert_eq!(fight.controller.target_id(), 30);
}

/// The list of the team's casts is read again from its start when the
/// macro starts (or combat comes back): the landed casts in that backlog
/// are taken in, its attempts are old and claim nothing; an attempt heard
/// after it does.
#[test]
fn a_backlog_read_after_a_restart_takes_in_landed_casts_but_not_attempts() {
    let mut surface = bolt_and_fire_vuln();
    surface.targets.push(target(11, "Drudge", 8.0, 0.0));
    surface.peers.casts.push(PeerCast {
        landed: false,
        seconds_remaining: 0.0,
        ..peer_cast(1, 70)
    });
    surface.peers.casts.push(PeerCast {
        target_id: 11,
        ..peer_cast(2, 70)
    });
    let mut fight = Fight::new(surface, fire_vulnerability_rule());

    fight.toggle();
    fight.observe_remote_casts();
    assert!(fight.controller.team_claims().is_empty());
    assert_eq!(fight.controller.team_debuffs().len(), 1);

    fight.surface.peers.casts.push(PeerCast {
        landed: false,
        seconds_remaining: 0.0,
        target_id: 11,
        ..peer_cast(3, 70)
    });
    fight.observe_remote_casts();
    assert_eq!(fight.controller.team_claims().len(), 1);
}
