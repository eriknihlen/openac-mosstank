//! The buff rule (`BuffSelfRule`): on its turn of the macro it casts the
//! first self buff due within the rebuff window, else the first bane or
//! aura due on the character or a profiled item, else uses a buff
//! consumable; one cast at a time through the shared cast tracker, readied
//! by the combat-mode gate. A Force Buff marks everything as run out.
//!
//! The C#'s `IBuffRuleHost` (the panel) comes in per call as a
//! [`BuffRuleHost`]; the gate, the cast tracker and the cooldown table are
//! shared handles bound once.

use ac_plugin_abi::CharacterInfo;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::magic::{CastGate, CastResult, SpellInfo};
use mosstank_automation::Automation;
use mosstank_automation::cast_tracker::{
    CastRequest, SpellCastOutcome, SpellCastOutcomeInfo, SpellCastTracker,
};
use mosstank_automation::equipment::is_weapon_slot;
use mosstank_automation::locks::{
    ActionLockKind, ActionLockTable, SharedActionLocks, item_use_lock,
};
use mosstank_combat::MacroLogChannel;
use mosstank_combat::gate::{GateTurn, Prepare, SharedCombatModeGate};
use mosstank_dotnet::text::ordinal;
use mosstank_settings::model::{
    BuffItemEnchantRow, BuffSettings, CombatSettings, ConsumableCategory, VitalSettings,
};
use mosstank_stores::storage::PluginStorage;
use openac_plugin_compat::number::format_f64;

use crate::castability::{AutomationCastability, CastabilityReport};
use crate::due::BuffDueTracker;
use crate::element::{self, Element};
use crate::item_ledger::ItemEnchantLedger;
use crate::plan::{self, BuffTierRejection, BuildOptions};
use crate::profile::{BuffLine, BuffProfileCache, BuffTargetKind};
use crate::recharge_controller::SharedCastTracker;

/// The item enchantment skill, which the client reports as a spell's
/// school.
const ITEM_ENCHANTMENT_SCHOOL: u32 = 32;

/// What the rule asks of its owner (the C#'s `IBuffRuleHost`).
pub trait BuffRuleHost {
    /// Whether the macro runs.
    fn macro_enabled(&self) -> bool;
    /// The panel's status line.
    fn set_status(&mut self, status: &str);
    /// A line in chat.
    fn announce(&mut self, automation: &mut dyn Automation, text: &str);
    /// A macro log line (gated by `/vt log`).
    fn log(&mut self, channel: MacroLogChannel, message: &str);
    /// A macro log line that is never also put in chat.
    fn mirror_to_log(&mut self, channel: MacroLogChannel, message: &str);
    /// Remembers the selection a burst borrows.
    fn capture_selection(&mut self, automation: &mut dyn Automation);
    /// Puts the borrowed selection back.
    fn restore_selection(&mut self, automation: &mut dyn Automation);
    /// The owner's full stop, when the session is lost.
    fn stop_from_buff_rule(&mut self, automation: &mut dyn Automation, status: &str);
    /// FastCastBuffs' forward hold.
    fn begin_fast_cast(&mut self, automation: &mut dyn Automation, spell: &SpellInfo);
}

/// What one call works with.
#[allow(missing_debug_implementations)]
pub struct BuffTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The buff settings.
    pub settings: &'a BuffSettings,
    /// The combat settings (buff consumables, and the gate's item list).
    pub combat: &'a CombatSettings,
    /// The vital settings (the gate's peace retries).
    pub vitals: &'a VitalSettings,
    /// The owner.
    pub host: &'a mut dyn BuffRuleHost,
}

/// What the rule made of the tracker outcome it waited on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CastAttemptOutcome {
    /// Nothing yet.
    #[default]
    None,
    /// It landed.
    Success,
    /// The client refused it.
    Rejected,
    /// It fizzled or was resisted.
    Failed,
    /// Nothing came back.
    Timeout,
}

#[derive(Debug, Clone)]
struct BuffPick {
    spell: SpellInfo,
    target_object_id: u32,
    target_name: String,
    is_item_enchant: bool,
    consumable_object_id: u32,
}

#[derive(Debug, Clone, Default)]
struct ItemCast {
    item_id: u32,
    family: u32,
    quality: i32,
    duration: f64,
    spell_name: String,
}

/// The buff rule.
#[allow(missing_debug_implementations)]
pub struct BuffSelfRule {
    buff_due: BuffDueTracker,
    buff_profile: BuffProfileCache,
    /// The item rows' reference spells by name, for the known list.
    reference_memo: ReferenceMemo,
    item_ledger: ItemEnchantLedger,
    tier_trace_signatures: HashMap<u32, String>,
    now_seconds: f64,
    bursting: bool,
    buff_cast_recast_remaining: f64,
    cast_awaiting_spell_id: u32,
    cast_awaiting_item: ItemCast,
    cast_outcome_item: ItemCast,
    cast_outcome: CastAttemptOutcome,
    cast_outcome_spell_id: u32,
    decline_reason: String,
    item_missing_warnings: HashSet<String>,
    last_cast_refusal: Option<String>,
    timer_world: String,
    timer_character: String,
    // Consumables.
    consumable_locks: SharedActionLocks,
    consumable_retry_at: HashMap<u32, f64>,
    pending_consumable: u32,
    pending_consumable_spell: u32,
    pending_consumable_family: u32,
    consumable_family_retry_at: HashMap<u32, f64>,
    consumable_completion: i64,
    consumable_deadline: f64,
    consumables_bound: bool,
    gate: SharedCombatModeGate,
    cast_tracker: SharedCastTracker,
}

impl Default for BuffSelfRule {
    fn default() -> Self {
        Self::new()
    }
}

impl BuffSelfRule {
    /// An idle rule with a cast tracker and a cooldown table of its own.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buff_due: BuffDueTracker::new(),
            buff_profile: BuffProfileCache::new(),
            reference_memo: ReferenceMemo::default(),
            item_ledger: ItemEnchantLedger::new(),
            tier_trace_signatures: HashMap::new(),
            now_seconds: 0.0,
            bursting: false,
            buff_cast_recast_remaining: 0.0,
            cast_awaiting_spell_id: 0,
            cast_awaiting_item: ItemCast::default(),
            cast_outcome_item: ItemCast::default(),
            cast_outcome: CastAttemptOutcome::None,
            cast_outcome_spell_id: 0,
            decline_reason: String::new(),
            item_missing_warnings: HashSet::new(),
            last_cast_refusal: None,
            timer_world: String::new(),
            timer_character: String::new(),
            consumable_locks: ActionLockTable::shared(),
            consumable_retry_at: HashMap::new(),
            pending_consumable: 0,
            pending_consumable_spell: 0,
            pending_consumable_family: 0,
            consumable_family_retry_at: HashMap::new(),
            consumable_completion: 0,
            consumable_deadline: 0.0,
            consumables_bound: false,
            gate: std::rc::Rc::new(std::cell::RefCell::new(
                mosstank_combat::gate::CombatModeGate::new(),
            )),
            cast_tracker: std::rc::Rc::new(std::cell::RefCell::new(SpellCastTracker::new())),
        }
    }

    /// Shares the stance gate.
    pub fn bind_combat_mode_gate(&mut self, gate: SharedCombatModeGate) {
        self.gate = gate;
    }

    /// Shares the macro's cast tracker.
    pub fn bind_cast_tracker(&mut self, tracker: SharedCastTracker) {
        self.cast_tracker = tracker;
    }

    /// Shares the macro's cooldown table and turns the buff consumables on
    /// (`BindConsumables`).
    pub fn bind_consumables(&mut self, locks: SharedActionLocks) {
        self.consumable_locks = locks;
        self.consumables_bound = true;
    }

    /// Whether a buff burst holds the character.
    #[must_use]
    pub fn is_bursting(&self) -> bool {
        self.bursting
    }

    /// Why the last pass did not want to buff.
    #[must_use]
    pub fn decline_reason(&self) -> &str {
        &self.decline_reason
    }

    /// The item timers.
    #[must_use]
    pub fn item_ledger(&self) -> &ItemEnchantLedger {
        &self.item_ledger
    }

    /// The item timers, to set their clock or flush them.
    pub fn item_ledger_mut(&mut self) -> &mut ItemEnchantLedger {
        &mut self.item_ledger
    }

    /// The rule body, for both list positions.
    pub fn tick(&mut self, turn: &mut BuffTurn<'_>, can_act: bool, idle: bool) -> bool {
        self.tick_buff_rule(turn, can_act, idle)
    }

    /// Force Buff.
    pub fn start_force(&mut self, turn: &mut BuffTurn<'_>) {
        if !turn.automation.is_available() {
            turn.host.set_status("Not in world.");
            return;
        }
        let timed = turn.automation.timed_enchantments();
        self.buff_due.observe(&timed);
        self.buff_due.force_all();
        let rows = self.capture_forced_item_rows(turn);
        self.buff_due.force_items(rows);
        self.item_ledger.force_all(self.now_seconds);
        // The reference client's own wording.
        turn.host.announce(turn.automation, "Force buff enabled.");
    }

    /// Cancel Force Buff.
    pub fn cancel_force(&mut self, automation: &mut dyn Automation, host: &mut dyn BuffRuleHost) {
        self.buff_due.cancel_force();
        self.item_ledger.cancel_force();
        host.announce(automation, "Force buff canceled.");
    }

    /// Moves the rule's clocks on (every frame).
    pub fn advance(
        &mut self,
        automation: &mut dyn Automation,
        host: &mut dyn BuffRuleHost,
        elapsed_seconds: f64,
    ) {
        let elapsed = elapsed_seconds.max(0.0);
        self.buff_cast_recast_remaining = (self.buff_cast_recast_remaining - elapsed).max(0.0);
        self.now_seconds += elapsed;
        self.observe_consumable_use(automation, host);
        self.item_ledger.expire(self.now_seconds);
    }

    /// Drops the cast this rule waits on.
    pub fn clear_cast_attempt(&mut self) {
        self.cast_tracker.borrow_mut().reset();
        self.cast_awaiting_spell_id = 0;
        self.cast_awaiting_item = ItemCast::default();
        self.cast_outcome = CastAttemptOutcome::None;
        self.cast_outcome_spell_id = 0;
        self.cast_outcome_item = ItemCast::default();
    }

    /// The macro stopped.
    pub fn stop(&mut self) {
        self.clear_cast_attempt();
        self.bursting = false;
        self.buff_due.cancel_force();
        self.item_ledger.cancel_force();
    }

    /// Ends the burst only.
    pub fn stop_burst_only(&mut self) {
        self.clear_cast_attempt();
        self.bursting = false;
    }

    /// Session teardown: every scrap of state.
    pub fn reset(&mut self) {
        self.timer_world.clear();
        self.timer_character.clear();
        self.clear_consumable_use();
        self.consumable_retry_at.clear();
        self.consumable_family_retry_at.clear();
        self.bursting = false;
        self.buff_cast_recast_remaining = 0.0;
        self.now_seconds = 0.0;
        self.buff_due.reset();
        self.item_ledger.reset();
        self.item_missing_warnings.clear();
        self.tier_trace_signatures.clear();
        self.last_cast_refusal = None;
        self.cast_awaiting_spell_id = 0;
        self.cast_awaiting_item = ItemCast::default();
        self.cast_outcome = CastAttemptOutcome::None;
        self.cast_outcome_spell_id = 0;
        self.cast_outcome_item = ItemCast::default();
    }

    /// Lets the once-per-run warnings be said again.
    pub fn reset_once_per_run_warnings(&mut self) {
        self.item_missing_warnings.clear();
    }

    /// Captures the self buffs' lines ahead of the pass that will want
    /// them, when the character's list is not the one captured: a large
    /// spellbook's lines are a call's worth of work on their own. Whether
    /// it did that work. The pass then finds them captured, as it would
    /// have captured them itself.
    pub fn warm_profile(&mut self, automation: &mut dyn Automation) -> bool {
        let known = automation.spells().known_self_buffs_shared();
        if self.buff_profile.is_current(&known) {
            return false;
        }
        self.buff_profile.capture_shared(&known);
        true
    }

    /// The Meta's `needsbuff`: whether anything is due now.
    pub fn has_anything_due(
        &mut self,
        automation: &mut dyn Automation,
        settings: &BuffSettings,
    ) -> bool {
        let mut quiet = QuietReport;
        !self
            .build_plan(automation, settings, None, &mut quiet)
            .is_empty()
    }

    /// The tracker said how a cast ended.
    pub fn observe_cast_outcome(&mut self, info: &SpellCastOutcomeInfo) {
        if self.cast_awaiting_spell_id == 0 || info.spell_id != self.cast_awaiting_spell_id {
            return;
        }
        self.cast_awaiting_spell_id = 0;
        self.cast_outcome_spell_id = info.spell_id;
        self.cast_outcome_item = std::mem::take(&mut self.cast_awaiting_item);
        self.cast_outcome = match info.outcome {
            SpellCastOutcome::Success | SpellCastOutcome::Kill => CastAttemptOutcome::Success,
            SpellCastOutcome::Rejected => CastAttemptOutcome::Rejected,
            SpellCastOutcome::Fail | SpellCastOutcome::PermanentFail => CastAttemptOutcome::Failed,
            _ => CastAttemptOutcome::Timeout,
        };
    }

    /// Drops the BuffCastRecast lock outright.
    pub fn clear_recast_lock(&mut self) {
        self.buff_cast_recast_remaining = 0.0;
    }

    /// Binds the item timers to the character's server and name once they
    /// are known (and again when they change).
    pub fn ensure_timer_persistence(
        &mut self,
        automation: &mut dyn Automation,
        storage: &mut dyn PluginStorage,
        host: &mut dyn BuffRuleHost,
    ) {
        let available = automation.is_available();
        let character = if available {
            automation.character()
        } else {
            CharacterInfo::default()
        };
        self.ensure_timer_persistence_with(available, &character, storage, host);
    }

    /// [`Self::ensure_timer_persistence`] with the client's answers read
    /// already, for an owner whose storage and client are one borrow.
    pub fn ensure_timer_persistence_with(
        &mut self,
        available: bool,
        character: &CharacterInfo,
        storage: &mut dyn PluginStorage,
        host: &mut dyn BuffRuleHost,
    ) {
        if !available {
            return;
        }
        let world = character.world.trim();
        let name = character.name.trim();
        if world.is_empty()
            || name.is_empty()
            || !storage.is_available()
            || (ordinal::equals_ignore_case(world, &self.timer_world)
                && ordinal::equals_ignore_case(name, &self.timer_character))
        {
            return;
        }
        let mut lines: Vec<String> = Vec::new();
        self.item_ledger.bind_persistence(
            storage,
            Some(world),
            Some(name),
            self.now_seconds,
            &mut |line| lines.push(line.to_owned()),
        );
        for line in lines {
            host.log(MacroLogChannel::Misc, &line);
        }
        world.clone_into(&mut self.timer_world);
        name.clone_into(&mut self.timer_character);
    }

    /// Death, a dispel: every item timer is forgotten.
    pub fn invalidate_item_timers(&mut self, automation: &mut dyn Automation) {
        self.clear_cast_attempt();
        self.clear_consumable_use();
        self.item_ledger.invalidate_all();
        automation.forget_enchantments(0);
        self.buff_due.cancel_force();
    }

    fn capture_forced_item_rows(&mut self, turn: &mut BuffTurn<'_>) -> Vec<(u32, u32)> {
        let mut rows = Vec::new();
        let items_available = turn.automation.items_available();
        let owned: std::rc::Rc<[InventoryItem]> = if items_available {
            turn.automation.owned_items_shared()
        } else {
            std::rc::Rc::from(Vec::new())
        };
        let character_id = turn.automation.character().id;
        let names = self.character_enchant_names(turn.settings);
        let mut report = RuleReport::new(&mut *turn.host);
        {
            let mut castability = AutomationCastability::new(
                &mut *turn.automation,
                Rc::clone(&owned),
                &turn.settings.blacklisted_spell_components,
                &mut report,
            );
            if character_id != 0 {
                for tier_one_name in &names {
                    if let Some(bane) = try_resolve_best_known_by_name(
                        &mut castability,
                        tier_one_name,
                        turn.settings,
                    ) {
                        rows.push((character_id, bane.family));
                    }
                }
            }
            if items_available {
                for row in &turn.settings.item_enchant_rows {
                    if row.casts_nothing() {
                        continue;
                    }
                    let Some(item) = try_resolve_item_target(&owned, row) else {
                        continue;
                    };
                    let spell = match row.spell_id {
                        Some(spell_id) => {
                            castability.automation_spell(spell_id).and_then(|exemplar| {
                                try_resolve_best_known(&mut castability, &exemplar, turn.settings)
                            })
                        }
                        None => try_resolve_best_known_by_name_memo(
                            &mut self.reference_memo,
                            &mut castability,
                            &row.spell_name,
                            turn.settings,
                        ),
                    };
                    if let Some(spell) = spell
                        && !spell.is_untargeted
                    {
                        rows.push((item.object_id, spell.family));
                    }
                }
            }
        }
        report.finish(self, &mut *turn.automation);
        rows
    }

    fn build_plan(
        &mut self,
        automation: &mut dyn Automation,
        settings: &BuffSettings,
        rebuff_when_under_seconds: Option<f64>,
        report: &mut dyn CastabilityReport,
    ) -> Vec<SpellInfo> {
        let mut untargeted_profile_spells: Vec<u32> = Vec::new();
        for row in &settings.item_enchant_rows {
            if !row.is_profiled_item_row() || row.casts_nothing() {
                continue;
            }
            let spell_id = row.spell_id.unwrap_or_default();
            if automation
                .spells()
                .try_get(spell_id)
                .is_some_and(|spell| spell.is_untargeted)
            {
                untargeted_profile_spells.push(spell_id);
            }
        }
        let known = automation.spells().known_self_buffs_shared();
        let lines = self.buff_profile.capture_shared(&known);
        let skills = automation.skills();
        let attributes = automation.attributes();
        let timed = automation.timed_enchantments();
        let level = automation.character().level;
        let owned: std::rc::Rc<[InventoryItem]> = if automation.items_available() {
            automation.owned_items_shared()
        } else {
            std::rc::Rc::from(Vec::new())
        };
        let forced = self.buff_due.forced_spell_ids().clone();
        let mut castability = AutomationCastability::new(
            automation,
            owned,
            &settings.blacklisted_spell_components,
            report,
        );
        plan::build(
            &lines,
            &skills,
            &attributes,
            &timed,
            settings,
            BuildOptions {
                force: false,
                rebuff_when_under_seconds,
                character_level: level,
                forced_spell_ids: Some(&forced),
                castability: Some(&mut castability),
                spell_catalog: None,
                additional_extra_spell_ids: Some(&untargeted_profile_spells),
            },
        )
    }

    fn consume_cast_outcome(&mut self, turn: &mut BuffTurn<'_>) {
        let outcome = std::mem::take(&mut self.cast_outcome);
        let spell_id = std::mem::take(&mut self.cast_outcome_spell_id);
        let item = std::mem::take(&mut self.cast_outcome_item);
        if outcome != CastAttemptOutcome::Success {
            return;
        }
        self.buff_due.note_recast(spell_id);
        if item.item_id == 0 {
            return;
        }
        self.buff_due.note_item_recast(item.item_id, item.family);
        let mut lines: Vec<String> = Vec::new();
        let mut log = |text: &str| lines.push(text.to_owned());
        let owned = turn
            .automation
            .owned_items_shared()
            .iter()
            .find(|owned| owned.object_id == item.item_id)
            .cloned();
        match owned {
            Some(owned) if owned.object_id != 0 => self.item_ledger.note_item_cast(
                &owned,
                item.family,
                spell_id,
                item.quality,
                item.duration,
                self.now_seconds,
                &item.spell_name,
                &mut log,
            ),
            _ => self.item_ledger.note_cast(
                item.item_id,
                item.family,
                spell_id,
                item.quality,
                item.duration,
                self.now_seconds,
                &item.spell_name,
                &mut log,
            ),
        }
        for line in lines {
            turn.host.log(MacroLogChannel::Misc, &line);
        }
    }

    fn tick_buff_rule(&mut self, turn: &mut BuffTurn<'_>, can_act: bool, idle: bool) -> bool {
        if !turn.automation.is_available() {
            if self.bursting {
                turn.host
                    .stop_from_buff_rule(turn.automation, "Lost the session.");
            }
            "there is no session to buff in".clone_into(&mut self.decline_reason);
            return false;
        }

        // Fold the world into the tracked table before anything reads it.
        let timed = turn.automation.timed_enchantments();
        self.buff_due.observe(&timed);
        self.consume_cast_outcome(turn);

        if self.pending_consumable != 0 {
            return true;
        }

        // The rule gates on the item-use slot, never on the client's
        // inventory transaction state.
        if self
            .consumable_locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
        {
            "the item slot is held".clone_into(&mut self.decline_reason);
            return self.pause_burst(turn);
        }

        if !turn.settings.enabled {
            "EnableBuffing is off".clone_into(&mut self.decline_reason);
            return self.pause_burst(turn);
        }

        if !turn.host.macro_enabled() {
            "the macro is not running".clone_into(&mut self.decline_reason);
            return self.end_burst_at(turn, idle);
        }

        let mut threshold = if idle {
            turn.settings.idle_buff_topoff_seconds
        } else {
            turn.settings.rebuff_when_under_seconds
        };
        if self.buff_cast_recast_remaining > 0.0 {
            threshold += turn.settings.buff_cast_recast_seconds;
        }
        // The reference client's floor on the rebuff window.
        if threshold < 10.0 {
            threshold = 10.0;
        }

        let Some(pick) = self.try_pick_buff(turn, threshold) else {
            let within = format_f64(threshold, "0.#").unwrap_or_default();
            let known = turn.automation.spells().known_self_buffs_shared().len();
            let carried = if turn.automation.items_available() {
                format!(
                    "{} items carried",
                    turn.automation.owned_items_shared().len()
                )
            } else {
                "the item surface is unavailable, so no component or consumable can be counted"
                    .to_owned()
            };
            self.decline_reason = format!(
                "nothing is due within {within}s that this character can cast \
                 ({known} self buffs known, {carried})"
            );
            return self.end_burst_at(turn, idle);
        };

        self.decline_reason.clear();

        if !self.bursting {
            self.bursting = true;
            turn.host.capture_selection(turn.automation);
        }

        if !can_act {
            return true;
        }

        // The macro's cast tracker owns this wait.
        if self.cast_tracker.borrow().is_busy() {
            return true;
        }

        if pick.consumable_object_id != 0 {
            return self.use_buff_consumable(turn, &pick);
        }

        // The owner's gate is always there (the C#'s `_owner.Gate`): a buff is
        // cast only from magic mode.
        {
            let gate = self.gate.clone();
            let mut gate = gate.borrow_mut();
            let mut gate_turn = GateTurn {
                automation: &mut *turn.automation,
                combat: turn.combat,
                vitals: turn.vitals,
            };
            if !gate.try_prepare(&mut gate_turn, Prepare::mode(CombatMode::Magic), None) {
                turn.host.set_status(gate.status());
                return true;
            }
        }

        // Arm the buff-cast-recast cooldown for its reset window.
        self.buff_cast_recast_remaining = turn.settings.buff_cast_recast_reset_seconds.max(0.0);

        self.try_cast(turn, &pick, "Buffing");
        true
    }

    fn try_pick_buff(&mut self, turn: &mut BuffTurn<'_>, threshold: f64) -> Option<BuffPick> {
        let mut report = RuleReport::new(&mut *turn.host);
        let due = self.build_plan(
            &mut *turn.automation,
            turn.settings,
            Some(threshold),
            &mut report,
        );
        report.finish(self, &mut *turn.automation);
        if let Some(first) = due.into_iter().next() {
            return Some(BuffPick {
                spell: first,
                target_object_id: turn.automation.character().id,
                target_name: "yourself".to_owned(),
                is_item_enchant: false,
                consumable_object_id: 0,
            });
        }
        self.try_pick_item_enchant(turn, threshold)
            .or_else(|| self.try_pick_consumable(turn, threshold))
    }

    fn try_pick_item_enchant(
        &mut self,
        turn: &mut BuffTurn<'_>,
        threshold: f64,
    ) -> Option<BuffPick> {
        let skills = turn.automation.skills();
        let level = turn.automation.character().level;
        if !plan::is_school_available(ITEM_ENCHANTMENT_SCHOOL, &skills, turn.settings, level) {
            return None;
        }
        let owned: std::rc::Rc<[InventoryItem]> = if turn.automation.items_available() {
            turn.automation.owned_items_shared()
        } else {
            std::rc::Rc::from(Vec::new())
        };

        // The self buffs are read once for all the rows below: nothing a
        // row does in between changes them.
        let mut known: Option<Rc<[SpellInfo]>> = None;
        if let Some(pick) = self.try_pick_character_enchant(turn, &owned, threshold, &mut known) {
            return Some(pick);
        }

        if !turn.settings.buff_auras {
            return None;
        }
        if turn.settings.item_enchant_rows.is_empty() || owned.is_empty() {
            return None;
        }
        for row in &turn.settings.item_enchant_rows {
            if row.casts_nothing() {
                continue;
            }
            let Some(item) = try_resolve_item_target(&owned, row) else {
                let name = match row.object_id {
                    Some(u32::MAX) => "equipped weapon".to_owned(),
                    Some(object_id) => object_id.to_string(),
                    None => row.item_name.clone(),
                };
                self.post_item_missing_warning_once(turn, &name);
                continue;
            };
            let spell = {
                let mut report = RuleReport::new(&mut *turn.host);
                let resolved = {
                    let mut castability = AutomationCastability::new(
                        &mut *turn.automation,
                        Rc::clone(&owned),
                        &turn.settings.blacklisted_spell_components,
                        &mut report,
                    );
                    if let Some(known) = &known {
                        castability = castability.with_known(Rc::clone(known));
                    }
                    let resolved = match row.spell_id {
                        Some(spell_id) => {
                            castability.automation_spell(spell_id).and_then(|exemplar| {
                                try_resolve_best_known(&mut castability, &exemplar, turn.settings)
                            })
                        }
                        None => try_resolve_best_known_by_name(
                            &mut castability,
                            &row.spell_name,
                            turn.settings,
                        ),
                    };
                    if known.is_none() {
                        known = castability.known_if_read();
                    }
                    resolved
                };
                report.finish(self, &mut *turn.automation);
                resolved
            };
            let Some(spell) = spell else {
                continue;
            };
            if spell.is_untargeted {
                continue;
            }
            // A force pass zeroes every item entry's stamp too.
            let remaining = if self.buff_due.is_item_forced(item.object_id, spell.family) {
                0.0
            } else {
                self.item_enchant_remaining_seconds(turn.automation, item.object_id, &spell)
            };
            if remaining >= threshold {
                continue;
            }
            self.log_row_pick(
                turn.host,
                "item row",
                &item.name,
                item.object_id,
                &spell,
                remaining,
                threshold,
            );
            return Some(BuffPick {
                spell,
                target_object_id: item.object_id,
                target_name: item.name.clone(),
                is_item_enchant: true,
                consumable_object_id: 0,
            });
        }
        None
    }

    fn try_pick_character_enchant(
        &mut self,
        turn: &mut BuffTurn<'_>,
        owned: &Rc<[InventoryItem]>,
        threshold: f64,
        known: &mut Option<Rc<[SpellInfo]>>,
    ) -> Option<BuffPick> {
        let character = turn.automation.character();
        if character.id == 0 || !turn.settings.buff_banes {
            return None;
        }
        for tier_one_name in self.character_enchant_names(turn.settings) {
            let spell = {
                let mut report = RuleReport::new(&mut *turn.host);
                let resolved = {
                    let mut castability = AutomationCastability::new(
                        &mut *turn.automation,
                        Rc::clone(owned),
                        &turn.settings.blacklisted_spell_components,
                        &mut report,
                    );
                    if let Some(known) = known.as_ref() {
                        castability = castability.with_known(Rc::clone(known));
                    }
                    let resolved = try_resolve_best_known_by_name_memo(
                        &mut self.reference_memo,
                        &mut castability,
                        &tier_one_name,
                        turn.settings,
                    );
                    if known.is_none() {
                        *known = castability.known_if_read();
                    }
                    resolved
                };
                report.finish(self, &mut *turn.automation);
                resolved
            };
            let Some(spell) = spell else {
                continue;
            };
            let remaining = if self.buff_due.is_item_forced(character.id, spell.family) {
                0.0
            } else {
                self.item_enchant_remaining_seconds(turn.automation, character.id, &spell)
            };
            // Due when what is left is under the window.
            if remaining >= threshold {
                continue;
            }
            self.log_row_pick(
                turn.host,
                "character row",
                &character.name,
                character.id,
                &spell,
                remaining,
                threshold,
            );
            return Some(BuffPick {
                spell,
                target_object_id: character.id,
                target_name: "yourself".to_owned(),
                is_item_enchant: true,
                consumable_object_id: 0,
            });
        }
        None
    }

    fn character_enchant_names(&self, settings: &BuffSettings) -> Vec<String> {
        let mut names = vec![element::bane_spell_name(Element::Physical).to_owned()];
        for element in element::banes(settings.bane_profile_mode, Some(&settings.bane_elements)) {
            names.push(element::bane_spell_name(element).to_owned());
        }
        names
    }

    #[allow(clippy::too_many_arguments)]
    fn log_row_pick(
        &self,
        host: &mut dyn BuffRuleHost,
        kind: &str,
        target_name: &str,
        target_object_id: u32,
        spell: &SpellInfo,
        remaining: f64,
        threshold: f64,
    ) {
        host.log(
            MacroLogChannel::Misc,
            &format!(
                "Buffing: {kind} {target_name} ({target_object_id}) \u{2192} {} [family {}], \
                 covered for {}s of {}s",
                spell.name,
                spell.family,
                format_f64(remaining, "0.#").unwrap_or_default(),
                format_f64(threshold, "0.#").unwrap_or_default(),
            ),
        );
    }

    fn item_enchant_remaining_seconds(
        &mut self,
        automation: &mut dyn Automation,
        item_object_id: u32,
        spell: &SpellInfo,
    ) -> f64 {
        let mut longest: f64 = 0.0;
        // Self-targeted auras are carried by the character even when their
        // row belongs to a weapon.
        if spell.is_self_targeted {
            for held in automation.timed_enchantments() {
                if held.family == spell.family && held.tier >= spell.tier {
                    longest = longest.max(held.seconds_remaining);
                }
            }
            return longest;
        }
        let owned = automation
            .owned_items_shared()
            .iter()
            .find(|item| item.object_id == item_object_id)
            .cloned();
        longest =
            match owned {
                Some(item) if item.object_id != 0 => self.item_ledger.remaining_item_seconds(
                    &item,
                    spell.family,
                    spell.tier,
                    self.now_seconds,
                ),
                _ if item_object_id == automation.character().id => self
                    .item_ledger
                    .remaining_seconds(item_object_id, spell.family, spell.tier, self.now_seconds),
                _ => 0.0,
            };
        for held in automation.tracked_enchantments(item_object_id) {
            if held.family != spell.family || held.quality < spell.difficulty {
                continue;
            }
            if held.seconds_remaining > longest {
                longest = held.seconds_remaining;
            }
        }
        longest
    }

    fn post_item_missing_warning_once(&mut self, turn: &mut BuffTurn<'_>, item_name: &str) {
        let text = format!(
            "Warning: item {item_name} is in profile but not found in inventory. Skipping buffs for item."
        );
        warn_once(
            &mut self.item_missing_warnings,
            turn.automation,
            turn.host,
            &text,
        );
    }

    fn end_burst_at(&mut self, turn: &mut BuffTurn<'_>, idle: bool) -> bool {
        if idle || !turn.settings.idle_buff_topoff {
            self.end_burst(turn)
        } else {
            self.pause_burst(turn)
        }
    }

    fn pause_burst(&mut self, turn: &mut BuffTurn<'_>) -> bool {
        if !self.bursting {
            return false;
        }
        self.bursting = false;
        turn.host.restore_selection(turn.automation);
        false
    }

    fn end_burst(&mut self, turn: &mut BuffTurn<'_>) -> bool {
        if !self.bursting {
            return false;
        }
        self.bursting = false;
        turn.host.restore_selection(turn.automation);
        turn.host.set_status("Idle.");
        false
    }

    fn note_cast_refused(&mut self, host: &mut dyn BuffRuleHost, spell: &SpellInfo, reason: &str) {
        let text = format!("SpellCaster: {} not issued \u{2014} {reason}", spell.name);
        if self.last_cast_refusal.as_deref() == Some(text.as_str()) {
            return;
        }
        host.log(MacroLogChannel::CastInfo, &text);
        self.last_cast_refusal = Some(text);
    }

    fn try_cast(&mut self, turn: &mut BuffTurn<'_>, pick: &BuffPick, label: &str) -> bool {
        let spell = &pick.spell;
        if pick.is_item_enchant || !spell.is_self_targeted {
            let target = pick.target_object_id;
            if target == 0 {
                turn.host.set_status(&format!("{}: no target", spell.name));
                self.note_cast_refused(turn.host, spell, "no target");
                return false;
            }
            if turn.automation.selected_object() != Some(target) {
                turn.automation.select(target);
            }
        }

        let gate = turn.automation.evaluate_gate(spell.spell_id, None);
        if gate != CastGate::Ready {
            turn.host.set_status(&format!("{}: {gate:?}", spell.name));
            self.note_cast_refused(
                turn.host,
                spell,
                &format!("{gate:?} on {}", pick.target_name),
            );
            return false;
        }

        let request = turn.automation.request_cast(spell.spell_id, None);
        if request != CastResult::Sent {
            turn.host.set_status(&format!("Refused {}.", spell.name));
            self.note_cast_refused(turn.host, spell, &format!("{request:?}"));
            return false;
        }
        self.last_cast_refusal = None;

        turn.host.log(
            MacroLogChannel::SpellCast,
            &format!(
                "Casting: {} on {} ({})",
                spell.name, pick.target_object_id, pick.target_name
            ),
        );

        turn.host.begin_fast_cast(turn.automation, spell);
        let mana = turn.automation.character().mana.0;
        let target_name = if spell.school == ITEM_ENCHANTMENT_SCHOOL {
            ""
        } else {
            pick.target_name.as_str()
        };
        self.cast_tracker.borrow_mut().begin(&CastRequest {
            spell_id: spell.spell_id,
            spell_name: &spell.name,
            target_object_id: pick.target_object_id,
            target_name,
            hits_multiple_targets: SpellCastTracker::hits_multiple_targets_for(spell),
            saying: &spell.saying,
            school: spell.school,
            can_kill: SpellCastTracker::can_kill_for(spell),
            current_mana: i32::try_from(mana).unwrap_or(i32::MAX),
            target_incarnation: 0,
        });
        turn.host
            .log(MacroLogChannel::CastInfo, "SpellCaster: Begin");
        self.cast_awaiting_spell_id = spell.spell_id;
        // What the item ledger will need when the result arrives.
        self.cast_awaiting_item = if pick.is_item_enchant {
            ItemCast {
                item_id: pick.target_object_id,
                family: spell.family,
                quality: spell.tier,
                duration: f64::from(spell.duration_seconds),
                spell_name: spell.name.clone(),
            }
        } else {
            ItemCast::default()
        };
        self.cast_outcome = CastAttemptOutcome::None;
        turn.host.set_status(&format!("{label}: {}", spell.name));
        turn.automation.log_info(&format!(
            "MossTank: casting {} (0x{:04X})",
            spell.name, spell.spell_id
        ));
        true
    }

    // --- Buff consumables (`BuffSelfRule.Consumables`).

    fn try_pick_consumable(&mut self, turn: &mut BuffTurn<'_>, threshold: f64) -> Option<BuffPick> {
        if !self.consumables_bound
            || !turn.automation.items_available()
            || self
                .consumable_locks
                .borrow_mut()
                .is_locked(ActionLockKind::ItemUse)
        {
            return None;
        }
        let automation = &mut *turn.automation;
        let combat = turn.combat;
        for item in automation.owned_items_shared().iter() {
            let gem = turn
                .settings
                .gem_food_items
                .iter()
                .find(|entry| entry.name == item.name);
            let configured_gem = gem.is_some();
            let configured_spell_id = gem.map_or(0, |gem| gem.spell_id);
            let configured_consumable = combat.consumable_names.contains(&item.name)
                && combat.consumable_categories.get(&item.name)
                    == Some(&ConsumableCategory::BuffConsumable);
            let spell_id = if configured_gem {
                configured_spell_id
            } else {
                item.appraised_spell_ids.first().copied().unwrap_or(0)
            };
            if !configured_gem && !configured_consumable {
                continue;
            }
            if self
                .consumable_retry_at
                .get(&item.object_id)
                .is_some_and(|&retry_at| retry_at > self.now_seconds)
            {
                continue;
            }
            if automation
                .object(item.object_id)
                .is_none_or(|world| world.last_id_time == 0)
                || spell_id == 0
            {
                continue;
            }
            let Some(spell) = automation.spells().try_get(spell_id) else {
                continue;
            };
            if self
                .consumable_family_retry_at
                .get(&spell.family)
                .is_some_and(|&retry_at| retry_at > self.now_seconds)
            {
                continue;
            }
            if spell.is_fellowship && !automation.in_fellowship() {
                continue;
            }
            let Some(properties) = automation.item_properties(item.object_id) else {
                continue;
            };
            if item.use_requires_skill > 0 {
                #[allow(clippy::cast_sign_loss)]
                let skill = automation.try_get_skill(item.use_requires_skill as u32);
                let refused = match skill {
                    None => true,
                    Some(skill) => {
                        matches!(skill.training, Training::Unknown | Training::Untrained)
                            || i64::from(skill.current) < i64::from(item.use_requires_skill_level)
                            || (item.use_requires_skill_specialized != 0
                                && skill.training != Training::Specialized)
                    }
                };
                if refused {
                    continue;
                }
            }
            if let Some(&(_, cooldown)) = properties.ints.iter().find(|(key, _)| *key == 280)
                && cooldown > 0
                && automation
                    .spells()
                    .cooldown_remaining(cooldown.unsigned_abs())
                    > 0.0
            {
                continue;
            }
            let forced = self.buff_due.forced_spell_ids();
            if automation.timed_enchantments().iter().any(|effect| {
                effect.family == spell.family
                    && effect.tier >= spell.tier
                    && !forced.contains(&effect.spell_id)
                    && effect.seconds_remaining >= threshold
            }) {
                continue;
            }
            return Some(BuffPick {
                spell,
                target_object_id: automation.character().id,
                target_name: item.name.clone(),
                is_item_enchant: false,
                consumable_object_id: item.object_id,
            });
        }
        None
    }

    fn use_buff_consumable(&mut self, turn: &mut BuffTurn<'_>, pick: &BuffPick) -> bool {
        if self
            .consumable_locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
        {
            return true;
        }
        let revision = turn.automation.last_item_completion().revision;
        let result = turn.automation.use_item(pick.consumable_object_id);
        turn.host.set_status(&format!(
            "Using {} for {}",
            pick.target_name, pick.spell.name
        ));
        if !result.accepted() {
            self.consumable_retry_at
                .insert(pick.consumable_object_id, self.now_seconds + 5.0);
            return result.status == ItemStatus::Busy;
        }
        self.pending_consumable = pick.consumable_object_id;
        self.pending_consumable_spell = pick.spell.spell_id;
        self.pending_consumable_family = pick.spell.family;
        self.consumable_completion = revision;
        self.consumable_deadline = self.now_seconds + 15.0;
        self.consumable_locks
            .borrow_mut()
            .arm(ActionLockKind::ItemUse, item_use_lock::TRANSACTION_SECONDS);
        true
    }

    fn observe_consumable_use(
        &mut self,
        automation: &mut dyn Automation,
        host: &mut dyn BuffRuleHost,
    ) {
        if self.pending_consumable == 0 {
            return;
        }
        let completion = automation.last_item_completion();
        let completed = completion.revision > self.consumable_completion
            && completion.source_id == self.pending_consumable;
        if !completed && self.now_seconds < self.consumable_deadline {
            self.consumable_locks
                .borrow_mut()
                .arm(ActionLockKind::ItemUse, item_use_lock::TRANSACTION_SECONDS);
            return;
        }
        let success = completed && completion.revision != 0 && completion.weenie_error == 0;
        // Allow the effect update to arrive after the item-use answer.
        let retry_at = self.now_seconds + if success { 5.0 } else { 30.0 };
        self.consumable_retry_at
            .insert(self.pending_consumable, retry_at);
        self.consumable_family_retry_at
            .insert(self.pending_consumable_family, retry_at);
        if success {
            self.buff_due.note_recast(self.pending_consumable_spell);
        } else {
            host.log(
                MacroLogChannel::Misc,
                "Buff consumable failed or timed out; delaying retry.",
            );
        }
        self.clear_consumable_use();
    }

    fn clear_consumable_use(&mut self) {
        if self.pending_consumable != 0 {
            self.consumable_locks
                .borrow_mut()
                .release(ActionLockKind::ItemUse);
        }
        self.pending_consumable = 0;
        self.pending_consumable_spell = 0;
        self.pending_consumable_family = 0;
        self.consumable_completion = 0;
        self.consumable_deadline = 0.0;
    }

    /// The tier trace (`NoteTierPick`): said once per family until the pick
    /// or the rejections change.
    fn note_tier_pick(
        &mut self,
        host: &mut dyn BuffRuleHost,
        line: &BuffLine,
        pick: Option<&SpellInfo>,
        rejections: &[BuffTierRejection],
    ) {
        let Some(highest) = line.tiers.first() else {
            return;
        };
        if let Some(picked) = pick
            && picked.tier >= highest.tier
        {
            return;
        }
        let floor = pick.map_or(i32::MIN, |spell| spell.tier);
        let higher: Vec<&BuffTierRejection> = rejections
            .iter()
            .filter(|rejection| rejection.spell.tier > floor)
            .collect();

        let mut signature =
            pick.map_or_else(|| "none".to_owned(), |spell| spell.spell_id.to_string());
        for rejection in &higher {
            signature.push_str(&format!(
                "|{}:{}",
                rejection.spell.spell_id, rejection.reason
            ));
        }
        if self.tier_trace_signatures.get(&line.family) == Some(&signature) {
            return;
        }
        self.tier_trace_signatures.insert(line.family, signature);

        let picked_text = pick.map_or_else(
            || "picked nothing".to_owned(),
            |spell| format!("picked {} (gen {})", spell.name, spell.tier),
        );
        let rejected_text = if higher.is_empty() {
            "none".to_owned()
        } else {
            higher
                .iter()
                .map(|rejection| format!("{} {}", rejection.spell.name, rejection.reason))
                .collect::<Vec<_>>()
                .join(", ")
        };
        host.log(
            MacroLogChannel::Timers,
            &format!(
                "Buffing: {} \u{2014} {picked_text}; rejected: {rejected_text}",
                line.reference().name
            ),
        );
    }
}

/// Says a warning once per run, in chat and in the log.
fn warn_once(
    warnings: &mut HashSet<String>,
    automation: &mut dyn Automation,
    host: &mut dyn BuffRuleHost,
    text: &str,
) {
    if !warnings.insert(text.to_owned()) {
        return;
    }
    host.announce(automation, text);
    host.mirror_to_log(MacroLogChannel::Misc, text);
}

/// What a castability walk reports, kept in order while the client is
/// borrowed and said afterwards (the C#'s `WarnOnce` and `NoteTierPick`
/// callbacks, which run inside the walk).
struct RuleReport<'h> {
    host: &'h mut dyn BuffRuleHost,
    events: Vec<ReportEvent>,
}

enum ReportEvent {
    Warn(String),
    Pick(Box<(BuffLine, Option<SpellInfo>, Vec<BuffTierRejection>)>),
}

impl<'h> RuleReport<'h> {
    fn new(host: &'h mut dyn BuffRuleHost) -> Self {
        Self {
            host,
            events: Vec::new(),
        }
    }

    fn finish(self, rule: &mut BuffSelfRule, automation: &mut dyn Automation) {
        let RuleReport { host, events } = self;
        for event in events {
            match event {
                ReportEvent::Warn(text) => {
                    warn_once(&mut rule.item_missing_warnings, automation, host, &text);
                }
                ReportEvent::Pick(boxed) => {
                    let (line, pick, rejections) = *boxed;
                    rule.note_tier_pick(host, &line, pick.as_ref(), &rejections);
                }
            }
        }
    }
}

impl CastabilityReport for RuleReport<'_> {
    fn warn(&mut self, text: &str) {
        self.events.push(ReportEvent::Warn(text.to_owned()));
    }

    fn note_tier_pick(
        &mut self,
        line: &BuffLine,
        pick: Option<&SpellInfo>,
        rejections: &[BuffTierRejection],
    ) {
        self.events.push(ReportEvent::Pick(Box::new((
            line.clone(),
            pick.cloned(),
            rejections.to_vec(),
        ))));
    }
}

/// A report that says nothing (the Meta's question).
struct QuietReport;

impl CastabilityReport for QuietReport {
    fn warn(&mut self, _text: &str) {}

    fn note_tier_pick(
        &mut self,
        _line: &BuffLine,
        _pick: Option<&SpellInfo>,
        _rejections: &[BuffTierRejection],
    ) {
    }
}

fn try_resolve_item_target(
    owned: &[InventoryItem],
    row: &BuffItemEnchantRow,
) -> Option<InventoryItem> {
    match row.object_id {
        Some(u32::MAX) => owned
            .iter()
            .find(|candidate| is_weapon_slot(candidate.equipped_location))
            .cloned(),
        Some(object_id) => owned
            .iter()
            .find(|candidate| candidate.object_id == object_id)
            .cloned(),
        None => owned
            .iter()
            .find(|candidate| candidate.name == row.item_name)
            .cloned(),
    }
}

/// Drops the trailing tier numeral from a tier-I name.
fn stem_of(tier_one_name: &str) -> &str {
    tier_one_name.strip_suffix(" I").unwrap_or(tier_one_name)
}

/// The best known tier of the family a tier-I name names
/// (`TryResolveBestKnown` by name).
pub(crate) fn try_resolve_best_known_by_name(
    castability: &mut AutomationCastability<'_>,
    tier_one_name: &str,
    settings: &BuffSettings,
) -> Option<SpellInfo> {
    let known = castability.known_self_buffs();
    let reference = reference_by_name(&known, tier_one_name)?;
    try_resolve_best_known(castability, &reference, settings)
}

/// [`try_resolve_best_known_by_name`] with the row's spell looked up
/// through `memo`: the lookup depends on the known list and the name
/// alone, and a large spellbook makes it costly for every row on every pass.
fn try_resolve_best_known_by_name_memo(
    memo: &mut ReferenceMemo,
    castability: &mut AutomationCastability<'_>,
    tier_one_name: &str,
    settings: &BuffSettings,
) -> Option<SpellInfo> {
    let known = castability.known_self_buffs();
    let reference = memo.reference(&known, tier_one_name)?;
    try_resolve_best_known(castability, &reference, settings)
}

/// The rows' reference spells by name, for one known list (the same
/// allocation: a shared list never changes).
#[derive(Debug, Default)]
struct ReferenceMemo {
    known: Option<Rc<[SpellInfo]>>,
    by_name: HashMap<String, Option<SpellInfo>>,
}

impl ReferenceMemo {
    fn reference(&mut self, known: &Rc<[SpellInfo]>, tier_one_name: &str) -> Option<SpellInfo> {
        if !self
            .known
            .as_ref()
            .is_some_and(|kept| Rc::ptr_eq(kept, known))
        {
            self.known = Some(Rc::clone(known));
            self.by_name.clear();
        }
        self.by_name
            .entry(tier_one_name.to_owned())
            .or_insert_with(|| reference_by_name(known, tier_one_name))
            .clone()
    }
}

/// A row's reference spell: its own tier-I spell by its exact name, else
/// the lowest tier (then id) of the known spells its stem starts.
fn reference_by_name(known: &[SpellInfo], tier_one_name: &str) -> Option<SpellInfo> {
    let stem = stem_of(tier_one_name);
    // The row's own tier-I spell, by its exact name.
    let mut reference = known
        .iter()
        .find(|candidate| ordinal::equals_ignore_case(&candidate.name, tier_one_name))
        .cloned();
    if reference.is_none() {
        for candidate in known.iter() {
            if !ordinal::starts_with_ignore_case(&candidate.name, stem) {
                continue;
            }
            let better = reference.as_ref().is_none_or(|current| {
                candidate.tier < current.tier
                    || (candidate.tier == current.tier && candidate.spell_id < current.spell_id)
            });
            if better {
                reference = Some(candidate.clone());
            }
        }
    }
    reference
}

/// The best tier of the reference's family the character can cast now
/// (`TryResolveBestKnown` by spell).
fn try_resolve_best_known(
    castability: &mut AutomationCastability<'_>,
    reference: &SpellInfo,
    settings: &BuffSettings,
) -> Option<SpellInfo> {
    let known = castability.known_self_buffs();
    let mut tiers: Vec<SpellInfo> = known
        .iter()
        .filter(|candidate| candidate.family == reference.family)
        .cloned()
        .collect();
    tiers.sort_by(|a, b| {
        b.tier
            .cmp(&a.tier)
            .then(b.quality().cmp(&a.quality()))
            .then(a.spell_id.cmp(&b.spell_id))
    });
    let mut skill_levels: HashMap<u32, u32> = HashMap::new();
    for skill in castability.skills() {
        skill_levels.insert(skill.skill_id, skill.current);
    }
    let mut line = BuffLine::new(
        reference.family,
        BuffTargetKind::Other,
        stem_of(&reference.name),
        tiers,
    );
    line.reference_override = Some(reference.clone());
    plan::try_pick_tier(
        &line,
        &skill_levels,
        settings.skill_excess_over_difficulty,
        Some(castability),
    )
}
