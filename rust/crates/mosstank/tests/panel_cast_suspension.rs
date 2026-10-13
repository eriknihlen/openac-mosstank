//! The hold on the pass while a cast, a use or a swing is in flight, and
//! what the rules below it may do meanwhile: the stance changes, the
//! swings, the wand's own slot and the route (ported from the C#'s
//! `MossTankPanelTests`, over its `CombatCapableFakeAutomation`).

mod common;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use common::World;
use common::automation::{
    FakeAutomation, SPELL_RESULT_TEXT_TYPE, combat_target, equipment_item, named_spell,
    navigation_at, skill,
};
use mosstank::MossTank;
use mosstank_automation::cast_tracker::CastRequest;
use mosstank_automation::locks::ActionLockKind;
use mosstank_expr::ExpressionValue;
use mosstank_meta::scheduler::MacroPassContext;

/// `new CombatCapableFakeAutomation { full vitals }`.
fn rig() -> FakeAutomation {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    automation.stamina = (100, 100);
    automation.mana = (100, 100);
    automation
}

fn open(automation: FakeAutomation) -> (World, MossTank) {
    let mut world = World::with_automation(automation);
    let plugin = world.enable();
    (world, plugin)
}

fn frames(world: &mut World, plugin: &mut MossTank, count: usize, seconds: f64) {
    for _ in 0..count {
        world.frame(plugin, seconds);
    }
}

fn call_log(world: &mut World) -> Vec<String> {
    world.automation().call_log.clone()
}

fn index_of(log: &[String], entry: &str) -> Option<usize> {
    log.iter().position(|line| line == entry)
}

fn item_use_locked(plugin: &MossTank) -> bool {
    plugin
        .tank()
        .unwrap()
        .action_locks()
        .borrow_mut()
        .is_locked(ActionLockKind::ItemUse)
}

/// `((IBuffRuleHost)panel).CastTracker.Begin(1, "Strength Self", 0, "", false)`:
/// a cast this macro issued and is still waiting on.
fn begin_strength_self(plugin: &MossTank) {
    plugin
        .tank()
        .unwrap()
        .cast_tracker()
        .borrow_mut()
        .begin(&CastRequest::new(1, "Strength Self", 0, "", false));
}

fn set_option(world: &mut World, plugin: &mut MossTank, name: &str, value: ExpressionValue) {
    world
        .with_tank(plugin, |tank, client| {
            tank.set_meta_option(client, name, &value)
        })
        .unwrap();
}

fn add_selected(world: &mut World, plugin: &mut MossTank, object_id: u32) {
    world.select_object(object_id);
    world.act(plugin, "AddSelectedItem");
}

/// The rig's position: the origin of cell 0x00010001, `north_south` map
/// units north.
fn stand_at(automation: &mut FakeAutomation, north_south: f64) {
    let position = Position {
        cell_id: 0x0001_0001,
        east_west: 0.0,
        north_south,
        elevation: 0.0,
        heading_degrees: 0.0,
        is_outdoor: true,
    };
    automation.navigation.position = position.clone();
    automation.navigation.confirmed_position = position;
}

/// A one-point route `north_south` map units north of the character, with
/// navigation on.
fn route_ahead(world: &mut World, plugin: &mut MossTank, north_south: f64) {
    stand_at(world.automation(), north_south);
    world.act(plugin, "AddRoutePoint");
    stand_at(world.automation(), 0.0);
    world.act(plugin, "ToggleNavigation");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MacroWieldsCasterEntersMagicBuffsThenWieldsWeaponFightsThenIdlePeace
#[test]
fn macro_wields_caster_enters_magic_buffs_then_wields_weapon_fights_then_idle_peace() {
    let mut automation = rig();
    automation.skills = vec![skill(1, "Life Magic", 300)];
    automation.set_known_self_buffs(vec![common::spell(
        1,
        10,
        "Increases the caster's Life Magic skill by 10 points.",
    )]);
    automation.item_entries = vec![
        common::item(10, "War Wand", 0x0000_8000),
        common::item(20, "Battle Axe", 1),
    ];
    automation.equipment = vec![
        equipment_item(10, "War Wand", 0x0000_8000),
        equipment_item(20, "Battle Axe", 1),
    ];
    automation.hostile_targets = vec![combat_target(30, "Drudge", 700, 2.0)];
    let (mut world, mut plugin) = open(automation);

    add_selected(&mut world, &mut plugin, 10); // Items profile: the wand
    add_selected(&mut world, &mut plugin, 20); // Items profile: the axe
    world.act_int(&mut plugin, "CycleMonsterWeaponAt", 0);
    world.act(&mut plugin, "ToggleIdlePeaceMode");

    world.act(&mut plugin, "ToggleCombat"); // Run Macro, starting in Peace

    let mut attacked = false;
    for _ in 0..60 {
        if attacked {
            break;
        }
        world.frame(&mut plugin, 0.7);
        attacked = world.automation().begin_count > 0;
    }
    let log = call_log(&mut world);
    assert!(attacked, "Never attacked. CallLog: {}", log.join(" | "));

    let equip_wand = index_of(&log, "Equip:0000000A");
    let enter_magic = index_of(&log, "EnterMode:Magic");
    let cast = index_of(&log, "Cast:1");
    let equip_weapon = index_of(&log, "Equip:00000014");
    let default_mode = index_of(&log, "EnterMode:Melee");
    let attack = index_of(&log, "Attack:0000001E");
    let joined = log.join(" | ");

    // Peace (already), then the wand: no separate peace request was needed
    // for the wand because the macro started in peace already.
    let equip_wand = equip_wand.expect("wand was never equipped");
    assert!(
        !log[..equip_wand]
            .iter()
            .any(|line| line == "EnterMode:Peace"),
        "{joined}"
    );
    assert!(
        enter_magic > Some(equip_wand),
        "Magic requested before the wand was wielded: {joined}"
    );
    assert!(
        cast > enter_magic,
        "cast happened before Magic mode was entered: {joined}"
    );
    assert!(
        equip_weapon > cast,
        "the weapon was wielded before the buff was cast: {joined}"
    );
    assert!(
        default_mode > equip_weapon,
        "combat mode entered before the weapon was equipped: {joined}"
    );
    assert!(
        !log.iter().any(|line| line == "EnterDefaultMode:Melee"),
        "{joined}"
    );
    assert!(
        attack > default_mode,
        "attack began before the default mode was entered: {joined}"
    );

    // With the hostile gone and Peace Mode When Idle on, the macro returns
    // to peace by itself.
    world.automation().hostile_targets.clear();
    for _ in 0..60 {
        if world.automation().combat.mode == CombatMode::Peace {
            break;
        }
        world.frame(&mut plugin, 0.5);
    }
    assert_eq!(world.automation().combat.mode, CombatMode::Peace);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ACastInFlightFreezesTheWholeMainTrackNotJustTheCastingRules
#[test]
fn a_cast_in_flight_freezes_the_whole_main_track_not_just_the_casting_rules() {
    let mut automation = rig();
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.act(&mut plugin, "ToggleCombat");
    begin_strength_self(&plugin);

    frames(&mut world, &mut plugin, 5, 0.3);

    assert!(
        !call_log(&mut world)
            .iter()
            .any(|line| line == "EnterMode:Peace")
    );
    assert_eq!(world.automation().combat.mode, CombatMode::Melee);

    plugin.tank().unwrap().cast_tracker().borrow_mut().reset();
    world.frame(&mut plugin, 0.01);
    world.frame(&mut plugin, 0.01);

    assert!(
        call_log(&mut world)
            .iter()
            .any(|line| line == "EnterMode:Peace")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheHostsInventoryBusyFlagAloneDoesNotSuspendThePass
#[test]
fn the_hosts_inventory_busy_flag_alone_does_not_suspend_the_pass() {
    let mut automation = rig();
    automation.is_casting = true;
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.act(&mut plugin, "ToggleCombat");

    frames(&mut world, &mut plugin, 5, 0.3);

    assert!(
        call_log(&mut world)
            .iter()
            .any(|line| line == "EnterMode:Peace")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AMetaCastHoldsThePassSoPeaceWhenIdleCannotBreakTheWindup
#[test]
fn a_meta_cast_holds_the_pass_so_peace_when_idle_cannot_break_the_windup() {
    let mut automation = rig();
    automation.skills = vec![ac_plugin_abi::character::Skill {
        training: Training::Specialized,
        ..skill(32, "Item Enchantment", 309)
    }];
    let mut wand = equipment_item(10, "Wand", 0x0000_8000);
    wand.valid_locations = 0x0100_0000;
    wand.equipped_location = 0x0100_0000;
    automation.equipment = vec![wand];
    automation.catalog_spells.push(SpellInfo {
        spell_id: 4213,
        name: "Colosseum Recall".to_owned(),
        family: 214,
        tier: 7,
        difficulty: 150,
        mana_cost: 50,
        duration_seconds: 0.0,
        school: 32,
        description: "Sends the caster to the Colosseum.".to_owned(),
        is_self_targeted: true,
        is_beneficial: true,
        saying: "Shurov Thizael".to_owned(),
        ..SpellInfo::default()
    });
    automation.known_spell_ids.insert(4213);
    automation.combat.mode = CombatMode::Magic;
    let mut world = World::with_automation(automation);
    world.profiles.insert(
        "mosstank/metas/Recall.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tExpr {actiontrycastbyid[4213]}\r\n\t\tDO:\tNone\r\n",
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Recall");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.act(&mut plugin, "ToggleCombat");

    for _ in 0..10 {
        if call_log(&mut world).iter().any(|line| line == "Cast:4213") {
            break;
        }
        world.frame(&mut plugin, 0.3);
    }

    let log = call_log(&mut world);
    let cast = index_of(&log, "Cast:4213")
        .unwrap_or_else(|| panic!("The meta never cast. CallLog: {}", log.join(" | ")));
    assert!(!log[cast..].iter().any(|line| line == "EnterMode:Peace"));
    assert!(plugin.tank().unwrap().cast_tracker().borrow().is_busy());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: InsideTheCreepBandTheRouteWalksInsteadOfAskingForPeace
#[test]
fn inside_the_creep_band_the_route_walks_instead_of_asking_for_peace() {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    // A waypoint one metre north: inside the creep band.
    route_ahead(&mut world, &mut plugin, 1.0 / 240.0);
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.act(&mut plugin, "ToggleCombat");

    frames(&mut world, &mut plugin, 4, 0.3);

    assert!(
        !call_log(&mut world)
            .iter()
            .any(|line| line == "EnterMode:Peace")
    );
    assert!(
        plugin
            .tank()
            .unwrap()
            .running_rule_names()
            .iter()
            .any(|name| name == "NavigateRouteIdle")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnOutstandingInventoryRequestDoesNotStopThePass
#[test]
fn an_outstanding_inventory_request_does_not_stop_the_pass() {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    automation.items_busy = true;
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.vt(&mut plugin, "log ActiveRule on");
    world.clear_chat();
    world.act(&mut plugin, "ToggleCombat");

    world.frame(&mut plugin, 0.1);
    world.frame(&mut plugin, 0.3);

    assert!(
        world
            .chat()
            .iter()
            .any(|line| line == "[MossTank] Picked IdlePeace P: 65   I=False, N=False, S=False"),
        "{:?}",
        world.chat()
    );
}

/// A battle axe in the hand's list and on the Items page, the monster's
/// weapon, and a drudge `distance` metres away.
fn axe_against_a_drudge(distance: f32) -> (World, MossTank) {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    automation.item_entries = vec![common::item(20, "Battle Axe", 1)];
    automation.equipment = vec![equipment_item(20, "Battle Axe", 1)];
    automation.hostile_targets = vec![combat_target(30, "Drudge", 700, distance)];
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    add_selected(&mut world, &mut plugin, 20);
    world.act_int(&mut plugin, "CycleMonsterWeaponAt", 0);
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemUseLockHoldsTheAttackRuleOffUntilItExpires
#[test]
fn an_item_use_lock_holds_the_attack_rule_off_until_it_expires() {
    let (mut world, mut plugin) = axe_against_a_drudge(2.0);
    world.act(&mut plugin, "ToggleCombat");

    let mut attacked = false;
    for _ in 0..60 {
        if attacked {
            break;
        }
        world.frame(&mut plugin, 0.7);
        attacked = world.automation().begin_count > 0;
    }
    assert!(
        attacked,
        "the rig never attacks at all. CallLog: {}",
        call_log(&mut world).join(" | ")
    );

    plugin
        .tank()
        .unwrap()
        .action_locks()
        .borrow_mut()
        .arm(ActionLockKind::ItemUse, 5.0);
    let before = world.automation().begin_count;
    frames(&mut world, &mut plugin, 5, 0.7);

    assert_eq!(world.automation().begin_count, before);

    frames(&mut world, &mut plugin, 5, 0.7);

    assert!(
        world.automation().begin_count > before,
        "the attack never resumed after the item-use lock expired"
    );
}

/// `Imperil Other VII` as the C# rig has it.
fn imperil(spell_id: u32) -> SpellInfo {
    SpellInfo {
        spell_id,
        name: "Imperil Other VII".to_owned(),
        family: 1,
        tier: 8,
        difficulty: 350,
        mana_cost: 30,
        duration_seconds: 60.0,
        school: 31,
        description: String::new(),
        is_self_targeted: false,
        is_beneficial: false,
        is_debuff: true,
        is_offensive: true,
        base_range_constant: 80.0,
        ..SpellInfo::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AWandCastIsWatchedToItsEndThoughItsOwnSlotHoldsTheAttackOff
#[test]
fn a_wand_cast_is_watched_to_its_end_though_its_own_slot_holds_the_attack_off() {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    automation.mana = (100, 100);
    let mut lens = common::item(800, "Imperil Lens", 0x8000);
    lens.equipped_location = 0x0010_0000;
    lens.spell_id = 90;
    lens.item_spellcraft = 400;
    automation.item_entries = vec![lens];
    automation.equipment = vec![equipment_item(800, "Imperil Lens", 0x8000)];
    automation.hostile_targets = vec![combat_target(30, "Drudge", 700, 2.0)];
    automation.catalog_spells.push(imperil(90));
    automation.combat.mode = CombatMode::Magic;
    let (mut world, mut plugin) = open(automation);
    add_selected(&mut world, &mut plugin, 800);
    world.act_int(&mut plugin, "ToggleMonsterImperilAt", 0);
    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(false),
    );
    world.act(&mut plugin, "ToggleCombat");

    for _ in 0..40 {
        if world.automation().apply_count > 0 {
            break;
        }
        world.frame(&mut plugin, 0.3);
    }
    assert!(
        world.automation().apply_count > 0,
        "the rig never used the wand at all. CallLog: {}",
        call_log(&mut world).join(" | ")
    );
    assert_eq!(world.automation().last_applied_item, (800, 30));
    assert!(item_use_locked(&plugin));

    // Several passes with the attack's gate shut. Two seconds in, well
    // short of the cast's own window.
    frames(&mut world, &mut plugin, 7, 0.3);
    assert!(item_use_locked(&plugin));

    world.post_captured(
        "You cast Imperil Other VII on Drudge.",
        SPELL_RESULT_TEXT_TYPE,
    );
    world.frame(&mut plugin, 0.3);

    assert!(!item_use_locked(&plugin));
    // The slot came down on the frame, ahead of the pass, so the attack had
    // its turn back on that very pass and is no longer paused.
    assert!(
        !world
            .ui_text(&mut plugin, "CombatStatus")
            .contains("Paused")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AProcChargeIsNotReleasedOnAPassTheAttackLost
#[test]
fn a_proc_charge_is_not_released_on_a_pass_the_attack_lost() {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    automation.mana = (100, 100);
    let mut sword = common::item(801, "Imperil Sword", 0x0001);
    sword.equipped_location = 0x0010_0000;
    sword.item_spellcraft = 400;
    sword.appraised_spell_ids = vec![91];
    automation.item_entries = vec![sword];
    automation.equipment = vec![equipment_item(801, "Imperil Sword", 0x0001)];
    automation.hostile_targets = vec![combat_target(30, "Drudge", 700, 2.0)];
    automation.catalog_spells.push(imperil(91));
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    add_selected(&mut world, &mut plugin, 801);
    world.act_int(&mut plugin, "ToggleMonsterImperilAt", 0);
    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(false),
    );
    world.act(&mut plugin, "ToggleCombat");

    for _ in 0..40 {
        if world.automation().begin_count > 0 {
            break;
        }
        world.frame(&mut plugin, 0.3);
    }
    assert!(
        world.automation().begin_count > 0,
        "the rig never began the proc's swing at all. CallLog: {}",
        call_log(&mut world).join(" | ")
    );
    assert_eq!(world.automation().last_begin_target, 30);
    // The proc arms nothing: the slot is free until someone else takes it.
    assert!(!item_use_locked(&plugin));
    assert_eq!(world.automation().release_count, 0);

    // Another rule takes the item slot for its own five-second window, and
    // the swing it interrupted is sitting at full power.
    plugin
        .tank()
        .unwrap()
        .action_locks()
        .borrow_mut()
        .arm(ActionLockKind::ItemUse, 5.0);
    let combat = &mut world.automation().combat;
    combat.request_in_progress = true;
    combat.build_in_progress = true;
    combat.power_bar_level = 1.0;
    frames(&mut world, &mut plugin, 8, 0.3);

    assert!(item_use_locked(&plugin));
    assert_eq!(world.automation().release_count, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ANavigationLockHoldsTheRouteRuleOff
#[test]
fn a_navigation_lock_holds_the_route_rule_off() {
    let mut automation = FakeAutomation::combat_capable();
    automation.health = (100, 100);
    let (mut world, mut plugin) = open(automation);
    // One waypoint the character is nowhere near, so the route rule has
    // something to do on every pass.
    route_ahead(&mut world, &mut plugin, 1.0);
    world.act(&mut plugin, "ToggleCombat");
    world.frame(&mut plugin, 0.3);

    let context = MacroPassContext::new(0.3, true);
    let route_valid = |world: &mut World, plugin: &mut MossTank| {
        world
            .with_tank(plugin, |tank, client| {
                tank.rule_valid_now(client, "NavigateRouteIdle", context)
            })
            .expect("the route rule")
    };
    assert!(
        route_valid(&mut world, &mut plugin),
        "the rig's route rule is not valid even with every lock clear"
    );

    let locks = plugin.tank().unwrap().action_locks();
    locks.borrow_mut().arm(ActionLockKind::Navigation, 3.0);
    assert!(!route_valid(&mut world, &mut plugin));

    locks.borrow_mut().release(ActionLockKind::Navigation);
    locks.borrow_mut().arm(ActionLockKind::DoorOpening, 3.0);
    assert!(!route_valid(&mut world, &mut plugin));

    locks.borrow_mut().release(ActionLockKind::DoorOpening);
    locks
        .borrow_mut()
        .arm(ActionLockKind::SpreadLockTargetRequested, 3.0);
    assert!(!route_valid(&mut world, &mut plugin));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AMonsterOutOfWeaponRangeDoesNotStarveTheRulesBelowTheAttack
#[test]
fn a_monster_out_of_weapon_range_does_not_starve_the_rules_below_the_attack() {
    let (mut world, mut plugin) = axe_against_a_drudge(12.0);
    world.act_text(&mut plugin, "SetApproachRangeText", "20");
    // Navigation off, so the monster-approach rule cannot claim the pass
    // either: what runs has to come from below both of them.
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.act(&mut plugin, "ToggleCombat");

    frames(&mut world, &mut plugin, 20, 0.3);

    assert_eq!(world.automation().begin_count, 0);
    assert!(
        call_log(&mut world)
            .iter()
            .any(|line| line == "EnterMode:Peace")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AStuckTransactionReleasesTheSuspensionOnVtanksTrackerWatchdog
#[test]
fn a_stuck_transaction_releases_the_suspension_on_vtanks_tracker_watchdog() {
    let mut automation = rig();
    automation.combat.mode = CombatMode::Melee;
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleIdlePeaceMode");
    world.act(&mut plugin, "ToggleCombat");
    // A cast the server never answers: the tracker's own budget ends it.
    begin_strength_self(&plugin);

    frames(&mut world, &mut plugin, 34, 0.3);

    assert!(
        call_log(&mut world)
            .iter()
            .any(|line| line == "EnterMode:Peace")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RandomHelperBuffsGoToANearbyFellowWithTheSettingOn
#[test]
fn random_helper_buffs_go_to_a_nearby_fellow_with_the_setting_on() {
    let mut automation = rig();
    automation.item_entries = vec![common::item(10, "War Wand", 0x0000_8000)];
    automation.equipment = vec![equipment_item(10, "War Wand", 0x0000_8000)];
    automation.skills = vec![skill(33, "Life Magic", 300)];
    automation.set_known_self_buffs(vec![named_spell(500, 600, "Armor Other I", 33)]);
    automation.combat.mode = CombatMode::Magic;
    automation.world_objects.push(WorldObject {
        object_id: 0x5000_0009,
        weenie_class_id: 123,
        name: "Fellow A".to_owned(),
        object_class: ObjectClass::Player,
        position: Some(navigation_at(0.0).position),
        ..WorldObject::default()
    });
    let (mut world, mut plugin) = open(automation);
    add_selected(&mut world, &mut plugin, 10);
    set_option(
        &mut world,
        &mut plugin,
        "RandomHelperBuffs",
        ExpressionValue::Boolean(true),
    );
    world.act(&mut plugin, "ToggleCombat");

    frames(&mut world, &mut plugin, 200, 0.3);

    assert!(
        world.automation().cast_targets.contains(&0x5000_0009),
        "{:?}",
        call_log(&mut world)
    );
}
