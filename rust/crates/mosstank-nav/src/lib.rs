//! MossTank's own navigation, converted from the C# plugin
//! (`Navigation.cs`, `NavigationMover.cs`, `OpenDoorRule.cs`,
//! `JumpPower.cs`, and the route helpers of the panel).
//!
//! MossTank walks its routes itself: it holds the client's movement keys
//! towards each waypoint ([`mover`]), runs the waypoint's action (a recall,
//! a portal, an NPC, a vendor, a chat line, a pause, a jump, a checkpoint),
//! hands a stuck leg to the client's own pathing when the profile allows,
//! and opens the doors that stand on the leg it is walking ([`door_rule`]).
//! The client's walk and movement calls are used exactly as the C# used
//! them through its host.
//!
//! - [`client`]: what the route reads from and asks of the client.
//! - [`heading`]: the compass arithmetic the steering shares.
//! - [`jump_power`]: a jump's charge time as the power it is asked with.
//! - [`mover`]: the close-in mover (held keys, its clock, the stuck clock).
//! - [`controller`]: the route follower (`NavigationController`).
//! - [`door_rule`]: opening a door as its own turn of the macro.
//! - [`route_view`]: the route as the panel draws and measures it.
//!
//! Nothing here keeps the client between calls: every call that reaches it
//! is handed a [`controller::RouteTurn`].

pub mod client;
pub mod controller;
pub mod door_rule;
pub mod heading;
pub mod jump_power;
pub mod mover;
pub mod route_view;

pub use client::NavigationAutomation;
pub use controller::{NavigationController, RouteTurn};
pub use door_rule::{DoorRuleHost, OpenDoorRule};
pub use mover::NavigationMover;
