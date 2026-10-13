//! Choosing the monster (the six rejection gates, then the selector), the
//! walk to a monster out of reach, and the breakable turn to face one.

use ac_plugin_abi::combat::{CombatSnapshot, CombatTarget};
use ac_plugin_abi::items::{EquipmentItem, InventoryItem};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::navigation::NavigationStatus;
use mosstank_automation::Automation;
use openac_plugin_compat::number::{format_f32, format_f64};

use super::equip::wielded_pair;
use super::{BREAKABLE_TURN_TOLERANCE_DEGREES, CombatController, Ctx, is_known_dead};
use crate::MacroLogChannel;
use crate::debuff;
use crate::failure::CombatSuppressionReason;
use crate::mover::{
    CREEP_DISTANCE_METERS, CreepSettings, FACE_HEADING_REISSUE_SECONDS, NO_FACE_HEADING_STAMP,
    desired_heading, signed_heading_delta,
};
use crate::selection::{self, CombatTargetCandidate};

/// `+0.0;-0.0` as .NET formats a signed delta.
fn signed_one_decimal(value: f32) -> String {
    let text = format_f32(value.abs(), "0.0").unwrap_or_default();
    if value < 0.0 {
        format!("-{text}")
    } else {
        format!("+{text}")
    }
}

impl CombatController {
    pub(super) fn refresh_target(&mut self, cx: &mut Ctx<'_>) {
        if self.target_id != 0
            && self.failures.reason(self.target_id, self.now) != CombatSuppressionReason::None
        {
            cx.automation.abort_attack();
            self.clear_target(cx.automation);
        }

        let combat = cx.automation.combat_snapshot();
        // Selection is not frozen for an engagement: a higher-priority
        // monster arriving mid-fight can win.
        self.refresh_spell_catalogs(cx.automation);
        let inventory = self.pass_inventory(cx.automation);
        let equipment = self.pass_equipment(cx.automation);
        let (wielded_weapon, wielded_offhand) = wielded_pair(&equipment);

        let last_target = self.last_target_id;
        let mut candidates = Vec::new();
        self.ring_candidate_count = 0;
        let range = self.acquisition_range;
        for target in self.targets.clone() {
            let Some(candidate) = self.try_build_candidate(
                cx,
                &target,
                &combat,
                last_target,
                &inventory,
                &equipment,
                range,
            ) else {
                continue;
            };
            if candidate.distance < cx.settings.ring_distance
                && candidate.rule.actions().uses_ring()
            {
                self.ring_candidate_count += 1;
            }
            candidates.push(candidate);
        }

        let chosen = selection::select(
            &candidates,
            cx.settings.debuff_each_first,
            cx.settings.selection_method,
            cx.settings.target_select_angle_range,
            wielded_weapon,
            wielded_offhand,
        );

        self.trace_target_selection(cx, &candidates, chosen.object_id());
        if chosen.object_id() == 0 {
            if self.target_id != 0 {
                cx.automation.abort_attack();
            }
            self.clear_target(cx.automation);
            return;
        }
        // The stored rule is the authored one.
        let rule = cx.settings.resolve_rule(&chosen.target);
        self.set_target(&chosen.target, rule, &cx.settings.monster_facts);
    }

    fn trace_target_selection(
        &mut self,
        cx: &mut Ctx<'_>,
        candidates: &[CombatTargetCandidate],
        chosen: u32,
    ) {
        if chosen != 0 || self.now < self.next_target_diagnostic {
            return;
        }
        self.next_target_diagnostic = self.now + 5.0;
        let visible = cx.automation.hostile_targets(f32::MAX);
        if !self.has_log() {
            return;
        }
        let selected = cx.automation.selected_object().unwrap_or(0);
        self.log_line(
            MacroLogChannel::RuleInfo,
            &format!(
                "Target scan: no selection, hostiles={}, candidates={}, range={}..{}, selected=0x{selected:08X}",
                visible.len(),
                candidates.len(),
                format_f64(cx.settings.minimum_range, "F2").unwrap_or_default(),
                format_f64(self.acquisition_range, "F2").unwrap_or_default(),
            ),
        );
        for target in &visible {
            let rule = cx.settings.resolve_rule(target);
            let suppression = self.failures.reason(target.object_id, self.now);
            let reason = if f64::from(target.distance) > self.acquisition_range {
                "outside maximum range".to_owned()
            } else if f64::from(target.distance) < cx.settings.minimum_range {
                "inside minimum range".to_owned()
            } else if suppression != CombatSuppressionReason::None {
                format!("{suppression:?}")
            } else if let Some(noted) = self.pass_target_reasons.get(&target.object_id) {
                noted.clone()
            } else if self.pass_invalid_targets.contains(&target.object_id) {
                "invalidated during attack pass".to_owned()
            } else if rule.priority() < 0 {
                "negative rule priority".to_owned()
            } else {
                match self.pass_candidates.get(&target.object_id) {
                    Some(None) => "no attack or due debuff after pass filtering".to_owned(),
                    Some(Some(_)) => "eligible candidate".to_owned(),
                    None => "absent from current acquisition snapshot".to_owned(),
                }
            };
            self.log_line(
                MacroLogChannel::RuleInfo,
                &format!(
                    "Target check: {} (0x{:08X}), distance={}, angle={}, reason={reason}, rule={}, priority={}, attacks={}, streak={}",
                    target.name,
                    target.object_id,
                    format_f32(target.distance, "F2").unwrap_or_default(),
                    format_f32(target.relative_angle_degrees, "F1").unwrap_or_default(),
                    rule.rule.expression(),
                    rule.priority(),
                    if rule.actions().attacks() { "True" } else { "False" },
                    if rule.actions().uses_streak() { "True" } else { "False" },
                ),
            );
        }
    }

    /// The six ordered rejection gates, then the fill.
    #[allow(clippy::too_many_arguments)]
    fn try_build_candidate(
        &mut self,
        cx: &mut Ctx<'_>,
        target: &CombatTarget,
        combat: &CombatSnapshot,
        last_target: u32,
        inventory: &[InventoryItem],
        equipment: &[EquipmentItem],
        maximum_range: f64,
    ) -> Option<CombatTargetCandidate> {
        #[allow(clippy::float_cmp)]
        if self.pass_candidate_range != maximum_range {
            self.pass_candidate_range = maximum_range;
            self.pass_candidates.clear();
        }
        if let Some(memo) = self.pass_candidates.get(&target.object_id) {
            return memo.clone();
        }
        if self.pass_invalid_targets.contains(&target.object_id) {
            return None;
        }
        // A corpse is not a candidate.
        if is_known_dead(target) {
            self.pass_candidates.insert(target.object_id, None);
            return None;
        }
        if self.failures.reason(target.object_id, self.now) != CombatSuppressionReason::None {
            self.pass_candidates.insert(target.object_id, None);
            return None;
        }
        // The rule must want it attacked at all.
        let authored = cx.settings.resolve_rule(target);
        let rule = self.with_pass_cleared_actions(target.object_id, &authored);
        if rule.priority() < 0 {
            self.pass_candidates.insert(target.object_id, None);
            return None;
        }
        let distance = f64::from(target.distance);
        if distance > maximum_range || distance < cx.settings.minimum_range {
            // Range depends on the range asked about: not memoised.
            return None;
        }

        let actions = rule.actions().clone();
        let (weapon, offhand, element) =
            self.resolve_wield_plan(cx, &actions, target, inventory, equipment);
        let extra = self.resolve_extra_vulnerability(&actions, target);
        let steps = debuff::build_chain(&actions, element, extra);
        let mut needs_debuff = false;
        for step in &steps {
            if self.is_debuff_step_due(cx, step, target, inventory) {
                needs_debuff = true;
                break;
            }
        }
        if !needs_debuff && !actions.attacks() && !actions.uses_streak() {
            self.pass_candidates.insert(target.object_id, None);
            return None;
        }
        let urgency = self.debuff_urgency(cx.automation, target, element, weapon, equipment);
        let candidate = CombatTargetCandidate {
            target: target.clone(),
            priority: rule.priority(),
            rule,
            distance,
            angle_delta: f64::from(target.relative_angle_degrees.abs()),
            urgency,
            needs_debuff,
            is_lock_selection: cx.settings.target_lock
                && combat.selected_id != 0
                && combat.selected_id == target.object_id,
            is_last_target: last_target != 0 && target.object_id == last_target,
            planned_weapon: weapon,
            planned_offhand: offhand,
        };
        self.pass_candidates
            .insert(target.object_id, Some(candidate.clone()));
        Some(candidate)
    }

    // --- The walk to a monster.

    /// Whether the monster worth walking to is beyond the creep distance
    /// (or there is none): the approach rule's idle-peace fallback gate.
    pub fn is_approach_outside_creep_distance(&mut self, cx: &mut Ctx<'_>) -> bool {
        if !self.enabled || !cx.settings.enabled || !cx.automation.is_available() {
            return true;
        }
        self.clear_pass_memos();
        self.select_approach_target(cx)
            .is_none_or(|approach| approach.distance >= CREEP_DISTANCE_METERS)
    }

    /// The walk's turn on the rule pass: it answers and arms the mover.
    pub fn claim_monster_approach_from_rule_pass(
        &mut self,
        cx: &mut Ctx<'_>,
        can_act: bool,
    ) -> bool {
        let _ = self.approach_mover.take_pending_seconds();
        let claimed = self.tick_monster_approach(cx, can_act);
        self.approach_mover.arm(claimed);
        claimed
    }

    /// One host frame of the armed mover.
    pub fn step_armed_approach_mover(&mut self, cx: &mut Ctx<'_>, elapsed_seconds: f64) {
        if self
            .approach_mover
            .try_take_mover_frame(elapsed_seconds)
            .is_some()
        {
            let _ = self.tick_monster_approach(cx, true);
        }
    }

    /// Walking to a monster the character cannot hit yet; true while there
    /// is one worth walking to.
    pub fn tick_monster_approach(&mut self, cx: &mut Ctx<'_>, can_act: bool) -> bool {
        if !self.enabled
            || !cx.settings.enabled
            || !cx.automation.is_available()
            || !can_act
            || cx.settings.approach_distance <= cx.settings.maximum_range
        {
            self.stop_approach_movement(cx.automation);
            return false;
        }
        // Its own pass: everything learned per pass is taken fresh.
        self.clear_pass_memos();
        let Some(approach) = self.select_approach_target(cx) else {
            self.stop_approach_movement(cx.automation);
            return false;
        };
        // The walk ends where the attack begins.
        if approach.distance <= cx.settings.maximum_range {
            self.stop_approach_movement(cx.automation);
            return false;
        }
        self.tick_approach_to(
            cx,
            approach.object_id(),
            &approach.target.name,
            approach.distance,
        )
    }

    fn select_approach_target(&mut self, cx: &mut Ctx<'_>) -> Option<CombatTargetCandidate> {
        #[allow(clippy::cast_possible_truncation)]
        let reachable = cx
            .automation
            .hostile_targets(cx.settings.approach_distance as f32);
        if reachable.is_empty() {
            return None;
        }
        let combat = cx.automation.combat_snapshot();
        self.refresh_spell_catalogs(cx.automation);
        let inventory = self.pass_inventory(cx.automation);
        let equipment = self.pass_equipment(cx.automation);
        let (wielded_weapon, wielded_offhand) = wielded_pair(&equipment);
        let mut candidates = Vec::new();
        let range = cx.settings.approach_distance;
        for target in &reachable {
            if let Some(candidate) = self.try_build_candidate(
                cx,
                target,
                &combat,
                self.last_target_id,
                &inventory,
                &equipment,
                range,
            ) {
                candidates.push(candidate);
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let chosen = selection::select(
            &candidates,
            cx.settings.debuff_each_first,
            cx.settings.selection_method,
            cx.settings.target_select_angle_range,
            wielded_weapon,
            wielded_offhand,
        );
        (chosen.object_id() != 0).then_some(chosen)
    }

    fn tick_approach_to(
        &mut self,
        cx: &mut Ctx<'_>,
        object_id: u32,
        name: &str,
        distance: f64,
    ) -> bool {
        let me = cx.automation.navigation_snapshot();
        let target = if me.is_available && !me.is_portal_space {
            cx.automation.navigation_object(object_id)
        } else {
            None
        };
        let Some(target) = target else {
            self.stop_approach_movement(cx.automation);
            return false;
        };
        let label = if name.trim().is_empty() {
            format!("0x{object_id:08X}")
        } else {
            name.to_owned()
        };
        self.set_status(format!(
            "Approaching {label} ({}m)",
            format_f64(distance, "0.0").unwrap_or_default()
        ));
        let creep = CreepSettings {
            combat: cx.settings,
            vitals: cx.vitals,
        };
        let claimed = self.approach_mover.steer(
            cx.automation,
            Some(&creep),
            &me.position,
            &target.position,
            distance,
        );
        self.drain_gate_stops(cx.automation);
        claimed
    }

    /// The approach rule lost its turn.
    pub fn stop_monster_approach_for_lost_turn(&mut self, automation: &mut dyn Automation) {
        self.approach_mover.stop_for_lost_turn(automation);
    }

    /// Drops what the walk holds without disarming it.
    pub(super) fn stop_approach_movement(&mut self, automation: &mut dyn Automation) {
        self.approach_mover.stop_movement(automation);
    }

    /// Whether the low-stop-distance warning is due (once per run, the
    /// owner's bookkeeping).
    pub fn take_low_stop_distance_warning(&mut self) -> bool {
        self.approach_mover.take_low_stop_distance_warning()
    }

    // --- The breakable turn.

    pub(super) fn ready_for_breakable_turn(
        &mut self,
        cx: &mut Ctx<'_>,
        spell: &SpellInfo,
        target_object_id: u32,
    ) -> bool {
        let character = cx.automation.character().id;
        if !cx.settings.use_breakable_turn_to
            || !spell.requires_turn_to
            || target_object_id == 0
            || target_object_id == character
        {
            self.stop_breakable_turn_movement(cx.automation);
            return true;
        }
        if !self.drive_breakable_turn(cx.automation, target_object_id) {
            self.stop_breakable_turn_movement(cx.automation);
            return true;
        }
        self.hold_pass_for_turn(target_object_id);
        false
    }

    /// The fists arm's unconditional turn, a shade off the bearing.
    pub(super) fn face_for_fists(
        &mut self,
        automation: &mut dyn Automation,
        target_object_id: u32,
    ) -> bool {
        const LEAD_DEGREES: f32 = 180.0 / 50.0;
        let me = automation.navigation_snapshot();
        if !me.is_available || me.is_portal_space {
            return true;
        }
        let Some(target) = automation.navigation_object(target_object_id) else {
            return true;
        };
        let desired = desired_heading(&me.position, &target.position) - LEAD_DEGREES;
        let delta = signed_heading_delta(me.position.heading_degrees, desired);
        if delta.abs() <= BREAKABLE_TURN_TOLERANCE_DEGREES {
            return true;
        }
        if automation.clear_movement_intent() != NavigationStatus::Accepted {
            return true;
        }
        if self.now - self.breakable_turn_face_heading_stamp >= FACE_HEADING_REISSUE_SECONDS {
            self.breakable_turn_face_heading_stamp = self.now;
            if automation.face_heading(desired) != NavigationStatus::Accepted {
                return true;
            }
        }
        self.breakable_turn_owned = true;
        self.set_status(format!(
            "Turning to {} ({}\u{b0})",
            self.target_name,
            signed_one_decimal(delta)
        ));
        false
    }

    /// One step of a turn in flight; true while there is turning left.
    fn drive_breakable_turn(
        &mut self,
        automation: &mut dyn Automation,
        target_object_id: u32,
    ) -> bool {
        let me = automation.navigation_snapshot();
        if !me.is_available || me.is_portal_space {
            return false;
        }
        let Some(target) = automation.navigation_object(target_object_id) else {
            return false;
        };
        let desired = desired_heading(&me.position, &target.position);
        let delta = signed_heading_delta(me.position.heading_degrees, desired);
        if delta.abs() <= BREAKABLE_TURN_TOLERANCE_DEGREES {
            return false;
        }
        if automation.clear_movement_intent() != NavigationStatus::Accepted {
            return false;
        }
        if self.now - self.breakable_turn_face_heading_stamp >= FACE_HEADING_REISSUE_SECONDS {
            self.breakable_turn_face_heading_stamp = self.now;
            if automation.face_heading(desired) != NavigationStatus::Accepted {
                return false;
            }
        }
        self.breakable_turn_owned = true;
        self.set_status(format!(
            "Turning to {} ({}\u{b0})",
            self.target_name,
            signed_one_decimal(delta)
        ));
        true
    }

    /// Turning owns the pass: raised once for the life of one turn.
    fn hold_pass_for_turn(&mut self, target_object_id: u32) {
        self.breakable_turn_target_id = target_object_id;
        if self.turn_holds_pass {
            return;
        }
        self.turn_holds_pass = true;
        (self.suspend_pass)();
    }

    /// Steps a turn that holds the pass, on the host frame.
    pub fn advance_held_turn(&mut self, automation: &mut dyn Automation, elapsed_seconds: f64) {
        if !self.turn_holds_pass {
            return;
        }
        self.advance_clock_from_frame(elapsed_seconds);
        if self.breakable_turn_target_id == 0
            || !self.drive_breakable_turn(automation, self.breakable_turn_target_id)
        {
            self.stop_breakable_turn_movement(automation);
        }
    }

    pub(super) fn stop_breakable_turn_movement(&mut self, automation: &mut dyn Automation) {
        self.breakable_turn_face_heading_stamp = NO_FACE_HEADING_STAMP;
        self.breakable_turn_target_id = 0;
        if self.turn_holds_pass {
            self.turn_holds_pass = false;
            (self.resume_pass)();
        }
        if !self.breakable_turn_owned {
            return;
        }
        let _ = automation.clear_movement_intent();
        self.breakable_turn_owned = false;
    }
}
