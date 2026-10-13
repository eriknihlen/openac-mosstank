//! MossTank's vendor and fellowship automation, converted from the C#
//! plugin (`VendorTradePlanner.cs`, `VendorTradeController.cs`,
//! `FellowshipManager.cs`, `MossTankCommands.Fellow.cs`, and the
//! `UbObjectSearch`/`UbChat` pieces they use).
//!
//! - [`planner`]: what one round at a vendor can buy or sell.
//! - [`sell_guard`]: what is never sold.
//! - [`controller`]: the vendor run and the `/ub vendor` commands.
//! - [`fellowship`]: VTank's fellowship manager.
//! - [`object_search`]: finding the object a typed name stands for.
//! - [`ub_chat`]: the shape of the lines the UtilityBelt side prints.
//! - [`client`]: what these read from and ask of the client.

pub mod client;
pub mod controller;
pub mod fellowship;
pub mod object_search;
pub mod planner;
pub mod sell_guard;
pub mod ub_chat;

pub use client::{FellowshipAutomation, VendorAutomation};
pub use controller::{
    VendorContext, VendorLootDecision, VendorTradeController, VendorTradeSettings,
};
pub use fellowship::FellowshipManager;
