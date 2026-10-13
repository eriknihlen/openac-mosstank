//! The random helper (`RandomHelperRule.cs`): now and then one random
//! beneficial spell on one nearby player. The rule remembers no target's
//! enchantments and no earlier pick.

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::magic::{CastGate, CastResult, SpellInfo};
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_automation::Automation;
use mosstank_automation::heading::horizontal_distance_meters;
use mosstank_automation::locks::{ActionLockKind, SharedActionLocks};
use mosstank_combat::MacroLogChannel;
use mosstank_combat::gate::{GateTurn, Prepare, SharedCombatModeGate};
use mosstank_settings::model::{BuffSettings, CombatSettings, VitalSettings};
use openac_plugin_compat::random::DotNetRandom;

use crate::buff_rule::try_resolve_best_known_by_name;
use crate::castability::{AutomationCastability, CastabilityReport};
use crate::plan::BuffTierRejection;
use crate::profile::BuffLine;

/// How near a player must stand, metres (exclusive).
const MAXIMUM_PLAYER_DISTANCE_METERS: f64 = 18.0;
/// How many spells are drawn before the rule gives up.
const MAXIMUM_SPELL_DRAWS: usize = 100;

/// The families' tier-I names the helper draws from.
const TIER_ONE_SPELL_NAMES: [&str; 11] = [
    "Endurance Other I",
    "Regeneration Other I",
    "Rejuvenation Other I",
    "Armor Other I",
    "Blade Protection Other I",
    "Bludgeoning Protection Other I",
    "Cold Protection Other I",
    "Fire Protection Other I",
    "Lightning Protection Other I",
    "Piercing Protection Other I",
    "Acid Protection Other I",
];

/// Where the rule's macro log lines go.
pub type HelperLog = Box<dyn FnMut(MacroLogChannel, &str)>;

/// The rule's name.
pub const RANDOM_HELPER_NAME: &str = "RandomHelper";

/// What one call of the random helper works with.
#[allow(missing_debug_implementations)]
pub struct HelperTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The buff settings.
    pub settings: &'a BuffSettings,
    /// The combat settings (for the stance gate).
    pub combat: &'a CombatSettings,
    /// The vital settings (for the stance gate).
    pub vitals: &'a VitalSettings,
}

/// The random helper (`RandomHelperRule`).
#[allow(missing_debug_implementations)]
pub struct RandomHelperRule {
    locks: SharedActionLocks,
    gate: SharedCombatModeGate,
    random: DotNetRandom,
    posted_warnings: Vec<String>,
    pending_spell: Option<SpellInfo>,
    pending_target_object_id: u32,
    pending_target_name: String,
    decline_reason: Option<String>,
    status: String,
    /// Where the rule's macro log lines go (`Log`).
    pub log: Option<HelperLog>,
}

/// Collects the castability's warnings, to be posted once the walk
/// released the client.
struct Collect(Vec<String>);

impl CastabilityReport for Collect {
    fn warn(&mut self, text: &str) {
        self.0.push(text.to_owned());
    }

    fn note_tier_pick(
        &mut self,
        _line: &BuffLine,
        _pick: Option<&SpellInfo>,
        _rejections: &[BuffTierRejection],
    ) {
    }
}

impl RandomHelperRule {
    /// A rule sharing the cooldown table and the stance gate, drawing from
    /// a generator seeded with `seed` (register MANA-1).
    #[must_use]
    pub fn new(locks: SharedActionLocks, gate: SharedCombatModeGate, seed: i32) -> Self {
        Self {
            locks,
            gate,
            random: DotNetRandom::new(seed),
            posted_warnings: Vec::new(),
            pending_spell: None,
            pending_target_object_id: 0,
            pending_target_name: String::new(),
            decline_reason: None,
            status: String::new(),
            log: None,
        }
    }

    /// What it is doing.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Why it last declined.
    #[must_use]
    pub fn decline_reason(&self) -> Option<&str> {
        self.decline_reason.as_deref()
    }

    /// Whether a cast is picked for this pass (`ValidNow`).
    pub fn valid_now(&mut self, turn: &mut HelperTurn<'_>, can_act: bool) -> bool {
        self.clear_pending_cast();
        self.decline_reason = None;

        if !can_act {
            return false;
        }
        if !turn.settings.random_helper_buffs {
            return self.decline("disabled");
        }
        if self.locks.borrow_mut().is_locked(ActionLockKind::ItemUse) {
            return self.decline("item use is locked");
        }
        if self
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::RandomHelperBuffLock)
        {
            return self.decline("the helper interval is still active");
        }
        let automation = &mut *turn.automation;
        if !automation.is_available() {
            return self.decline("the automation surface is unavailable");
        }
        if automation.is_casting() {
            return self.decline("another cast is in progress");
        }
        let navigation = automation.navigation_snapshot();
        if !navigation.is_available {
            return self.decline("the local position is unavailable");
        }
        let players = capture_nearby_players(automation, &navigation.position);
        if players.is_empty() {
            return self.decline("no nearby player is eligible");
        }

        // One target is retained for the complete spell search.
        let target = players
            [usize::try_from(self.random.next_max(count(players.len()))).unwrap_or(0)]
        .clone();
        let owned = if automation.items_available() {
            automation.owned_items()
        } else {
            Vec::new()
        };
        let mut report = Collect(Vec::new());
        let mut picked: Option<SpellInfo> = None;
        {
            let mut castability = AutomationCastability::new(
                automation,
                owned,
                &turn.settings.blacklisted_spell_components,
                &mut report,
            );
            for _ in 0..MAXIMUM_SPELL_DRAWS {
                let tier_one_name = TIER_ONE_SPELL_NAMES[usize::try_from(
                    self.random.next_max(count(TIER_ONE_SPELL_NAMES.len())),
                )
                .unwrap_or(0)];
                if let Some(spell) =
                    try_resolve_best_known_by_name(&mut castability, tier_one_name, turn.settings)
                {
                    picked = Some(spell);
                    break;
                }
            }
        }
        for text in report.0 {
            self.warn_once(turn.automation, &text);
        }
        let Some(spell) = picked else {
            return self.decline("no helper spell is currently castable");
        };
        self.status = format!("Preparing {} for {}", spell.name, target.name);
        self.pending_spell = Some(spell);
        self.pending_target_object_id = target.object_id;
        self.pending_target_name = target.name;
        true
    }

    /// The rule never reads as running; set running, it issues the cast it
    /// picked (`Running`).
    pub fn set_running(&mut self, turn: &mut HelperTurn<'_>, running: bool) {
        if !running {
            self.clear_pending_cast();
            return;
        }
        self.issue_pending_cast(turn);
    }

    /// Clears the pending work and the interval lock (`Reset`).
    pub fn reset(&mut self) {
        self.clear_pending_cast();
        self.locks
            .borrow_mut()
            .release(ActionLockKind::RandomHelperBuffLock);
        self.reset_once_per_run_warnings();
        self.decline_reason = None;
        self.status.clear();
    }

    /// Lets the once-per-run warnings be said again.
    pub fn reset_once_per_run_warnings(&mut self) {
        self.posted_warnings.clear();
    }

    fn issue_pending_cast(&mut self, turn: &mut HelperTurn<'_>) {
        // Consume the pass-local pick before calling shared services, so a
        // re-entrant activation cannot issue it twice.
        let Some(spell) = self.pending_spell.take() else {
            return;
        };
        let target_object_id = self.pending_target_object_id;
        let target_name = std::mem::take(&mut self.pending_target_name);
        self.clear_pending_cast();

        {
            let mut gate = self.gate.borrow_mut();
            let mut gate_turn = GateTurn {
                automation: &mut *turn.automation,
                combat: turn.combat,
                vitals: turn.vitals,
            };
            if !gate.try_prepare(&mut gate_turn, Prepare::mode(CombatMode::Magic), None) {
                gate.status().clone_into(&mut self.status);
                return;
            }
        }

        let gate = turn
            .automation
            .evaluate_gate(spell.spell_id, Some(target_object_id));
        if gate != CastGate::Ready {
            self.status = format!("Cannot cast {}: {gate:?}", spell.name);
            return;
        }
        let request = turn
            .automation
            .request_cast(spell.spell_id, Some(target_object_id));
        if request != CastResult::Sent {
            self.status = format!("Cannot cast {}: {request:?}", spell.name);
            return;
        }
        // A preparation attempt or a refused request does not consume the
        // interval: only a cast accepted for issue earns the lock.
        self.locks.borrow_mut().arm(
            ActionLockKind::RandomHelperBuffLock,
            turn.settings.random_helper_interval_seconds,
        );
        self.status = format!("Casting {} on {target_name}", spell.name);
        if let Some(log) = self.log.as_mut() {
            log(
                MacroLogChannel::SpellCast,
                &format!(
                    "Casting: {} on {target_object_id} ({target_name})",
                    spell.name
                ),
            );
        }
        turn.automation.log_info(&format!(
            "MossTank: random helper {} -> {target_name}",
            spell.name
        ));
    }

    fn decline(&mut self, reason: &str) -> bool {
        self.decline_reason = Some(reason.to_owned());
        self.status.clear();
        false
    }

    fn warn_once(&mut self, automation: &mut dyn Automation, text: &str) {
        if self.posted_warnings.iter().any(|posted| posted == text) {
            return;
        }
        self.posted_warnings.push(text.to_owned());
        mosstank_combat::post(automation, &format!("[MossTank] {text}"));
        automation.log_warn(&format!("MossTank: {text}"));
    }

    fn clear_pending_cast(&mut self) {
        self.pending_spell = None;
        self.pending_target_object_id = 0;
        self.pending_target_name.clear();
    }
}

fn count(length: usize) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

/// The players other than the character standing within reach, in the
/// object table's order.
fn capture_nearby_players(
    automation: &mut dyn Automation,
    local: &ac_plugin_abi::world::Position,
) -> Vec<WorldObject> {
    let me = automation.character().id;
    automation
        .capture_objects()
        .into_iter()
        .filter(|candidate| {
            candidate.object_class == ObjectClass::Player
                && candidate.object_id != me
                && candidate.position.as_ref().is_some_and(|position| {
                    horizontal_distance_meters(local, position) < MAXIMUM_PLAYER_DISTANCE_METERS
                })
        })
        .collect()
}
