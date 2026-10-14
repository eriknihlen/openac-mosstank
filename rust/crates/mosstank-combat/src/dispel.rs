//! Clearing vulnerabilities (`DispelController`): the learned self dispel
//! with chorizite, else a dispel item by the reference's order, else a drum
//! on the fellow carrying the worst vulnerabilities; one at a time, watched
//! to the server's answer.

use std::collections::HashMap;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::magic::CastGate;
use ac_plugin_abi::social::FellowMember;
use mosstank_automation::Automation;
use mosstank_automation::locks::{
    ActionLockKind, ActionLockTable, SharedActionLocks, item_use_lock,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_data::spell_id;
use mosstank_settings::model::{CombatSettings, VitalSettings};
use mosstank_settings::monster::MonsterActionFlags;

use crate::debuff;
use crate::gate::{GateTurn, Prepare, SharedCombatModeGate};

const ERADICATE_LIFE_MAGIC_SELF: u32 = spell_id::EradicateLifeMagicSelf;
const ACTION_TIMEOUT_SECONDS: f64 = 15.0;
const ALLY_DISPEL_RANGE_METERS: f32 = 50.0;
const CREATURE_ENCHANTMENT_SKILL: u32 = 31;
const ARCANE_LORE_SKILL: u32 = 14;
const DISPEL_PROTECTION_SPELL: u32 = 3179;

const HIGH_DIFFICULTY_ITEMS: [&str; 3] = [
    "Rune of Dispel",
    "Society Gem of Dispelling",
    "Black Market Gem of Dispelling",
];

const NORMAL_DIFFICULTY_ITEMS: [&str; 4] = [
    "Rune of Dispel",
    "Chocolate Gromnie",
    "Condensed Dispel Potion",
    "Gem of Stillness",
];

/// The status when nothing is being done.
pub const IDLE_STATUS: &str = "Dispel idle";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispelSource {
    Spell,
    Item,
    AllyItem,
}

#[derive(Debug, Clone)]
struct Pending {
    source: DispelSource,
    object_id: u32,
    name: String,
    revision: i64,
}

/// What one call works with.
#[allow(missing_debug_implementations)]
pub struct DispelTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The vital settings (the three dispel switches).
    pub settings: &'a VitalSettings,
    /// The Items profile the drum is looked for in.
    pub combat: &'a CombatSettings,
}

/// The dispel rule.
#[allow(missing_debug_implementations)]
pub struct DispelController {
    pending: Option<Pending>,
    pending_seconds: f64,
    retry_delay: f64,
    frame_observed_seconds: f64,
    status: String,
    gate: Option<SharedCombatModeGate>,
    action_locks: SharedActionLocks,
}

impl Default for DispelController {
    fn default() -> Self {
        Self::new()
    }
}

impl DispelController {
    /// An idle controller with a cooldown table of its own.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: None,
            pending_seconds: 0.0,
            retry_delay: 0.0,
            frame_observed_seconds: 0.0,
            status: IDLE_STATUS.to_owned(),
            gate: None,
            action_locks: ActionLockTable::shared(),
        }
    }

    /// What the controller is doing.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether a learned dispel is unanswered.
    #[must_use]
    pub fn cast_in_flight(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.source == DispelSource::Spell)
    }

    /// Shares the stance gate.
    pub fn bind_combat_mode_gate(&mut self, gate: SharedCombatModeGate) {
        self.gate = Some(gate);
    }

    /// Shares the macro's cooldown table.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = locks;
    }

    /// Reads the server's answer on the host frame; nothing is issued.
    pub fn observe_pending_receipt(
        &mut self,
        automation: &mut dyn Automation,
        elapsed_seconds: f64,
    ) {
        if self.pending.is_none() || !automation.is_available() {
            return;
        }
        let elapsed = elapsed_seconds.max(0.0);
        self.frame_observed_seconds += elapsed;
        self.observe_pending(automation, elapsed);
    }

    /// One turn: true when the rule claims the pass.
    pub fn tick(&mut self, turn: &mut DispelTurn<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        let elapsed = elapsed_seconds.max(0.0);
        let pending_elapsed = (elapsed - self.frame_observed_seconds).max(0.0);
        self.frame_observed_seconds = 0.0;
        self.retry_delay = (self.retry_delay - elapsed).max(0.0);
        if self.observe_pending(turn.automation, pending_elapsed) {
            return true;
        }

        let settings = turn.settings;
        if !can_act
            || self.retry_delay > 0.0
            || !turn.automation.is_available()
            || (!settings.cast_dispel_self
                && !settings.use_dispel_items
                && !settings.use_dispel_drum)
            || self
                .action_locks
                .borrow_mut()
                .is_locked(ActionLockKind::ItemUse)
        {
            return false;
        }

        if settings.cast_dispel_self && self.try_start_self_dispel(turn) {
            return true;
        }
        if settings.use_dispel_items
            && let Some(item) = select_dispel_item(turn.automation)
        {
            let automation = &mut *turn.automation;
            let revision = automation.last_item_completion().revision;
            let result = automation.use_item(item.object_id);
            if result.accepted() {
                self.action_locks
                    .borrow_mut()
                    .arm(ActionLockKind::ItemUse, item_use_lock::IMMEDIATE_SECONDS);
                self.pending = Some(Pending {
                    source: DispelSource::Item,
                    object_id: item.object_id,
                    name: item.name.clone(),
                    revision,
                });
                self.pending_seconds = 0.0;
                self.status = format!("Using {}", item.name);
                return true;
            }
            self.status = format!("Waiting to use {}", item.name);
            return result.status == ItemStatus::Busy;
        }
        if settings.use_dispel_drum && self.try_start_ally_dispel(turn) {
            return true;
        }

        IDLE_STATUS.clone_into(&mut self.status);
        false
    }

    /// Back to the start.
    pub fn reset(&mut self) {
        self.pending = None;
        self.pending_seconds = 0.0;
        self.frame_observed_seconds = 0.0;
        self.retry_delay = 0.0;
        IDLE_STATUS.clone_into(&mut self.status);
    }

    fn try_prepare_magic(
        &mut self,
        turn: &mut DispelTurn<'_>,
        request: Prepare<'_>,
    ) -> Option<bool> {
        let gate = self.gate.clone()?;
        let mut gate = gate.borrow_mut();
        let mut gate_turn = GateTurn {
            automation: &mut *turn.automation,
            combat: turn.combat,
            vitals: turn.settings,
        };
        if gate.try_prepare(&mut gate_turn, request, None) {
            Some(true)
        } else {
            gate.status().clone_into(&mut self.status);
            Some(false)
        }
    }

    fn try_start_self_dispel(&mut self, turn: &mut DispelTurn<'_>) -> bool {
        let automation = &mut *turn.automation;
        let Some(spell) = automation.spells().try_get(ERADICATE_LIFE_MAGIC_SELF) else {
            return false;
        };
        if !automation.spells().is_known(ERADICATE_LIFE_MAGIC_SELF)
            || !has_vulnerability_at_or_below(automation, spell.difficulty)
            || !automation
                .owned_items_shared()
                .iter()
                .any(|item| item.stack_size > 0 && item.name == "Chorizite")
        {
            return false;
        }

        match self.try_prepare_magic(turn, Prepare::mode(CombatMode::Magic)) {
            Some(false) => return true,
            Some(true) => {}
            None => {
                if turn.automation.combat_snapshot().mode != CombatMode::Magic {
                    let mode = turn.automation.enter_combat_mode(CombatMode::Magic);
                    self.status = if mode.accepted() {
                        "Switching to Magic for self dispel".to_owned()
                    } else {
                        "Waiting for Magic mode to self dispel".to_owned()
                    };
                    return true;
                }
            }
        }

        let automation = &mut *turn.automation;
        let target = automation.character().id;
        if automation.evaluate_gate(ERADICATE_LIFE_MAGIC_SELF, Some(target)) != CastGate::Ready {
            "Waiting to cast Eradicate Life Magic Self".clone_into(&mut self.status);
            return true;
        }

        let revision = automation.last_cast_completion().revision;
        if !automation.cast(ERADICATE_LIFE_MAGIC_SELF, Some(target)) {
            "Self dispel was refused".clone_into(&mut self.status);
            self.retry_delay = 0.25;
            return true;
        }
        self.pending = Some(Pending {
            source: DispelSource::Spell,
            object_id: ERADICATE_LIFE_MAGIC_SELF,
            name: spell.name.clone(),
            revision,
        });
        self.pending_seconds = 0.0;
        self.status = format!("Casting {}", spell.name);
        true
    }

    fn try_start_ally_dispel(&mut self, turn: &mut DispelTurn<'_>) -> bool {
        if self
            .action_locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
        {
            return false;
        }
        if !turn.automation.in_fellowship() {
            return false;
        }
        let Some(drum) = select_awakener(turn.automation, turn.combat) else {
            return false;
        };
        let Some(target) = select_ally(turn.automation) else {
            return false;
        };

        let request = Prepare {
            override_item_id: drum.object_id,
            auto_select: false,
            ..Prepare::mode(CombatMode::Magic)
        };
        match self.try_prepare_magic(turn, request) {
            Some(false) => return true,
            Some(true) => {}
            None => {
                if turn.automation.combat_snapshot().mode != CombatMode::Magic {
                    let mode = turn.automation.enter_combat_mode(CombatMode::Magic);
                    self.status = if mode.accepted() {
                        "Switching to Magic for ally dispel".to_owned()
                    } else {
                        "Waiting for Magic mode to dispel ally".to_owned()
                    };
                    return true;
                }
            }
        }

        let automation = &mut *turn.automation;
        let revision = automation.last_item_completion().revision;
        let result = automation.apply_item(drum.object_id, target.object_id);
        if !result.accepted() {
            self.status = format!("Waiting to use {} on {}", drum.name, target.name);
            return result.status == ItemStatus::Busy;
        }
        // The drum is an item use like any other.
        self.action_locks
            .borrow_mut()
            .arm(ActionLockKind::ItemUse, item_use_lock::IMMEDIATE_SECONDS);
        self.pending = Some(Pending {
            source: DispelSource::AllyItem,
            object_id: drum.object_id,
            name: format!("{} on {}", drum.name, target.name),
            revision,
        });
        self.pending_seconds = 0.0;
        self.status = format!("Using {} on {}", drum.name, target.name);
        true
    }

    fn observe_pending(&mut self, automation: &mut dyn Automation, elapsed_seconds: f64) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        self.pending_seconds += elapsed_seconds;

        if pending.source == DispelSource::Spell {
            let completion = automation.last_cast_completion();
            if completion.revision > pending.revision {
                pending.revision = completion.revision;
                if completion.spell_id == pending.object_id {
                    return self.finish(completion.is_success(), completion.weenie_error);
                }
            }
        } else {
            let completion = automation.last_item_completion();
            if completion.revision > pending.revision {
                pending.revision = completion.revision;
                if completion.source_id == pending.object_id {
                    let success = completion.revision != 0 && completion.weenie_error == 0;
                    return self.finish(success, completion.weenie_error);
                }
            }
        }

        if self.pending_seconds < ACTION_TIMEOUT_SECONDS {
            return true;
        }
        let name = self
            .pending
            .as_ref()
            .map_or_else(String::new, |p| p.name.clone());
        self.status = format!("Dispel timed out: {name}");
        self.clear_pending();
        true
    }

    fn finish(&mut self, succeeded: bool, weenie_error: u32) -> bool {
        let name = self
            .pending
            .as_ref()
            .map_or_else(|| "dispel".to_owned(), |p| p.name.clone());
        self.status = if succeeded {
            format!("Dispel completed: {name}")
        } else {
            format!("Dispel failed (0x{weenie_error:X}): {name}")
        };
        self.clear_pending();
        true
    }

    fn clear_pending(&mut self) {
        self.pending = None;
        self.pending_seconds = 0.0;
        self.retry_delay = 0.25;
    }
}

fn select_dispel_item(automation: &mut dyn Automation) -> Option<InventoryItem> {
    let inventory = automation.owned_items_shared();
    if has_vulnerability_at_or_below(automation, 400)
        && let Some(item) = find(&inventory, &HIGH_DIFFICULTY_ITEMS)
    {
        return Some(item);
    }
    if has_vulnerability_at_or_below(automation, 350) {
        return find(&inventory, &NORMAL_DIFFICULTY_ITEMS);
    }
    None
}

fn find(inventory: &[InventoryItem], names: &[&str]) -> Option<InventoryItem> {
    for name in names {
        if let Some(item) = inventory
            .iter()
            .find(|item| item.stack_size > 0 && item.name == *name)
        {
            return Some(item.clone());
        }
    }
    None
}

fn select_awakener(
    automation: &mut dyn Automation,
    combat: &CombatSettings,
) -> Option<InventoryItem> {
    let creature = automation.try_get_skill(CREATURE_ENCHANTMENT_SKILL)?;
    let arcane = automation.try_get_skill(ARCANE_LORE_SKILL)?;
    let character = automation.character().id;
    let drum = automation
        .owned_items_shared()
        .iter()
        .find(|item| {
            matches!(item.name.as_str(), "Awakener" | "Attenuated Awakener")
                && combat.combat_item_names.contains(&item.name)
                && (item.container_id == character || item.wielder_id == character)
        })
        .cloned()?;
    let valid = if drum.name == "Awakener" {
        creature.training == Training::Specialized && arcane.current >= 110
    } else {
        matches!(creature.training, Training::Trained | Training::Specialized)
            && arcane.current >= 110
    };
    valid.then_some(drum)
}

fn select_ally(automation: &mut dyn Automation) -> Option<FellowMember> {
    let character = automation.character().id;
    let mut highest_score = 0;
    let mut selected: Option<FellowMember> = None;
    for member in automation.fellowship_members() {
        if member.object_id == character || member.distance > ALLY_DISPEL_RANGE_METERS {
            continue;
        }
        let tracked = automation.tracked_enchantments(member.object_id);
        if tracked.iter().any(|enchantment| {
            enchantment.spell_id == DISPEL_PROTECTION_SPELL && enchantment.seconds_remaining > 0.0
        }) {
            continue;
        }

        let mut qualities: HashMap<MonsterDamageType, i32> = HashMap::new();
        for enchantment in &tracked {
            if enchantment.seconds_remaining <= 0.0 || enchantment.is_untargeted {
                continue;
            }
            let Some(spell) = automation.spells().try_get(enchantment.spell_id) else {
                continue;
            };
            if spell.difficulty > 350 {
                continue;
            }
            let Some((identity, _)) = debuff::classify(&spell) else {
                continue;
            };
            if identity.flag != MonsterActionFlags::VULNERABILITY
                || identity.damage_type == MonsterDamageType::Auto
            {
                continue;
            }
            let quality = enchantment.quality;
            let entry = qualities.entry(identity.damage_type).or_insert(quality);
            if quality > *entry {
                *entry = quality;
            }
        }

        let score: i32 = qualities.values().filter(|&&quality| quality > 250).sum();
        if score <= highest_score {
            continue;
        }
        highest_score = score;
        selected = Some(member);
    }
    selected.filter(|member| member.object_id != 0)
}

fn has_vulnerability_at_or_below(automation: &mut dyn Automation, maximum_difficulty: i32) -> bool {
    for active in automation.active_enchantments() {
        if active.seconds_remaining < 0.0 {
            continue;
        }
        let Some(spell) = automation.spells().try_get(active.spell_id) else {
            continue;
        };
        if spell.difficulty > maximum_difficulty || spell.is_untargeted {
            continue;
        }
        if debuff::classify(&spell)
            .is_some_and(|(identity, _)| identity.flag == MonsterActionFlags::VULNERABILITY)
        {
            return true;
        }
    }
    false
}
