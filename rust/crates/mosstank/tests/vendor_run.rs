//! The vendor run in the shell (ported from the C#'s `MossTankPanelTests`):
//! the `/ub vendor` and `/ub autovendor` commands reach it, it wakes when a
//! counter opens, owns the character for the visit, and holds the route at
//! the counter until it stops.

mod common;

use ac_plugin_abi::combat::{CombatMode, CombatSnapshot};
use ac_plugin_abi::items::{EquipmentItem, EquipmentPlacement, InventoryItem};
use ac_plugin_abi::magic::CastResult;
use ac_plugin_abi::navigation::{NavigationObject, NavigationSnapshot};
use ac_plugin_abi::trade::{
    NO_VALUE_LIMIT, VendorEvent, VendorItem, VendorProfile, VendorResult, VendorState,
    VendorStatus, VendorTransaction, VendorTransactionKind,
};
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use ac_plugin_abi::{Event, calls};
use common::World;
use mosstank::MossTank;
use mosstank_automation::locks::ActionLockKind;
use mosstank_formats::route::{
    NavigationSettings, RouteMode, RouteRecallKind, RouteWaypoint, RouteWaypointType,
};
use mosstank_formats::utl::{self, LootAction, LootRule};

/// The counter's id (`FrameVendorSurface.VendorId`).
const VENDOR_ID: u32 = 0x7C95_B01C;

/// What a frame lasts in the C# tests.
const FRAME_SECONDS: f64 = 0.05;

/// "Acdream" on Coldeve with ten free slots in the main pack, as the C#
/// panel tests' fake has them (`FakeAutomation.MainPackFreeSlots`).
fn shop_world() -> World {
    let mut world = World::with_character("Acdream");
    if let Some(character) = world.fake.character.as_mut() {
        character.main_pack_free_slots = 10;
    }
    world
}

fn frames(world: &mut World, plugin: &mut MossTank, count: usize) {
    for _ in 0..count {
        world.with_tank(plugin, |tank, client| tank.on_tick(client, FRAME_SECONDS));
    }
}

fn locked(plugin: &MossTank, kind: ActionLockKind) -> bool {
    plugin.tank().expect("enabled").is_action_locked(kind)
}

fn sent() -> VendorResult {
    VendorResult {
        status: VendorStatus::Sent,
        notice: None,
    }
}

/// The counter's profile (`FrameVendorSurface.Profile`): pays 75%, deals
/// in weapons and food.
fn shop_profile() -> VendorProfile {
    VendorProfile {
        buy_rate: 0.75,
        deals_in_item_types: 0x1 | 0x20,
        minimum_value: NO_VALUE_LIMIT,
        maximum_value: NO_VALUE_LIMIT,
        deals_in_magical_items: true,
        alternate_currency_weenie_class_id: 0,
        alternate_currency_amount: 0,
        alternate_currency_name: None,
    }
}

/// The counter open (or closed) with `listings` (`FrameVendorSurface`).
fn counter(world: &mut World, open: bool, listings: Vec<VendorItem>) {
    world.set_reply::<calls::VendorState>(&VendorState {
        is_available: true,
        is_open: open,
        vendor_id: if open { VENDOR_ID } else { 0 },
        vendor_name: if open { "Shopkeeper" } else { "" }.to_owned(),
        items: listings,
        profile: shop_profile(),
        ..VendorState::default()
    });
}

/// Every staging and commit call answers that it was sent, but selling,
/// which finds nothing staged.
fn counter_takes_commands(world: &mut World) {
    world.set_reply::<calls::VendorAddBuy>(&sent());
    world.set_reply::<calls::VendorAddSell>(&sent());
    world.set_reply::<calls::VendorClearBuy>(&sent());
    world.set_reply::<calls::VendorClearSell>(&sent());
    world.set_reply::<calls::VendorBuyAll>(&sent());
    world.set_reply::<calls::VendorSellAll>(&VendorResult {
        status: VendorStatus::InvalidItem,
        notice: None,
    });
}

fn shopkeeper_profile(world: &mut World, rules: &[LootRule]) {
    world.profiles.insert(
        "mosstank/ub/autovendor/Shopkeeper.utl",
        &utl::serialize_rules(rules),
    );
}

fn rule(name: &str, expression: &str, action: LootAction, keep_count: i32) -> LootRule {
    let mut rule = LootRule::named(name, expression);
    rule.action = action;
    rule.keep_count = keep_count;
    rule
}

fn open(world: &mut World, plugin: &mut MossTank, listings: Vec<VendorItem>) {
    counter(world, true, listings);
    world.send(plugin, Event::Vendor(VendorEvent::Opened(VENDOR_ID)));
}

fn close(world: &mut World, plugin: &mut MossTank) {
    counter(world, false, Vec::new());
    world.send(plugin, Event::Vendor(VendorEvent::Closed));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: VendorCommandsAreRoutedToTheVendorRun
#[test]
fn vendor_commands_are_routed_to_the_vendor_run() {
    let mut world = shop_world();
    let mut plugin = world.enable();
    world.clear_chat();

    world.command(&mut plugin, "ub", "autovendor");
    world.command(&mut plugin, "ub", "vendor addbuy 5 Ration");
    world.command(&mut plugin, "ub", "vendor opencancel");
    world.command(&mut plugin, "ub", "autovendor stop");

    assert_eq!(
        world.chat(),
        [
            "[UB] AutoVendor Fatal - no vendor, cannot start",
            "[UB] Name: Ration count: 5 param:5 Ration",
            "[UB] Error: addbuy: No vendor open",
            "[UB] AutoVendor finished: ",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ARunningVendorRunHoldsTheNavigationLockThroughTheFramePass
#[test]
fn a_running_vendor_run_holds_the_navigation_lock_through_the_frame_pass() {
    let mut world = shop_world();
    shopkeeper_profile(&mut world, &[rule("rest", "*", LootAction::NoLoot, 0)]);
    counter(&mut world, false, Vec::new());
    counter_takes_commands(&mut world);
    let mut plugin = world.enable();
    world.command(&mut plugin, "ub", "opt set AutoVendor.Enabled true");

    open(&mut world, &mut plugin, Vec::new());
    frames(&mut world, &mut plugin, 1);

    // The run took the character on the frame pass that started it, so the
    // route stands still instead of walking to the next waypoint.
    assert!(locked(&plugin, ActionLockKind::Navigation));

    close(&mut world, &mut plugin);
    frames(&mut world, &mut plugin, 1);

    assert!(!locked(&plugin, ActionLockKind::Navigation));
}

/// Standing at the counter (`NavigationAt(0f)`).
fn here() -> Position {
    Position {
        cell_id: 0x0001_0001,
        is_outdoor: true,
        ..Position::default()
    }
}

/// The live route: the vendor, a two-second pause, the recall
/// (`ShopRoute`).
fn shop_route(at: &Position) -> NavigationSettings {
    NavigationSettings {
        enabled: true,
        mode: RouteMode::Circular,
        waypoints: vec![
            RouteWaypoint {
                kind: RouteWaypointType::OpenVendor,
                position: at.clone(),
                object_id: VENDOR_ID,
                object_name: "Shopkeeper".to_owned(),
                ..RouteWaypoint::default()
            },
            RouteWaypoint {
                kind: RouteWaypointType::Pause,
                position: at.clone(),
                duration_milliseconds: 2000,
                ..RouteWaypoint::default()
            },
            RouteWaypoint {
                kind: RouteWaypointType::Recall,
                position: at.clone(),
                recall: RouteRecallKind::LifestoneRecall,
                recall_spell_id: 1635,
                recall_spell_name: "Lifestone Recall".to_owned(),
                ..RouteWaypoint::default()
            },
        ],
        ..NavigationSettings::default()
    }
}

/// Pyreals in the pack (`ShopCoin`).
fn shop_coin(amount: i32) -> InventoryItem {
    InventoryItem {
        object_id: 30,
        weenie_class_id: 273,
        name: "Pyreal".to_owned(),
        item_type: 0x10,
        container_id: 1,
        stack_size: amount,
        value: amount,
        object_class: ObjectClass::Money,
        maximum_stack_size: 25000,
        ..InventoryItem::default()
    }
}

/// Rations in the pack (`ShopRation`).
fn shop_ration(id: u32, stack: i32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: id + 2000,
        name: "Ration".to_owned(),
        item_type: 0x20,
        container_id: 1,
        stack_size: stack,
        value: 5 * stack,
        maximum_stack_size: 25,
        object_class: ObjectClass::Food,
        ..InventoryItem::default()
    }
}

fn casts(world: &World) -> Vec<u32> {
    world
        .requests::<calls::MagicCast>()
        .into_iter()
        .map(|request| request.spell_id)
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ARunningVendorRunHoldsTheRouteAtTheCounterUntilItStops
#[test]
fn a_running_vendor_run_holds_the_route_at_the_counter_until_it_stops() {
    let ration = VendorItem {
        template_id: 9001,
        weenie_class_id: 5000,
        name: "Ration".to_owned(),
        object_class: ObjectClass::Food,
        unit_price: 5,
        stack_size: 1,
        max_stack_size: 25,
        item_type: 0x20,
    };
    let mut world = shop_world();
    let at = here();
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: at.clone(),
        ..NavigationSnapshot::default()
    });
    world.set_reply::<calls::NavigationObject>(&Some(NavigationObject {
        object_id: VENDOR_ID,
        name: "Shopkeeper".to_owned(),
        position: at.clone(),
        ..NavigationObject::default()
    }));
    world.set_reply::<calls::ItemsOwned>(&vec![shop_coin(1000)]);
    world.set_reply::<calls::MagicCast>(&CastResult::Sent);
    // The C# panel tests' fake stands in magic mode and has no equipment
    // surface, so a recall needs no wand; the SDK always has one, so the
    // character here wields a wand, which keeps the magic mode it is in.
    wield_wand(&mut world);
    world.set_reply::<calls::CombatSnapshot>(&CombatSnapshot {
        mode: CombatMode::Magic,
        ..CombatSnapshot::default()
    });
    counter(&mut world, false, Vec::new());
    counter_takes_commands(&mut world);
    shopkeeper_profile(
        &mut world,
        &[
            rule("rations", "name ~= ration", LootAction::KeepUpTo, 10),
            rule("rest", "*", LootAction::NoLoot, 0),
        ],
    );
    let route = mosstank_formats::nav::save(&shop_route(&at)).unwrap();
    world.profiles.insert("mosstank/navs/Shop.nav", &route);

    let mut plugin = world.enable();
    world.command(&mut plugin, "ub", "opt set AutoVendor.Enabled true");
    world.vt(&mut plugin, "nav load Shop");
    world.vt(&mut plugin, "opt set EnableNav true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });

    // The route is walking its vendor waypoint: its rule claims the pass
    // and arms the mover, which is the state the live route was in when the
    // window came up.
    frames(&mut world, &mut plugin, 4);
    assert_eq!(0, plugin.tank().unwrap().route_waypoint_index());

    open(&mut world, &mut plugin, vec![ration]);

    // Eight seconds of ordinary frames: four times the two-second pause, so
    // a route that is free to run reaches the recall many times over.
    for frame in 0..160 {
        frames(&mut world, &mut plugin, 1);
        assert!(
            locked(&plugin, ActionLockKind::Navigation),
            "the run holds navigation for every frame it is running ({frame})"
        );
        assert!(
            plugin.tank().unwrap().route_waypoint_index() <= 1,
            "the route passed the pause at frame {frame}"
        );
        assert!(casts(&world).is_empty());
    }

    // The buy is staged and waiting on the counter's answer, which is where
    // the live run was when the recall went out under it.
    assert!(!world.requests::<calls::VendorBuyAll>().is_empty());

    // The counter answers; the run has nothing left to do and stops.
    world.set_reply::<calls::ItemsOwned>(&vec![shop_coin(950), shop_ration(40, 10)]);
    world.send(
        &mut plugin,
        Event::Vendor(VendorEvent::TransactionCompleted(VendorTransaction {
            kind: VendorTransactionKind::Buy,
            success: true,
            notice: None,
        })),
    );
    frames(&mut world, &mut plugin, 20);

    assert!(!locked(&plugin, ActionLockKind::Navigation));

    // With the counter done the route picks up where it left off: the pause
    // runs out and the recall is cast.
    frames(&mut world, &mut plugin, 120);

    assert!(casts(&world).contains(&1635));
}

// ---- Not a port of a C# test: what holds the pass --------------------------
//
// The C# has no test for the owner the suspension report names; this one
// checks that a running vendor run is named as the vendor run.

#[test]
fn a_running_vendor_run_is_named_as_what_holds_the_pass() {
    let mut world = shop_world();
    shopkeeper_profile(&mut world, &[rule("rest", "*", LootAction::NoLoot, 0)]);
    counter(&mut world, false, Vec::new());
    counter_takes_commands(&mut world);
    let mut plugin = world.enable();
    world.command(&mut plugin, "ub", "opt set AutoVendor.Enabled true");
    frames(&mut world, &mut plugin, 1);
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 1);

    open(&mut world, &mut plugin, Vec::new());
    frames(&mut world, &mut plugin, 2);

    let lines = plugin.tank().expect("enabled").action_history_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("Pause: a vendor run owns the character")),
        "{lines:?}"
    );
}

/// A wand in hand, the mode it keeps the character in being magic.
fn wield_wand(world: &mut World) {
    const WAND: u32 = 11;
    const WAND_SLOT: u32 = 0x0100_0000;
    world.set_reply::<calls::EquipmentOwned>(&vec![EquipmentItem {
        object_id: WAND,
        name: "War Wand".to_owned(),
        item_type: 0x8000,
        valid_locations: WAND_SLOT,
        equipped_location: WAND_SLOT,
        container_id: 0x5000_0001,
        wielder_id: 0x5000_0001,
        object_class: ObjectClass::WandStaffOrb,
        ..EquipmentItem::default()
    }]);
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: WAND,
        name: "War Wand".to_owned(),
        object_class: ObjectClass::WandStaffOrb,
        is_owned: true,
        ..WorldObject::default()
    }));
    world.set_reply::<calls::EquipmentPlacements>(&vec![EquipmentPlacement {
        object_id: WAND,
        equipped_location: WAND_SLOT,
    }]);
}
