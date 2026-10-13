//! The buff pass as the panel runs it: when it rebuffs, what a cast's
//! receipt, words and result do to it, the tiers it walks to and the lines
//! it logs (ported from the C#'s `MossTankPanelTests`, over its
//! `FakeAutomation`).

mod common;

use common::World;
use common::automation::{FakeAutomation, active, skill};
use mosstank::MossTank;

/// `BuffPassAutomation`: three self buffs, each for a trained skill.
fn buff_pass_automation() -> FakeAutomation {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.skills = vec![
        skill(1, "Life Magic", 300),
        skill(2, "War Magic", 300),
        skill(3, "Item Tinkering", 300),
    ];
    automation.set_known_self_buffs(vec![
        common::spell(
            1,
            10,
            "Increases the caster's Life Magic skill by 10 points.",
        ),
        common::spell(
            2,
            20,
            "Increases the caster's War Magic skill by 10 points.",
        ),
        common::spell(
            3,
            30,
            "Increases the caster's Item Tinkering skill by 10 points.",
        ),
    ]);
    automation
}

fn open(automation: FakeAutomation) -> (World, MossTank) {
    let mut world = World::with_automation(automation);
    let plugin = world.enable();
    (world, plugin)
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn ticks(world: &mut World, plugin: &mut MossTank, count: usize, seconds: f64) {
    for _ in 0..count {
        tick(world, plugin, seconds);
    }
}

fn buff_status(world: &mut World, plugin: &mut MossTank) -> String {
    world.ui_text(plugin, "BuffStatus")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnAcceptedCastAdvancesTheQueueOnItsSuccessfulResult
#[test]
fn an_accepted_cast_advances_the_queue_on_its_successful_result() {
    let (mut world, mut plugin) = open(buff_pass_automation());
    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 8, 0.3);
    // The plain fake never marks the enchantment active, so a later scan
    // re-queues the same three; only the first pass is under test.
    assert_eq!(world.cast_spell_ids()[..3], [1, 2, 3]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RunningMacroRebuffsNormallyAndUsesWiderIdleTopoffOnlyWhenEnabled
#[test]
fn running_macro_rebuffs_normally_and_uses_wider_idle_topoff_only_when_enabled() {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.skills = vec![skill(1, "Life Magic", 300)];
    automation.set_known_self_buffs(vec![common::spell(
        1,
        10,
        "Increases the caster's Life Magic skill by 10 points.",
    )]);
    automation.active_enchantments = vec![active(1, 10, 1, 600.0)];
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    assert!(world.cast_spell_ids().is_empty());

    world.act(&mut plugin, "ToggleIdleBuffTopoff");
    tick(&mut world, &mut plugin, 1.0);

    assert_eq!(world.cast_spell_ids(), [1]);
    assert!(buff_status(&mut world, &mut plugin).starts_with("Buffing"));
}
