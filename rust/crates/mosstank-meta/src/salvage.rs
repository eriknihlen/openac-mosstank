//! The salvage staging the `ust*` functions share (`SalvageStagingManager`):
//! items staged one by one, then salvaged with the character's ust.

use std::collections::BTreeSet;

use ac_plugin_abi::items::InventoryItem;

use crate::host::MetaHost;

/// The items staged for the ust.
#[derive(Debug, Clone, Default)]
pub struct SalvageStaging {
    staged: BTreeSet<u32>,
}

fn ust(inventory: &[InventoryItem]) -> Option<&InventoryItem> {
    inventory
        .iter()
        .filter(|item| item.name == "Ust")
        .min_by_key(|item| item.object_id)
}

impl SalvageStaging {
    /// How many are staged.
    #[must_use]
    pub fn count(&self) -> usize {
        self.staged.len()
    }

    /// Stages an owned item that is not worn; false otherwise.
    pub fn add(&mut self, host: &mut dyn MetaHost, object_id: u32) -> bool {
        if !host
            .owned_items_shared()
            .iter()
            .any(|item| item.object_id == object_id && item.equipped_location == 0)
        {
            return false;
        }
        self.staged.insert(object_id);
        true
    }

    /// Opens the ust (uses it); false without one.
    pub fn open(&mut self, host: &mut dyn MetaHost) -> bool {
        let inventory = host.owned_items_shared();
        let Some(found) = ust(&inventory).map(|item| item.object_id) else {
            return false;
        };
        host.use_item(found).accepted()
    }

    /// Salvages the staged items still owned; false without a ust, with
    /// nothing staged, or when the client refuses.
    pub fn salvage(&mut self, host: &mut dyn MetaHost) -> bool {
        let inventory = host.owned_items_shared();
        let Some(tool) = ust(&inventory).map(|item| item.object_id) else {
            return false;
        };
        let items: Vec<u32> = inventory
            .iter()
            .filter(|item| item.object_id != tool && self.staged.contains(&item.object_id))
            .map(|item| item.object_id)
            .collect();
        if items.is_empty() || !host.salvage(tool, items).accepted() {
            return false;
        }
        self.staged.clear();
        true
    }

    /// Forgets what was staged.
    pub fn clear(&mut self) {
        self.staged.clear();
    }
}
