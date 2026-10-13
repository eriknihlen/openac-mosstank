//! What the looter reads from and asks of the client beyond the rules'
//! shared view ([`Automation`]): the loot surface (`ILootAutomation`), the
//! item commands it alone uses, the object table, the loot classifiers
//! other plugins registered, and the plugin's own storage. The plugin
//! implements it over the SDK host; tests implement it with fakes.

use ac_plugin_abi::items::{
    AppraisalState, InventoryCompletion, InventoryItem, ItemResult, ItemStatus, LootClassification,
    LootContainer, LootContext, LootedItem,
};
use ac_plugin_abi::world::{ItemProperties, Position, WorldObject};
use mosstank_automation::Automation;

/// A corpse (or outside container) near the character, as the looter reads
/// it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LootCorpse {
    /// Its id.
    pub object_id: u32,
    /// Its weenie class.
    pub weenie_class_id: u32,
    /// Its name.
    pub name: String,
    /// How far, metres.
    pub distance: f32,
    /// Its long description ("Killed by ...").
    pub long_description: String,
    /// The client flags a generated rare in it.
    pub is_generated_rare: bool,
    /// It was described.
    pub is_identified: bool,
    /// The server answered its description request, with a description or
    /// without one (a corpse that decayed meanwhile is answered with
    /// nothing).
    pub is_appraisal_answered: bool,
    /// Where it lies, when known.
    pub position: Option<Position>,
}

impl LootCorpse {
    /// A corpse with an id, a name and a distance, otherwise blank.
    #[must_use]
    pub fn new(object_id: u32, name: &str, distance: f32) -> Self {
        Self {
            object_id,
            weenie_class_id: 1,
            name: name.to_owned(),
            distance,
            ..Self::default()
        }
    }

    /// Whether the client can place it.
    #[must_use]
    pub fn has_position(&self) -> bool {
        self.position.is_some()
    }
}

impl From<LootContainer> for LootCorpse {
    /// The SDK's container. Whether a description request was answered
    /// without a description is not on the container: the plugin's client
    /// view fills it in from the appraisal answers (register LOOT-3), so
    /// here it reads as not answered.
    fn from(container: LootContainer) -> Self {
        Self {
            object_id: container.object_id,
            weenie_class_id: container.weenie_class_id,
            name: container.name,
            distance: container.distance,
            long_description: container.long_description,
            is_generated_rare: container.is_generated_rare,
            is_identified: container.is_identified,
            is_appraisal_answered: false,
            position: container.position,
        }
    }
}

/// An answer with no words: the status alone.
#[must_use]
pub fn status_result(status: ItemStatus) -> ItemResult {
    ItemResult::of(status)
}

/// The status as .NET names the enum member.
#[must_use]
pub fn status_name(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Unavailable => "Unavailable",
        ItemStatus::InvalidItem => "InvalidItem",
        ItemStatus::InvalidTarget => "InvalidTarget",
        ItemStatus::Busy => "Busy",
        ItemStatus::Started => "Started",
        ItemStatus::Refused => "Refused",
        ItemStatus::Completed => "Completed",
    }
}

/// The client as the looter sees it.
pub trait LootHost: Automation {
    // --- The loot surface (`ILootAutomation`).

    /// Whether looting can be asked anything.
    fn loot_available(&mut self) -> bool;

    /// The container asked to open, 0 for none.
    fn requested_container_id(&mut self) -> u32;

    /// The open container, 0 for none.
    fn current_container_id(&mut self) -> u32;

    /// Whether the open container's contents have arrived.
    fn current_contents_ready(&mut self) -> bool;

    /// The appraisal slot.
    fn appraisal(&mut self) -> AppraisalState;

    /// The last inventory request that ended.
    fn last_inventory_completion(&mut self) -> InventoryCompletion;

    /// The corpses within so many metres.
    fn loot_corpses(&mut self, maximum_distance: f32) -> Vec<LootCorpse>;

    /// What the open container holds.
    fn loot_contents(&mut self) -> Vec<InventoryItem>;

    /// An item's properties (in the open container or owned); `None` when
    /// the client cannot describe it yet.
    fn loot_properties(&mut self, object_id: u32) -> Option<ItemProperties>;

    /// Opens a corpse.
    fn loot_open(&mut self, container_id: u32) -> ItemResult;

    /// Closes the open container.
    fn loot_close(&mut self, container_id: u32) -> ItemResult;

    /// Asks for an appraisal of a corpse or an item.
    fn loot_identify(&mut self, object_id: u32) -> ItemResult;

    /// Picks an item out of the open container.
    fn loot_pickup(&mut self, object_id: u32) -> ItemResult;

    // --- The item commands the looter alone uses (`IItemAutomation`).

    /// Whether every pack the character carries has been listed.
    fn owned_inventory_complete(&mut self) -> bool {
        false
    }

    /// Salvages items with a tool.
    fn salvage(&mut self, tool_id: u32, item_ids: &[u32]) -> ItemResult;

    /// Sells an item to the open vendor.
    fn sell(&mut self, object_id: u32) -> ItemResult;

    /// The vendor whose shop is open, 0 for none.
    fn active_vendor_id(&mut self) -> u32;

    // --- The spellbook.

    /// Whether the character knows a spell.
    fn is_spell_known(&mut self, spell_id: u32) -> bool;

    // --- The object table.

    /// Every object the client knows.
    fn objects(&mut self) -> Vec<WorldObject>;

    // --- The loot classifiers (`IPluginLootClassifierRegistry`).

    /// Asks a registered classifier; `None` when it is not there or could
    /// not classify.
    fn classify(&mut self, classifier_id: &str, context: LootContext)
    -> Option<LootClassification>;

    /// Tells a classifier an item was looted; whether it heard.
    fn notify_looted(&mut self, classifier_id: &str, item: LootedItem) -> bool;

    /// Tells a classifier an item went away; whether it heard.
    fn notify_item_removed(&mut self, classifier_id: &str, object_id: u32) -> bool;

    // --- The plugin's own storage (`IPluginHost.Storage`).

    /// Whether the plugin's storage can read and write.
    fn storage_available(&mut self) -> bool {
        false
    }

    /// The text under `key` in the plugin's storage.
    ///
    /// # Errors
    /// The file exists but cannot be read.
    fn storage_read_text(&mut self, key: &str) -> Result<Option<String>, String> {
        let _ = key;
        Ok(None)
    }

    /// Replaces the text under `key` in the plugin's storage.
    ///
    /// # Errors
    /// The storage is unavailable, or the file cannot be written.
    fn storage_write_text(&mut self, key: &str, content: &str) -> Result<(), String> {
        let _ = (key, content);
        Err("Plugin storage is unavailable.".to_owned())
    }

    // --- The log.

    /// A failure in the client's log.
    fn log_error(&mut self, text: &str);
}
