//! Game data MossTank carries.
//!
//! - [`spell_id`]: spell ids by name, generated from the C# spell table.
//! - [`damage`]: damage types and the game database's element lists.

pub mod damage;
#[rustfmt::skip]
pub mod spell_id;
