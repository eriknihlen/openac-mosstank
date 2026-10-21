//! The Items page's items as a fresh session assesses them: a profiled item
//! is appraised without being added again, and one the client refuses or is
//! busy on does not starve the rest (ported from the C#'s
//! `MossTankPanelTests`, over its `FakeAutomation`).

mod common;

use common::World;
use common::automation::FakeAutomation;
use mosstank::MossTank;

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn ticks(world: &mut World, plugin: &mut MossTank, count: usize) {
    for _ in 0..count {
        tick(world, plugin, 0.3);
    }
}

/// "Prover" owning `items`.
fn prover(items: Vec<ac_plugin_abi::items::InventoryItem>) -> FakeAutomation {
    let mut automation = FakeAutomation::default();
    automation.name = "Prover".to_owned();
    automation.item_entries = items;
    automation
}

fn sword_and_wand() -> Vec<ac_plugin_abi::items::InventoryItem> {
    vec![
        common::item(10, "Fire Sword", 1),
        common::item(11, "Ice Wand", 0x8000),
    ]
}

/// A first session adds `object_ids` to the Items page; the plugin is then
/// turned off, the client becomes `next`, and a fresh session starts.
fn second_session(
    first: FakeAutomation,
    object_ids: &[u32],
    next: FakeAutomation,
) -> (World, MossTank) {
    let mut world = World::with_automation(first);
    let mut plugin = world.enable();
    for &object_id in object_ids {
        world.select_object(object_id);
        world.act(&mut plugin, "AddSelectedItem");
    }
    world.disable(&mut plugin);
    world.automation = Some(next);
    let plugin = world.enable();
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AProfileItemLoadedWithTheCharacterIsAssessedWithoutBeingReAdded
#[test]
fn a_profile_item_loaded_with_the_character_is_assessed_without_being_re_added() {
    // A fresh session: the same character, the same sword, not yet
    // assessed by this client. The profile names it; nobody re-adds it.
    let mut next = prover(vec![common::item(10, "Fire Sword", 1)]);
    next.unassessed.insert(10);
    let (mut world, mut plugin) =
        second_session(prover(vec![common::item(10, "Fire Sword", 1)]), &[10], next);
    // The client names none of its buffs here, so the item is written
    // with no spell and read back as kept without buffs.
    assert!(
        world
            .ui_list(&mut plugin, "ItemRows")
            .contains(&"Fire Sword (NO BUFFS)".to_owned())
    );

    ticks(&mut world, &mut plugin, 20);

    assert!(world.automation().identified.contains(&10));
    assert!(world.automation().unassessed.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemTheClientRefusesToAssessDoesNotStarveTheOthers
#[test]
fn an_item_the_client_refuses_to_assess_does_not_starve_the_others() {
    // A fresh session in which the client refuses the first item's
    // appraisal outright, every time.
    let mut next = prover(sword_and_wand());
    next.unassessed.extend([10, 11]);
    next.identify_refusals.insert(10);
    let (mut world, mut plugin) = second_session(prover(sword_and_wand()), &[10, 11], next);

    ticks(&mut world, &mut plugin, 20);

    assert!(world.automation().identified.contains(&11));
    assert_eq!(
        world
            .automation()
            .unassessed
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [10]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemTheChannelIsBusyOnDoesNotStarveTheOthersAndIsComeBackTo
#[test]
fn an_item_the_channel_is_busy_on_does_not_starve_the_others_and_is_come_back_to() {
    // The description channel is held by somebody else's question about the
    // first item: the shape a corpse identification in flight puts this
    // sweep in.
    let mut next = prover(sword_and_wand());
    next.unassessed.extend([10, 11]);
    next.identify_busy.insert(10);
    let (mut world, mut plugin) = second_session(prover(sword_and_wand()), &[10, 11], next);

    ticks(&mut world, &mut plugin, 20);

    // The second item is served meanwhile, and the busy one is still being
    // asked about rather than written off.
    let automation = world.automation();
    assert!(automation.identified.contains(&11));
    assert!(!automation.unassessed.contains(&11));
    assert!(automation.identified.iter().filter(|&&id| id == 10).count() > 1);

    automation.identify_busy.clear();
    ticks(&mut world, &mut plugin, 20);

    assert!(world.automation().unassessed.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AStagedSelfRechargeIsNotWipedByTheHelperRowsLosingTick
#[test]
fn a_staged_self_recharge_is_not_wiped_by_the_helper_rows_losing_tick() {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.health = (10, 100);
    let mut bread = common::item(60, "Bread", 0x20);
    // BoosterVital 2 = VitalKind.Health.
    bread.booster_vital = 2;
    automation.item_entries = vec![bread];
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();
    world.select_object(60);
    world.act(&mut plugin, "AddSelectedConsumable");

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6);

    assert_eq!(world.automation().used_item_ids, [60]);
}
