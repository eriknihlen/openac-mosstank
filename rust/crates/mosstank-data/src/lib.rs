//! Game data MossTank carries.
//!
//! - [`spell_id`]: spell ids by name, generated from the C# spell table.
//! - [`damage`]: damage types, VTank's element numbers and the game
//!   database's element lists.
//! - [`material`]: materials by id and name.
//! - [`tinker`]: the tinkering arithmetic and the rend table.
//! - [`experience`]: experience targets, cost tables and the spend planner.
//! - [`attack_spells`]: the attack-spell lines and their best known members.
//! - [`weapon`]: a weapon's element, launcher kind, and the weapon ladder.

pub mod attack_spells;
pub mod damage;
pub mod experience;
pub mod material;
#[rustfmt::skip]
pub mod spell_id;
pub mod tinker;
pub mod weapon;
