//! The client as MossTank's rules see it (`IAutomationSurface`): the
//! character, the spellbook, casting, the combat stance, the inventory, the
//! object table, the fellowship, the network's other clients, where the
//! character stands, and the log.
//!
//! Every read goes to the client, so each takes `&mut self` (the SDK's host
//! is reached through `&mut`). A method a group's C# interface gives a
//! default body has the same default here.

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, CastGate};
use ac_plugin_abi::peers::NetworkClient;
use ac_plugin_abi::social::{FellowMember, FellowshipStatus};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, Position, WorldObject};

use crate::spells::SpellCatalog;

/// What the rules read from and ask of the client.
pub trait Automation {
    /// Whether the client is in a state to be asked anything (in world).
    fn is_available(&mut self) -> bool;

    // --- The character (`ICharacterInfo`).

    /// The character: its id, world and vitals (now, at full, base).
    fn character(&mut self) -> CharacterInfo;

    /// Its skills.
    fn skills(&mut self) -> Vec<Skill>;

    /// One skill.
    fn try_get_skill(&mut self, skill_id: u32) -> Option<Skill> {
        self.skills()
            .into_iter()
            .find(|skill| skill.skill_id == skill_id)
    }

    // --- The spellbook (`ISpellCatalog`).

    /// The spell table and the character's spellbook.
    fn spells(&mut self) -> &mut dyn SpellCatalog;

    // --- Casting (`IMagicCommands`).

    /// Whether a cast would be accepted: at the selection (`None`) or at
    /// one object.
    fn evaluate_gate(&mut self, spell_id: u32, target: Option<u32>) -> CastGate;

    /// Casts, at the selection (`None`) or at one object; true only when
    /// the request went out.
    fn cast(&mut self, spell_id: u32, target: Option<u32>) -> bool;

    /// Whether the character carries the spell's components.
    fn has_components(&mut self, spell_id: u32) -> bool {
        let _ = spell_id;
        true
    }

    /// The newest cast the server answered.
    fn last_cast_completion(&mut self) -> CastCompletion {
        CastCompletion::default()
    }

    // --- The stance (`ICombatAutomation`).

    /// The combat stance.
    fn combat_mode(&mut self) -> CombatMode;

    /// Asks for a stance.
    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult;

    // --- The inventory (`IItemAutomation`).

    /// Whether the inventory can be asked anything.
    fn items_available(&mut self) -> bool;

    /// Whether an inventory transaction is outstanding.
    fn items_busy(&mut self) -> bool {
        false
    }

    /// Everything the character carries or wears.
    fn owned_items(&mut self) -> Vec<InventoryItem>;

    /// The newest use the server answered.
    fn last_item_completion(&mut self) -> ItemUseCompletion {
        ItemUseCompletion::default()
    }

    /// Uses an item.
    fn use_item(&mut self, object_id: u32) -> ItemResult;

    /// Uses an item on another object.
    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult;

    /// An appraised item's properties.
    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties>;

    // --- The object table (`IWorldObjectAutomation`).

    /// Whether the object table can be asked anything.
    fn objects_available(&mut self) -> bool {
        false
    }

    /// One object.
    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        let _ = object_id;
        None
    }

    /// Asks the client to appraise an object.
    fn identify(&mut self, object_id: u32) -> ItemResult {
        let _ = object_id;
        ItemResult {
            status: ac_plugin_abi::items::ItemStatus::Unavailable,
            notice: None,
        }
    }

    // --- The fellowship (`IFellowshipAutomation`).

    /// Whether the character is in a fellowship.
    fn in_fellowship(&mut self) -> bool {
        false
    }

    /// The fellows.
    fn fellowship_members(&mut self) -> Vec<FellowMember> {
        Vec::new()
    }

    /// Asks the client to stream the fellows' vitals, or to stop.
    fn request_fellow_vitals(&mut self, requested: bool) -> FellowshipStatus {
        let _ = requested;
        FellowshipStatus::Unavailable
    }

    // --- The network of clients (`INetworkAutomation`).

    /// Whether the network can be asked anything.
    fn network_available(&mut self) -> bool {
        false
    }

    /// The other clients on the network.
    fn network_clients(&mut self) -> Vec<NetworkClient> {
        Vec::new()
    }

    // --- Where the character stands (`INavigationAutomation`).

    /// The character's position.
    fn position(&mut self) -> Position {
        Position::default()
    }

    // --- Chat (`IPluginChat`).

    /// The chat lines captured after `after_sequence`, oldest first.
    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        let _ = after_sequence;
        Vec::new()
    }

    /// A system line in the character's chat window.
    fn post_system_message(&mut self, text: &str);

    // --- The log (`IPluginLogger`).

    /// An information line in the client's log.
    fn log_info(&mut self, text: &str);

    /// A warning line in the client's log.
    fn log_warn(&mut self, text: &str);
}

/// Whether the client has finished identifying an object
/// (`ConfiguredSupplyReadiness.IsAssessed`): a zero property means "none"
/// only after that; without an object table nothing is.
pub fn is_assessed(automation: &mut dyn Automation, object_id: u32) -> bool {
    automation.objects_available()
        && automation
            .object(object_id)
            .is_some_and(|object| object.last_id_time != 0)
}

/// An identified item's properties (`ConfiguredSupplyReadiness
/// .TryCaptureProperties`).
pub fn try_capture_properties(
    automation: &mut dyn Automation,
    object_id: u32,
) -> Option<ItemProperties> {
    if !is_assessed(automation, object_id) {
        return None;
    }
    automation.item_properties(object_id)
}

/// Readies a combat stance before a rule uses it (`CombatModeGate`): the
/// right caster or weapon wielded, the stance entered. The recharge asks it
/// for the magic stance; the gate itself belongs to the combat slice.
pub trait CombatModePreparation {
    /// Whether the stance is ready; when not, the gate has done the next
    /// step towards it. `override_item_id` names the caster to wield (0 for
    /// the gate's own pick); `auto_select` lets it pick one.
    fn try_prepare(
        &mut self,
        automation: &mut dyn Automation,
        wanted: ac_plugin_abi::combat::CombatMode,
        override_item_id: u32,
        auto_select: bool,
    ) -> bool;

    /// What the gate is doing, for the panel.
    fn status(&self) -> String;
}
