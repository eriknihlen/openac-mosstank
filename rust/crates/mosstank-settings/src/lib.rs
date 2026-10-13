//! MossTank's settings model and the formats and tables around it.
//!
//! - [`model`]: the live settings groups a settings profile fills.
//! - [`options`]: VTank's option catalogue (names, types, defaults, pages,
//!   descriptions, choice labels).
//! - [`defaults`]: the settings document a new profile starts from.
//! - [`profile`]: the settings profile (`.usd`) read into and written back
//!   from the live settings.
//! - [`items`]: the item tables a settings profile carries.
//! - [`monster`]: monster rules, their spec language and the monster facts.
//! - [`monster_table`]: the profile's `MyMonsters` table.
//! - [`ammunition`]: the ammunition a launcher is fed, from the game database.
//!
//! Pure: no host calls.

pub mod ammunition;
pub mod defaults;
pub mod items;
pub mod model;
pub mod monster;
pub mod monster_table;
pub mod options;
pub mod profile;
