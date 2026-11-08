//! The identify queue the Mag features share ([`IdentifyQueue`]).
//!
//! The client keeps one appraisal in flight and queues none, so the Mag
//! features ask through one paced queue: at most one request every 499 ms
//! (the looter's pace), none while the looter is identifying, a busy slot
//! retried on a later turn, an item the client does not know dropped, and
//! an id already waiting not queued again.

use std::collections::VecDeque;

use ac_plugin_abi::items::ItemStatus;

use crate::host::MagHost;

/// The time between two requests, milliseconds (the looter's
/// `IDENTIFY_REQUEST_INTERVAL_SECONDS`, 0.499 s).
pub const REQUEST_INTERVAL_MS: f64 = 499.0;

/// The objects waiting for an appraisal, in the order they were asked for.
#[derive(Debug, Default)]
pub struct IdentifyQueue {
    pending: VecDeque<u32>,
    last_sent_ms: Option<f64>,
}

impl IdentifyQueue {
    /// Queues an object, unless it is waiting already.
    pub fn push(&mut self, object_id: u32) {
        if !self.contains(object_id) {
            self.pending.push_back(object_id);
        }
    }

    /// Takes an object out of the queue (its appraisal came, or it is no
    /// longer wanted).
    pub fn remove(&mut self, object_id: u32) {
        self.pending.retain(|&id| id != object_id);
    }

    /// Whether an object is waiting.
    #[must_use]
    pub fn contains(&self, object_id: u32) -> bool {
        self.pending.contains(&object_id)
    }

    /// Nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// How many objects are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Forgets every waiting object (the character left the world).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.last_sent_ms = None;
    }

    /// One turn: asks for the first waiting object's appraisal when the
    /// pace allows and the looter is not identifying (`looter_busy`).
    pub fn tick(&mut self, host: &mut dyn MagHost, looter_busy: bool) {
        if looter_busy {
            return;
        }
        let Some(&object_id) = self.pending.front() else {
            return;
        };
        let now = host.now_ms();
        if self
            .last_sent_ms
            .is_some_and(|sent| now - sent < REQUEST_INTERVAL_MS)
        {
            return;
        }
        self.last_sent_ms = Some(now);
        match host.identify(object_id).status {
            // The slot is taken, or the character is not in the world: the
            // object stays first for a later turn.
            ItemStatus::Busy | ItemStatus::Unavailable => {}
            _ => {
                self.pending.pop_front();
            }
        }
    }
}
