//! The item-enchantment timers a character has seen cast, kept across
//! sessions in the plugin's own storage (`item-enchant-timers/`), one file
//! per server and character that repeats that identity in its body. A file
//! that does not match, does not read, or was written after the clock's
//! "now" (the clock went back) is deleted rather than trusted.

use mosstank_dotnet::text::{casing, ordinal};
use openac_plugin_compat::json::{self, JsonValue};

use crate::recovery::sha256_hex;
use crate::stj::{self, JsonError, Object};
use crate::storage::PluginStorage;

const CURRENT_VERSION: i32 = 1;
const FOLDER: &str = "item-enchant-timers";

/// The earliest and latest moments .NET's `DateTimeOffset` holds, in Unix
/// milliseconds.
const MIN_UNIX_MS: i64 = -62_135_596_800_000;
const MAX_UNIX_MS: i64 = 253_402_300_799_999;

/// Whose timers a file holds: a server and a character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEnchantScope {
    /// The server's name.
    pub server_name: String,
    /// The character's name.
    pub character_name: String,
}

impl ItemEnchantScope {
    /// The scope of a server and character, trimmed.
    #[must_use]
    pub fn new(server_name: Option<&str>, character_name: Option<&str>) -> Self {
        Self {
            server_name: server_name.map(str::trim).unwrap_or_default().to_owned(),
            character_name: character_name.map(str::trim).unwrap_or_default().to_owned(),
        }
    }

    /// Whether both names are known.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.server_name.is_empty() && !self.character_name.is_empty()
    }

    /// Whether `other` names the same server and character (any case).
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        ordinal::equals_ignore_case(&self.server_name, &other.server_name)
            && ordinal::equals_ignore_case(&self.character_name, &other.character_name)
    }

    /// The file this scope's timers are kept in.
    #[must_use]
    pub fn key(&self) -> String {
        let normalized = format!(
            "{}\0{}",
            casing::to_upper_invariant(&self.server_name),
            casing::to_upper_invariant(&self.character_name)
        );
        format!("{FOLDER}/{}.json", sha256_hex(&normalized))
    }
}

/// One cast seen on an item: the spell, its family, and when it ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEnchantRecord {
    /// The item's object id.
    pub item_object_id: u32,
    /// The item's weenie class.
    pub weenie_class_id: u32,
    /// The item's name.
    pub item_name: String,
    /// The spell's family.
    pub family: u32,
    /// The spell.
    pub spell_id: u32,
    /// The spell's power.
    pub quality: i32,
    /// When it was seen, in Unix milliseconds.
    pub recorded_at_unix_ms: i64,
    /// When it ends, in Unix milliseconds.
    pub expires_at_unix_ms: i64,
}

/// What a load found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemEnchantLoadStatus {
    /// No storage, or no character yet.
    Unavailable,
    /// No timers kept.
    Missing,
    /// The timers read.
    Loaded,
    /// The file is from after now: the clock went back, and it was deleted.
    ClockRollback,
    /// The file did not match or did not read, and was deleted.
    Invalid,
}

/// A load's outcome: the timers still running, whether the file should be
/// written again (rows were dropped), and why it was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEnchantLoadResult {
    /// What was found.
    pub status: ItemEnchantLoadStatus,
    /// The running timers.
    pub records: Vec<ItemEnchantRecord>,
    /// Rows were dropped, so the file should be written again.
    pub requires_rewrite: bool,
    /// Why the file was rejected.
    pub error: Option<String>,
}

impl ItemEnchantLoadResult {
    fn empty(status: ItemEnchantLoadStatus, error: Option<&str>) -> Self {
        Self {
            status,
            records: Vec::new(),
            requires_rewrite: false,
            error: error.map(str::to_owned),
        }
    }
}

struct Row {
    item_object_id: u32,
    weenie_class_id: u32,
    item_name: Option<String>,
    family: u32,
    spell_id: u32,
    quality: i32,
    recorded_at: i64,
    expires_at: i64,
}

struct Document {
    version: i32,
    server_name: Option<String>,
    character_name: Option<String>,
    saved_at: i64,
    entries: Option<Vec<Option<Row>>>,
}

fn read_document(text: &str) -> Result<Option<Document>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "Document")? else {
        return Ok(None);
    };
    // No PropertyNameCaseInsensitive here: the camel-case names match exactly.
    let o = o.case_sensitive();
    let entries = match o.get("entries") {
        None => Some(Vec::new()),
        Some(JsonValue::Null) => None,
        Some(JsonValue::Array(items)) => Some(
            items
                .iter()
                .map(|item| -> Result<Option<Row>, JsonError> {
                    let Some(row) = Object::of(item, "$.entries", "Row")? else {
                        return Ok(None);
                    };
                    let row = row.case_sensitive();
                    Ok(Some(Row {
                        item_object_id: row.uint("itemObjectId", 0)?,
                        weenie_class_id: row.uint("weenieClassId", 0)?,
                        item_name: row.string("itemName", "")?,
                        family: row.uint("family", 0)?,
                        spell_id: row.uint("spellId", 0)?,
                        quality: row.int("quality", 0)?,
                        recorded_at: row.long("recordedAtUnixMilliseconds", 0)?,
                        expires_at: row.long("expiresAtUnixMilliseconds", 0)?,
                    }))
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => {
            return Err(
                "The JSON value could not be converted to System.Collections.Generic.List`1[Row]. Path: $.entries"
                    .to_owned(),
            );
        }
    };
    Ok(Some(Document {
        version: o.int("version", 0)?,
        server_name: o.string("serverName", "")?,
        character_name: o.string("characterName", "")?,
        saved_at: o.long("savedAtUnixMilliseconds", 0)?,
        entries,
    }))
}

/// `DateTimeOffset.FromUnixTimeMilliseconds`'s range check.
fn checked_unix_ms(value: i64) -> Result<i64, String> {
    if (MIN_UNIX_MS..=MAX_UNIX_MS).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "Valid values are between {MIN_UNIX_MS} and {MAX_UNIX_MS}, inclusive. (Parameter 'milliseconds')"
        ))
    }
}

const ROLLBACK: &str = "The system clock is earlier than the stored timer observations.";

/// Reads a scope's timers, `now` in Unix milliseconds: those still running,
/// rows that are incomplete, expired or superseded dropped (and the file
/// marked for rewriting).
pub fn load(
    storage: &mut dyn PluginStorage,
    scope: &ItemEnchantScope,
    now_unix_ms: i64,
) -> ItemEnchantLoadResult {
    if !storage.is_available() || !scope.is_valid() {
        return ItemEnchantLoadResult::empty(ItemEnchantLoadStatus::Unavailable, None);
    }
    match try_load(storage, scope, now_unix_ms) {
        Ok(result) => result,
        Err(message) => {
            delete(storage, scope);
            ItemEnchantLoadResult::empty(ItemEnchantLoadStatus::Invalid, Some(&message))
        }
    }
}

fn try_load(
    storage: &mut dyn PluginStorage,
    scope: &ItemEnchantScope,
    now: i64,
) -> Result<ItemEnchantLoadResult, String> {
    let key = scope.key();
    let Some(text) = storage
        .read_text(&key)?
        .filter(|text| !casing::is_white_space_only(text))
    else {
        return Ok(ItemEnchantLoadResult::empty(
            ItemEnchantLoadStatus::Missing,
            None,
        ));
    };
    let document = read_document(&text)?;
    let valid = document.as_ref().filter(|document| {
        document.version == CURRENT_VERSION
            && document.entries.is_some()
            && scope.matches(&ItemEnchantScope::new(
                document.server_name.as_deref(),
                document.character_name.as_deref(),
            ))
    });
    let Some(document) = valid else {
        storage.delete(&key)?;
        return Ok(ItemEnchantLoadResult::empty(
            ItemEnchantLoadStatus::Invalid,
            Some("The stored timer scope or schema did not match."),
        ));
    };
    if checked_unix_ms(document.saved_at)? > now {
        return Ok(reject_rollback(storage, scope));
    }
    let mut rewrite = false;
    let mut records: Vec<ItemEnchantRecord> = Vec::new();
    for row in document.entries.iter().flatten() {
        let Some(row) = row else {
            return Err("Object reference not set to an instance of an object.".to_owned());
        };
        let recorded_at = checked_unix_ms(row.recorded_at)?;
        let expires_at = checked_unix_ms(row.expires_at)?;
        // A future observation means the clock moved behind a prior cast;
        // keeping only some rows would make the same rollback look valid.
        if recorded_at > now {
            return Ok(reject_rollback(storage, scope));
        }
        let item_name = row.item_name.clone().unwrap_or_default();
        if row.item_object_id == 0
            || row.weenie_class_id == 0
            || casing::is_white_space_only(&item_name)
            || row.family == 0
            || row.spell_id == 0
            || expires_at <= recorded_at
            || expires_at <= now
        {
            rewrite = true;
            continue;
        }
        let record = ItemEnchantRecord {
            item_object_id: row.item_object_id,
            weenie_class_id: row.weenie_class_id,
            item_name,
            family: row.family,
            spell_id: row.spell_id,
            quality: row.quality,
            recorded_at_unix_ms: recorded_at,
            expires_at_unix_ms: expires_at,
        };
        let same = |held: &ItemEnchantRecord| {
            held.item_object_id == record.item_object_id
                && held.family == record.family
                && held.spell_id == record.spell_id
        };
        match records.iter_mut().find(|held| same(held)) {
            Some(held) => {
                rewrite = true;
                if held.expires_at_unix_ms >= expires_at {
                    continue;
                }
                *held = record;
            }
            None => records.push(record),
        }
    }
    Ok(ItemEnchantLoadResult {
        status: ItemEnchantLoadStatus::Loaded,
        records,
        requires_rewrite: rewrite,
        error: None,
    })
}

fn reject_rollback(
    storage: &mut dyn PluginStorage,
    scope: &ItemEnchantScope,
) -> ItemEnchantLoadResult {
    delete(storage, scope);
    ItemEnchantLoadResult::empty(ItemEnchantLoadStatus::ClockRollback, Some(ROLLBACK))
}

/// Writes a scope's timers (deleting the file when there are none); false
/// when there is nowhere to write or the write failed.
pub fn save(
    storage: &mut dyn PluginStorage,
    scope: &ItemEnchantScope,
    records: &[ItemEnchantRecord],
    now_unix_ms: i64,
) -> bool {
    if !storage.is_available() || !scope.is_valid() {
        return false;
    }
    let key = scope.key();
    if records.is_empty() {
        return storage.delete(&key).is_ok();
    }
    let mut ordered: Vec<&ItemEnchantRecord> = records.iter().collect();
    ordered.sort_by_key(|record| (record.item_object_id, record.family, record.spell_id));
    let entries = ordered
        .into_iter()
        .map(|record| {
            JsonValue::object([
                ("itemObjectId", stj::uint(record.item_object_id)),
                ("weenieClassId", stj::uint(record.weenie_class_id)),
                ("itemName", stj::string(&record.item_name)),
                ("family", stj::uint(record.family)),
                ("spellId", stj::uint(record.spell_id)),
                ("quality", stj::int(record.quality)),
                (
                    "recordedAtUnixMilliseconds",
                    JsonValue::Number(json::JsonNumber::I64(record.recorded_at_unix_ms)),
                ),
                (
                    "expiresAtUnixMilliseconds",
                    JsonValue::Number(json::JsonNumber::I64(record.expires_at_unix_ms)),
                ),
            ])
        })
        .collect();
    let document = JsonValue::object([
        ("version", stj::int(CURRENT_VERSION)),
        ("serverName", stj::string(&scope.server_name)),
        ("characterName", stj::string(&scope.character_name)),
        (
            "savedAtUnixMilliseconds",
            JsonValue::Number(json::JsonNumber::I64(now_unix_ms)),
        ),
        ("entries", JsonValue::Array(entries)),
    ]);
    storage.write_text(&key, &stj::write(&document)).is_ok()
}

/// Deletes a scope's timers; whether there were any.
pub fn delete(storage: &mut dyn PluginStorage, scope: &ItemEnchantScope) -> bool {
    if !storage.is_available() || !scope.is_valid() {
        return false;
    }
    storage.delete(&scope.key()).unwrap_or(false)
}
