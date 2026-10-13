//! Which hand holds what: signed ids, imported hand-use flags, the off
//! hand's automatic picks, and the peace-when-idle rule
//! (`CombatControllerTests`, sixth part).

mod common;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::EquipmentItem;
use ac_plugin_abi::world::ObjectClass;
use common::{
    FakeAutomation, Fight, equipment, inventory_item, only_rule, peaceful, physical, rule_with,
    single_rule, skill, target,
};
use mosstank_combat::CombatModeGate;
use mosstank_combat::gate::{GateTurn, Prepare};
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::usd::{VtankCell, VtankRow};
use mosstank_meta::host::MetaHost;
use mosstank_meta::scheduler::{
    IdlePeaceContext, IdlePeaceRule, MACRO_RULE_TABLE, MacroPassContext, MacroRule, MacroRuleSlot,
};
use mosstank_meta::testing::FakeHost;
use mosstank_settings::model::{AllSettings, CombatSettings, VitalSettings};
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions, secondary_equip};

/// `DriveEquipPasses`: one equipment step per pass, the gate's clock moved
/// between them; true once the gate reports the character ready.
fn drive_equip_passes(fight: &mut Fight) -> bool {
    for _ in 0..12 {
        if fight.equip_one_step("Fixture") {
            return true;
        }
        fight
            .controller
            .gate()
            .borrow_mut()
            .advance_pass(&mut fight.surface, 0.1);
    }
    false
}

fn fire_row(actions: MonsterRuleActions) -> CombatSettings {
    only_rule(
        CombatSettings::default(),
        rule_with(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                damage_type: MonsterDamageType::Fire,
                ..actions
            },
        ),
    )
}

/// `SecondarySettings`.
fn secondary_settings(primary: u32, secondary: i32) -> CombatSettings {
    let mut settings = fire_row(MonsterRuleActions {
        weapon_object_id: primary,
        secondary_equip_raw: secondary,
        ..MonsterRuleActions::default()
    });
    // A rule may only name a weapon the Items page carries.
    settings.combat_item_object_ids.insert(primary);
    settings
}

/// `SecondarySkills`.
fn secondary_skills(
    shield_trained: bool,
    dual_wield_trained: bool,
) -> Vec<ac_plugin_abi::character::Skill> {
    let mut skills = Vec::new();
    if shield_trained {
        skills.push(skill(48, "Shield", Training::Trained, 300));
    }
    if dual_wield_trained {
        skills.push(skill(49, "Dual Wield", Training::Trained, 300));
    }
    skills
}

fn shield(id: u32) -> EquipmentItem {
    let mut shield = equipment(id, "Shield", 0).valid(0x0020_0000).build();
    shield.object_class = ObjectClass::Armor;
    shield
}

fn secondary_calls(log: &[String]) -> Vec<&String> {
    log.iter()
        .filter(|call| call.starts_with("EquipSecondary:"))
        .collect()
}

fn has(log: &[String], entry: &str) -> bool {
    log.iter().any(|call| call == entry)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SignedMonsterEquipmentIdsReachPrimaryAndOneHandSecondaryDispatch
#[test]
fn signed_monster_equipment_ids_reach_primary_and_one_hand_secondary_dispatch() {
    const PRIMARY: u32 = 0x8001_AC87;
    const SECONDARY: u32 = 0x8001_B291;
    let mut profile = mosstank_settings::defaults::create();
    {
        let monsters = profile
            .find_mut(mosstank_settings::monster_table::TABLE_NAME)
            .unwrap();
        #[allow(clippy::cast_possible_wrap)]
        {
            monsters.rows[0].cells[3] = VtankCell::int(PRIMARY as i32);
            monsters.rows[0].cells[19] = VtankCell::int(SECONDARY as i32);
        }
    }
    let rules = mosstank_settings::monster_table::try_read(&profile, None)
        .unwrap()
        .unwrap();
    assert_eq!(rules.len(), 1);
    let actions = rules[0].actions().clone();
    let mut surface = FakeAutomation {
        combat_snapshot: common::in_mode(CombatMode::Peace),
        equipment_items: vec![
            equipment(PRIMARY, "Primary", 1).build(),
            equipment(SECONDARY, "Secondary", 1).build(),
        ],
        ..FakeAutomation::default()
    };
    let combat = CombatSettings::default();
    let vitals = VitalSettings::default();
    let mut gate = CombatModeGate::new();
    let request = || Prepare {
        override_item_id: actions.weapon_object_id,
        auto_select: false,
        secondary_item_id: Some(actions.offhand_object_id),
        ..Prepare::mode(CombatMode::Melee)
    };

    let mut turn = GateTurn {
        automation: &mut surface,
        combat: &combat,
        vitals: &vitals,
    };
    assert!(!gate.try_prepare(&mut turn, request(), None));
    gate.advance_pass(&mut surface, 0.1);
    let mut turn = GateTurn {
        automation: &mut surface,
        combat: &combat,
        vitals: &vitals,
    };
    assert!(!gate.try_prepare(&mut turn, request(), None));

    assert_eq!(
        surface.call_log,
        ["Equip:8001AC87", "EquipSecondary:8001B291"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AutomaticPrimaryHonorsImportedHandUseFlagsIncludingSignedIds
#[test]
fn automatic_primary_honors_imported_hand_use_flags_including_signed_ids() {
    const SIGNED_SWORD: u32 = 0x8001_AC87;
    // No imported row (-1) defaults to both hands.
    for (imported_uses, expects_equip) in [(0, false), (1, true), (2, false), (3, true), (-1, true)]
    {
        let surface = FakeAutomation {
            combat_snapshot: peaceful(),
            equipment_items: vec![equipment(SIGNED_SWORD, "Signed Fire Sword", 0x0010).build()],
            ..FakeAutomation::default()
        };
        let mut settings = fire_row(MonsterRuleActions::default());
        settings.combat_item_object_ids.insert(SIGNED_SWORD);
        settings.combat_item_order_ids.push(SIGNED_SWORD);
        if imported_uses >= 0 {
            settings
                .item_use_specifiers
                .insert(SIGNED_SWORD, imported_uses);
        }
        let mut fight = Fight::new(surface, settings);

        fight.equip_one_step("Fixture");

        assert_eq!(
            has(&fight.surface.call_log, "Equip:8001AC87"),
            expects_equip,
            "{imported_uses}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: DisabledHeldWeaponIsNotTheAutomaticPrimaryFallback
#[test]
fn disabled_held_weapon_is_not_the_automatic_primary_fallback() {
    const SIGNED_SWORD: u32 = 0x8001_B291;
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        character_skills: vec![skill(33, "Life Magic", Training::Trained, 300)],
        equipment_items: vec![
            equipment(SIGNED_SWORD, "Disabled Held Sword", 0x0010)
                .equipped(0x0010_0000)
                .build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = only_rule(
        CombatSettings {
            maximum_range: 40.0,
            ..CombatSettings::default()
        },
        single_rule(MonsterActionFlags::ATTACK, MonsterDamageType::Auto),
    );
    settings.combat_item_object_ids.insert(SIGNED_SWORD);
    settings.item_use_specifiers.insert(SIGNED_SWORD, 0);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);

    assert!(
        fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("autoselecting drain")),
        "{:?}",
        fight.surface.posted_system_messages
    );
    assert!(!has(&fight.surface.call_log, "Equip:8001B291"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ExplicitEquipmentIdsBypassImportedHandUseFlags
#[test]
fn explicit_equipment_ids_bypass_imported_hand_use_flags() {
    const PRIMARY: u32 = 0x8000_DEB2;
    const SECONDARY: u32 = 0x8001_AC87;
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![
            equipment(PRIMARY, "Explicit Primary", 0x0010).build(),
            equipment(SECONDARY, "Explicit Secondary", 0x0010).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = fire_row(MonsterRuleActions {
        weapon_object_id: PRIMARY,
        offhand_object_id: SECONDARY,
        ..MonsterRuleActions::default()
    });
    settings.combat_item_object_ids.insert(PRIMARY);
    settings.item_use_specifiers.insert(PRIMARY, 0);
    settings.item_use_specifiers.insert(SECONDARY, 0);
    let mut fight = Fight::new(surface, settings);

    drive_equip_passes(&mut fight);

    assert!(has(&fight.surface.call_log, "Equip:8000DEB2"));
    assert!(has(&fight.surface.call_log, "EquipSecondary:8001AC87"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: NamedOffhandSkipsPackItemThatCannotBeHeld
#[test]
fn named_offhand_skips_pack_item_that_cannot_be_held() {
    const PRIMARY: u32 = 700;
    const PACK_ITEM: u32 = 0x8001_B292;
    let mut surface = FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![equipment(PRIMARY, "Primary", 0x0010).build()],
        // A trophy sharing the shield's name: worn nowhere at all.
        item_entries: vec![inventory_item(PACK_ITEM, "Shield of Souls", 1, 0, false)],
        ..FakeAutomation::default()
    };
    surface.world_only_object_ids.insert(PACK_ITEM);
    let mut settings = fire_row(MonsterRuleActions {
        weapon_object_id: PRIMARY,
        offhand_name: "Shield of Souls".to_owned(),
        ..MonsterRuleActions::default()
    });
    settings.combat_item_object_ids.insert(PRIMARY);
    let mut fight = Fight::new(surface, settings);

    assert!(drive_equip_passes(&mut fight));
    assert!(!has(&fight.surface.call_log, "EquipSecondary:8001B292"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: MissingEquipmentViewOffhandRetainsInventoryFallback
#[test]
fn missing_equipment_view_offhand_retains_inventory_fallback() {
    const PRIMARY: u32 = 700;
    const INVENTORY_ONLY_SECONDARY: u32 = 0x8001_B291;
    let mut surface = FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![equipment(PRIMARY, "Primary", 0x0010).build()],
        item_entries: vec![inventory_item(
            INVENTORY_ONLY_SECONDARY,
            "Inventory Offhand",
            1,
            0,
            false,
        )],
        ..FakeAutomation::default()
    };
    surface
        .world_only_object_ids
        .insert(INVENTORY_ONLY_SECONDARY);
    let mut settings = fire_row(MonsterRuleActions {
        weapon_object_id: PRIMARY,
        offhand_object_id: INVENTORY_ONLY_SECONDARY,
        ..MonsterRuleActions::default()
    });
    settings.combat_item_object_ids.insert(PRIMARY);
    let mut fight = Fight::new(surface, settings);

    drive_equip_passes(&mut fight);

    assert!(has(&fight.surface.call_log, "EquipSecondary:8001B291"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AutoWeaponExcludesPrimaryCasterTwoHandAndPrimaryOnlyRows
#[test]
fn auto_weapon_excludes_primary_caster_two_hand_and_primary_only_rows() {
    const PRIMARY: u32 = 100;
    const CASTER: u32 = 200;
    const TWO_HAND: u32 = 300;
    const PRIMARY_ONLY: u32 = 400;
    const LEFT_HAND: u32 = 500;
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![
            equipment(PRIMARY, "Primary", 0x0010).build(),
            equipment(CASTER, "Caster", 0).item_type(0x8000).build(),
            equipment(TWO_HAND, "Two hand", 0x0010)
                .valid(0x0210_0000)
                .build(),
            equipment(PRIMARY_ONLY, "Primary only", 0x0010).build(),
            equipment(LEFT_HAND, "Left hand", 0x0010).build(),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = secondary_settings(PRIMARY, secondary_equip::AUTO_WEAPON);
    for id in [PRIMARY, CASTER, TWO_HAND, PRIMARY_ONLY, LEFT_HAND] {
        settings.combat_item_object_ids.insert(id);
        settings.combat_item_order_ids.push(id);
    }
    settings.item_use_specifiers.insert(PRIMARY_ONLY, 1);
    settings.item_use_specifiers.insert(LEFT_HAND, 2);
    let mut fight = Fight::new(surface, settings);

    drive_equip_passes(&mut fight);

    let log = &fight.surface.call_log;
    assert!(has(log, "EquipSecondary:000001F4"), "{log:?}");
    for absent in [
        "EquipSecondary:00000064",
        "EquipSecondary:000000C8",
        "EquipSecondary:0000012C",
        "EquipSecondary:00000190",
    ] {
        assert!(!has(log, absent), "{absent}: {log:?}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SecondaryEquipModesDispatchByTrainedSkills
#[test]
fn secondary_equip_modes_dispatch_by_trained_skills() {
    const PRIMARY: u32 = 100;
    const WEAPON: u32 = 200;
    const SHIELD: u32 = 300;
    for (mode, shield_trained, dual_wield_trained, expected_secondary) in [
        (secondary_equip::AUTO_SHIELD, false, false, 300),
        (secondary_equip::AUTO_WEAPON, false, false, 200),
        (secondary_equip::NONE, false, false, 0),
        (secondary_equip::AUTO, true, false, 300),
        (secondary_equip::AUTO, false, true, 200),
        (secondary_equip::AUTO, true, true, 300),
        (secondary_equip::AUTO, false, false, 300),
    ] {
        let surface = FakeAutomation {
            combat_snapshot: peaceful(),
            character_skills: secondary_skills(shield_trained, dual_wield_trained),
            equipment_items: vec![
                equipment(PRIMARY, "Primary", 0x0010).build(),
                equipment(WEAPON, "Secondary Weapon", 0x0010).build(),
                shield(SHIELD),
            ],
            ..FakeAutomation::default()
        };
        let mut settings = secondary_settings(PRIMARY, mode);
        settings.combat_item_object_ids.insert(WEAPON);
        settings.combat_item_object_ids.insert(SHIELD);
        settings.combat_item_order_ids.push(WEAPON);
        settings.combat_item_order_ids.push(SHIELD);
        settings.item_use_specifiers.insert(WEAPON, 2);
        let mut fight = Fight::new(surface, settings);

        drive_equip_passes(&mut fight);

        let secondary = secondary_calls(&fight.surface.call_log);
        let case = format!("{mode} {shield_trained} {dual_wield_trained}");
        if expected_secondary == 0 {
            assert!(secondary.is_empty(), "{case}: {secondary:?}");
        } else {
            let wanted = format!("EquipSecondary:{expected_secondary:08X}");
            assert!(
                secondary.iter().any(|call| **call == wanted),
                "{case}: {secondary:?}"
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: LoadingAProfileWithoutHandRulesReleasesTheEarlierRestriction
#[test]
fn loading_a_profile_without_hand_rules_releases_the_earlier_restriction() {
    const SWORD: u32 = 0x8001_AC87;
    let surface = || FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![equipment(SWORD, "Signed Fire Sword", 0x0010).build()],
        ..FakeAutomation::default()
    };
    let mut restricted = mosstank_settings::defaults::create();
    #[allow(clippy::cast_possible_wrap)]
    restricted
        .find_mut("ItemUseSpecifiers")
        .unwrap()
        .rows
        .push(VtankRow {
            cells: vec![VtankCell::int(SWORD as i32), VtankCell::int(0)],
        });
    let mut unrestricted = mosstank_settings::defaults::create();
    unrestricted
        .tables
        .retain(|(name, _)| name != "ItemUseSpecifiers");

    let mut all = AllSettings::default();
    let arm = |all: &mut AllSettings| {
        let settings = &mut all.combat;
        settings.combat_item_object_ids.insert(SWORD);
        settings.combat_item_order_ids.push(SWORD);
        settings.rules.clear();
        settings.rules.push(single_rule(
            MonsterActionFlags::ATTACK,
            MonsterDamageType::Fire,
        ));
    };
    mosstank_settings::profile::load(&restricted.render(), &mut all, None).unwrap();
    arm(&mut all);

    let mut restricted_fight = Fight::new(surface(), all.combat.clone());
    drive_equip_passes(&mut restricted_fight);
    assert!(!has(&restricted_fight.surface.call_log, "Equip:8001AC87"));

    mosstank_settings::profile::load(&unrestricted.render(), &mut all, None).unwrap();
    arm(&mut all);
    // The C# shares one fake between the two fights; the log carries on.
    let mut released = Fight::new(restricted_fight.surface, all.combat.clone());
    drive_equip_passes(&mut released);

    assert!(has(&released.surface.call_log, "Equip:8001AC87"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ShieldSelectionIgnoresWeaponHandRestrictions
#[test]
fn shield_selection_ignores_weapon_hand_restrictions() {
    const PRIMARY: u32 = 100;
    const SHIELD: u32 = 300;
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![
            equipment(PRIMARY, "Primary", 0x0010).build(),
            shield(SHIELD),
        ],
        ..FakeAutomation::default()
    };
    let mut settings = secondary_settings(PRIMARY, secondary_equip::AUTO);
    settings.combat_item_object_ids.insert(SHIELD);
    settings.combat_item_order_ids.push(SHIELD);
    // Right hand only: meaningless for a shield, and it must be ignored.
    settings.item_use_specifiers.insert(SHIELD, 1);
    let mut fight = Fight::new(surface, settings);

    drive_equip_passes(&mut fight);

    assert!(has(&fight.surface.call_log, "EquipSecondary:0000012C"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: CasterSelectionIgnoresWeaponHandRestrictions
#[test]
fn caster_selection_ignores_weapon_hand_restrictions() {
    const WAND: u32 = 990;
    let surface = FakeAutomation {
        combat_snapshot: peaceful(),
        equipment_items: vec![equipment(WAND, "Fixture Wand", 0).item_type(0x8000).build()],
        ..FakeAutomation::default()
    };
    let mut settings = fire_row(MonsterRuleActions {
        weapon_to_use_raw: 0,
        ..MonsterRuleActions::default()
    });
    settings.combat_item_object_ids.insert(WAND);
    settings.combat_item_order_ids.push(WAND);
    settings.item_use_specifiers.insert(WAND, 2);
    let mut fight = Fight::new(surface, settings);

    drive_equip_passes(&mut fight);

    assert!(has(&fight.surface.call_log, "Equip:000003DE"));
    assert!(
        !fight
            .surface
            .posted_system_messages
            .iter()
            .any(|message| message.contains("add at least one wand"))
    );
}

// ---- Peace when idle ----

/// The rule's view of the client: the scheduler's fake host and the
/// profile's switch.
struct Idle {
    host: FakeHost,
    idle_peace_mode: bool,
    requests: usize,
}

impl Idle {
    fn new(mode: CombatMode, idle_peace_mode: bool) -> Self {
        let mut host = FakeHost::default();
        host.combat_mode = mode;
        Self {
            host,
            idle_peace_mode,
            requests: 0,
        }
    }

    /// The C# fake's mode-change count, read off the host's last request.
    fn count_requests(&mut self) -> usize {
        if self.host.requested_mode.take().is_some() {
            self.requests += 1;
        }
        self.requests
    }
}

impl IdlePeaceContext for Idle {
    fn meta_host(&mut self) -> &mut dyn MetaHost {
        &mut self.host
    }

    fn idle_peace_mode(&mut self) -> bool {
        self.idle_peace_mode
    }
}

/// `IdleTurn`.
fn idle_turn(can_act: bool) -> MacroPassContext {
    MacroPassContext::new(0.3, can_act)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: IdlePeaceRequestsOnEveryPassItWins
#[test]
fn idle_peace_requests_on_every_pass_it_wins() {
    // The host ignores the mode change, so the rule stays valid.
    let mut cx = Idle::new(CombatMode::Melee, true);
    let mut rule = IdlePeaceRule::default();

    assert!(rule.valid_now(&mut cx, idle_turn(true)));
    rule.set_running(&mut cx, true);
    assert_eq!(cx.count_requests(), 1);
    assert!(
        rule.status
            .as_deref()
            .unwrap()
            .to_lowercase()
            .contains("peace")
    );

    assert!(rule.valid_now(&mut cx, idle_turn(true)));
    rule.set_running(&mut cx, true);
    assert_eq!(cx.count_requests(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: IdlePeaceIsLastInTheListAndIsInertWhenAskedWithCanActFalse
#[test]
fn idle_peace_is_last_in_the_list_and_is_inert_when_asked_with_can_act_false() {
    let mut cx = Idle::new(CombatMode::Melee, true);
    let mut rule = IdlePeaceRule::default();

    let last = MACRO_RULE_TABLE.len() - 1;
    assert_eq!(
        MACRO_RULE_TABLE[last].slot(),
        Some(MacroRuleSlot::IdlePeace)
    );
    assert!(
        !MACRO_RULE_TABLE[..last]
            .iter()
            .any(|entry| entry.slot() == Some(MacroRuleSlot::IdlePeace))
    );

    assert!(!rule.valid_now(&mut cx, idle_turn(false)));

    rule.set_running(&mut cx, false);
    assert_eq!(cx.count_requests(), 0);
    assert!(rule.status.is_none());
    assert_eq!(cx.host.combat_mode, CombatMode::Melee);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: IdlePeaceModeOffNeverRequests
#[test]
fn idle_peace_mode_off_never_requests() {
    let mut cx = Idle::new(CombatMode::Melee, false);
    let mut rule = IdlePeaceRule::default();

    assert!(!rule.valid_now(&mut cx, idle_turn(true)));
    assert_eq!(cx.count_requests(), 0);
    assert!(rule.status.is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: IdlePeaceIsInvalidWhenAlreadyInPeace
#[test]
fn idle_peace_is_invalid_when_already_in_peace() {
    let mut cx = Idle::new(CombatMode::Peace, true);
    let mut rule = IdlePeaceRule::default();

    assert!(!rule.valid_now(&mut cx, idle_turn(true)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: IdlePeaceStillFiresWhenCombatPolicyIsDisabled
#[test]
fn idle_peace_still_fires_when_combat_policy_is_disabled() {
    let settings = CombatSettings {
        idle_peace_mode: true,
        enabled: false,
        ..CombatSettings::default()
    };
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        ..FakeAutomation::default()
    };
    let mut fight = Fight::new(surface, settings.clone());

    fight.toggle();
    fight.tick(0.25);
    assert!(!fight.has_target());
    assert!(fight.status().to_lowercase().contains("disabled"));

    // The rule reads the same client (here the scheduler's fake, in melee)
    // and the same profile.
    let mut cx = Idle::new(fight.surface.combat_snapshot.mode, settings.idle_peace_mode);
    let mut rule = IdlePeaceRule::default();
    assert!(rule.valid_now(&mut cx, idle_turn(true)));
    rule.set_running(&mut cx, true);

    assert_eq!(cx.host.requested_mode, Some(CombatMode::Peace));
    assert_eq!(cx.count_requests(), 1);
}
