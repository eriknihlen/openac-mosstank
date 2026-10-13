//! The debuff chain, the debuff spells and the debuff tracker.

use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use mosstank_combat::debuff::{
    self, CombatDebuffStep, DebuffIdentity, DebuffTracker, build_chain, has_extra_vulnerability,
};
use mosstank_data::damage::MonsterDamageType as D;
use mosstank_settings::monster::{MonsterActionFlags as F, MonsterRuleActions};

fn actions(flags: F, damage_type: D, extra: D) -> MonsterRuleActions {
    MonsterRuleActions {
        flags,
        damage_type,
        extra_vulnerability: extra,
        ..MonsterRuleActions::default()
    }
}

fn identities(steps: &[CombatDebuffStep]) -> Vec<DebuffIdentity> {
    steps.iter().map(|step| step.identity).collect()
}

fn id(flag: F, damage: D) -> DebuffIdentity {
    DebuffIdentity::new(flag, damage)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: ChainOrderIsAuthenticTwelveSteps
#[test]
fn chain_order_is_authentic_twelve_steps() {
    let steps = build_chain(
        &actions(
            F::YIELD
                | F::WEAKENING_CURSE
                | F::FESTERING_CURSE
                | F::CORRUPTION
                | F::DESTRUCTIVE_CURSE
                | F::CORROSION
                | F::IMPERIL
                | F::VULNERABILITY
                | F::GRAVITY_WELL
                | F::BROADSIDE
                | F::FESTER,
            D::Fire,
            D::Acid,
        ),
        D::Fire,
        D::Acid,
    );

    assert_eq!(
        identities(&steps),
        [
            id(F::YIELD, D::Auto),
            id(F::WEAKENING_CURSE, D::Auto),
            id(F::FESTERING_CURSE, D::Auto),
            id(F::CORRUPTION, D::Auto),
            id(F::DESTRUCTIVE_CURSE, D::Auto),
            id(F::CORROSION, D::Auto),
            id(F::IMPERIL, D::Auto),
            id(F::VULNERABILITY, D::Fire),
            id(F::VULNERABILITY, D::Acid),
            id(F::GRAVITY_WELL, D::Auto),
            id(F::BROADSIDE, D::Auto),
            id(F::FESTER, D::Auto),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: TheExtraVulnerabilityIsNotGatedOnTheVulnerabilityColumn
#[test]
fn the_extra_vulnerability_is_not_gated_on_the_vulnerability_column() {
    let actions = actions(F::ATTACK, D::Auto, D::Acid);
    let extra = id(F::VULNERABILITY, D::Acid);

    assert_eq!(
        identities(&build_chain(&actions, D::Auto, D::Acid)),
        [extra]
    );
    assert!(debuff::required(&actions).contains(&extra));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: AnUnresolvedExtraVulnerabilityAddsNoStep
#[test]
fn an_unresolved_extra_vulnerability_adds_no_step() {
    assert!(!has_extra_vulnerability(D::None));
    assert!(!has_extra_vulnerability(D::Auto));
    assert!(has_extra_vulnerability(D::Acid));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: OnlyCorruptionDestructiveCurseAndCorrosionAreZeroTolerance
#[test]
fn only_corruption_destructive_curse_and_corrosion_are_zero_tolerance() {
    let steps = build_chain(&actions(F(0x3FFF), D::Fire, D::None), D::Fire, D::None);

    let zero: Vec<F> = steps
        .iter()
        .filter(|step| step.zero_tolerance)
        .map(|step| step.identity.flag)
        .collect();
    assert_eq!(zero, [F::CORRUPTION, F::DESTRUCTIVE_CURSE, F::CORROSION]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: NaturalVulnUsesTheAttackElementAndExtraVulnIsIndependentOfTheColumn
#[test]
fn natural_vuln_uses_the_attack_element_and_extra_vuln_is_independent_of_the_column() {
    let column_off = build_chain(&actions(F::NONE, D::Auto, D::None), D::Fire, D::Acid);
    assert_eq!(identities(&column_off), [id(F::VULNERABILITY, D::Acid)]);

    let column_on = build_chain(
        &actions(F::VULNERABILITY, D::Auto, D::None),
        D::Cold,
        D::None,
    );
    assert_eq!(identities(&column_on), [id(F::VULNERABILITY, D::Cold)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: AnElementlessPlanHasNoNaturalVulnStep
#[test]
fn an_elementless_plan_has_no_natural_vuln_step() {
    let steps = build_chain(
        &actions(F::VULNERABILITY, D::Auto, D::None),
        D::None,
        D::None,
    );
    assert!(steps.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: ChooseTakesTheFirstDueStepAndStops
#[test]
fn choose_takes_the_first_due_step_and_stops() {
    let steps = build_chain(
        &actions(F::YIELD | F::IMPERIL | F::FESTER, D::Auto, D::None),
        D::None,
        D::None,
    );

    let mut asked = Vec::new();
    let chosen = debuff::choose(&steps, |step| {
        asked.push(step.identity.flag);
        step.identity.flag != F::YIELD
    });

    assert_eq!(chosen.map(|step| step.identity.flag), Some(F::IMPERIL));
    assert_eq!(asked, [F::YIELD, F::IMPERIL]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatDebuffChainTests.cs :: NeedsDebuffIsTheSameTwelveTestsAsAFlatOr
#[test]
fn needs_debuff_is_the_same_twelve_tests_as_a_flat_or() {
    let steps = build_chain(&actions(F::FESTER, D::Auto, D::None), D::None, D::None);

    assert!(debuff::needs_debuff(&steps, |_| true));
    assert!(!debuff::needs_debuff(&steps, |_| false));
    assert!(!debuff::needs_debuff(&[], |_| true));
}

fn spell(id: u32, name: &str, tier: i32, duration: f32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: id,
        tier,
        difficulty: 300,
        mana_cost: 20,
        duration_seconds: duration,
        school: 33,
        is_debuff: true,
        is_offensive: true,
        ..SpellInfo::default()
    }
}

fn completion(revision: i64, spell_id: u32, target_id: u32, weenie_error: u32) -> CastCompletion {
    CastCompletion {
        revision,
        spell_id,
        target_id,
        weenie_error,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: ClassifierMapsEveryVtankMonsterDebuffColumn
#[test]
fn classifier_maps_every_vtank_monster_debuff_column() {
    let cases: [(&str, i32, D); 17] = [
        ("Fester Other VII", 1 << 0, D::Auto),
        ("Broadside of a Barn", 1 << 1, D::Auto),
        ("Gravity Well", 1 << 2, D::Auto),
        ("Incantation of Imperil Other", 1 << 3, D::Auto),
        ("Magic Yield Other VI", 1 << 4, D::Auto),
        ("Incantation of Fire Vulnerability Other", 1 << 5, D::Fire),
        ("Acid Vulnerability Other VI", 1 << 5, D::Acid),
        ("Blade Vulnerability Other VI", 1 << 5, D::Slash),
        ("Bludgeoning Vulnerability Other VI", 1 << 5, D::Bludgeon),
        ("Cold Vulnerability Other VI", 1 << 5, D::Cold),
        ("Lightning Vulnerability Other VI", 1 << 5, D::Electric),
        ("Piercing Vulnerability Other VI", 1 << 5, D::Pierce),
        ("Weakening Curse VII", 1 << 9, D::Auto),
        ("Festering Curse VII", 1 << 10, D::Auto),
        ("Corruption VII", 1 << 11, D::Auto),
        ("Destructive Curse VII", 1 << 12, D::Auto),
        ("Corrosion VII", 1 << 13, D::Auto),
    ];
    for (name, flag, damage) in cases {
        let (identity, _) = debuff::classify(&spell(1, name, 8, 30.0)).expect(name);
        assert_eq!(identity.flag, F(flag), "{name}");
        assert_eq!(identity.damage_type, damage, "{name}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: NoLureIsACreatureVulnerability
#[test]
fn no_lure_is_a_creature_vulnerability() {
    for name in [
        "Incantation of Lure Blade",
        "Acid Lure VI",
        "Blade Lure III",
        "Bludgeon Lure VI",
        "Flame Lure III",
        "Frost Lure VI",
        "Lightning Lure III",
        "Piercing Lure VI",
        "Incantation of Flame Lure",
    ] {
        assert!(
            debuff::classify(&spell(1, name, 8, 30.0)).is_none(),
            "{name}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: TrackerWaitsForMatchingSuccessfulServerReceipt
#[test]
fn tracker_waits_for_matching_successful_server_receipt() {
    let mut tracker = DebuffTracker::new();
    let identity = id(F::IMPERIL, D::Auto);
    let spell = spell(10, "Imperil Other VII", 8, 60.0);
    tracker.begin(99, identity, spell.clone(), 10.0, 4);

    assert!(!tracker.observe(&completion(5, 11, 99, 0), 12.0).completed);
    assert!(tracker.has_pending());

    let failed = tracker.observe(&completion(6, 10, 99, 0x0402), 13.0);
    assert!(failed.completed);
    assert!(!failed.succeeded);
    assert!(tracker.is_due(99, identity, &spell, 13.0, 5.0));

    tracker.begin(99, identity, spell.clone(), 14.0, 6);
    let succeeded = tracker.observe(&completion(7, 10, 99, 0), 15.0);
    assert!(succeeded.succeeded);
    assert!(!tracker.is_due(99, identity, &spell, 69.0, 5.0));
    assert!(tracker.is_due(99, identity, &spell, 70.0, 5.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: ZeroToleranceStepsGetNoPrecastWindow
#[test]
fn zero_tolerance_steps_get_no_precast_window() {
    let mut tracker = DebuffTracker::new();
    let identity = id(F::CORROSION, D::Auto);
    let spell = SpellInfo {
        is_damage_over_time: true,
        ..spell(10, "Corrosion VII", 8, 60.0)
    };
    tracker.begin(99, identity, spell.clone(), 0.0, 0);
    tracker.observe(&completion(1, 10, 99, 0), 1.0);

    assert!(!tracker.is_due(99, identity, &spell, 60.0, 0.0));
    assert!(tracker.is_due(99, identity, &spell, 61.0, 0.0));

    // With the ordinary precast window the same timer is due early.
    assert!(tracker.is_due(99, identity, &spell, 45.0, 20.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: FakeImperilCreatesVtankThreeThousandSecondLocalMarker
#[test]
fn fake_imperil_creates_vtank_three_thousand_second_local_marker() {
    let mut tracker = DebuffTracker::new();
    let identity = id(F::IMPERIL, D::Auto);
    let learned = spell(10, "Imperil Other VII", 7, 60.0);

    tracker.record_fake_imperil(99, 10.0);

    assert!(!tracker.is_due(99, identity, &learned, 3009.0, 0.0));
    assert!(tracker.is_due(99, identity, &learned, 3010.0, 0.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: ARemoteCastIsAppliedForItsRemainingSecondsOnTheLocalClock
#[test]
fn a_remote_cast_is_applied_for_its_remaining_seconds_on_the_local_clock() {
    let mut tracker = DebuffTracker::new();
    let identity = id(F::VULNERABILITY, D::Fire);
    let spell = spell(10, "Fire Vulnerability Other VII", 7, 30.0);

    assert!(tracker.record_remote(99, identity, &spell, 100.0, 30.0));

    assert!(tracker.is_applied(99, identity, Some(&spell), 129.9));
    assert!(!tracker.is_applied(99, identity, Some(&spell), 130.0));
    assert!(!tracker.is_due(99, identity, &spell, 129.9, 0.0));
    assert!(tracker.is_due(99, identity, &spell, 130.0, 0.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: ARemoteCastWithNothingLeftIsNotRecorded
#[test]
fn a_remote_cast_with_nothing_left_is_not_recorded() {
    for seconds_remaining in [0.0, -5.0, f64::NAN] {
        let mut tracker = DebuffTracker::new();
        let identity = id(F::IMPERIL, D::Auto);
        let spell = spell(10, "Imperil Other VII", 7, 30.0);

        assert!(!tracker.record_remote(99, identity, &spell, 10.0, seconds_remaining));
        assert!(!tracker.is_applied(99, identity, Some(&spell), 10.0));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DebuffSchedulerTests.cs :: ARemoteRecordMergesByHigherTierAndLaterExpiry
#[test]
#[allow(clippy::cast_possible_truncation)]
fn a_remote_record_merges_by_higher_tier_and_later_expiry() {
    let cases: [(i32, f64, i32, f64, i32, f64, bool); 6] = [
        (7, 60.0, 5, 20.0, 7, 60.0, false),
        (7, 60.0, 5, 100.0, 7, 110.0, true),
        (5, 60.0, 7, 20.0, 7, 60.0, true),
        (5, 60.0, 7, 100.0, 7, 110.0, true),
        (7, 60.0, 7, 20.0, 7, 60.0, false),
        (7, 60.0, 7, 100.0, 7, 110.0, true),
    ];
    for (local_tier, local_expires, remote_tier, remote_remaining, tier, expires, changed) in cases
    {
        let mut tracker = DebuffTracker::new();
        let identity = id(F::IMPERIL, D::Auto);
        let local = spell(
            10,
            "Imperil Other (local)",
            local_tier,
            local_expires as f32,
        );
        tracker.record_applied(99, identity, &local, 0.0);
        let remote = spell(20, "Imperil Other (remote)", remote_tier, 30.0);

        assert_eq!(
            tracker.record_remote(99, identity, &remote, 10.0, remote_remaining),
            changed
        );

        let probe = spell(30, "Imperil Other (probe)", tier, 30.0);
        let stronger = spell(31, "Imperil Other (stronger)", tier + 1, 30.0);
        assert!(tracker.is_applied(99, identity, Some(&probe), expires - 0.5));
        assert!(!tracker.is_applied(99, identity, Some(&probe), expires));
        assert!(!tracker.is_applied(99, identity, Some(&stronger), expires - 0.5));
        assert!(!tracker.is_due(99, identity, &probe, expires - 0.5, 0.0));
    }
}
