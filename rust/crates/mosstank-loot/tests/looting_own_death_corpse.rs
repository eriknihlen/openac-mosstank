//! `loot_own_death_corpse`: this character's own death corpse is looted at
//! once, whoever killed the character, and everything in it is taken
//! whatever the loot rules say, rare-only looting or not. The switch is on
//! by default; off, the corpse is a monster's kill like any other (ported
//! from the C#'s `LootingTests.OwnDeathCorpse`). Those tests are not yet in
//! this branch's C# tree, so they are named in "C# source" lines the census
//! does not read; make them Source lines when the C# tree catches up.

mod common;

use common::{World, enabled_settings, item, rule};
use mosstank_formats::utl::LootAction;
use mosstank_loot::LootController;
use mosstank_loot::host::LootCorpse;
use mosstank_settings::model::LootSettings;

const OWN_DEATH_CORPSE: u32 = 0x7000_0F01;
const OWN_DEATH_CORPSE_ITEM: u32 = 0x7000_0F02;

fn ruby_only_settings(loot_own_death_corpse: bool) -> LootSettings {
    LootSettings {
        loot_own_death_corpse,
        ..enabled_settings(vec![rule("Rubies", "Ruby", LootAction::Keep)])
    }
}

fn death_corpse_world(corpse_name: &str) -> World {
    World::with_corpses(vec![LootCorpse {
        is_identified: true,
        long_description: "Killed by Drudge Slinker.".to_owned(),
        ..LootCorpse::new(OWN_DEATH_CORPSE, corpse_name, 3.0)
    }])
}

/// Drop the own-death check from the corpse pick and the corpse (a
/// monster's kill, seen for no time at all) is never opened; drop the
/// keep-everything decision and the pyreal no rule keeps stays behind.
// C# source: LootingTests.OwnDeathCorpse.cs :: WithTheOptionOnYourOwnDeathCorpseIsOpenedAtOnceAndEmptied
#[test]
fn with_the_option_on_your_own_death_corpse_is_opened_at_once_and_emptied() {
    let mut world = death_corpse_world("Corpse of Tester");
    let mut controller = LootController::new(ruby_only_settings(true));

    assert!(controller.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [OWN_DEATH_CORPSE]);
    world.current = OWN_DEATH_CORPSE;
    world.contents = vec![item(OWN_DEATH_CORPSE_ITEM, "Pyreal", 273)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));

    // Nothing is appraised: everything is taken, so nothing needs judging.
    assert!(world.identified.is_empty());
    assert_eq!(world.picked, [OWN_DEATH_CORPSE_ITEM]);
}

// C# source: LootingTests.OwnDeathCorpse.cs :: WithTheOptionOffYourOwnDeathCorpseIsAnotherKillersCorpse
#[test]
fn with_the_option_off_your_own_death_corpse_is_another_killers_corpse() {
    let mut world = death_corpse_world("Corpse of Tester");
    let mut controller = LootController::new(ruby_only_settings(false));

    controller.tick(&mut world, 0.25, true);

    assert!(world.opened.is_empty());
}

// C# source: LootingTests.OwnDeathCorpse.cs :: TheOptionLeavesAnotherPlayersDeathCorpseAlone
#[test]
fn the_option_leaves_another_players_death_corpse_alone() {
    let mut world = death_corpse_world("Corpse of Someone Else");
    let mut controller = LootController::new(ruby_only_settings(true));

    controller.tick(&mut world, 0.25, true);

    assert!(world.opened.is_empty());
}

// C# source: LootingTests.OwnDeathCorpse.cs :: TheOptionIsOnByDefault
#[test]
fn the_option_is_on_by_default() {
    assert!(LootSettings::default().loot_own_death_corpse);
}

// C# source: LootingTests.OwnDeathCorpse.cs :: WithRareOnlyLootingOnYourOwnDeathCorpseIsStillOpened
#[test]
fn with_rare_only_looting_on_your_own_death_corpse_is_still_opened() {
    let mut world = death_corpse_world("Corpse of Tester");
    let mut controller = LootController::new(LootSettings {
        loot_only_rare_corpses: true,
        ..ruby_only_settings(true)
    });

    assert!(controller.tick(&mut world, 0.25, true));

    assert_eq!(world.opened, [OWN_DEATH_CORPSE]);
}

/// Rare-only looting describes nothing until a rare is announced; the own
/// death corpse is described regardless, and nothing else is. Keep the
/// rare-only gate on the description pass and the undescribed death
/// corpse is never asked about, so never looted.
// C# source: LootingTests.OwnDeathCorpse.cs :: WithRareOnlyLootingOnlyYourOwnDeathCorpseIsDescribed
#[test]
fn with_rare_only_looting_only_your_own_death_corpse_is_described() {
    const MONSTER_CORPSE: u32 = 0x7000_0F03;
    let mut world = World::with_corpses(vec![
        LootCorpse::new(MONSTER_CORPSE, "Corpse of Drudge", 3.0),
        LootCorpse::new(OWN_DEATH_CORPSE, "Corpse of Tester", 3.0),
    ]);
    let mut controller = LootController::new(LootSettings {
        loot_only_rare_corpses: true,
        ..ruby_only_settings(true)
    });

    // The corpses are listed every couple of seconds while no rare is
    // announced, so the death corpse is asked about within that.
    for _ in 0..5 {
        controller.tick_identification(&mut world, 0.5);
    }

    assert_eq!(world.identified, [OWN_DEATH_CORPSE]);
}
