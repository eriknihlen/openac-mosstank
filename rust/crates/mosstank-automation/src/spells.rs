//! The spell table and the character's spellbook (`ISpellCatalog`).

use std::collections::HashMap;
use std::rc::Rc;

use ac_plugin_abi::magic::{ComponentInfo, SpellInfo};
use mosstank_dotnet::text::{casing, ordinal};

/// The spells the client knows (`ISpellCatalog`).
pub trait SpellCatalog {
    /// The self buffs the character knows.
    fn known_self_buffs(&mut self) -> Vec<SpellInfo>;

    /// The ids of [`Self::known_self_buffs`], in its order.
    fn known_self_buff_ids(&mut self) -> Vec<u32> {
        self.known_self_buffs()
            .iter()
            .map(|spell| spell.spell_id)
            .collect()
    }

    /// One spell of the whole table.
    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo>;

    /// The war and void attack spells the character knows
    /// (`KnownAttackSpells`).
    fn known_attack_spells(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    /// Every hostile spell the character knows, debuffs included
    /// (`KnownCombatSpells`).
    fn known_combat_spells(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    /// [`Self::known_self_buffs`], shared: a client that keeps the list
    /// hands out the one it keeps instead of a copy.
    fn known_self_buffs_shared(&mut self) -> Rc<[SpellInfo]> {
        self.known_self_buffs().into()
    }

    /// [`Self::known_attack_spells`], shared.
    fn known_attack_spells_shared(&mut self) -> Rc<[SpellInfo]> {
        self.known_attack_spells().into()
    }

    /// [`Self::known_combat_spells`], shared.
    fn known_combat_spells_shared(&mut self) -> Rc<[SpellInfo]> {
        self.known_combat_spells().into()
    }

    /// Whether the character knows the spell.
    fn is_known(&mut self, spell_id: u32) -> bool {
        let _ = spell_id;
        false
    }

    /// Seconds left on a shared cooldown (an item's property 280).
    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        let _ = cooldown_id;
        0.0
    }

    /// The whole table, for a lookup by name; empty when the client does
    /// not hand it out.
    fn all(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    /// The whole table, shared, for lookups by name and id: [`Self::all`]
    /// read into a [`SpellTable`]. A client that keeps the table hands out
    /// the one it keeps.
    fn table(&mut self) -> Rc<SpellTable> {
        Rc::new(SpellTable::new(self.all()))
    }

    /// One spell component.
    fn try_get_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        let _ = component_id;
        None
    }

    /// A spell by name (`TryFindByName`): the first whose name is the same
    /// ignoring case, else with `partial_match` the first whose name
    /// contains it.
    fn try_find_by_name(&mut self, name: &str, partial_match: bool) -> Option<SpellInfo> {
        if casing::is_white_space_only(name) {
            return None;
        }
        let all = self.all();
        if let Some(spell) = all
            .iter()
            .find(|spell| ordinal::equals_ignore_case(&spell.name, name))
        {
            return Some(spell.clone());
        }
        if !partial_match {
            return None;
        }
        all.into_iter()
            .find(|spell| ordinal::contains_ignore_case(&spell.name, name))
    }
}

/// A catalogue of a fixed list: the whole table, nothing known as a self
/// buff unless listed as one. For tests and for a caller that has the list.
#[derive(Debug, Clone, Default)]
pub struct StaticSpellCatalog {
    /// Every spell of the table.
    pub spells: Vec<SpellInfo>,
    /// The character's self buffs.
    pub self_buffs: Vec<SpellInfo>,
    /// The components.
    pub components: Vec<ComponentInfo>,
}

impl StaticSpellCatalog {
    /// A table of `spells`, no self buffs.
    #[must_use]
    pub fn of(spells: Vec<SpellInfo>) -> Self {
        Self {
            spells,
            ..Self::default()
        }
    }
}

impl SpellCatalog for StaticSpellCatalog {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        self.self_buffs.clone()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.spells
            .iter()
            .chain(&self.self_buffs)
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
    }

    fn all(&mut self) -> Vec<SpellInfo> {
        self.spells.clone()
    }

    fn try_get_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        self.components
            .iter()
            .find(|component| component.component_id == component_id)
            .cloned()
    }
}

/// The client's whole spell table, read once and looked up by name.
///
/// Through the client each read of the table decodes all of it (some six
/// thousand spells, page by page), where the C# reads an object in its own
/// memory; the table is the client's static data, so the plugin reads it
/// once and keeps it ([`SpellCatalog::try_find_by_name`] answers from it
/// exactly as from a fresh read).
#[derive(Debug, Clone, Default)]
pub struct SpellTable {
    spells: Vec<SpellInfo>,
    /// The first spell of each name, by the hash of its ordinal
    /// ignore-case form: the spells of one hash with different forms each
    /// keep their first.
    by_name: HashMap<u64, NameBucket, BuildFoldedHash>,
}

/// The spells of one name hash: the first of each ignore-case form.
#[derive(Debug, Clone)]
enum NameBucket {
    One(usize),
    Many(Vec<usize>),
}

impl NameBucket {
    fn indexes(&self) -> &[usize] {
        match self {
            Self::One(index) => std::slice::from_ref(index),
            Self::Many(indexes) => indexes,
        }
    }
}

/// The map's keys are hashes already: they are used as they are.
#[derive(Debug, Clone, Copy, Default)]
struct BuildFoldedHash;

impl std::hash::BuildHasher for BuildFoldedHash {
    type Hasher = FoldedHasher;
    fn build_hasher(&self) -> FoldedHasher {
        FoldedHasher(0)
    }
}

/// Passes a `u64` key through (FNV-1a for anything else).
#[derive(Debug, Clone, Copy)]
struct FoldedHasher(u64);

impl std::hash::Hasher for FoldedHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(FNV_PRIME);
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The characters `StringComparison.OrdinalIgnoreCase` compares in place
/// of `text`'s: two texts are equal ignoring case exactly when these are.
fn folded(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars().map(|c| {
        if c.is_ascii() {
            // An ASCII character's form is its ASCII upper case.
            c.to_ascii_uppercase()
        } else {
            casing::ordinal_ignore_case_key(c)
        }
    })
}

/// The hash of `text`'s ignore-case form.
fn folded_hash(text: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    if text.is_ascii() {
        // An ASCII character is its byte: the same hash, byte by byte.
        for byte in text.bytes() {
            hash = (hash ^ u64::from(byte.to_ascii_uppercase())).wrapping_mul(FNV_PRIME);
        }
        return hash;
    }
    for c in folded(text) {
        hash = (hash ^ u64::from(u32::from(c))).wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Whether two texts have the same ignore-case form.
fn same_folded(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    folded(a).eq(folded(b))
}

impl SpellTable {
    /// The table `spells`, in the client's order.
    #[must_use]
    pub fn new(spells: Vec<SpellInfo>) -> Self {
        let mut by_name: HashMap<u64, NameBucket, BuildFoldedHash> =
            HashMap::with_capacity_and_hasher(spells.len(), BuildFoldedHash);
        for (index, spell) in spells.iter().enumerate() {
            match by_name.entry(folded_hash(&spell.name)) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(NameBucket::One(index));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let bucket = entry.get_mut();
                    if bucket
                        .indexes()
                        .iter()
                        .any(|&kept| same_folded(&spells[kept].name, &spell.name))
                    {
                        continue;
                    }
                    let mut indexes = bucket.indexes().to_vec();
                    indexes.push(index);
                    *bucket = NameBucket::Many(indexes);
                }
            }
        }
        Self { spells, by_name }
    }

    /// The first spell named `name` ignoring case.
    fn by_folded_name(&self, name: &str) -> Option<&SpellInfo> {
        let bucket = self.by_name.get(&folded_hash(name))?;
        bucket
            .indexes()
            .iter()
            .map(|&index| &self.spells[index])
            .find(|spell| same_folded(&spell.name, name))
    }

    /// Every spell, in the client's order.
    #[must_use]
    pub fn spells(&self) -> &[SpellInfo] {
        &self.spells
    }

    /// Whether the table has no spells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spells.is_empty()
    }

    /// A spell by name as [`SpellCatalog::try_find_by_name`] finds it in
    /// this table: the first whose name is the same ignoring case, else
    /// with `partial_match` the first whose name contains it.
    #[must_use]
    pub fn find_by_name(&self, name: &str, partial_match: bool) -> Option<&SpellInfo> {
        if casing::is_white_space_only(name) {
            return None;
        }
        if let Some(spell) = self.by_folded_name(name) {
            return Some(spell);
        }
        if !partial_match {
            return None;
        }
        self.spells
            .iter()
            .find(|spell| ordinal::contains_ignore_case(&spell.name, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spell(spell_id: u32, name: &str) -> SpellInfo {
        SpellInfo {
            spell_id,
            name: name.to_owned(),
            ..SpellInfo::default()
        }
    }

    #[test]
    fn an_ascii_character_is_its_own_ignore_case_key_upper_cased() {
        for code in 0u8..128 {
            let c = char::from(code);
            assert_eq!(
                casing::ordinal_ignore_case_key(c),
                c.to_ascii_uppercase(),
                "{code:#x}"
            );
        }
    }

    // The table answers every lookup as the scan of a fresh read does.
    #[test]
    fn the_table_finds_what_a_scan_of_the_whole_table_finds() {
        let spells = vec![
            spell(1, "Strength Self I"),
            spell(2, "STRENGTH SELF I"),
            spell(3, "Adja's Blessing"),
            spell(4, "Straße Self"),
            spell(5, "Ærfaldt's Gift"),
            spell(6, "\u{17F}tar"),
            spell(7, "Incantation of Strength Self"),
            spell(8, ""),
        ];
        let table = SpellTable::new(spells.clone());
        let mut scan = StaticSpellCatalog::of(spells);
        let names = [
            "strength self i",
            "Strength Self I",
            "adja's blessing",
            "STRASSE SELF",
            "straße self",
            "ærfaldt's gift",
            "ÆRFALDT'S GIFT",
            "STAR",
            "\u{17F}TAR",
            "Strength",
            "self",
            "",
            "   ",
            "nothing like it",
        ];
        for name in names {
            for partial in [false, true] {
                assert_eq!(
                    table.find_by_name(name, partial).map(|s| s.spell_id),
                    scan.try_find_by_name(name, partial).map(|s| s.spell_id),
                    "{name:?} partial {partial}"
                );
            }
        }
    }
}
