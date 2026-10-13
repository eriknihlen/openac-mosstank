//! The rare this character just found is never walked away from (ported
//! from the C#'s `MossTankPanelTests.RareWalk`). The corpse the server says
//! generated this character's rare is walked to up to the hundred metres
//! MossTank ever walks to a corpse, even when the profile's corpse approach
//! range is shorter, and the route waits while it is; behind an option that
//! is off unless turned on.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::{ItemResult, ItemStatus, LootContainer, LootState};
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;
use mosstank_formats::utl::{LootRule, VtankLootProfile};

const ORDINARY_CORPSE: u32 = 0x7000_0E01;
const RARE_CORPSE: u32 = 0x7000_0E02;

/// The character's name, as the server's announcements say it.
const NAME: &str = "Rare Walker";

/// Due north of the start, `north_south` map units (`RareWalkSnapshot`).
fn snapshot(north_south: f64) -> NavigationSnapshot {
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

/// A corpse due north of the character, behind the route
/// (`RareWalkCorpse`).
fn corpse(object_id: u32, distance: f32, description: Option<&str>) -> LootContainer {
    LootContainer {
        object_id,
        weenie_class_id: 1,
        name: "Corpse of Drudge Bloodletter".to_owned(),
        distance,
        is_identified: description.is_some(),
        long_description: description.unwrap_or_default().to_owned(),
        position: Some(Position {
            cell_id: 0x0001_0001,
            north_south: f64::from(distance) / 240.0,
            is_outdoor: true,
            ..Position::default()
        }),
        ..LootContainer::default()
    }
}

fn killed(rare: bool) -> String {
    if rare {
        format!("Killed by {NAME}. This corpse generated a rare item!")
    } else {
        format!("Killed by {NAME}.")
    }
}

fn set_corpses(world: &mut World, corpses: Vec<LootContainer>) {
    world.set_reply::<calls::LootCorpses>(&corpses);
}

fn tick(world: &mut World, plugin: &mut MossTank) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, 0.3));
}

fn option(plugin: &MossTank, name: &str) -> bool {
    plugin.tank().unwrap().option(name).is_truthy()
}

fn walk_to_own_rares(plugin: &MossTank) -> bool {
    plugin
        .tank()
        .unwrap()
        .settings()
        .inventory
        .loot
        .walk_to_own_rare_corpses
}

fn opt(world: &mut World, plugin: &mut MossTank, setting: &str) {
    world.vt(plugin, &format!("opt set {setting}"));
}

fn said(world: &World, text: &str) -> bool {
    world.chat().iter().any(|line| line.contains(text))
}

/// The meta's looting setup on Coldeve: rare-only looting, a fifteen metre
/// corpse approach range, looting outranking the route (or the route
/// outranking looting), and one route point sixty-six metres south for the
/// route to walk to (`RareWalkPanel`).
fn rare_walk_world(
    loot_priority_boost: bool,
    nav_priority_boost: bool,
    rare_only: bool,
    walk_own_rares: bool,
) -> (World, MossTank) {
    let mut world = World::with_character(NAME);
    world.chat_log = Some(Vec::new());
    world.set_reply::<calls::LootState>(&LootState {
        is_available: true,
        current_contents_ready: true,
        ..LootState::default()
    });
    world.set_reply::<calls::LootOpen>(&ItemResult {
        status: ItemStatus::Started,
        ..ItemResult::default()
    });
    world.set_reply::<calls::NavigationSnapshot>(&snapshot(0.0));
    // The client takes every movement the macro asks for.
    world.set_reply::<calls::NavigationSetIntent>(
        &ac_plugin_abi::navigation::NavigationStatus::Accepted,
    );
    // `AddLootRule`: the loot editor's new rule, keep everything.
    let mut rule = LootRule::default();
    "Rule 1".clone_into(&mut rule.name);
    let profile = VtankLootProfile {
        rules: vec![rule],
        ..VtankLootProfile::default()
    };
    world.profiles.insert(
        "mosstank/loot/RareWalk.utl",
        &mosstank_formats::utl::write(&profile),
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "loot load RareWalk");
    if !option(&plugin, "EnableLooting") {
        opt(&mut world, &mut plugin, "EnableLooting true");
    }
    if walk_to_own_rares(&plugin) != walk_own_rares {
        let flipped = !walk_to_own_rares(&plugin);
        opt(
            &mut world,
            &mut plugin,
            &format!("WalkToRareCorpse {flipped}"),
        );
    }
    opt(
        &mut world,
        &mut plugin,
        &format!("LootOnlyRareCorpses {rare_only}"),
    );
    opt(&mut world, &mut plugin, "CorpseApproachRange-Max 0.0625");
    opt(
        &mut world,
        &mut plugin,
        &format!("LootPriorityBoost {loot_priority_boost}"),
    );
    opt(
        &mut world,
        &mut plugin,
        &format!("NavPriorityBoost {nav_priority_boost}"),
    );
    world.set_reply::<calls::NavigationSnapshot>(&snapshot(-66.0 / 240.0));
    world.with_tank(&mut plugin, |tank, client| tank.add_route_point(client));
    world.set_reply::<calls::NavigationSnapshot>(&snapshot(0.0));
    // `ToggleNavigation`.
    let navigating = option(&plugin, "EnableNav");
    opt(
        &mut world,
        &mut plugin,
        &format!("EnableNav {}", !navigating),
    );
    world.vt(&mut plugin, "log ActiveRule on");
    world.vt(&mut plugin, "log Loot on");
    world.clear_chat();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    (world, plugin)
}

fn opened(world: &World) -> Option<u32> {
    world.requests::<calls::LootOpen>().last().copied()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: WithTheOptionOffAnAnnouncedRarePastTheRangeIsLeftAndTheRouteWalksOn
#[test]
fn with_the_option_off_an_announced_rare_past_the_range_is_left_and_the_route_walks_on() {
    let (mut world, mut plugin) = rare_walk_world(true, false, true, false);
    assert!(!walk_to_own_rares(&plugin));
    tick(&mut world, &mut plugin);

    set_corpses(&mut world, vec![corpse(RARE_CORPSE, 15.3, None)]);
    tick(&mut world, &mut plugin);
    world.post_chat(
        &mut plugin,
        &format!("{NAME} has discovered the Warrior's Crystal!"),
    );
    tick(&mut world, &mut plugin);
    set_corpses(
        &mut world,
        vec![corpse(RARE_CORPSE, 15.3, Some(&killed(true)))],
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        said(&world, "Picked NavigateRouteIdle"),
        "{:#?}",
        world.chat()
    );
    assert!(!said(&world, "Picked NavigateCorpsePriority"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: WithTheOptionOffARouteThatOutranksLootingDoesNotWaitForTheRare
#[test]
fn with_the_option_off_a_route_that_outranks_looting_does_not_wait_for_the_rare() {
    let (mut world, mut plugin) = rare_walk_world(false, true, true, false);
    tick(&mut world, &mut plugin);
    set_corpses(
        &mut world,
        vec![corpse(RARE_CORPSE, 10.0, Some(&killed(true)))],
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        said(&world, "Picked NavigateRoutePriority"),
        "{:#?}",
        world.chat()
    );
    assert!(!said(&world, "Holding the route"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: WithTheOptionOffAnUndescribedCorpseFarBackDoesNotHoldTheRoute
#[test]
fn with_the_option_off_an_undescribed_corpse_far_back_does_not_hold_the_route() {
    let (mut world, mut plugin) = rare_walk_world(true, false, true, false);
    tick(&mut world, &mut plugin);

    set_corpses(&mut world, vec![corpse(RARE_CORPSE, 40.0, None)]);
    tick(&mut world, &mut plugin);
    world.post_chat(
        &mut plugin,
        &format!("{NAME} has discovered the Warrior's Crystal!"),
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        !world
            .chat()
            .contains(&"[MossTank] Holding this tick until the corpse id arrives.".to_owned())
    );
    assert!(
        said(&world, "Picked NavigateRouteIdle"),
        "{:#?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: WithTheOptionAndRareOnlyLootingOffTheAnnouncementIsNotListenedFor
#[test]
fn with_the_option_and_rare_only_looting_off_the_announcement_is_not_listened_for() {
    let (mut world, mut plugin) = rare_walk_world(true, false, false, false);
    world.post_chat(
        &mut plugin,
        &format!("{NAME} has discovered the Warrior's Crystal!"),
    );
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(!said(&world, "rare announced"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: TheOwnRareWalkOptionIsOffByDefaultAndSurvivesARestart
#[test]
fn the_own_rare_walk_option_is_off_by_default_and_survives_a_restart() {
    let mut world = World::with_character("Prover");
    let mut plugin = world.enable();
    assert!(!walk_to_own_rares(&plugin));

    // `ToggleWalkToOwnRareCorpses`.
    opt(&mut world, &mut plugin, "WalkToRareCorpse true");

    assert!(walk_to_own_rares(&plugin));
    assert!(world.plugin_data.keys().any(|key| {
        key.starts_with("profiles/macro/sidecar/")
            && world
                .plugin_data
                .get(key)
                .unwrap()
                .contains("\"InventoryLootWalkToOwnRareCorpses\": true")
    }));
    world.disable(&mut plugin);
    let restarted = world.enable();
    assert!(walk_to_own_rares(&restarted));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: AnAnnouncedRareJustPastTheApproachRangeIsWalkedToAndOpened
#[test]
fn an_announced_rare_just_past_the_approach_range_is_walked_to_and_opened() {
    let (mut world, mut plugin) = rare_walk_world(true, false, true, true);

    // The fight is over and the route walks on.
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);
    assert!(
        said(&world, "Picked NavigateRouteIdle"),
        "{:#?}",
        world.chat()
    );

    // The two corpses land behind the character, and half a second later the
    // server announces the rare.
    set_corpses(
        &mut world,
        vec![
            corpse(ORDINARY_CORPSE, 10.4, None),
            corpse(RARE_CORPSE, 15.3, None),
        ],
    );
    tick(&mut world, &mut plugin);
    world.post_chat(
        &mut plugin,
        &format!("{NAME} has discovered the Warrior's Crystal!"),
    );
    tick(&mut world, &mut plugin);
    assert!(
        said(&world, "rare announced (Warrior's Crystal)"),
        "{:#?}",
        world.chat()
    );

    // Both descriptions arrive.
    set_corpses(
        &mut world,
        vec![
            corpse(ORDINARY_CORPSE, 10.4, Some(&killed(false))),
            corpse(RARE_CORPSE, 15.3, Some(&killed(true))),
        ],
    );
    world.clear_chat();
    world.fake.requests.clear();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        said(&world, "Picked NavigateCorpsePriority"),
        "{:#?}",
        world.chat()
    );
    assert!(!said(&world, "Picked NavigateRouteIdle"));
    assert!(
        world
            .requests::<calls::NavigationSetIntent>()
            .iter()
            .any(|intent| intent.forward)
    );

    // Arrived: the rare's corpse is in arm's reach, and it is opened.
    set_corpses(
        &mut world,
        vec![
            corpse(ORDINARY_CORPSE, 12.0, Some(&killed(false))),
            corpse(RARE_CORPSE, 2.0, Some(&killed(true))),
        ],
    );
    for _ in 0..12 {
        if opened(&world).is_some() {
            break;
        }
        tick(&mut world, &mut plugin);
    }

    assert_eq!(opened(&world), Some(RARE_CORPSE));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: AnUndescribedCorpseFromTheAnnouncementHoldsTheRouteOutToTheRareReach
#[test]
fn an_undescribed_corpse_from_the_announcement_holds_the_route_out_to_the_rare_reach() {
    let (mut world, mut plugin) = rare_walk_world(true, false, true, true);
    tick(&mut world, &mut plugin);

    set_corpses(&mut world, vec![corpse(RARE_CORPSE, 40.0, None)]);
    tick(&mut world, &mut plugin);
    world.post_chat(
        &mut plugin,
        &format!("{NAME} has discovered the Warrior's Crystal!"),
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        world
            .chat()
            .contains(&"[MossTank] Holding this tick until the corpse id arrives.".to_owned()),
        "{:#?}",
        world.chat()
    );
    assert!(!said(&world, "Picked NavigateRouteIdle"));

    set_corpses(
        &mut world,
        vec![corpse(RARE_CORPSE, 40.0, Some(&killed(true)))],
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        said(&world, "Picked NavigateCorpsePriority"),
        "{:#?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: ARouteThatOutranksLootingStillWaitsForThisCharactersRare
#[test]
fn a_route_that_outranks_looting_still_waits_for_this_characters_rare() {
    let (mut world, mut plugin) = rare_walk_world(false, true, true, true);
    tick(&mut world, &mut plugin);

    set_corpses(&mut world, vec![corpse(RARE_CORPSE, 30.0, None)]);
    tick(&mut world, &mut plugin);
    world.post_chat(
        &mut plugin,
        &format!("{NAME} has discovered the Warrior's Crystal!"),
    );
    tick(&mut world, &mut plugin);
    set_corpses(
        &mut world,
        vec![corpse(RARE_CORPSE, 30.0, Some(&killed(true)))],
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        !said(&world, "Picked NavigateRoutePriority"),
        "{:#?}",
        world.chat()
    );
    assert!(said(&world, "Picked NavigateCorpseIdle"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.RareWalk.cs :: AnOrdinaryKillPastTheApproachRangeIsStillLeftBehind
#[test]
fn an_ordinary_kill_past_the_approach_range_is_still_left_behind() {
    let (mut world, mut plugin) = rare_walk_world(true, false, false, true);
    set_corpses(
        &mut world,
        vec![corpse(ORDINARY_CORPSE, 20.0, Some(&killed(false)))],
    );
    world.post_chat(
        &mut plugin,
        "Someone Else has discovered the Warrior's Crystal!",
    );
    world.clear_chat();
    tick(&mut world, &mut plugin);
    tick(&mut world, &mut plugin);

    assert!(
        said(&world, "Picked NavigateRouteIdle"),
        "{:#?}",
        world.chat()
    );
    assert!(!said(&world, "Picked NavigateCorpsePriority"));
}
