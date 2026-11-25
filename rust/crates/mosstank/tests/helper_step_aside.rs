//! The helper rule (a hurt fellow healed with Heal Other) never holds the
//! macro's pass for good when it cannot get a cast started: the route rule
//! below it gets its turn.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::social::{FellowMember, FellowshipState, FellowshipStatus};
use ac_plugin_abi::world::Position;
use common::World;
use common::automation::{FakeAutomation, skill};
use mosstank::MossTank;
use mosstank_data::spell_id;

/// The rig's position: the origin of cell 0x00010001, `north_south` map
/// units north.
fn stand_at(automation: &mut FakeAutomation, north_south: f64) {
    let position = Position {
        cell_id: 0x0001_0001,
        north_south,
        is_outdoor: true,
        ..Position::default()
    };
    automation.navigation.is_available = true;
    automation.navigation.local_object_id = 1;
    automation.navigation.position = position.clone();
    automation.navigation.confirmed_position = position;
}

/// Heal Other's line: the reference spell and one tier the character can
/// cast.
fn heal_other_line() -> Vec<SpellInfo> {
    let line = |spell_id: u32, name: &str, tier: i32, difficulty: i32| SpellInfo {
        spell_id,
        name: name.to_owned(),
        family: 79,
        tier,
        difficulty,
        mana_cost: 10,
        school: 33,
        is_self_targeted: false,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    vec![
        line(spell_id::AdjaSGift, "Adja's Gift", 7, 300),
        line(1166, "Heal Other VI", 6, 250),
    ]
}

/// A Life mage in a fellowship with one fellow at 10% health, 10 m away,
/// whose vitals have just streamed in, and a route waypoint far enough
/// north that the route rule always has something to do. `gate` is the
/// client's answer to a cast.
fn hurt_fellow_on_a_route(gate: CastGate) -> (World, MossTank) {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.skills = vec![skill(33, "Life Magic", 400)];
    automation.catalog_spells = heal_other_line();
    automation.cast_gate = gate;
    stand_at(&mut automation, 0.0);
    let mut world = World::with_automation(automation);
    world.set_reply::<calls::SpellsIsKnown>(&true);
    world.set_reply::<calls::FellowshipState>(&FellowshipState {
        is_in_fellowship: true,
        member_count: 2,
        ..FellowshipState::default()
    });
    world.set_reply::<calls::FellowshipMembers>(&vec![FellowMember {
        object_id: 71,
        name: "Hurt".to_owned(),
        current_health: 10,
        max_health: 100,
        current_stamina: 100,
        max_stamina: 100,
        current_mana: 100,
        max_mana: 100,
        distance: 10.0,
        vitals_age_seconds: Some(1.0),
        ..FellowMember::default()
    }]);
    world.set_reply::<calls::FellowshipRequestVitals>(&FellowshipStatus::Accepted);
    let mut plugin = world.enable();

    stand_at(world.automation(), 1.0);
    world.act(&mut plugin, "AddRoutePoint");
    stand_at(world.automation(), 0.0);
    world.act(&mut plugin, "ToggleNavigation");
    // The meta's death state: no fighting, no buffing, the route on.
    world.vt(&mut plugin, "opt set EnableCombat false");
    world.vt(&mut plugin, "opt set EnableBuffing false");
    world.vt(&mut plugin, "log ActiveRule on");
    world.clear_chat();
    world.act(&mut plugin, "ToggleCombat");
    (world, plugin)
}

fn picked(world: &World, rule: &str) -> bool {
    let line = format!("[MossTank] Picked {rule} P:");
    world.chat().iter().any(|said| said.starts_with(&line))
}

/// With combat off and a fellow under the helper's threshold that the
/// client refuses every cast at, the helper rule wins the first passes, then
/// steps aside (saying why), and the route rule runs.
#[test]
fn a_helper_that_cannot_cast_lets_the_route_run() {
    let (mut world, mut plugin) = hurt_fellow_on_a_route(CastGate::Refused);

    for _ in 0..4 {
        world.frame(&mut plugin, 0.3);
    }
    assert!(picked(&world, "UseHealersHeart"), "{:?}", world.chat());
    assert!(!picked(&world, "NavigateRouteIdle"), "{:?}", world.chat());

    for _ in 0..60 {
        world.frame(&mut plugin, 0.3);
    }
    assert!(picked(&world, "NavigateRouteIdle"), "{:?}", world.chat());
    assert!(!world.automation().movement_intents.is_empty());
    let logs = world.all_logs();
    assert!(
        logs.iter()
            .any(|line| line.starts_with("helper: can't heal Hurt: the client refuses")),
        "{logs:?}"
    );
    assert!(world.automation().cast_spell_ids.is_empty());
}

/// The same fellow, with the client taking the cast: the helper heals,
/// combat off or not, as the reference's helper does.
#[test]
fn a_helper_that_can_cast_heals_with_combat_off() {
    let (mut world, mut plugin) = hurt_fellow_on_a_route(CastGate::Ready);

    for _ in 0..4 {
        world.frame(&mut plugin, 0.3);
    }
    assert!(picked(&world, "UseHealersHeart"), "{:?}", world.chat());
    assert!(
        world
            .automation()
            .cast_spell_ids
            .iter()
            .any(|&spell| spell == spell_id::AdjaSGift || spell == 1166),
        "{:?}",
        world.automation().call_log
    );
    let logs = world.all_logs();
    assert!(
        !logs
            .iter()
            .any(|line| line.starts_with("helper: can't heal")),
        "{logs:?}"
    );
}
