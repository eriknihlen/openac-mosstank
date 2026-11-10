//! Debuffs: which ones a monster's rule asks for and in what order
//! (`CombatDebuffChain`), which spells are which debuff, by their family
//! in the spell table ([`DebuffLines`]), and what has landed on whom
//! (`DebuffTracker`).

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use mosstank_automation::spells::{SpellCatalog, SpellTable};
use mosstank_data::damage::MonsterDamageType;
use mosstank_dotnet::text::ordinal;
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions};

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

/// The spell the reference client names for each debuff: the debuff is
/// that spell's family in the spell table. Two are named by their seventh
/// level, the rest by their first.
const SEEDS: [(&str, MonsterActionFlags, MonsterDamageType); 17] = [
    (
        "Fester Other I",
        MonsterActionFlags::FESTER,
        MonsterDamageType::Auto,
    ),
    (
        "Broadside of a Barn",
        MonsterActionFlags::BROADSIDE,
        MonsterDamageType::Auto,
    ),
    (
        "Gravity Well",
        MonsterActionFlags::GRAVITY_WELL,
        MonsterDamageType::Auto,
    ),
    (
        "Imperil Other I",
        MonsterActionFlags::IMPERIL,
        MonsterDamageType::Auto,
    ),
    (
        "Magic Yield Other I",
        MonsterActionFlags::YIELD,
        MonsterDamageType::Auto,
    ),
    (
        "Weakening Curse I",
        MonsterActionFlags::WEAKENING_CURSE,
        MonsterDamageType::Auto,
    ),
    (
        "Festering Curse I",
        MonsterActionFlags::FESTERING_CURSE,
        MonsterDamageType::Auto,
    ),
    (
        "Corruption I",
        MonsterActionFlags::CORRUPTION,
        MonsterDamageType::Auto,
    ),
    (
        "Destructive Curse I",
        MonsterActionFlags::DESTRUCTIVE_CURSE,
        MonsterDamageType::Auto,
    ),
    (
        "Corrosion I",
        MonsterActionFlags::CORROSION,
        MonsterDamageType::Auto,
    ),
    (
        "Acid Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Acid,
    ),
    (
        "Bludgeoning Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Bludgeon,
    ),
    (
        "Cold Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Cold,
    ),
    (
        "Fire Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Fire,
    ),
    (
        "Lightning Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Electric,
    ),
    (
        "Piercing Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Pierce,
    ),
    (
        "Blade Vulnerability Other I",
        MonsterActionFlags::VULNERABILITY,
        MonsterDamageType::Slash,
    ),
];

/// One debuff's place in the spell table.
#[derive(Debug, Clone, PartialEq)]
struct DebuffLine {
    identity: DebuffIdentity,
    /// The named spell's school, fellowship and cast-at-nothing flags and
    /// formula: what the levels a character learns share with it.
    school: u32,
    is_fellowship: bool,
    is_untargeted: bool,
    component_set: (u32, u32, u32, u32),
}

/// How long a debuff lasts, seconds: the spell table's duration, else the
/// "over N seconds" of its description. The client's spell table carries no
/// duration for an enchantment projectile (Corrosion's 15 seconds and
/// Corruption's 30 are only in the text), and 0 would make the curse due
/// again the moment it landed.
#[must_use]
pub fn duration_seconds(spell: &SpellInfo) -> f64 {
    let listed = f64::from(spell.duration_seconds);
    if listed.is_finite() && listed > 0.0 {
        return listed;
    }
    described_duration(&spell.description).unwrap_or(0.0)
}

fn described_duration(description: &str) -> Option<f64> {
    let words: Vec<&str> = description.split_whitespace().collect();
    words.windows(3).find_map(|window| {
        if !window[0].eq_ignore_ascii_case("over") {
            return None;
        }
        let amount: f64 = window[1].parse().ok()?;
        let unit = window[2]
            .trim_end_matches(['.', ',', ';'])
            .to_ascii_lowercase();
        let scale = match unit.as_str() {
            "second" | "seconds" => 1.0,
            "minute" | "minutes" => 60.0,
            _ => return None,
        };
        (amount.is_finite() && amount > 0.0).then_some(amount * scale)
    })
}

/// A spell name without its "Incantation of " prefix.
pub(crate) fn normalize(name: &str) -> &str {
    const INCANTATION: &str = "Incantation of ";
    if ordinal::starts_with_ignore_case(name, INCANTATION) {
        &name[INCANTATION.len()..]
    } else {
        name
    }
}

/// Which spells are which debuff, read from the spell table.
///
/// A debuff is a family of the table, not a name: the game names the
/// seventh level of most debuffs for itself ("Gossamer Flesh" is the
/// seventh Imperil), and two debuffs are known by that seventh name. The
/// family also holds the debuff's self-cast twin and the spells only
/// monsters and items cast, so the spell a character throws is further
/// held to the named spell's line ([`Self::classify_learned`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DebuffLines {
    by_family: HashMap<u32, DebuffLine>,
}

impl DebuffLines {
    /// The debuffs of `table`. A debuff whose named spell the table lacks
    /// has no spells.
    #[must_use]
    pub fn build(table: &SpellTable) -> Self {
        let mut by_family = HashMap::with_capacity(SEEDS.len());
        for (name, flag, damage_type) in SEEDS {
            let Some(seed) = table.find_by_name(name, false) else {
                continue;
            };
            // Family 0 is "no family": every spell without one would match.
            if seed.family == 0 {
                continue;
            }
            by_family.entry(seed.family).or_insert(DebuffLine {
                identity: DebuffIdentity::new(flag, damage_type),
                school: seed.school,
                is_fellowship: seed.is_fellowship,
                is_untargeted: seed.is_untargeted,
                component_set: seed.component_set,
            });
        }
        Self { by_family }
    }

    /// The debuffs of the client's spell table.
    #[must_use]
    pub fn of(spells: &mut dyn SpellCatalog) -> Self {
        Self::build(&spells.table())
    }

    /// Which debuff a spell is, with its place in the chain: the debuff
    /// whose family it is in. This is what a spell an item casts, a cast
    /// another client reports and an enchantment in force are told by.
    #[must_use]
    pub fn classify(&self, spell: &SpellInfo) -> Option<(DebuffIdentity, i32)> {
        let line = self.by_family.get(&spell.family)?;
        Some((line.identity, order_of(line.identity.flag)))
    }

    /// [`Self::classify`] for a spell the character throws: it must also
    /// be of the named spell's school, with its fellowship and
    /// cast-at-nothing flags and its herb, powder, potion and talisman.
    /// That is what keeps the self-cast twin out (it differs in the
    /// talisman).
    #[must_use]
    pub fn classify_learned(&self, spell: &SpellInfo) -> Option<(DebuffIdentity, i32)> {
        let line = self.by_family.get(&spell.family)?;
        (spell.school == line.school
            && spell.is_fellowship == line.is_fellowship
            && spell.is_untargeted == line.is_untargeted
            && spell.component_set == line.component_set)
            .then(|| (line.identity, order_of(line.identity.flag)))
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
    /// A projectile went out: it is up only once a result line says it
    /// reached the monster.
    pub awaiting_landing: bool,
}

/// How long a debuff projectile has to report reaching its monster after
/// the server finished the cast: the attack spells' result budget.
pub const LANDING_TIMEOUT_SECONDS: f64 =
    mosstank_automation::cast_tracker::SpellCastTracker::RESULT_TIMEOUT_SECONDS;

#[derive(Debug, Clone)]
struct Pending {
    target_object_id: u32,
    identity: DebuffIdentity,
    spell: SpellInfo,
    dispatched_at: f64,
    projectile: bool,
}

/// A debuff projectile the server launched, not yet known to have landed.
#[derive(Debug, Clone)]
struct InFlight {
    target_object_id: u32,
    identity: DebuffIdentity,
    spell: SpellInfo,
    launched_at: f64,
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
    in_flight: Option<InFlight>,
    observed_completion_revision: i64,
    retry_not_before: f64,
}

impl Default for DebuffTracker {
    fn default() -> Self {
        Self {
            applied: HashMap::new(),
            pending: None,
            in_flight: None,
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

    /// Whether a cast is unanswered, or its projectile not yet landed.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.in_flight.is_some()
    }

    /// Whether the server finished the cast and its projectile is flying.
    #[must_use]
    pub fn is_in_flight(&self) -> bool {
        self.in_flight.is_some()
    }

    /// Whether the last busy refusal's wait is still running.
    #[must_use]
    pub fn retry_held(&self, now: f64) -> bool {
        now < self.retry_not_before
    }

    /// The unanswered spell's name.
    #[must_use]
    pub fn pending_name(&self) -> &str {
        if let Some(flying) = &self.in_flight {
            return flying.spell.name.as_str();
        }
        self.pending.as_ref().map_or("", |p| p.spell.name.as_str())
    }

    /// The unanswered cast's monster.
    #[must_use]
    pub fn pending_target(&self) -> u32 {
        if let Some(flying) = &self.in_flight {
            return flying.target_object_id;
        }
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
        self.begin_cast(
            target_object_id,
            identity,
            spell,
            now,
            completion_revision,
            false,
        );
    }

    /// A cast went out; a projectile's answer only launches it, and it is
    /// up once [`Self::land`] is told it reached the monster.
    pub fn begin_cast(
        &mut self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: SpellInfo,
        now: f64,
        completion_revision: i64,
        projectile: bool,
    ) {
        self.observed_completion_revision =
            self.observed_completion_revision.max(completion_revision);
        self.in_flight = None;
        self.pending = Some(Pending {
            target_object_id,
            identity,
            spell,
            dispatched_at: now,
            projectile,
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
                awaiting_landing: false,
            };
        }
        if pending.projectile {
            let completion = DebuffCompletion {
                completed: true,
                succeeded: true,
                spell_name: pending.spell.name.clone(),
                weenie_error: 0,
                target_object_id: pending.target_object_id,
                spell: Some(pending.spell.clone()),
                awaiting_landing: true,
            };
            self.in_flight = Some(InFlight {
                target_object_id: pending.target_object_id,
                identity: pending.identity,
                spell: pending.spell,
                launched_at: now,
            });
            return completion;
        }
        let duration = duration_seconds(&pending.spell);
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
            awaiting_landing: false,
        }
    }

    /// The flying debuff's monster and spell.
    #[must_use]
    pub fn in_flight(&self) -> Option<(u32, &SpellInfo)> {
        self.in_flight
            .as_ref()
            .map(|flying| (flying.target_object_id, &flying.spell))
    }

    /// The flying debuff reached its monster and took hold: it is up from
    /// now. Its monster and spell.
    pub fn land(&mut self, now: f64) -> Option<(u32, SpellInfo)> {
        let flying = self.in_flight.take()?;
        self.record_applied(flying.target_object_id, flying.identity, &flying.spell, now);
        Some((flying.target_object_id, flying.spell))
    }

    /// The flying debuff reached its monster but did not take hold (a
    /// resist): it is due again, and it was no miss.
    pub fn reached(&mut self) -> Option<u32> {
        self.in_flight.take().map(|flying| flying.target_object_id)
    }

    /// No word of the flying debuff in time: it went somewhere else. Its
    /// monster and spell.
    pub fn expire_in_flight(&mut self, now: f64) -> Option<(u32, SpellInfo)> {
        match &self.in_flight {
            Some(flying) if now - flying.launched_at >= LANDING_TIMEOUT_SECONDS => {
                let flying = self.in_flight.take()?;
                Some((flying.target_object_id, flying.spell))
            }
            _ => None,
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
        let duration = duration_seconds(spell);
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
        self.in_flight = None;
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
        self.in_flight = None;
        self.observed_completion_revision = 0;
        self.retry_not_before = f64::MIN;
    }
}
