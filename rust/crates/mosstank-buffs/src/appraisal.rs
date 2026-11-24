//! Asking the client to appraise items a rule wants and has never seen
//! appraised ([`AppraisalRequests`]): one question out at a time; one that
//! is not answered is let go after a while, and its item held back before
//! it is asked about again, so an item that never answers is not asked
//! about on every pass.

use std::collections::HashMap;

use mosstank_automation::Automation;

/// How long an appraisal request is waited for before it is let go,
/// seconds.
pub const REPLY_SECONDS: f64 = 10.0;
/// How long an item whose request went unanswered is held back before it
/// is asked about again, seconds.
pub const UNANSWERED_HOLD_SECONDS: f64 = 60.0;
/// How long an item whose request the client did not take is held back,
/// seconds.
pub const NOT_TAKEN_HOLD_SECONDS: f64 = 5.0;

/// The question out: the item, the appraisal stamp it had when asked, and
/// when.
#[derive(Debug, Clone, Copy)]
struct Asked {
    object_id: u32,
    version: i32,
    at: f64,
}

/// The appraisal requests one rule makes.
#[derive(Debug, Default)]
pub struct AppraisalRequests {
    asked: Option<Asked>,
    held_until: HashMap<u32, f64>,
}

impl AppraisalRequests {
    /// Nothing asked.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes in the answer to the question out, or lets it go once it has
    /// waited [`REPLY_SECONDS`] (its item then waits
    /// [`UNANSWERED_HOLD_SECONDS`]). `now` is the rule's clock, seconds.
    pub fn observe(&mut self, automation: &mut dyn Automation, now: f64) {
        if !self.held_until.is_empty() {
            self.held_until.retain(|_, &mut until| until > now);
        }
        let Some(asked) = self.asked else {
            return;
        };
        let current = assessment_version(automation, asked.object_id);
        if current != 0 && current != asked.version {
            self.asked = None;
        } else if now - asked.at >= REPLY_SECONDS {
            self.held_until
                .insert(asked.object_id, now + UNANSWERED_HOLD_SECONDS);
            self.asked = None;
        }
    }

    /// Whether a question is out.
    #[must_use]
    pub fn is_waiting(&self) -> bool {
        self.asked.is_some()
    }

    /// Whether `object_id` may be asked about now: nothing else is out and
    /// it is not held back.
    #[must_use]
    pub fn can_ask(&self, object_id: u32, now: f64) -> bool {
        self.asked.is_none()
            && self
                .held_until
                .get(&object_id)
                .is_none_or(|&until| until <= now)
    }

    /// Asks the client to appraise `object_id` when [`Self::can_ask`];
    /// whether the request was sent.
    pub fn ask(&mut self, automation: &mut dyn Automation, object_id: u32, now: f64) -> bool {
        if object_id == 0 || !self.can_ask(object_id, now) {
            return false;
        }
        let version = assessment_version(automation, object_id);
        if !automation.identify(object_id).accepted() {
            self.held_until
                .insert(object_id, now + NOT_TAKEN_HOLD_SECONDS);
            return false;
        }
        self.held_until.remove(&object_id);
        self.asked = Some(Asked {
            object_id,
            version,
            at: now,
        });
        true
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        self.asked = None;
        self.held_until.clear();
    }
}

/// The client's appraisal stamp of an object (0 when it has none).
fn assessment_version(automation: &mut dyn Automation, object_id: u32) -> i32 {
    automation
        .object(object_id)
        .map_or(0, |object| object.last_id_time)
}
