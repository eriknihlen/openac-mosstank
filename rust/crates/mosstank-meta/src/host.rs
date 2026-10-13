//! The client as the meta engine reaches it: one trait, [`MetaHost`], the
//! plugin implements over its SDK host for the length of one call, and the
//! client's own records ([`ac_plugin_abi`]) for what it answers. Every
//! method maps onto one SDK call, so the C#'s `IPluginHost` surface the
//! engine and its functions used is all here and nothing else is.

pub use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill, Training};
pub use ac_plugin_abi::combat::{
    AttackHeight, CombatMode, CombatResult, CombatSnapshot, CombatStatus, CombatTarget,
};
pub use ac_plugin_abi::items::{
    EquipmentItem, EquipmentResult, EquipmentStatus, InventoryItem, ItemResult, ItemState,
    ItemStatus, ItemUseCompletion, LootContainer,
};
pub use ac_plugin_abi::magic::{CastResult, ComponentInfo, SpellInfo, SpellList};
pub use ac_plugin_abi::navigation::{MovementIntent, NavigationSnapshot, NavigationStatus};
pub use ac_plugin_abi::peers::{NetworkClient, PeerCast};
pub use ac_plugin_abi::social::{
    FellowMember, FellowshipState, LoginCharacter, LoginState, SelectionState,
};
pub use ac_plugin_abi::world::{
    Confirmation, ItemProperties, ObjectClass, PortalTransition, Position, WorldObject, WorldTime,
};
pub use ac_plugin_abi::{CharacterInfo, ChatLine, LogLevel};
use mosstank_stores::storage::{PluginStorage, StorageError};

/// A window the plugin asks the client to show: the C#'s
/// `PluginPanelDescriptor` with the markup it is drawn from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelDescriptor {
    /// The window's id.
    pub window_id: String,
    /// Its title.
    pub title: String,
    /// The text on its shelf icon.
    pub icon_text: String,
    /// It opens shown.
    pub start_visible: bool,
    /// It sits on the side shelf.
    pub show_in_side_panel: bool,
}

/// The spell table's caster and target effects, formula version, display
/// order and component loss (`spelldata`'s CasterEffect, TargetEffect,
/// Generation, SortKey and Speed).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpellExtras {
    /// The caster's effect.
    pub caster_effect: u32,
    /// The target's effect.
    pub target_effect: u32,
    /// The formula version.
    pub formula_version: u32,
    /// The display order.
    pub display_order: i32,
    /// The component loss.
    pub component_loss: f32,
}

/// What [`MetaHost::ub_display`] answers for one kind of UtilityBelt line:
/// whether it is shown, and the text class it is printed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UbDisplay {
    /// The line is shown.
    pub show: bool,
    /// The text class (chat type) it is printed in.
    pub chat_type: u32,
}

/// The four kinds of UtilityBelt line, each with its own display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbLineKind {
    /// An ordinary line.
    Generic,
    /// A debug line.
    Debug,
    /// What `/ub mexec` says.
    Expression,
    /// An error.
    Error,
}

/// The client during one plugin call.
///
/// The methods follow the SDK's `Host` (and the C#'s `IPluginHost`) one for
/// one; a host without a window answers the window methods with "no".
pub trait MetaHost {
    // ---- the log and chat ----

    /// A line in the session's log.
    fn log(&mut self, level: LogLevel, message: &str);
    /// Shows a line in the chat transcript, in a text class (0 for the
    /// system class: the C#'s `PostSystemMessage`).
    fn chat_post(&mut self, text: &str, log_text_type: u32);
    /// Submits a line as if typed; false when the client took no line.
    fn chat_submit(&mut self, line: &str) -> bool;
    /// Stages text in the chat entry without sending it; false when the
    /// player is typing.
    fn chat_compose(&mut self, text: &str) -> bool;
    /// The chat lines after `after_sequence`.
    fn chat_captured(&mut self, after_sequence: u64) -> Vec<ChatLine>;
    /// Registers a chat filter the client asks before showing a line
    /// (the plugin hands the question to [`crate::runtime::ExpressionRuntime::filter_chat`]);
    /// its handle.
    fn chat_add_filter(&mut self) -> u32;
    /// Removes a chat filter.
    fn chat_remove_filter(&mut self, handle: u32) -> bool;
    /// How a kind of UtilityBelt line is displayed; the reference's
    /// defaults unless the plugin's settings say otherwise.
    fn ub_display(&mut self, kind: UbLineKind) -> UbDisplay {
        crate::chat::ub_default_display(kind)
    }

    // ---- the character ----

    /// The character in the world, or `None`.
    fn character(&mut self) -> Option<CharacterInfo>;
    /// One skill.
    fn skill(&mut self, skill_id: u32) -> Option<Skill>;
    /// The six attributes.
    fn attributes(&mut self) -> Vec<Attribute>;
    /// The enchantments in effect on the character.
    fn enchantments(&mut self) -> Vec<ActiveEnchantment>;

    // ---- the world ----

    /// Every object the client knows.
    fn objects(&mut self) -> Vec<WorldObject>;
    /// One object.
    fn object(&mut self, object_id: u32) -> Option<WorldObject>;
    /// Every property the client holds for an object (its appraisal).
    fn properties(&mut self, object_id: u32) -> Option<ItemProperties>;
    /// One integer property of an object.
    fn int_property(&mut self, object_id: u32, property: u32) -> Option<i32>;
    /// Asks for an object's appraisal.
    fn identify(&mut self, object_id: u32) -> ItemResult;
    /// The open outside container, 0 for none.
    fn open_container(&mut self) -> u32;
    /// The game's calendar.
    fn world_time(&mut self) -> WorldTime;
    /// Where the character stands.
    fn navigation(&mut self) -> NavigationSnapshot;
    /// Holds movement keys.
    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus;
    /// Lets go of the movement keys.
    fn clear_movement_intent(&mut self) -> NavigationStatus;

    // ---- the selection ----

    /// The selection.
    fn selection(&mut self) -> SelectionState;
    /// Selects an object.
    fn select(&mut self, object_id: u32) -> bool;

    // ---- items and equipment ----

    /// Where the item commands stand (busy, the open vendor).
    fn item_state(&mut self) -> ItemState;
    /// Everything the character owns.
    fn owned_items(&mut self) -> Vec<InventoryItem>;
    /// Uses an item.
    fn use_item(&mut self, object_id: u32) -> ItemResult;
    /// Uses an item on a target.
    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult;
    /// Gives an item.
    fn give_item(&mut self, object_id: u32, target_id: u32, amount: u32) -> ItemResult;
    /// Drops an item.
    fn drop_item(&mut self, object_id: u32, amount: u32) -> ItemResult;
    /// Moves an item into a container at a slot, joining a stack of the
    /// same thing already there when `join_stack` is set.
    fn move_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
        join_stack: bool,
    ) -> ItemResult;
    /// Salvages items with a tool.
    fn salvage(&mut self, tool_id: u32, item_ids: Vec<u32>) -> ItemResult;
    /// The corpses within `maximum_metres`.
    fn corpses(&mut self, maximum_metres: f32) -> Vec<LootContainer>;
    /// Everything owned that can be worn or wielded.
    fn owned_equipment(&mut self) -> Vec<EquipmentItem>;
    /// Wears or wields an item (location bits, 0 to choose).
    fn equip(&mut self, object_id: u32, location: u32) -> EquipmentResult;
    /// An equip or unequip is in flight.
    fn equipment_busy(&mut self) -> bool;

    // ---- spells and magic ----

    /// One spell.
    fn spell(&mut self, spell_id: u32) -> Option<SpellInfo>;
    /// One of the known-spell lists.
    fn known_spells(&mut self, list: SpellList) -> Vec<SpellInfo>;
    /// Whether the character knows a spell.
    fn is_spell_known(&mut self, spell_id: u32) -> bool;
    /// One spell component.
    fn spell_component(&mut self, component_id: u32) -> Option<ComponentInfo>;
    /// Seconds left on a cooldown group.
    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64;
    /// A cast is in flight.
    fn is_casting(&mut self) -> bool;
    /// The character carries a spell's components.
    fn has_components(&mut self, spell_id: u32) -> bool;
    /// Casts a spell at a target (0 for none).
    fn cast(&mut self, spell_id: u32, target_id: u32) -> CastResult;
    /// The parts of a spell's table entry `spelldata` reports that the
    /// client's spell record does not carry; zero where the client does
    /// not say.
    fn spell_extras(&mut self, spell_id: u32) -> SpellExtras {
        let _ = spell_id;
        SpellExtras::default()
    }

    // ---- combat ----

    /// The combat state.
    fn combat(&mut self) -> CombatSnapshot;
    /// The hostile creatures within `maximum_metres`.
    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget>;
    /// Enters a combat mode.
    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult;

    // ---- fellowship, login, the other clients ----

    /// The character's fellowship.
    fn fellowship(&mut self) -> FellowshipState;
    /// Its members, complete.
    fn fellowship_roster(&mut self) -> Vec<FellowMember>;
    /// The account's characters and the next login.
    fn login(&mut self) -> LoginState;
    /// The character entered at the next return to the list.
    fn login_set_next(&mut self, character_id: u32) -> bool;
    /// No next login.
    fn login_clear_next(&mut self) -> bool;
    /// The other clients on this computer.
    fn peer_clients(&mut self) -> Vec<NetworkClient>;
    /// The casts they announced after `after_sequence`.
    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast>;

    // ---- windows ----

    /// A window can be shown.
    fn has_ui(&mut self) -> bool;
    /// Registers a window; its handle, or `None` when it is refused.
    fn register_panel(&mut self, descriptor: &PanelDescriptor, markup: &str) -> Option<u32>;
    /// Removes a window.
    fn unregister_panel(&mut self, handle: u32);
    /// Whether one of the client's own windows exists.
    fn ui_view_exists(&mut self, view: &str) -> bool;
    /// Whether one of the client's own windows is shown.
    fn ui_view_visible(&mut self, view: &str) -> bool;
    /// Whether a control of one of the client's own windows exists.
    fn ui_control_exists(&mut self, view: &str, control: &str) -> bool;
    /// Labels a control of one of the client's own windows; false when the
    /// control takes no label.
    fn ui_set_control_label(&mut self, view: &str, control: &str, label: &str) -> bool;
    /// Shows or hides a control of one of the client's own windows.
    fn ui_set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool;

    // ---- storage ----

    /// The plugin's own storage.
    fn storage(&mut self) -> &mut dyn PluginStorage;
    /// A handle on the same storage that outlives the call, for the global
    /// variables every client on the server shares (they are read and
    /// written as expressions run); `None` when there is none.
    fn shared_storage(&mut self) -> Option<Box<dyn PluginStorage>>;
}

/// A boxed storage as a [`PluginStorage`].
pub struct BoxedStorage(pub Box<dyn PluginStorage>);

impl std::fmt::Debug for BoxedStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoxedStorage")
    }
}

impl PluginStorage for BoxedStorage {
    fn is_available(&mut self) -> bool {
        self.0.is_available()
    }
    fn root_path(&mut self) -> Option<String> {
        self.0.root_path()
    }
    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        self.0.read_text(key)
    }
    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.0.list(prefix)
    }
    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        self.0.write_text(key, content)
    }
    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        self.0.ensure_directory(prefix)
    }
    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        self.0.delete(key)
    }
    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        self.0.lock(key)
    }
    fn unlock(&mut self, handle: u32) -> bool {
        self.0.unlock(handle)
    }
}

/// The character's facts the engine reads, with the C#'s answers when the
/// character is not in the world (`ICharacterInfo` of no character).
#[derive(Debug, Clone, Default)]
pub(crate) struct Character {
    pub in_world: bool,
    pub id: u32,
    pub name: String,
    pub world: String,
    pub account: String,
    pub index: i32,
    pub health: (u32, u32, u32),
    pub stamina: (u32, u32, u32),
    pub mana: (u32, u32, u32),
}

impl Character {
    pub(crate) fn of(host: &mut dyn MetaHost) -> Self {
        match host.character() {
            Some(info) => Self {
                in_world: true,
                id: info.id,
                name: info.name,
                world: info.world,
                account: info.account,
                index: info.character_index,
                health: info.health,
                stamina: info.stamina,
                mana: info.mana,
            },
            None => Self::default(),
        }
    }
}
