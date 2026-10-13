//! The magic attack: the arm (ring, bolt or arc, streak, drain, fists) the
//! rule's columns choose, the best usable spell of it, the flight check,
//! the breakable turn, and the cast through the shared tracker.

use std::cell::RefCell;
use std::collections::HashMap;

use ac_plugin_abi::combat::AttackHeight;
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::projectile::{ProjectilePathQuery, ProjectilePathResult, ProjectilePathStatus};
use ac_plugin_abi::world::ProjectilePathKind;
use mosstank_automation::Automation;
use mosstank_automation::cast_tracker::{CastRequest, SpellCastTracker};
use mosstank_automation::components::uses_blacklisted_component;
use mosstank_data::attack_spells::{
    AttackSpellCatalog, AttackSpellChoice, VOID_RING_SPELL_ID, VtankCombatSpellType,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::model::UseArcsMode;
use mosstank_settings::monster::MonsterActionFlags;

use super::swing::projectile_status;
use super::{AttackPassOutcome, CombatController, Ctx, element_name};
use crate::MacroLogChannel;

const NO_USABLE_ATTACK_SPELL: &str = "No usable attack spell";

/// Whether a spell is cast in an instant (the reference's own test).
pub(super) fn is_vtank_instant_cast(spell: &SpellInfo) -> bool {
    if spell.difficulty < 50 {
        return true;
    }
    if spell.is_untargeted
        && !spell.is_fellowship
        && spell.duration_seconds >= 60.0
        && matches!(spell.school, 31 | 33)
    {
        return true;
    }
    matches!(spell.family, 243..=249 | 639)
}

/// What the usability test reads, borrowed for one plan.
struct Usability<'a> {
    automation: RefCell<&'a mut dyn Automation>,
    blacklist: &'a str,
    hunt_excess: i32,
    components: RefCell<&'a mut HashMap<u32, bool>>,
    refusals: RefCell<&'a mut HashMap<u32, (String, i32, String)>>,
}

impl Usability<'_> {
    /// Whether the attack may pick the spell; a refusal is remembered.
    fn usable(&self, spell: &SpellInfo) -> bool {
        let refusal = {
            let mut automation = self.automation.borrow_mut();
            if uses_blacklisted_component(automation.spells(), spell, self.blacklist) {
                Some("uses a blacklisted component")
            } else if !self.has_components(&mut **automation, spell.spell_id) {
                Some("missing components")
            } else if !can_cast_hunt_spell(&mut **automation, spell, self.hunt_excess) {
                Some("skill too low")
            } else {
                None
            }
        };
        let Some(reason) = refusal else {
            return true;
        };
        self.refusals.borrow_mut().insert(
            spell.spell_id,
            (spell.name.clone(), spell.quality(), reason.to_owned()),
        );
        false
    }

    fn has_components(&self, automation: &mut dyn Automation, spell_id: u32) -> bool {
        let mut components = self.components.borrow_mut();
        if let Some(&cached) = components.get(&spell_id) {
            return cached;
        }
        let answer = automation.has_components(spell_id);
        components.insert(spell_id, answer);
        answer
    }
}

/// The skill margin the attack-spell tier walk asks for (range is not
/// part of it).
fn can_cast_hunt_spell(automation: &mut dyn Automation, spell: &SpellInfo, excess: i32) -> bool {
    if spell.school == 0 {
        return true;
    }
    let Some(skill) = automation.try_get_skill(spell.school) else {
        return true;
    };
    i64::from(skill.current) >= i64::from(spell.difficulty) + i64::from(excess)
}

impl CombatController {
    /// Runs `plan` with the usability test over this pass's caches.
    fn with_usability<R>(
        &mut self,
        cx: &mut Ctx<'_>,
        plan: impl FnOnce(&AttackSpellCatalog, &dyn Fn(&SpellInfo) -> bool) -> R,
    ) -> R {
        let mut components = std::mem::take(&mut self.pass_components);
        let mut refusals = std::mem::take(&mut self.pass_spell_refusals);
        let result = {
            let usability = Usability {
                automation: RefCell::new(&mut *cx.automation),
                blacklist: &cx.settings.blacklisted_spell_components,
                hunt_excess: cx.settings.hunt_skill_excess_over_difficulty,
                components: RefCell::new(&mut components),
                refusals: RefCell::new(&mut refusals),
            };
            let usable = |spell: &SpellInfo| usability.usable(spell);
            plan(&self.attack_catalog, &usable)
        };
        self.pass_components = components;
        self.pass_spell_refusals = refusals;
        result
    }

    pub(super) fn has_casting_components(
        &mut self,
        automation: &mut dyn Automation,
        spell_id: u32,
    ) -> bool {
        if let Some(&cached) = self.pass_components.get(&spell_id) {
            return cached;
        }
        let answer = automation.has_components(spell_id);
        self.pass_components.insert(spell_id, answer);
        answer
    }

    pub(super) fn refresh_spell_catalogs(&mut self, automation: &mut dyn Automation) {
        let spells = automation.spells().known_combat_spells_shared();
        let attacks = automation.spells().known_attack_spells_shared();
        // The same shared list is the same list; another is compared.
        let same = |kept: Option<&std::rc::Rc<[SpellInfo]>>, now: &std::rc::Rc<[SpellInfo]>| {
            kept.is_some_and(|kept| std::rc::Rc::ptr_eq(kept, now) || **kept == **now)
        };
        if same(self.combat_spell_snapshot.as_ref(), &spells)
            && same(self.attack_spell_snapshot.as_ref(), &attacks)
        {
            return;
        }
        if attacks.is_empty() {
            self.attack_catalog = AttackSpellCatalog::build(&spells);
        } else {
            let mut union = Vec::with_capacity(spells.len() + attacks.len());
            let mut seen = std::collections::HashSet::new();
            for spell in spells.iter().chain(attacks.iter()) {
                if seen.insert(spell.spell_id) {
                    union.push(spell.clone());
                }
            }
            self.attack_catalog = AttackSpellCatalog::build(&union);
        }
        self.combat_spell_snapshot = Some(spells);
        self.attack_spell_snapshot = Some(attacks);
    }

    pub(super) fn tick_magic(&mut self, cx: &mut Ctx<'_>) -> AttackPassOutcome {
        if cx.automation.is_casting() {
            self.set_status(format!("Casting at {}", self.target_name));
            return AttackPassOutcome::Claimed;
        }
        self.refresh_spell_catalogs(cx.automation);
        self.plan_kept_the_monster_in_play = false;
        let target = self.find_target(self.target_id);
        let actions = self.decision_actions();
        let element = self.resolve_attack_element(cx, &actions, &target);

        if actions.damage_type == MonsterDamageType::Fists {
            let fists = self.attack_catalog.resolve_tusker_fists();
            if let Some(fists) = fists
                && self.with_usability(cx, |_, usable| usable(&fists))
            {
                if fists.is_projectile {
                    let (clear, path) = self.projectile_path_is_clear(
                        cx,
                        self.target_id,
                        ProjectilePathKind::Straight,
                        AttackHeight::Medium,
                    );
                    if !clear {
                        self.set_status(projectile_status(&path, &self.target_name));
                        self.invalidate_for_pass(self.target_id);
                        return AttackPassOutcome::Retry;
                    }
                }
                // Fists turns whatever the turning option says.
                if !self.face_for_fists(cx.automation, self.target_id) {
                    return AttackPassOutcome::Claimed;
                }
                self.cast_attack_spell(
                    cx,
                    &AttackSpellChoice {
                        spell: fists,
                        kind: VtankCombatSpellType::War,
                        damage_type: MonsterDamageType::Fists,
                        cast_without_target: false,
                    },
                    &target,
                );
                return AttackPassOutcome::Claimed;
            }
        }

        let flag3 = actions.uses_primary_attack();
        let flag4 = actions.uses_ring();
        let flag5 = actions.uses_streak();
        let ring_count = self.ring_candidate_count;
        let plan: Option<AttackSpellChoice>;

        if (flag4 && ring_count >= cx.settings.minimum_ring_targets)
            || (flag4 && !flag3 && !flag5 && ring_count > 0)
        {
            plan = if element == MonsterDamageType::DrainAuto {
                self.plan_drain(cx, &target, true)
            } else {
                match self.plan_ring(cx, element) {
                    Some(ring) => Some(ring),
                    None => self.plan_bolt_or_arc(cx, element, &target),
                }
            };
        } else if (flag3 && !flag5) || (!flag3 && !flag5 && flag4) {
            plan = if element == MonsterDamageType::DrainAuto {
                self.plan_drain(cx, &target, false)
            } else if self.target_actions().damage_type == MonsterDamageType::Random {
                // A rolled element names its war spell outright.
                self.plan_rolled_war(element)
            } else {
                self.plan_bolt_or_arc(cx, element, &target)
            };
        } else if !flag3 && flag5 {
            plan = if element == MonsterDamageType::DrainAuto {
                self.plan_drain(cx, &target, false)
            } else {
                match self.plan_streak(cx, element) {
                    Some(streak) => Some(streak),
                    None => self.warn_no_streak(cx, element, &target),
                }
            };
        } else {
            if !flag3 || !flag5 {
                self.set_status(format!("No attack configured for {}", self.target_name));
                self.cannot_attack_reason =
                    Some("no attack is configured for it on the Monsters tab".to_owned());
                self.invalidate_for_pass(self.target_id);
                return AttackPassOutcome::Retry;
            }
            if element == MonsterDamageType::DrainAuto {
                plan = self.plan_drain(cx, &target, false);
            } else {
                let streak = self.plan_streak(cx, element);
                plan = if self.is_finishing_blow(cx, &target, streak.as_ref()) {
                    match streak {
                        Some(streak) => Some(streak),
                        None => self.warn_no_streak(cx, element, &target),
                    }
                } else {
                    self.plan_bolt_or_arc(cx, element, &target)
                };
            }
        }

        let Some(chosen) = plan else {
            if !self.status.starts_with(NO_USABLE_ATTACK_SPELL)
                && !self.pass_spell_refusals.is_empty()
            {
                let reason = self.no_usable_attack_spell_reason();
                self.set_status(reason);
            }
            if self.status.starts_with(NO_USABLE_ATTACK_SPELL) {
                self.cannot_attack_reason = Some(self.status.clone());
            }
            if !self.plan_kept_the_monster_in_play {
                self.invalidate_for_pass(self.target_id);
            }
            return AttackPassOutcome::Retry;
        };

        // A streak flies the way a bolt does, so it is tested the same way.
        if chosen.kind == VtankCombatSpellType::Streak {
            let (clear, path) = self.projectile_path_is_clear(
                cx,
                self.target_id,
                ProjectilePathKind::Straight,
                AttackHeight::Medium,
            );
            if !clear {
                self.set_status(projectile_status(&path, &self.target_name));
                let mut off = MonsterActionFlags::STREAK;
                if !self.known_clear(self.target_id, Some(ProjectilePathKind::Arc)) {
                    off |= MonsterActionFlags::ATTACK;
                }
                self.clear_actions_for_pass(self.target_id, off);
                return AttackPassOutcome::Retry;
            }
        }

        self.cast_attack_spell(cx, &chosen, &target);
        AttackPassOutcome::Claimed
    }

    fn plan_rolled_war(&self, element: MonsterDamageType) -> Option<AttackSpellChoice> {
        self.attack_catalog
            .resolve_base_tier(element, VtankCombatSpellType::War)
            .map(|spell| AttackSpellChoice {
                spell,
                kind: VtankCombatSpellType::War,
                damage_type: element,
                cast_without_target: false,
            })
    }

    /// The ring arm: admitted on the first rung's components alone.
    fn plan_ring(
        &mut self,
        cx: &mut Ctx<'_>,
        element: MonsterDamageType,
    ) -> Option<AttackSpellChoice> {
        let mut base_tier = self
            .attack_catalog
            .resolve_base_tier(element, VtankCombatSpellType::Ring);
        let void_ring = matches!(
            element,
            MonsterDamageType::Nether | MonsterDamageType::VoidBasic
        );
        if base_tier.is_none() && void_ring {
            base_tier = cx.automation.spells().try_get(VOID_RING_SPELL_ID);
        }
        let family = base_tier?;
        if !self.has_casting_components(cx.automation, family.spell_id) {
            return None;
        }
        let spell = self.with_usability(cx, |catalog, usable| {
            if void_ring {
                catalog.resolve_family_of(&family, Some(usable))
            } else {
                catalog.resolve(element, VtankCombatSpellType::Ring, Some(usable))
            }
        });
        Some(AttackSpellChoice {
            spell: spell.unwrap_or(family),
            kind: VtankCombatSpellType::Ring,
            damage_type: element,
            cast_without_target: true,
        })
    }

    fn plan_streak(
        &mut self,
        cx: &mut Ctx<'_>,
        element: MonsterDamageType,
    ) -> Option<AttackSpellChoice> {
        self.with_usability(cx, |catalog, usable| {
            catalog.resolve(element, VtankCombatSpellType::Streak, Some(usable))
        })
        .map(|spell| AttackSpellChoice {
            spell,
            kind: VtankCombatSpellType::Streak,
            damage_type: element,
            cast_without_target: false,
        })
    }

    fn warn_no_streak(
        &mut self,
        cx: &mut Ctx<'_>,
        element: MonsterDamageType,
        target: &ac_plugin_abi::combat::CombatTarget,
    ) -> Option<AttackSpellChoice> {
        self.post_attack_warning(
            cx.automation,
            &format!(
                "No streak spell usable for element '{}', using bolt/arc instead.",
                element_name(element)
            ),
        );
        self.plan_bolt_or_arc(cx, element, target)
    }

    /// Whether the streak is the finishing move: the bar is the last blow's
    /// size (the streak's difficulty until a blow is seen).
    fn is_finishing_blow(
        &self,
        cx: &mut Ctx<'_>,
        target: &ac_plugin_abi::combat::CombatTarget,
        streak: Option<&AttackSpellChoice>,
    ) -> bool {
        let Some(choice) = streak else {
            return false;
        };
        let threshold = if self.health.last_damage() <= 0 {
            choice.spell.difficulty / 7
        } else {
            self.health.last_damage() / 7
        };
        let remaining = self.remaining_health(cx);
        self.health.target_object_id() == target.object_id && remaining > 0 && remaining < threshold
    }

    pub(super) fn remaining_health(&self, cx: &mut Ctx<'_>) -> i32 {
        let automation = &mut *cx.automation;
        let mut is_tracked = |object_id: u32| {
            !automation.objects_available() || automation.object(object_id).is_some()
        };
        self.health
            .remaining_health(&cx.settings.monster_facts, &mut is_tracked)
    }

    fn plan_bolt_or_arc(
        &mut self,
        cx: &mut Ctx<'_>,
        element: MonsterDamageType,
        target: &ac_plugin_abi::combat::CombatTarget,
    ) -> Option<AttackSpellChoice> {
        // The decision's own record of which shape it found blocked.
        let mut bolt_blocked = false;
        let mut arc_blocked = false;
        let mut projectile_refusal: Option<String> = None;
        for _ in 0..3 {
            let (bolt, arc) = self.with_usability(cx, |catalog, usable| {
                let bolt = (!bolt_blocked)
                    .then(|| catalog.resolve(element, VtankCombatSpellType::War, Some(usable)))
                    .flatten();
                let arc = (!arc_blocked)
                    .then(|| catalog.resolve(element, VtankCombatSpellType::Arc, Some(usable)))
                    .flatten();
                (bolt, arc)
            });

            let (kind, spell) = match (bolt, arc) {
                (None, None) => {
                    self.post_attack_warning(
                        cx.automation,
                        &format!(
                            "Warning: no usable attack spell detected for element \"{}\"",
                            element_name(element)
                        ),
                    );
                    let status = match projectile_refusal {
                        Some(refusal) => refusal,
                        None => self.no_usable_attack_spell_reason(),
                    };
                    self.set_status(status);
                    return None;
                }
                (None, Some(arc)) => (VtankCombatSpellType::Arc, arc),
                (Some(bolt), None) => (VtankCombatSpellType::War, bolt),
                (Some(bolt), Some(arc)) => {
                    if bolt.quality() > arc.quality() {
                        (VtankCombatSpellType::War, bolt)
                    } else if arc.quality() > bolt.quality() {
                        (VtankCombatSpellType::Arc, arc)
                    } else if cx.settings.use_arcs == UseArcsMode::AT_RANGE {
                        // The setting decides only an exact tie.
                        if f64::from(target.distance) >= cx.settings.arc_range {
                            (VtankCombatSpellType::Arc, arc)
                        } else {
                            (VtankCombatSpellType::War, bolt)
                        }
                    } else if cx.settings.use_arcs == UseArcsMode::YES {
                        (VtankCombatSpellType::Arc, arc)
                    } else {
                        (VtankCombatSpellType::War, bolt)
                    }
                }
            };

            let arc_shape = kind == VtankCombatSpellType::Arc;
            let shape = if arc_shape {
                ProjectilePathKind::Arc
            } else {
                ProjectilePathKind::Straight
            };
            if spell.is_projectile {
                let height = self.height_for_shape(cx, shape);
                let (clear, path) =
                    self.projectile_path_is_clear(cx, self.target_id, shape, height);
                if !clear {
                    let refusal = projectile_status(&path, &self.target_name);
                    self.set_status(refusal.clone());
                    projectile_refusal = Some(refusal);
                    if arc_shape {
                        arc_blocked = true;
                    } else {
                        bolt_blocked = true;
                        // A streak flies as a bolt does.
                        self.clear_actions_for_pass(self.target_id, MonsterActionFlags::STREAK);
                    }
                    if arc_blocked && bolt_blocked {
                        // Neither shape can reach: the attack column is off,
                        // the monster stays in the running.
                        self.clear_actions_for_pass(self.target_id, MonsterActionFlags::ATTACK);
                        self.plan_kept_the_monster_in_play = true;
                        return None;
                    }
                    continue;
                }
            }
            return Some(AttackSpellChoice {
                spell,
                kind,
                damage_type: element,
                cast_without_target: false,
            });
        }
        None
    }

    /// The void caster's drain arm.
    #[allow(clippy::cast_possible_truncation)]
    fn plan_drain(
        &mut self,
        cx: &mut Ctx<'_>,
        target: &ac_plugin_abi::combat::CombatTarget,
        ring: bool,
    ) -> Option<AttackSpellChoice> {
        if self.health.target_object_id() != target.object_id {
            return None;
        }
        let character = cx.automation.character();
        let health = i32::try_from(character.health.0).unwrap_or(i32::MAX);
        let maximum_health = i32::try_from(character.health.1).unwrap_or(i32::MAX);
        let target_health = self.remaining_health(cx);
        if health == 0 || maximum_health == 0 || target_health == 0 {
            return None;
        }
        // One below the health the recharge calls normal.
        let floor =
            (f64::from(maximum_health) * cx.vitals.normal_health.clamp(0.0, 1.0)).ceil() as i32 - 1;
        let can_drain = !cx.settings.monster_facts.is_immune_to_magic(&target.name)
            && target_health != i32::MAX;
        let drains = self.game_info.drain_spell_options.clone();
        let martyrs = self.game_info.martyr_spell_options.clone();
        let combat_spells = cx.automation.spells().known_combat_spells_shared();
        let attack_spells = cx.automation.spells().known_attack_spells_shared();
        let find_known = |spell_id: u32| -> Option<SpellInfo> {
            if spell_id == 0 {
                return None;
            }
            combat_spells
                .iter()
                .chain(attack_spells.iter())
                .find(|spell| spell.spell_id == spell_id)
                .cloned()
        };
        let spell_id = self.with_usability(cx, |_, usable| {
            let mut castable =
                |candidate: u32| find_known(candidate).is_some_and(|spell| usable(&spell));
            crate::drain::select_spell(
                health,
                maximum_health,
                floor,
                target_health,
                can_drain,
                ring,
                &drains,
                &martyrs,
                &mut castable,
            )
        });
        let Some(chosen) = (spell_id != 0).then(|| find_known(spell_id)).flatten() else {
            self.log_line(
                MacroLogChannel::DebuffChoice,
                &format!("Drain: nothing better than a heal against {}", target.name),
            );
            return None;
        };
        self.log_line(
            MacroLogChannel::DebuffChoice,
            &format!(
                "Drain: {} ({target_health} left, floor {floor})",
                chosen.name
            ),
        );
        Some(AttackSpellChoice {
            spell: chosen,
            kind: if ring {
                VtankCombatSpellType::Ring
            } else {
                VtankCombatSpellType::War
            },
            damage_type: MonsterDamageType::DrainAuto,
            cast_without_target: false,
        })
    }

    /// Why the pass found no attack spell: one clause per reason.
    fn no_usable_attack_spell_reason(&self) -> String {
        if self.pass_spell_refusals.is_empty() {
            return format!("{NO_USABLE_ATTACK_SPELL}: none of the wanted kind is known");
        }
        let mut reasons: Vec<&str> = self
            .pass_spell_refusals
            .values()
            .map(|(_, _, reason)| reason.as_str())
            .collect();
        reasons.sort_unstable();
        reasons.dedup();
        let clauses: Vec<String> = reasons
            .iter()
            .map(|reason| {
                let mut group: Vec<&(String, i32, String)> = self
                    .pass_spell_refusals
                    .values()
                    .filter(|(_, _, r)| r == reason)
                    .collect();
                group.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                let best = &group[0].0;
                let others = group.len() - 1;
                if others == 0 {
                    format!("{best}: {reason}")
                } else {
                    format!("{best} and {others} lower: {reason}")
                }
            })
            .collect();
        format!("{NO_USABLE_ATTACK_SPELL} ({})", clauses.join("; "))
    }

    pub(super) fn report_given_up_target(
        &mut self,
        automation: &mut dyn Automation,
        target_id: u32,
        name: &str,
        reason: &str,
    ) {
        if target_id == 0
            || self
                .reported_give_ups
                .get(&target_id)
                .is_some_and(|said| said == reason)
        {
            return;
        }
        if self.reported_give_ups.len() >= 256 {
            self.reported_give_ups.clear();
        }
        self.reported_give_ups.insert(target_id, reason.to_owned());
        crate::post(
            automation,
            &format!("[MossTank] Not attacking {name}: {reason}"),
        );
    }

    fn cast_attack_spell(
        &mut self,
        cx: &mut Ctx<'_>,
        choice: &AttackSpellChoice,
        target: &ac_plugin_abi::combat::CombatTarget,
    ) {
        let _ = target;
        let shape = if choice.kind == VtankCombatSpellType::Arc {
            ProjectilePathKind::Arc
        } else {
            ProjectilePathKind::Straight
        };
        if !choice.cast_without_target && choice.spell.is_projectile {
            let height = self.height_for_shape(cx, shape);
            let (clear, path) = self.projectile_path_is_clear(cx, self.target_id, shape, height);
            if !clear {
                self.set_status(projectile_status(&path, &self.target_name));
                return;
            }
        }
        // A cast of one attack school holds the other off for a while.
        if self
            .cast_tracker
            .borrow()
            .is_school_locked_out(choice.spell.school)
        {
            self.set_status(format!("Waiting to cast at {}", self.target_name));
            return;
        }
        if !choice.cast_without_target
            && !self.ready_for_breakable_turn(cx, &choice.spell, self.target_id)
        {
            return;
        }
        let gate = if choice.cast_without_target {
            cx.automation.evaluate_gate(choice.spell.spell_id, None)
        } else {
            cx.automation
                .evaluate_gate(choice.spell.spell_id, Some(self.target_id))
        };
        if gate != CastGate::Ready {
            self.set_status(if gate == CastGate::Busy {
                format!("Waiting to cast at {}", self.target_name)
            } else {
                format!("Cannot cast {}", choice.spell.name)
            });
            return;
        }
        // A swing and a cast both want the character.
        if self.physical_result_armed || self.pending_physical_target != 0 {
            cx.automation.abort_attack();
            self.disarm_physical_result_text();
            self.pending_physical_target = 0;
        }
        let dispatched = if choice.cast_without_target {
            cx.automation.cast(choice.spell.spell_id, None)
        } else {
            cx.automation
                .cast(choice.spell.spell_id, Some(self.target_id))
        };
        if !dispatched {
            self.set_status(format!("Could not start {}", choice.spell.name));
            return;
        }

        self.log_line(
            MacroLogChannel::SpellCast,
            &format!(
                "Casting: {} on {} ({})",
                choice.spell.name, self.target_id, self.target_name
            ),
        );
        self.set_status(if choice.kind == VtankCombatSpellType::Ring {
            format!("{} around {}", choice.spell.name, self.target_name)
        } else {
            format!("{} \u{2192} {}", choice.spell.name, self.target_name)
        });

        let character = cx.automation.character();
        let self_cast = self.target_id != 0 && self.target_id == character.id;
        let target_name = if choice.cast_without_target {
            String::new()
        } else if self_cast {
            "yourself".to_owned()
        } else {
            self.target_name.clone()
        };
        let incarnation = if choice.cast_without_target {
            0
        } else {
            self.find_target(self.target_id).incarnation
        };
        self.cast_tracker.borrow_mut().begin(&CastRequest {
            spell_id: choice.spell.spell_id,
            spell_name: &choice.spell.name,
            target_object_id: if choice.cast_without_target {
                0
            } else {
                self.target_id
            },
            target_name: &target_name,
            hits_multiple_targets: SpellCastTracker::hits_multiple_targets_for(&choice.spell),
            saying: &choice.spell.saying,
            school: choice.spell.school,
            can_kill: SpellCastTracker::can_kill_for(&choice.spell),
            current_mana: i32::try_from(character.mana.0).unwrap_or(i32::MAX),
            target_incarnation: incarnation,
        });
        self.log_line(MacroLogChannel::CastInfo, "SpellCaster: Begin");
        if !choice.cast_without_target {
            self.pending_attack_spell = choice.spell.spell_id;
            self.pending_attack_target = self.target_id;
        }
    }

    /// The height a flight is tested at: an arc high, a bolt level, the
    /// rest at the profile's swing height.
    pub(super) fn height_for_shape(&self, cx: &Ctx<'_>, kind: ProjectilePathKind) -> AttackHeight {
        match kind {
            ProjectilePathKind::Arc => AttackHeight::High,
            ProjectilePathKind::Straight => AttackHeight::Medium,
            ProjectilePathKind::Missile => super::swing::attack_height(cx.settings.attack_height.0),
        }
    }

    /// Whether the way to the monster is clear for this shape (asked once
    /// per pass).
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn projectile_path_is_clear(
        &mut self,
        cx: &mut Ctx<'_>,
        target_object_id: u32,
        kind: ProjectilePathKind,
        height: AttackHeight,
    ) -> (bool, ProjectilePathResult) {
        if !cx.settings.use_projectile_awareness {
            return (true, clear_result());
        }
        if let Some((_, memo)) = self
            .pass_clearance
            .iter()
            .find(|((target, shape), _)| *target == target_object_id && *shape == kind)
        {
            return (memo.status == ProjectilePathStatus::Clear, memo.clone());
        }
        let query = ProjectilePathQuery {
            target_id: target_object_id,
            kind: crate::query_path_kind(kind),
            target_height: height,
            projectile_radius: cx.settings.collision_projectile_radius as f32,
            step_distance: cx.settings.collision_step_distance as f32,
            maximum_collision_checks: cx.settings.collision_sample_budget,
            diagnostics: cx.settings.show_collision_debug,
        };
        let mut result = cx.automation.evaluate_projectile_path(&query);
        if cx.settings.show_collision_debug && !result.debug_samples.is_empty() {
            cx.automation.show_projectile_samples(&result.debug_samples);
            cx.automation.log_info(&format!(
                "MossTank collision {kind:?}: {:?}, {} marker(s), {} check(s)",
                result.status,
                result.debug_samples.len(),
                result.collision_checks
            ));
        }
        if result.status == ProjectilePathStatus::Unavailable {
            // A client that cannot test a flight says nothing about it: the
            // shot goes ahead untested.
            self.post_attack_warning(
                cx.automation,
                "Warning: this client cannot test projectile paths, so \"Don't Shoot at Walls\" has no effect.",
            );
            result = clear_result();
        }
        self.pass_clearance
            .push(((target_object_id, kind), result.clone()));
        (result.status == ProjectilePathStatus::Clear, result)
    }

    /// What the pass already knows about a flight (untested: clear).
    pub(super) fn known_clear(
        &self,
        target_object_id: u32,
        kind: Option<ProjectilePathKind>,
    ) -> bool {
        let Some(shape) = kind else {
            return true;
        };
        self.pass_clearance
            .iter()
            .find(|((target, memo_shape), _)| *target == target_object_id && *memo_shape == shape)
            .is_none_or(|(_, result)| result.status == ProjectilePathStatus::Clear)
    }
}

fn clear_result() -> ProjectilePathResult {
    ProjectilePathResult {
        status: ProjectilePathStatus::Clear,
        ..ProjectilePathResult::default()
    }
}
