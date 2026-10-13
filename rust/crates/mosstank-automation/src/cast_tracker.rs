//! One spell cast in flight (`SpellCastTracker`): from the request, through
//! the character's spoken words (the launch), to the result sentence, a
//! timeout, or the target's deletion.
//!
//! The C# raises callbacks (re-issue the cast, an attempt, an answer, the
//! outcome); here they queue as [`CastEvent`]s the owner drains with
//! [`SpellCastTracker::take_events`] after each call, in the order the C#
//! raised them, so re-issuing a cast can use the host the owner holds.

use ac_plugin_abi::magic::SpellInfo;
use mosstank_dotnet::text::{casing, ordinal};

use crate::combat_text::{self, CombatResultTextClass, log_text_type};
use crate::locks::{ActionLockKind, SharedActionLocks};

/// Where the cast is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpellCastTrackerState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// Requested; the spoken words have not been heard.
    AwaitingLaunch,
    /// Launched; the result sentence has not been read.
    AwaitingResult,
}

/// How a cast ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpellCastOutcome {
    /// Not ended.
    #[default]
    None,
    /// A success line matched.
    Success,
    /// A kill line matched.
    Kill,
    /// A fizzle or resist line matched.
    Fail,
    /// A permanent-fail line matched.
    PermanentFail,
    /// The client refused it.
    Rejected,
    /// The attempt budget expired with no acknowledgement.
    LaunchTimeout,
    /// No result line came in time.
    ResultTimeout,
}

/// One ended cast.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpellCastOutcomeInfo {
    /// How it ended.
    pub outcome: SpellCastOutcome,
    /// The spell.
    pub spell_id: u32,
    /// Its name.
    pub spell_name: String,
    /// The target.
    pub target_object_id: u32,
    /// The target's name (empty for an item enchantment).
    pub target_name: String,
    /// The spell hits several targets.
    pub hits_multiple_targets: bool,
    /// The client's error code, 0 for none.
    pub weenie_error: u32,
    /// The line that ended it.
    pub text: String,
    /// The target's incarnation when it was cast at.
    pub target_incarnation: u16,
}

/// Something the tracker asks of or tells its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastEvent {
    /// Send the cast again: the server has not acknowledged it.
    Reissue {
        /// The spell.
        spell_id: u32,
        /// The target.
        target_object_id: u32,
    },
    /// One more cast sent at a target that has not answered yet.
    Attempted(u32),
    /// The target answered: the cast now waits for its result.
    Answered(u32),
    /// The cast ended.
    Completed(SpellCastOutcomeInfo),
}

/// What [`SpellCastTracker::begin`] is told about the cast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastRequest<'a> {
    /// The spell.
    pub spell_id: u32,
    /// Its name.
    pub spell_name: &'a str,
    /// The target.
    pub target_object_id: u32,
    /// The target's name.
    pub target_name: &'a str,
    /// The spell hits several targets.
    pub hits_multiple_targets: bool,
    /// The words the character speaks.
    pub saying: &'a str,
    /// The magic skill it is cast with.
    pub school: u32,
    /// It may claim a killing blow.
    pub can_kill: bool,
    /// The caster's mana now.
    pub current_mana: i32,
    /// The target's incarnation.
    pub target_incarnation: u16,
}

impl<'a> CastRequest<'a> {
    /// A request with the C#'s defaults: no saying, school 0, cannot kill,
    /// unlimited mana, incarnation 0.
    #[must_use]
    pub fn new(
        spell_id: u32,
        spell_name: &'a str,
        target_object_id: u32,
        target_name: &'a str,
        hits_multiple_targets: bool,
    ) -> Self {
        Self {
            spell_id,
            spell_name,
            target_object_id,
            target_name,
            hits_multiple_targets,
            saying: "",
            school: 0,
            can_kill: false,
            current_mana: i32::MAX,
            target_incarnation: 0,
        }
    }
}

/// One cast in flight.
#[derive(Debug, Default)]
pub struct SpellCastTracker {
    state: SpellCastTrackerState,
    spell_id: u32,
    spell_name: String,
    target_object_id: u32,
    target_incarnation: u16,
    target_name: String,
    hits_multiple_targets: bool,
    saying: String,
    observed_chat_sequence: u64,
    launch_elapsed: f64,
    result_elapsed: f64,
    outcome: Option<SpellCastOutcomeInfo>,
    school: u32,
    can_kill: bool,
    reissue_interval: f64,
    next_reissue_at: f64,
    action_locks: Option<SharedActionLocks>,
    events: Vec<CastEvent>,
}

impl SpellCastTracker {
    /// The attempt budget, seconds.
    pub const LAUNCH_TIMEOUT_SECONDS: f64 = 5.0;
    /// The result timer's tick, seconds.
    pub const RESULT_TICK_SECONDS: f64 = 0.907;
    /// The result budget in whole ticks (4500 ms over the 907 ms tick).
    pub const RESULT_TICK_BUDGET: i32 = 4;
    /// The result budget, seconds.
    pub const RESULT_TIMEOUT_SECONDS: f64 =
        Self::RESULT_TICK_BUDGET as f64 * Self::RESULT_TICK_SECONDS;
    /// The longest a cast keeps the tracker busy.
    pub const WORST_CASE_BUSY_SECONDS: f64 =
        Self::LAUNCH_TIMEOUT_SECONDS + Self::RESULT_TIMEOUT_SECONDS;
    /// Item enchantment: its result names the item, not a creature.
    pub const ITEM_ENCHANTMENT_SCHOOL: u32 = 32;
    /// War magic's skill id.
    pub const WAR_MAGIC_SCHOOL: u32 = 34;
    /// Void magic's skill id.
    pub const VOID_MAGIC_SCHOOL: u32 = 43;
    /// How long finishing one attack school holds the other off, seconds.
    pub const CROSS_SCHOOL_LOCKOUT_SECONDS: f64 = 5.5;
    /// The gap between re-issues of an unacknowledged cast, seconds.
    pub const REISSUE_INTERVAL_SECONDS: f64 = 0.2;
    /// The gap when mana is nearly out, seconds.
    pub const LOW_MANA_REISSUE_INTERVAL_SECONDS: f64 = 0.1;
    /// Mana below which the shorter gap is used.
    pub const LOW_MANA_THRESHOLD: i32 = 10;

    /// An idle tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reissue_interval: Self::REISSUE_INTERVAL_SECONDS,
            next_reissue_at: Self::REISSUE_INTERVAL_SECONDS,
            ..Self::default()
        }
    }

    /// The shared cooldown table the cross-school lockout lives in.
    /// Unbound, the lockout is neither armed nor refused for.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = Some(locks);
    }

    /// Whether a spell of this school may not be issued, because a cast of
    /// the other attack school has just finished.
    #[must_use]
    pub fn is_school_locked_out(&self, school: u32) -> bool {
        let Some(locks) = &self.action_locks else {
            return false;
        };
        let kind = match school {
            Self::VOID_MAGIC_SCHOOL => ActionLockKind::VoidSpellLockedOut,
            Self::WAR_MAGIC_SCHOOL => ActionLockKind::WarSpellLockedOut,
            _ => return false,
        };
        locks.borrow_mut().is_locked(kind)
    }

    /// Whether a spell's cast may claim a killing blow: only these, so a
    /// fellow's or a pet's kill line arriving mid-debuff is not credited to
    /// the debuff's target.
    #[must_use]
    pub fn can_kill_for(spell: &SpellInfo) -> bool {
        spell.school == Self::WAR_MAGIC_SCHOOL
            || matches!(spell.family, 640 | 639)
            || matches!(
                spell.saying.as_str(),
                "feazhzhapaj" | "equinzhapaj" | "tugakquati"
            )
    }

    /// Whether a spell hits several targets.
    #[must_use]
    pub fn hits_multiple_targets_for(spell: &SpellInfo) -> bool {
        spell.formula_component_ids.first() == Some(&110)
            || spell.saying == "tugakquati"
            || spell.family == 638
    }

    /// The one short window after a cast is confirmed in flight during
    /// which the macro nudges itself.
    #[must_use]
    pub fn jiggle_window_open(&self) -> bool {
        self.state == SpellCastTrackerState::AwaitingResult
            && self.result_elapsed < Self::RESULT_TICK_SECONDS
    }

    /// Where the cast is.
    #[must_use]
    pub fn state(&self) -> SpellCastTrackerState {
        self.state
    }

    /// Whether a cast is in flight.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.state != SpellCastTrackerState::Idle
    }

    /// The spell in flight.
    #[must_use]
    pub fn spell_id(&self) -> u32 {
        self.spell_id
    }

    /// Its target.
    #[must_use]
    pub fn target_object_id(&self) -> u32 {
        self.target_object_id
    }

    /// Whether it hits several targets.
    #[must_use]
    pub fn hits_multiple_targets(&self) -> bool {
        self.hits_multiple_targets
    }

    /// The events queued since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<CastEvent> {
        std::mem::take(&mut self.events)
    }

    /// Starts waiting for a cast just requested.
    pub fn begin(&mut self, request: &CastRequest<'_>) {
        self.state = SpellCastTrackerState::AwaitingLaunch;
        self.saying = normalize(request.saying);
        self.spell_id = request.spell_id;
        self.spell_name = request.spell_name.to_owned();
        self.target_object_id = request.target_object_id;
        self.target_incarnation = request.target_incarnation;
        // An item enchantment's success line names the item, not the
        // creature it is worn by: there is no target name to check.
        self.target_name = if request.school == Self::ITEM_ENCHANTMENT_SCHOOL {
            String::new()
        } else {
            request.target_name.to_owned()
        };
        self.hits_multiple_targets = request.hits_multiple_targets;
        self.launch_elapsed = 0.0;
        self.result_elapsed = 0.0;
        self.outcome = None;
        self.school = request.school;
        self.can_kill = request.can_kill;
        // Nearly out of mana, the client is re-poked twice as often.
        self.reissue_interval = if request.current_mana < Self::LOW_MANA_THRESHOLD {
            Self::LOW_MANA_REISSUE_INTERVAL_SECONDS
        } else {
            Self::REISSUE_INTERVAL_SECONDS
        };
        self.next_reissue_at = self.reissue_interval;
    }

    /// Drops the cast in flight (arming the cross-school lockout when it had
    /// launched).
    pub fn reset(&mut self) {
        self.arm_cross_school_lockout();
        let locks = self.action_locks.take();
        let events = std::mem::take(&mut self.events);
        *self = Self::new();
        self.action_locks = locks;
        self.events = events;
    }

    /// Finishing a launched cast holds the other attack school off; one
    /// that never left the ground arms nothing.
    fn arm_cross_school_lockout(&self) {
        if self.state != SpellCastTrackerState::AwaitingResult {
            return;
        }
        let Some(locks) = &self.action_locks else {
            return;
        };
        if self.school == Self::VOID_MAGIC_SCHOOL {
            locks.borrow_mut().arm(
                ActionLockKind::WarSpellLockedOut,
                Self::CROSS_SCHOOL_LOCKOUT_SECONDS,
            );
        } else if self.school == Self::WAR_MAGIC_SCHOOL {
            locks.borrow_mut().arm(
                ActionLockKind::VoidSpellLockedOut,
                Self::CROSS_SCHOOL_LOCKOUT_SECONDS,
            );
        }
    }

    /// A deleted object ends the wait only when it is the awaited target.
    pub fn reset_for_target(&mut self, object_id: u32) {
        if self.is_busy() && object_id != 0 && object_id == self.target_object_id {
            self.reset();
        }
    }

    /// Reads one chat line. `log_text_type` is the client log it came from:
    /// the kill sentence is a plain line and every spell result a magic one,
    /// which keeps a player typing "You killed Drudge!" from ending the wait.
    pub fn observe_chat(
        &mut self,
        sequence: u64,
        text: &str,
        own_speech: bool,
        log_text_type: u32,
    ) {
        if !self.is_busy() || text.is_empty() {
            return;
        }
        if sequence != 0 && sequence <= self.observed_chat_sequence {
            return;
        }
        if sequence != 0 {
            self.observed_chat_sequence = sequence;
        }

        // The gesture echo is the character's own words logged as
        // spellcasting: the same words typed carry the plain type, and
        // somebody else's gesture is not ours.
        if self.state == SpellCastTrackerState::AwaitingLaunch
            && !self.saying.is_empty()
            && own_speech
            && log_text_type == log_text_type::SPELLCASTING
        {
            if normalize(text) == self.saying {
                self.state = SpellCastTrackerState::AwaitingResult;
                self.result_elapsed = 0.0;
                self.events.push(CastEvent::Answered(self.target_object_id));
            } else {
                // A different spell was spoken: this attempt is over.
                self.reset();
            }
            return;
        }

        // Result text belongs to this attempt only after its spell words.
        if self.state != SpellCastTrackerState::AwaitingResult {
            return;
        }

        let result = combat_text::classify(text);
        match result.class {
            CombatResultTextClass::None => {}
            CombatResultTextClass::Kill => {
                // Only a spell that can kill claims a killing blow.
                if !self.can_kill || log_text_type != log_text_type::DEFAULT {
                    return;
                }
                self.complete(SpellCastOutcome::Kill, 0, text);
            }
            // A spell's own result is logged as magic; anything else
            // wearing those words is somebody talking.
            _ if log_text_type != log_text_type::MAGIC => {}
            CombatResultTextClass::PermanentFail => {
                // The failure classes match on the sentence alone.
                if self.hits_multiple_targets {
                    return;
                }
                self.complete(SpellCastOutcome::PermanentFail, 0, text);
            }
            CombatResultTextClass::Fail => self.complete(SpellCastOutcome::Fail, 0, text),
            CombatResultTextClass::Success => {
                // Success names the spell and the target, so it is the one
                // class checked against them.
                if !result.spell_name.is_empty()
                    && !self.spell_name.is_empty()
                    && result.spell_name != self.spell_name
                {
                    return;
                }
                if !result.target_name.is_empty()
                    && !self.target_name.is_empty()
                    && !ordinal::equals_ignore_case(&result.target_name, &self.target_name)
                {
                    return;
                }
                self.complete(SpellCastOutcome::Success, 0, text);
            }
        }
    }

    /// Moves the cast's clocks on: re-issues an unacknowledged cast every
    /// interval until the budget runs out, and times the result out.
    pub fn advance(&mut self, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        match self.state {
            SpellCastTrackerState::Idle => {}
            SpellCastTrackerState::AwaitingLaunch => {
                self.launch_elapsed += elapsed;
                while self.launch_elapsed >= self.next_reissue_at {
                    if self.next_reissue_at >= Self::LAUNCH_TIMEOUT_SECONDS {
                        self.complete(SpellCastOutcome::LaunchTimeout, 0, "");
                        return;
                    }
                    self.next_reissue_at += self.reissue_interval;
                    self.events.push(CastEvent::Reissue {
                        spell_id: self.spell_id,
                        target_object_id: self.target_object_id,
                    });
                    self.events
                        .push(CastEvent::Attempted(self.target_object_id));
                }
                if self.launch_elapsed >= Self::LAUNCH_TIMEOUT_SECONDS {
                    self.complete(SpellCastOutcome::LaunchTimeout, 0, "");
                }
            }
            SpellCastTrackerState::AwaitingResult => {
                self.result_elapsed += elapsed;
                if self.result_elapsed >= Self::RESULT_TIMEOUT_SECONDS {
                    self.complete(SpellCastOutcome::ResultTimeout, 0, "");
                }
            }
        }
    }

    /// The last outcome, once.
    pub fn try_consume_outcome(&mut self) -> Option<SpellCastOutcomeInfo> {
        self.outcome.take()
    }

    fn complete(&mut self, outcome: SpellCastOutcome, weenie_error: u32, text: &str) {
        self.arm_cross_school_lockout();
        let info = SpellCastOutcomeInfo {
            outcome,
            spell_id: self.spell_id,
            spell_name: self.spell_name.clone(),
            target_object_id: self.target_object_id,
            target_name: self.target_name.clone(),
            hits_multiple_targets: self.hits_multiple_targets,
            weenie_error,
            text: text.to_owned(),
            target_incarnation: self.target_incarnation,
        };
        self.state = SpellCastTrackerState::Idle;
        self.launch_elapsed = 0.0;
        self.result_elapsed = 0.0;
        self.outcome = Some(info.clone());
        self.events.push(CastEvent::Completed(info));
    }
}

/// Spoken lines are compared without spaces or case.
fn normalize(text: &str) -> String {
    casing::to_lower_invariant(&text.replace(' ', ""))
}

/// The tracker as the rules share it (one plugin, one thread).
pub type SharedCastTracker = std::rc::Rc<std::cell::RefCell<SpellCastTracker>>;
