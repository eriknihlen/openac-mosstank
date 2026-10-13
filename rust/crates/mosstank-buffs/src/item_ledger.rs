//! The spells cast on items and how long each has left
//! (`ItemEnchantLedger`): the client does not report an item's enchantments,
//! so the buff rule keeps its own table from the casts it saw land, on the
//! macro's clock, and keeps the ones with a durable item identity across
//! sessions in the plugin's storage (per server and character).
//!
//! The C# writes the file inside each change; here a change marks the
//! table for writing and [`ItemEnchantLedger::flush`] writes it while the
//! owner holds the client's storage (divergence STORE-6).

use ac_plugin_abi::items::InventoryItem;
use mosstank_dotnet::collections::{DotNetDictionary, Ordinal};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_stores::item_enchant_store::{
    self, ItemEnchantLoadResult, ItemEnchantLoadStatus, ItemEnchantRecord, ItemEnchantScope,
};
use mosstank_stores::storage::PluginStorage;
use openac_plugin_compat::number::format_f64;

type Key = (u32, u32, u32);

#[derive(Debug, Clone, Default)]
struct Entry {
    /// The due stamp a force zeroes.
    a: f64,
    /// The real expiry.
    b: f64,
    quality: i32,
    weenie_class_id: u32,
    item_name: String,
    recorded_at_unix_ms: i64,
    expires_at_unix_ms: i64,
}

/// What the table should do to its file at the next flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Write {
    Delete,
    Save(i64),
    DeleteThenSave(i64),
}

/// The item-enchantment timers.
#[derive(Debug, Default)]
pub struct ItemEnchantLedger {
    entries: DotNetDictionary<Key, Entry, Ordinal<Key>>,
    scope: Option<ItemEnchantScope>,
    utc_now_ms: i64,
    last_utc_observation: Option<i64>,
    write: Option<Write>,
}

impl ItemEnchantLedger {
    /// An empty table with nowhere to keep it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The wall clock, Unix milliseconds (the C#'s `TimeProvider`); the
    /// owner sets it at the start of each call.
    pub fn set_utc_now(&mut self, utc_now_ms: i64) {
        self.utc_now_ms = utc_now_ms;
    }

    /// Whether any entry's due stamp is still zeroed by a force.
    #[must_use]
    pub fn has_forced_entries(&self) -> bool {
        self.entries.values().any(|entry| entry.a < entry.b)
    }

    /// How many timers are kept.
    #[must_use]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Chooses the server and character the timers are kept for and
    /// restores their unexpired timers onto the macro's clock.
    pub fn bind_persistence(
        &mut self,
        storage: &mut dyn PluginStorage,
        server_name: Option<&str>,
        character_name: Option<&str>,
        now_seconds: f64,
        log: &mut dyn FnMut(&str),
    ) -> ItemEnchantLoadResult {
        // A change still waiting belongs to the scope being left: it is
        // written there before the timers are bound elsewhere, as the C#'s
        // write-through store has already written it.
        self.flush(storage);
        self.entries.clear();
        self.write = None;
        let scope = ItemEnchantScope::new(server_name, character_name);
        if !scope.is_valid() {
            self.scope = None;
            return ItemEnchantLoadResult {
                status: ItemEnchantLoadStatus::Unavailable,
                records: Vec::new(),
                requires_rewrite: false,
                error: None,
            };
        }
        let utc_now = self.utc_now_ms;
        self.last_utc_observation = Some(utc_now);
        let result = item_enchant_store::load(storage, &scope, utc_now);
        self.scope = Some(scope);
        for record in &result.records {
            #[allow(clippy::cast_precision_loss)]
            let remaining = (record.expires_at_unix_ms - utc_now) as f64 / 1000.0;
            if remaining <= 0.0 {
                continue;
            }
            let expiry = now_seconds + remaining;
            self.entries.insert(
                (record.item_object_id, record.family, record.spell_id),
                Entry {
                    a: expiry,
                    b: expiry,
                    quality: record.quality,
                    weenie_class_id: record.weenie_class_id,
                    item_name: record.item_name.clone(),
                    recorded_at_unix_ms: record.recorded_at_unix_ms,
                    expires_at_unix_ms: record.expires_at_unix_ms,
                },
            );
        }
        if result.requires_rewrite {
            self.persist(utc_now);
        }
        match result.status {
            ItemEnchantLoadStatus::ClockRollback => {
                log("Stored item spell timers were reset after a clock rollback.");
            }
            ItemEnchantLoadStatus::Invalid => {
                log("Stored item spell timers were invalid and have been reset.");
            }
            _ if !self.entries.is_empty() => {
                log(&format!(
                    "Restored {} item spell timer(s).",
                    self.entries.len()
                ));
            }
            _ => {}
        }
        result
    }

    /// Writes what changed, while the owner holds the storage.
    pub fn flush(&mut self, storage: &mut dyn PluginStorage) {
        let Some(write) = self.write.take() else {
            return;
        };
        let Some(scope) = self.scope.clone() else {
            return;
        };
        match write {
            Write::Delete => {
                item_enchant_store::delete(storage, &scope);
            }
            Write::Save(utc_now) => {
                // Persistence is advisory: a storage failure cannot stop the
                // macro.
                let _ = item_enchant_store::save(storage, &scope, &self.records(), utc_now);
            }
            Write::DeleteThenSave(utc_now) => {
                item_enchant_store::delete(storage, &scope);
                let _ = item_enchant_store::save(storage, &scope, &self.records(), utc_now);
            }
        }
    }

    /// A cast seen landing on an object with no durable identity (the
    /// character).
    #[allow(clippy::too_many_arguments)]
    pub fn note_cast(
        &mut self,
        item_object_id: u32,
        family: u32,
        spell_id: u32,
        quality: i32,
        duration_seconds: f64,
        now_seconds: f64,
        spell_name: &str,
        log: &mut dyn FnMut(&str),
    ) {
        self.note_cast_core(
            item_object_id,
            0,
            "",
            family,
            spell_id,
            quality,
            duration_seconds,
            now_seconds,
            spell_name,
            log,
        );
    }

    /// A cast seen landing on an owned item.
    #[allow(clippy::too_many_arguments)]
    pub fn note_item_cast(
        &mut self,
        item: &InventoryItem,
        family: u32,
        spell_id: u32,
        quality: i32,
        duration_seconds: f64,
        now_seconds: f64,
        spell_name: &str,
        log: &mut dyn FnMut(&str),
    ) {
        self.note_cast_core(
            item.object_id,
            item.weenie_class_id,
            &item.name,
            family,
            spell_id,
            quality,
            duration_seconds,
            now_seconds,
            spell_name,
            log,
        );
    }

    #[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
    fn note_cast_core(
        &mut self,
        item_object_id: u32,
        weenie_class_id: u32,
        item_name: &str,
        family: u32,
        spell_id: u32,
        quality: i32,
        duration_seconds: f64,
        now_seconds: f64,
        spell_name: &str,
        log: &mut dyn FnMut(&str),
    ) {
        // A spell with no duration is not tracked at all.
        if !duration_seconds.is_finite() || duration_seconds <= 0.0 || !now_seconds.is_finite() {
            return;
        }
        let utc_now = self.observe_utc_now();
        let expiry = now_seconds + duration_seconds;
        if !expiry.is_finite() {
            return;
        }
        let expires_at = utc_now as f64 + duration_seconds * 1000.0;
        if expires_at > 253_402_300_799_999.0 {
            return;
        }
        let expires_at_unix_ms = expires_at.round() as i64;
        let key = (item_object_id, family, spell_id);
        let mut has_entry = self.entries.contains_key(&key);
        if has_entry
            && weenie_class_id != 0
            && self.entries.get(&key).is_some_and(|entry| {
                entry.weenie_class_id != 0
                    && (entry.weenie_class_id != weenie_class_id
                        || !ordinal::equals_ignore_case(&entry.item_name, item_name))
            })
        {
            self.entries.remove(&key);
            has_entry = false;
        }
        let identity_enriched = has_entry
            && weenie_class_id != 0
            && self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.weenie_class_id == 0)
            && !casing::is_white_space_only(item_name);
        if identity_enriched && let Some(entry) = self.entries.get_mut(&key) {
            entry.weenie_class_id = weenie_class_id;
            entry.item_name = item_name.trim().to_owned();
        }
        if !has_entry {
            self.entries.insert(
                key,
                Entry {
                    a: f64::MIN,
                    b: f64::MIN,
                    ..Entry::default()
                },
            );
        }
        let expiry_text = format_f64(expiry, "0.###").unwrap_or_default();
        let current = self
            .entries
            .get_mut(&key)
            .unwrap_or_else(|| unreachable!("the entry was just made"));
        if current.b < expiry {
            log(&format!(
                "Cast {spell_name} on {item_object_id} ending at {expiry_text}, OVERRIDDEN"
            ));
            current.a = expiry;
            current.b = expiry;
            current.quality = quality;
            current.weenie_class_id = weenie_class_id;
            item_name.trim().clone_into(&mut current.item_name);
            current.recorded_at_unix_ms = utc_now;
            current.expires_at_unix_ms = expires_at_unix_ms;
            self.persist(utc_now);
            return;
        }
        log(&format!(
            "Cast {spell_name} on {item_object_id} ending at {expiry_text}, newer already present"
        ));
        if identity_enriched {
            self.persist(utc_now);
        }
    }

    /// The longest time left on the object for the family at this quality
    /// or better.
    #[must_use]
    pub fn remaining_seconds(
        &self,
        item_object_id: u32,
        family: u32,
        quality: i32,
        now_seconds: f64,
    ) -> f64 {
        let mut longest = 0.0;
        for (&(item, entry_family, _), entry) in self.entries.iter() {
            if item != item_object_id || entry_family != family || entry.quality < quality {
                continue;
            }
            let remaining = entry.a - now_seconds;
            if remaining > longest {
                longest = remaining;
            }
        }
        longest
    }

    /// The same for an owned item: timers kept under another item's
    /// identity (the id was reused) are dropped first.
    pub fn remaining_item_seconds(
        &mut self,
        item: &InventoryItem,
        family: u32,
        quality: i32,
        now_seconds: f64,
    ) -> f64 {
        if !self.matches_persisted_identity(item) {
            self.invalidate_item(item.object_id);
            return 0.0;
        }
        self.remaining_seconds(item.object_id, family, quality, now_seconds)
    }

    /// Force Buff: every entry reads as about to expire.
    pub fn force_all(&mut self, now_seconds: f64) {
        for key in self.keys() {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.a = now_seconds;
            }
        }
    }

    /// Ends a force: every entry's real time left comes back.
    pub fn cancel_force(&mut self) {
        for key in self.keys() {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.a = entry.b;
            }
        }
    }

    /// Drops the entries whose real expiry has passed.
    pub fn expire(&mut self, now_seconds: f64) {
        let utc_now = self.observe_utc_now();
        let gone: Vec<Key> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.b < now_seconds)
            .map(|(&key, _)| key)
            .collect();
        let changed = !gone.is_empty();
        for key in gone {
            self.entries.remove(&key);
        }
        if changed {
            self.persist(utc_now);
        }
    }

    /// Forgets every timer of one item.
    pub fn invalidate_item(&mut self, item_object_id: u32) {
        if item_object_id == 0 {
            return;
        }
        self.remove_where(|key| key.0 == item_object_id);
    }

    /// Forgets one family's timers on one item.
    pub fn invalidate(&mut self, item_object_id: u32, family: u32) {
        if item_object_id == 0 || family == 0 {
            return;
        }
        self.remove_where(|key| key.0 == item_object_id && key.1 == family);
    }

    /// Forgets everything, the kept file included (death, a dispel).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
        self.request_delete();
    }

    /// Session teardown.
    pub fn reset(&mut self) {
        self.entries.clear();
        self.scope = None;
        self.last_utc_observation = None;
        self.write = None;
    }

    fn keys(&self) -> Vec<Key> {
        self.entries.keys().copied().collect()
    }

    fn remove_where(&mut self, predicate: impl Fn(&Key) -> bool) {
        let gone: Vec<Key> = self
            .entries
            .keys()
            .filter(|key| predicate(key))
            .copied()
            .collect();
        let changed = !gone.is_empty();
        for key in gone {
            self.entries.remove(&key);
        }
        if changed {
            let utc_now = self.observe_utc_now();
            self.persist(utc_now);
        }
    }

    /// The wall clock; one that went back wipes the table and its file.
    fn observe_utc_now(&mut self) -> i64 {
        let utc_now = self.utc_now_ms;
        if let Some(previous) = self.last_utc_observation
            && utc_now < previous
        {
            self.entries.clear();
            self.request_delete();
        }
        self.last_utc_observation = Some(utc_now);
        utc_now
    }

    fn request_delete(&mut self) {
        if self.scope.is_some() {
            self.write = Some(Write::Delete);
        }
    }

    fn persist(&mut self, utc_now: i64) {
        if self.scope.is_none() {
            return;
        }
        self.write = Some(match self.write {
            Some(Write::Delete | Write::DeleteThenSave(_)) => Write::DeleteThenSave(utc_now),
            _ => Write::Save(utc_now),
        });
    }

    /// The rows with a durable item identity; the character's rows stay in
    /// the session only.
    fn records(&self) -> Vec<ItemEnchantRecord> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.weenie_class_id != 0 && !entry.item_name.is_empty())
            .map(|(&(item, family, spell_id), entry)| ItemEnchantRecord {
                item_object_id: item,
                weenie_class_id: entry.weenie_class_id,
                item_name: entry.item_name.clone(),
                family,
                spell_id,
                quality: entry.quality,
                recorded_at_unix_ms: entry.recorded_at_unix_ms,
                expires_at_unix_ms: entry.expires_at_unix_ms,
            })
            .collect()
    }

    fn matches_persisted_identity(&self, item: &InventoryItem) -> bool {
        for (&(object_id, _, _), entry) in self.entries.iter() {
            if object_id != item.object_id {
                continue;
            }
            // Session-only rows carry no stamp.
            if entry.weenie_class_id == 0 && entry.item_name.is_empty() {
                continue;
            }
            if entry.weenie_class_id != item.weenie_class_id
                || !ordinal::equals_ignore_case(&entry.item_name, item.name.trim())
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::items::InventoryItem;
    use mosstank_stores::item_enchant_store::ItemEnchantScope;
    use mosstank_stores::storage::{MemoryStorage, PluginStorage};

    use super::*;

    /// A timer noted for one character and not yet flushed is written to
    /// that character's file when the timers are bound to another one: the
    /// C#'s store writes through, so nothing noted is lost at a rebind.
    #[test]
    fn a_change_still_waiting_is_written_to_its_own_scope_before_a_rebind() {
        let mut storage = MemoryStorage::new();
        let mut ledger = ItemEnchantLedger::new();
        ledger.set_utc_now(1_700_000_000_000);
        ledger.bind_persistence(
            &mut storage,
            Some("Coldeve"),
            Some("Alpha"),
            0.0,
            &mut |_| {},
        );
        let item = InventoryItem {
            object_id: 0x5000_0001,
            weenie_class_id: 1234,
            name: "Wand".to_owned(),
            ..InventoryItem::default()
        };
        ledger.note_item_cast(&item, 7, 1001, 1, 3600.0, 0.0, "Defender", &mut |_| {});

        ledger.bind_persistence(
            &mut storage,
            Some("Coldeve"),
            Some("Beta"),
            0.0,
            &mut |_| {},
        );

        let alpha = ItemEnchantScope::new(Some("Coldeve"), Some("Alpha")).key();
        assert!(storage.read_text(&alpha).unwrap().is_some());
    }
}
