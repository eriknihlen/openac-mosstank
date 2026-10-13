//! The debuff arm: the first due step of the chain, from its best source
//! (a learned spell, a wand, a proc or a grenade), with the flight checked
//! and the wield gate run; the item debuff watched to its end.

use ac_plugin_abi::combat::{AttackHeight, CombatMode, CombatSnapshot, CombatStatus, CombatTarget};
use ac_plugin_abi::items::{EquipmentStatus, InventoryItem};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::navigation::MovementIntent;
use mosstank_automation::Automation;
use mosstank_automation::components::uses_blacklisted_component;
use mosstank_automation::locks::{ActionLockKind, item_use_lock};
use mosstank_settings::model::CombatSettings;
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions};

use super::equip::resolve_inventory_offhand_object_id;
use super::swing::projectile_status;
use super::{CombatController, Ctx, POWER_RELEASE_EPSILON, PendingItemDebuff};
use crate::MacroLogChannel;
use crate::debuff::{self, CombatDebuffStep, DebuffCompletion, DebuffIdentity};
use crate::gate::{CombatModeGate, GateTurn, Prepare};
use crate::item_debuff::{self, CombatDebuffSource, CombatDebuffSourceKind, DebuffPlanInput};

/// What the debuff arm did with the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DebuffArmOutcome {
    Idle,
    Claimed,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebuffPassResult {
    Idle,
    Claimed,
    ColumnDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebuffStartResult {
    Skipped,
    Handled,
}

fn display_name(target: &CombatTarget) -> String {
    if target.name.trim().is_empty() {
        format!("0x{:08X}", target.object_id)
    } else {
        target.name.clone()
    }
}

impl CombatController {
    pub(super) fn tick_debuffs(
        &mut self,
        cx: &mut Ctx<'_>,
        combat: &CombatSnapshot,
    ) -> DebuffArmOutcome {
        if self.debuffs.has_pending() {
            let status = if cx.automation.is_casting() {
                format!("Casting {}", self.debuffs.pending_name())
            } else {
                format!("Waiting for {}", self.debuffs.pending_name())
            };
            self.set_status(status);
            return DebuffArmOutcome::Claimed;
        }
        // A busy refusal holds the arm for its wait.
        if self.debuffs.retry_held(self.now) {
            self.set_status("Waiting after a busy refusal");
            return DebuffArmOutcome::Claimed;
        }

        self.refresh_spell_catalogs(cx.automation);
        let items = self.pass_inventory(cx.automation);
        let target = self.find_target(self.target_id);
        if target.object_id == 0 {
            return DebuffArmOutcome::Idle;
        }

        let actions = self.decision_actions();
        let element = self.resolve_attack_element(cx, &actions, &target);
        let extra = self.resolve_extra_vulnerability(&actions, &target);
        let steps = debuff::build_chain(&actions, element, extra);
        let mut due = None;
        for step in &steps {
            if self.is_debuff_step_due(cx, step, &target, &items) {
                due = Some(*step);
                break;
            }
        }
        let Some(due) = due else {
            return DebuffArmOutcome::Idle;
        };
        let result = self.tick_debuff_step(cx, due, &actions, &target, combat, &items);
        if result != DebuffPassResult::ColumnDisabled {
            return if result == DebuffPassResult::Claimed {
                DebuffArmOutcome::Claimed
            } else {
                DebuffArmOutcome::Idle
            };
        }
        // The step cannot be delivered: its column is off for the pass and
        // the whole choice is made again.
        self.clear_actions_for_pass(self.target_id, due.identity.flag);
        DebuffArmOutcome::Retry
    }

    fn tick_debuff_step(
        &mut self,
        cx: &mut Ctx<'_>,
        due: CombatDebuffStep,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
        combat: &CombatSnapshot,
        items: &[InventoryItem],
    ) -> DebuffPassResult {
        let choice = loop {
            let Some(winner) = self.choose_debuff_source(cx, due.identity, target, items, true)
            else {
                return DebuffPassResult::Idle;
            };
            let Some(shape) = winner.path_kind else {
                break winner;
            };
            let height = self.height_for_shape(cx, shape);
            let (clear, path) = self.projectile_path_is_clear(cx, target.object_id, shape, height);
            if clear {
                break winner;
            }
            self.set_status(projectile_status(&path, &target.name));
            // Without the fallback the column goes off; with it the blocked
            // shape is now known and the next-best source answers.
            if !cx.settings.allow_debuff_fallback {
                return DebuffPassResult::ColumnDisabled;
            }
        };

        if uses_blacklisted_component(
            cx.automation.spells(),
            &choice.spell,
            &cx.settings.blacklisted_spell_components,
        ) {
            return DebuffPassResult::Idle;
        }
        if !self.ready_for_breakable_turn(cx, &choice.spell, target.object_id) {
            return DebuffPassResult::Claimed;
        }
        if choice.kind != CombatDebuffSourceKind::LearnedSpell {
            let offhand = resolve_inventory_offhand_object_id(actions, items);
            let result = self.try_start_item_debuff(cx, &choice, target, combat, items, offhand);
            return if result == DebuffStartResult::Handled {
                DebuffPassResult::Claimed
            } else {
                DebuffPassResult::Idle
            };
        }

        // A learned debuff is cast from a wand: the wield gate runs first.
        if !self.prepare_for_learned_debuff(cx, actions, target, items) {
            return DebuffPassResult::Claimed;
        }

        let gate = cx
            .automation
            .evaluate_gate(choice.spell.spell_id, Some(target.object_id));
        if gate == CastGate::Busy {
            self.set_status("Waiting to debuff");
            return DebuffPassResult::Claimed;
        }
        if gate != CastGate::Ready
            || !cx
                .automation
                .cast(choice.spell.spell_id, Some(target.object_id))
        {
            return DebuffPassResult::Idle;
        }

        let revision = cx.automation.last_cast_completion().revision;
        self.debuffs.begin(
            target.object_id,
            choice.identity,
            choice.spell.clone(),
            self.now,
            revision,
        );
        self.announce_cast_attempt(cx.automation, target.object_id, &choice.spell);
        let target_name = display_name(target);
        self.log_line(
            MacroLogChannel::SpellCast,
            &format!(
                "Casting: {} on {} ({target_name})",
                choice.spell.name, target.object_id
            ),
        );
        self.set_status(format!("{} \u{2192} {target_name}", choice.spell.name));
        DebuffPassResult::Claimed
    }

    /// The wield gate for a learned debuff: the attack's own wand when the
    /// profile switches wands to debuff, else the first profiled wand.
    fn prepare_for_learned_debuff(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
        items: &[InventoryItem],
    ) -> bool {
        let equipment = self.pass_equipment(cx.automation);
        let mut attack_weapon = 0;
        if cx.settings.switch_wands_to_debuff {
            let (weapon, _, _) = self.resolve_wield_plan(cx, actions, target, items, &equipment);
            if weapon != 0
                && let Some(item) = equipment.iter().find(|item| item.object_id == weapon)
                && CombatModeGate::mode_for(item) == CombatMode::Magic
            {
                attack_weapon = weapon;
            }
        }
        let request = Prepare {
            override_item_id: attack_weapon,
            auto_select: attack_weapon == 0,
            captured: Some(&equipment),
            ..Prepare::mode(CombatMode::Magic)
        };
        let gate = self.gate.clone();
        let ready = {
            let mut turn = GateTurn {
                automation: &mut *cx.automation,
                combat: cx.settings,
                vitals: cx.vitals,
            };
            gate.borrow_mut().try_prepare(&mut turn, request, None)
        };
        self.drain_gate_stops(cx.automation);
        if ready {
            return true;
        }
        let status = gate.borrow().status().to_owned();
        self.set_status(status);
        false
    }

    #[allow(clippy::too_many_lines)]
    fn try_start_item_debuff(
        &mut self,
        cx: &mut Ctx<'_>,
        source: &CombatDebuffSource,
        target: &CombatTarget,
        combat: &CombatSnapshot,
        inventory: &[InventoryItem],
        desired_offhand: u32,
    ) -> DebuffStartResult {
        if !cx.automation.equipment_available() {
            return DebuffStartResult::Skipped;
        }
        if cx.automation.equipment_busy() {
            self.set_status(format!(
                "Equipping {}",
                item_name(source.item_object_id, inventory)
            ));
            return DebuffStartResult::Handled;
        }
        let Some(item) = inventory
            .iter()
            .find(|candidate| candidate.object_id == source.item_object_id)
            .cloned()
        else {
            return DebuffStartResult::Skipped;
        };

        if source.kind == CombatDebuffSourceKind::Grenade && desired_offhand != 0 {
            let equipment = self.pass_equipment(cx.automation);
            if let Some(offhand) = equipment
                .iter()
                .find(|candidate| candidate.object_id == desired_offhand)
                && offhand.equipped_location == 0
            {
                let result = cx.automation.equip(offhand.object_id);
                if result.status == EquipmentStatus::Started
                    || result.status == EquipmentStatus::Busy
                {
                    self.set_status(format!("Equipping {}", offhand.name));
                    return DebuffStartResult::Handled;
                }
                return DebuffStartResult::Skipped;
            }
        }

        if item.equipped_location == 0 {
            let equip = cx.automation.equip(item.object_id);
            if matches!(
                equip.status,
                EquipmentStatus::Started | EquipmentStatus::Busy
            ) {
                self.set_status(format!("Equipping {}", item.name));
                return DebuffStartResult::Handled;
            }
            return DebuffStartResult::Skipped;
        }

        let desired_mode = match source.kind {
            CombatDebuffSourceKind::CasterItem => CombatMode::Magic,
            CombatDebuffSourceKind::Grenade => CombatMode::Missile,
            _ if item.item_type & 0x0000_0100 != 0 => CombatMode::Missile,
            _ => CombatMode::Melee,
        };
        if combat.mode != desired_mode {
            self.enter_debuff_mode(cx.automation, desired_mode);
            return DebuffStartResult::Handled;
        }

        let target_name = display_name(target);
        if source.kind == CombatDebuffSourceKind::CasterItem {
            if !cx.automation.items_available() || cx.automation.items_busy() {
                self.set_status(format!("Waiting to use {}", item.name));
                return DebuffStartResult::Handled;
            }
            let apply = cx.automation.apply_item(item.object_id, target.object_id);
            if !apply.accepted() {
                return DebuffStartResult::Skipped;
            }
            // The wand owns the character until its cast is over.
            self.action_locks.borrow_mut().arm(
                ActionLockKind::ItemUse,
                item_use_lock::HELD_ITEM_CAST_SECONDS,
            );
            self.pending_item_debuff = Some(PendingItemDebuff {
                source: source.clone(),
                target_object_id: target.object_id,
                target_name: target_name.clone(),
                item_name: item.name.clone(),
                dispatched_at: self.now,
                physical_completion_revision: combat.completion_revision,
                power: 0.0,
                attack_completed_at: None,
                recovery_sent: false,
            });
            self.announce_cast_attempt(cx.automation, target.object_id, &source.spell);
            self.set_status(format!(
                "{} via {} \u{2192} {target_name}",
                source.spell.name, item.name
            ));
            return DebuffStartResult::Handled;
        }

        if combat.request_in_progress
            || combat.server_response_pending
            || combat.repeat_attack_in_progress
        {
            self.set_status(format!("Waiting to fire {}", item.name));
            return DebuffStartResult::Handled;
        }
        let power = if desired_mode == CombatMode::Missile {
            1.0
        } else {
            0.0
        };
        let begin = cx
            .automation
            .begin_attack(target.object_id, AttackHeight::Medium, power);
        if begin.status != CombatStatus::Started {
            return if begin.status == CombatStatus::Busy {
                DebuffStartResult::Handled
            } else {
                DebuffStartResult::Skipped
            };
        }
        self.pending_item_debuff = Some(PendingItemDebuff {
            source: source.clone(),
            target_object_id: target.object_id,
            target_name: target_name.clone(),
            item_name: item.name.clone(),
            dispatched_at: self.now,
            physical_completion_revision: combat.completion_revision,
            power,
            attack_completed_at: None,
            recovery_sent: false,
        });
        self.announce_cast_attempt(cx.automation, target.object_id, &source.spell);
        self.set_status(format!("Charging {} for {target_name}", item.name));
        DebuffStartResult::Handled
    }

    fn enter_debuff_mode(&mut self, automation: &mut dyn Automation, mode: CombatMode) {
        let result = automation.enter_combat_mode(mode);
        self.set_status(if result.status == CombatStatus::Refused {
            result
                .notice
                .unwrap_or_else(|| format!("Cannot enter {} mode", crate::mode_name(mode)))
        } else {
            format!("Entering {} mode", crate::mode_name(mode))
        });
    }

    pub(super) fn clear_pending_item_debuff(&mut self) {
        // The item is done with: the slot goes down early.
        if self
            .pending_item_debuff
            .as_ref()
            .is_some_and(|pending| pending.source.kind == CombatDebuffSourceKind::CasterItem)
        {
            self.action_locks
                .borrow_mut()
                .release(ActionLockKind::ItemUse);
        }
        self.pending_item_debuff = None;
    }

    pub(super) fn tick_pending_item_debuff(
        &mut self,
        automation: &mut dyn Automation,
        settings: &CombatSettings,
        combat: &CombatSnapshot,
    ) {
        let Some(pending) = self.pending_item_debuff.clone() else {
            return;
        };
        if self.now - pending.dispatched_at >= 15.0 {
            automation.abort_attack();
            self.set_status(format!("{} timed out", pending.source.spell.name));
            self.clear_pending_item_debuff();
            return;
        }
        if pending.source.kind == CombatDebuffSourceKind::CasterItem {
            self.tick_wand_cast_recovery(automation, settings);
            self.set_status(format!("Waiting for {}", pending.source.spell.name));
            return;
        }

        if combat.request_in_progress {
            if combat.build_in_progress
                && combat.power_bar_level + POWER_RELEASE_EPSILON >= pending.power
            {
                let release = automation.release_attack();
                self.set_status(if release.status == CombatStatus::Released {
                    format!("Firing {}", pending.item_name)
                } else {
                    format!(
                        "Attack release: {}",
                        super::swing::status_name(release.status)
                    )
                });
            } else {
                self.set_status(format!("Charging {}", pending.item_name));
            }
            return;
        }
        if combat.server_response_pending || combat.repeat_attack_in_progress {
            self.set_status(format!("Waiting for {}", pending.source.spell.name));
            return;
        }
        if combat.completion_revision > pending.physical_completion_revision {
            if let Some(stored) = self.pending_item_debuff.as_mut() {
                stored.physical_completion_revision = combat.completion_revision;
                stored.attack_completed_at.get_or_insert(self.now);
            }
            if combat.completion_weenie_error != 0 {
                self.set_status(format!(
                    "{} failed (0x{:X})",
                    pending.item_name, combat.completion_weenie_error
                ));
                self.clear_pending_item_debuff();
                return;
            }
        }
        let completed = self
            .pending_item_debuff
            .as_ref()
            .and_then(|stored| stored.attack_completed_at);
        if let Some(completed) = completed
            && self.now - completed >= 1.0
        {
            self.clear_pending_item_debuff();
            self.set_status(format!("Retrying {}", pending.source.spell.name));
            return;
        }
        self.set_status(format!("Waiting for {}", pending.source.spell.name));
    }

    fn tick_wand_cast_recovery(
        &mut self,
        automation: &mut dyn Automation,
        settings: &CombatSettings,
    ) {
        let Some(pending) = self.pending_item_debuff.as_mut() else {
            return;
        };
        let age = self.now - pending.dispatched_at;
        if settings.jump_out_wand_casting && !pending.recovery_sent && age >= 0.2 {
            let _ = automation.set_movement_intent(MovementIntent {
                jump: true,
                ..MovementIntent::default()
            });
            let _ = automation.clear_movement_intent();
            pending.recovery_sent = true;
        }
    }

    /// How badly a monster needs debuffing (`DebuffUrgency`).
    pub(super) fn debuff_urgency(
        &mut self,
        automation: &mut dyn Automation,
        target: &CombatTarget,
        element: mosstank_data::damage::MonsterDamageType,
        planned_weapon: u32,
        equipment: &[ac_plugin_abi::items::EquipmentItem],
    ) -> i32 {
        const MELEE_WEAPON: u32 = 0x0000_0001;
        const MISSILE_WEAPON: u32 = 0x0000_0100;
        let mut score = 0;
        let vulnerability = DebuffIdentity::new(MonsterActionFlags::VULNERABILITY, element);
        let spell = self.find_debuff_spell(automation, vulnerability);
        if self
            .debuffs
            .is_applied(target.object_id, vulnerability, spell.as_ref(), self.now)
        {
            score += 1;
        }
        let physical = planned_weapon != 0
            && equipment
                .iter()
                .find(|item| item.object_id == planned_weapon)
                .is_some_and(|item| item.item_type & (MELEE_WEAPON | MISSILE_WEAPON) != 0);
        let identity = DebuffIdentity::new(
            if physical {
                MonsterActionFlags::IMPERIL
            } else {
                MonsterActionFlags::YIELD
            },
            mosstank_data::damage::MonsterDamageType::Auto,
        );
        let spell = self.find_debuff_spell(automation, identity);
        if self
            .debuffs
            .is_applied(target.object_id, identity, spell.as_ref(), self.now)
        {
            score += 2;
        }
        score
    }

    /// The strongest known spell of a debuff (once per pass).
    pub(super) fn find_debuff_spell(
        &mut self,
        automation: &mut dyn Automation,
        identity: DebuffIdentity,
    ) -> Option<SpellInfo> {
        if let Some(cached) = self.pass_debuff_spells.get(&identity) {
            return cached.clone();
        }
        if !self.pass_debuff_spells_complete {
            // One walk sorts every known spell to its debuff, keeping the
            // first of the highest tier: what a walk per debuff finds.
            let mut best: std::collections::HashMap<DebuffIdentity, SpellInfo> =
                std::collections::HashMap::new();
            for spell in automation.spells().known_combat_spells() {
                let Some((found, _)) = debuff::classify(&spell) else {
                    continue;
                };
                if best
                    .get(&found)
                    .is_none_or(|current| spell.tier > current.tier)
                {
                    best.insert(found, spell);
                }
            }
            for (found, spell) in best {
                self.pass_debuff_spells.entry(found).or_insert(Some(spell));
            }
            self.pass_debuff_spells_complete = true;
        }
        let best = self
            .pass_debuff_spells
            .get(&identity)
            .cloned()
            .unwrap_or_default();
        self.pass_debuff_spells.insert(identity, best.clone());
        best
    }

    fn debuff_sources(
        &mut self,
        cx: &mut Ctx<'_>,
        identity: DebuffIdentity,
        target: &CombatTarget,
        inventory: &[InventoryItem],
        log: bool,
    ) -> Vec<CombatDebuffSource> {
        if let Some(cached) = self.pass_debuff_sources.get(&(identity, target.object_id)) {
            return cached.clone();
        }
        let skills = cx.automation.skills();
        let grenades = self.game_info.grenade_options.clone();
        let mut lines: Vec<String> = Vec::new();
        let sources = {
            let mut input = DebuffPlanInput {
                settings: cx.settings,
                skills: &skills,
                spells: cx.automation.spells(),
                items: inventory,
                grenades: &grenades,
                target_distance: f64::from(target.distance),
            };
            let mut sink = |line: &str| lines.push(line.to_owned());
            item_debuff::sources(
                identity,
                &mut input,
                if log { Some(&mut sink) } else { None },
            )
        };
        for line in lines {
            self.log_line(MacroLogChannel::DebuffChoice, &line);
        }
        self.pass_debuff_sources
            .insert((identity, target.object_id), sources.clone());
        sources
    }

    /// The one source a step will be applied from.
    fn choose_debuff_source(
        &mut self,
        cx: &mut Ctx<'_>,
        identity: DebuffIdentity,
        target: &CombatTarget,
        inventory: &[InventoryItem],
        log: bool,
    ) -> Option<CombatDebuffSource> {
        let sources = self.debuff_sources(cx, identity, target, inventory, log);
        sources.into_iter().find(|source| {
            !cx.settings.allow_debuff_fallback
                || self.known_clear(target.object_id, source.path_kind)
        })
    }

    pub(super) fn is_debuff_step_due(
        &mut self,
        cx: &mut Ctx<'_>,
        step: &CombatDebuffStep,
        target: &CombatTarget,
        inventory: &[InventoryItem],
    ) -> bool {
        let sources = self.debuff_sources(cx, step.identity, target, inventory, false);
        let Some(first) = sources.first() else {
            return false;
        };
        let tolerance = if step.zero_tolerance {
            0.0
        } else {
            cx.settings.debuff_precast_seconds
        };
        self.debuffs.is_due(
            target.object_id,
            step.identity,
            &first.spell,
            self.now,
            tolerance,
        )
    }

    pub(super) fn on_learned_debuff_completed(
        &mut self,
        automation: &mut dyn Automation,
        completion: DebuffCompletion,
    ) {
        if !completion.completed {
            return;
        }
        if !completion.succeeded {
            self.set_status(format!(
                "{} failed (0x{:X})",
                completion.spell_name, completion.weenie_error
            ));
            return;
        }
        if let Some(spell) = completion.spell {
            self.announce_cast_success(automation, completion.target_object_id, &spell);
        }
    }

    pub(super) fn announce_cast_attempt(
        &mut self,
        automation: &mut dyn Automation,
        target_object_id: u32,
        spell: &SpellInfo,
    ) {
        if !(self.cast_sharing_enabled)() {
            return;
        }
        let skill = effective_skill_for(automation, spell);
        automation.announce_cast_attempt(target_object_id, spell.spell_id, skill);
    }

    pub(super) fn announce_cast_success(
        &mut self,
        automation: &mut dyn Automation,
        target_object_id: u32,
        spell: &SpellInfo,
    ) {
        if !(self.cast_sharing_enabled)()
            || spell.duration_seconds <= 0.0
            || spell.duration_seconds.is_nan()
        {
            return;
        }
        let skill = effective_skill_for(automation, spell);
        automation.announce_cast_success(
            target_object_id,
            spell.spell_id,
            skill,
            f64::from(spell.duration_seconds),
        );
    }

    /// Reads the server's answer to a learned debuff on the host frame.
    pub fn observe_learned_debuff_receipt(
        &mut self,
        automation: &mut dyn Automation,
        elapsed_seconds: f64,
    ) {
        if !self.debuffs.has_pending() || !automation.is_available() {
            return;
        }
        self.advance_clock_from_frame(elapsed_seconds);
        let completion = automation.last_cast_completion();
        let answered = self.debuffs.observe(&completion, self.now);
        self.on_learned_debuff_completed(automation, answered);
        self.debuffs.expire_pending(self.now, 15.0);
    }
}

/// The character's skill in the spell's school (0 when none).
fn effective_skill_for(automation: &mut dyn Automation, spell: &SpellInfo) -> i32 {
    if spell.school == 0 {
        return 0;
    }
    automation
        .try_get_skill(spell.school)
        .map_or(0, |skill| i32::try_from(skill.current).unwrap_or(i32::MAX))
}

fn item_name(object_id: u32, inventory: &[InventoryItem]) -> String {
    inventory
        .iter()
        .find(|item| item.object_id == object_id)
        .map_or_else(|| format!("0x{object_id:08X}"), |item| item.name.clone())
}
