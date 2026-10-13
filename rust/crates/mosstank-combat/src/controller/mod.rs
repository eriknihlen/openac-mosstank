//! The fight (`CombatController`): picks the monster to fight from each
//! scan, debuffs it in the reference's twelve-step order, readies the
//! weapon and stance through the gate, and swings (on an executor stepped by
//! the host frame) or casts the best attack spell the character can use;
//! it reads the results from chat and the server's receipts, gives up on
//! monsters it cannot reach, walks to monsters out of reach on a rule of its
//! own, and shares its debuffs with the other clients on this computer.
//!
//! The C#'s callbacks into the panel are bound closures here, and the
//! client, the settings and the crafting hook come in per call as a
//! [`CombatTurn`].

// The C#'s boolean forms are kept as written, to read beside it.
#![allow(clippy::nonminimal_bool)]

mod debuffs;
mod equip;
mod magic;
mod results;
mod swing;
mod targets;

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::combat::{CombatMode, CombatTarget};
use ac_plugin_abi::items::{EquipmentItem, InventoryItem};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::projectile::ProjectilePathResult;
use ac_plugin_abi::world::ProjectilePathKind;
use mosstank_automation::Automation;
use mosstank_automation::cast_tracker::{
    SharedCastTracker, SpellCastOutcomeInfo, SpellCastTracker,
};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
use mosstank_data::attack_spells::AttackSpellCatalog;
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;
use mosstank_settings::model::{CombatSettings, VitalSettings};
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions, ResolvedMonsterRule};
use openac_plugin_compat::number::format_f32;

use crate::debuff::{DebuffIdentity, DebuffTracker};
use crate::failure::CombatFailureTracker;
use crate::gate::{CombatModeGate, SharedCombatModeGate};
use crate::health::MonsterHealthTracker;
use crate::item_debuff::CombatDebuffSource;
use crate::mover::{NO_FACE_HEADING_STAMP, NavigationMover};
use crate::selection::CombatTargetCandidate;
use crate::{MacroLog, MacroLogChannel, post};

pub use equip::AmmunitionPlan;

/// The crafting the ammunition choice may fall back on (the C#'s
/// `BindAmmunitionCraftRequest`): plans are opaque handles the crafting
/// owner understands.
pub trait AmmunitionCrafting {
    /// A plan that makes `count` of `name`, if one can be made now.
    fn resolve(&mut self, name: &str, count: i32) -> Option<u32>;
    /// Starts a plan; true when it went out.
    fn request(&mut self, plan: u32) -> bool;
}

/// What one call works with.
#[allow(missing_debug_implementations)]
pub struct CombatTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The combat settings (the fight teaches the monster facts species
    /// words, so they are mutable).
    pub settings: &'a mut CombatSettings,
    /// The vital settings (the drain floor, the gate's peace retries).
    pub vitals: &'a VitalSettings,
    /// The crafting, when the plugin has it.
    pub crafting: Option<&'a mut dyn AmmunitionCrafting>,
}

impl CombatTurn<'_> {
    /// The call's view with the settings read-only, which is how every part
    /// of the fight but the scan reads them.
    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            automation: &mut *self.automation,
            settings: &*self.settings,
            vitals: self.vitals,
            crafting: reborrow_crafting(&mut self.crafting),
        }
    }
}

/// One call's view with the settings read-only.
#[allow(missing_debug_implementations)]
pub struct Ctx<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The combat settings.
    pub settings: &'a CombatSettings,
    /// The vital settings.
    pub vitals: &'a VitalSettings,
    /// The crafting, when the plugin has it.
    pub crafting: Option<&'a mut dyn AmmunitionCrafting>,
}

/// How long after the server closes an attack sequence result text still
/// belongs to it, seconds.
const PHYSICAL_RESULT_TEXT_TAIL_SECONDS: f64 = 2.0;
/// How long a sent swing may stay unanswered, seconds.
const UNANSWERED_SWING_SECONDS: f64 = 4.5;
/// How much nearer the monster must come for the wait to start over, metres.
const SWING_CLOSING_PROGRESS_METERS: f32 = 0.5;
/// Seconds navigation is held after a kill.
const POST_KILL_NAVIGATION_LOCK_SECONDS: f64 = 3.0;
/// The power bar's release tolerance.
const POWER_RELEASE_EPSILON: f32 = 0.005;
/// The breakable turn's tolerance, degrees.
const BREAKABLE_TURN_TOLERANCE_DEGREES: f32 = 2.0;
/// How often the stalled-health check runs, seconds.
const GHOST_SWEEP_INTERVAL_SECONDS: f64 = 6.271;
/// The two attack schools, by skill id.
const WAR_MAGIC_SKILL: u32 = 34;
const VOID_MAGIC_SKILL: u32 = 43;

const RANDOM_DAMAGE_CYCLE: [MonsterDamageType; 7] = [
    MonsterDamageType::Pierce,
    MonsterDamageType::Bludgeon,
    MonsterDamageType::Slash,
    MonsterDamageType::Acid,
    MonsterDamageType::Electric,
    MonsterDamageType::Cold,
    MonsterDamageType::Fire,
];

/// A debuff from an item, in flight.
#[derive(Debug, Clone)]
struct PendingItemDebuff {
    source: CombatDebuffSource,
    target_object_id: u32,
    target_name: String,
    item_name: String,
    dispatched_at: f64,
    physical_completion_revision: i64,
    power: f32,
    attack_completed_at: Option<f64>,
    recovery_sent: bool,
}

/// What one turn of the decision did with the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttackPassOutcome {
    Claimed,
    Retry,
}

/// The fight.
#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
pub struct CombatController {
    debuffs: DebuffTracker,
    failures: CombatFailureTracker,
    health: MonsterHealthTracker,
    remote_cast_cursor: i64,
    cast_sharing_enabled: Box<dyn FnMut() -> bool>,
    cast_sharing_peer_tag: Box<dyn FnMut() -> Option<String>>,
    targets: Vec<CombatTarget>,
    combat_spell_snapshot: Option<Vec<SpellInfo>>,
    attack_spell_snapshot: Option<Vec<SpellInfo>>,
    attack_catalog: AttackSpellCatalog,
    now: f64,
    last_elapsed_seconds: f64,
    planned_weapon: u32,
    planned_offhand: Option<u32>,
    until_scan: f64,
    acquisition_range: f64,
    target_id: u32,
    target_rule: Option<ResolvedMonsterRule>,
    target_name: String,
    target_distance: f32,
    target_text: String,
    mode_text: String,
    last_mode: CombatMode,
    paused: bool,
    observed_physical_completion: i64,
    observed_attack_cast_completion: i64,
    pending_physical_target: u32,
    pending_attack_spell: u32,
    pass_elements: Vec<(MonsterRuleActions, u32, MonsterDamageType)>,
    pass_debuff_sources: HashMap<(DebuffIdentity, u32), Vec<CombatDebuffSource>>,
    pass_clearance: Vec<((u32, ProjectilePathKind), ProjectilePathResult)>,
    pass_debuff_spells: HashMap<DebuffIdentity, Option<SpellInfo>>,
    pass_deliverable: HashMap<(MonsterDamageType, u32), bool>,
    pass_components: HashMap<u32, bool>,
    pass_invalid_targets: HashSet<u32>,
    plan_kept_the_monster_in_play: bool,
    pass_cleared_actions: HashMap<u32, MonsterActionFlags>,
    pass_candidates: HashMap<u32, Option<CombatTargetCandidate>>,
    pass_candidate_range: f64,
    pass_target_reasons: HashMap<u32, String>,
    pass_equipment: Option<Vec<EquipmentItem>>,
    pass_inventory: Option<Vec<InventoryItem>>,
    pass_spell_refusals: HashMap<u32, (String, i32, String)>,
    pass_ammunition_counts: Option<HashMap<String, i32>>,
    pass_ammunition_answers: HashMap<String, bool>,
    pending_attack_target: u32,
    last_target_id: u32,
    pending_item_debuff: Option<PendingItemDebuff>,
    observed_chat_sequence: u64,
    item_transaction_chat_sequence: u64,
    observed_item_completion: i64,
    combat_policy_suspended: bool,
    breakable_turn_owned: bool,
    approach_mover: NavigationMover,
    breakable_turn_face_heading_stamp: f64,
    random_damage_index: usize,
    observed_jiggle_cast_completion: i64,
    selection_jiggle_active: bool,
    selection_jiggle_previous_player: bool,
    next_selection_jiggle_at: f64,
    selection_jiggle_until: f64,
    action_locks: SharedActionLocks,
    holds_route_after_kill: Box<dyn Fn() -> bool>,
    physical_swing_sent_at: f64,
    physical_swing_armed_at: f64,
    physical_swing_closest_distance: f32,
    physical_result_armed: bool,
    physical_result_target_id: u32,
    physical_result_target_name: String,
    physical_result_incarnation: u16,
    physical_completed_at: f64,
    suspend_pass: Box<dyn FnMut()>,
    resume_pass: Box<dyn FnMut()>,
    turn_holds_pass: bool,
    breakable_turn_target_id: u32,
    cast_tracker: SharedCastTracker,
    game_info: VtankGameInfoDatabase,
    enabled: bool,
    combat_control: bool,
    status: String,
    held_item_driven_by_turn: bool,
    log: Option<MacroLog>,
    gate: SharedCombatModeGate,
    /// Whether the gate's stops are the fight's to run: true for the gate
    /// the fight made itself, false once a gate is bound from outside, whose
    /// stops belong to the binder.
    owns_gate_stops: bool,
    gate_ammunition_plan: Option<AmmunitionPlan>,
    swing_armed: bool,
    swing_timer_running: bool,
    swing_target_id: u32,
    swing_target_name: String,
    swing_height: ac_plugin_abi::combat::AttackHeight,
    swing_power: f32,
    next_swing_at: f64,
    swing_stop_count: i32,
    ring_candidate_count: i32,
    planned_weapon_search_found_nothing: bool,
    attack_has_no_usable_weapon: bool,
    reported_give_ups: HashMap<u32, String>,
    cannot_attack_reason: Option<String>,
    posted_attack_warnings: HashSet<String>,
    decision_actions: Option<MonsterRuleActions>,
    next_target_diagnostic: f64,
    frame_advanced_since_tick: f64,
    until_ghost_sweep: f64,
    cast_outcomes: Vec<SpellCastOutcomeInfo>,
}

impl Default for CombatController {
    fn default() -> Self {
        Self::new()
    }
}

impl CombatController {
    /// A stopped fight with a gate, a cast tracker and a cooldown table of
    /// its own and no game database.
    #[must_use]
    pub fn new() -> Self {
        Self::with(
            VtankGameInfoDatabase::empty(),
            std::rc::Rc::new(std::cell::RefCell::new(SpellCastTracker::new())),
        )
    }

    /// A stopped fight over a game database and a shared cast tracker.
    #[must_use]
    pub fn with(game_info: VtankGameInfoDatabase, cast_tracker: SharedCastTracker) -> Self {
        let gate: SharedCombatModeGate =
            std::rc::Rc::new(std::cell::RefCell::new(CombatModeGate::new()));
        let mut approach_mover = NavigationMover::new();
        approach_mover.bind_combat_mode_gate(gate.clone());
        Self {
            debuffs: DebuffTracker::new(),
            failures: CombatFailureTracker::new(),
            health: MonsterHealthTracker::new(),
            remote_cast_cursor: 0,
            cast_sharing_enabled: Box::new(|| true),
            cast_sharing_peer_tag: Box::new(|| None),
            targets: Vec::new(),
            combat_spell_snapshot: None,
            attack_spell_snapshot: None,
            attack_catalog: AttackSpellCatalog::build(&[]),
            now: 0.0,
            last_elapsed_seconds: 0.0,
            planned_weapon: 0,
            planned_offhand: None,
            until_scan: 0.0,
            acquisition_range: 0.0,
            target_id: 0,
            target_rule: None,
            target_name: String::new(),
            target_distance: 0.0,
            target_text: "Target  \u{2014}".to_owned(),
            mode_text: "Mode  Unknown".to_owned(),
            last_mode: CombatMode::Unknown,
            paused: false,
            observed_physical_completion: 0,
            observed_attack_cast_completion: 0,
            pending_physical_target: 0,
            pending_attack_spell: 0,
            pass_elements: Vec::new(),
            pass_debuff_sources: HashMap::new(),
            pass_clearance: Vec::new(),
            pass_debuff_spells: HashMap::new(),
            pass_deliverable: HashMap::new(),
            pass_components: HashMap::new(),
            pass_invalid_targets: HashSet::new(),
            plan_kept_the_monster_in_play: false,
            pass_cleared_actions: HashMap::new(),
            pass_candidates: HashMap::new(),
            pass_candidate_range: f64::NAN,
            pass_target_reasons: HashMap::new(),
            pass_equipment: None,
            pass_inventory: None,
            pass_spell_refusals: HashMap::new(),
            pass_ammunition_counts: None,
            pass_ammunition_answers: HashMap::new(),
            pending_attack_target: 0,
            last_target_id: 0,
            pending_item_debuff: None,
            observed_chat_sequence: 0,
            item_transaction_chat_sequence: 0,
            observed_item_completion: 0,
            combat_policy_suspended: false,
            breakable_turn_owned: false,
            approach_mover,
            breakable_turn_face_heading_stamp: NO_FACE_HEADING_STAMP,
            random_damage_index: 0,
            observed_jiggle_cast_completion: 0,
            selection_jiggle_active: false,
            selection_jiggle_previous_player: false,
            next_selection_jiggle_at: 0.0,
            selection_jiggle_until: 0.0,
            action_locks: ActionLockTable::shared(),
            holds_route_after_kill: Box::new(|| false),
            physical_swing_sent_at: f64::NEG_INFINITY,
            physical_swing_armed_at: f64::NEG_INFINITY,
            physical_swing_closest_distance: f32::INFINITY,
            physical_result_armed: false,
            physical_result_target_id: 0,
            physical_result_target_name: String::new(),
            physical_result_incarnation: 0,
            physical_completed_at: f64::NEG_INFINITY,
            suspend_pass: Box::new(|| {}),
            resume_pass: Box::new(|| {}),
            turn_holds_pass: false,
            breakable_turn_target_id: 0,
            cast_tracker,
            game_info,
            enabled: false,
            combat_control: false,
            status: "Combat off".to_owned(),
            held_item_driven_by_turn: false,
            log: None,
            gate,
            owns_gate_stops: true,
            gate_ammunition_plan: None,
            swing_armed: false,
            swing_timer_running: false,
            swing_target_id: 0,
            swing_target_name: String::new(),
            swing_height: ac_plugin_abi::combat::AttackHeight::Medium,
            swing_power: 0.0,
            next_swing_at: 0.0,
            swing_stop_count: 0,
            ring_candidate_count: 0,
            planned_weapon_search_found_nothing: false,
            attack_has_no_usable_weapon: false,
            reported_give_ups: HashMap::new(),
            cannot_attack_reason: None,
            posted_attack_warnings: HashSet::new(),
            decision_actions: None,
            next_target_diagnostic: 0.0,
            frame_advanced_since_tick: 0.0,
            until_ghost_sweep: GHOST_SWEEP_INTERVAL_SECONDS,
            cast_outcomes: Vec::new(),
        }
    }

    // --- Bindings.

    /// The shared cast tracker.
    #[must_use]
    pub fn cast_tracker(&self) -> SharedCastTracker {
        self.cast_tracker.clone()
    }

    /// The stance gate the fight readies through.
    #[must_use]
    pub fn gate(&self) -> SharedCombatModeGate {
        self.gate.clone()
    }

    /// Shares a gate (the panel's): the approach mover steers through it too.
    /// The binder runs the gate's stops (the C#'s gate built with the panel's
    /// `StopMacroFromGate`); the fight no longer does.
    pub fn bind_combat_mode_gate(&mut self, gate: SharedCombatModeGate) {
        self.gate = gate.clone();
        self.owns_gate_stops = false;
        self.approach_mover.bind_combat_mode_gate(gate);
    }

    /// Lets the fight freeze the whole rule pass while the character turns.
    pub fn bind_pass_suspension(&mut self, suspend: Box<dyn FnMut()>, resume: Box<dyn FnMut()>) {
        self.suspend_pass = suspend;
        self.resume_pass = resume;
    }

    /// Shares the macro's cooldown table; a kill holds navigation off while
    /// `holds_route_after_kill` says looting wants it.
    pub fn bind_action_locks(
        &mut self,
        locks: SharedActionLocks,
        holds_route_after_kill: Box<dyn Fn() -> bool>,
    ) {
        self.action_locks = locks.clone();
        self.holds_route_after_kill = holds_route_after_kill;
        self.cast_tracker
            .borrow_mut()
            .bind_action_locks(locks.clone());
        self.gate.borrow_mut().bind_action_locks(locks);
    }

    /// Whether casts are shared with the other clients, and which tag a
    /// peer must carry for its casts to be taken in (blank: every peer);
    /// both asked each time.
    pub fn bind_cast_sharing(
        &mut self,
        enabled: Box<dyn FnMut() -> bool>,
        peer_tag: Box<dyn FnMut() -> Option<String>>,
    ) {
        self.cast_sharing_enabled = enabled;
        self.cast_sharing_peer_tag = peer_tag;
    }

    /// Where the macro log lines go.
    pub fn set_log(&mut self, log: Option<MacroLog>) {
        self.log = log;
    }

    /// Takes a newer game database.
    pub fn replace_game_info(&mut self, game_info: VtankGameInfoDatabase) {
        self.game_info = game_info;
    }

    /// The game database every decision reads.
    #[must_use]
    pub fn game_info(&self) -> &VtankGameInfoDatabase {
        &self.game_info
    }

    // --- State.

    /// Whether the macro (the fight) runs.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// What the fight is doing.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The target line for the panel.
    #[must_use]
    pub fn target_text(&self) -> &str {
        &self.target_text
    }

    /// The mode line for the panel.
    #[must_use]
    pub fn mode_text(&self) -> &str {
        &self.mode_text
    }

    /// Whether a monster is the target.
    #[must_use]
    pub fn has_target(&self) -> bool {
        self.target_id != 0
    }

    /// The controller's clock, seconds.
    #[must_use]
    pub fn clock_seconds(&self) -> f64 {
        self.now
    }

    /// Whether a debuff from an item is in flight.
    #[must_use]
    pub fn has_pending_item_debuff(&self) -> bool {
        self.pending_item_debuff.is_some()
    }

    /// Whether a cast from a held wand is in flight (it holds the pass).
    #[must_use]
    pub fn held_item_cast_in_flight(&self) -> bool {
        self.pending_item_debuff.as_ref().is_some_and(|pending| {
            pending.source.kind == crate::item_debuff::CombatDebuffSourceKind::CasterItem
        })
    }

    /// Whether a learned debuff is unanswered (it holds the pass).
    #[must_use]
    pub fn learned_debuff_cast_in_flight(&self) -> bool {
        self.debuffs.has_pending()
    }

    /// Whether a turn to face the target holds the pass.
    #[must_use]
    pub fn turn_holds_pass(&self) -> bool {
        self.turn_holds_pass
    }

    /// The button's label.
    #[must_use]
    pub fn button_text(&self) -> &'static str {
        if self.enabled {
            "Stop Macro"
        } else {
            "Run Macro"
        }
    }

    /// The ended casts the fight saw while it drained the shared tracker,
    /// for the other owners of casts (the buff rule), oldest first.
    pub fn take_cast_outcomes(&mut self) -> Vec<SpellCastOutcomeInfo> {
        std::mem::take(&mut self.cast_outcomes)
    }

    /// What the attack holds the turn for: the monster, how far, and the
    /// status.
    #[must_use]
    pub fn running_detail(&self) -> String {
        if self.target_id == 0 {
            return if self.pending_item_debuff.is_none() {
                self.status.clone()
            } else {
                format!("no target; {}", self.status)
            };
        }
        let distance = self
            .targets
            .iter()
            .find(|target| target.object_id == self.target_id)
            .map_or_else(
                || "not in the last scan".to_owned(),
                |target| {
                    format!(
                        "{} m",
                        format_f32(target.distance, "0.0").unwrap_or_default()
                    )
                },
            );
        format!(
            "{} (0x{:08X}, {distance}): {}",
            self.target_name, self.target_id, self.status
        )
    }

    /// Whether this monster is one the fight follows and has not given up
    /// on.
    #[must_use]
    pub fn is_tracked_and_not_blacklisted(&self, object_id: u32) -> bool {
        object_id != 0
            && self.failures.is_known(object_id)
            && self.failures.reason(object_id, self.now)
                == crate::failure::CombatSuppressionReason::None
    }

    /// `/vt fakeimp`.
    pub fn record_fake_imperil(&mut self, target_object_id: u32) -> bool {
        if target_object_id == 0 {
            return false;
        }
        self.debuffs.record_fake_imperil(target_object_id, self.now);
        true
    }

    // --- Helpers every part uses.

    fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    fn log_line(&self, channel: MacroLogChannel, text: &str) {
        if let Some(log) = &self.log {
            log(channel, text);
        }
    }

    fn has_log(&self) -> bool {
        self.log.is_some()
    }

    fn post_attack_warning(&mut self, automation: &mut dyn Automation, text: &str) {
        if !self.posted_attack_warnings.insert(text.to_owned()) {
            return;
        }
        post(automation, &format!("[MossTank] {text}"));
    }

    /// A warning, once, beside the attack's own.
    pub fn post_warning_once(&mut self, automation: &mut dyn Automation, text: &str) {
        self.post_attack_warning(automation, text);
    }

    fn find_target(&self, object_id: u32) -> CombatTarget {
        self.targets
            .iter()
            .find(|target| target.object_id == object_id)
            .cloned()
            .unwrap_or_default()
    }

    fn note_target_reason(&mut self, object_id: u32, reason: &str) {
        if object_id != 0 {
            self.pass_target_reasons
                .insert(object_id, reason.to_owned());
        }
    }

    fn invalidate_for_pass(&mut self, object_id: u32) {
        if object_id == 0 {
            return;
        }
        self.pass_invalid_targets.insert(object_id);
        self.pass_candidates.remove(&object_id);
    }

    /// The rule with what this pass learned taken off it.
    fn with_pass_cleared_actions(
        &self,
        object_id: u32,
        rule: &ResolvedMonsterRule,
    ) -> ResolvedMonsterRule {
        match self.pass_cleared_actions.get(&object_id) {
            Some(&cleared) => {
                let mut actions = rule.actions().clone();
                actions.flags = MonsterActionFlags(actions.flags.0 & !cleared.0);
                ResolvedMonsterRule {
                    rule: rule.rule.with_actions(actions),
                    index: rule.index,
                    evaluation_error: rule.evaluation_error.clone(),
                }
            }
            None => rule.clone(),
        }
    }

    fn clear_actions_for_pass(&mut self, object_id: u32, flags: MonsterActionFlags) {
        if object_id == 0 {
            return;
        }
        let held = self
            .pass_cleared_actions
            .get(&object_id)
            .copied()
            .unwrap_or(MonsterActionFlags::NONE);
        self.pass_cleared_actions.insert(object_id, held | flags);
        self.pass_candidates.remove(&object_id);
    }

    /// The target's stored actions (the default row's with no target).
    fn target_actions(&self) -> MonsterRuleActions {
        self.target_rule
            .as_ref()
            .map_or_else(MonsterRuleActions::default, |rule| rule.actions().clone())
    }

    /// The target's actions as this pass sees them.
    fn pass_actions(&self) -> MonsterRuleActions {
        match &self.target_rule {
            Some(rule) => self
                .with_pass_cleared_actions(self.target_id, rule)
                .actions()
                .clone(),
            None => MonsterRuleActions::default(),
        }
    }

    /// The columns this one decision works from.
    fn decision_actions(&self) -> MonsterRuleActions {
        self.decision_actions
            .clone()
            .unwrap_or_else(|| self.pass_actions())
    }

    fn arm_post_kill_navigation_lock(&mut self) {
        if !(self.holds_route_after_kill)() {
            return;
        }
        self.action_locks.borrow_mut().arm(
            ActionLockKind::Navigation,
            POST_KILL_NAVIGATION_LOCK_SECONDS,
        );
    }

    /// Runs a stop the gate asked for (the C#'s `stopMacro` callback, which
    /// the controller binds to its own disable on the gate it made itself).
    /// A bound gate's stops are left for the binder.
    fn drain_gate_stops(&mut self, automation: &mut dyn Automation) {
        if !self.owns_gate_stops {
            return;
        }
        loop {
            let stop = self.gate.borrow_mut().take_stop_request();
            let Some(notice) = stop else {
                break;
            };
            self.disable(automation, &notice);
        }
    }

    fn pass_equipment(&mut self, automation: &mut dyn Automation) -> Vec<EquipmentItem> {
        if let Some(equipment) = &self.pass_equipment {
            return equipment.clone();
        }
        let equipment = if automation.equipment_available() {
            automation.owned_equipment()
        } else {
            Vec::new()
        };
        self.pass_equipment = Some(equipment.clone());
        equipment
    }

    fn pass_inventory(&mut self, automation: &mut dyn Automation) -> Vec<InventoryItem> {
        if let Some(inventory) = &self.pass_inventory {
            return inventory.clone();
        }
        let inventory = automation.owned_items();
        self.pass_inventory = Some(inventory.clone());
        inventory
    }

    fn clear_pass_memos(&mut self) {
        self.pass_elements.clear();
        self.pass_debuff_sources.clear();
        self.pass_debuff_spells.clear();
        self.pass_deliverable.clear();
        self.pass_components.clear();
        self.pass_spell_refusals.clear();
        self.pass_clearance.clear();
        self.pass_ammunition_counts = None;
        self.pass_ammunition_answers.clear();
        self.gate_ammunition_plan = None;
        self.pass_invalid_targets.clear();
        self.pass_cleared_actions.clear();
        self.pass_candidates.clear();
        self.pass_target_reasons.clear();
        self.pass_candidate_range = f64::NAN;
        self.pass_equipment = None;
        self.pass_inventory = None;
    }

    // --- The macro's switch.

    /// Clears every action lock the fight holds.
    pub fn clear_action_locks(&mut self, automation: &mut dyn Automation) {
        automation.abort_attack();
        self.reset_swing_executor();
        self.stop_approach_movement(automation);
        self.stop_breakable_turn_movement(automation);
        self.stop_selection_jiggle();
        self.pending_physical_target = 0;
        self.pending_attack_spell = 0;
        self.pending_attack_target = 0;
        self.last_target_id = 0;
        self.clear_pending_item_debuff();
        self.debuffs.clear_pending();
        self.gate.borrow_mut().reset();
        self.until_scan = 0.0;
        if self.enabled {
            self.set_status("Action locks cleared");
        }
    }

    /// Starts or stops the macro.
    pub fn toggle(&mut self, cx: &mut CombatTurn<'_>) {
        if self.enabled {
            self.disable(cx.automation, "Macro stopped");
            post(cx.automation, "[MossTank] Macro stopped.");
            return;
        }
        if !cx.automation.is_available() {
            self.set_status("Not in world");
            return;
        }
        self.combat_control = cx.automation.combat_control(true);
        self.enabled = true;
        self.paused = false;
        self.combat_policy_suspended = !cx.settings.enabled;
        self.remote_cast_cursor = 0;
        self.until_scan = 0.0;
        self.set_status(if cx.settings.enabled {
            "Scanning for targets"
        } else {
            "Combat disabled"
        });
        post(cx.automation, "[MossTank] Macro started.");
    }

    /// The attack won or lost its turn.
    pub fn set_paused(&mut self, automation: &mut dyn Automation, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        if paused && self.enabled {
            // Losing the turn tears down the attack's own work, not the walk.
            self.disarm_swing_executor(automation);
            self.stop_breakable_turn_movement(automation);
        } else if self.enabled {
            self.until_scan = 0.0;
        }
    }

    /// One step of getting equipped for a named monster (the panel's
    /// command); true once ready.
    pub fn equip_one_step_for_monster(
        &mut self,
        cx: &mut CombatTurn<'_>,
        monster_name: &str,
    ) -> bool {
        self.clear_pass_memos();
        let target = CombatTarget {
            name: monster_name.trim().to_owned(),
            health_fraction: 1.0,
            seconds_since_health_update: f64::INFINITY,
            ..CombatTarget::default()
        };
        self.target_name.clone_from(&target.name);
        self.target_rule = Some(cx.settings.resolve_rule(&target));
        let mut ctx = cx.ctx();
        if self.tick_equipment(&mut ctx) {
            return false;
        }
        self.try_prepare_attack(&mut ctx)
    }

    /// One turn of the attack.
    pub fn on_tick(&mut self, cx: &mut CombatTurn<'_>, elapsed_seconds: f64) {
        // What the shared cast tracker said since the last call (a re-send,
        // a timeout) is heard first, on the clock it happened on.
        self.drain_cast_events(&mut cx.ctx());
        if !self.enabled {
            return;
        }
        if !cx.automation.is_available() {
            self.disable(cx.automation, "Session ended");
            return;
        }
        if self.paused {
            return;
        }
        if !cx.settings.enabled {
            if self.combat_policy_suspended {
                return;
            }
            self.combat_policy_suspended = true;
            if self.target_id != 0 || self.pending_physical_target != 0 {
                cx.automation.abort_attack();
            }
            self.reset_swing_executor();
            self.pending_physical_target = 0;
            self.pending_attack_spell = 0;
            self.pending_attack_target = 0;
            self.clear_pending_item_debuff();
            self.debuffs.reset();
            self.failures.reset();
            self.clear_target(cx.automation);
            self.set_status("Combat disabled");
            return;
        }
        if self.combat_policy_suspended {
            self.combat_policy_suspended = false;
            self.remote_cast_cursor = 0;
            self.until_scan = 0.0;
            self.set_status("Scanning for targets");
        }

        self.clear_pass_memos();

        // One wall clock between the turn and the frame drivers.
        self.last_elapsed_seconds = elapsed_seconds.max(0.0);
        self.now += (self.last_elapsed_seconds - self.frame_advanced_since_tick).max(0.0);
        self.frame_advanced_since_tick = 0.0;
        let cast_completion = cx.automation.last_cast_completion();
        self.observe_selection_jiggle(cx.automation, cx.settings.do_jiggle, &cast_completion);
        self.tick_selection_jiggle(cx.automation);
        let completion = self.debuffs.observe(&cast_completion, self.now);
        self.on_learned_debuff_completed(cx.automation, completion);
        self.debuffs.expire_pending(self.now, 15.0);

        let current = cx.automation.combat_snapshot();
        self.observe_item_debuff_receipts(&mut cx.ctx());
        self.observe_item_transaction(&mut cx.ctx());
        self.observe_attack_receipts(&current, &cast_completion);
        if current.mode != self.last_mode {
            self.last_mode = current.mode;
            self.mode_text = format!("Mode  {}", crate::mode_name(current.mode));
        }

        self.until_ghost_sweep -= elapsed_seconds.max(0.0);
        if self.until_ghost_sweep <= 0.0 {
            self.until_ghost_sweep = GHOST_SWEEP_INTERVAL_SECONDS;
            self.check_stalled_health_ghost(&mut cx.ctx());
        }

        self.until_scan -= elapsed_seconds.max(0.0);
        if self.until_scan <= 0.0 {
            // The attack's pool is what the character can hit now.
            self.acquisition_range = cx.settings.maximum_range;
            #[allow(clippy::cast_possible_truncation)]
            let range = self.acquisition_range as f32;
            self.targets = cx.automation.hostile_targets(range);
            if self.has_log() {
                self.log_line(
                    MacroLogChannel::Timers,
                    &format!(
                        "Combat scan: {} hostile(s) within {}m",
                        self.targets.len(),
                        openac_plugin_compat::number::format_f64(self.acquisition_range, "0.0")
                            .unwrap_or_default()
                    ),
                );
            }
            self.failures
                .observe_targets(&self.targets, self.now, cx.settings);
            let scanned = self.targets.clone();
            for target in &scanned {
                self.health
                    .observe(target, self.now, &cx.settings.monster_facts);
                cx.settings
                    .monster_facts
                    .learn(target.species_id, Some(&target.species_name));
            }
            let live: HashSet<u32> = self.targets.iter().map(|target| target.object_id).collect();
            self.debuffs.retain_targets(&live);
            self.until_scan = cx.settings.scan_interval_seconds.max(0.05);
            self.refresh_target(&mut cx.ctx());
        }

        if let Some(pending) = &self.pending_item_debuff {
            let kind = pending.source.kind;
            self.set_status(format!("Waiting on {kind:?} debuff"));
            self.held_item_driven_by_turn =
                kind == crate::item_debuff::CombatDebuffSourceKind::CasterItem;
            self.tick_pending_item_debuff(cx.automation, cx.settings, &current);
            return;
        }

        // One monster the character cannot act against must not cost the
        // whole pass: what turned out undeliverable is turned off and the
        // choice made again. With the flight check off there is one attempt.
        let budget = if cx.settings.use_projectile_awareness {
            cx.settings.maximum_collision_checks_per_tick.max(1)
        } else {
            1
        };
        self.run_attack_loop(&mut cx.ctx(), budget);
        // "The monster I picked last time" is a per-pass memory.
        self.last_target_id = self.target_id;
    }

    fn run_attack_loop(&mut self, cx: &mut Ctx<'_>, budget: i32) {
        for attempt in 0..budget {
            self.pass_debuff_sources.clear();
            if attempt > 0 {
                self.refresh_target(cx);
            }
            if self.target_id == 0 {
                self.stop_approach_movement(cx.automation);
                self.set_status("Waiting for a target");
                return;
            }
            self.cannot_attack_reason = None;
            if self.run_attack_attempt(cx) != AttackPassOutcome::Retry {
                self.log_line(
                    MacroLogChannel::Timers,
                    &format!(
                        "Attack evaluation complete. Loop iterations: {}",
                        attempt + 1
                    ),
                );
                return;
            }
            self.log_line(
                MacroLogChannel::RuleInfo,
                &format!(
                    "Attack: {} yielded nothing this pass ({}), choosing again",
                    self.target_name, self.status
                ),
            );
            if let Some(cannot) = self.cannot_attack_reason.clone() {
                let (id, name) = (self.target_id, self.target_name.clone());
                self.report_given_up_target(cx.automation, id, &name, &cannot);
            }
            self.clear_target(cx.automation);
        }
        self.log_line(
            MacroLogChannel::Timers,
            &format!("Attack evaluation complete. Loop iterations: {budget}"),
        );
    }

    fn run_attack_attempt(&mut self, cx: &mut Ctx<'_>) -> AttackPassOutcome {
        if is_known_dead(&self.find_target(self.target_id)) {
            // It died under the pass: choose again from what is left.
            self.set_status(format!("{} is dead", self.target_name));
            self.invalidate_for_pass(self.target_id);
            return AttackPassOutcome::Retry;
        }
        self.stop_approach_movement(cx.automation);
        let pass_actions = self.pass_actions();
        self.decision_actions = Some(self.resolve_random_damage(pass_actions));

        let combat = cx.automation.combat_snapshot();
        match self.tick_debuffs(cx, &combat) {
            debuffs::DebuffArmOutcome::Claimed => {
                if self.status == "Waiting for a target" {
                    self.set_status(format!("Debuffing {}", self.target_name));
                }
                return AttackPassOutcome::Claimed;
            }
            debuffs::DebuffArmOutcome::Retry => return AttackPassOutcome::Retry,
            debuffs::DebuffArmOutcome::Idle => {}
        }

        if self.tick_equipment(cx) {
            if self.status == "Waiting for a target" {
                self.set_status(format!("Changing equipment for {}", self.target_name));
            }
            return AttackPassOutcome::Claimed;
        }

        let decision = self.decision_actions();
        if !decision.attacks() && !decision.uses_streak() {
            self.set_status(format!("Debuffs complete for {}", self.target_name));
            self.invalidate_for_pass(self.target_id);
            return AttackPassOutcome::Retry;
        }

        if !self.try_prepare_attack(cx) {
            // Unsettled equipment is waited on; no usable weapon at all
            // yields the monster.
            return if self.attack_has_no_usable_weapon {
                AttackPassOutcome::Retry
            } else {
                AttackPassOutcome::Claimed
            };
        }

        let combat = cx.automation.combat_snapshot();
        if combat.mode == CombatMode::Magic {
            return self.tick_magic(cx);
        }
        if !matches!(combat.mode, CombatMode::Melee | CombatMode::Missile) {
            self.set_status(format!(
                "Unsupported mode: {}",
                crate::mode_name(combat.mode)
            ));
            return AttackPassOutcome::Claimed;
        }
        self.tick_physical(cx, &combat)
    }

    fn resolve_random_damage(&mut self, actions: MonsterRuleActions) -> MonsterRuleActions {
        if actions.damage_type != MonsterDamageType::Random {
            return actions;
        }
        let damage = RANDOM_DAMAGE_CYCLE[self.random_damage_index];
        self.random_damage_index = (self.random_damage_index + 1) % RANDOM_DAMAGE_CYCLE.len();
        MonsterRuleActions {
            damage_type: damage,
            ..actions
        }
    }

    fn set_target(
        &mut self,
        target: &CombatTarget,
        resolved: ResolvedMonsterRule,
        facts: &mosstank_settings::monster::MonsterFactTable,
    ) {
        self.target_id = target.object_id;
        self.target_rule = Some(resolved);
        self.target_name = if target.name.trim().is_empty() {
            format!("0x{:08X}", target.object_id)
        } else {
            target.name.clone()
        };
        self.target_distance = target.distance;
        self.target_text = format!(
            "Target  {}  {}m",
            self.target_name,
            format_f32(self.target_distance, "0.0").unwrap_or_default()
        );
        self.health
            .set_target(self.target_id, &target.name, self.now);
        // What the host already knows about its health is the first report.
        self.health.observe(target, self.now, facts);
    }

    fn clear_target(&mut self, automation: &mut dyn Automation) {
        self.health.clear(self.now);
        self.disarm_physical_result_text();
        // Dropping the target drops its swing (the cancel ends it).
        if self.pending_physical_target != 0 {
            self.pending_physical_target = 0;
            automation.abort_attack();
        }
        self.release_swing_executor_for_retarget();
        self.physical_swing_sent_at = f64::NEG_INFINITY;
        self.physical_swing_armed_at = f64::NEG_INFINITY;
        self.physical_swing_closest_distance = f32::INFINITY;
        self.stop_approach_movement(automation);
        self.stop_breakable_turn_movement(automation);
        self.stop_selection_jiggle();
        self.target_id = 0;
        self.target_rule = None;
        self.target_name.clear();
        self.target_distance = 0.0;
        "Target  \u{2014}".clone_into(&mut self.target_text);
    }

    /// Stops the macro (the gate's no-wand stop lands here too).
    pub fn disable(&mut self, automation: &mut dyn Automation, status: &str) {
        automation.abort_attack();
        self.reset_swing_executor();
        self.stop_approach_movement(automation);
        self.stop_breakable_turn_movement(automation);
        self.enabled = false;
        self.paused = false;
        self.combat_policy_suspended = false;
        self.targets.clear();
        self.combat_spell_snapshot = None;
        self.attack_spell_snapshot = None;
        self.attack_catalog = AttackSpellCatalog::build(&[]);
        self.debuffs.reset();
        self.failures.reset();
        self.health.reset();
        self.observed_physical_completion = 0;
        self.observed_attack_cast_completion = 0;
        self.pending_physical_target = 0;
        self.pending_attack_spell = 0;
        self.pending_attack_target = 0;
        self.clear_pending_item_debuff();
        self.observed_chat_sequence = 0;
        self.item_transaction_chat_sequence = 0;
        self.observed_item_completion = 0;
        // A stopped macro must not leave the busy latch up.
        self.cast_tracker.borrow_mut().reset();
        self.planned_weapon = 0;
        self.planned_offhand = None;
        self.gate.borrow_mut().reset();
        self.random_damage_index = 0;
        self.observed_jiggle_cast_completion = 0;
        self.clear_target(automation);
        self.set_status(status);
        if self.combat_control {
            automation.combat_control(false);
        }
        self.combat_control = false;
    }

    /// Moves the controller's clock on from a frame driver.
    fn advance_clock_from_frame(&mut self, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        self.now += elapsed;
        self.frame_advanced_since_tick += elapsed;
    }
}

/// Nothing left to kill: the host said it died, or its health reads zero.
fn is_known_dead(target: &CombatTarget) -> bool {
    target.object_id != 0
        && (target.is_dead || (target.is_health_known && target.health_fraction <= 0.0))
}

/// Whether the character is trained in a skill.
fn is_trained(automation: &mut dyn Automation, skill_id: u32) -> bool {
    automation.try_get_skill(skill_id).is_some_and(|skill| {
        matches!(
            skill.training,
            ac_plugin_abi::character::Training::Trained
                | ac_plugin_abi::character::Training::Specialized
        )
    })
}

/// The reference client's element word.
fn element_name(element: MonsterDamageType) -> String {
    match element {
        MonsterDamageType::Electric => "Lightning".to_owned(),
        MonsterDamageType::VoidBasic | MonsterDamageType::Nether => "Void".to_owned(),
        other => format!("{other:?}"),
    }
}

/// Reborrows the crafting hook for a shorter call.
fn reborrow_crafting<'b>(
    crafting: &'b mut Option<&mut dyn AmmunitionCrafting>,
) -> Option<&'b mut dyn AmmunitionCrafting> {
    match crafting {
        Some(crafting) => Some(&mut **crafting),
        None => None,
    }
}
