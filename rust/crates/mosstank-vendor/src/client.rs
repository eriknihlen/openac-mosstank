//! What the vendor run and the fellowship manager read from and ask of the
//! client, on top of the route's view of it ([`NavigationAutomation`]):
//! the C#'s `IVendorAutomation`, the inventory moves a split makes, the
//! world objects a search looks through, and `IFellowshipAutomation`.

use ac_plugin_abi::items::{InventoryCompletion, ItemResult};
use ac_plugin_abi::social::{FellowMember, FellowshipState, FellowshipStatus};
use ac_plugin_abi::trade::{VendorResult, VendorState};
use ac_plugin_abi::world::{ItemProperties, WorldObject};
use mosstank_nav::NavigationAutomation;

use crate::ub_chat::UbChatSink;

/// The client as the vendor run sees it.
pub trait VendorAutomation: NavigationAutomation + UbChatSink {
    /// The open vendor: what it sells, what it buys, and the staged lists.
    fn vendor_state(&mut self) -> VendorState;

    /// A listing's properties (`IVendorAutomation.TryCaptureProperties`).
    fn vendor_item_properties(&mut self, template_id: u32) -> Option<ItemProperties>;

    /// Stages a listing on the buy list.
    fn vendor_add_to_buy_list(&mut self, template_id: u32, count: i32) -> VendorResult;

    /// Stages an owned item on the sell list.
    fn vendor_add_to_sell_list(&mut self, item_id: u32) -> VendorResult;

    /// Empties the buy list.
    fn vendor_clear_buy_list(&mut self) -> VendorResult;

    /// Empties the sell list.
    fn vendor_clear_sell_list(&mut self) -> VendorResult;

    /// Buys the buy list.
    fn vendor_buy_all(&mut self) -> VendorResult;

    /// Sells the sell list.
    fn vendor_sell_all(&mut self) -> VendorResult;

    /// The newest inventory move the server answered.
    fn last_inventory_completion(&mut self) -> InventoryCompletion;

    /// Moves (or splits, with an amount) an item into a container
    /// (`MoveToContainer`).
    fn move_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
    ) -> ItemResult;

    /// Every object the client knows (`Objects.CaptureObjects`).
    fn world_objects(&mut self) -> Vec<WorldObject>;

    /// The container open on screen, 0 for none.
    fn open_container_id(&mut self) -> u32 {
        0
    }

    /// The selected object, if any.
    fn selected_object_id(&mut self) -> Option<u32> {
        None
    }
}

/// The client as the fellowship manager sees it.
pub trait FellowshipAutomation: NavigationAutomation {
    /// The fellowship the character is in.
    fn fellowship_state(&mut self) -> FellowshipState;

    /// Every member, the character included (`CaptureRoster`).
    fn fellowship_roster(&mut self) -> Vec<FellowMember>;

    /// Recruits a player.
    fn fellowship_recruit(&mut self, player_id: u32) -> FellowshipStatus;

    /// Dismisses a member.
    fn fellowship_dismiss(&mut self, player_id: u32) -> FellowshipStatus;

    /// Hands the lead to a member.
    fn fellowship_assign_leader(&mut self, player_id: u32) -> FellowshipStatus;

    /// Opens or closes the fellowship.
    fn fellowship_set_open(&mut self, open: bool) -> FellowshipStatus;
}
