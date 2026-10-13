//! The pet rules as the panel builds them: the two refill bands, each on
//! its own threshold, and an essence on the Items page appraised in a fresh
//! session and then summoned with (ported from the C#'s
//! `MossTankPanelTests`, over its `FakeAutomation`).

mod common;

use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::world::ItemProperties;
use common::World;
use common::automation::{FakeAutomation, skill};
use mosstank::MossTank;
use mosstank_meta::scheduler::{MacroIndependentSlot, MacroPassContext, MacroRule, MacroRuleSlot};

/// The Encapsulated Spirit's class (`PetAutomationTests.EncapsulatedSpiritClass`).
const ENCAPSULATED_SPIRIT_CLASS: u32 = 49_485;

/// An essence's appraisal: its shared summoning cooldown, and no pet class,
/// which the server never sends.
fn essence() -> ItemProperties {
    ItemProperties {
        ints: vec![(280, 213)],
        ..ItemProperties::default()
    }
}

/// `Item(10, "Cold Rift", 0)` with `charges` of 50 left, and a spirit.
fn essence_and_spirit(charges: i32) -> FakeAutomation {
    let mut rift = common::item(10, "Cold Rift", 0);
    rift.structure = charges;
    rift.maximum_structure = 50;
    let mut spirit = common::item(11, "Encapsulated Spirit", 0);
    spirit.weenie_class_id = ENCAPSULATED_SPIRIT_CLASS;
    let mut automation = FakeAutomation::default();
    automation.item_entries = vec![rift, spirit];
    automation.properties.insert(10, essence());
    automation
}

/// The essence selected and added to the Items page.
fn with_essence_on_the_items_page(automation: FakeAutomation) -> (World, MossTank) {
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();
    world.automation().selected = Some(10);
    world.act(&mut plugin, "AddSelectedItem");
    (world, plugin)
}

fn valid_now(
    world: &mut World,
    plugin: &mut MossTank,
    rule: &mut dyn MacroRule<mosstank::tank::TankFamily>,
    elapsed_seconds: f64,
) -> bool {
    world.with_tank(plugin, |tank, client| {
        tank.rule_valid_now(client, rule, MacroPassContext::new(elapsed_seconds, true))
    })
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheIdleBandRefillsACombatPetOnTheIdleThresholdNotTheNormalOne
#[test]
fn the_idle_band_refills_a_combat_pet_on_the_idle_threshold_not_the_normal_one() {
    let (mut world, mut plugin) = with_essence_on_the_items_page(essence_and_spirit(3));
    let p = &mut plugin;
    world.vt(p, "opt set petrefillcount-normal 0");
    world.vt(p, "opt set petrefillcount-idle 3");
    world.act(p, "ToggleCombat");

    let mut rule = p
        .tank()
        .unwrap()
        .create_rule(MacroRuleSlot::RefillPetChargesIdle);

    // Three charges left is under the idle threshold and over the normal
    // one: only a rule reading its own setting stops for this.
    assert!(valid_now(&mut world, p, rule.as_mut(), 0.3));

    world.vt(p, "opt set petrefillcount-idle 0");

    assert!(!valid_now(&mut world, p, rule.as_mut(), 2.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheHelperBandRefillsACombatPetOnTheNormalThreshold
/// The refill stands in the helper band too, on the normal threshold: it
/// needs no monster and no summon setting, only an essence on the Items
/// page that is nearly out and a spirit.
#[test]
fn the_helper_band_refills_a_combat_pet_on_the_normal_threshold() {
    let (mut world, mut plugin) = with_essence_on_the_items_page(essence_and_spirit(1));
    let p = &mut plugin;
    world.vt(p, "opt set summonpets false");
    world.vt(p, "opt set petrefillcount-normal 1");
    world.act(p, "ToggleCombat");

    let mut rule = p
        .tank()
        .unwrap()
        .create_rule(MacroRuleSlot::RefillPetChargesNormal);

    assert!(valid_now(&mut world, p, rule.as_mut(), 0.3));

    world.vt(p, "opt set petrefillcount-normal 0");

    assert!(!valid_now(&mut world, p, rule.as_mut(), 2.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemsPageEssenceIsAppraisedInAFreshSessionAndThenSummonedWith
/// A fresh session: the profile names an essence this client has not
/// appraised. The Items page gets it appraised, and once the answer is in
/// the summon rule knows it for a pet device and uses it.
#[test]
fn an_items_page_essence_is_appraised_in_a_fresh_session_and_then_summoned_with() {
    let mut first = FakeAutomation::default();
    "Prover".clone_into(&mut first.name);
    first.item_entries = vec![common::item(10, "Cold Rift", 0)];
    first.properties.insert(10, essence());
    let (mut world, mut first_plugin) = with_essence_on_the_items_page(first);
    assert!(
        world
            .ui_list(&mut first_plugin, "ItemRows")
            .iter()
            .any(|row| row.starts_with("Cold Rift"))
    );
    world.disable(&mut first_plugin);

    let mut automation = FakeAutomation::default();
    "Prover".clone_into(&mut automation.name);
    automation.item_entries = vec![common::item(10, "Cold Rift", 0)];
    automation.hostile_targets = vec![CombatTarget {
        object_id: 30,
        name: "Drudge".to_owned(),
        weenie_class_id: 700,
        distance: 2.0,
        relative_angle_degrees: 0.0,
        is_health_known: true,
        health_fraction: 1.0,
        ..CombatTarget::default()
    }];
    automation.skills = vec![skill(54, "Summoning", 300)];
    automation.unassessed.insert(10);
    automation.appraisal_replies.insert(10, essence());
    let plugin_data = world.plugin_data.clone();
    let profiles = world.profiles.clone();
    let mut world = World::with_automation(automation);
    world.plugin_data = plugin_data;
    world.profiles = profiles;
    let mut plugin = world.enable();
    let p = &mut plugin;
    assert!(
        world
            .ui_list(p, "ItemRows")
            .iter()
            .any(|row| row.starts_with("Cold Rift"))
    );
    let mut summon = p
        .tank()
        .unwrap()
        .create_independent_rule(MacroIndependentSlot::SummonPet);
    world.act(p, "ToggleCombat");

    assert!(!valid_now(&mut world, p, summon.as_mut(), 0.3));

    for _ in 0..20 {
        if world.automation().unassessed.is_empty() {
            break;
        }
        tick(&mut world, p, 0.3);
    }

    assert!(world.automation().identified.contains(&10));
    // With the answer in, the macro's own pass summons with it.
    for _ in 0..5 {
        if world.automation().used_item_ids.contains(&10) {
            break;
        }
        tick(&mut world, p, 0.3);
    }
    assert!(world.automation().used_item_ids.contains(&10));
}
