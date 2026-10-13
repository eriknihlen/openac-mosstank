//! Which of the character's buffs a Force Buff has marked as run out
//! (`BuffDueTracker`): the polled form of the reference's two
//! active-enchantment hooks, plus the item half of the force.

use std::collections::HashSet;

use ac_plugin_abi::character::ActiveEnchantment;

/// The forced buffs.
#[derive(Debug, Clone, Default)]
pub struct BuffDueTracker {
    tracked: HashSet<u32>,
    forced: HashSet<u32>,
    forced_items: HashSet<(u32, u32)>,
}

impl BuffDueTracker {
    /// An empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The spells a force marked as run out.
    #[must_use]
    pub fn forced_spell_ids(&self) -> &HashSet<u32> {
        &self.forced
    }

    /// Whether any spell is forced.
    #[must_use]
    pub fn has_forced_entries(&self) -> bool {
        !self.forced.is_empty()
    }

    /// Whether any item row is forced.
    #[must_use]
    pub fn has_forced_items(&self) -> bool {
        !self.forced_items.is_empty()
    }

    /// Folds this pass's enchantments in: a new one starts unforced, one
    /// that left is dropped.
    pub fn observe(&mut self, active: &[ActiveEnchantment]) {
        let mut seen: Vec<u32> = Vec::new();
        for enchantment in active {
            if enchantment.spell_id == 0 {
                continue;
            }
            seen.push(enchantment.spell_id);
            if self.tracked.insert(enchantment.spell_id) {
                self.forced.remove(&enchantment.spell_id);
            }
        }
        let gone: Vec<u32> = self
            .tracked
            .iter()
            .filter(|spell_id| !seen.contains(spell_id))
            .copied()
            .collect();
        for spell_id in gone {
            self.tracked.remove(&spell_id);
            self.forced.remove(&spell_id);
        }
    }

    /// Force Buff: every tracked entry reads as about to expire.
    pub fn force_all(&mut self) {
        self.forced.extend(self.tracked.iter().copied());
    }

    /// Forces item rows (an item and a family).
    pub fn force_items(&mut self, rows: impl IntoIterator<Item = (u32, u32)>) {
        self.forced_items.extend(rows);
    }

    /// Whether an item's row in a family still reads as forced.
    #[must_use]
    pub fn is_item_forced(&self, item_object_id: u32, family: u32) -> bool {
        self.forced_items.contains(&(item_object_id, family))
    }

    /// An item row was recast.
    pub fn note_item_recast(&mut self, item_object_id: u32, family: u32) {
        self.forced_items.remove(&(item_object_id, family));
    }

    /// A spell was recast.
    pub fn note_recast(&mut self, spell_id: u32) {
        self.forced.remove(&spell_id);
    }

    /// Ends a force (the item ledger is restored too).
    pub fn cancel_force(&mut self) {
        self.forced.clear();
        self.forced_items.clear();
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        self.tracked.clear();
        self.forced.clear();
        self.forced_items.clear();
    }
}
