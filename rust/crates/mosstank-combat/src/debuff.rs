//! Debuffs: which ones a monster's rule asks for and in what order
//! (`CombatDebuffChain`), which spells are which debuff
//! (`DebuffSpellCatalog`), and what has landed on whom (`DebuffTracker`).

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions};
use openac_plugin_compat::text::ordinal;

/// One debuff: its column, and for a vulnerability its element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DebuffIdentity {
    /// The rule column.
    pub flag: MonsterActionFlags,
    /// The element (`Auto` for every debuff but a vulnerability).
    pub damage_type: MonsterDamageType,
}

impl DebuffIdentity {
    /// A debuff.
    #[must_use]
    pub const fn new(flag: MonsterActionFlags, damage_type: MonsterDamageType) -> Self {
        Self { flag, damage_type }
    }
}

/// One step of the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatDebuffStep {
    /// The debuff.
    pub identity: DebuffIdentity,
    /// Renewed only once it has run out entirely.
    pub zero_tolerance: bool,
}

/// The reference client's twelve steps.
const ORDER: [(MonsterActionFlags, bool); 12] = [
    (MonsterActionFlags::YIELD, false),
    (MonsterActionFlags::WEAKENING_CURSE, false),
    (MonsterActionFlags::FESTERING_CURSE, false),
    (MonsterActionFlags::CORRUPTION, true),
    (MonsterActionFlags::DESTRUCTIVE_CURSE, true),
    (MonsterActionFlags::CORROSION, true),
    (MonsterActionFlags::IMPERIL, false),
    // The attack element's vulnerability.
    (MonsterActionFlags::VULNERABILITY, false),
    // The extra vulnerability.
    (MonsterActionFlags::VULNERABILITY, false),
    (MonsterActionFlags::GRAVITY_WELL, false),
    (MonsterActionFlags::BROADSIDE, false),
    (MonsterActionFlags::FESTER, false),
];

const NATURAL_VULNERABILITY_STEP: usize = 7;
const EXTRA_VULNERABILITY_STEP: usize = 8;

/// Where a column sits in the chain (`i32::MAX` when not at all).
#[must_use]
pub fn order_of(flag: MonsterActionFlags) -> i32 {
    ORDER
        .iter()
        .position(|&(step, _)| step == flag)
        .map_or(i32::MAX, |index| i32::try_from(index).unwrap_or(i32::MAX))
}

/// Whether the extra vulnerability is a step at all: exactly when its column
/// names an element.
#[must_use]
pub fn has_extra_vulnerability(element: MonsterDamageType) -> bool {
    !matches!(element, MonsterDamageType::None | MonsterDamageType::Auto)
}

/// The chain a rule asks for.
#[must_use]
pub fn build_chain(
    actions: &MonsterRuleActions,
    attack_element: MonsterDamageType,
    extra_vulnerability: MonsterDamageType,
) -> Vec<CombatDebuffStep> {
    let mut steps = Vec::with_capacity(ORDER.len());
    for (index, &(flag, zero_tolerance)) in ORDER.iter().enumerate() {
        let mut element = MonsterDamageType::Auto;
        if index == NATURAL_VULNERABILITY_STEP {
            if !actions.flags.intersects(MonsterActionFlags::VULNERABILITY)
                || matches!(
                    attack_element,
                    MonsterDamageType::None | MonsterDamageType::Auto
                )
            {
                continue;
            }
            element = attack_element;
        } else if index == EXTRA_VULNERABILITY_STEP {
            if !has_extra_vulnerability(extra_vulnerability) {
                continue;
            }
            element = extra_vulnerability;
        } else if !actions.flags.intersects(flag) {
            continue;
        }
        steps.push(CombatDebuffStep {
            identity: DebuffIdentity::new(flag, element),
            zero_tolerance,
        });
    }
    steps
}

/// The first due step.
pub fn choose(
    steps: &[CombatDebuffStep],
    mut is_due: impl FnMut(&CombatDebuffStep) -> bool,
) -> Option<CombatDebuffStep> {
    steps.iter().find(|step| is_due(step)).copied()
}

/// Whether any step is due.
pub fn needs_debuff(
    steps: &[CombatDebuffStep],
    is_due: impl FnMut(&CombatDebuffStep) -> bool,
) -> bool {
    choose(steps, is_due).is_some()
}

/// The columns, in the catalogue's order.
const COLUMNS: [MonsterActionFlags; 11] = [
    MonsterActionFlags::FESTER,
    MonsterActionFlags::BROADSIDE,
    MonsterActionFlags::GRAVITY_WELL,
    MonsterActionFlags::IMPERIL,
    MonsterActionFlags::YIELD,
    MonsterActionFlags::VULNERABILITY,
    MonsterActionFlags::WEAKENING_CURSE,
    MonsterActionFlags::FESTERING_CURSE,
    MonsterActionFlags::CORRUPTION,
    MonsterActionFlags::DESTRUCTIVE_CURSE,
    MonsterActionFlags::CORROSION,
];

/// The debuffs a rule asks for (`DebuffSpellCatalog.Required`).
#[must_use]
pub fn required(actions: &MonsterRuleActions) -> HashSet<DebuffIdentity> {
    let mut required = HashSet::new();
    for flag in COLUMNS {
        if !actions.flags.intersects(flag) {
            continue;
        }
        let damage = if flag == MonsterActionFlags::VULNERABILITY {
            actions.damage_type
        } else {
            MonsterDamageType::Auto
        };
        required.insert(DebuffIdentity::new(flag, damage));
    }
    // The extra vulnerability stands on its own.
    if has_extra_vulnerability(actions.extra_vulnerability) {
        required.insert(DebuffIdentity::new(
            MonsterActionFlags::VULNERABILITY,
            actions.extra_vulnerability,
        ));
    }
    required
}

/// Which debuff a spell is, by its name, with its place in the chain
/// (`DebuffSpellCatalog.TryClassify`).
#[must_use]
pub fn classify(spell: &SpellInfo) -> Option<(DebuffIdentity, i32)> {
    let name = normalize(&spell.name);
    let starts = |prefix: &str| ordinal::starts_with_ignore_case(name, prefix);
    let mut damage = MonsterDamageType::Auto;
    let flag = if starts("Fester Other") {
        MonsterActionFlags::FESTER
    } else if starts("Broadside of a Barn") {
        MonsterActionFlags::BROADSIDE
    } else if starts("Gravity Well") {
        MonsterActionFlags::GRAVITY_WELL
    } else if starts("Imperil Other") {
        MonsterActionFlags::IMPERIL
    } else if starts("Magic Yield Other") {
        MonsterActionFlags::YIELD
    } else if ordinal::contains_ignore_case(name, " Vulnerability Other")
        || starts("Vulnerability Other")
    {
        // A creature vulnerability is the "<element> Vulnerability Other"
        // family and nothing else: a Lure is an item enchantment.
        damage = damage_from_name(name);
        MonsterActionFlags::VULNERABILITY
    } else if starts("Weakening Curse") {
        MonsterActionFlags::WEAKENING_CURSE
    } else if starts("Festering Curse") {
        MonsterActionFlags::FESTERING_CURSE
    } else if starts("Corruption") {
        MonsterActionFlags::CORRUPTION
    } else if starts("Destructive Curse") {
        MonsterActionFlags::DESTRUCTIVE_CURSE
    } else if starts("Corrosion") {
        MonsterActionFlags::CORROSION
    } else {
        return None;
    };
    Some((DebuffIdentity::new(flag, damage), order_of(flag)))
}

fn normalize(name: &str) -> &str {
    const INCANTATION: &str = "Incantation of ";
    if ordinal::starts_with_ignore_case(name, INCANTATION) {
        &name[INCANTATION.len()..]
    } else {
        name
    }
}

/// The element a spell's name names.
#[must_use]
pub fn damage_from_name(name: &str) -> MonsterDamageType {
    let has = |word: &str| ordinal::contains_ignore_case(name, word);
    if has("Blade") {
        MonsterDamageType::Slash
    } else if has("Piercing") {
        MonsterDamageType::Pierce
    } else if has("Bludgeon") {
        MonsterDamageType::Bludgeon
    } else if has("Cold") || has("Frost") {
        MonsterDamageType::Cold
    } else if has("Fire") || has("Flame") {
        MonsterDamageType::Fire
    } else if has("Acid") {
        MonsterDamageType::Acid
    } else if has("Lightning") {
        MonsterDamageType::Electric
    } else if has("Nether") {
        MonsterDamageType::Nether
    } else {
        MonsterDamageType::Auto
    }
}

/// What the server says when it will not take a request yet.
pub mod cast_refusal {
    /// "The character is already doing something."
    pub const CHARACTER_WAS_BUSY: u32 = 0x1D;
    /// How long such a refusal is left alone, seconds.
    pub const RETRY_WAIT_SECONDS: f64 = 0.25;
}

/// The server's answer to a learned debuff, matched to the cast in flight.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DebuffCompletion {
    /// An answer was matched.
    pub completed: bool,
    /// It landed.
    pub succeeded: bool,
    /// The spell's name.
    pub spell_name: String,
    /// The server's error, 0 for none.
    pub weenie_error: u32,
    /// The monster it went at.
    pub target_object_id: u32,
    /// The spell.
    pub spell: Option<SpellInfo>,
}

#[derive(Debug, Clone)]
struct Pending {
    target_object_id: u32,
    identity: DebuffIdentity,
    spell: SpellInfo,
    dispatched_at: f64,
}

#[derive(Debug, Clone, Copy)]
struct Applied {
    spell_id: u32,
    tier: i32,
    expires_at: f64,
}

/// The session's debuffs: one is up only once the server said it landed.
#[derive(Debug, Clone)]
pub struct DebuffTracker {
    applied: HashMap<(u32, DebuffIdentity), Applied>,
    pending: Option<Pending>,
    observed_completion_revision: i64,
    retry_not_before: f64,
}

impl Default for DebuffTracker {
    fn default() -> Self {
        Self {
            applied: HashMap::new(),
            pending: None,
            observed_completion_revision: 0,
            retry_not_before: f64::MIN,
        }
    }
}

impl DebuffTracker {
    /// An empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a cast is unanswered.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether the last busy refusal's wait is still running.
    #[must_use]
    pub fn retry_held(&self, now: f64) -> bool {
        now < self.retry_not_before
    }

    /// The unanswered spell's name.
    #[must_use]
    pub fn pending_name(&self) -> &str {
        self.pending.as_ref().map_or("", |p| p.spell.name.as_str())
    }

    /// The unanswered cast's monster.
    #[must_use]
    pub fn pending_target(&self) -> u32 {
        self.pending.as_ref().map_or(0, |p| p.target_object_id)
    }

    /// Whether the debuff is due on the monster.
    #[must_use]
    pub fn is_due(
        &self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: &SpellInfo,
        now: f64,
        tolerance_seconds: f64,
    ) -> bool {
        let Some(applied) = self.applied.get(&(target_object_id, identity)) else {
            return true;
        };
        if applied.spell_id != spell.spell_id && spell.tier > applied.tier {
            return true;
        }
        now >= applied.expires_at - tolerance_seconds.max(0.0)
    }

    /// Whether the debuff is up at this spell's tier or better.
    #[must_use]
    pub fn is_applied(
        &self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: Option<&SpellInfo>,
        now: f64,
    ) -> bool {
        let Some(known) = spell else {
            return false;
        };
        let Some(applied) = self.applied.get(&(target_object_id, identity)) else {
            return false;
        };
        if applied.spell_id == known.spell_id {
            now < applied.expires_at
        } else {
            applied.tier >= known.tier && now < applied.expires_at
        }
    }

    /// A cast went out.
    pub fn begin(
        &mut self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: SpellInfo,
        now: f64,
        completion_revision: i64,
    ) {
        self.observed_completion_revision =
            self.observed_completion_revision.max(completion_revision);
        self.pending = Some(Pending {
            target_object_id,
            identity,
            spell,
            dispatched_at: now,
        });
    }

    /// Reads the server's newest cast answer.
    pub fn observe(&mut self, completion: &CastCompletion, now: f64) -> DebuffCompletion {
        if completion.revision <= self.observed_completion_revision {
            return DebuffCompletion::default();
        }
        self.observed_completion_revision = completion.revision;
        let Some(pending) = self.pending.as_ref() else {
            return DebuffCompletion::default();
        };
        if pending.spell.spell_id != completion.spell_id
            || pending.target_object_id != completion.target_id
        {
            return DebuffCompletion::default();
        }
        let pending = self.pending.take().unwrap_or_else(|| unreachable!());
        if !completion.is_success() {
            if completion.weenie_error == cast_refusal::CHARACTER_WAS_BUSY {
                self.retry_not_before = now + cast_refusal::RETRY_WAIT_SECONDS;
            }
            return DebuffCompletion {
                completed: true,
                succeeded: false,
                spell_name: pending.spell.name.clone(),
                weenie_error: completion.weenie_error,
                target_object_id: pending.target_object_id,
                spell: Some(pending.spell),
            };
        }
        let duration = f64::from(pending.spell.duration_seconds).max(0.0);
        self.applied.insert(
            (pending.target_object_id, pending.identity),
            Applied {
                spell_id: pending.spell.spell_id,
                tier: pending.spell.tier,
                expires_at: now + duration,
            },
        );
        DebuffCompletion {
            completed: true,
            succeeded: true,
            spell_name: pending.spell.name.clone(),
            weenie_error: 0,
            target_object_id: pending.target_object_id,
            spell: Some(pending.spell),
        }
    }

    /// Gives up on an unanswered cast after the timeout.
    pub fn expire_pending(&mut self, now: f64, timeout_seconds: f64) -> bool {
        match &self.pending {
            Some(pending) if now - pending.dispatched_at >= timeout_seconds => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }

    /// Records a debuff as landed.
    pub fn record_applied(
        &mut self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: &SpellInfo,
        now: f64,
    ) {
        let duration = f64::from(spell.duration_seconds).max(0.0);
        self.applied.insert(
            (target_object_id, identity),
            Applied {
                spell_id: spell.spell_id,
                tier: spell.tier,
                expires_at: now + duration,
            },
        );
    }

    /// Records a cast another client on this computer saw land: the tier
    /// only rises and the expiry only moves later. True when it changed.
    pub fn record_remote(
        &mut self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: &SpellInfo,
        now: f64,
        seconds_remaining: f64,
    ) -> bool {
        // Zero, negative or not a number: nothing left to record.
        if seconds_remaining.is_nan() || seconds_remaining <= 0.0 {
            return false;
        }
        let expires_at = now + seconds_remaining;
        let key = (target_object_id, identity);
        let Some(existing) = self.applied.get(&key).copied() else {
            self.applied.insert(
                key,
                Applied {
                    spell_id: spell.spell_id,
                    tier: spell.tier,
                    expires_at,
                },
            );
            return true;
        };
        let stronger = spell.tier > existing.tier;
        let longer = expires_at > existing.expires_at;
        if !stronger && !longer {
            return false;
        }
        self.applied.insert(
            key,
            Applied {
                spell_id: if stronger {
                    spell.spell_id
                } else {
                    existing.spell_id
                },
                tier: existing.tier.max(spell.tier),
                expires_at: existing.expires_at.max(expires_at),
            },
        );
        true
    }

    /// `/vt fakeimp`: Gossamer Flesh, recorded locally for 3,000 seconds at
    /// a tier above every learnable Imperil.
    pub fn record_fake_imperil(&mut self, target_object_id: u32, now: f64) {
        const GOSSAMER_FLESH: u32 = 0x081A;
        const DURATION_SECONDS: f64 = 3000.0;
        self.applied.insert(
            (
                target_object_id,
                DebuffIdentity::new(MonsterActionFlags::IMPERIL, MonsterDamageType::Auto),
            ),
            Applied {
                spell_id: GOSSAMER_FLESH,
                tier: i32::MAX,
                expires_at: now + DURATION_SECONDS,
            },
        );
    }

    /// Drops the unanswered cast.
    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    /// Forgets the monsters no longer in the scan.
    pub fn retain_targets(&mut self, live_targets: &HashSet<u32>) {
        self.applied
            .retain(|(target, _), _| live_targets.contains(target));
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        self.applied.clear();
        self.pending = None;
        self.observed_completion_revision = 0;
        self.retry_not_before = f64::MIN;
    }
}
