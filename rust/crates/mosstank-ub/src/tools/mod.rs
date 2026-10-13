//! The UB tools a `/ub` command starts and the plugin's frame drives: the
//! item giver, the counter, stacking and cramming, tinkering, spending
//! experience and dressing by profile, and the name tags.
//!
//! - [`stack_cram`]: `/ub autostack`, `/ub autocram`
//!   (`InventoryMaintenance`, `StackCramCommandRun`).
//! - [`give`]: `/ub give`, `/ub ig` (`ProfileGiveController`).
//! - [`count`]: `/ub count` (`InventoryCountController`).
//! - [`equip`]: `/ub equip` (`EquipProfileController`).
//! - [`tinker`], [`tinker_page`]: the Tinker page and `/ub autotinker`,
//!   `getjob`, `tinkcalc` (`TinkerJobManager`, `MossTankPanel.Tinker`).
//! - [`xp`]: `/ub xp` (`ExperienceSpendController`).
//!
//! [`UbTools`] holds the ones a command reaches; the commands
//! ([`crate::commands::UbCommands`]) own it, and the plugin drives each
//! one's frame in the order the C# panel does (see [`UbTools`]).

pub mod count;
pub mod equip;
pub mod give;
pub mod stack_cram;
pub mod tinker;
pub mod tinker_page;
pub mod xp;

use count::InventoryCountController;
use equip::EquipProfileController;
use give::ProfileGiveController;
use stack_cram::StackCramCommandRun;
use tinker_page::TinkerPage;
use xp::ExperienceSpendController;

/// The tools the `/ub` commands start, each driven by the plugin's frame.
///
/// The plugin's frame (`MossTankPanel.OnTick`) drives the prologue owners
/// in this order, each told it may act only while none before it owns the
/// character: the command jump, the item giver, the counter, the vendor
/// run, the equipment profile, then the stack-or-cram pass.
#[derive(Debug, Default)]
pub struct UbTools {
    /// `/ub give` and `/ub ig`.
    pub give: ProfileGiveController,
    /// `/ub count`.
    pub count: InventoryCountController,
    /// `/ub autostack` and `/ub autocram`.
    pub stack_cram: StackCramCommandRun,
    /// The Tinker page, its run, and `/ub autotinker`, `getjob`, `tinkcalc`.
    pub tinker: TinkerPage,
    /// `/ub xp`.
    pub xp: ExperienceSpendController,
    /// `/ub equip`.
    pub equip: EquipProfileController,
}

impl UbTools {
    /// Nothing under way.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
