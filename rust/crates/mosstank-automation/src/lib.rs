//! What MossTank's game logic reads from and asks of the client: the C#'s
//! `IAutomationSurface`, as traits over the plugin API's own records
//! (`ac-plugin-abi`). The plugin implements them over the SDK host for the
//! length of one call; tests implement them with fakes. Each group grows
//! with the slices that need it.
//!
//! - [`SpellCatalog`]: the spell table and the character's spellbook.

pub mod spells;

pub use spells::{SpellCatalog, StaticSpellCatalog};
