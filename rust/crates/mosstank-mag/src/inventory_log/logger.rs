//! When the log is written ([`InventoryLogger`]): Mag-Tools'
//! `InventoryLogger` handlers, staged so no call of the plugin does more
//! than a tick's worth.
//!
//! - After the login, once the client has listed the whole inventory and
//!   every item in it is described (the listing can come first): the
//!   last save is read (one tick), opened (the next), its items read
//!   [`READ_ITEMS_PER_TICK`] a tick; then every owned item is recorded,
//!   [`ITEMS_PER_TICK`] a tick, each combined with its record in the last
//!   save, and the log is written. An item that needs an appraisal and has none, here or in the
//!   last save, is asked for through the identify queue.
//! - When every item asked for has come in: written again.
//! - A new item that needs an appraisal, directly in the main pack, is
//!   asked for once.
//! - The kept list follows the inventory through the session (the items
//!   changed are recorded again, a tick's worth at a time), so logging off
//!   records again only the last ones changed, then writes it once.
//!
//! A log of an unknown version is left as it is: nothing is written over it.
//!
//! Where it differs from Mag-Tools:
//! - The log is written right after the login's records whether or not a
//!   log existed (Mag-Tools, with none, wrote only once every appraisal
//!   was in), and Mag-Tools waited for the appraisals it asked for only
//!   when no log existed; here it also waits, and writes again when they
//!   are in, when the last save had items without one. The first-run chat
//!   lines are still only for a character with no log yet.
//! - Logging off before the login's records are all made writes nothing,
//!   so the last save stays whole.

use std::collections::{HashMap, HashSet, VecDeque};

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, ObjectClass};

use super::file::{LogHeader, LogReader, ReadError, VERSION, storage_key, write};
use super::record::{ItemRecord, combine, same_item};
use crate::host::{MagHost, described_owned_items};
use crate::identify::IdentifyQueue;

/// Items recorded a tick.
pub const ITEMS_PER_TICK: usize = 100;

/// The items changed since they were recorded that the logoff records
/// again before its write: a tick's batch (a whole heavy inventory is
/// several times what the call may run; the rest is left, and said).
pub const LOGOFF_ITEMS: usize = ITEMS_PER_TICK;

/// The last save's items read a tick.
pub const READ_ITEMS_PER_TICK: usize = 100;

/// Mag-Tools' own chat lines (`Debug.WriteToChat`): this prefix, in the
/// system colour.
const CHAT_PREFIX: &str = "<{Mag-Tools}>: ";
const CHAT_TYPE: u32 = 5;

const FIRST_RUN: &str =
    "Requesting id information for all armor/weapon inventory. This will take a few minutes...";
const FIRST_RUN_DONE: &str =
    "Requesting id information for all armor/weapon inventory completed. Log file written.";
const CORRUPT: &str = "Inventory file is corrupt.";

/// `ObjectClassNeedsIdent`: the kinds whose log is worth an appraisal.
#[must_use]
pub fn needs_ident(class: ObjectClass, name: &str) -> bool {
    match class {
        ObjectClass::Armor
        | ObjectClass::Clothing
        | ObjectClass::MeleeWeapon
        | ObjectClass::MissileWeapon
        | ObjectClass::WandStaffOrb
        | ObjectClass::Jewelry => true,
        // Aetheria are gems; summoning essences are misc.
        ObjectClass::Gem => name.contains("Aetheria"),
        ObjectClass::Misc => name.contains("Essence"),
        _ => false,
    }
}

/// The last save's records, by id and class.
type Previous = HashMap<(u32, u8), ItemRecord>;

/// Where the logger is in the session.
#[derive(Debug, Default)]
enum Stage {
    /// Waiting for the login and the whole inventory.
    #[default]
    Idle,
    /// The last save read, its items not yet found.
    Opening(String),
    /// Reading the last save's items.
    Reading {
        reader: Box<LogReader>,
        previous: Previous,
    },
    /// Recording the owned items.
    Building {
        previous: Previous,
        pending: VecDeque<InventoryItem>,
    },
    /// The kept list is whole and follows the inventory.
    Ready,
    /// The log is not ours to write (an unknown version, or unreadable).
    Refused,
}

/// One owned item's record, its JSON text, and the appraisal time the
/// world reported for it (the session's own clock) when it was recorded.
#[derive(Debug)]
struct Kept {
    record: ItemRecord,
    json: String,
    session_id_time: i32,
}

impl Kept {
    fn new(record: ItemRecord, session_id_time: i32) -> Self {
        Self {
            json: record.to_json(),
            record,
            session_id_time,
        }
    }
}

/// The inventory logger of the character in the world.
#[derive(Debug, Default)]
pub struct InventoryLogger {
    logged_in: bool,
    stage: Stage,
    kept: Vec<Kept>,
    index: HashMap<u32, usize>,
    player_id: u32,
    character: String,
    world: String,
    key: String,
    /// No log existed at the login (Mag-Tools' first-run lines).
    first_run: bool,
    /// `requestedIds`: new items asked for; never cleared in a session.
    requested: HashSet<u32>,
    /// The items asked for at the login whose appraisal has not come in.
    awaiting: HashSet<u32>,
    /// `loggedInAndWaitingForIdData`.
    waiting: bool,
    /// Items changed since they were recorded, in the order heard.
    dirty: VecDeque<u32>,
    dirty_set: HashSet<u32>,
}

impl InventoryLogger {
    /// The login completed.
    pub fn on_login_complete(&mut self) {
        self.logged_in = true;
    }

    /// The character left the world: everything is forgotten.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The kept records: the inventory as last recorded.
    pub fn records(&self) -> impl Iterator<Item = &ItemRecord> {
        self.kept.iter().map(|kept| &kept.record)
    }

    /// The kept list is whole (the login's records are all made).
    #[must_use]
    pub fn is_built(&self) -> bool {
        matches!(self.stage, Stage::Ready)
    }

    /// An object changed (`CreateObject`, `ChangeObject`): a kept item is
    /// recorded again on a later tick; a new item that needs an appraisal,
    /// directly in the main pack, is asked for once.
    pub fn on_object_changed(
        &mut self,
        change: &ObjectChange,
        queue: &mut IdentifyQueue,
        enabled: bool,
    ) {
        if !enabled || !self.logged_in {
            return;
        }
        let recording = matches!(
            self.stage,
            Stage::Opening(_) | Stage::Reading { .. } | Stage::Building { .. } | Stage::Ready
        );
        if !recording {
            return;
        }
        let id = change.object_id;
        let owned = change.current.as_ref().is_some_and(|o| o.is_owned);
        // Before the list is whole any change counts: an item still to be
        // recorded may have left (the records made later drop it).
        if (!self.is_built() || self.index.contains_key(&id) || owned) && self.dirty_set.insert(id)
        {
            self.dirty.push_back(id);
        }
        // Mag-Tools asks on every creation, and on other changes once it is
        // not waiting for the login's appraisals.
        if change.kind != ObjectChangeKind::Created && self.waiting {
            return;
        }
        if let Some(object) = &change.current
            && self.player_id != 0
            && !object.has_appraisal_data
            && needs_ident(object.object_class, &object.name)
            && object.container_id == self.player_id
            && self.requested.insert(id)
        {
            queue.push(id);
        }
    }

    /// One tick: a tick's worth of reading, recording or recording again,
    /// and the saves that follow. True when it did work.
    pub fn tick(
        &mut self,
        host: &mut dyn MagHost,
        queue: &mut IdentifyQueue,
        enabled: bool,
    ) -> bool {
        if !enabled || !self.logged_in {
            return false;
        }
        match &mut self.stage {
            Stage::Idle => {
                // The whole inventory, every item in it described.
                let Some(owned) = described_owned_items(host) else {
                    return false;
                };
                // With no log yet, the records start in the same tick. No
                // character yet: nothing done.
                if self.start(host, &owned) {
                    self.build(host, queue, ITEMS_PER_TICK);
                }
                !matches!(self.stage, Stage::Idle)
            }
            Stage::Opening(text) => {
                let text = std::mem::take(text);
                self.open(host, text);
                true
            }
            Stage::Reading { reader, previous } => {
                match reader.read_items(READ_ITEMS_PER_TICK) {
                    Ok(records) => {
                        for record in records {
                            let key = (record.id, record.object_class.tag());
                            previous.entry(key).or_insert(record);
                        }
                        if reader.remaining() == 0 {
                            let previous = std::mem::take(previous);
                            self.begin_building(host, previous);
                        }
                    }
                    Err(_) => {
                        post(host, CORRUPT);
                        self.begin_building(host, Previous::new());
                    }
                }
                true
            }
            Stage::Building { .. } => {
                self.build(host, queue, ITEMS_PER_TICK);
                true
            }
            Stage::Ready => self.refresh(host, ITEMS_PER_TICK),
            Stage::Refused => false,
        }
    }

    /// Logging off: the items changed since they were recorded are recorded
    /// again (up to [`LOGOFF_ITEMS`]), then the kept list is written, once.
    /// A list not yet whole is not written (the last save stays).
    pub fn on_logoff(&mut self, host: &mut dyn MagHost, enabled: bool) {
        if !enabled || !self.logged_in {
            return;
        }
        self.logged_in = false;
        if !self.is_built() {
            return;
        }
        // The write follows here, so the recording does not write too.
        let waiting = std::mem::take(&mut self.waiting);
        self.refresh(host, LOGOFF_ITEMS);
        if waiting && self.awaiting.is_empty() && self.first_run {
            post(host, FIRST_RUN_DONE);
        }
        if !self.dirty.is_empty() {
            host.log(
                LogLevel::Warn,
                &format!(
                    "The inventory log {} was written without {} items changed just before the logoff.",
                    self.key,
                    self.dirty.len()
                ),
            );
        }
        self.save(host);
    }

    /// The whole inventory is listed and described (`owned`): the
    /// character, its log's key, and the last save's text. True when there
    /// is no last save and the records begin.
    fn start(&mut self, host: &mut dyn MagHost, owned: &[InventoryItem]) -> bool {
        let Some(character) = host.character() else {
            return false;
        };
        self.player_id = character.id;
        self.key = storage_key(&character.world, &character.name);
        self.character = character.name;
        self.world = character.world;
        match host.storage().read_text(&self.key) {
            Ok(None) => {
                self.first_run = true;
                post(host, FIRST_RUN);
                self.stage = Stage::Building {
                    previous: Previous::new(),
                    pending: owned.iter().cloned().collect(),
                };
                true
            }
            Ok(Some(text)) => {
                self.stage = Stage::Opening(text);
                false
            }
            Err(error) => {
                host.log(
                    LogLevel::Warn,
                    &format!(
                        "The inventory log {} could not be read ({error}): it is left as it is.",
                        self.key
                    ),
                );
                self.stage = Stage::Refused;
                false
            }
        }
    }

    /// The last save's header, and where its items are.
    fn open(&mut self, host: &mut dyn MagHost, text: String) {
        match LogReader::new(text) {
            Ok(reader) => {
                self.stage = Stage::Reading {
                    reader: Box::new(reader),
                    previous: Previous::new(),
                };
            }
            Err(ReadError::Corrupt) => {
                post(host, CORRUPT);
                self.begin_building(host, Previous::new());
            }
            Err(ReadError::UnknownVersion(version)) => {
                host.log(
                    LogLevel::Warn,
                    &format!(
                        "The inventory log {} is of version {version:?}, not {VERSION}: it is left as it is.",
                        self.key
                    ),
                );
                self.stage = Stage::Refused;
            }
        }
    }

    fn begin_building(&mut self, host: &mut dyn MagHost, previous: Previous) {
        let pending: VecDeque<InventoryItem> = host.owned_items_shared().iter().cloned().collect();
        self.stage = Stage::Building { previous, pending };
    }

    /// Records up to `budget` owned items; the last one written, the list
    /// is whole and the log is written.
    fn build(&mut self, host: &mut dyn MagHost, queue: &mut IdentifyQueue, budget: usize) {
        let Stage::Building { previous, pending } = &mut self.stage else {
            return;
        };
        let utc = host.utc_unix_ms();
        for _ in 0..budget {
            let Some(item) = pending.pop_front() else {
                break;
            };
            let (live, session_id_time) = live_record(host, &item, utc, None);
            let older = previous.remove(&(live.id, live.object_class.tag()));
            let older_appraised = older.as_ref().is_some_and(|o| o.has_id_data);
            if !older_appraised && !live.has_id_data && needs_ident(live.object_class, &live.name) {
                queue.push(live.id);
                self.awaiting.insert(live.id);
            }
            let record = match older {
                Some(older) => combine(older, live),
                None => live,
            };
            if let Some(&at) = self.index.get(&record.id) {
                self.kept[at] = Kept::new(record, session_id_time);
            } else {
                self.index.insert(record.id, self.kept.len());
                self.kept.push(Kept::new(record, session_id_time));
            }
        }
        if !pending.is_empty() {
            return;
        }
        self.stage = Stage::Ready;
        self.save(host);
        if self.awaiting.is_empty() {
            if self.first_run {
                post(host, FIRST_RUN_DONE);
            }
        } else {
            self.waiting = true;
        }
    }

    /// Records again up to `budget` of the items changed; the last
    /// appraisal waited for in, the log is written again.
    fn refresh(&mut self, host: &mut dyn MagHost, budget: usize) -> bool {
        if self.dirty.is_empty() {
            return false;
        }
        let owned = host.owned_items_shared();
        let utc = host.utc_unix_ms();
        let mut complete = None;
        let mut later = Vec::new();
        let mut recorded = 0;
        for _ in 0..budget {
            let Some(id) = self.dirty.pop_front() else {
                break;
            };
            self.dirty_set.remove(&id);
            if let Some(item) = owned.iter().find(|item| item.object_id == id) {
                let at = self.index.get(&id).copied();
                let (live, session_id_time) =
                    live_record(host, item, utc, at.map(|at| &self.kept[at]));
                let record = match at {
                    Some(at) if same_item(&self.kept[at].record, &live) => {
                        combine(self.kept[at].record.clone(), live)
                    }
                    _ => live,
                };
                if record.has_id_data {
                    self.awaiting.remove(&id);
                }
                recorded += 1;
                let kept = Kept::new(record, session_id_time);
                match at {
                    Some(at) => self.kept[at] = kept,
                    None => {
                        self.index.insert(id, self.kept.len());
                        self.kept.push(kept);
                    }
                }
            } else if !self.index.contains_key(&id) {
                // Not the character's, and never recorded: nothing to do.
            } else if *complete.get_or_insert_with(|| host.owned_inventory_complete()) {
                // Gone: it is not in the inventory any more.
                self.awaiting.remove(&id);
                if let Some(at) = self.index.remove(&id) {
                    self.kept.remove(at);
                    for position in self.index.values_mut() {
                        if *position > at {
                            *position -= 1;
                        }
                    }
                }
            } else {
                later.push(id);
            }
        }
        for id in later {
            if self.dirty_set.insert(id) {
                self.dirty.push_back(id);
            }
        }
        let mut saved = false;
        if self.waiting && self.awaiting.is_empty() {
            self.waiting = false;
            self.save(host);
            saved = true;
            if self.first_run {
                post(host, FIRST_RUN_DONE);
            }
        }
        // A few changed items (an arrow shot, a pack moved) are a light
        // tick; a full batch or a write is a tick's worth.
        recorded >= budget || saved
    }

    /// Writes the kept list (`DumpInventoryToFile`'s write).
    fn save(&mut self, host: &mut dyn MagHost) {
        let header = LogHeader {
            version: VERSION,
            character: self.character.clone(),
            world: self.world.clone(),
            saved_unix_ms: host.utc_unix_ms(),
        };
        let text = write(&header, self.kept.iter().map(|kept| kept.json.as_str()));
        if let Err(error) = host.storage().write_text(&self.key, &text) {
            host.log(
                LogLevel::Warn,
                &format!("The inventory log {} was not written: {error}", self.key),
            );
        }
    }
}

/// The record of an owned item as the client has it now, and the
/// appraisal time the world reports for it. Its appraisal counts as taken
/// in now, unless `kept` recorded the same appraisal already.
fn live_record(
    host: &mut dyn MagHost,
    item: &InventoryItem,
    utc_ms: i64,
    kept: Option<&Kept>,
) -> (ItemRecord, i32) {
    let object = host.object(item.object_id);
    let properties = host.item_properties(item.object_id).unwrap_or_default();
    let session_id_time = object.as_ref().map_or(0, |o| o.last_id_time);
    let id_time = match kept {
        Some(kept) if kept.record.has_id_data && kept.session_id_time == session_id_time => {
            kept.record.last_id_time
        }
        _ => utc_ms,
    };
    let record = ItemRecord::from_live(item, object.as_ref(), &properties, id_time);
    (record, session_id_time)
}

fn post(host: &mut dyn MagHost, text: &str) {
    host.chat_post(&format!("{CHAT_PREFIX}{text}"), CHAT_TYPE);
}
