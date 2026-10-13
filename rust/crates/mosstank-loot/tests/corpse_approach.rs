//! The looter's walk: which corpse it steers at, the two radii that bound
//! it, and what a lost pass does to it (`LootingTests.CorpseApproach.cs`).

mod common;

use common::{World, approach_corpse, enabled_settings, expression_rule, own_rare_corpse};
use mosstank_combat::mover::MOVER_INTERVAL_SECONDS;
use mosstank_loot::LootController;
use mosstank_loot::corpse_approach::{
    ApproachTurn, CorpseApproachController, OWN_RARE_WALK_GIVE_UP_SECONDS,
};
use mosstank_loot::host::LootCorpse;
use mosstank_meta::scheduler::{MACRO_RULE_TABLE, MacroRuleSlot};
use mosstank_settings::model::LootSettings;

/// `ApproachSettings`.
fn approach_settings(range: f64, walk_to_own_rares: bool) -> LootSettings {
    LootSettings {
        corpse_approach_range: range,
        scan_interval_seconds: 0.05,
        walk_to_own_rare_corpses: walk_to_own_rares,
        ..enabled_settings(vec![expression_rule("*")])
    }
}

/// `ApproachAutomation`: the corpses, and the character in the world at
/// the origin facing north.
fn approach_world(corpses: Vec<LootCorpse>) -> World {
    World {
        navigation_available: true,
        navigation_position: ac_plugin_abi::world::Position {
            cell_id: 0x7F7F_0001,
            is_outdoor: true,
            ..ac_plugin_abi::world::Position::default()
        },
        ..World::with_corpses(corpses)
    }
}

/// `Approach`/`ApproachWithLoot`: the walk, the looter it walks for, and
/// the client.
struct Walk {
    world: World,
    loot: LootController,
    approach: CorpseApproachController,
}

impl Walk {
    fn new(corpse: LootCorpse, settings: LootSettings) -> Self {
        Self {
            world: approach_world(vec![corpse]),
            loot: LootController::new(settings),
            approach: CorpseApproachController::new(),
        }
    }

    fn claim(&mut self, can_act: bool) -> bool {
        let mut turn = ApproachTurn {
            host: &mut self.world,
            loot: &mut self.loot,
            creep: None,
        };
        self.approach.claim_from_rule_pass(&mut turn, can_act)
    }

    fn step(&mut self, elapsed: f64) {
        let mut turn = ApproachTurn {
            host: &mut self.world,
            loot: &mut self.loot,
            creep: None,
        };
        self.approach.step_armed_mover(&mut turn, elapsed);
    }

    fn status(&self) -> &str {
        self.approach.status()
    }

    fn has_own_rare_corpse_to_fetch(&mut self) -> bool {
        let range = self.loot.settings.corpse_approach_range;
        self.loot
            .has_own_rare_corpse_to_fetch(&mut self.world, range)
    }

    /// `WalkUntilGivenUp`: host frames of twenty a second, a rule pass every
    /// fifth, until the walk gives up (its status then) or the frames run
    /// out (empty). Pacing moves the character a metre east and back every
    /// half second.
    fn walk_until_given_up(&mut self, frames: usize, pace: bool) -> String {
        for frame in 0..frames {
            if pace {
                self.world.navigation_position.east_west = if (frame / 10) % 2 == 0 {
                    0.0
                } else {
                    1.0 / 240.0
                };
            }
            self.step(0.05);
            if self.status().starts_with("Gave up") {
                return self.status().to_owned();
            }
            if frame % 5 == 4 {
                let claimed = self.claim(true);
                if self.status().starts_with("Gave up") {
                    return self.status().to_owned();
                }
                assert!(claimed);
            }
        }
        String::new()
    }
}

fn corpse(id: u32, distance: f32) -> LootCorpse {
    approach_corpse(id, distance)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: ACorpseBeyondArmsReachIsWalkedTo
#[test]
fn a_corpse_beyond_arms_reach_is_walked_to() {
    let mut walk = Walk::new(corpse(0x7000_2000, 12.0), approach_settings(30.0, false));

    assert!(walk.claim(true));

    assert_eq!(walk.world.intents.len(), 1);
    let intent = &walk.world.intents[0];
    assert!(intent.forward);
    assert!(intent.run);
    assert!(walk.status().contains("Walking to"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: TheShippedZeroRangeWalksNowhere
#[test]
fn the_shipped_zero_range_walks_nowhere() {
    let mut walk = Walk::new(corpse(0x7000_2001, 12.0), approach_settings(0.0, false));

    assert!(!walk.claim(true));
    assert!(walk.world.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: ACorpseFurtherThanTheRangeIsNotWalkedTo
#[test]
fn a_corpse_further_than_the_range_is_not_walked_to() {
    let mut walk = Walk::new(corpse(0x7000_2002, 20.0), approach_settings(15.0, false));

    assert!(!walk.claim(true));
    assert!(walk.world.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: NoActiveProfileNeverWalksToACorpseForAnExternalClassifier
#[test]
fn no_active_profile_never_walks_to_a_corpse_for_an_external_classifier() {
    let mut settings = approach_settings(30.0, false);
    settings.profile_active = false;
    settings.external_classifier_id = "utility/loot".to_owned();
    settings.rules.clear();
    let mut walk = Walk::new(corpse(0x7000_2010, 12.0), settings);

    assert!(!walk.claim(true));
    assert!(walk.world.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: TheWalkEndsAtTheInnerRadius
#[test]
fn the_walk_ends_at_the_inner_radius() {
    for (distance, walks) in [(4.0, true), (3.0, false)] {
        let mut walk = Walk::new(
            corpse(0x7000_2003, distance),
            approach_settings(30.0, false),
        );

        assert_eq!(walk.claim(true), walks, "{distance}");
        assert_eq!(!walk.world.intents.is_empty(), walks, "{distance}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: ACorpseTheCharacterMayNotLootIsNotWalkedTo
#[test]
fn a_corpse_the_character_may_not_loot_is_not_walked_to() {
    let stranger = LootCorpse {
        long_description: "Killed by Stranger.".to_owned(),
        ..corpse(0x7000_2004, 12.0)
    };
    let mut walk = Walk::new(stranger, approach_settings(30.0, false));

    assert!(!walk.claim(true));
    assert!(walk.world.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: LosingThePassStopsTheWalk
#[test]
fn losing_the_pass_stops_the_walk() {
    let mut walk = Walk::new(corpse(0x7000_2005, 12.0), approach_settings(30.0, false));

    assert!(walk.claim(true));
    assert_eq!(walk.world.movement_clears, 0);

    walk.approach.stop_for_lost_turn(&mut walk.world);

    assert_eq!(walk.world.movement_clears, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: TheArmedWalkStepsOnTheMoverInterval
#[test]
fn the_armed_walk_steps_on_the_mover_interval() {
    let mut walk = Walk::new(corpse(0x7000_2006, 12.0), approach_settings(30.0, false));

    assert!(walk.claim(true));
    assert_eq!(walk.world.intents.len(), 1);

    walk.step(MOVER_INTERVAL_SECONDS / 2.0);
    assert_eq!(walk.world.intents.len(), 1);

    walk.step(MOVER_INTERVAL_SECONDS);
    assert_eq!(walk.world.intents.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: ADisarmedWalkDoesNotSteer
#[test]
fn a_disarmed_walk_does_not_steer() {
    let mut walk = Walk::new(corpse(0x7000_2007, 12.0), approach_settings(30.0, false));

    walk.step(1.0);

    assert!(walk.world.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: TheWalkOutranksTheOpenAndTheRoute
#[test]
fn the_walk_outranks_the_open_and_the_route() {
    fn position(slot: MacroRuleSlot) -> Option<usize> {
        MACRO_RULE_TABLE
            .iter()
            .position(|entry| entry.slot() == Some(slot))
    }

    assert!(
        position(MacroRuleSlot::NavigateCorpsePriority)
            < position(MacroRuleSlot::OpenCorpsePriority)
    );
    assert!(
        position(MacroRuleSlot::OpenCorpsePriority)
            < position(MacroRuleSlot::NavigateRoutePriority)
    );
    assert!(position(MacroRuleSlot::NavigateCorpseIdle) < position(MacroRuleSlot::OpenCorpseIdle));
    assert!(position(MacroRuleSlot::OpenCorpseIdle) < position(MacroRuleSlot::NavigateRouteIdle));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: TheInnerRadiusComesFromTheProfile
#[test]
fn the_inner_radius_comes_from_the_profile() {
    for (minimum_range, walks) in [(3.0, true), (8.0, false)] {
        let mut settings = approach_settings(30.0, false);
        settings.corpse_minimum_approach_range = minimum_range;
        let mut walk = Walk::new(corpse(0x7000_2020, 6.0), settings);

        assert_eq!(walk.claim(true), walks, "{minimum_range}");
        assert_eq!(!walk.world.intents.is_empty(), walks, "{minimum_range}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: AnInnerRadiusWiderThanTheOuterReachLeavesNoBand
#[test]
fn an_inner_radius_wider_than_the_outer_reach_leaves_no_band() {
    let mut settings = approach_settings(10.0, false);
    settings.corpse_minimum_approach_range = 20.0;
    let mut walk = Walk::new(corpse(0x7000_2021, 8.0), settings);

    assert!(!walk.claim(true));
    assert!(walk.world.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: ThisCharactersRareIsWalkedToOutToTheRareReach
#[test]
fn this_characters_rare_is_walked_to_out_to_the_rare_reach() {
    for (distance, walks) in [(30.0, true), (99.0, true), (101.0, false)] {
        let mut walk = Walk::new(
            own_rare_corpse(0x7000_2030, distance),
            approach_settings(15.0, true),
        );

        assert_eq!(walk.claim(true), walks, "{distance}");
        assert_eq!(!walk.world.intents.is_empty(), walks, "{distance}");
        if walks {
            assert!(walk.status().contains("past the corpse approach range"));
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: OnlyThisCharactersRareGetsTheRareReach
#[test]
fn only_this_characters_rare_gets_the_rare_reach() {
    for description in [
        "Killed by Stranger. This corpse generated a rare item!",
        "Killed by Tester.",
    ] {
        let corpse = LootCorpse {
            long_description: description.to_owned(),
            ..corpse(0x7000_2031, 30.0)
        };
        let mut walk = Walk::new(corpse, approach_settings(15.0, true));

        assert!(!walk.claim(true), "{description}");
        assert!(walk.world.intents.is_empty(), "{description}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: WithTheOptionOffThisCharactersRarePastTheRangeIsLeft
#[test]
fn with_the_option_off_this_characters_rare_past_the_range_is_left() {
    let settings = approach_settings(15.0, false);
    assert!(!settings.walk_to_own_rare_corpses);
    let mut walk = Walk::new(own_rare_corpse(0x7000_2032, 30.0), settings);

    assert!(!walk.claim(true));
    assert!(walk.world.intents.is_empty());
    assert!(!walk.has_own_rare_corpse_to_fetch());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: WithTheOptionOffARareWalkIsNeverGivenUp
#[test]
fn with_the_option_off_a_rare_walk_is_never_given_up() {
    let mut walk = Walk::new(
        own_rare_corpse(0x7000_2033, 20.0),
        approach_settings(30.0, false),
    );

    assert!(walk.claim(true));
    for frame in 0..200 {
        walk.step(0.05);
        if frame % 5 == 4 {
            assert!(walk.claim(true));
        }
    }
    assert!(!walk.status().contains("Gave up"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: AStuckWalkToThisCharactersRareIsGivenUp
#[test]
fn a_stuck_walk_to_this_characters_rare_is_given_up() {
    let mut walk = Walk::new(
        own_rare_corpse(0x7000_2034, 30.0),
        approach_settings(15.0, true),
    );
    assert!(walk.claim(true));
    assert!(walk.has_own_rare_corpse_to_fetch());

    let given_up = walk.walk_until_given_up(100, false);

    assert!(given_up.contains("no ground covered"), "{given_up}");
    assert!(!walk.claim(true));
    assert!(!walk.has_own_rare_corpse_to_fetch());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: AWalkToThisCharactersRareThatNeverArrivesIsGivenUpAfterItsTimeCap
#[test]
fn a_walk_to_this_characters_rare_that_never_arrives_is_given_up_after_its_time_cap() {
    let mut walk = Walk::new(
        own_rare_corpse(0x7000_2035, 30.0),
        approach_settings(15.0, true),
    );
    assert!(walk.claim(true));

    // Pacing a metre east and back: always covering ground, never there.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (OWN_RARE_WALK_GIVE_UP_SECONDS / 0.05) as usize;
    assert_eq!(walk.walk_until_given_up(frames - 40, true), "");
    assert!(walk.has_own_rare_corpse_to_fetch());

    let given_up = walk.walk_until_given_up(80, true);

    assert!(given_up.contains("not reached in 60 seconds"), "{given_up}");
    assert!(!walk.claim(true));
    assert!(!walk.has_own_rare_corpse_to_fetch());
}
