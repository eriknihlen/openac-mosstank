//! The frame pass between heartbeats: the answers a waiting pass is
//! waiting for (the item channel going free, a corpse opening, its
//! contents arriving, it shutting) run the next pass on the frame they
//! land, and the slots a corpse open took are given back on the frame
//! (ported from the C#'s `MossTankPanelTests`, over its `FakeAutomation`
//! and `FrameLootSurface`).

mod common;

use ac_plugin_abi::items::LootContainer;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::automation::FakeAutomation;
use common::{FrameLootSurface, World};
use mosstank::MossTank;
use mosstank_automation::locks::ActionLockKind;

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn passes(plugin: &MossTank) -> i64 {
    plugin.tank().unwrap().pass_count()
}

fn locked(plugin: &MossTank, kind: ActionLockKind) -> bool {
    plugin
        .tank()
        .unwrap()
        .action_locks()
        .borrow_mut()
        .is_locked(kind)
}

/// `panel.AddLootRule(); if (!panel.LootEnabled) panel.ToggleLooting();`
fn loot_everything(world: &mut World, plugin: &mut MossTank) {
    world.act(plugin, "AddLootRule");
    if !world.ui_bool(plugin, "LootEnabled") {
        world.act(plugin, "ToggleLooting");
    }
}

/// `new PluginLootContainer(CorpseId, 1u, "Corpse", distance, false, false, false)`.
fn corpse(distance: f32) -> LootContainer {
    LootContainer {
        object_id: FrameLootSurface::CORPSE_ID,
        weenie_class_id: 1,
        name: "Corpse".to_owned(),
        distance,
        ..LootContainer::default()
    }
}

/// The character's own identified corpse, three metres off.
fn own_corpse(world: &mut World) -> LootContainer {
    LootContainer {
        is_identified: true,
        long_description: format!("Killed by {}.", world.automation().name),
        ..corpse(3.0)
    }
}

/// `new FakeAutomation { LootSurface = loot }`, enabled.
fn with_frame_loot() -> (World, MossTank) {
    let mut world = World::with_automation(FakeAutomation::default());
    world.loot = Some(FrameLootSurface::default());
    let plugin = world.enable();
    (world, plugin)
}

/// Ticks until the corpse is asked open, twelve at most.
fn until_opened(world: &mut World, plugin: &mut MossTank) {
    for _ in 0..12 {
        if world.loot().opened != 0 {
            break;
        }
        tick(world, plugin, 0.3);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheFramePassPokesWhenTheItemChannelGoesFree
/// The item channel going free is a receipt: the pass waiting behind it
/// runs on that frame rather than at the next heartbeat.
#[test]
fn the_frame_pass_pokes_when_the_item_channel_goes_free() {
    let mut automation = FakeAutomation::default();
    automation.items_busy = true;
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();

    world.act(&mut plugin, "ToggleCombat");
    for _ in 0..4 {
        tick(&mut world, &mut plugin, 0.3);
    }
    let settled = passes(&plugin);

    // A frame well inside the heartbeat, on its own, is not a pass.
    tick(&mut world, &mut plugin, 0.01);
    assert_eq!(passes(&plugin), settled);

    world.automation().items_busy = false;
    tick(&mut world, &mut plugin, 0.01);

    assert_eq!(passes(&plugin), settled + 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheFramePassGivesBackTheSlotsACorpseOpenTook
/// The slots a corpse open takes are given back on the host's frame, not
/// on a rule pass: the item slot is what stops the loot rules running.
#[test]
fn the_frame_pass_gives_back_the_slots_a_corpse_open_took() {
    let (mut world, mut plugin) = with_frame_loot();
    loot_everything(&mut world, &mut plugin);
    let corpse = own_corpse(&mut world);
    world.loot().corpses = vec![corpse];

    world.act(&mut plugin, "ToggleCombat");
    until_opened(&mut world, &mut plugin);

    assert_eq!(world.loot().opened, FrameLootSurface::CORPSE_ID);
    assert!(locked(&plugin, ActionLockKind::ItemUse));
    assert!(locked(&plugin, ActionLockKind::Navigation));

    // The container is open now, which is the moment the slots go back.
    world.loot().current = FrameLootSurface::CORPSE_ID;
    tick(&mut world, &mut plugin, 0.05);

    assert!(!locked(&plugin, ActionLockKind::ItemUse));
    assert!(!locked(&plugin, ActionLockKind::Navigation));
    assert!(!locked(&plugin, ActionLockKind::CorpseOpenAttempt));
}

/// A navigation snapshot `north_south` map units north, outdoors.
fn standing_at(north_south: f64) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x0001_0001,
            north_south,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnUndescribedCorpseWithinReachHoldsEveryWalkOffForThePass
/// With looting on, a corpse whose description has not arrived, within
/// the approach range plus ten, holds every walk off for the pass, the
/// route included, and the loot channel says so. Once it is described and
/// turns out not to be ours, the walks are free again.
#[test]
fn an_undescribed_corpse_within_reach_holds_every_walk_off_for_the_pass() {
    let mut automation = FakeAutomation::default();
    automation.navigation = standing_at(0.0);
    let mut world = World::with_automation(automation);
    world.loot = Some(FrameLootSurface::default());
    let mut plugin = world.enable();
    loot_everything(&mut world, &mut plugin);
    // One waypoint the character is nowhere near, so the route rule has
    // something to do on every pass.
    world.automation().navigation = standing_at(1.0);
    world.act(&mut plugin, "AddRoutePoint");
    world.automation().navigation = standing_at(0.0);
    world.act(&mut plugin, "ToggleNavigation");
    world.loot().corpses = vec![corpse(8.0)];
    world.vt(&mut plugin, "log ActiveRule on");
    world.vt(&mut plugin, "log Loot on");
    world.clear_chat();
    world.act(&mut plugin, "ToggleCombat");

    tick(&mut world, &mut plugin, 0.3);
    tick(&mut world, &mut plugin, 0.3);

    let chat = world.chat();
    let holding = "[MossTank] Holding this tick until the corpse id arrives.";
    assert!(chat.iter().any(|line| line == holding), "{chat:?}");
    assert!(
        !chat
            .iter()
            .any(|line| line.contains("Picked NavigateRouteIdle")),
        "{chat:?}"
    );
    assert!(
        !chat
            .iter()
            .any(|line| line.contains("Picked NavigateCorpseIdle")),
        "{chat:?}"
    );

    // Described now, and somebody else's kill: nothing to loot, nothing
    // to wait for, and the route walks on.
    let described = LootContainer {
        is_identified: true,
        long_description: "Killed by Stranger.".to_owned(),
        ..world.loot().corpses[0].clone()
    };
    world.loot().corpses = vec![described];
    world.clear_chat();
    tick(&mut world, &mut plugin, 0.3);
    tick(&mut world, &mut plugin, 0.3);

    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.contains("Picked NavigateRouteIdle")),
        "{chat:?}"
    );
    assert!(!chat.iter().any(|line| line == holding), "{chat:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheOpenRuleHoldsThePassWhileItsOwnSlotIsUp
/// The open rule is valid while the item slot is held: the pass after
/// the open is still the open rule's, with the slot up.
#[test]
fn the_open_rule_holds_the_pass_while_its_own_slot_is_up() {
    let (mut world, mut plugin) = with_frame_loot();
    loot_everything(&mut world, &mut plugin);
    let corpse = own_corpse(&mut world);
    world.loot().corpses = vec![corpse];
    world.vt(&mut plugin, "log ActiveRule on");
    world.act(&mut plugin, "ToggleCombat");
    until_opened(&mut world, &mut plugin);
    assert_eq!(world.loot().opened, FrameLootSurface::CORPSE_ID);
    assert!(locked(&plugin, ActionLockKind::ItemUse));

    // The container has not opened; the slot is still up.
    world.clear_chat();
    tick(&mut world, &mut plugin, 0.3);

    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.contains("Picked OpenCorpseIdle") && line.contains("I=True")),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ACorpseReceiptRunsTheNextPassOnItsOwnFrame
/// The container opening and shutting, and its contents arriving, each
/// run the next pass on the frame they land.
#[test]
fn a_corpse_receipt_runs_the_next_pass_on_its_own_frame() {
    let (mut world, mut plugin) = with_frame_loot();
    loot_everything(&mut world, &mut plugin);
    world.act(&mut plugin, "ToggleCombat");
    // Settle: nothing is changing, so nothing pokes and the pass runs on
    // the heartbeat alone.
    for _ in 0..4 {
        tick(&mut world, &mut plugin, 0.3);
    }
    let settled = passes(&plugin);
    tick(&mut world, &mut plugin, 0.01);
    assert_eq!(passes(&plugin), settled);

    // The container opened.
    world.loot().current = FrameLootSurface::CORPSE_ID;
    tick(&mut world, &mut plugin, 0.01);
    assert_eq!(passes(&plugin), settled + 1);

    // Its contents arrived.
    world.loot().contents_ready = false;
    tick(&mut world, &mut plugin, 0.3);
    let before_contents = passes(&plugin);
    world.loot().contents_ready = true;
    tick(&mut world, &mut plugin, 0.01);
    assert_eq!(passes(&plugin), before_contents + 1);

    // The container shut.
    tick(&mut world, &mut plugin, 0.3);
    let before_close = passes(&plugin);
    world.loot().current = 0;
    tick(&mut world, &mut plugin, 0.01);
    assert_eq!(passes(&plugin), before_close + 1);
}

/// `NavigationAt(0f)` moved to `east_west` map units.
fn standing_east(east_west: f64) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x0001_0001,
            east_west,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheRouteMoverSteersOnHostFramesBetweenSchedulerPasses
/// The rule pass arms the route mover; the host's frames drive it. A
/// mover that only steered on the pass would overshoot every turn.
#[test]
fn the_route_mover_steers_on_host_frames_between_scheduler_passes() {
    let mut automation = FakeAutomation::default();
    automation.navigation = standing_east(0.0);
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();

    world.act(&mut plugin, "AddRoutePoint");
    // Twelve metres west of the waypoint, so the mover has a real leg.
    world.automation().navigation = standing_east(0.05);
    world.act(&mut plugin, "ToggleNavigation");
    assert!(world.ui_bool(&mut plugin, "NavigationEnabled"));
    world.act(&mut plugin, "ToggleCombat");

    // The first frame carries the pass that arms the mover.
    tick(&mut world, &mut plugin, 0.05);
    assert!(!world.automation().movement_intents.is_empty());
    let after_arming = world.automation().movement_intents.len();

    // Four more frames, none of which carries a pass: the scheduler's
    // heartbeat is 0.293 s and only 0.2 s of it has gone by.
    for _ in 0..4 {
        tick(&mut world, &mut plugin, 0.05);
    }

    assert_eq!(world.automation().movement_intents.len(), after_arming + 4);
}
