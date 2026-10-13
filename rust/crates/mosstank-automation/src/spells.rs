//! The spell table and the character's spellbook (`ISpellCatalog`).

use std::collections::HashMap;
use std::rc::Rc;

use ac_plugin_abi::magic::{ComponentInfo, SpellInfo};
use mosstank_dotnet::text::{casing, ordinal};

/// The spells the client knows (`ISpellCatalog`).
pub trait SpellCatalog {
    /// The self buffs the character knows.
    fn known_self_buffs(&mut self) -> Vec<SpellInfo>;

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
    /// The first spell of each name, by its ordinal ignore-case key.
    by_name: HashMap<String, usize>,
}

/// The text `StringComparison.OrdinalIgnoreCase` compares in place of
/// `text`: two texts are equal ignoring case exactly when these are equal.
fn ignore_case_key(text: &str) -> String {
    if text.is_ascii() {
        // An ASCII character's key is its ASCII upper case.
        return text.to_ascii_uppercase();
    }
    text.chars().map(casing::ordinal_ignore_case_key).collect()
}

impl SpellTable {
    /// The table `spells`, in the client's order.
    #[must_use]
    pub fn new(spells: Vec<SpellInfo>) -> Self {
        let mut by_name = HashMap::with_capacity(spells.len());
        for (index, spell) in spells.iter().enumerate() {
            by_name.entry(ignore_case_key(&spell.name)).or_insert(index);
        }
        Self { spells, by_name }
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
        if let Some(&index) = self.by_name.get(&ignore_case_key(name)) {
            return self.spells.get(index);
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
