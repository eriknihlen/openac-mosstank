//! MossTank's looting, converted from the C# (`Looting*.cs`,
//! `MossTankLootClassifier.cs`, `ManaDonorJournal.cs`,
//! `ManaStoneFillConfirmation.cs`, `VtankLootRequirementEvaluator.cs`,
//! `ReadScrollRule.cs`).
//!
//! - [`requirements`]: VTClassic's typed requirements.
//! - [`expression`]: MossTank's own rule expressions.
//! - [`rules`]: the rule list's verdict and whether an item needs an
//!   appraisal.
//! - [`salvage`]: combining salvage bags.
//! - [`mana`] and [`donors`]: draining donors into mana stones, and the
//!   journal that permits it.
//! - [`host`]: what the looter asks of the client ([`LootHost`]).
//! - [`controller`], [`corpse_scan`], [`own_rare`]: the looter itself.
//! - [`corpse_approach`]: the walk to a corpse the looter cannot reach.
//! - [`scroll`]: reading the scrolls it picked up.
//! - [`classifier`]: the loot classifier other plugins can ask.

pub mod classifier;
pub mod controller;
pub mod corpse_approach;
pub mod corpse_scan;
pub mod crafting;
pub mod donors;
pub mod expression;

pub mod host;
pub mod mana;
pub mod own_rare;
pub mod requirements;
pub mod rules;
pub mod salvage;
pub mod scroll;

pub use controller::LootController;
pub use corpse_scan::{CORPSE_ID_WAIT_MARGIN_METERS, CORPSE_OPEN_RANGE_METERS};
pub use host::{LootCorpse, LootHost};
pub use own_rare::RARE_CORPSE_REACH_METERS;
