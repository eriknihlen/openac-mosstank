//! The client as MossTank's rules see it (`IAutomationSurface`): the
//! character, the spellbook, casting, the combat stance, the inventory, the
//! object table, the fellowship, the network's other clients, where the
//! character stands, and the log.
//!
//! Every read goes to the client, so each takes `&mut self` (the SDK's host
//! is reached through `&mut`). A method a group's C# interface gives a
//! default body has the same default here.

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill};
use ac_plugin_abi::combat::{
    AttackHeight, CombatMode, CombatResult, CombatSnapshot, CombatStatus, CombatTarget,
};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentObservation, EquipmentPlacement, EquipmentResult, EquipmentStatus,
    InventoryItem, ItemResult, ItemUseCompletion,
};
use ac_plugin_abi::magic::{CastCompletion, CastGate, CastResult, TrackedEnchantment};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::projectile::{ProjectilePathQuery, ProjectilePathResult};
use ac_plugin_abi::social::{BusyState, FellowMember, FellowshipStatus, SelectionAction};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::ui::ProjectileSample;
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

    /// Whether the player is typing in the chat bar.
    fn chat_input_active(&mut self) -> bool {
        false
    }

    // --- More of the character.

    /// Its attributes.
    fn attributes(&mut self) -> Vec<Attribute> {
        Vec::new()
    }

    /// The timed enchantments on it (`TimedEnchantments`).
    fn timed_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        Vec::new()
    }

    // --- More of casting.

    /// Whether a cast is under way.
    fn is_casting(&mut self) -> bool {
        false
    }

    /// Casts and says how the request went (`RequestCast`).
    fn request_cast(&mut self, spell_id: u32, target: Option<u32>) -> CastResult {
        if self.cast(spell_id, target) {
            CastResult::Sent
        } else {
            CastResult::Unavailable
        }
    }

    // --- The fight (`ICombatAutomation`).

    /// The combat state now. Without a fuller view, only the stance.
    fn combat_snapshot(&mut self) -> CombatSnapshot {
        CombatSnapshot {
            mode: self.combat_mode(),
            ..CombatSnapshot::default()
        }
    }

    /// The creatures worth attacking within a range, metres.
    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        let _ = maximum_metres;
        Vec::new()
    }

    /// Starts a swing at a creature.
    fn begin_attack(&mut self, target_id: u32, height: AttackHeight, power: f32) -> CombatResult {
        let _ = (target_id, height, power);
        CombatResult::of(CombatStatus::Unavailable)
    }

    /// Lets the swing go.
    fn release_attack(&mut self) -> CombatResult {
        CombatResult::of(CombatStatus::Unavailable)
    }

    /// Stops attacking.
    fn abort_attack(&mut self) -> CombatResult {
        CombatResult::of(CombatStatus::Unavailable)
    }

    /// Lets go of a creature the server stopped talking about.
    fn dismiss_ghost(&mut self, target_id: u32) -> CombatResult {
        let _ = target_id;
        CombatResult::of(CombatStatus::Unavailable)
    }

    /// Takes combat over from the client (`true`) or hands it back.
    fn combat_control(&mut self, acquire: bool) -> bool {
        let _ = acquire;
        false
    }

    // --- What is worn and wielded (`IEquipmentAutomation`).

    /// Whether equipment can be asked anything.
    fn equipment_available(&mut self) -> bool {
        false
    }

    /// Whether an equip is outstanding.
    fn equipment_busy(&mut self) -> bool {
        false
    }

    /// Everything owned that can be worn or wielded.
    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        Vec::new()
    }

    /// Every worn item's place, in the order the world placed them.
    fn equipment_placements(&mut self) -> Vec<EquipmentPlacement> {
        Vec::new()
    }

    /// The items that went on or came off since the last ask, oldest
    /// first (the C# `PlacementObserved` event, polled).
    fn take_equipment_observations(&mut self) -> Vec<EquipmentObservation> {
        Vec::new()
    }

    /// Wields or wears an item where it goes.
    fn equip(&mut self, object_id: u32) -> EquipmentResult {
        let _ = object_id;
        EquipmentResult {
            status: EquipmentStatus::Unavailable,
            notice: None,
        }
    }

    /// Wields an item in the off hand.
    fn equip_secondary(&mut self, object_id: u32) -> EquipmentResult {
        let _ = object_id;
        EquipmentResult {
            status: EquipmentStatus::Unavailable,
            notice: None,
        }
    }

    /// Puts an owned item into a container (`MoveToContainer`).
    fn move_item(&mut self, object_id: u32, container_id: u32) -> ItemResult {
        let _ = (object_id, container_id);
        ItemResult {
            status: ac_plugin_abi::items::ItemStatus::Unavailable,
            notice: None,
        }
    }

    // --- What holds the character busy (`IRecoveryAutomation`).

    /// The busy references.
    fn busy_state(&mut self) -> BusyState {
        BusyState::default()
    }

    // --- Flight paths (`IProjectileAutomation`).

    /// Whether flight paths can be asked.
    fn projectiles_available(&mut self) -> bool {
        false
    }

    /// Flies a probe from the character to a target.
    fn evaluate_projectile_path(&mut self, query: &ProjectilePathQuery) -> ProjectilePathResult {
        let _ = query;
        ProjectilePathResult::default()
    }

    /// Shows a flight's markers in the world.
    fn show_projectile_samples(&mut self, samples: &[ProjectileSample]) {
        let _ = samples;
    }

    // --- Moving the character by hand (`INavigationAutomation`).

    /// The character's movement snapshot.
    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        NavigationSnapshot::default()
    }

    /// One object as navigation sees it.
    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        let _ = object_id;
        None
    }

    /// Holds movement keys.
    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        let _ = intent;
        NavigationStatus::Unavailable
    }

    /// Lets every movement key go.
    fn clear_movement_intent(&mut self) -> NavigationStatus {
        NavigationStatus::Unavailable
    }

    /// Turns the character to a heading, degrees.
    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        let _ = heading_degrees;
        NavigationStatus::Unavailable
    }

    // --- The selection (`ISelectionAutomation`, `ISelectionService`).

    /// The selected object.
    fn selected_object(&mut self) -> Option<u32> {
        None
    }

    /// Selects an object.
    fn select(&mut self, object_id: u32) -> bool {
        let _ = object_id;
        false
    }

    /// Steps the selection (previous selection, previous or next player).
    fn selection_step(&mut self, action: SelectionAction) -> bool {
        let _ = action;
        false
    }

    // --- Effects on other objects (`IEnchantmentAutomation`).

    /// What the client knows is on an object.
    fn tracked_enchantments(&mut self, target_id: u32) -> Vec<TrackedEnchantment> {
        let _ = target_id;
        Vec::new()
    }

    /// Tells the client a cast landed on an object.
    fn report_cast(&mut self, target_id: u32, spell_id: u32, duration_seconds: f64) -> bool {
        let _ = (target_id, spell_id, duration_seconds);
        false
    }

    /// Forgets the casts reported on an object (0 for every object).
    fn forget_enchantments(&mut self, target_id: u32) {
        let _ = target_id;
    }

    // --- Casts shared with the other clients on this computer.

    /// Tells the other clients a cast at a monster went out.
    fn announce_cast_attempt(&mut self, target_id: u32, spell_id: u32, skill: i32) -> bool {
        let _ = (target_id, spell_id, skill);
        false
    }

    /// Tells the other clients a cast at a monster landed.
    fn announce_cast_success(
        &mut self,
        target_id: u32,
        spell_id: u32,
        skill: i32,
        duration_seconds: f64,
    ) -> bool {
        let _ = (target_id, spell_id, skill, duration_seconds);
        false
    }

    /// The other clients' casts after a sequence, oldest first.
    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        let _ = after_sequence;
        Vec::new()
    }

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
