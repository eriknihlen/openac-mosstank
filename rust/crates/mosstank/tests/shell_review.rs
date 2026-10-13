//! Shell behaviour a review against the C# panel found missing: each test
//! names the C# lines it follows.

mod common;

use common::World;

fn fresh() -> (World, mosstank::MossTank) {
    let mut world = World::with_character("Review Tester");
    let plugin = world.enable();
    (world, plugin)
}

/// The engine is built with the character's meta (`MossTankPanel.cs`
/// constructor): no transition, so nothing is announced on enable.
#[test]
fn enabling_announces_no_meta_state_change() {
    let (world, _plugin) = fresh();
    assert!(
        !world.chat().iter().any(|line| line.contains("Meta state:")),
        "{:?}",
        world.chat()
    );
}

/// `vtsetmetastate[]` sets the fight's copy of the state inside the
/// function (`MossTankPanel.cs` `RegisterVtankExpressionFunctions`).
#[test]
fn vtsetmetastate_sets_the_fights_meta_state_at_once() {
    let (mut world, mut plugin) = fresh();
    world
        .with_tank(&mut plugin, |tank, client| {
            tank.evaluate_expression(client, "vtsetmetastate[`Hunting`]")
        })
        .unwrap();
    let state = world.with_tank(&mut plugin, |tank, _| {
        tank.settings().combat.meta_state.clone()
    });
    assert_eq!(state, "Hunting");
}

/// .NET's `float.TryParse` takes NaN, and `power is < 0f or > 1f` lets it
/// through (`MossTankCommands.SetAttackBar`).
#[test]
fn the_attack_bar_takes_nan_as_the_reference_does() {
    let (mut world, mut plugin) = fresh();
    world.clear_chat();
    world.vt(&mut plugin, "setattackbar NaN");
    assert_eq!(world.chat(), ["[MossTank] Attack bar set to NaN."]);
    let power = world.with_tank(&mut plugin, |tank, _| tank.settings().combat.attack_power);
    assert!(power.is_nan());
    // The side-car cannot hold NaN: the serializer's refusal is logged and
    // the save reports failure, as `WriteJson` catches it.
    assert!(world.logs(ac_plugin_abi::LogLevel::Warn).iter().any(|line| {
        line.starts_with("MossTank profile could not be saved: .NET number values such as positive and negative infinity")
    }));
}

/// The slots the review listed hold live rules now.
#[test]
fn the_crafting_pet_and_helper_slots_hold_live_rules() {
    let (mut world, mut plugin) = fresh();
    let shapes = world.with_tank(&mut plugin, |tank, _| tank.rule_shapes());
    for name in [
        "SplitPeasCritical",
        "CraftFood",
        "CraftFoodIdle",
        "RefillPetChargesNormal",
        "RefillPetChargesIdle",
        "RandomHelper",
    ] {
        let (_, shape) = shapes
            .iter()
            .find(|(slot, _)| slot == name)
            .unwrap_or_else(|| panic!("{name} is not in the list"));
        assert!(
            !matches!(shape, mosstank_meta::scheduler::RuleShape::Absent(_)),
            "{name} is absent"
        );
    }
}
