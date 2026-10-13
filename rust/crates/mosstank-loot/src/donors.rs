//! The mana donor journal (`ManaDonorJournal`): permission to drain an item
//! is given only to items the looter itself collected for their mana, kept
//! per server and character in the plugin's storage, and never inferred
//! from what happens to be in the pack. A drain is written down before it
//! is sent, so an interrupted session never repeats a destructive use.

use ac_plugin_abi::items::InventoryItem;
use mosstank_dotnet::collections::{DotNetDictionary, Ordinal};
use mosstank_dotnet::text::casing;
use mosstank_stores::stj::{self, Object};
use openac_plugin_compat::json::{self, JsonValue, system_text_json};
use sha2::{Digest, Sha256};

use crate::mana::ManaStoneTransferPlan;

/// Where the journal keeps its file and reports what went wrong (the C#'s
/// `IPluginStorage` and `IPluginLogger`).
pub trait DonorStore {
    /// Whether the storage can read and write at all.
    fn is_available(&mut self) -> bool;

    /// The text under `key`, `None` when nothing is stored there.
    ///
    /// # Errors
    /// The file exists but cannot be read.
    fn read_text(&mut self, key: &str) -> Result<Option<String>, String>;

    /// Replaces what is under `key`.
    ///
    /// # Errors
    /// The file cannot be written.
    fn write_text(&mut self, key: &str, content: &str) -> Result<(), String>;

    /// An information line in the client's log.
    fn info(&mut self, message: &str);

    /// A failure in the client's log.
    fn error(&mut self, message: &str);
}

/// One item the looter collected for its mana.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonorEntry {
    /// The item.
    pub object_id: u32,
    /// Its weenie class when it was collected.
    pub class_id: u32,
    /// Its name when it was collected.
    pub name: String,
    /// The stone a drain into which went out unconfirmed; 0 for none.
    pub stone_id: u32,
    /// It left the character's hands (or turned into something else).
    pub removed: bool,
    /// Why it may not be drained, `None` when it may.
    pub blocked_reason: Option<String>,
}

/// The journal.
#[derive(Debug, Default)]
pub struct ManaDonorJournal {
    entries: DotNetDictionary<u32, DonorEntry, Ordinal<u32>>,
    scope: Option<String>,
    key: Option<String>,
    failed: bool,
    identity_ready: bool,
    changes: Vec<String>,
}

impl ManaDonorJournal {
    /// An empty journal, bound to nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The donors, in the journal's order.
    pub fn entries(&self) -> impl Iterator<Item = &DonorEntry> {
        self.entries.values()
    }

    /// The lines reported since the last call (the C#'s `Changed`).
    pub fn take_changes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.changes)
    }

    /// Whether drains may be planned: nothing failed, and the storage is
    /// either absent or bound to a character.
    pub fn can_operate(&self, store: &mut dyn DonorStore) -> bool {
        !self.failed && (!store.is_available() || self.identity_ready)
    }

    /// Binds the journal to a server and character, reading its file when
    /// the pair changed.
    pub fn bind(&mut self, store: &mut dyn DonorStore, server: &str, character: &str) {
        self.identity_ready =
            !casing::is_white_space_only(server) && !casing::is_white_space_only(character);
        if !self.identity_ready {
            return;
        }
        let scope = format!(
            "{}\0{}",
            casing::to_upper_invariant(trim(server)),
            casing::to_upper_invariant(trim(character))
        );
        if self.scope.as_deref() == Some(scope.as_str()) {
            return;
        }
        let digest = Sha256::digest(scope.as_bytes());
        let hex: String = digest.iter().map(|byte| format!("{byte:02X}")).collect();
        let key = format!("mana-donors/{hex}.json");
        self.scope = Some(scope);
        self.key = Some(key.clone());
        self.entries.clear();
        self.failed = false;
        if !store.is_available() {
            return;
        }
        let read = store
            .read_text(&key)
            .and_then(|text| text.map(|text| read_entries(&text)).transpose());
        match read {
            Ok(None) => {}
            Ok(Some(entries)) => {
                for entry in entries {
                    self.entries.insert(entry.object_id, entry);
                }
            }
            Err(error) => {
                self.entries.clear();
                self.fail(
                    store,
                    "Could not read mana donor history; draining is disabled.",
                    &error,
                );
            }
        }
    }

    /// Records an item collected for its mana.
    pub fn remember(&mut self, store: &mut dyn DonorStore, item: &InventoryItem) -> bool {
        if self.failed || item.object_id == 0 {
            return false;
        }
        self.entries.insert(
            item.object_id,
            DonorEntry {
                object_id: item.object_id,
                class_id: item.weenie_class_id,
                name: item.name.clone(),
                stone_id: 0,
                removed: false,
                blocked_reason: None,
            },
        );
        self.save(store)
    }

    /// Writes the drain down before it is sent; false when it may not go.
    pub fn begin(&mut self, store: &mut dyn DonorStore, plan: &ManaStoneTransferPlan) -> bool {
        if !self.donor_ready(store, plan.tank_object_id) {
            return false;
        }
        let Some(entry) = self.entries.get_mut(&plan.tank_object_id) else {
            return false;
        };
        // Persist before sending: interruption between send and
        // acknowledgement must never make the next session repeat a
        // destructive request.
        entry.stone_id = plan.stone_object_id;
        entry.blocked_reason = Some("Awaiting inventory confirmation".to_owned());
        self.save(store)
    }

    /// The drain did not go out: the stone is free again, and the donor is
    /// held back with the reason (none: it may go again).
    pub fn not_sent(&mut self, store: &mut dyn DonorStore, donor: u32, reason: Option<&str>) {
        let Some(entry) = self.entries.get_mut(&donor) else {
            return;
        };
        entry.stone_id = 0;
        entry.blocked_reason = reason.map(str::to_owned);
        self.save(store);
    }

    /// The donor was drained.
    pub fn forget(&mut self, store: &mut dyn DonorStore, donor: u32) {
        if self.entries.remove(&donor).is_some() {
            self.save(store);
        }
    }

    /// Whether the donor may be drained now.
    pub fn donor_ready(&self, store: &mut dyn DonorStore, object_id: u32) -> bool {
        self.can_operate(store)
            && self.entries.get(&object_id).is_some_and(|entry| {
                !entry.removed && entry.stone_id == 0 && entry.blocked_reason.is_none()
            })
    }

    /// Whether a drain into this stone is still unconfirmed.
    #[must_use]
    pub fn stone_held(&self, object_id: u32) -> bool {
        self.entries
            .values()
            .any(|entry| entry.stone_id == object_id)
    }

    /// Settles the journal against the pack: an item that turned into
    /// something else, or is gone from a complete inventory, loses its
    /// permission; a held stone that charged (or is gone from a complete
    /// inventory) is let go.
    pub fn reconcile(
        &mut self,
        store: &mut dyn DonorStore,
        owned: &[InventoryItem],
        complete: bool,
    ) {
        if self.failed {
            return;
        }
        let find = |id: u32| owned.iter().find(|item| item.object_id == id);
        let mut changed = false;
        let ids: Vec<u32> = self.entries.keys().copied().collect();
        for id in ids {
            let Some(entry) = self.entries.get_mut(&id) else {
                continue;
            };
            let donor = find(entry.object_id);
            if let Some(donor) = donor.filter(|_| !entry.removed)
                && donor.weenie_class_id != 0
                && !donor.name.is_empty()
                && (donor.weenie_class_id != entry.class_id || donor.name != entry.name)
            {
                entry.removed = true;
                changed = true;
            } else if donor.is_none() && complete && !entry.removed {
                entry.removed = true;
                changed = true;
                let message = format!(
                    "ManaDrain: {} is no longer owned; removed donor permission.",
                    entry.name
                );
                store.info(&message);
                self.changes.push(message);
            }

            let Some(entry) = self.entries.get_mut(&id) else {
                continue;
            };
            if entry.stone_id != 0 {
                let stone = find(entry.stone_id);
                // Late charge updates settle the held stone without
                // resending. A missing object proves nothing until all
                // packs are listed.
                if stone.is_some_and(|stone| stone.effects & 1 != 0)
                    || (stone.is_none() && complete)
                {
                    entry.stone_id = 0;
                    entry.blocked_reason = Some(
                        "Previous drain resolved; donor requires review if still owned".to_owned(),
                    );
                    changed = true;
                    let message =
                        format!("ManaDrain: reconciled the held stone for {}.", entry.name);
                    store.info(&message);
                    self.changes.push(message);
                }
            }
            if entry.removed && entry.stone_id == 0 {
                self.entries.remove(&id);
                changed = true;
            }
        }
        if changed {
            self.save(store);
        }
    }

    fn save(&mut self, store: &mut dyn DonorStore) -> bool {
        if self.failed {
            return false;
        }
        if !store.is_available() {
            return true;
        }
        let Some(key) = self.key.clone().filter(|_| self.identity_ready) else {
            return false;
        };
        let document = write_entries(self.entries.values());
        match store.write_text(&key, &document) {
            Ok(()) => true,
            Err(error) => {
                self.fail(
                    store,
                    "Could not save mana donor history; draining is disabled.",
                    &error,
                );
                false
            }
        }
    }

    fn fail(&mut self, store: &mut dyn DonorStore, message: &str, error: &str) {
        self.failed = true;
        store.error(&format!("{message} {error}"));
    }
}

fn trim(text: &str) -> &str {
    text.trim_matches(casing::is_white_space)
}

/// The file as `JsonSerializer.Serialize(Entry[])` writes it: compact,
/// properties in declaration order.
fn write_entries<'a>(entries: impl Iterator<Item = &'a DonorEntry>) -> String {
    let document = JsonValue::array(entries.map(|entry| {
        JsonValue::object([
            ("ObjectId", stj::uint(entry.object_id)),
            ("ClassId", stj::uint(entry.class_id)),
            ("Name", stj::string(&entry.name)),
            ("StoneId", stj::uint(entry.stone_id)),
            ("Removed", JsonValue::Bool(entry.removed)),
            (
                "BlockedReason",
                stj::string_or_null(entry.blocked_reason.as_deref()),
            ),
        ])
    }));
    system_text_json::to_string(&document, &system_text_json::Options::COMPACT)
        .expect("the donor list holds no numbers that cannot be written")
}

/// Reads the file; an entry without an identity, or a second entry for the
/// same item, refuses the whole file.
fn read_entries(text: &str) -> Result<Vec<DonorEntry>, String> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let items = match &document {
        JsonValue::Array(items) => items,
        JsonValue::Null => return Err("Missing donor list.".to_owned()),
        _ => {
            return Err(
                "The JSON value could not be converted to ManaDonorJournal+Entry[]. Path: $"
                    .to_owned(),
            );
        }
    };
    let mut entries: Vec<DonorEntry> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("$[{index}]");
        let object = Object::of(item, &path, "ManaDonorJournal+Entry")?
            .ok_or_else(|| "Invalid or duplicate donor identity.".to_owned())?
            .case_sensitive();
        let entry = DonorEntry {
            object_id: object.uint("ObjectId", 0)?,
            class_id: object.uint("ClassId", 0)?,
            name: object.string("Name", "")?.unwrap_or_default(),
            stone_id: object.uint("StoneId", 0)?,
            removed: object.bool("Removed", false)?,
            blocked_reason: object
                .string("BlockedReason", "")?
                .filter(|_| object.get("BlockedReason").is_some()),
        };
        if entry.object_id == 0
            || entry.class_id == 0
            || casing::is_white_space_only(&entry.name)
            || entries
                .iter()
                .any(|other| other.object_id == entry.object_id)
        {
            return Err("Invalid or duplicate donor identity.".to_owned());
        }
        entries.push(entry);
    }
    Ok(entries)
}
