//! The spell table and the character's spellbook (`ISpellCatalog`).

use ac_plugin_abi::magic::{ComponentInfo, SpellInfo};
use openac_plugin_compat::text::{casing, ordinal};

/// The spells the client knows (`ISpellCatalog`).
pub trait SpellCatalog {
    /// The self buffs the character knows.
    fn known_self_buffs(&mut self) -> Vec<SpellInfo>;

    /// One spell of the whole table.
    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo>;

    /// The whole table, for a lookup by name; empty when the client does
    /// not hand it out.
    fn all(&mut self) -> Vec<SpellInfo> {
        Vec::new()
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
