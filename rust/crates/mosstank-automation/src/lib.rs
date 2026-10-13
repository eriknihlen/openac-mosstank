//! What MossTank's game logic reads from and asks of the client: the C#'s
//! `IAutomationSurface`, as traits over the plugin API's own records
//! (`ac-plugin-abi`). The plugin implements them over the SDK host for the
//! length of one call; tests implement them with fakes. Each group grows
//! with the slices that need it.
//!
//! - [`SpellCatalog`]: the spell table and the character's spellbook.
//! - [`cast_tracker`]: one spell cast in flight.
//! - [`combat_text`]: reading the character's own combat log.
//! - [`locks`]: the cooldown slots the rules share.
//! - [`components`]: the profile's blacklist of spell components.
//! - [`equipment`]: what the character wields, as the server confirmed it.
//! - [`Automation`]: the client as a whole (`IAutomationSurface`).

pub mod cast_tracker;
pub mod combat_text;
pub mod components;
pub mod equipment;
pub mod heading;
pub mod locks;
pub mod spells;
pub mod surface;

pub use locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
pub use spells::{SpellCatalog, StaticSpellCatalog};
pub use surface::Automation;
