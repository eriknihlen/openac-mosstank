//! The physical attack: the pass points the swing executor at a monster,
//! and the executor (stepped by the host frame and the pass alike) asks for
//! a swing on its own period, lets a charged one go the moment the bar is
//! full, and gives up on one that never went out or was never answered.

use ac_plugin_abi::combat::{AttackHeight, CombatMode, CombatSnapshot, CombatStatus};
use ac_plugin_abi::world::ProjectilePathKind;
use mosstank_automation::Automation;
use mosstank_automation::locks::ActionLockKind;
use mosstank_settings::monster::MonsterActionFlags;
use openac_plugin_compat::number::{format_f32, format_f64};

use super::{
    AttackPassOutcome, CombatController, Ctx, POWER_RELEASE_EPSILON, SWING_CLOSING_PROGRESS_METERS,
    UNANSWERED_SWING_SECONDS,
};
use crate::MacroLogChannel;

/// The executor's repeat period, seconds.
const SWING_REPEAT_SECONDS: f64 = 0.263;
/// How long asking for a swing holds the shot slot, seconds.
const SWING_SHOT_FLOOR_SECONDS: f64 = 0.75;
/// How long calling a swing off holds the shot slot, seconds.
const SWING_STOP_FLOOR_SECONDS: f64 = 1.0;

/// What one executor step leaves for the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SwingExecutorOutcome {
    None,
    Retry,
}

fn translate(outcome: SwingExecutorOutcome) -> AttackPassOutcome {
    if outcome == SwingExecutorOutcome::Retry {
        AttackPassOutcome::Retry
    } else {
        AttackPassOutcome::Claimed
    }
}

/// .NET's enum name for a combat command answer.
pub(super) fn status_name(status: CombatStatus) -> String {
    format!("{status:?}")
}

fn notice_suffix(notice: Option<&String>) -> String {
    match notice {
        Some(text) if !text.trim().is_empty() => format!(" - {text}"),
        _ => String::new(),
    }
}

impl CombatController {
    pub(super) fn tick_physical(
        &mut self,
        cx: &mut Ctx<'_>,
        combat: &CombatSnapshot,
    ) -> AttackPassOutcome {
        // A repeat at something other than our monster is ended.
        if (combat.server_response_pending || combat.repeat_attack_in_progress)
            && self.pending_physical_target == 0
            && !combat.server_response_pending
            && combat.selected_id != self.target_id
        {
            cx.automation.abort_attack();
            self.set_status("Ending a repeat at another monster");
            return AttackPassOutcome::Claimed;
        }

        // A swing already in the air belongs to the executor.
        if combat.request_in_progress
            || combat.server_response_pending
            || combat.repeat_attack_in_progress
        {
            if !self.swing_timer_running {
                // Left over from a dropped monster: ended, not waited on.
                cx.automation.abort_attack();
                self.log_line(
                    MacroLogChannel::CastInfo,
                    "Swing: ended an attack left over from a dropped monster",
                );
                self.set_status(format!(
                    "Ending a leftover attack before {}",
                    self.target_name
                ));
                return AttackPassOutcome::Claimed;
            }
            let outcome = self.pump_swing_executor(cx.automation);
            return translate(outcome);
        }

        let inventory = self.pass_inventory(cx.automation);
        let physical_target = self.find_target(self.target_id);
        let decision = self.decision_actions();
        let physical_actions =
            self.resolve_physical_actions(decision, &physical_target, &inventory);
        if combat.mode == CombatMode::Missile {
            let height = cx.settings.attack_height;
            let (clear, path) = self.projectile_path_is_clear(
                cx,
                self.target_id,
                ProjectilePathKind::Missile,
                attack_height(height.0),
            );
            if !clear {
                self.set_status(projectile_status(&path, &self.target_name));
                // The shot cannot reach: choose again.
                self.clear_actions_for_pass(
                    self.target_id,
                    MonsterActionFlags::ATTACK | MonsterActionFlags::STREAK,
                );
                return AttackPassOutcome::Retry;
            }
        }
        // The power table reads the element the attack resolved to.
        let element = self.resolve_attack_element(cx, &physical_actions, &physical_target);
        let skills = cx.automation.skills();
        let desired_power =
            crate::power::resolve(&physical_actions, element, cx.settings, &skills, &inventory)
                .unwrap_or(combat.desired_power);
        // A swing and a turn both want the character.
        self.stop_breakable_turn_movement(cx.automation);
        let (id, name) = (self.target_id, self.target_name.clone());
        self.arm_swing_executor(
            id,
            &name,
            attack_height(cx.settings.attack_height.0),
            desired_power,
        );
        let outcome = self.pump_swing_executor(cx.automation);
        translate(outcome)
    }

    pub(super) fn arm_swing_executor(
        &mut self,
        target_object_id: u32,
        target_name: &str,
        height: AttackHeight,
        power: f32,
    ) {
        if self.swing_target_id != target_object_id {
            // A new monster's first swing goes out at once.
            self.next_swing_at = self.now;
        }
        self.swing_target_id = target_object_id;
        target_name.clone_into(&mut self.swing_target_name);
        self.swing_height = height;
        self.swing_power = power;
        self.swing_armed = true;
        self.swing_timer_running = true;
        self.swing_stop_count = 0;
    }

    pub(super) fn pump_swing_executor(
        &mut self,
        automation: &mut dyn Automation,
    ) -> SwingExecutorOutcome {
        if !self.swing_timer_running || !self.enabled || !automation.is_available() {
            self.set_status("Swing timer is not running");
            return SwingExecutorOutcome::None;
        }
        self.action_locks.borrow_mut().advance_to(self.now);
        let combat = automation.combat_snapshot();
        if combat.request_in_progress {
            if combat.build_in_progress
                && combat.power_bar_level + POWER_RELEASE_EPSILON >= combat.desired_power
            {
                return self.release_charged_swing(automation);
            }
            self.set_status(format!(
                "Charging {}%",
                format_f32(combat.power_bar_level * 100.0, "0").unwrap_or_default()
            ));
            self.give_up_on_swing_that_never_went_out(automation);
            return SwingExecutorOutcome::None;
        }
        if combat.server_response_pending || combat.repeat_attack_in_progress {
            self.stamp_swing_sent();
            self.give_up_on_unanswered_swing(automation);
            self.set_status(format!("Attacking {}", self.swing_target_name));
            return SwingExecutorOutcome::None;
        }
        if !self.swing_armed {
            self.set_status("Swing is not armed");
            self.stop_swing_executor(automation);
            return SwingExecutorOutcome::None;
        }
        if self.now < self.next_swing_at {
            self.set_status(format!(
                "Next swing at {} in {} s",
                self.swing_target_name,
                format_f64(self.next_swing_at - self.now, "0.0").unwrap_or_default()
            ));
            return SwingExecutorOutcome::None;
        }
        self.next_swing_at = self.now + SWING_REPEAT_SECONDS;
        self.ask_for_swing(automation)
    }

    fn release_charged_swing(&mut self, automation: &mut dyn Automation) -> SwingExecutorOutcome {
        let release = automation.release_attack();
        if release.status == CombatStatus::Released {
            self.stamp_swing_sent();
            self.set_status(format!("Attacking {}", self.swing_target_name));
            return SwingExecutorOutcome::None;
        }
        self.set_status(format!("Attack release: {}", status_name(release.status)));
        if !matches!(
            release.status,
            CombatStatus::InvalidTarget | CombatStatus::Refused
        ) {
            self.give_up_on_swing_that_never_went_out(automation);
            return SwingExecutorOutcome::None;
        }
        // The swing never left the client and the answer names the
        // monster: it leaves the running, uncharged.
        self.log_line(
            MacroLogChannel::CastInfo,
            &format!(
                "Swing: {} releasing at {} (0x{:08X}){}",
                status_name(release.status),
                self.swing_target_name,
                self.swing_target_id,
                notice_suffix(release.notice.as_ref())
            ),
        );
        let id = self.swing_target_id;
        self.invalidate_for_pass(id);
        self.clear_target(automation);
        SwingExecutorOutcome::Retry
    }

    fn ask_for_swing(&mut self, automation: &mut dyn Automation) -> SwingExecutorOutcome {
        if self
            .action_locks
            .borrow_mut()
            .is_locked(ActionLockKind::MeleeAttackShot)
        {
            self.set_status(format!(
                "Waiting out the interval between shots at {}",
                self.swing_target_name
            ));
            return SwingExecutorOutcome::None;
        }
        let begin =
            automation.begin_attack(self.swing_target_id, self.swing_height, self.swing_power);
        let status = match begin.status {
            CombatStatus::Started => format!("Charging {}", self.swing_target_name),
            CombatStatus::Busy => format!("Waiting on {}", self.swing_target_name),
            CombatStatus::InvalidTarget => "Target disappeared".to_owned(),
            CombatStatus::WrongMode => "Waiting for combat mode".to_owned(),
            other => format!("Attack refused: {}", status_name(other)),
        };
        self.set_status(status);
        self.log_line(
            MacroLogChannel::CastInfo,
            &format!(
                "Swing: {} at {} (0x{:08X}){}",
                status_name(begin.status),
                self.swing_target_name,
                self.swing_target_id,
                notice_suffix(begin.notice.as_ref())
            ),
        );
        if matches!(
            begin.status,
            CombatStatus::InvalidTarget | CombatStatus::Refused
        ) {
            // Nothing armed and nothing reached the server: the monster
            // leaves the running, uncharged.
            automation.abort_attack();
            let id = self.swing_target_id;
            self.invalidate_for_pass(id);
            self.clear_target(automation);
            return SwingExecutorOutcome::Retry;
        }
        if begin.status == CombatStatus::Started {
            // The floor starts where the swing is asked for.
            self.action_locks
                .borrow_mut()
                .arm(ActionLockKind::MeleeAttackShot, SWING_SHOT_FLOOR_SECONDS);
            self.pending_physical_target = self.swing_target_id;
            self.physical_swing_armed_at = self.now;
            self.physical_swing_sent_at = f64::NEG_INFINITY;
            self.physical_swing_closest_distance = f32::INFINITY;
            let (id, name) = (self.swing_target_id, self.swing_target_name.clone());
            self.arm_physical_result_text(id, &name);
        }
        SwingExecutorOutcome::None
    }

    /// The executor's own teardown: the call off and the stop floor.
    pub(super) fn stop_swing_executor(&mut self, automation: &mut dyn Automation) {
        if self.swing_armed
            || self
                .action_locks
                .borrow_mut()
                .is_locked(ActionLockKind::MeleeAttackShot)
        {
            return;
        }
        self.swing_stop_count += 1;
        self.swing_target_id = 0;
        self.swing_target_name.clear();
        automation.abort_attack();
        self.pending_physical_target = 0;
        self.disarm_physical_result_text();
        self.physical_swing_sent_at = f64::NEG_INFINITY;
        self.physical_swing_armed_at = f64::NEG_INFINITY;
        self.physical_swing_closest_distance = f32::INFINITY;
        self.action_locks
            .borrow_mut()
            .arm(ActionLockKind::MeleeAttackShot, SWING_STOP_FLOOR_SECONDS);
        self.swing_timer_running = false;
    }

    /// The attack lost its turn; true once really standing still.
    pub(super) fn disarm_swing_executor(&mut self, automation: &mut dyn Automation) -> bool {
        if self.swing_armed {
            self.swing_armed = false;
            self.action_locks
                .borrow_mut()
                .release(ActionLockKind::MeleeAttackShot);
            self.swing_stop_count = 0;
            self.swing_timer_running = true;
        }
        if self.swing_stop_count <= 1 {
            self.stop_swing_executor(automation);
        }
        self.swing_stop_count > 1
    }

    /// A hand-over to the next monster: no stop floor.
    pub(super) fn release_swing_executor_for_retarget(&mut self) {
        self.swing_armed = false;
        self.swing_timer_running = false;
        self.swing_stop_count = 0;
        self.swing_target_id = 0;
        self.swing_target_name.clear();
    }

    /// The executor put down entirely.
    pub(super) fn reset_swing_executor(&mut self) {
        self.release_swing_executor_for_retarget();
        self.next_swing_at = 0.0;
    }

    /// Steps the swing executor on the host frame.
    pub fn drive_swing_executor(&mut self, automation: &mut dyn Automation, elapsed_seconds: f64) {
        if !self.swing_timer_running || !self.enabled || self.paused || !automation.is_available() {
            return;
        }
        self.advance_clock_from_frame(elapsed_seconds);
        let _ = self.pump_swing_executor(automation);
    }

    fn stamp_swing_sent(&mut self) {
        if self.pending_physical_target == 0 || self.physical_swing_sent_at != f64::NEG_INFINITY {
            return;
        }
        self.physical_swing_sent_at = self.now;
        self.physical_swing_armed_at = f64::NEG_INFINITY;
        self.physical_swing_closest_distance = f32::INFINITY;
    }

    /// A swing asked for that never got out: torn down, uncharged.
    fn give_up_on_swing_that_never_went_out(&mut self, automation: &mut dyn Automation) {
        if self.physical_swing_armed_at == f64::NEG_INFINITY
            || self.now - self.physical_swing_armed_at < UNANSWERED_SWING_SECONDS
        {
            return;
        }
        self.physical_swing_armed_at = f64::NEG_INFINITY;
        self.pending_physical_target = 0;
        self.disarm_physical_result_text();
        automation.abort_attack();
        self.log_line(
            MacroLogChannel::CastInfo,
            &format!(
                "Swing: never left the client for {} (0x{:08X}) after {}s, asking again",
                self.target_name,
                self.target_id,
                format_f64(UNANSWERED_SWING_SECONDS, "0.0").unwrap_or_default()
            ),
        );
    }

    fn give_up_on_unanswered_swing(&mut self, automation: &mut dyn Automation) {
        if self.pending_physical_target == 0 || self.physical_swing_sent_at == f64::NEG_INFINITY {
            return;
        }
        // Ground gained towards the monster starts the wait over.
        let distance = self.find_target(self.pending_physical_target).distance;
        if distance > 0.0 && distance < self.physical_swing_closest_distance {
            let closed = self.physical_swing_closest_distance != f32::INFINITY
                && distance + SWING_CLOSING_PROGRESS_METERS <= self.physical_swing_closest_distance;
            self.physical_swing_closest_distance = distance;
            if closed {
                self.physical_swing_sent_at = self.now;
                return;
            }
        }
        if self.now - self.physical_swing_sent_at < UNANSWERED_SWING_SECONDS {
            return;
        }
        let stalled = self.pending_physical_target;
        self.physical_swing_sent_at = self.now;
        automation.abort_attack();
        self.log_line(
            MacroLogChannel::CastInfo,
            &format!(
                "Swing: no result from {} (0x{stalled:08X}) after {}s, cancelling",
                self.target_name,
                format_f64(UNANSWERED_SWING_SECONDS, "0.0").unwrap_or_default()
            ),
        );
    }
}

/// The settings' attack height as the swing request names it.
pub(super) fn attack_height(value: i32) -> AttackHeight {
    match value {
        1 => AttackHeight::High,
        3 => AttackHeight::Low,
        _ => AttackHeight::Medium,
    }
}

/// The status a refused flight leaves.
pub(super) fn projectile_status(
    result: &ac_plugin_abi::projectile::ProjectilePathResult,
    target_name: &str,
) -> String {
    use ac_plugin_abi::projectile::ProjectilePathStatus as S;
    let target = if target_name.trim().is_empty() {
        "target"
    } else {
        target_name
    };
    match result.status {
        S::Blocked if result.blocking_object_id != 0 => format!(
            "Projectile path to {target} blocked by 0x{:08X}",
            result.blocking_object_id
        ),
        S::Blocked => format!("Projectile path to {target} is blocked"),
        S::Unavailable => "Projectile collision data is unavailable".to_owned(),
        S::BudgetExceeded => "Projectile collision-check budget exhausted".to_owned(),
        S::InvalidTarget => format!("Cannot resolve projectile path to {target}"),
        S::Error => result
            .notice
            .clone()
            .unwrap_or_else(|| "Projectile collision check failed".to_owned()),
        S::Clear => format!("Cannot fire at {target}"),
    }
}
