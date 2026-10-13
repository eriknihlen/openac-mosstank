//! The macro scheduler, its rule table and adapters, and its debug log.
//! Ported from the C#'s `MacroSchedulerTests`, `MacroRuleAdapterTests` and
//! `MacroDebugLogTests`.

use std::cell::RefCell;
use std::rc::Rc;

use mosstank_meta::debug_log::{MAXIMUM_BYTES, MacroDebugLog};
use mosstank_meta::scheduler::{
    AbsentMacroRule, ControllerMacroRule, HEARTBEAT_SECONDS, MACRO_RULE_TABLE, MacroContext,
    MacroIndependentSlot, MacroLogChannel, MacroPassContext, MacroRule, MacroRulePreChain,
    MacroRuleProvider, MacroRuleSentinel, MacroRuleSlot, MacroScheduler, build,
};
use mosstank_meta::testing::SharedStorage;
use mosstank_stores::storage::PluginStorage;

/// The scheduler's hooks as a test binds them.
#[derive(Default)]
struct Cx {
    log: Option<Vec<(MacroLogChannel, String)>>,
    meta: Option<Vec<f64>>,
    started: i32,
    lock_suffix: Option<String>,
}

impl MacroContext for Cx {
    fn has_meta_pass(&self) -> bool {
        self.meta.is_some()
    }

    fn meta_pass(&mut self, elapsed_seconds: f64) {
        if let Some(seen) = self.meta.as_mut() {
            seen.push(elapsed_seconds);
        }
    }

    fn pass_starting(&mut self) {
        self.started += 1;
    }

    fn has_log(&self) -> bool {
        self.log.is_some()
    }

    fn log(&mut self, channel: MacroLogChannel, message: &str) {
        if let Some(log) = self.log.as_mut() {
            log.push((channel, message.to_owned()));
        }
    }

    fn lock_state_suffix(&mut self) -> String {
        self.lock_suffix.clone().unwrap_or_default()
    }
}

impl Cx {
    fn logging() -> Self {
        Self {
            log: Some(Vec::new()),
            ..Self::default()
        }
    }

    fn rule_info(&self) -> Vec<(MacroLogChannel, String)> {
        self.log
            .iter()
            .flatten()
            .filter(|(channel, _)| *channel == MacroLogChannel::RuleInfo)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Default)]
struct ProbeState {
    valid: bool,
    reason: Option<String>,
    detail: Option<String>,
    requires_can_act: bool,
    can_act_seen: Vec<bool>,
    valid_now_calls: usize,
    running_writes: Vec<bool>,
    running: bool,
    elapsed_seen: Vec<f64>,
    started_when_asked: Vec<i32>,
}

/// A rule the test watches.
#[derive(Clone)]
struct Probe {
    name: String,
    state: Rc<RefCell<ProbeState>>,
}

impl Probe {
    fn new(name: &str, valid: bool) -> Self {
        Self {
            name: name.to_owned(),
            state: Rc::new(RefCell::new(ProbeState {
                valid,
                requires_can_act: true,
                ..ProbeState::default()
            })),
        }
    }

    fn rule(&self) -> Box<dyn MacroRule<Cx>> {
        Box::new(self.clone())
    }

    fn set_valid(&self, valid: bool) {
        self.state.borrow_mut().valid = valid;
    }

    fn running(&self) -> bool {
        self.state.borrow().running
    }

    fn calls(&self) -> usize {
        self.state.borrow().valid_now_calls
    }

    fn writes(&self) -> Vec<bool> {
        self.state.borrow().running_writes.clone()
    }
}

impl MacroRule<Cx> for Probe {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn valid_now(&mut self, cx: &mut Cx, context: MacroPassContext) -> bool {
        let mut state = self.state.borrow_mut();
        state.valid_now_calls += 1;
        state.can_act_seen.push(context.can_act);
        state.elapsed_seen.push(context.elapsed_seconds);
        state.started_when_asked.push(cx.started);
        if state.requires_can_act && !context.can_act {
            return false;
        }
        state.valid
    }

    fn running(&self) -> bool {
        self.state.borrow().running
    }

    fn set_running(&mut self, _cx: &mut Cx, running: bool) {
        let mut state = self.state.borrow_mut();
        state.running_writes.push(running);
        state.running = running;
    }

    fn running_detail(&mut self, _cx: &mut Cx) -> Option<String> {
        self.state.borrow().detail.clone()
    }

    fn decline_reason(&mut self, _cx: &mut Cx) -> Option<String> {
        self.state.borrow().reason.clone()
    }
}

fn scheduler(main: &[&Probe]) -> MacroScheduler<Cx> {
    MacroScheduler::new(
        main.iter().map(|probe| probe.rule()).collect(),
        Vec::new(),
        Vec::new(),
    )
}

fn started(cx: &mut Cx, main: &[&Probe]) -> MacroScheduler<Cx> {
    let mut scheduler = scheduler(main);
    scheduler.start(cx);
    scheduler
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: AStoppedMacroRunsOnlyItsOwnShortList
#[test]
fn a_stopped_macro_runs_only_its_own_short_list() {
    let mut cx = Cx::default();
    let main = Probe::new("Attack", true);
    let when_off = Probe::new("RefillWieldedMana", true);
    let mut scheduler = MacroScheduler::new(vec![main.rule()], Vec::new(), vec![when_off.rule()]);

    assert!(scheduler.advance(&mut cx, HEARTBEAT_SECONDS));
    assert_eq!(main.calls(), 0);
    assert_eq!(when_off.calls(), 1);
    assert!(when_off.running());

    scheduler.start(&mut cx);
    assert!(!when_off.running());
    assert!(scheduler.advance(&mut cx, HEARTBEAT_SECONDS));
    assert_eq!(main.calls(), 1);
    assert_eq!(when_off.calls(), 1);

    scheduler.stop(&mut cx);
    assert!(scheduler.advance(&mut cx, HEARTBEAT_SECONDS));
    assert_eq!(main.calls(), 1);
    assert_eq!(when_off.calls(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: AShutGateLeavesTheStoppedPassEmpty
#[test]
fn a_shut_gate_leaves_the_stopped_pass_empty() {
    let mut cx = Cx::default();
    let when_off = Probe::new("RefillWieldedMana", false);
    let mut scheduler = MacroScheduler::new(
        vec![Probe::new("Attack", false).rule()],
        Vec::new(),
        vec![when_off.rule()],
    );

    assert!(scheduler.advance(&mut cx, HEARTBEAT_SECONDS));

    assert_eq!(when_off.calls(), 1);
    assert!(!when_off.running());
}

struct Provider;

impl MacroRuleProvider<Cx> for Provider {
    fn create(&mut self, slot: MacroRuleSlot) -> Box<dyn MacroRule<Cx>> {
        Probe::new(slot.name(), false).rule()
    }

    fn create_independent(&mut self, slot: MacroIndependentSlot) -> Box<dyn MacroRule<Cx>> {
        Probe::new(slot.name(), false).rule()
    }
}

const VTANK_ORDER: [&str; 66] = [
    "Sentinel START",
    "SplitPeasCritical",
    "CraftFoodCritical",
    "RechargeSelfNormal",
    "RefillWieldedMana",
    "BuffSelfNormal",
    "SplitPeasNormal",
    "Sentinel POSTBUFF",
    "DispelSelf",
    "UseDispelItem",
    "UseHealersHeart",
    "RechargeOther",
    "DispelAllies",
    "CraftFood",
    "RefillPetChargesNormal",
    "Sentinel POSTHELPER",
    "FellowshipManager",
    "Sentinel POSTAUTOFELLOW",
    "OpenDoor",
    "Sentinel PREPRIORITYLOOTACTIONS",
    "ReadScrollPriority",
    "StackCramPriority",
    "SalvageItemsPriority",
    "Sentinel POSTPRIORITYLOOTACTIONS",
    "Sentinel PREPRIORITYLOOT",
    "NavigateCorpsePriority",
    "OpenCorpsePriority",
    "LootCorpsePriority",
    "CorpseWaitPriority",
    "Sentinel POSTPRIORITYLOOT",
    "Sentinel PREPRIORITYNAV",
    "NavigateRoutePriority",
    "Sentinel POSTPRIORITYNAV",
    "Sentinel PREATTACK",
    "Attack",
    "Sentinel POSTATTACK",
    "Sentinel PREIDLESTATUS",
    "SplitPeasIdle",
    "CraftFoodIdle",
    "RefillPetChargesIdle",
    "Sentinel PREIDLELOOTACTIONS",
    "ReadScrollIdle",
    "StackCramIdle",
    "SalvageItemsIdle",
    "Sentinel POSTIDLELOOTACTIONS",
    "Sentinel PREIDLELOOT",
    "NavigateCorpseIdle",
    "OpenCorpseIdle",
    "LootCorpseIdle",
    "CorpseWaitIdle",
    "Sentinel POSTIDLELOOT",
    "Sentinel PREIDLEBUFF",
    "BuffSelfIdle",
    "Sentinel POSTIDLEBUFF",
    "Sentinel PRETARGETAPPROACH",
    "NavigateMonster",
    "Sentinel POSTTARGETAPPROACH",
    "Sentinel PREIDLERECHARGE",
    "RechargeSelfNoTarget",
    "Sentinel POSTIDLERECHARGE",
    "Sentinel PRENAVROUTE",
    "NavigateRouteIdle",
    "Sentinel POSTNAVROUTE",
    "RandomHelper",
    "Sentinel END",
    "IdlePeace",
];

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: TableIsVtanksSixtySixEntriesInOrder
#[test]
fn table_is_vtanks_sixty_six_entries_in_order() {
    assert_eq!(MACRO_RULE_TABLE.len(), 66);
    let names: Vec<String> = MACRO_RULE_TABLE.iter().map(|entry| entry.name()).collect();
    assert_eq!(names, VTANK_ORDER);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: BuiltListMatchesTheTableOrder
#[test]
fn built_list_matches_the_table_order() {
    let scheduler: MacroScheduler<Cx> = build(&mut Provider);
    let names: Vec<String> = scheduler
        .main_rules()
        .iter()
        .map(|rule| rule.name())
        .collect();
    assert_eq!(names, VTANK_ORDER);
    let independent: Vec<String> = scheduler
        .independent_rules()
        .iter()
        .map(|rule| rule.name())
        .collect();
    assert_eq!(independent, ["SummonPet"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: SentinelsSitAtVtanksPositions
#[test]
fn sentinels_sit_at_vtanks_positions() {
    let sentinels: Vec<usize> = MACRO_RULE_TABLE
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.slot().is_none())
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        sentinels,
        [
            0, 7, 15, 17, 19, 23, 24, 29, 30, 32, 33, 35, 36, 40, 44, 45, 50, 51, 53, 54, 56, 57,
            59, 60, 62, 64,
        ]
    );
    assert_eq!(MACRO_RULE_TABLE[65].slot(), Some(MacroRuleSlot::IdlePeace));
    assert_eq!(MACRO_RULE_TABLE[64].name(), "Sentinel END");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: SentinelNeverWins
#[test]
fn sentinel_never_wins() {
    let mut cx = Cx::default();
    let mut sentinel = MacroRuleSentinel::new("START");
    let context = MacroPassContext::new(1.0, true);
    assert!(!MacroRule::<Cx>::valid_now(&mut sentinel, &mut cx, context));
    MacroRule::<Cx>::set_running(&mut sentinel, &mut cx, true);
    assert!(!MacroRule::<Cx>::running(&sentinel));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: FirstValidRuleWinsAndEveryOtherRuleIsToldRunningFalse
#[test]
fn first_valid_rule_wins_and_every_other_rule_is_told_running_false() {
    let mut cx = Cx::default();
    let high = Probe::new("high", false);
    let winner = Probe::new("winner", true);
    let low = Probe::new("low", true);
    let mut scheduler = started(&mut cx, &[&high, &winner, &low]);

    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(scheduler.last_executed_index(), Some(1));
    assert!(winner.running());
    assert!(!high.running());
    assert!(!low.running());
    assert_eq!(high.writes(), [false]);
    assert_eq!(low.writes(), [false]);
    assert_eq!(winner.writes(), [true]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: RulesAfterTheWinnerAreNotAskedThisPass
#[test]
fn rules_after_the_winner_are_not_asked_this_pass() {
    let mut cx = Cx::default();
    let above = Probe::new("above", false);
    let winner = Probe::new("winner", true);
    let below = Probe::new("below", true);
    let mut scheduler = started(&mut cx, &[&above, &winner, &below]);

    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(above.state.borrow().can_act_seen, [true]);
    assert_eq!(winner.state.borrow().can_act_seen, [true]);
    assert_eq!(below.calls(), 0);
    assert!(below.state.borrow().can_act_seen.is_empty());
    assert_eq!(below.writes(), [false]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ARuleNotAskedForSeveralPassesSeesTheWholeGapWhenNextAsked
#[test]
fn a_rule_not_asked_for_several_passes_sees_the_whole_gap_when_next_asked() {
    let mut cx = Cx::default();
    let upper = Probe::new("upper", true);
    let lower = Probe::new("lower", false);
    let mut scheduler = started(&mut cx, &[&upper, &lower]);

    scheduler.run_pass(&mut cx, 0.3);
    scheduler.run_pass(&mut cx, 0.3);
    scheduler.run_pass(&mut cx, 0.3);
    upper.set_valid(false);
    scheduler.run_pass(&mut cx, 0.3);

    let seen = upper.state.borrow().elapsed_seen.clone();
    assert_eq!(seen.len(), 4);
    assert!(seen.iter().all(|value| (value - 0.3).abs() < 1e-6));
    let gap = lower.state.borrow().elapsed_seen.clone();
    assert_eq!(gap.len(), 1);
    assert!((gap[0] - 1.2).abs() < 1e-6, "{gap:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: AClaimBelowTheWinnerDoesNotStealTheSlot
#[test]
fn a_claim_below_the_winner_does_not_steal_the_slot() {
    let mut cx = Cx::default();
    let winner = Probe::new("winner", true);
    let below = Probe::new("below", true);
    below.state.borrow_mut().requires_can_act = false;
    let mut scheduler = started(&mut cx, &[&winner, &below]);

    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(scheduler.last_executed_index(), Some(0));
    assert!(!below.running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ARuleThatBecomesInvalidIsToldRunningFalseOnTheNextPass
#[test]
fn a_rule_that_becomes_invalid_is_told_running_false_on_the_next_pass() {
    let mut cx = Cx::default();
    let rule = Probe::new("rule", true);
    let mut scheduler = started(&mut cx, &[&rule]);

    scheduler.run_pass(&mut cx, 1.0);
    assert!(rule.running());

    rule.set_valid(false);
    scheduler.run_pass(&mut cx, 1.0);

    assert!(!rule.running());
    assert_eq!(rule.writes(), [true, false]);
    assert_eq!(scheduler.last_executed_index(), None);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: StopTearsDownEveryRule
#[test]
fn stop_tears_down_every_rule() {
    let mut cx = Cx::default();
    let main = Probe::new("main", true);
    let independent = Probe::new("independent", true);
    let mut scheduler =
        MacroScheduler::new(vec![main.rule()], vec![independent.rule()], Vec::new());
    scheduler.start(&mut cx);
    scheduler.run_pass(&mut cx, 1.0);
    assert!(main.running());
    assert!(independent.running());

    scheduler.stop(&mut cx);

    assert!(!main.running());
    assert!(!independent.running());
    assert!(!scheduler.is_running());
    assert_eq!(scheduler.last_executed_index(), None);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: HeartbeatRunsOnePassEveryTwoHundredNinetyThreeMilliseconds
#[test]
fn heartbeat_runs_one_pass_every_two_hundred_ninety_three_milliseconds() {
    let mut cx = Cx::default();
    let rule = Probe::new("rule", false);
    let mut scheduler = scheduler(&[&rule]);

    // Stopped: no pass at all, however much time goes by.
    assert!(!scheduler.advance(&mut cx, 10.0));
    assert_eq!(rule.calls(), 0);

    scheduler.start(&mut cx);
    assert!(scheduler.advance(&mut cx, 0.0));
    assert_eq!(scheduler.pass_count(), 1);

    // Then the 293 ms heartbeat, re-armed on every fire.
    assert!(!scheduler.advance(&mut cx, 0.1));
    assert!(!scheduler.advance(&mut cx, 0.1));
    assert!(!scheduler.advance(&mut cx, 0.09));
    assert_eq!(scheduler.pass_count(), 1);
    assert!(scheduler.advance(&mut cx, 0.01));
    assert_eq!(scheduler.pass_count(), 2);
    assert!(!scheduler.advance(&mut cx, 0.1));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PassElapsedIsAccumulatedAcrossFrames
#[test]
fn pass_elapsed_is_accumulated_across_frames() {
    let mut cx = Cx::default();
    let recorder = Probe::new("recorder", false);
    recorder.state.borrow_mut().requires_can_act = false;
    let mut scheduler = scheduler(&[&recorder]);
    scheduler.start(&mut cx);
    scheduler.advance(&mut cx, 0.0);
    recorder.state.borrow_mut().elapsed_seen.clear();

    scheduler.advance(&mut cx, 0.1);
    scheduler.advance(&mut cx, 0.1);
    scheduler.advance(&mut cx, 0.1);

    let seen = recorder.state.borrow().elapsed_seen.clone();
    assert_eq!(seen.len(), 1);
    assert!((seen[0] - 0.3).abs() < 1e-6);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PassStartingFiresBeforeTheRulesOnEveryPass
#[test]
fn pass_starting_fires_before_the_rules_on_every_pass() {
    let mut cx = Cx::default();
    let probe = Probe::new("probe", false);
    let mut scheduler = started(&mut cx, &[&probe]);

    scheduler.run_pass(&mut cx, 0.3);
    assert_eq!(cx.started, 1);
    assert_eq!(probe.state.borrow().started_when_asked, [1]);

    scheduler.run_pass(&mut cx, 0.3);
    assert_eq!(cx.started, 2);
    assert_eq!(probe.state.borrow().started_when_asked, [1, 2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PokeForcesAnImmediatePass
#[test]
fn poke_forces_an_immediate_pass() {
    let mut cx = Cx::default();
    let rule = Probe::new("rule", false);
    let mut scheduler = started(&mut cx, &[&rule]);
    scheduler.advance(&mut cx, 0.0);
    assert_eq!(scheduler.pass_count(), 1);

    assert!(!scheduler.advance(&mut cx, 0.05));
    scheduler.poke();
    assert!(scheduler.advance(&mut cx, 0.001));
    assert_eq!(scheduler.pass_count(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PokeWhileStoppedIsIgnored
#[test]
fn poke_while_stopped_is_ignored() {
    let mut cx = Cx::default();
    let mut scheduler = scheduler(&[&Probe::new("rule", false)]);
    scheduler.poke();
    assert!(!scheduler.advance(&mut cx, 0.0));
    assert_eq!(scheduler.pass_count(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: SuspensionSkipsTheMainTrackButNotTheIndependentTrack
#[test]
fn suspension_skips_the_main_track_but_not_the_independent_track() {
    let mut cx = Cx::default();
    let main = Probe::new("main", true);
    let independent = Probe::new("independent", true);
    let mut scheduler =
        MacroScheduler::new(vec![main.rule()], vec![independent.rule()], Vec::new());
    scheduler.start(&mut cx);

    scheduler.suspend();
    assert!(scheduler.is_suspended());
    scheduler.run_pass(&mut cx, 1.0);

    assert!(independent.running());
    assert!(!main.running());
    assert_eq!(main.calls(), 0);

    scheduler.resume();
    assert!(!scheduler.is_suspended());
    scheduler.run_pass(&mut cx, 1.0);
    assert!(main.running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ReleasingTheLastSuspensionPokesThePass
#[test]
fn releasing_the_last_suspension_pokes_the_pass() {
    let mut cx = Cx::default();
    let main = Probe::new("main", true);
    let mut scheduler = started(&mut cx, &[&main]);
    scheduler.advance(&mut cx, 0.0);
    assert_eq!(scheduler.pass_count(), 1);

    scheduler.suspend();
    scheduler.suspend();
    scheduler.resume();
    assert!(scheduler.is_suspended());
    assert!(!scheduler.advance(&mut cx, 0.01));

    scheduler.resume();
    assert!(!scheduler.is_suspended());
    assert!(scheduler.advance(&mut cx, 0.01));
    assert_eq!(scheduler.pass_count(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: TheMetaPassRunsInsideThePassAndHonoursTheSuspension
#[test]
fn the_meta_pass_runs_inside_the_pass_and_honours_the_suspension() {
    let mut cx = Cx {
        meta: Some(Vec::new()),
        ..Cx::default()
    };
    let main = Probe::new("main", true);
    let independent = Probe::new("independent", true);
    let mut scheduler =
        MacroScheduler::new(vec![main.rule()], vec![independent.rule()], Vec::new());
    scheduler.start(&mut cx);

    scheduler.run_pass(&mut cx, 0.3);
    assert_eq!(cx.meta.as_deref(), Some(&[0.3][..]));

    scheduler.suspend();
    scheduler.run_pass(&mut cx, 0.3);
    assert_eq!(cx.meta.as_deref(), Some(&[0.3][..]));
    assert!(independent.running());

    scheduler.resume();
    scheduler.run_pass(&mut cx, 0.3);
    let seen = cx.meta.clone().unwrap();
    assert_eq!(seen.len(), 2);
    assert!((seen[1] - 0.6).abs() < 1e-10);
    assert!((seen.iter().sum::<f64>() - 0.9).abs() < 1e-10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: AShorterMetaIntervalLooksAtTheMetaBetweenPassesWithoutRushingItsClock
#[test]
fn a_shorter_meta_interval_looks_at_the_meta_between_passes_without_rushing_its_clock() {
    for (interval, expected_looks) in [(0.1, 6), (HEARTBEAT_SECONDS, 2)] {
        let mut cx = Cx {
            meta: Some(Vec::new()),
            ..Cx::default()
        };
        let mut scheduler = MacroScheduler::new(
            vec![Probe::new("main", true).rule()],
            Vec::new(),
            Vec::new(),
        );
        scheduler.meta_interval_seconds = interval;
        scheduler.start(&mut cx);

        let mut total = 0.0;
        for _ in 0..12 {
            scheduler.advance(&mut cx, 0.05);
            total += 0.05;
        }

        let seen = cx.meta.clone().unwrap();
        assert_eq!(seen.len(), expected_looks, "{interval}");
        let handed: f64 = seen.iter().sum();
        assert!(handed <= total + 1e-9, "handed {handed} of {total}");
        assert!(
            handed >= total - interval - 1e-9,
            "handed {handed} of {total}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: StartingTheMacroAgainOwesTheMetaNoSuspendedBacklog
#[test]
fn starting_the_macro_again_owes_the_meta_no_suspended_backlog() {
    let mut cx = Cx {
        meta: Some(Vec::new()),
        ..Cx::default()
    };
    let main = Probe::new("main", true);
    let mut scheduler = started(&mut cx, &[&main]);

    scheduler.suspend();
    scheduler.run_pass(&mut cx, 0.3);
    scheduler.run_pass(&mut cx, 0.3);
    assert!(cx.meta.as_ref().unwrap().is_empty());

    scheduler.stop(&mut cx);
    scheduler.resume();
    scheduler.start(&mut cx);
    scheduler.run_pass(&mut cx, 0.3);

    assert_eq!(cx.meta.as_deref(), Some(&[0.3][..]));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: IndependentRulesAreAssignedRunningFromValidNowEveryPass
#[test]
fn independent_rules_are_assigned_running_from_valid_now_every_pass() {
    let mut cx = Cx::default();
    let independent = Probe::new("independent", true);
    let mut scheduler = MacroScheduler::new(Vec::new(), vec![independent.rule()], Vec::new());
    scheduler.start(&mut cx);

    scheduler.run_pass(&mut cx, 1.0);
    assert!(independent.running());

    independent.set_valid(false);
    scheduler.run_pass(&mut cx, 1.0);
    assert!(!independent.running());
    assert_eq!(independent.writes(), [true, false]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PreChainGateBlocksTheWrapperBeforeThePrimaryIsConsulted
#[test]
fn pre_chain_gate_blocks_the_wrapper_before_the_primary_is_consulted() {
    let mut cx = Cx::default();
    let primary = Probe::new("primary", true);
    let gate = Rc::new(RefCell::new(false));
    let open = gate.clone();
    let mut chain = MacroRulePreChain::new(
        primary.rule(),
        vec![Box::new(move |_: &mut Cx| *open.borrow())],
        Vec::new(),
    );
    let context = MacroPassContext::new(1.0, true);

    assert!(!chain.valid_now(&mut cx, context));
    assert_eq!(primary.calls(), 0);

    *gate.borrow_mut() = true;
    assert!(chain.valid_now(&mut cx, context));
    assert_eq!(primary.calls(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PreChainFallbackDoesNotMakeTheWrapperValid
#[test]
fn pre_chain_fallback_does_not_make_the_wrapper_valid() {
    let mut cx = Cx::default();
    let primary = Probe::new("primary", false);
    let fallback = Probe::new("fallback", true);
    let mut chain = MacroRulePreChain::new(primary.rule(), Vec::new(), vec![fallback.rule()]);

    assert!(!chain.valid_now(&mut cx, MacroPassContext::new(1.0, true)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PreChainRunsAValidFallbackInsteadOfThePrimary
#[test]
fn pre_chain_runs_a_valid_fallback_instead_of_the_primary() {
    let mut cx = Cx::default();
    let primary = Probe::new("primary", true);
    let fallback = Probe::new("fallback", true);
    let mut chain = MacroRulePreChain::new(primary.rule(), Vec::new(), vec![fallback.rule()]);

    chain.set_running(&mut cx, true);

    assert!(fallback.running());
    assert!(!primary.running());
    assert!(primary.writes().is_empty());

    chain.set_running(&mut cx, false);
    assert!(!fallback.running());
    assert_eq!(primary.writes(), [false]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: PreChainRunsThePrimaryWhenNoFallbackIsValid
#[test]
fn pre_chain_runs_the_primary_when_no_fallback_is_valid() {
    let mut cx = Cx::default();
    let primary = Probe::new("primary", true);
    let fallback = Probe::new("fallback", false);
    let mut chain = MacroRulePreChain::new(primary.rule(), Vec::new(), vec![fallback.rule()]);

    chain.set_running(&mut cx, true);

    assert!(primary.running());
    assert!(!fallback.running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: RuleInfoStatesADeclinedRulesReasonOnceAndAgainWhenItChanges
#[test]
fn rule_info_states_a_declined_rules_reason_once_and_again_when_it_changes() {
    let mut cx = Cx::logging();
    let quiet = Probe::new("quiet", false);
    let talkative = Probe::new("BuffSelf", false);
    talkative.state.borrow_mut().reason = Some("EnableBuffing is off".to_owned());
    let mut scheduler = started(&mut cx, &[&quiet, &talkative]);

    scheduler.run_pass(&mut cx, 1.0);
    scheduler.run_pass(&mut cx, 1.0);
    talkative.state.borrow_mut().reason = Some("nothing is due".to_owned());
    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(
        cx.rule_info(),
        [
            (
                MacroLogChannel::RuleInfo,
                "(BuffSelf) declined: EnableBuffing is off".to_owned()
            ),
            (
                MacroLogChannel::RuleInfo,
                "(BuffSelf) declined: nothing is due".to_owned()
            ),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ARuleThatWinsAndThenDeclinesAgainStatesItsReasonAgain
#[test]
fn a_rule_that_wins_and_then_declines_again_states_its_reason_again() {
    let mut cx = Cx::logging();
    let rule = Probe::new("BuffSelf", false);
    rule.state.borrow_mut().reason = Some("nothing is due".to_owned());
    let mut scheduler = started(&mut cx, &[&rule]);

    scheduler.run_pass(&mut cx, 1.0);
    rule.set_valid(true);
    rule.state.borrow_mut().reason = None;
    scheduler.run_pass(&mut cx, 1.0);
    rule.set_valid(false);
    rule.state.borrow_mut().reason = Some("nothing is due".to_owned());
    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(
        cx.rule_info(),
        [
            (
                MacroLogChannel::RuleInfo,
                "(BuffSelf) declined: nothing is due".to_owned()
            ),
            (MacroLogChannel::RuleInfo, "(BuffSelf) Running".to_owned()),
            (
                MacroLogChannel::RuleInfo,
                "(BuffSelf) declined: nothing is due".to_owned()
            ),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: RuleInfoCarriesTheWinnersOwnRunningDetailWhenItHasOne
#[test]
fn rule_info_carries_the_winners_own_running_detail_when_it_has_one() {
    let mut cx = Cx::logging();
    let rule = Probe::new("NavigateRouteIdle", true);
    rule.state.borrow_mut().detail =
        Some("[targ range 5.657, targ loc 33.79, 42.106, 0.401 ]".to_owned());
    let mut scheduler = started(&mut cx, &[&rule]);

    scheduler.run_pass(&mut cx, 1.0);

    assert!(cx.log.unwrap().contains(&(
        MacroLogChannel::RuleInfo,
        "(NavigateRouteIdle) Running [targ range 5.657, targ loc 33.79, 42.106, 0.401 ]".to_owned()
    )));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ActiveRuleHeaderNamesTheCurrentMainRuleCountEveryPass
#[test]
fn active_rule_header_names_the_current_main_rule_count_every_pass() {
    let mut cx = Cx::logging();
    let (a, b, c) = (
        Probe::new("a", false),
        Probe::new("b", false),
        Probe::new("c", false),
    );
    let mut scheduler = started(&mut cx, &[&a, &b, &c]);

    scheduler.run_pass(&mut cx, 1.0);

    assert!(cx.log.unwrap().contains(&(
        MacroLogChannel::ActiveRule,
        "----------- Primary logic loop started (3 rules) -----------".to_owned()
    )));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ActiveRulePickedLineNamesTheWinnerWithItsListIndexAndTheLockSuffix
#[test]
fn active_rule_picked_line_names_the_winner_with_its_list_index_and_the_lock_suffix() {
    let mut cx = Cx {
        lock_suffix: Some("   I=True, N=False, S=False".to_owned()),
        ..Cx::logging()
    };
    let skipped = Probe::new("skipped", false);
    let winner = Probe::new("winner", true);
    let mut scheduler = started(&mut cx, &[&skipped, &winner]);

    scheduler.run_pass(&mut cx, 1.0);

    assert!(cx.log.unwrap().contains(&(
        MacroLogChannel::ActiveRule,
        "Picked winner P: 1   I=True, N=False, S=False".to_owned()
    )));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: ActiveRulePostsAllRulesInactiveWhenNoRuleIsValid
#[test]
fn active_rule_posts_all_rules_inactive_when_no_rule_is_valid() {
    let mut cx = Cx {
        lock_suffix: Some("   I=False, N=False, S=False".to_owned()),
        ..Cx::logging()
    };
    let (a, b) = (Probe::new("a", false), Probe::new("b", false));
    let mut scheduler = started(&mut cx, &[&a, &b]);

    scheduler.run_pass(&mut cx, 1.0);

    let log = cx.log.unwrap();
    assert!(log.contains(&(
        MacroLogChannel::ActiveRule,
        "All rules inactive.   I=False, N=False, S=False".to_owned()
    )));
    assert!(!log.iter().any(|(_, message)| message.starts_with("Picked")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: RuleInfoPostsRunningForTheWinnerOnEveryPassItWinsAndNeverForALoser
#[test]
fn rule_info_posts_running_for_the_winner_on_every_pass_it_wins_and_never_for_a_loser() {
    let mut cx = Cx::logging();
    let loser = Probe::new("loser", true);
    loser.state.borrow_mut().requires_can_act = false;
    let buff_self = Probe::new("BuffSelf", true);
    let mut scheduler = started(&mut cx, &[&buff_self, &loser]);

    scheduler.run_pass(&mut cx, 1.0);
    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(
        cx.rule_info(),
        [
            (MacroLogChannel::RuleInfo, "(BuffSelf) Running".to_owned()),
            (MacroLogChannel::RuleInfo, "(BuffSelf) Running".to_owned()),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: RuleInfoStopsWhenTheWinnerStopsWinning
#[test]
fn rule_info_stops_when_the_winner_stops_winning() {
    let mut cx = Cx::logging();
    let rule = Probe::new("BuffSelf", true);
    let mut scheduler = started(&mut cx, &[&rule]);

    scheduler.run_pass(&mut cx, 1.0);
    rule.set_valid(false);
    scheduler.run_pass(&mut cx, 1.0);

    assert_eq!(cx.rule_info().len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroSchedulerTests.cs :: UnboundLogAndLockStateSuffixDoNotThrow
#[test]
fn unbound_log_and_lock_state_suffix_do_not_throw() {
    let mut cx = Cx::default();
    let mut scheduler = started(&mut cx, &[&Probe::new("a", true)]);
    scheduler.run_pass(&mut cx, 1.0);
    assert_eq!(scheduler.pass_count(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroRuleAdapterTests.cs :: AbsentRuleIsNeverValidAndSwallowsRunning
#[test]
fn absent_rule_is_never_valid_and_swallows_running() {
    let mut cx = Cx::default();
    let mut rule = AbsentMacroRule::new("Slot", "not ported");
    let context = MacroPassContext::new(1.0, true);

    assert!(!MacroRule::<Cx>::valid_now(&mut rule, &mut cx, context));
    MacroRule::<Cx>::set_running(&mut rule, &mut cx, true);
    assert!(!MacroRule::<Cx>::running(&rule));
    assert_eq!(rule.reason, "not ported");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroRuleAdapterTests.cs :: GateBlockedRuleDoesNotTickItsController
#[test]
fn gate_blocked_rule_does_not_tick_its_controller() {
    let mut cx = Cx::default();
    let calls = Rc::new(RefCell::new(0));
    let counted = calls.clone();
    let mut rule = ControllerMacroRule::<Cx>::new(
        "rule",
        Box::new(move |_, _| {
            *counted.borrow_mut() += 1;
            true
        }),
    )
    .with_gate(Box::new(|_| false));

    assert!(!rule.valid_now(&mut cx, MacroPassContext::new(1.0, true)));
    assert_eq!(*calls.borrow(), 0);
    assert_eq!(
        rule.decline_reason(&mut cx).as_deref(),
        Some("the rule's own gate is closed")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroRuleAdapterTests.cs :: ARuleAskedWithoutTheTurnIsNotTickedAndDoesNotClaim
#[test]
fn a_rule_asked_without_the_turn_is_not_ticked_and_does_not_claim() {
    let mut cx = Cx::default();
    let calls = Rc::new(RefCell::new(0));
    let counted = calls.clone();
    let mut rule = ControllerMacroRule::<Cx>::new(
        "rule",
        Box::new(move |_, _| {
            *counted.borrow_mut() += 1;
            true
        }),
    );

    assert!(!rule.valid_now(&mut cx, MacroPassContext::new(1.0, false)));
    assert_eq!(*calls.borrow(), 0);
    assert!(rule.valid_now(&mut cx, MacroPassContext::new(1.0, true)));
    assert_eq!(*calls.borrow(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroRuleAdapterTests.cs :: TeardownRunsOnceOnTheTrueToFalseEdge
#[test]
fn teardown_runs_once_on_the_true_to_false_edge() {
    let mut cx = Cx::default();
    let teardowns = Rc::new(RefCell::new(0));
    let counted = teardowns.clone();
    let mut rule = ControllerMacroRule::<Cx>::new("rule", Box::new(|_, _| true))
        .with_lost_turn(Box::new(move |_| *counted.borrow_mut() += 1));

    rule.set_running(&mut cx, false);
    assert_eq!(*teardowns.borrow(), 0);

    rule.set_running(&mut cx, true);
    rule.set_running(&mut cx, false);
    assert_eq!(*teardowns.borrow(), 1);

    rule.set_running(&mut cx, false);
    assert_eq!(*teardowns.borrow(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroDebugLogTests.cs :: KeepsNewestWholeUtf8RecordsWithinByteLimit
#[test]
fn keeps_newest_whole_utf8_records_within_byte_limit() {
    let mut storage = SharedStorage::new();
    // A storage that counts its writes.
    struct Counting<'s> {
        storage: &'s mut SharedStorage,
        writes: usize,
    }
    impl PluginStorage for Counting<'_> {
        fn is_available(&mut self) -> bool {
            true
        }
        fn root_path(&mut self) -> Option<String> {
            None
        }
        fn read_text(&mut self, key: &str) -> Result<Option<String>, String> {
            self.storage.read_text(key)
        }
        fn list(&mut self, prefix: &str) -> Result<Vec<String>, String> {
            self.storage.list(prefix)
        }
        fn write_text(&mut self, key: &str, content: &str) -> Result<(), String> {
            self.writes += 1;
            self.storage.write_text(key, content)
        }
        fn ensure_directory(&mut self, _prefix: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn delete(&mut self, key: &str) -> Result<bool, String> {
            self.storage.delete(key)
        }
    }
    let mut counting = Counting {
        storage: &mut storage,
        writes: 0,
    };
    let mut log = MacroDebugLog::new("debug/test.log");
    log.record(0, "oldest");
    for index in 0..3000 {
        log.record(0, &format!("{index}:{}", "\u{754c}".repeat(300)));
    }
    log.record(0, "newest");
    log.flush(&mut counting).unwrap();
    let writes = counting.writes;
    let text = storage.get("debug/test.log").unwrap();
    assert!(text.len() <= MAXIMUM_BYTES);
    assert!(!text.contains("oldest"));
    assert!(text.ends_with("newest\n"));
    assert!(
        text.split('\n')
            .filter(|line| !line.is_empty())
            .all(|line| line.starts_with('['))
    );
    let mut counting = Counting {
        storage: &mut storage,
        writes,
    };
    log.flush(&mut counting).unwrap();
    assert_eq!(counting.writes, writes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MacroDebugLogTests.cs :: OversizedRecordStaysWithinLimit
#[test]
fn oversized_record_stays_within_limit() {
    let mut storage = SharedStorage::new();
    let mut log = MacroDebugLog::new("debug/test.log");
    log.record(0, "before");
    log.record(0, &"\u{754c}".repeat(MAXIMUM_BYTES));
    let mut available = storage.clone();
    log.flush(&mut available).unwrap();
    let text = storage.get("debug/test.log").unwrap();
    assert!(text.contains("before"));
    assert!(text.contains("Record omitted"));
    assert!(text.len() < MAXIMUM_BYTES);
    let _ = storage.is_available();
}

#[test]
fn a_record_is_stamped_in_the_round_trip_form_with_its_offset() {
    let mut storage = SharedStorage::new();
    let mut log = MacroDebugLog::new("debug/stamp.log");
    log.record(1_700_000_000_123, "line\r\nbreak");
    log.flush(&mut storage).unwrap();
    assert_eq!(
        storage.get("debug/stamp.log").unwrap(),
        "[2023-11-14T22:13:20.1230000+00:00] line  break\n"
    );
}
