//! The named cooldown slots the rules share (`ActionLockTable`): each is a
//! "nobody may do this kind of thing until T" marker. The rule that starts
//! a slow operation arms its slot; every rule whose work would collide with
//! it refuses while the slot is up. The clock is the macro's own elapsed
//! seconds, advanced once per host frame, so a lock behaves the same in a
//! test and in a session.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A cooldown slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum ActionLockKind {
    Navigation,
    ItemUse,
    Salvage,
    CastDrainRing,
    TryingPortal,
    MeleeAttackShot,
    MeleeStopTimeout,
    SpreadLockTargetRequested,
    RandomHelperBuffLock,
    ManaStoneUse,
    RechargeLevelBoostHealth,
    RechargeLevelBoostStamina,
    RechargeLevelBoostMana,
    DoorOpening,
    FellowRecruit,
    WarSpellLockedOut,
    VoidSpellLockedOut,
    BuffCastRecast,
    CorpseOpenAttempt,
}

/// How long using an item holds [`ActionLockKind::ItemUse`].
pub mod item_use_lock {
    /// A one-shot use with nothing to wait for.
    pub const IMMEDIATE_SECONDS: f64 = 0.75;
    /// A use whose result the macro waits for: held for the wait and
    /// dropped the moment the transaction ends.
    pub const TRANSACTION_SECONDS: f64 = 1.5;
    /// A spell cast from a held item: its launch window, its result window,
    /// and two seconds' grace.
    pub const HELD_ITEM_CAST_SECONDS: f64 = 6.0 + 3.5 + 2.0;
}

/// One shared deadline table.
#[derive(Debug, Clone, Default)]
pub struct ActionLockTable {
    deadlines: HashMap<ActionLockKind, f64>,
    now: f64,
}

/// The table as the controllers share it (one plugin, one thread).
pub type SharedActionLocks = Rc<RefCell<ActionLockTable>>;

impl ActionLockTable {
    /// An empty table at time zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A new table to share.
    #[must_use]
    pub fn shared() -> SharedActionLocks {
        Rc::new(RefCell::new(Self::new()))
    }

    /// Seconds since the table was created.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Moves the clock on.
    pub fn advance(&mut self, elapsed_seconds: f64) {
        self.now += elapsed_seconds.max(0.0);
    }

    /// Pushes the clock to one owner's own reading of it, never back.
    pub fn advance_to(&mut self, now: f64) {
        if now > self.now {
            self.now = now;
        }
    }

    /// Arms a slot; an arm never shortens a lock already up.
    pub fn arm(&mut self, kind: ActionLockKind, seconds: f64) {
        let deadline = self.now + seconds.max(0.0);
        match self.deadlines.get_mut(&kind) {
            Some(existing) => {
                if deadline > *existing {
                    *existing = deadline;
                }
            }
            None => {
                self.deadlines.insert(kind, deadline);
            }
        }
    }

    /// Drops a slot before its deadline.
    pub fn release(&mut self, kind: ActionLockKind) {
        self.deadlines.remove(&kind);
    }

    /// Whether the slot is up (a deadline on this instant still counts); an
    /// expired one is pruned as it is read.
    pub fn is_locked(&mut self, kind: ActionLockKind) -> bool {
        let Some(&deadline) = self.deadlines.get(&kind) else {
            return false;
        };
        if deadline >= self.now {
            return true;
        }
        self.deadlines.remove(&kind);
        false
    }

    /// Drops every slot.
    pub fn clear_all(&mut self) {
        self.deadlines.clear();
    }
}
