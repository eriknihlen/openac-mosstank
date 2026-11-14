//! Debuffs: which ones a monster's rule asks for and in what order
//! (`CombatDebuffChain`), which spells are which debuff, by their family
//! in the spell table ([`DebuffLines`]), and what has landed on whom
//! (`DebuffTracker`).

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use mosstank_automation::spells::{SpellCatalog, SpellTable};
use mosstank_data::damage::MonsterDamageType;
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

/// How long a debuff lasts, seconds: the reference client's own table for
/// three Void curses, else the spell table's duration. The spell table
/// stores no duration for an enchantment projectile (Corrosion and
/// Corruption have 0 there), and 0 would make the curse due again the
/// moment it landed.
#[must_use]
pub fn duration_seconds(spell: &SpellInfo) -> f64 {
    match spell.family {
        // Corruption.
        638 => 31.0,
        // Corrosion.
        637 => 16.0,
        // Destructive Curse.
        636 => 31.0,
        _ => f64::from(spell.duration_seconds).max(0.0),
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
    /// A projectile went out and no result line has come: it is up only
    /// once one says it reached the monster.
    pub awaiting_landing: bool,
    /// A projectile's result line came before the server finished the
    /// cast: what it said is already counted (a landing recorded and
    /// announced, a resist or refusal ended the wait).
    pub answered_by_line: bool,
}

/// How long a debuff projectile has to report reaching its monster after
/// the server finished the cast: the attack spells' result budget.
pub const LANDING_TIMEOUT_SECONDS: f64 =
    mosstank_automation::cast_tracker::SpellCastTracker::RESULT_TIMEOUT_SECONDS;

/// A projectile debuff's wait for its result line, from the dispatch on.
#[derive(Debug, Clone)]
struct LandingWatch {
    /// The monster's name as the result line gives it.
    target_name: String,
    /// A result line already answered.
    answered: bool,
}

#[derive(Debug, Clone)]
struct Pending {
    target_object_id: u32,
    identity: DebuffIdentity,
    spell: SpellInfo,
    dispatched_at: f64,
    landing: Option<LandingWatch>,
}

/// A debuff projectile the server launched, not yet known to have landed.
#[derive(Debug, Clone)]
struct InFlight {
    target_object_id: u32,
    target_name: String,
    identity: DebuffIdentity,
    spell: SpellInfo,
    launched_at: f64,
}

/// The projectile debuff whose result line is awaited.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WatchedLanding<'a> {
    /// The monster.
    pub target_object_id: u32,
    /// Its name as the result line gives it (empty when unknown).
    pub target_name: &'a str,
    /// The spell.
    pub spell: &'a SpellInfo,
}

#[derive(Debug, Clone, Copy)]
struct Applied {
    spell_id: u32,
    tier: i32,
    expires_at: f64,
}

/// How long a teammate's cast attempt holds a debuff off a monster for
/// everyone else, seconds: as long as the reference counts an attempt
/// another client logged.
pub const CLAIM_SECONDS: f64 = 6.0;

/// A teammate's cast attempt on a monster: the debuff counts as up there,
/// at the attempt's tier or below, until it lands or the claim runs out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebuffClaim {
    /// The monster.
    pub target_object_id: u32,
    /// The debuff.
    pub identity: DebuffIdentity,
    /// Who is casting it.
    pub caster_id: u32,
    /// The spell attempted.
    pub spell_id: u32,
    /// Its tier.
    pub tier: i32,
    /// When the claim runs out (the controller's clock, seconds).
    pub expires_at: f64,
}

/// The session's debuffs: one is up only once the server said it landed.
#[derive(Debug, Clone)]
pub struct DebuffTracker {
    applied: HashMap<(u32, DebuffIdentity), Applied>,
    claims: Vec<DebuffClaim>,
    /// Each monster's latest teammate debuff: when the last of the debuffs
    /// a teammate landed on it runs out. This character's own never count.
    team: HashMap<u32, f64>,
    pending: Option<Pending>,
    in_flight: Option<InFlight>,
    observed_completion_revision: i64,
    retry_not_before: f64,
}

impl Default for DebuffTracker {
    fn default() -> Self {
        Self {
            applied: HashMap::new(),
            claims: Vec::new(),
            team: HashMap::new(),
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
        // A teammate casting it at this tier or better has it in hand.
        if self.is_claimed(target_object_id, identity, spell, now) {
            return false;
        }
        let Some(applied) = self.applied.get(&(target_object_id, identity)) else {
            return true;
        };
        if applied.spell_id != spell.spell_id && spell.tier > applied.tier {
            return true;
        }
        now >= applied.expires_at - tolerance_seconds.max(0.0)
    }

    /// Whether a teammate's attempt at this spell's tier or better is
    /// claiming the debuff on the monster.
    #[must_use]
    pub fn is_claimed(
        &self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: &SpellInfo,
        now: f64,
    ) -> bool {
        self.claims.iter().any(|claim| {
            claim.target_object_id == target_object_id
                && claim.identity == identity
                && claim.tier >= spell.tier
                && now < claim.expires_at
        })
    }

    /// A teammate is casting a debuff at a monster: it counts as up there
    /// for [`CLAIM_SECONDS`].
    pub fn record_claim(
        &mut self,
        target_object_id: u32,
        identity: DebuffIdentity,
        spell: &SpellInfo,
        caster_id: u32,
        now: f64,
    ) {
        self.expire_claims(now);
        self.claims.push(DebuffClaim {
            target_object_id,
            identity,
            caster_id,
            spell_id: spell.spell_id,
            tier: spell.tier,
            expires_at: now + CLAIM_SECONDS,
        });
    }

    /// The claims still running.
    #[must_use]
    pub fn claims(&self, now: f64) -> Vec<DebuffClaim> {
        self.claims
            .iter()
            .filter(|claim| now < claim.expires_at)
            .copied()
            .collect()
    }

    /// Forgets the claims that ran out.
    pub fn expire_claims(&mut self, now: f64) {
        self.claims.retain(|claim| now < claim.expires_at);
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
        self.in_flight = None;
        self.pending = Some(Pending {
            target_object_id,
            identity,
            spell,
            dispatched_at: now,
            landing: None,
        });
    }

    /// The cast just begun is a projectile: it is up only once its result
    /// line names `target_name`, and that line is awaited from now on,
    /// since the server sends it as the projectile lands, which can be
    /// before it finishes the cast.
    pub fn watch_landing(&mut self, target_name: &str) {
        if let Some(pending) = &mut self.pending {
            pending.landing = Some(LandingWatch {
                target_name: target_name.to_owned(),
                answered: false,
            });
        }
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
        let mut answer = DebuffCompletion {
            completed: true,
            succeeded: true,
            spell_name: pending.spell.name.clone(),
            weenie_error: 0,
            target_object_id: pending.target_object_id,
            spell: None,
            awaiting_landing: false,
            answered_by_line: false,
        };
        if !completion.is_success() {
            if completion.weenie_error == cast_refusal::CHARACTER_WAS_BUSY {
                self.retry_not_before = now + cast_refusal::RETRY_WAIT_SECONDS;
            }
            answer.succeeded = false;
            answer.weenie_error = completion.weenie_error;
            answer.spell = Some(pending.spell);
            return answer;
        }
        match pending.landing {
            // Its result line already came: nothing more to wait for.
            Some(LandingWatch { answered: true, .. }) => {
                answer.answered_by_line = true;
                answer.spell = Some(pending.spell);
            }
            Some(LandingWatch {
                target_name,
                answered: false,
            }) => {
                answer.awaiting_landing = true;
                answer.spell = Some(pending.spell.clone());
                self.in_flight = Some(InFlight {
                    target_object_id: pending.target_object_id,
                    target_name,
                    identity: pending.identity,
                    spell: pending.spell,
                    launched_at: now,
                });
            }
            None => {
                self.record_applied(
                    pending.target_object_id,
                    pending.identity,
                    &pending.spell,
                    now,
                );
                answer.spell = Some(pending.spell);
            }
        }
        answer
    }

    /// The projectile debuff whose result line is awaited: the cast still
    /// out, or the projectile flying after it.
    #[must_use]
    pub fn watched_landing(&self) -> Option<WatchedLanding<'_>> {
        if let Some(flying) = &self.in_flight {
            return Some(WatchedLanding {
                target_object_id: flying.target_object_id,
                target_name: &flying.target_name,
                spell: &flying.spell,
            });
        }
        let pending = self.pending.as_ref()?;
        match &pending.landing {
            Some(watch) if !watch.answered => Some(WatchedLanding {
                target_object_id: pending.target_object_id,
                target_name: &watch.target_name,
                spell: &pending.spell,
            }),
            _ => None,
        }
    }

    /// The awaited projectile reached its monster and took hold: it is up
    /// from now. Its monster and spell. A cast still out stays out until
    /// the server finishes it.
    pub fn land(&mut self, now: f64) -> Option<(u32, SpellInfo)> {
        let (target_object_id, identity, spell) = if let Some(flying) = self.in_flight.take() {
            (flying.target_object_id, flying.identity, flying.spell)
        } else {
            let pending = self.pending.as_mut()?;
            let watch = pending.landing.as_mut().filter(|watch| !watch.answered)?;
            watch.answered = true;
            (
                pending.target_object_id,
                pending.identity,
                pending.spell.clone(),
            )
        };
        self.record_applied(target_object_id, identity, &spell, now);
        Some((target_object_id, spell))
    }

    /// The awaited projectile's result line said it did not take hold (a
    /// resist, or a refusal the same cast would meet again): it is due
    /// again. Its monster.
    pub fn end_landing_watch(&mut self) -> Option<u32> {
        if let Some(flying) = self.in_flight.take() {
            return Some(flying.target_object_id);
        }
        let pending = self.pending.as_mut()?;
        let watch = pending.landing.as_mut().filter(|watch| !watch.answered)?;
        watch.answered = true;
        Some(pending.target_object_id)
    }

    /// Stops waiting for the flying projectile (its monster died or left):
    /// no landing, and no miss. Its monster and spell.
    pub fn abandon_flight(&mut self) -> Option<(u32, SpellInfo)> {
        self.in_flight
            .take()
            .map(|flying| (flying.target_object_id, flying.spell))
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
        // The landed cast is the real record of what its caster claimed.
        self.claims.retain(|claim| {
            claim.target_object_id != target_object_id || claim.identity != identity
        });
        let expires_at = now + seconds_remaining;
        let team = self.team.entry(target_object_id).or_insert(f64::MIN);
        *team = team.max(expires_at);
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

    /// Every debuff in force, landed by this character or a teammate: the
    /// monster, the debuff, its spell and the seconds left; by monster,
    /// then in the chain's order.
    #[must_use]
    pub fn in_force(&self, now: f64) -> Vec<(u32, DebuffIdentity, u32, f64)> {
        let mut entries: Vec<(u32, DebuffIdentity, u32, f64)> = self
            .applied
            .iter()
            .filter(|(_, applied)| now < applied.expires_at)
            .map(|(&(target, identity), applied)| {
                (target, identity, applied.spell_id, applied.expires_at - now)
            })
            .collect();
        entries.sort_by_key(|&(target, identity, _, _)| {
            (target, order_of(identity.flag), identity.damage_type as i32)
        });
        entries
    }

    /// Whether any debuff is in force on the monster, this character's own
    /// or a teammate's.
    #[must_use]
    pub fn has_any_in_force(&self, target_object_id: u32, now: f64) -> bool {
        self.applied
            .iter()
            .any(|(&(target, _), applied)| target == target_object_id && now < applied.expires_at)
    }

    /// Whether a debuff a teammate landed on the monster is in force (this
    /// character's own debuffs never count).
    #[must_use]
    pub fn has_team_debuff(&self, target_object_id: u32, now: f64) -> bool {
        self.team
            .get(&target_object_id)
            .is_some_and(|&expires_at| now < expires_at)
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
        self.claims
            .retain(|claim| live_targets.contains(&claim.target_object_id));
        self.team.retain(|target, _| live_targets.contains(target));
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        self.applied.clear();
        self.claims.clear();
        self.team.clear();
        self.pending = None;
        self.in_flight = None;
        self.observed_completion_revision = 0;
        self.retry_not_before = f64::MIN;
    }
}

/// A debuff in words: its column, and a vulnerability's element.
#[must_use]
pub fn label(identity: DebuffIdentity) -> String {
    let column = match identity.flag {
        MonsterActionFlags::FESTER => "Fester",
        MonsterActionFlags::BROADSIDE => "Broadside",
        MonsterActionFlags::GRAVITY_WELL => "Gravity Well",
        MonsterActionFlags::IMPERIL => "Imperil",
        MonsterActionFlags::YIELD => "Yield",
        MonsterActionFlags::VULNERABILITY => "Vulnerability",
        MonsterActionFlags::WEAKENING_CURSE => "Weakening Curse",
        MonsterActionFlags::FESTERING_CURSE => "Festering Curse",
        MonsterActionFlags::CORRUPTION => "Corruption",
        MonsterActionFlags::DESTRUCTIVE_CURSE => "Destructive Curse",
        MonsterActionFlags::CORROSION => "Corrosion",
        _ => "Debuff",
    };
    if identity.flag == MonsterActionFlags::VULNERABILITY {
        format!("{column} ({:?})", identity.damage_type)
    } else {
        column.to_owned()
    }
}
