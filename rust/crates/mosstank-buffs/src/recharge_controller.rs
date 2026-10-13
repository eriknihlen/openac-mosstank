//! The self-recharge and helper rules (`VitalRechargeController`): one use
//! or cast at a time, started on a turn of the macro and watched to the
//! server's answer on every frame. An item use holds the shared item slot
//! for its window; a cast holds the pass while the shared cast tracker has
//! it in flight. The recharge boost reads a vital as worse off than it is
//! for a while after a recharge was asked for, so it is carried through.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::magic::CastGate;
use ac_plugin_abi::social::FellowshipStatus;
use mosstank_automation::cast_tracker::{CastRequest, SpellCastTracker};
use mosstank_automation::locks::{
    ActionLockKind, ActionLockTable, SharedActionLocks, item_use_lock,
};
use mosstank_automation::{Automation, CombatModePreparation};
use mosstank_settings::model::{CombatSettings, VitalKind, VitalSettings};
use openac_plugin_compat::text::ordinal;

use crate::recharge::{self, VitalRechargeChoice, VitalRechargeSourceKind, vital_name};
use crate::vitals::{self, Vitals};

/// The status when nothing is being done.
pub const IDLE_STATUS: &str = "Vitals idle";

/// How long an unanswered use is waited out before it is given up, seconds.
const PENDING_TIMEOUT_SECONDS: f64 = 15.0;

/// The cast tracker as the rules share it.
pub type SharedCastTracker = Rc<RefCell<SpellCastTracker>>;

/// What one call works with: the client and the live settings.
#[allow(missing_debug_implementations)]
pub struct VitalTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The vital settings.
    pub settings: &'a VitalSettings,
    /// The combat settings (consumables, casters, the blacklist).
    pub combat: &'a CombatSettings,
}

#[derive(Debug, Clone)]
struct Pending {
    choice: VitalRechargeChoice,
    /// The newest receipt stamp this wait has already looked at.
    revision: i64,
}

/// One server-receipt-driven recharge state machine.
#[allow(missing_debug_implementations)]
pub struct VitalRechargeController {
    cast_tracker: Option<SharedCastTracker>,
    combat_mode_gate: Option<Rc<RefCell<dyn CombatModePreparation>>>,
    pending: Option<Pending>,
    retry_delay: f64,
    recharge_trace_delay: f64,
    pending_seconds: f64,
    health_boost_remaining: f64,
    stamina_boost_remaining: f64,
    mana_boost_remaining: f64,
    vitals_requested: bool,
    helper_trace_lines: HashSet<String>,
    status: String,
    action_locks: SharedActionLocks,
    pending_hold_until: f64,
    frame_observed_seconds: f64,
}

impl Default for VitalRechargeController {
    fn default() -> Self {
        Self::new()
    }
}

impl VitalRechargeController {
    /// An idle controller with a cooldown table of its own.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cast_tracker: None,
            combat_mode_gate: None,
            pending: None,
            retry_delay: 0.0,
            recharge_trace_delay: 0.0,
            pending_seconds: 0.0,
            health_boost_remaining: 0.0,
            stamina_boost_remaining: 0.0,
            mana_boost_remaining: 0.0,
            vitals_requested: false,
            helper_trace_lines: HashSet::new(),
            status: IDLE_STATUS.to_owned(),
            action_locks: ActionLockTable::shared(),
            pending_hold_until: 0.0,
            frame_observed_seconds: 0.0,
        }
    }

    /// Shares the cast tracker: a learned-spell recharge waits on it and
    /// never replaces another owner's cast.
    pub fn bind_cast_tracker(&mut self, tracker: SharedCastTracker) {
        self.cast_tracker = Some(tracker);
    }

    /// Shares the stance gate a magic recharge readies its stance through.
    pub fn bind_combat_mode_gate(&mut self, gate: Rc<RefCell<dyn CombatModePreparation>>) {
        self.combat_mode_gate = Some(gate);
    }

    /// Shares the macro's cooldown table: a kit, a stone or a bite of food
    /// holds the item slot for as long as the macro waits on it.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = locks;
    }

    /// What the controller is doing, for the panel.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether a kit, food or caster item this controller used is still
    /// unanswered.
    #[must_use]
    pub fn item_use_in_flight(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| p.choice.uses_item())
    }

    /// Whether the unanswered recharge is one the reference stops its whole
    /// pass for: a kit, a caster item or a learned spell, not food.
    #[must_use]
    pub fn holds_pass(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.choice.source_kind != VitalRechargeSourceKind::Food)
    }

    /// Whether a learned-spell recharge is still unanswered.
    #[must_use]
    pub fn cast_in_flight(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| !p.choice.uses_item())
    }

    fn release_item_use(&mut self) {
        if let Some(pending) = &self.pending
            && pending.choice.source_kind != VitalRechargeSourceKind::LearnedSpell
        {
            // Only while the window this owner armed is still running: past
            // it the slot may belong to somebody else.
            let mut locks = self.action_locks.borrow_mut();
            if locks.now() < self.pending_hold_until {
                locks.release(ActionLockKind::ItemUse);
            }
        }
        self.pending_hold_until = 0.0;
    }

    /// Whether the outstanding use still holds the pass: a cast for as long
    /// as it is in flight, an item use for the window it armed.
    fn pending_still_holds_the_pass(&self) -> bool {
        self.pending.as_ref().is_some_and(|waiting| {
            waiting.choice.source_kind == VitalRechargeSourceKind::LearnedSpell
                || self.action_locks.borrow().now() < self.pending_hold_until
        })
    }

    /// One turn of the rule: true when it claims the pass. `enabled` is
    /// whether the rule has the turn (it still watches what it started);
    /// `helpers` picks the helper rows over the self rows.
    pub fn tick(
        &mut self,
        turn: &mut VitalTurn<'_>,
        elapsed_seconds: f64,
        enabled: bool,
        no_target: bool,
        helpers: bool,
    ) -> bool {
        let elapsed = elapsed_seconds.max(0.0);
        // What the frame already watched off this transaction is not
        // counted a second time: one wall clock between the two.
        let pending_elapsed = (elapsed - self.frame_observed_seconds).max(0.0);
        self.frame_observed_seconds = 0.0;
        self.retry_delay = (self.retry_delay - elapsed).max(0.0);
        self.recharge_trace_delay = (self.recharge_trace_delay - elapsed).max(0.0);
        self.health_boost_remaining = (self.health_boost_remaining - elapsed).max(0.0);
        self.stamina_boost_remaining = (self.stamina_boost_remaining - elapsed).max(0.0);
        self.mana_boost_remaining = (self.mana_boost_remaining - elapsed).max(0.0);
        if !turn.settings.enabled || !turn.automation.is_available() {
            self.release_item_use();
            self.pending = None;
            self.frame_observed_seconds = 0.0;
            self.clear_boosts();
            IDLE_STATUS.clone_into(&mut self.status);
            self.sync_vitals_request(turn.automation, false);
            return false;
        }
        self.observe_pending(turn, pending_elapsed);
        if let Some(pending) = &self.pending {
            self.status = format!(
                "Recharging {}: {}",
                vital_name(pending.choice.vital),
                pending.choice.name
            );
            return self.pending_still_holds_the_pass();
        }

        // No turn this pass, so nothing new is begun.
        if !enabled {
            self.clear_boosts();
            IDLE_STATUS.clone_into(&mut self.status);
            self.sync_vitals_request(turn.automation, false);
            return false;
        }
        if helpers {
            let wanted = turn.settings.help_others && turn.automation.in_fellowship();
            self.sync_vitals_request(turn.automation, wanted);
        }

        let boost = turn.settings.recharge_boost_amount;
        let adjustment = |remaining: f64| if remaining > 0.0 { boost } else { 0 };
        let need = vitals::decide_need(
            &Vitals::from(&turn.automation.character()),
            turn.settings,
            no_target,
            (
                adjustment(self.health_boost_remaining),
                adjustment(self.stamina_boost_remaining),
                adjustment(self.mana_boost_remaining),
            ),
        );
        if !helpers && need.is_none() {
            "Vitals ready".clone_into(&mut self.status);
            return false;
        }
        let mut helper = None;
        if need.is_none() {
            // The reference gates both helper rows on the item-use slot,
            // the slot a kit or a heart holds while its animation runs.
            if !self
                .action_locks
                .borrow_mut()
                .is_locked(ActionLockKind::ItemUse)
            {
                helper = self.plan_helper(turn);
            }
            if helper.is_none() {
                "Vitals ready".clone_into(&mut self.status);
                return false;
            }
        }
        if helpers && need.is_some() {
            // The helper rows' turn; a self need belongs to the self row and
            // was already offered there this pass.
            "Vitals ready".clone_into(&mut self.status);
            return false;
        }
        // The rule is valid whenever a vital is below its threshold,
        // whatever it then finds to use: a vital it has no answer for holds
        // the pass. The retry delay only paces how often a plan is looked
        // for again.
        if self.retry_delay > 0.0 {
            "Waiting to retry".clone_into(&mut self.status);
            return true;
        }

        let choice = match (need, helper) {
            (None, Some(helper)) => helper,
            (Some(need), _) => match self.plan_self(turn, need) {
                Some(choice) => choice,
                None => {
                    self.status = format!("No {} recharge available", vital_name(need));
                    self.retry_delay = 1.0;
                    return true;
                }
            },
            (None, None) => unreachable!("a missing helper returned above"),
        };

        if choice.required_mode == Some(CombatMode::Magic)
            && let Some(gate) = self.combat_mode_gate.clone()
        {
            let item_spell = choice.source_kind == VitalRechargeSourceKind::CasterItem;
            let mut gate = gate.borrow_mut();
            if !gate.try_prepare(
                turn.automation,
                CombatMode::Magic,
                if item_spell { choice.item_object_id } else { 0 },
                !item_spell,
            ) {
                self.arm_boost(turn.settings, choice.vital);
                self.status = gate.status();
                return true;
            }
        }

        if let Some(required) = choice.required_mode
            && turn.automation.combat_mode() != required
        {
            if required == CombatMode::Magic {
                self.arm_boost(turn.settings, choice.vital);
            }
            let mode = turn.automation.enter_combat_mode(required);
            self.status = if mode.accepted() {
                format!("Switching to {required:?} for {}", choice.name)
            } else {
                format!("Waiting for {required:?}: {}", choice.name)
            };
            return true;
        }

        let revision = if choice.source_kind == VitalRechargeSourceKind::LearnedSpell {
            turn.automation.last_cast_completion().revision
        } else {
            turn.automation.last_item_completion().revision
        };
        if !self.start(turn.automation, &choice) {
            self.status = format!("Waiting to use {}", choice.name);
            self.retry_delay = 0.25;
            return true;
        }
        if choice.uses_item() {
            // An item is in the character's hands until the server answers
            // for it; the attack and every other item rule wait that out.
            let mut locks = self.action_locks.borrow_mut();
            locks.arm(ActionLockKind::ItemUse, item_use_lock::TRANSACTION_SECONDS);
            self.pending_hold_until = locks.now() + item_use_lock::TRANSACTION_SECONDS;
        }
        self.pending_seconds = 0.0;
        self.status = format!("Recharging {}: {}", vital_name(choice.vital), choice.name);
        let line = if choice.target_object_id == 0 {
            format!(
                "Vitals: {} \u{2192} {} on self",
                vital_name(choice.vital),
                choice.name
            )
        } else {
            format!(
                "Vitals: {} \u{2192} {} at fellow 0x{:08X}",
                vital_name(choice.vital),
                choice.name,
                choice.target_object_id
            )
        };
        self.pending = Some(Pending { choice, revision });
        turn.automation.log_info(&line);
        true
    }

    /// The self plan, its trace (paced to one every five seconds) logged.
    fn plan_self(
        &mut self,
        turn: &mut VitalTurn<'_>,
        need: VitalKind,
    ) -> Option<VitalRechargeChoice> {
        let mut traced: Vec<String> = Vec::new();
        let mut sink = |line: &str| traced.push(line.to_owned());
        let trace: Option<&mut dyn FnMut(&str)> = if self.recharge_trace_delay <= 0.0 {
            Some(&mut sink)
        } else {
            None
        };
        let choice = recharge::try_plan(need, turn.automation, turn.settings, turn.combat, trace);
        for line in traced {
            self.recharge_trace_delay = 5.0;
            turn.automation.log_info(&line);
        }
        choice
    }

    /// The helper plan; each distinct line of its walk reaches the log once
    /// per session.
    fn plan_helper(&mut self, turn: &mut VitalTurn<'_>) -> Option<VitalRechargeChoice> {
        let mut traced: Vec<String> = Vec::new();
        let mut sink = |line: &str| traced.push(line.to_owned());
        let choice =
            recharge::try_plan_helper(turn.automation, turn.settings, turn.combat, Some(&mut sink));
        for line in traced {
            if self.helper_trace_lines.insert(line.clone()) {
                turn.automation.log_info(&line);
            }
        }
        choice
    }

    /// Drops everything: the use in flight (and its slot), the delays, the
    /// boosts, the memory of the vitals subscription.
    pub fn reset(&mut self) {
        self.release_item_use();
        self.pending = None;
        self.pending_seconds = 0.0;
        self.frame_observed_seconds = 0.0;
        self.retry_delay = 0.0;
        self.recharge_trace_delay = 0.0;
        self.clear_boosts();
        // The host drops its subscription with the session; only the
        // plugin-side memory of it is stale here.
        self.vitals_requested = false;
        self.helper_trace_lines.clear();
        IDLE_STATUS.clone_into(&mut self.status);
    }

    fn sync_vitals_request(&mut self, automation: &mut dyn Automation, wanted: bool) {
        if wanted == self.vitals_requested {
            return;
        }
        if !automation.is_available() {
            self.vitals_requested = false;
            return;
        }
        if automation.request_fellow_vitals(wanted) == FellowshipStatus::Accepted {
            self.vitals_requested = wanted;
        }
    }

    fn arm_boost(&mut self, settings: &VitalSettings, vital: VitalKind) {
        let duration = settings.recharge_boost_time_seconds.max(0.0);
        match vital {
            VitalKind::Health => self.health_boost_remaining = duration,
            VitalKind::Stamina => self.stamina_boost_remaining = duration,
            VitalKind::Mana => self.mana_boost_remaining = duration,
        }
    }

    fn clear_boost(&mut self, vital: VitalKind) {
        match vital {
            VitalKind::Health => self.health_boost_remaining = 0.0,
            VitalKind::Stamina => self.stamina_boost_remaining = 0.0,
            VitalKind::Mana => self.mana_boost_remaining = 0.0,
        }
    }

    fn clear_boosts(&mut self) {
        self.health_boost_remaining = 0.0;
        self.stamina_boost_remaining = 0.0;
        self.mana_boost_remaining = 0.0;
    }

    fn is_level_boost_spell(choice: &VitalRechargeChoice) -> bool {
        let name = match choice.vital {
            VitalKind::Health => "Adja's Intervention",
            VitalKind::Stamina => "Robustification",
            VitalKind::Mana => "Meditative Trance",
        };
        ordinal::equals_ignore_case(&choice.name, name)
    }

    fn start(&mut self, automation: &mut dyn Automation, choice: &VitalRechargeChoice) -> bool {
        if choice.source_kind == VitalRechargeSourceKind::LearnedSpell {
            if let Some(tracker) = &self.cast_tracker {
                let tracker = tracker.borrow();
                if tracker.is_busy() {
                    return false;
                }
                let Some(candidate) = automation.spells().try_get(choice.spell_id) else {
                    return false;
                };
                if tracker.is_school_locked_out(candidate.school) {
                    return false;
                }
            }
            let target = (choice.target_object_id != 0).then_some(choice.target_object_id);
            let started = automation.evaluate_gate(choice.spell_id, target) == CastGate::Ready
                && automation.cast(choice.spell_id, target);
            if started
                && let Some(tracker) = &self.cast_tracker
                && let Some(spell) = automation.spells().try_get(choice.spell_id)
            {
                let character = automation.character();
                let target_name =
                    if choice.target_object_id == 0 || choice.target_object_id == character.id {
                        "yourself".to_owned()
                    } else {
                        automation
                            .object(choice.target_object_id)
                            .map(|target| target.name)
                            .unwrap_or_default()
                    };
                let current_mana = i32::try_from(character.mana.0).unwrap_or(i32::MAX);
                tracker.borrow_mut().begin(&CastRequest {
                    saying: &spell.saying,
                    school: spell.school,
                    can_kill: SpellCastTracker::can_kill_for(&spell),
                    current_mana,
                    ..CastRequest::new(
                        spell.spell_id,
                        &spell.name,
                        choice.target_object_id,
                        &target_name,
                        SpellCastTracker::hits_multiple_targets_for(&spell),
                    )
                });
            }
            return started;
        }

        let result = if choice.source_kind == VitalRechargeSourceKind::Food {
            automation.use_item(choice.item_object_id)
        } else {
            let target = if choice.target_object_id == 0 {
                automation.character().id
            } else {
                choice.target_object_id
            };
            automation.apply_item(choice.item_object_id, target)
        };
        result.accepted()
    }

    /// Reads the server's answer to what this controller started, on the
    /// host frame rather than the macro pass (an unanswered use holds the
    /// pass, so the pass cannot be what ends the wait). Nothing is started
    /// here.
    pub fn observe_pending_receipt(&mut self, turn: &mut VitalTurn<'_>, elapsed_seconds: f64) {
        if self.pending.is_none() || !turn.automation.is_available() {
            return;
        }
        let elapsed = elapsed_seconds.max(0.0);
        self.frame_observed_seconds += elapsed;
        self.observe_pending(turn, elapsed);
    }

    /// An item the server has yet to answer for is watched to its end
    /// whoever owns the pass meanwhile.
    fn observe_pending(&mut self, turn: &mut VitalTurn<'_>, elapsed_seconds: f64) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        self.pending_seconds += elapsed_seconds.max(0.0);
        let complete = self.try_complete(turn.automation, &mut pending);
        self.pending = Some(pending);
        if complete {
            let choice = &self.pending.as_ref().expect("just put back").choice;
            if turn.settings.clear_level_boost_flag_on_cast
                && choice.source_kind == VitalRechargeSourceKind::LearnedSpell
                && Self::is_level_boost_spell(choice)
            {
                let vital = choice.vital;
                self.clear_boost(vital);
            }
            self.release_item_use();
            self.pending = None;
            self.pending_seconds = 0.0;
            self.retry_delay = 0.25;
            return;
        }
        if self.pending_seconds < PENDING_TIMEOUT_SECONDS {
            return;
        }
        let name = self
            .pending
            .as_ref()
            .map(|p| p.choice.name.clone())
            .unwrap_or_default();
        self.status = format!("Timed out: {name}");
        self.release_item_use();
        self.pending = None;
        self.pending_seconds = 0.0;
        self.retry_delay = 1.0;
    }

    /// Whether the server has answered for this use or cast. A receipt that
    /// is not this one is stepped over (its stamp becomes the floor, so it
    /// is never read twice) and the wait goes on.
    fn try_complete(&self, automation: &mut dyn Automation, pending: &mut Pending) -> bool {
        if pending.choice.source_kind == VitalRechargeSourceKind::LearnedSpell {
            // The shared cast owner watches the spell's words and result on
            // every frame.
            if let Some(tracker) = &self.cast_tracker {
                return !tracker.borrow().is_busy();
            }
            let cast = automation.last_cast_completion();
            if cast.revision <= pending.revision {
                return false;
            }
            pending.revision = cast.revision;
            return cast.spell_id == pending.choice.spell_id;
        }

        let used = automation.last_item_completion();
        if used.revision <= pending.revision {
            return false;
        }
        pending.revision = used.revision;
        used.source_id == pending.choice.item_object_id
    }
}
