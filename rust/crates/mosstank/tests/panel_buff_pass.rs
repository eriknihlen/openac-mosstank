//! The buff pass as the panel runs it: when it rebuffs, what a cast's
//! receipt, words and result do to it, the tiers it walks to and the lines
//! it logs (ported from the C#'s `MossTankPanelTests`, over its
//! `FakeAutomation`).

mod common;

use std::collections::BTreeSet;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::ItemUseCompletion;
use ac_plugin_abi::magic::{CastGate, CastResult, SpellInfo};
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::World;
use common::automation::{
    FakeAutomation, SPELLCASTING_TEXT_TYPE, active, attribute, component, skill,
};
use mosstank::MossTank;
use mosstank_automation::cast_tracker::SpellCastTrackerState;
use mosstank_automation::locks::ActionLockKind;
use mosstank_buffs::due::BuffDueTracker;
use mosstank_combat::gate::NO_WAND_NOTICE;
use mosstank_expr::ExpressionValue;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};

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

const LIFE_MAGIC: &str = "Increases the caster's Life Magic skill by 10 points.";
const STRENGTH: &str = "Increases the caster's Strength by 10 points.";

/// Full vitals, Life Magic at `life_magic`, and the given self buffs.
fn caster(life_magic: u32, buffs: Vec<SpellInfo>) -> FakeAutomation {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.skills = vec![skill(1, "Life Magic", life_magic)];
    automation.set_known_self_buffs(buffs);
    automation
}

fn tiered(spell_id: u32, family: u32, description: &str, tier: i32, difficulty: i32) -> SpellInfo {
    let mut spell = common::spell(spell_id, family, description);
    spell.tier = tier;
    spell.difficulty = difficulty;
    spell
}

fn with_components(mut spell: SpellInfo, components: &[u32]) -> SpellInfo {
    spell.formula_component_ids = components.to_vec();
    spell
}

fn set_option(world: &mut World, plugin: &mut MossTank, name: &str, value: ExpressionValue) {
    world
        .with_tank(plugin, |tank, client| {
            tank.set_meta_option(client, name, &value)
        })
        .unwrap();
}

fn chat_has(world: &World, line: &str) -> bool {
    world.chat().iter().any(|message| message == line)
}

fn chat_count(world: &World, predicate: impl Fn(&str) -> bool) -> usize {
    world
        .chat()
        .iter()
        .filter(|message| predicate(message))
        .count()
}

/// Worn gear low on mana (`Item(0x50005001, "Low Wand", 1) with { ... }`).
fn low_wand() -> ac_plugin_abi::items::InventoryItem {
    let mut item = common::item(0x5000_5001, "Low Wand", 0x0000_0001);
    item.equipped_location = 0x0000_0002;
    item.item_current_mana = 10;
    item.item_maximum_mana = 100;
    item
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheIdleTopoffWindowIsItsOwnProfileNumber
#[test]
fn the_idle_topoff_window_is_its_own_profile_number() {
    for (idle_window_seconds, casts) in [(1200.0, true), (300.0, false)] {
        let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
        automation.active_enchantments = vec![active(1, 10, 1, 600.0)];
        let (mut world, mut plugin) = open(automation);
        set_option(
            &mut world,
            &mut plugin,
            "IdleBuffTopoffTimeSeconds",
            ExpressionValue::Number(idle_window_seconds),
        );

        // Ten minutes left is outside the ordinary rebuff window either way,
        // so nothing is cast until the idle pass is switched on.
        world.act(&mut plugin, "ToggleCombat");
        tick(&mut world, &mut plugin, 0.0);
        assert!(world.cast_spell_ids().is_empty());

        world.act(&mut plugin, "ToggleIdleBuffTopoff");
        tick(&mut world, &mut plugin, 1.0);

        assert_eq!(
            !world.cast_spell_ids().is_empty(),
            casts,
            "{idle_window_seconds}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: StoppingTheMacroEndsAnAutomaticBuffPassInProgress
#[test]
fn stopping_the_macro_ends_an_automatic_buff_pass_in_progress() {
    let (mut world, mut plugin) = open(buff_pass_automation());

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    assert!(!world.cast_spell_ids().is_empty());
    assert!(buff_status(&mut world, &mut plugin).starts_with("Buffing"));

    let casts_when_stopped = world.cast_spell_ids().len();
    assert!(
        casts_when_stopped < 3,
        "the pass must still be mid-queue for this pin to mean anything"
    );

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 10, 1.0);

    assert_eq!(world.cast_spell_ids().len(), casts_when_stopped);
    assert!(!buff_status(&mut world, &mut plugin).contains("Buffing"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheStoppedMacroListIsTheWornManaRuleUnderItsOwnSwitch
/// The C# asks the stopped list's rule directly (`ValidNow`); the shell
/// does not hand its rules out, so the rule is asked the way the frame asks
/// it, by passes with the macro stopped. The list's names and order are
/// pinned in `shell_rules.rs`.
#[test]
fn the_stopped_macro_list_is_the_worn_mana_rule_under_its_own_switch() {
    let mut automation = FakeAutomation::default();
    automation.item_entries = vec![low_wand()];
    let (mut world, mut plugin) = open(automation);
    assert_eq!(
        plugin.tank().unwrap().macro_disabled_rule_names(),
        ["RefillWieldedMana"]
    );

    world.vt(&mut plugin, "opt set RefillWornMana true");
    world.vt(&mut plugin, "opt set ManaChargesWhenOff false");
    ticks(&mut world, &mut plugin, 3, 0.3);
    assert!(!world.automation().identified.contains(&0x5000_5001));

    world.vt(&mut plugin, "opt set ManaChargesWhenOff true");
    ticks(&mut world, &mut plugin, 3, 0.3);
    assert!(world.automation().identified.contains(&0x5000_5001));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: StoppingTheMacroInsideACastStillLeavesTheStoppedListRunning
#[test]
fn stopping_the_macro_inside_a_cast_still_leaves_the_stopped_list_running() {
    let mut automation = buff_pass_automation();
    automation.suppress_cast_completion = true;
    automation.item_entries = vec![low_wand()];
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "opt set RefillWornMana true");
    world.vt(&mut plugin, "opt set ManaChargesWhenOff true");

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    assert_eq!(world.cast_spell_ids().len(), 1);
    // One more frame, which is where the hold on the pass is taken: it is
    // taken beside the pass, on the frame after the cast went out.
    tick(&mut world, &mut plugin, 0.1);

    // Stopped with the cast still unanswered and the hold in place.
    world.act(&mut plugin, "ToggleCombat");
    world.automation().identified.clear();

    ticks(&mut world, &mut plugin, 40, 0.3);

    assert!(world.automation().identified.contains(&0x5000_5001));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheStoppedMacroListKeepsThePassAliveWithoutBuffing
#[test]
fn the_stopped_macro_list_keeps_the_pass_alive_without_buffing() {
    let (mut world, mut plugin) = open(buff_pass_automation());

    world.vt(&mut plugin, "opt set ManaChargesWhenOff true");
    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    let casts_when_stopped = world.cast_spell_ids().len();
    assert!(!world.cast_spell_ids().is_empty());

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 10, 1.0);

    assert_eq!(world.cast_spell_ids().len(), casts_when_stopped);
    assert!(!buff_status(&mut world, &mut plugin).contains("Buffing"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ForceBuffWithTheMacroOffCastsNothingUntilRunMacro
#[test]
fn force_buff_with_the_macro_off_casts_nothing_until_run_macro() {
    let (mut world, mut plugin) = open(buff_pass_automation());

    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 20, 0.3);
    assert!(world.cast_spell_ids().is_empty());

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 20, 0.3);
    assert!(!world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TogglingBuffingWithTheMacroOffCastsNothing
#[test]
fn toggling_buffing_with_the_macro_off_casts_nothing() {
    let (mut world, mut plugin) = open(buff_pass_automation());

    world.act(&mut plugin, "ForceBuff");
    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(false),
    );
    ticks(&mut world, &mut plugin, 10, 0.3);
    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(true),
    );
    ticks(&mut world, &mut plugin, 20, 0.3);

    assert!(world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AServerRejectedCastRePicksTheSameSpellInsteadOfAdvancing
#[test]
fn a_server_rejected_cast_re_picks_the_same_spell_instead_of_advancing() {
    let mut automation = buff_pass_automation();
    automation.next_cast_weenie_error = 0x1D;
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 8, 0.3);

    let casts = world.cast_spell_ids();
    assert!(!casts.is_empty());
    assert!(casts.iter().all(|&id| id == 1), "{casts:?}");
    assert!(buff_status(&mut world, &mut plugin).contains("Spell 1"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnAcknowledgedCastWithNoResultTextKeepsThePassSuspended
#[test]
fn an_acknowledged_cast_with_no_result_text_keeps_the_pass_suspended() {
    let mut automation = buff_pass_automation();
    automation.suppress_cast_result_text = true;
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    assert_eq!(world.cast_spell_ids().len(), 1);

    // Three seconds is inside the 4 x 907 ms result budget.
    ticks(&mut world, &mut plugin, 10, 0.3);

    assert!(!world.automation().is_casting);
    assert_eq!(world.cast_spell_ids().len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: APermanentlyRefusedSpellIsPickedAgainForeverBecauseAuthenticNeverGivesUp
#[test]
fn a_permanently_refused_spell_is_picked_again_forever() {
    let mut automation = buff_pass_automation();
    automation.next_cast_weenie_error = 0x1D;
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 40, 1.0);

    let casts = world.cast_spell_ids();
    assert!(casts.len() > 5, "the refused spell was never re-issued");
    assert!(casts.iter().all(|&id| id == 1), "{casts:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheCastSuspensionHoldsForTheResultThoughIsCastingNeverLatched
#[test]
fn the_cast_suspension_holds_for_the_result_though_is_casting_never_latched() {
    let mut automation = buff_pass_automation();
    automation.suppress_cast_completion = true;
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    assert_eq!(buff_cast_requests(&world), 1);

    ticks(&mut world, &mut plugin, 10, 0.3);

    assert!(!world.automation().is_casting);
    assert_eq!(buff_cast_requests(&world), 1);

    // Past the 5000 ms attempt watchdog the tracker drops to idle and
    // re-issues the SAME spell; it never walks the queue.
    ticks(&mut world, &mut plugin, 12, 0.3);

    assert!(buff_cast_requests(&world) > 1);
    assert!(world.cast_spell_ids().iter().all(|&id| id == 1));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AMasteryBuffThatLandsUnlocksTheHigherTierOfTheNextPick
#[test]
fn a_mastery_buff_that_lands_unlocks_the_higher_tier_of_the_next_pick() {
    let mut automation = caster(
        100,
        vec![
            common::spell(1, 10, LIFE_MAGIC),
            // The second family: tier 2 needs skill 205, tier 1 needs 15.
            tiered(2, 20, STRENGTH, 2, 200),
            tiered(3, 20, STRENGTH, 1, 10),
        ],
    );
    automation.attributes = vec![attribute(0, "Strength", 100)];
    automation.raise_skill_on_cast = Some((1, 1, 300));
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert_eq!(world.cast_spell_ids(), [1, 2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheIdleTopoffWindowBelongsToRow53NotToTheRebuffRule
#[test]
fn the_idle_topoff_window_belongs_to_row_53_not_to_the_rebuff_rule() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    // 600 s left: over the 300 s rebuff window, under the 1200 s idle one.
    automation.active_enchantments = vec![active(1, 10, 1, 600.0)];
    let (mut world, mut plugin) = open(automation);
    set_option(
        &mut world,
        &mut plugin,
        "IdleBuffTopoff",
        ExpressionValue::Boolean(true),
    );
    world.vt(&mut plugin, "log ActiveRule on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.3);

    assert!(
        chat_count(&world, |m| m.starts_with("[MossTank] Picked BuffSelfIdle")) > 0,
        "{:?}",
        world.chat()
    );
    assert_eq!(
        chat_count(&world, |m| m.starts_with("[MossTank] Picked BuffSelf ")),
        0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: WithIdleTopoffOffTheWiderWindowIsNotConsideredAtAll
#[test]
fn with_idle_topoff_off_the_wider_window_is_not_considered_at_all() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    automation.active_enchantments = vec![active(1, 10, 1, 600.0)];
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert!(world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AFamilyWhoseTopTierHasNoScarabsFallsToTheCastableLowerTier
#[test]
fn a_family_whose_top_tier_has_no_scarabs_falls_to_the_castable_lower_tier() {
    let mut automation = caster(
        300,
        vec![
            with_components(tiered(2, 20, STRENGTH, 2, 200), &[7]),
            with_components(tiered(3, 20, STRENGTH, 1, 10), &[8]),
        ],
    );
    automation.attributes = vec![attribute(0, "Strength", 100)];
    automation.item_entries = vec![common::item(50, "Pyreal Scarab", 1)];
    automation.components.insert(7, component(7, "Lead Scarab"));
    automation
        .components
        .insert(8, component(8, "Pyreal Scarab"));
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert_eq!(world.cast_spell_ids(), [3]);
    assert!(
        chat_has(
            &world,
            "[MossTank] Out of component \"Lead Scarab\" \u{2014} spells that need it are off."
        ),
        "{:?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AFamilyWithNoCastableTierIsSkippedAndTheNextFamilyIsCast
#[test]
fn a_family_with_no_castable_tier_is_skipped_and_the_next_family_is_cast() {
    let mut automation = caster(
        300,
        vec![
            with_components(common::spell(2, 20, STRENGTH), &[7]),
            with_components(
                common::spell(3, 21, "Increases the caster's Endurance by 10 points."),
                &[8],
            ),
        ],
    );
    automation.attributes = vec![
        attribute(0, "Strength", 100),
        attribute(1, "Endurance", 100),
    ];
    automation.item_entries = vec![common::item(50, "Pyreal Scarab", 1)];
    automation.components.insert(7, component(7, "Lead Scarab"));
    automation
        .components
        .insert(8, component(8, "Pyreal Scarab"));
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert_eq!(world.cast_spell_ids(), [3]);
    assert!(
        chat_has(
            &world,
            "[MossTank] Buff skipped: no known spell in the class of Spell 2."
        ),
        "{:?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ARefusedCastSaysWhichSpellAndWhyOnceOnTheCastInfoChannel
#[test]
fn a_refused_cast_says_which_spell_and_why_once_on_the_cast_info_channel() {
    let mut automation = caster(300, vec![common::spell(1, 20, STRENGTH)]);
    automation.attributes = vec![attribute(0, "Strength", 100)];
    automation
        .cast_refusals
        .insert(1, CastResult::MissingComponents);
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "log CastInfo on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 8, 0.3);

    assert!(world.cast_spell_ids().is_empty());
    assert_eq!(
        chat_count(&world, |m| m.contains("not issued")),
        1,
        "{:?}",
        world.chat()
    );
    assert!(chat_has(
        &world,
        "[MossTank] SpellCaster: Spell 1 not issued \u{2014} MissingComponents"
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ATierTheHostHasNoComponentsForIsNotACandidateAndTheWalkDropsToTheNext
#[test]
fn a_tier_the_host_has_no_components_for_is_not_a_candidate() {
    let mut automation = caster(
        300,
        vec![
            common::spell(1, 20, STRENGTH),
            tiered(2, 20, STRENGTH, 6, 250),
        ],
    );
    automation.attributes = vec![attribute(0, "Strength", 100)];
    automation.missing_component_spell_ids.insert(2);
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "log CastInfo on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 8, 0.3);

    assert_eq!(world.cast_spell_ids(), [1]);
    assert_eq!(chat_count(&world, |m| m.contains("not issued")), 0);
}

/// `Skills = [Skill 288]`, Strength, and the family-90 ladder of the
/// timers tests.
fn timers_automation() -> FakeAutomation {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.skills = vec![skill(1, "Skill", 288)];
    automation.attributes = vec![attribute(0, "Strength", 100)];
    let mut fellowship = tiered(8, 90, STRENGTH, 8, 340);
    fellowship.is_fellowship = true;
    automation.set_known_self_buffs(vec![
        // Lowest difficulty, self-targeted: the family's own anchor, and the
        // tier the walk should land on: needed = 250 + 5 = 255 <= 288.
        tiered(6, 90, STRENGTH, 6, 250),
        // needed = 296 + 5 = 301 > 288: skill-short.
        tiered(7, 90, STRENGTH, 7, 296),
        fellowship,
    ]);
    automation
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TimersTraceNamesEveryHigherTierAndTheFirstFailingTermWithNumbers
#[test]
fn timers_trace_names_every_higher_tier_and_the_first_failing_term_with_numbers() {
    let (mut world, mut plugin) = open(timers_automation());
    world.vt(&mut plugin, "log Timers on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.3);

    assert!(
        chat_has(
            &world,
            "[MossTank] Buffing: Spell 6 \u{2014} picked Spell 6 (gen 6); rejected: Spell 8 unknown, Spell 7 skill 288 < 301"
        ),
        "{:?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: NoTimersLineWhenTheHighestKnownTierIsPicked
#[test]
fn no_timers_line_when_the_highest_known_tier_is_picked() {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.skills = vec![skill(1, "Skill", 300)];
    automation.attributes = vec![attribute(0, "Strength", 100)];
    automation.set_known_self_buffs(vec![
        tiered(1, 91, STRENGTH, 1, 10),
        tiered(2, 91, STRENGTH, 2, 20),
    ]);
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "log Timers on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert_eq!(world.cast_spell_ids(), [2]);
    assert_eq!(chat_count(&world, |m| m.contains("Buffing:")), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TimersTraceIsNotReemittedAcrossRepeatedIdenticalPasses
#[test]
fn timers_trace_is_not_reemitted_across_repeated_identical_passes() {
    let (mut world, mut plugin) = open(timers_automation());
    world.vt(&mut plugin, "log Timers on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.3);
    let trace = |m: &str| m.starts_with("[MossTank] Buffing: Spell 6");
    assert_eq!(chat_count(&world, trace), 1);

    ticks(&mut world, &mut plugin, 5, 0.3);
    assert_eq!(chat_count(&world, trace), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheBuffCastRecastLockWidensTheDueWindowRatherThanBlockingPicks
#[test]
fn the_buff_cast_recast_lock_widens_the_due_window_rather_than_blocking_picks() {
    let mut automation = caster(
        300,
        vec![
            common::spell(1, 10, LIFE_MAGIC),
            common::spell(2, 20, STRENGTH),
        ],
    );
    automation.attributes = vec![attribute(0, "Strength", 100)];
    automation.active_enchantments = vec![active(2, 20, 1, 310.0)];
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert_eq!(world.cast_spell_ids(), [1, 2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ForceBuffZeroesTheDueStampsAndCancelPutsThemBack
#[test]
fn force_buff_zeroes_the_due_stamps_and_cancel_puts_them_back() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    automation.active_enchantments = vec![active(1, 10, 1, 1800.0)];
    let (mut world, mut plugin) = open(automation);

    // Nothing is due: 1800 s left against a 300 s threshold.
    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.3);
    assert!(world.cast_spell_ids().is_empty());

    // Force-buff: everything now reads as about to expire, and cancelling
    // it puts it back before the next heartbeat can act.
    world.act(&mut plugin, "ForceBuff");
    world.act(&mut plugin, "CancelForceBuff");
    ticks(&mut world, &mut plugin, 6, 0.3);
    assert!(world.cast_spell_ids().is_empty());

    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 6, 0.3);
    assert_eq!(world.cast_spell_ids(), [1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AForcedEntryStopsBeingForcedOnceItIsRecast
#[test]
fn a_forced_entry_stops_being_forced_once_it_is_recast() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    automation.active_enchantments = vec![active(1, 10, 1, 1800.0)];
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 20, 0.3);

    assert_eq!(world.cast_spell_ids(), [1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheBuffCastEmitsVtanksSpellCastLogLine
#[test]
fn the_buff_cast_emits_vtanks_spell_cast_log_line() {
    let (mut world, mut plugin) = open(buff_pass_automation());

    world.vt(&mut plugin, "log SpellCast on");
    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.3);

    assert!(chat_count(&world, |m| m.contains("Casting: Spell 1 on ")) > 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AFizzledBuffIsSimplyStillDueOnTheNextHeartbeat
#[test]
fn a_fizzled_buff_is_simply_still_due_on_the_next_heartbeat() {
    let mut automation = buff_pass_automation();
    automation.cast_result_text = Some("Your spell fizzled.".to_owned());
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 10, 0.3);

    let casts = world.cast_spell_ids();
    assert!(casts.len() > 2, "{casts:?}");
    assert!(casts.iter().all(|&id| id == 1));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: APermanentFailIsNotADropBecauseEqHasNoSuchBlacklist
#[test]
fn a_permanent_fail_is_not_a_drop_because_eq_has_no_such_blacklist() {
    let mut automation = buff_pass_automation();
    automation.cast_result_text = Some("Target is out of range".to_owned());
    let (mut world, mut plugin) = open(automation);

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 10, 0.3);

    let casts = world.cast_spell_ids();
    assert!(casts.len() > 2, "{casts:?}");
    assert!(casts.iter().all(|&id| id == 1));
}

/// The casts the buff rule asked for (`CastRequestCount`): the rule logs
/// each request it sends. The fake's own count also has the tracker's
/// re-sends of an unanswered cast, which go out through the same client
/// call; the C# sends those through `Cast` and does not count them.
fn buff_cast_requests(world: &World) -> usize {
    world
        .infos()
        .iter()
        .filter(|line| line.starts_with("MossTank: casting "))
        .count()
}

/// How the cast tracker's casts ended (`tracker.Completed`), read from the
/// lines the fight logs on the CastInfo channel for each outcome.
fn cast_outcomes(world: &World) -> Vec<String> {
    const OUTCOMES: [&str; 6] = [
        "[MossTank] SpellCaster: Spell kill reset",
        "[MossTank] SpellCaster: Spell permanent fail reset",
        "[MossTank] SpellCaster: Spell fail reset",
        "[MossTank] SpellCaster: Spell success reset",
        "[MossTank] SpellCaster: gave up waiting",
        "[MossTank] SpellCaster: Cast refused",
    ];
    world
        .chat()
        .into_iter()
        .filter(|line| OUTCOMES.iter().any(|outcome| line.starts_with(outcome)))
        .collect()
}

const LAUNCH_TIMEOUT: &str = "[MossTank] SpellCaster: gave up waiting for the cast to start";

fn tracker_state(plugin: &MossTank) -> SpellCastTrackerState {
    plugin.tank().unwrap().cast_tracker().borrow().state()
}

fn tracker_busy(plugin: &MossTank) -> bool {
    plugin.tank().unwrap().cast_tracker().borrow().is_busy()
}

/// `NavigationAt(0f) with { Position = (0x00010001, 0, north_south, 0) }`.
fn stand_at(automation: &mut FakeAutomation, north_south: f64) {
    let position = Position {
        cell_id: 0x0001_0001,
        east_west: 0.0,
        north_south,
        elevation: 0.0,
        heading_degrees: 0.0,
        is_outdoor: true,
    };
    automation.navigation = NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: 1,
        position: position.clone(),
        confirmed_position: position,
        ..NavigationSnapshot::default()
    };
}

/// A buff pass with a one-point route a metre ahead of the character.
fn buff_pass_on_a_route(automation: FakeAutomation) -> (World, MossTank) {
    let mut automation = automation;
    stand_at(&mut automation, 1.0);
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "AddRoutePoint");
    stand_at(world.automation(), 0.0);
    world.act(&mut plugin, "ToggleNavigation");
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnOutstandingInventoryRequestDoesNotStopTheBuffRule
#[test]
fn an_outstanding_inventory_request_does_not_stop_the_buff_rule() {
    let mut automation = buff_pass_automation();
    automation.items_busy = true;
    automation.is_casting = true;
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "log ActiveRule on");
    world.clear_chat();

    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);

    // The rule keeps the pass AND casts.
    assert!(
        chat_count(&world, |m| m.contains("Picked BuffSelf")) > 0,
        "{:?}",
        world.chat()
    );
    assert!(!world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AGenericCastReceiptCannotAdvanceOrRejectTheCurrentAttempt
#[test]
fn a_generic_cast_receipt_cannot_advance_or_reject_the_current_attempt() {
    for error in [0, 0x1D] {
        let mut automation = buff_pass_automation();
        automation.suppress_cast_completion = true;
        let (mut world, mut plugin) = open(automation);
        world.vt(&mut plugin, "log CastInfo on");
        world.act(&mut plugin, "ToggleCombat");
        tick(&mut world, &mut plugin, 0.0);

        world.automation().post_cast_receipt(1, error);
        tick(&mut world, &mut plugin, 0.2);
        assert_eq!(
            tracker_state(&plugin),
            SpellCastTrackerState::AwaitingLaunch
        );
        assert!(cast_outcomes(&world).is_empty(), "{error}");
        assert!(world.cast_spell_ids().len() > 1, "{error}");

        ticks(&mut world, &mut plugin, 17, 0.3);
        assert_eq!(cast_outcomes(&world), [LAUNCH_TIMEOUT], "{error}");
        assert_eq!(buff_cast_requests(&world), 2, "{error}");

        // A delayed reply from the first attempt cannot acknowledge or
        // cancel its successor, even when both use the same spell and
        // target.
        world.automation().post_cast_receipt(1, error);
        tick(&mut world, &mut plugin, 0.2);
        assert_eq!(
            tracker_state(&plugin),
            SpellCastTrackerState::AwaitingLaunch
        );
        assert_eq!(cast_outcomes(&world).len(), 1);

        let saying = world.automation().known_self_buffs()[0].saying.clone();
        let own = world.automation().object_id;
        world.post_chat_from(own, 0, &saying, SPELLCASTING_TEXT_TYPE);
        tick(&mut world, &mut plugin, 0.0);
        assert_eq!(
            tracker_state(&plugin),
            SpellCastTrackerState::AwaitingResult
        );
        world.automation().post_cast_receipt(1, error);
        tick(&mut world, &mut plugin, 0.0);
        assert_eq!(
            tracker_state(&plugin),
            SpellCastTrackerState::AwaitingResult
        );
        assert_eq!(cast_outcomes(&world).len(), 1);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: BuffSpeechAndResultResumeNavigationWithoutAnyCastReceipt
#[test]
fn buff_speech_and_result_resume_navigation_without_any_cast_receipt() {
    let mut automation = buff_pass_automation();
    automation.suppress_cast_receipt = true;
    let (mut world, mut plugin) = buff_pass_on_a_route(automation);
    world.vt(&mut plugin, "log CastInfo on");
    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);
    ticks(&mut world, &mut plugin, 5, 0.3);

    assert_eq!(world.automation().last_completion().revision, 0);
    let outcomes = cast_outcomes(&world);
    assert_eq!(outcomes.len(), 3, "{outcomes:?}");
    for (outcome, spell) in outcomes.iter().zip(["Spell 1", "Spell 2", "Spell 3"]) {
        assert!(
            outcome.starts_with("[MossTank] SpellCaster: Spell success reset")
                && outcome.contains(&format!("You cast {spell} on yourself")),
            "{outcome}"
        );
    }
    assert_eq!(world.cast_spell_ids(), [1, 2, 3]);
    assert!(!tracker_busy(&plugin));
    assert!(!world.automation().movement_intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnUnansweredBuffReturnsToNormalRulePriorityAfterTheAttemptBudget
#[test]
fn an_unanswered_buff_returns_to_normal_rule_priority_after_the_attempt_budget() {
    for gate_after_issue in [CastGate::Ready, CastGate::Busy] {
        let mut automation = buff_pass_automation();
        automation.suppress_cast_completion = true;
        let (mut world, mut plugin) = buff_pass_on_a_route(automation);
        world.vt(&mut plugin, "log CastInfo on");
        world.act(&mut plugin, "ToggleCombat");
        tick(&mut world, &mut plugin, 0.0);
        assert_eq!(world.cast_spell_ids().len(), 1);

        world.automation().is_casting = true;
        world.automation().cast_gate = gate_after_issue;
        ticks(&mut world, &mut plugin, 20, 0.3);

        assert!(
            cast_outcomes(&world)
                .iter()
                .any(|line| line == LAUNCH_TIMEOUT),
            "{gate_after_issue:?}"
        );
        // Buffing still has priority while the buff is due. Timeout does
        // not fabricate success or force a turn for the lower-priority
        // route.
        assert!(world.automation().movement_intents.is_empty());
        if gate_after_issue == CastGate::Ready {
            assert!(world.cast_spell_ids().len() > 25);
            assert_eq!(
                tracker_state(&plugin),
                SpellCastTrackerState::AwaitingLaunch
            );
        } else {
            assert_eq!(world.cast_spell_ids().len(), 1);
            assert!(!tracker_busy(&plugin));
        }
    }
}

/// Full vitals and a loaf of bread for health on the Consumables page.
fn bread_on_the_consumables_page() -> (World, MossTank) {
    let mut automation = FakeAutomation::with_full_vitals();
    let mut bread = common::item(60, "Bread", 0x20);
    // BoosterVital 2 = VitalKind.Health.
    bread.booster_vital = 2;
    automation.item_entries = vec![bread];
    let (mut world, mut plugin) = open(automation);
    world.select_object(60);
    world.act(&mut plugin, "AddSelectedConsumable");
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheItemSlotHoldsTheConsumableRulesOffAsWellAsTheAttack
#[test]
fn the_item_slot_holds_the_consumable_rules_off_as_well_as_the_attack() {
    let (mut world, mut plugin) = bread_on_the_consumables_page();
    world.act(&mut plugin, "ToggleCombat");
    // The macro clears every slot as it starts, so the window opens after
    // the first frame.
    tick(&mut world, &mut plugin, 0.0);

    // Somebody else's window, and only now is health worth a bite: two
    // seconds of passes eat nothing.
    let locks = plugin.tank().unwrap().action_locks();
    locks.borrow_mut().arm(ActionLockKind::ItemUse, 5.0);
    world.automation().health.0 = 10;
    ticks(&mut world, &mut plugin, 6, 0.3);
    assert!(world.automation().used_item_ids.is_empty());

    locks.borrow_mut().release(ActionLockKind::ItemUse);
    ticks(&mut world, &mut plugin, 6, 0.3);
    assert_eq!(world.automation().used_item_ids, [60]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnsweredConsumableUsesFollowOneAnotherInsideASecond
#[test]
fn answered_consumable_uses_follow_one_another_inside_a_second() {
    let (mut world, mut plugin) = bread_on_the_consumables_page();
    world.act(&mut plugin, "ToggleCombat");
    tick(&mut world, &mut plugin, 0.0);

    world.automation().health.0 = 10;
    ticks(&mut world, &mut plugin, 6, 0.1);
    assert_eq!(world.automation().used_item_ids, [60]);

    // The server answers, and one second of frames follows: a fraction of
    // the suspension watchdog.
    world.automation().item_completion = ItemUseCompletion {
        revision: 1,
        source_id: 60,
        target_id: 0,
        weenie_error: 0,
    };
    ticks(&mut world, &mut plugin, 10, 0.1);

    assert_eq!(world.automation().used_item_ids, [60, 60]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ForceBuffGoesThroughTheModeGateBecauseMEIsConsumableNotForce
/// The C# runs this over its combat-capable fake: equipment and nothing to
/// wield. The fake here has equipment with nothing in it, which is the
/// same gate path.
#[test]
fn force_buff_goes_through_the_mode_gate() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    automation.equipment.clear();
    automation.combat.mode = CombatMode::Peace;
    let (mut world, mut plugin) = open(automation);

    // No wand anywhere: the gate's own path here is the no-wand notice and
    // the stop, exactly as the reference does.
    world.act(&mut plugin, "ToggleCombat");
    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 5, 0.3);

    assert!(world.cast_spell_ids().is_empty());
    assert!(chat_count(&world, |m| m.contains(NO_WAND_NOTICE)) > 0);
    assert!(!plugin.tank().unwrap().macro_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TogglingBuffingOffPausesTheForceBuffInsteadOfCancellingIt
#[test]
fn toggling_buffing_off_pauses_the_force_buff_instead_of_cancelling_it() {
    let (mut world, mut plugin) = open(caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]));
    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(false),
    );

    world.act(&mut plugin, "ToggleCombat");
    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 4, 0.3);
    // EnableBuffing holds the pass.
    assert!(world.cast_spell_ids().is_empty());

    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(true),
    );
    ticks(&mut world, &mut plugin, 4, 0.3);

    assert_eq!(world.cast_spell_ids(), [1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AForceBuffWithTheMacroOffNeitherBuffsNorTopsUpVitals
#[test]
fn a_force_buff_with_the_macro_off_neither_buffs_nor_tops_up_vitals() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    automation.health = (10, 100);
    let mut bread = common::item(60, "Bread", 0x20);
    bread.booster_vital = 2;
    automation.item_entries = vec![bread];
    let (mut world, mut plugin) = open(automation);
    world.select_object(60);
    world.act(&mut plugin, "AddSelectedConsumable");

    // The macro stays OFF, so nothing in the main list runs at all.
    world.act(&mut plugin, "ForceBuff");
    ticks(&mut world, &mut plugin, 6, 0.3);

    assert!(world.cast_spell_ids().is_empty());
    assert!(world.automation().used_item_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ASkippedBuffFamilyWarnsOnceInChatAndMirrorsToTheHostLog
#[test]
fn a_skipped_buff_family_warns_once_in_chat_and_mirrors_to_the_host_log() {
    let (mut world, mut plugin) = open(caster(5, vec![common::spell(1, 10, LIFE_MAGIC)]));
    world.vt(&mut plugin, "log Misc on");

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 12, 0.3);

    assert!(world.cast_spell_ids().is_empty());
    const WARNING: &str = "Buff skipped: no known spell in the class of Spell 1.";
    assert_eq!(
        chat_count(&world, |m| m.contains(WARNING)),
        1,
        "{:?}",
        world.chat()
    );
    assert_eq!(
        world
            .infos()
            .iter()
            .filter(|line| line.contains(WARNING))
            .count(),
        1,
        "{:?}",
        world.infos()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: EnablingBuffingWithTheMacroOffCastsNothing
#[test]
fn enabling_buffing_with_the_macro_off_casts_nothing() {
    let (mut world, mut plugin) = open(buff_pass_automation());

    world.vt(&mut plugin, "opt set ManaChargesWhenOff true");
    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(false),
    );
    ticks(&mut world, &mut plugin, 5, 0.3);

    set_option(
        &mut world,
        &mut plugin,
        "EnableBuffing",
        ExpressionValue::Boolean(true),
    );
    ticks(&mut world, &mut plugin, 20, 0.3);

    assert!(world.cast_spell_ids().is_empty());

    // And the macro still starts it, so the pin is not vacuous.
    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 4, 0.3);
    assert!(!world.cast_spell_ids().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ARechargeWithNothingToUseHoldsThePassAsTheReferenceDoes
#[test]
fn a_recharge_with_nothing_to_use_holds_the_pass_as_the_reference_does() {
    // Health is low and nothing can answer it: no kit, no food, no spell.
    let mut automation = FakeAutomation::with_full_vitals();
    automation.health = (10, 100);
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleCombat");

    let mut held = Vec::new();
    for _ in 0..12 {
        tick(&mut world, &mut plugin, 0.3);
        held.extend(
            plugin
                .tank()
                .unwrap()
                .running_rule_names()
                .into_iter()
                .filter(|name| name == "RechargeSelfNormal"),
        );
    }

    assert!(!held.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: WithTheMacroOffOnlyRefillWieldedManaMayRun
#[test]
fn with_the_macro_off_only_refill_wielded_mana_may_run() {
    let mut automation = caster(300, vec![common::spell(1, 10, LIFE_MAGIC)]);
    automation.health = (10, 100);
    let mut bread = common::item(60, "Bread", 0x20);
    bread.booster_vital = 2;
    automation.item_entries = vec![bread];
    let (mut world, mut plugin) = open(automation);
    world.select_object(60);
    world.act(&mut plugin, "AddSelectedConsumable");
    world.vt(&mut plugin, "opt set ManaChargesWhenOff true");

    let mut ran_with_macro_off = BTreeSet::new();
    for _ in 0..20 {
        tick(&mut world, &mut plugin, 0.3);
        ran_with_macro_off.extend(plugin.tank().unwrap().running_rule_names());
    }

    assert!(
        ran_with_macro_off
            .iter()
            .all(|name| name == "RefillWieldedMana"),
        "{ran_with_macro_off:?}"
    );
    assert!(!ran_with_macro_off.contains("RechargeSelfNormal"));
    assert!(!ran_with_macro_off.contains("BuffSelf"));
    assert!(world.cast_spell_ids().is_empty());
    assert!(world.automation().used_item_ids.is_empty());

    // The same world with the macro ON runs both of them, so the
    // assertions above are about the GATE, not about an empty fixture.
    world.act(&mut plugin, "ToggleCombat");
    let mut ran_with_macro_on = BTreeSet::new();
    for _ in 0..20 {
        tick(&mut world, &mut plugin, 0.3);
        ran_with_macro_on.extend(plugin.tank().unwrap().running_rule_names());
    }

    assert!(
        ran_with_macro_on.contains("RechargeSelfNormal"),
        "{ran_with_macro_on:?}"
    );
    assert!(!world.automation().used_item_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AForceEndsWhenTheStampsItZeroedHaveBeenRecast
#[test]
fn a_force_ends_when_the_stamps_it_zeroed_have_been_recast() {
    let mut automation = buff_pass_automation();
    // Spells 1 and 2 are up with hours left: due ONLY under the force.
    automation.active_enchantments = vec![active(1, 10, 1, 1800.0), active(2, 20, 1, 1800.0)];
    // Spell 3 is genuinely due and can never land, so the pick is never
    // empty. It sorts last, so it does not starve the two forced ones.
    automation.refused_cast_spell_ids.insert(3);
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleCombat");
    world.act(&mut plugin, "ForceBuff");

    ticks(&mut world, &mut plugin, 40, 0.3);
    assert_eq!(world.cast_spell_ids(), [1, 2]);

    ticks(&mut world, &mut plugin, 20, 0.3);
    assert_eq!(world.cast_spell_ids(), [1, 2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CancelForceBuffStopsTheForceOnTheCall
#[test]
fn cancel_force_buff_stops_the_force_on_the_call() {
    let mut automation = buff_pass_automation();
    automation.active_enchantments = vec![
        active(1, 10, 1, 1800.0),
        active(2, 20, 1, 1800.0),
        active(3, 30, 1, 1800.0),
    ];
    let (mut world, mut plugin) = open(automation);
    world.act(&mut plugin, "ToggleCombat");

    world.act(&mut plugin, "ForceBuff");
    tick(&mut world, &mut plugin, 0.3);
    let casts_before_cancel = world.cast_spell_ids().len();

    world.act(&mut plugin, "CancelForceBuff");

    ticks(&mut world, &mut plugin, 20, 0.3);
    assert_eq!(world.cast_spell_ids().len(), casts_before_cancel);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FastCastBuffsHoldsForwardOnlyUntilInstantBuffCastEnds
#[test]
fn fast_cast_buffs_holds_forward_only_until_instant_buff_cast_ends() {
    let mut automation = FakeAutomation::default();
    automation.skills = vec![skill(1, "Life Magic", 300)];
    automation.set_known_self_buffs(vec![common::spell(1, 10, LIFE_MAGIC)]);
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "opt set FastCastBuffs true");

    world.act(&mut plugin, "ToggleCombat");
    world.act(&mut plugin, "ForceBuff");
    tick(&mut world, &mut plugin, 0.0);

    let intents = world.automation().movement_intents.clone();
    assert_eq!(intents.len(), 1, "{intents:?}");
    assert!(intents[0].forward);
    assert_eq!(world.automation().clear_movement_count, 0);

    tick(&mut world, &mut plugin, 0.25);

    assert_eq!(world.automation().clear_movement_count, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FastCastBuffsNeverAppliesForwardMovementToWarSpells
#[test]
fn fast_cast_buffs_never_applies_forward_movement_to_war_spells() {
    let mut automation = FakeAutomation::default();
    automation.skills = vec![skill(1, "Life Magic", 300)];
    let mut war = common::spell(1, 10, LIFE_MAGIC);
    war.school = 34;
    automation.set_known_self_buffs(vec![war]);
    let (mut world, mut plugin) = open(automation);
    world.vt(&mut plugin, "opt set FastCastBuffs true");

    world.act(&mut plugin, "ToggleCombat");
    world.act(&mut plugin, "ForceBuff");
    tick(&mut world, &mut plugin, 0.0);

    assert!(world.automation().movement_intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ADepartedIdIsRetiredEvenWhenTheEntryCountIsUnchanged
#[test]
fn a_departed_id_is_retired_even_when_the_entry_count_is_unchanged() {
    let mut tracker = BuffDueTracker::new();
    tracker.observe(&[active(1, 10, 1, 600.0), active(2, 20, 1, 600.0)]);
    tracker.force_all();
    let forced: BTreeSet<u32> = tracker.forced_spell_ids().iter().copied().collect();
    assert_eq!(forced.into_iter().collect::<Vec<_>>(), [1, 2]);

    // Spell 1 drops; spell 2 gains a second layer. Two entries either way.
    tracker.observe(&[active(2, 20, 1, 600.0), active(2, 20, 2, 900.0)]);

    let forced: BTreeSet<u32> = tracker.forced_spell_ids().iter().copied().collect();
    assert_eq!(forced.into_iter().collect::<Vec<_>>(), [2]);
}

/// Life Magic and one buff whose words are "abracadabra"; the server took
/// the request and has said nothing yet.
fn awaiting_launch() -> FakeAutomation {
    let mut automation = caster(300, Vec::new());
    automation.object_id = 0x5000_0001;
    let mut spell = common::spell(1, 10, LIFE_MAGIC);
    spell.saying = "abracadabra".to_owned();
    automation.set_known_self_buffs(vec![spell]);
    automation.suppress_cast_completion = true;
    automation
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnOwnTellDuringTheLaunchWaitDoesNotDropTheCastLatch
#[test]
fn an_own_tell_during_the_launch_wait_does_not_drop_the_cast_latch() {
    let (mut world, mut plugin) = open(awaiting_launch());

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 3, 0.3);
    assert_eq!(buff_cast_requests(&world), 1);

    world.post_chat_from(0x5000_0001, 3, "meet me at the portal", 0);
    ticks(&mut world, &mut plugin, 3, 0.3);

    assert_eq!(buff_cast_requests(&world), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: OnlyAGestureEndsTheLaunchWaitNotTypedLocalSpeech
#[test]
fn only_a_gesture_ends_the_launch_wait_not_typed_local_speech() {
    let (mut world, mut plugin) = open(awaiting_launch());

    world.act(&mut plugin, "ToggleCombat");
    ticks(&mut world, &mut plugin, 3, 0.3);
    assert_eq!(buff_cast_requests(&world), 1);

    // The same words typed into local chat are not a gesture: they carry
    // the plain log type, and the wait goes on.
    world.post_chat_from(0x5000_0001, 0, "hocus pocus", 0);
    ticks(&mut world, &mut plugin, 3, 0.3);
    assert_eq!(buff_cast_requests(&world), 1);

    // A DIFFERENT spell's words, gestured by us: this wait is over, the
    // latch drops, and the pass re-derives the same pick.
    world.post_chat_from(0x5000_0001, 0, "hocus pocus", SPELLCASTING_TEXT_TYPE);
    ticks(&mut world, &mut plugin, 3, 0.3);

    assert_eq!(buff_cast_requests(&world), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ManagingTheFellowshipIsAProfileChoice
#[test]
fn managing_the_fellowship_is_a_profile_choice() {
    for (manages, status) in [
        (true, "Not in a fellowship"),
        (false, "Fellow manager disabled"),
    ] {
        let (mut world, mut plugin) = open(FakeAutomation::default());
        set_option(
            &mut world,
            &mut plugin,
            "AutoFellowManagement",
            ExpressionValue::Boolean(manages),
        );

        world.act(&mut plugin, "ToggleCombat");
        ticks(&mut world, &mut plugin, 4, 0.3);

        assert_eq!(
            world.ui_text(&mut plugin, "FellowshipManagerStatus"),
            status,
            "{manages}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedExtraBuffExemplarIsVisibleAndRemovableFromTheBuffUi
#[test]
fn imported_extra_buff_exemplar_is_visible_and_removable_from_the_buff_ui() {
    let key = format!(
        "{}/{}",
        mosstank_stores::directory::SETTINGS_FOLDER,
        mosstank_stores::directory::auto_character_file_name("Imported Extra", "", "usd")
    );
    let mut profile = mosstank_settings::defaults::create();
    profile
        .find_mut("ExtraBuffSpells")
        .unwrap()
        .rows
        .push(VtankRow {
            cells: vec![VtankCell::int(1)],
        });
    let mut automation = FakeAutomation::default();
    automation.name = "Imported Extra".to_owned();
    automation.set_known_self_buffs(vec![common::spell(1, 10, STRENGTH)]);
    let mut world = World::with_automation(automation);
    world.profiles.insert(&key, &profile.render());
    let mut plugin = world.enable();

    assert_eq!(world.ui_list(&mut plugin, "ExtraBuffRows"), ["Spell 1"]);
    world.act_int(&mut plugin, "DeleteExtraBuffAt", 0);

    assert!(world.ui_list(&mut plugin, "ExtraBuffRows").is_empty());
    let usd = VtankDatabase::parse(world.profiles.get(&key).unwrap()).unwrap();
    assert!(usd.find("ExtraBuffSpells").unwrap().rows.is_empty());
}
