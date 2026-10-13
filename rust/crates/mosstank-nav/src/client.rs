//! What the route reads from and asks of the client, beyond the rules'
//! common view of it ([`Automation`]): the C#'s `INavigationAutomation`,
//! the chat bar, the vendor windows, the appraisal queue and the cast
//! requests a route waypoint makes.
//!
//! A method the C# interface gives a default body has the same default
//! here.

use ac_plugin_abi::magic::{CastResult, SpellInfo, SpellList};
use ac_plugin_abi::navigation::{
    GoToReport, MoveChannel, MoveDirection, MovePace, MoveUnit, MovementIntent, NavigationObject,
    NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::world::Position;
use mosstank_automation::Automation;

/// The client as the route sees it.
pub trait NavigationAutomation: Automation {
    /// The character's movement now (`Snapshot`).
    fn navigation(&mut self) -> NavigationSnapshot;

    /// One object by id, with what is known of its door and lock
    /// (`TryGetObject`).
    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject>;

    /// The object nearest `near` named exactly `name` (any case), within
    /// `maximum_distance_meters` across the ground (`TryFindObject`).
    fn find_object(
        &mut self,
        name: &str,
        near: Position,
        maximum_distance_meters: f64,
    ) -> Option<NavigationObject> {
        let _ = (name, near, maximum_distance_meters);
        None
    }

    /// Every object whose position is known (`CaptureObjects`).
    fn navigation_objects(&mut self) -> Vec<NavigationObject> {
        Vec::new()
    }

    /// Holds movement keys until replaced or cleared.
    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus;

    /// Releases the held movement keys.
    fn clear_movement_intent(&mut self) -> NavigationStatus;

    /// Turns on the spot to a compass heading.
    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        let _ = heading_degrees;
        NavigationStatus::Unavailable
    }

    /// Begins a move the client carries out and ends by itself (`Move`).
    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        let _ = (direction, pace, amount, unit);
        NavigationStatus::Unavailable
    }

    /// Ends the move on one channel (`StopMoving(channel)`).
    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        let _ = channel;
        NavigationStatus::Unavailable
    }

    /// Jumps with `power` of a full charge.
    fn jump(&mut self, power: f32) -> NavigationStatus {
        let _ = power;
        NavigationStatus::Unavailable
    }

    /// Walks to a place along a route the client plans
    /// (`GoTo(position, arrivalMeters)`).
    fn go_to_place(&mut self, position: Position, arrival_meters: f32) -> NavigationStatus {
        let _ = (position, arrival_meters);
        NavigationStatus::Unavailable
    }

    /// Ends the walk this plugin started.
    fn stop_go_to(&mut self) -> NavigationStatus {
        NavigationStatus::Unavailable
    }

    /// The latest walk the client planned.
    fn go_to_report(&mut self) -> GoToReport {
        GoToReport::default()
    }

    /// Whether the player is typing in the chat bar (`IsInputActive`).
    fn chat_input_active(&mut self) -> bool {
        false
    }

    /// Submits a line as if typed into the chat bar (`Submit`); false when
    /// the client took no line.
    fn submit_chat(&mut self, text: &str) -> bool {
        let _ = text;
        false
    }

    /// The vendor the client has open (`Items.ActiveVendorObjectId`), 0
    /// for none.
    fn active_vendor_id(&mut self) -> u32 {
        0
    }

    /// The vendor the trade side is working (`Vendor.VendorObjectId` while
    /// `Vendor.IsOpen`), 0 for none.
    fn open_vendor_id(&mut self) -> u32 {
        0
    }

    /// The object an appraisal is awaited for
    /// (`Loot.Appraisal.AwaitingObjectId`), 0 for none.
    fn appraisal_awaiting_id(&mut self) -> u32 {
        0
    }

    /// Whether a cast is in the air (`Magic.IsCasting`).
    fn is_casting(&mut self) -> bool {
        false
    }

    /// Casts a spell at the selection and says why when it does not go out
    /// (`Magic.RequestCast`).
    fn request_cast(&mut self, spell_id: u32) -> CastResult {
        let _ = spell_id;
        CastResult::Unavailable
    }

    /// One of the character's known-spell lists (`KnownSelfBuffs`,
    /// `KnownAttackSpells`, `KnownCombatSpells`).
    fn known_spell_list(&mut self, list: SpellList) -> Vec<SpellInfo> {
        let _ = list;
        Vec::new()
    }
}

/// MossTank's tag on the lines it posts in chat.
pub const CHAT_TAG: &str = "[MossTank]";

/// Posts a line in the character's chat window under MossTank's tag
/// (`MossTankChat.Post`): the tag is added unless the line already has it.
pub fn post(automation: &mut dyn Automation, text: &str) {
    if text.starts_with(CHAT_TAG) {
        automation.post_system_message(text);
    } else {
        automation.post_system_message(&format!("{CHAT_TAG} {text}"));
    }
}
