//! Recall waypoints, and the tight waypoint that asks for magic mode
//! (`NavigationTests`).

mod common;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::EquipmentItem;
use ac_plugin_abi::magic::{CastCompletion, CastResult};
use ac_plugin_abi::world::ObjectClass;
use common::*;
use mosstank_formats::route::{RouteMode, RouteRecallKind, RouteWaypoint, RouteWaypointType};
use mosstank_nav::controller::LOW_WAYPOINT_DISTANCE_WARNING;

fn recall_waypoint() -> RouteWaypoint {
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall = RouteRecallKind::LifestoneRecall;
    recall.recall_spell_id = 1635;
    recall.recall_spell_name = "Lifestone Recall".to_owned();
    recall
}

/// A lifestone recall on a character who can cast: no combat-mode gate is
/// bound, so the spell goes out on the first tick (`RecallReadyToCast`).
fn recall_ready_to_cast(answer: CastResult) -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.magic().request_answer = answer;
    rig(
        automation,
        RouteMode::Circular,
        vec![
            recall_waypoint(),
            waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
        ],
    )
}

/// The same recall on a character standing still in peace mode, with the
/// macro's combat-mode gate bound (`RecallInPeace`).
fn recall_in_peace() -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.combat_mode = CombatMode::Peace;
    automation.server_mode = CombatMode::Peace;
    automation.magic();
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![
            recall_waypoint(),
            waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
        ],
    );
    rig.bind_gate();
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARecallWaypointIsDoneOnTheFirstTickAfterArrival
#[test]
fn a_recall_waypoint_is_done_on_the_first_tick_after_arrival() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().cast_spell_ids, [1635]);

    rig.automation.navigation_snapshot.is_portal_space = true;
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot = snapshot(position(4.0, 0.0));
    assert!(rig.tick(0.05, true));

    assert_eq!(rig.index(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RouteRecallKindListsVTanksTwentySixRecallsInOrderPlusMarketplaceLast
#[test]
fn route_recall_kind_lists_vtanks_twenty_six_recalls_in_order_plus_marketplace_last() {
    let names: Vec<&str> = RouteRecallKind::ALL
        .iter()
        .map(|kind| kind.name())
        .collect();
    assert_eq!(
        names,
        [
            "PrimaryPortalRecall",
            "SecondaryPortalRecall",
            "LifestoneRecall",
            "LifestoneSending",
            "PortalRecall",
            "RecallAphusLassel",
            "RecallTheSanctuary",
            "RecallToTheSingularityCaul",
            "GlendenWoodRecall",
            "AerlintheRecall",
            "MountLetheRecall",
            "UlgrimsRecall",
            "BurRecall",
            "ParadoxTouchedOlthoiInfestedAreaRecall",
            "CallOfTheMhoireForge",
            "ColosseumRecall",
            "FacilityHubRecall",
            "GearKnightInvasionAreaCampRecall",
            "LostCityOfNeftetRecall",
            "ReturnToTheKeep",
            "RynthidRecall",
            "ViridianRiseRecall",
            "ViridianRiseGreatTreeRecall",
            "CelestialHandStrongholdRecall",
            "RadiantBloodStrongholdRecall",
            "EldrytchWebStrongholdRecall",
            "Marketplace",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RecallNameAndSpellIdTablesAgree
#[test]
fn recall_name_and_spell_id_tables_agree() {
    use RouteRecallKind as K;
    for (kind, name, spell_id) in [
        (K::PrimaryPortalRecall, "Primary Portal Recall", 48),
        (K::SecondaryPortalRecall, "Secondary Portal Recall", 2647),
        (K::LifestoneRecall, "Lifestone Recall", 1635),
        (K::LifestoneSending, "Lifestone Sending", 1636),
        (K::PortalRecall, "Portal Recall", 2645),
        (K::RecallAphusLassel, "Recall Aphus Lassel", 2931),
        (K::RecallTheSanctuary, "Recall the Sanctuary", 2023),
        (
            K::RecallToTheSingularityCaul,
            "Recall to the Singularity Caul",
            2943,
        ),
        (K::GlendenWoodRecall, "Glenden Wood Recall", 3865),
        (K::AerlintheRecall, "Aerlinthe Recall", 2041),
        (K::MountLetheRecall, "Mount Lethe Recall", 2813),
        (K::UlgrimsRecall, "Ulgrim's Recall", 2941),
        (K::BurRecall, "Bur Recall", 4084),
        (
            K::ParadoxTouchedOlthoiInfestedAreaRecall,
            "Paradox-touched Olthoi Infested Area Recall",
            4198,
        ),
        (K::CallOfTheMhoireForge, "Call of the Mhoire Forge", 4128),
        (K::ColosseumRecall, "Colosseum Recall", 4213),
        (K::FacilityHubRecall, "Facility Hub Recall", 5175),
        (
            K::GearKnightInvasionAreaCampRecall,
            "Gear Knight Invasion Area Camp Recall",
            5330,
        ),
        (
            K::LostCityOfNeftetRecall,
            "Lost City of Neftet Recall",
            5541,
        ),
        (K::ReturnToTheKeep, "Return to the Keep", 4214),
        (K::RynthidRecall, "Rynthid Recall", 6150),
        (K::ViridianRiseRecall, "Viridian Rise Recall", 6321),
        (
            K::ViridianRiseGreatTreeRecall,
            "Viridian Rise Great Tree Recall",
            6322,
        ),
        (
            K::CelestialHandStrongholdRecall,
            "Celestial Hand Stronghold Recall",
            6325,
        ),
        (
            K::RadiantBloodStrongholdRecall,
            "Radiant Blood Stronghold Recall",
            6327,
        ),
        (
            K::EldrytchWebStrongholdRecall,
            "Eldrytch Web Stronghold Recall",
            6326,
        ),
        (K::Marketplace, "Marketplace Recall", 0),
    ] {
        assert_eq!(kind.display_name(), name);
        assert_eq!(kind.spell_id(), spell_id, "{name}");
    }
}

fn single_recall(recall: RouteWaypoint) -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.magic();
    rig(automation, RouteMode::Once, vec![recall])
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RecallWaypointWithNonZeroSpellIdCastsThatSpell
#[test]
fn recall_waypoint_with_non_zero_spell_id_casts_that_spell() {
    let kind = RouteRecallKind::AerlintheRecall;
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall = kind;
    recall.recall_spell_id = kind.spell_id();
    recall.recall_spell_name = kind.display_name().to_owned();
    let mut rig = single_recall(recall);

    assert!(rig.tick(0.1, true));

    assert!(rig.automation.magic().cast_spell_ids.contains(&2041));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RecallWaypointForMarketplaceSubmitsTheSlashCommandNotACast
#[test]
fn recall_waypoint_for_marketplace_submits_the_slash_command_not_a_cast() {
    let kind = RouteRecallKind::Marketplace;
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall = kind;
    recall.recall_spell_id = kind.spell_id();
    recall.recall_spell_name = kind.display_name().to_owned();
    let mut rig = single_recall(recall);

    assert!(rig.tick(0.1, true));

    assert!(rig.automation.magic().cast_spell_ids.is_empty());
    assert!(
        rig.automation
            .submitted_chat
            .iter()
            .any(|line| line == "/marketplace")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RecallWaypointWithUnresolvedSpellNameRefusesAndSkipsWithoutCasting
#[test]
fn recall_waypoint_with_unresolved_spell_name_refuses_and_skips_without_casting() {
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall_spell_id = 0;
    recall.recall_spell_name = "NotARealSpell".to_owned();
    let mut rig = single_recall(recall);

    assert!(rig.tick(0.1, true));

    assert!(rig.automation.magic().cast_spell_ids.is_empty());
    assert!(rig.automation.submitted_chat.is_empty());
    assert!(rig.status().contains("NotARealSpell"));

    assert!(!rig.tick(0.1, true));
    assert_eq!(rig.status(), "Once route complete.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARecallInPeaceModeAsksForMagicModeAndCastsNothing
#[test]
fn a_recall_in_peace_mode_asks_for_magic_mode_and_casts_nothing() {
    let mut rig = recall_in_peace();

    assert!(rig.tick(0.05, true));

    assert!(
        rig.automation
            .mode_requests
            .iter()
            .any(|request| request == "EnterMode:Magic")
    );
    assert!(rig.automation.magic().requested_spell_ids.is_empty());
    assert!(rig.automation.magic().cast_spell_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARecallCastsOnceTheCharacterIsStandingInMagicMode
#[test]
fn a_recall_casts_once_the_character_is_standing_in_magic_mode() {
    let mut rig = recall_in_peace();

    assert!(rig.tick(0.05, true));
    assert!(rig.automation.magic().cast_spell_ids.is_empty());

    rig.automation.combat_mode = CombatMode::Magic;
    rig.automation.server_mode = CombatMode::Magic;

    assert!(rig.tick(0.05, true));

    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);
    assert_eq!(rig.automation.magic().cast_spell_ids, [1635]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARefusedRecallSaysWhyInChatOnceAcrossItsRetries
#[test]
fn a_refused_recall_says_why_in_chat_once_across_its_retries() {
    let mut rig = recall_ready_to_cast(CastResult::MissingComponents);

    // The first attempt and two retries, all refused.
    assert!(rig.tick(0.05, true));
    assert!(rig.tick(2.5, true));
    assert!(rig.tick(2.5, true));

    assert_eq!(rig.automation.magic().requested_spell_ids.len(), 3);
    assert!(rig.automation.magic().cast_spell_ids.is_empty());
    assert_eq!(
        rig.automation.posted_system_messages,
        ["[MossTank] Recall 'Lifestone Recall' refused: MissingComponents."]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARecallStillInTheAirIsNotResentAndSaysNothing
#[test]
fn a_recall_still_in_the_air_is_not_resent_and_says_nothing() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    // The cast is in the air, and the client would answer anything sent on
    // top of it with Unavailable.
    rig.automation.magic().is_casting = true;
    rig.automation.magic().request_answer = CastResult::Unavailable;

    assert!(rig.tick(2.5, true));
    assert!(rig.tick(2.5, true));

    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);
    assert!(rig.automation.posted_system_messages.is_empty());
}

/// The client answers `is_casting` false while a cast waits for the
/// server, and the server answers a recall only after its windup, gesture
/// and a second of recoil (2.4 to 4.3 s on the local server), past the
/// two-second retry. The recall is not sent again in that time: the
/// retries were refused as too busy, each refusal sent another, and the
/// one on its way when the first cast ended was cast too (register NAV-5;
/// the owner's report of 2026-10-15).
/// C# test (pending eriknihlen/openac-mosstank#16): `NavigationTests.ARecallTheServerHasNotAnsweredIsNotSentAgain`.
#[test]
fn a_recall_the_server_has_not_answered_is_not_sent_again() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    // Four seconds without an answer, and the client says nothing is
    // casting.
    for _ in 0..80 {
        assert!(rig.tick(0.05, true));
    }
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);
    assert!(rig.automation.posted_system_messages.is_empty());

    // The server's answer: the cast landed, the teleport is on its way.
    rig.automation.magic().last_completion = CastCompletion {
        revision: 1,
        spell_id: 1635,
        target_id: 0,
        weenie_error: 0,
    };
    for _ in 0..40 {
        assert!(rig.tick(0.05, true));
    }
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    // Portal space, then the arrival somewhere else, ends the waypoint.
    rig.automation.navigation_snapshot.is_portal_space = true;
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot = snapshot(position(4.0, 0.0));
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.index(), 1);
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);
}

/// An answer that is an error (here the server's "too busy") sends the
/// recall again at once, as one with the client's casting flag does.
/// C# test (pending eriknihlen/openac-mosstank#16): `NavigationTests.ARecallAnsweredWithAnErrorIsSentAgainAtOnce`.
#[test]
fn a_recall_answered_with_an_error_is_sent_again_at_once() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    assert!(rig.tick(0.5, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    rig.automation.magic().last_completion = CastCompletion {
        revision: 1,
        spell_id: 1635,
        target_id: 0,
        weenie_error: 0x1D,
    };
    assert!(rig.tick(0.05, true));

    assert_eq!(rig.automation.magic().requested_spell_ids, [1635, 1635]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARecallThatCompletedWithoutPortalSpaceRecastsAtOnce
#[test]
fn a_recall_that_completed_without_portal_space_recasts_at_once() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    rig.automation.magic().is_casting = true;
    assert!(rig.tick(0.5, true));
    assert_eq!(rig.automation.magic().requested_spell_ids.len(), 1);

    // The cast ends with a server error and no portal space.
    rig.automation.magic().is_casting = false;
    rig.automation.magic().last_completion = CastCompletion {
        revision: 1,
        spell_id: 1635,
        target_id: 0,
        weenie_error: 0x1D,
    };

    assert!(rig.tick(0.05, true));

    assert_eq!(rig.automation.magic().requested_spell_ids, [1635, 1635]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARecallThatLandedIsNotCastAgainWhileTheTeleportStarts
#[test]
fn a_recall_that_landed_is_not_cast_again_while_the_teleport_starts() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    rig.automation.magic().is_casting = true;
    assert!(rig.tick(0.5, true));
    rig.automation.magic().is_casting = false;
    // The cast ends cleanly; portal space has not come yet.
    rig.automation.magic().last_completion = CastCompletion {
        revision: 1,
        spell_id: 1635,
        target_id: 0,
        weenie_error: 0,
    };

    // Four seconds: inside the 4.5 s a clean cast is given to show its
    // teleport (the C# test waited five, which now sees the recast; the
    // owner's ruling of 2026-10-16, "Do as vtank").
    for _ in 0..80 {
        assert!(rig.tick(0.05, true));
    }

    assert_eq!(rig.automation.magic().requested_spell_ids.len(), 1);
}

/// The server refuses a recall cast within 3.5 s of the last teleport, and
/// the cast still ends with a clean "done": only a chat line says so. Like
/// VirindiTank, the waypoint casts again once the cast's result is overdue
/// (4.5 s) and is done when the teleport finally comes. The owner's ruling
/// of 2026-10-16: "Do as vtank, both in Rust and C#."
/// C# test (pending eriknihlen/openac-mosstank#16): `NavigationTests.ARecallTheServerRefusedIsCastAgainAndDoneWhenTheTeleportComes`.
#[test]
fn a_recall_the_server_refused_is_cast_again_and_done_when_the_teleport_comes() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    // A clean answer, and no portal space after it.
    rig.automation.magic().last_completion = CastCompletion {
        revision: 1,
        spell_id: 1635,
        target_id: 0,
        weenie_error: 0,
    };
    for _ in 0..80 {
        assert!(rig.tick(0.05, true));
    }
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);
    assert_eq!(rig.status(), "Recall: Lifestone Recall.");

    assert!(rig.tick(0.6, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635, 1635]);
    assert_eq!(rig.status(), "Recall: Lifestone Recall (retrying).");

    // This one lands, and the teleport comes.
    rig.automation.magic().last_completion = CastCompletion {
        revision: 2,
        spell_id: 1635,
        target_id: 0,
        weenie_error: 0,
    };
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot.is_portal_space = true;
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot = snapshot(position(4.0, 0.0));
    assert!(rig.tick(0.05, true));

    assert_eq!(rig.index(), 1);
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635, 1635]);
    assert!(rig.automation.posted_system_messages.is_empty());
}

/// VirindiTank's recall waypoint has no timeout, and neither has this one:
/// a recall that never teleports is cast again and again, and the route
/// stays on it (the old 30 s give-up walked off toward the next waypoint,
/// 22 km away live; the owner's ruling of 2026-10-16).
/// C# test (pending eriknihlen/openac-mosstank#16): `NavigationTests.ARecallThatNeverTeleportsIsNeverGivenUp`.
#[test]
fn a_recall_that_never_teleports_is_never_given_up() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    for revision in 1..=20 {
        rig.automation.magic().last_completion = CastCompletion {
            revision,
            spell_id: 1635,
            target_id: 0,
            weenie_error: 0,
        };
        assert!(rig.tick(0.05, true));
        assert!(rig.tick(5.0, true));
    }

    assert_eq!(rig.index(), 0);
    assert_eq!(rig.automation.magic().requested_spell_ids.len(), 21);
    assert_eq!(rig.status(), "Recall: Lifestone Recall (retrying).");
}

/// A recall the server never answers is held (register NAV-5) only as long
/// as VirindiTank's caster would hold it, 5 s to be spoken and 4.5 s for its
/// result; then it is sent again rather than parking the route for good.
/// C# test (pending eriknihlen/openac-mosstank#16): `NavigationTests.ARecallNeverAnsweredIsSentAgainWhenVTanksCasterWouldGiveItUp`.
#[test]
fn a_recall_never_answered_is_sent_again_when_vtanks_caster_would_give_it_up() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    for _ in 0..180 {
        assert!(rig.tick(0.05, true));
    }
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);

    assert!(rig.tick(0.6, true));
    assert_eq!(rig.automation.magic().requested_spell_ids, [1635, 1635]);
}

/// While the client holds a cast up, nothing more is sent however long it
/// takes.
/// C# test (pending eriknihlen/openac-mosstank#16): `NavigationTests.ARecallIsNotSentAgainWhileTheHostHoldsACastUp`.
#[test]
fn a_recall_is_not_sent_again_while_the_client_holds_a_cast_up() {
    let mut rig = recall_ready_to_cast(CastResult::Sent);

    assert!(rig.tick(0.05, true));
    rig.automation.magic().is_casting = true;
    for _ in 0..40 {
        assert!(rig.tick(0.5, true));
    }

    assert_eq!(rig.automation.magic().requested_spell_ids, [1635]);
    assert_eq!(rig.index(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AGenuineRecallRefusalOfMissingComponentsStillPostsItsOneLine
#[test]
fn a_genuine_recall_refusal_of_missing_components_still_posts_its_one_line() {
    let mut rig = recall_ready_to_cast(CastResult::MissingComponents);

    assert!(rig.tick(0.05, true));
    assert!(rig.tick(2.5, true));

    assert!(rig.automation.magic().cast_spell_ids.is_empty());
    assert_eq!(
        rig.automation.posted_system_messages,
        ["[MossTank] Recall 'Lifestone Recall' refused: MissingComponents."]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RecallWaitsUntilTheCharacterHasStopped
#[test]
fn recall_waits_until_the_character_has_stopped() {
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall_spell_id = 48;
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.magic();
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![
            recall,
            waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
        ],
    );

    // Standing still from the first tick, so it casts.
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().cast_spell_ids, [48]);
    rig.automation.magic().cast_spell_ids.clear();
    // The cast fizzles, so the recall is to be sent again (one still
    // unanswered is not, register NAV-5).
    rig.automation.magic().last_completion = CastCompletion {
        revision: 1,
        spell_id: 48,
        target_id: 0,
        weenie_error: 0x402,
    };

    // A retry is due, but the character has moved more than the tolerance
    // since the last tick, so the recall waits.
    rig.automation.navigation_snapshot = snapshot(position(0.0, 3.0 / 240.0));
    assert!(rig.tick(2.5, true));
    assert!(rig.automation.magic().cast_spell_ids.is_empty());

    // Stopped again: the retry goes out.
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.magic().cast_spell_ids, [48]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: RecallWaypointResolvesItsNameWhenItRunsAndCastsTheRecall
#[test]
fn recall_waypoint_resolves_its_name_when_it_runs_and_casts_the_recall() {
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall_spell_name = "Lifestone Recall".to_owned();
    let mut rig = single_recall(recall);

    assert!(rig.tick(0.1, true));

    assert!(rig.automation.magic().cast_spell_ids.contains(&1635));
    assert_eq!(
        rig.settings.waypoints[0].recall,
        RouteRecallKind::LifestoneRecall
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: UnknownRecallNameIsAnnouncedInChatOnce
#[test]
fn unknown_recall_name_is_announced_in_chat_once() {
    let mut recall = waypoint(RouteWaypointType::Recall, position(0.0, 0.0));
    recall.recall_spell_name = "Not A Recall At All".to_owned();
    let mut rig = rig(
        FakeAutomation::at(snapshot(position(0.0, 0.0))),
        RouteMode::Circular,
        vec![recall],
    );

    assert!(rig.tick(0.1, true));
    assert!(rig.tick(0.1, true));

    assert_eq!(
        rig.automation.posted_system_messages,
        ["[MossTank] Recall spell 'Not A Recall At All' not found; skipping waypoint."]
    );
}

/// A point a metre ahead (inside the 1.5 m creep band, so the mover walks
/// and asks for magic mode on every tick it wants to walk), a character in
/// peace mode with a wand in hand, and the macro's gate bound
/// (`ArrivalOverride`).
fn arrival_override(minimum_distance_meters: f64, idle_peace_mode: bool) -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0)));
    automation.combat_mode = CombatMode::Peace;
    automation.equipment_items = vec![EquipmentItem {
        object_id: 800,
        name: "Recovery Wand".to_owned(),
        item_type: 0x0000_8000,
        valid_locations: 0x0010_0000,
        equipped_location: 0x0010_0000,
        container_id: 0,
        wielder_id: 1,
        object_class: ObjectClass::WandStaffOrb,
        ..EquipmentItem::default()
    }];
    let mut rig = rig_min(
        automation,
        RouteMode::Circular,
        minimum_distance_meters,
        vec![
            waypoint(RouteWaypointType::Point, position(0.0, 1.0 / 240.0)),
            waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
        ],
    );
    rig.combat.idle_peace_mode = idle_peace_mode;
    rig.bind_gate();
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: LowWaypointDistanceInPeaceWarnsOnceAndForcesMagicMode
#[test]
fn low_waypoint_distance_in_peace_warns_once_and_forces_magic_mode() {
    let mut rig = arrival_override(0.5, true);

    assert!(rig.tick(1.0, true));

    // Not advanced: the arrival was refused this tick.
    assert_eq!(rig.index(), 0);
    assert_eq!(rig.automation.posted_system_messages.len(), 1);
    assert_eq!(
        rig.automation.posted_system_messages[0].replace("[MossTank] ", ""),
        LOW_WAYPOINT_DISTANCE_WARNING
    );
    assert_eq!(rig.automation.mode_requests, ["EnterMode:Magic"]);

    // The warning is posted once per approach, not once per pass.
    rig.automation.mode_requests.clear();
    assert!(rig.tick(1.0, true));
    assert_eq!(rig.automation.posted_system_messages.len(), 1);

    rig.controller.reset_once_per_run_warnings();
    assert!(rig.tick(1.0, true));
    assert_eq!(rig.automation.posted_system_messages.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: LowWaypointDistanceForcesMagicEvenWithIdlePeaceOff
#[test]
fn low_waypoint_distance_forces_magic_even_with_idle_peace_off() {
    let mut rig = arrival_override(0.5, false);

    assert!(rig.tick(1.0, true));

    assert!(rig.automation.posted_system_messages.is_empty());
    assert_eq!(rig.automation.mode_requests, ["EnterMode:Magic"]);
    assert_eq!(rig.index(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: OrdinaryWaypointDistanceArrivesWithoutTouchingCombatMode
#[test]
fn ordinary_waypoint_distance_arrives_without_touching_combat_mode() {
    let mut rig = arrival_override(2.0, true);

    assert!(rig.tick(1.0, true));

    assert_eq!(rig.index(), 1);
    assert!(rig.automation.posted_system_messages.is_empty());
    assert!(rig.automation.mode_requests.is_empty());
}
