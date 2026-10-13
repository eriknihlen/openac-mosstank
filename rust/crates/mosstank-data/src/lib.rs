//! Game data MossTank carries.
//!
//! - [`spell_id`]: spell ids by name, generated from the C# spell table.
//! - [`damage`]: damage types and the game database's element lists.
//! - [`material`]: materials by id and name.
//! - [`tinker`]: the tinkering arithmetic and the rend table.
//! - [`experience`]: experience targets, cost tables and the spend planner.

pub mod damage;
pub mod experience;
pub mod material;
#[rustfmt::skip]
pub mod spell_id;
pub mod tinker;
