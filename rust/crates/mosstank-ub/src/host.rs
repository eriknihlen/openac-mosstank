//! The client as the UB tab reaches it: everything the meta engine reaches
//! ([`MetaHost`]) and the few calls only the UB commands make: the client's
//! own turn and jump, the character options, allegiance, opening and
//! closing a container, the chat input interceptor, the other clients'
//! broadcasts, the dungeon data, and what the UB tools add: merging stacks,
//! the appraisal slot, the vitals and spending experience, the allegiance,
//! labels over objects, the shared profile folder, and loot rules read
//! against the client. Every method maps onto one SDK call, or (the
//! rules) onto the macro's own rule engine over the same client.

pub use ac_plugin_abi::character::{
    AdvancementKind, AdvancementResult, AdvancementStatus, OptionResult, OptionStatus, Skill, Vital,
};
use ac_plugin_abi::items::InventoryItem;
pub use ac_plugin_abi::items::{AppraisalState, ItemResult};
pub use ac_plugin_abi::navigation::{
    MoveChannel, MoveDirection, MovePace, MoveReport, MoveUnit, NavigationStatus,
};
pub use ac_plugin_abi::social::{AllegianceResult, AllegianceState, AllegianceStatus};
pub use ac_plugin_abi::ui::WorldLabel;
pub use ac_plugin_abi::world::Floorplan;
use ac_plugin_abi::world::ItemProperties;
use mosstank_formats::utl::LootRule;
pub use mosstank_loot::rules::LootDecision;
use mosstank_meta::host::MetaHost;
use mosstank_stores::storage::PluginStorage;

/// The client during one plugin call, as the UB tab reaches it.
pub trait UbHost: MetaHost {
    // ---- chat ----

    /// Installs a chat input interceptor; its handle.
    ///
    /// # Errors
    /// The client refused (a plugin may hold sixteen).
    fn chat_add_interceptor(&mut self) -> Result<u32, String>;
    /// Removes one.
    fn chat_remove_interceptor(&mut self, handle: u32) -> bool;

    // ---- navigation ----

    /// Turns on the spot to a compass heading, exactly.
    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus;
    /// Begins a move the client carries out; an amount of 0 keeps going
    /// until stopped.
    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus;
    /// Ends the move on one channel.
    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus;
    /// Jumps with a power of a full charge (above 0, at most 1).
    fn jump(&mut self, power: f32) -> NavigationStatus;
    /// The latest move on each channel and the count of jumps begun.
    fn move_report(&mut self) -> MoveReport;

    // ---- the character ----

    /// Every skill.
    fn skills(&mut self) -> Vec<Skill>;
    /// The character options by name, with their values.
    fn character_options(&mut self) -> Vec<(String, bool)>;
    /// Sets one character option by name.
    fn set_character_option(&mut self, name: &str, value: bool) -> OptionResult;

    // ---- social ----

    /// Swears allegiance to a player.
    fn allegiance_swear(&mut self, patron_id: u32) -> AllegianceResult;
    /// Breaks allegiance with a player.
    fn allegiance_break(&mut self, target_id: u32) -> AllegianceResult;

    /// Answers the server's open question.
    fn answer_dialog(&mut self, context_id: u32, accept: bool) -> bool;

    // ---- containers ----

    /// Opens a container or corpse.
    fn open_loot(&mut self, container_id: u32) -> ItemResult;
    /// Closes it.
    fn close_loot(&mut self, container_id: u32) -> ItemResult;

    // ---- the session ----

    /// Logs the character off and ends the session gracefully; false when
    /// there is no session to end.
    fn request_close(&mut self) -> bool;

    // ---- the other clients ----

    /// Asks the other clients (wearing one of `tags`; all with none) to run
    /// `line`, `delay_ms` apart.
    fn broadcast_command(&mut self, line: &str, tags: &[String], delay_ms: i32) -> bool;
    /// The labels this client answers to.
    fn set_peer_tags(&mut self, tags: &[String]) -> bool;
    /// Asks the other clients to share what `capabilities` names; the
    /// lease's handle, `None` when refused.
    fn peer_subscribe(&mut self, capabilities: u32) -> Option<u32>;
    /// Ends a lease.
    fn peer_unsubscribe(&mut self, handle: u32) -> bool;
    /// Whether the other clients are read and written now.
    fn peers_connected(&mut self) -> bool;

    // ---- dungeons ----

    /// Whether a cell is an indoor cell that sees nothing outside.
    fn is_sealed_dungeon(&mut self, cell_id: u32) -> bool;
    /// A landblock's indoor floorplan (`None`: the plan of nothing).
    fn dungeon_floorplan(&mut self, landblock_id: u32) -> Option<Floorplan>;

    // ---- the UB tools ----

    /// Whether a loot rule matches an item, its requirements read against
    /// this client (`LootRule.IsMatch`).
    fn loot_rule_matches(
        &mut self,
        rule: &LootRule,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> bool;
    /// Whether appraising an item could change which rule decides it
    /// (`LootRuleEngine.NeedsIdentify`).
    fn loot_needs_identify(
        &mut self,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
    ) -> bool;
    /// The rules' decision on an item, `owned` counted for keep-up-to
    /// (`LootRuleEngine.Decide`).
    fn loot_decide(
        &mut self,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
        owned: &[InventoryItem],
    ) -> Option<LootDecision>;
    /// The shared profile folder (`IPluginHost.VtankProfiles`): the UB
    /// tree's equipment profiles and map markers.
    fn profile_storage(&mut self) -> &mut dyn PluginStorage;
    /// Moves `amount` of one stack onto another of the same thing (0: all
    /// that fit).
    fn merge_items(&mut self, source_id: u32, target_id: u32, amount: u32) -> ItemResult;
    /// The appraisal slot: what it waits on, what it last gave up on, and
    /// whether the object shown came back as an unsuccessful appraisal.
    fn appraisal(&mut self) -> AppraisalState;
    /// Drops an item the client still lists but the server no longer has
    /// (`IItemAutomation.ForgetStaleItem`).
    fn forget_stale_item(&mut self, object_id: u32) -> ItemResult;
    /// The three vitals, with their ranks.
    fn vitals(&mut self) -> Vec<Vital>;
    /// Spends experience on one stat.
    fn advance(&mut self, kind: AdvancementKind, stat_id: u32, cost: u64) -> AdvancementResult;
    /// The character's allegiance.
    fn allegiance(&mut self) -> AllegianceState;
    /// Replaces the plugin's labels over objects in the world; false when
    /// the client refused the set.
    fn show_labels(&mut self, labels: &[WorldLabel]) -> bool;
}
