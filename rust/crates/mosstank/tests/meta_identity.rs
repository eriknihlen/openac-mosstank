//! A native meta through the whole shell while the character's identity
//! settles (ported from the one case of the C#'s
//! `AuthenticMetaExecutionTests` that needs the panel), and what the
//! plugin keeps once it is switched off (the cases of `MetaGameEventTests`
//! and `MossTankPluginTests` about letting go of the client).

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::items::EquipmentObservation;
use common::{World, character, proof_fixture};
use mosstank::MossTank;
use mosstank_meta::engine::{DECISION_INTERVAL_SECONDS, DEFAULT_STATE_NAME};
use mosstank_stores::meta_store::BY_CHARACTER;

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn selected_meta(world: &mut World, plugin: &mut MossTank) -> String {
    world.with_tank(plugin, |tank, client| tank.selected_meta_profile(client))
}

fn loot_priority_boost(plugin: &MossTank) -> bool {
    plugin
        .tank()
        .unwrap()
        .option("LootPriorityBoost")
        .is_truthy()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AuthenticMetaExecutionTests.cs :: ControlledNativeMetaIsReselectedAfterIdentitySettlesAndThenRuns
#[test]
fn controlled_native_meta_is_reselected_after_identity_settles_and_then_runs() {
    let mut world = World::with_character("Privileged Meta Tester");
    world.plugin_data.insert(
        "imports/rynthify-controlled-meta.met",
        &proof_fixture("rynthify-controlled-meta.met"),
    );
    let mut plugin = world.enable();
    tick(&mut world, &mut plugin, 0.1);

    world.vt(&mut plugin, "meta load rynthify-controlled-meta.met");
    assert_eq!(
        selected_meta(&mut world, &mut plugin),
        "rynthify-controlled-meta"
    );

    world.fake.character = Some(character("Meta Tester"));
    tick(&mut world, &mut plugin, 0.1);

    assert_eq!(selected_meta(&mut world, &mut plugin), BY_CHARACTER);
    world.vt(&mut plugin, "meta load rynthify-controlled-meta.met");
    world.vt(&mut plugin, "opt set LootPriorityBoost false");
    world.vt(&mut plugin, "opt set EnableMeta true");

    tick(&mut world, &mut plugin, DECISION_INTERVAL_SECONDS + 0.01);
    tick(&mut world, &mut plugin, DECISION_INTERVAL_SECONDS + 0.01);

    assert!(!plugin.tank().unwrap().macro_running());
    assert_eq!(plugin.tank().unwrap().meta_state(), DEFAULT_STATE_NAME);
    assert!(!loot_priority_boost(&plugin));

    world.vt(&mut plugin, "start");
    tick(&mut world, &mut plugin, DECISION_INTERVAL_SECONDS + 0.01);
    tick(&mut world, &mut plugin, DECISION_INTERVAL_SECONDS + 0.01);

    assert!(plugin.tank().unwrap().macro_running());
    assert_eq!(
        selected_meta(&mut world, &mut plugin),
        "rynthify-controlled-meta"
    );
    assert_eq!(plugin.tank().unwrap().meta_state(), "RynthReady");
    assert!(loot_priority_boost(&plugin));
}

/// The C# engine unsubscribes from the client's events when it is
/// disposed. The plugin hands the engine its events itself (register
/// META-3): once the plugin is switched off it holds no engine, and an
/// event that still arrives reaches nothing.
// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: DisposeUnsubscribesFromTheClient
#[test]
fn dispose_unsubscribes_from_the_client() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    assert!(plugin.tank().is_some());

    world.disable(&mut plugin);
    world.clear_chat();
    world.send(&mut plugin, Event::LoginComplete);
    world.send(&mut plugin, Event::Logoff);
    world.tick(&mut plugin);

    assert!(plugin.tank().is_none());
    assert!(world.chat().is_empty());
}

fn placement() -> Event {
    Event::EquipmentPlacement(EquipmentObservation::default())
}

/// The C# panel and its loot classifier each observe equipment placements
/// from enable to disable, and a reload leaves none of the old observers
/// behind. The plugin keeps one buffer of placements for both: it is
/// emptied when the plugin is switched off and fills only while it is on.
// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPluginTests.cs :: ReloadDetachesEquipmentObserverBeforeTheNextPanelSubscribes
#[test]
fn reload_detaches_equipment_observer_before_the_next_panel_subscribes() {
    let mut world = World::with_character("Barris");
    let mut first = world.enable();
    world.send(&mut first, placement());
    assert_eq!(first.observed_placements(), 1);

    world.disable(&mut first);
    assert_eq!(first.observed_placements(), 0);
    world.send(&mut first, placement());
    assert_eq!(first.observed_placements(), 0);

    let mut second = world.enable();
    assert_eq!(second.observed_placements(), 0);
    world.send(&mut second, placement());
    assert_eq!(second.observed_placements(), 1);

    world.disable(&mut second);
    assert_eq!(second.observed_placements(), 0);
}
