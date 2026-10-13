//! `/ub clearbugged` (`ClearBuggedRun`): asks the server to describe every
//! item the character carries and lets go of the ones the server no longer
//! has, the "ghost" items a client keeps listing after the server lost them.
//!
//! A refusal alone proves little: the server also refuses an item made to
//! resist appraisal, and a repeat request inside five seconds of an
//! unsuccessful one. So an item counts as a ghost only when refused twice,
//! the second request going out at least [`RECHECK_DELAY_SECONDS`] after
//! the first refusal came back. Worn and wielded items are left alone, and
//! so is a pack that still lists contents. Requests go out one at a time, at
//! most one every [`IDENTIFY_INTERVAL_SECONDS`].

use std::collections::{HashMap, VecDeque};

use ac_plugin_abi::items::{AppraisalState, InventoryItem, ItemResult, ItemStatus};
use openac_plugin_compat::number;

use crate::chat::{self, tools};
use crate::host::UbHost;

/// The reference's identify queue sends one request every 499 ms.
pub const IDENTIFY_INTERVAL_SECONDS: f64 = 0.499;
/// How long after the first refusal the item is asked about again.
pub const RECHECK_DELAY_SECONDS: f64 = 5.0;
/// How long a run may go with no request out, no answer back and nothing
/// let go of before it ends.
pub const NO_PROGRESS_TIMEOUT_SECONDS: f64 = 30.0;
/// How long one request may go unanswered before the item is left alone.
pub const ANSWER_TIMEOUT_SECONDS: f64 = 10.0;
/// How often the run says how many items it still waits on.
pub const WAITING_LINE_INTERVAL_SECONDS: f64 = 10.0;

const NAME: &str = "ClearBugged";

/// What the run reaches of the client.
pub trait ClearBuggedClient {
    /// The character is in the world.
    fn in_world(&mut self) -> bool;
    /// The character's own id.
    fn own_object_id(&mut self) -> u32;
    /// Everything the character owns.
    fn owned_items(&mut self) -> Vec<InventoryItem>;
    /// An item command now would be answered busy.
    fn items_busy(&mut self) -> bool;
    /// Asks the server to describe an object.
    fn identify(&mut self, object_id: u32) -> ItemResult;
    /// The appraisal slot.
    fn appraisal(&mut self) -> AppraisalState;
    /// Lets go of an item the server no longer has.
    fn forget_stale_item(&mut self, object_id: u32) -> ItemResult;
}

impl<T: UbHost + ?Sized> ClearBuggedClient for T {
    fn in_world(&mut self) -> bool {
        self.character().is_some()
    }

    fn own_object_id(&mut self) -> u32 {
        self.character().map_or(0, |character| character.id)
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        mosstank_meta::host::MetaHost::owned_items(self)
    }

    fn items_busy(&mut self) -> bool {
        self.item_state().is_busy
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        mosstank_meta::host::MetaHost::identify(self, object_id)
    }

    fn appraisal(&mut self) -> AppraisalState {
        UbHost::appraisal(self)
    }

    fn forget_stale_item(&mut self, object_id: u32) -> ItemResult {
        UbHost::forget_stale_item(self, object_id)
    }
}

/// One `/ub clearbugged` run.
#[derive(Debug, Clone, Default)]
pub struct ClearBuggedRun {
    first_checks: VecDeque<u32>,
    rechecks: Vec<(u32, f64)>,
    to_forget: Vec<u32>,
    names: HashMap<u32, String>,
    running: bool,
    clock: f64,
    since_request: f64,
    awaiting: u32,
    awaiting_recheck: bool,
    awaiting_age: f64,
    awaiting_revision: i64,
    since_progress: f64,
    carried: usize,
    until_waiting_line: f64,
}

impl ClearBuggedRun {
    /// Nothing under way.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a run is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Starts a run over everything carried; refused while one is under way.
    /// The lines it says go to `write`.
    pub fn start<C: ClearBuggedClient + ?Sized>(
        &mut self,
        client: &mut C,
        write: &mut dyn FnMut(String),
    ) {
        if self.running {
            write(chat::tool_error(
                tools::INVENTORY_MANAGER,
                &format!("{NAME}: already running."),
            ));
            return;
        }
        if !client.in_world() {
            write(chat::tool_error(
                tools::INVENTORY_MANAGER,
                &format!("{NAME}: not in the world."),
            ));
            return;
        }
        self.reset();
        let owned = client.owned_items();
        let own = client.own_object_id();
        // The count is everything carried, as the reference counts it; only
        // what can safely be let go of is asked about.
        let carried = owned
            .iter()
            .filter(|item| item.object_id != 0 && item.object_id != own)
            .count();
        for item in &owned {
            if !is_candidate(item, &owned, own) {
                continue;
            }
            self.first_checks.push_back(item.object_id);
            self.names.insert(item.object_id, item.name.clone());
        }
        self.running = true;
        self.carried = carried;
        self.since_request = IDENTIFY_INTERVAL_SECONDS;
        write(chat::tool(
            tools::INVENTORY_MANAGER,
            &format!("{NAME}: Identifying {carried} items, to check for bugged items..."),
        ));
        // The reference's appraiser announces the job it was handed.
        write(chat::line(&format!(
            "new Assessor.Job initialized with {carried} items..."
        )));
        self.finish_when_done(write);
    }

    /// Drives the run; nothing while none is under way.
    pub fn tick<C: ClearBuggedClient + ?Sized>(
        &mut self,
        client: &mut C,
        elapsed_seconds: f64,
        write: &mut dyn FnMut(String),
    ) {
        if !self.running {
            return;
        }
        self.since_progress += elapsed_seconds;
        if self.since_progress >= NO_PROGRESS_TIMEOUT_SECONDS {
            self.finish(write);
            return;
        }
        if !client.in_world() {
            self.say_waiting_when_due(elapsed_seconds, write);
            return;
        }
        self.clock += elapsed_seconds;
        self.since_request += elapsed_seconds;
        self.forget_confirmed_ghosts(client, write);
        if self.awaiting != 0 {
            self.awaiting_age += elapsed_seconds;
            self.poll_answer(client);
        }
        if self.awaiting == 0 && self.since_request >= IDENTIFY_INTERVAL_SECONDS {
            self.request_next(client);
        }
        self.finish_when_done(write);
        self.say_waiting_when_due(elapsed_seconds, write);
    }

    /// Drops a run without a word, as a session that ends does.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The items still to be settled, each in exactly one place.
    fn pending(&self) -> usize {
        self.first_checks.len()
            + self.rechecks.len()
            + self.to_forget.len()
            + usize::from(self.awaiting != 0)
    }

    /// The appraiser's progress line, on the first turn and every ten
    /// seconds after (due once the interval has passed, not on the mark).
    fn say_waiting_when_due(&mut self, elapsed_seconds: f64, write: &mut dyn FnMut(String)) {
        if !self.running {
            return;
        }
        self.until_waiting_line -= elapsed_seconds;
        if self.until_waiting_line >= 0.0 {
            return;
        }
        self.until_waiting_line = WAITING_LINE_INTERVAL_SECONDS;
        let pending = self.pending();
        #[allow(clippy::cast_precision_loss)]
        let seconds = pending as f64 * IDENTIFY_INTERVAL_SECONDS;
        write(chat::line(&format!(
            "Assessor waiting to ID {pending} of {} items. This will take about {} seconds.",
            self.carried,
            number::format_f64(seconds, "n2").expect("a number format the kit prints")
        )));
    }

    fn request_next<C: ClearBuggedClient + ?Sized>(&mut self, client: &mut C) {
        let mut due: Option<usize> = None;
        for (index, (_, due_at)) in self.rechecks.iter().enumerate() {
            if *due_at <= self.clock && due.is_none_or(|best| *due_at < self.rechecks[best].1) {
                due = Some(index);
            }
        }
        let object_id = match due {
            Some(index) => self.rechecks[index].0,
            None => match self.first_checks.front() {
                Some(id) => *id,
                None => return,
            },
        };
        let recheck = due.is_some();

        // An item moved out of reach since the run began is no longer asked
        // about.
        if !is_still_candidate(client, object_id) {
            self.consume(due);
            return;
        }

        let asked = client.identify(object_id);
        self.since_request = 0.0;
        if asked.status == ItemStatus::Busy {
            return; // Another request holds the slot; asked again next turn.
        }
        self.consume(due);
        self.since_progress = 0.0;
        if !asked.accepted() {
            return;
        }
        self.awaiting = object_id;
        self.awaiting_recheck = recheck;
        self.awaiting_age = 0.0;
        self.awaiting_revision = client.appraisal().revision;
    }

    fn consume(&mut self, due: Option<usize>) {
        match due {
            Some(index) => {
                self.rechecks.remove(index);
            }
            None => {
                self.first_checks.pop_front();
            }
        }
    }

    /// Reads the slot for the answer to the request in flight: a change to
    /// the slot that completes this item. A slot that merely stops
    /// reporting the wait has not changed.
    fn poll_answer<C: ClearBuggedClient + ?Sized>(&mut self, client: &mut C) {
        let appraisal = client.appraisal();
        if appraisal.last_abandoned_id == self.awaiting {
            self.give_up_on_answer();
            return;
        }
        if appraisal.awaiting_id == self.awaiting {
            self.awaiting_revision = appraisal.revision;
        } else if appraisal.current_id == self.awaiting
            && appraisal.revision != self.awaiting_revision
        {
            self.answered(appraisal.current_object_unsuccessful);
            return;
        }
        if self.awaiting_age >= ANSWER_TIMEOUT_SECONDS {
            self.give_up_on_answer();
        }
    }

    fn answered(&mut self, refused: bool) {
        let object_id = self.awaiting;
        let recheck = self.awaiting_recheck;
        self.awaiting = 0;
        self.awaiting_recheck = false;
        self.since_progress = 0.0;
        if !refused {
            return;
        }
        if recheck {
            self.to_forget.push(object_id);
        } else {
            self.rechecks
                .push((object_id, self.clock + RECHECK_DELAY_SECONDS));
        }
    }

    fn give_up_on_answer(&mut self) {
        self.awaiting = 0;
        self.awaiting_recheck = false;
        self.since_progress = 0.0;
    }

    fn forget_confirmed_ghosts<C: ClearBuggedClient + ?Sized>(
        &mut self,
        client: &mut C,
        write: &mut dyn FnMut(String),
    ) {
        while !self.to_forget.is_empty() {
            if client.items_busy() {
                return;
            }
            let object_id = self.to_forget[0];
            let name = self.names.get(&object_id).cloned().unwrap_or_default();
            let result = client.forget_stale_item(object_id);
            if result.status == ItemStatus::Busy {
                return;
            }
            self.to_forget.remove(0);
            self.since_progress = 0.0;
            if result.status == ItemStatus::Completed {
                // The reference's appraiser reports each one as its error.
                write(chat::tool_error(
                    tools::ASSESSOR,
                    &format!("Bugged Item: 0x{object_id:08X} {name}"),
                ));
            } else {
                let why = match result.notice.as_deref() {
                    Some(notice) if !notice.trim().is_empty() => notice.to_owned(),
                    _ => format!("{:?}", result.status),
                };
                write(chat::tool_error(
                    tools::INVENTORY_MANAGER,
                    &format!("{NAME}: could not remove 0x{object_id:08X} {name}: {why}"),
                ));
            }
        }
    }

    fn finish_when_done(&mut self, write: &mut dyn FnMut(String)) {
        if !self.running
            || self.awaiting != 0
            || !self.first_checks.is_empty()
            || !self.rechecks.is_empty()
            || !self.to_forget.is_empty()
        {
            return;
        }
        self.finish(write);
    }

    /// Ends the run with the reference's own last line.
    fn finish(&mut self, write: &mut dyn FnMut(String)) {
        self.reset();
        write(chat::tool(
            tools::INVENTORY_MANAGER,
            &format!("{NAME}: Done!"),
        ));
    }
}

/// Carried, not the character, not worn or wielded, and not a pack that
/// still lists contents.
fn is_candidate(item: &InventoryItem, owned: &[InventoryItem], own: u32) -> bool {
    if item.object_id == 0 || item.object_id == own {
        return false;
    }
    if item.equipped_location != 0 || item.wielder_id != 0 {
        return false;
    }
    !owned
        .iter()
        .any(|other| other.container_id == item.object_id)
}

fn is_still_candidate<C: ClearBuggedClient + ?Sized>(client: &mut C, object_id: u32) -> bool {
    let owned = client.owned_items();
    let own = client.own_object_id();
    owned
        .iter()
        .find(|item| item.object_id == object_id)
        .is_some_and(|item| is_candidate(item, &owned, own))
}
