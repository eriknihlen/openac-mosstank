//! MossTank's meta engine, converted from the C# plugin: the engine that
//! runs a meta's rules state by state, the expression runtime with the
//! host's functions, the views a meta creates, and the macro scheduler
//! that paces the macro's rules and looks at the meta every pass.
//!
//! The client comes in through [`host::MetaHost`], which the plugin
//! implements over its SDK host for the length of one call; the rest of the
//! plugin through [`services::MetaServices`]. Nothing here keeps the
//! client between calls.

pub mod chat;
pub mod experience;
mod functions;
pub mod held_motions;
pub mod host;
pub mod login_roster;
pub mod materials;
pub mod quests;
pub mod runtime;
pub mod salvage;
pub mod services;
pub mod status_hud;
pub mod view;
pub mod view_manager;

pub use functions::host_function_names;
pub mod debug_log;
pub mod engine;
pub mod events;
pub mod scheduler;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
