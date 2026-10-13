//! The UB tools a `/ub` command starts and the plugin's frame drives: the
//! item giver, the counter, stacking and cramming, tinkering, spending
//! experience and dressing by profile, and the name tags.
//!
//! - [`stack_cram`]: `/ub autostack`, `/ub autocram`
//!   (`InventoryMaintenance`, `StackCramCommandRun`).
//! - [`give`]: `/ub give`, `/ub ig` (`ProfileGiveController`).
//! - [`clear_bugged`]: `/ub clearbugged` (`ClearBuggedRun`).
//! - [`count`]: `/ub count` (`InventoryCountController`).
//! - [`equip`]: `/ub equip` (`EquipProfileController`).
//! - [`tinker`], [`tinker_page`]: the Tinker page and `/ub autotinker`,
//!   `getjob`, `tinkcalc` (`TinkerJobManager`, `MossTankPanel.Tinker`).
//! - [`xp`]: `/ub xp` (`ExperienceSpendController`).
//!
//! [`UbTools`] holds the ones a command reaches; the commands
//! ([`crate::commands::UbCommands`]) own it, and the plugin drives each
//! one's frame in the order the C# panel does (see [`UbTools`]).

pub mod clear_bugged;
pub mod count;
pub mod equip;
pub mod give;
pub mod nametags;
pub mod stack_cram;
pub mod tinker;
pub mod tinker_page;
pub mod xp;

use crate::commands::UbCall;
use clear_bugged::ClearBuggedRun;
use count::InventoryCountController;
use equip::EquipProfileController;
use give::ProfileGiveController;
use nametags::{CatalogNametagSettings, NametagController};
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
    /// `/ub clearbugged`.
    pub clear_bugged: ClearBuggedRun,
    /// The Tinker page, its run, and `/ub autotinker`, `getjob`, `tinkcalc`.
    pub tinker: TinkerPage,
    /// `/ub xp`.
    pub xp: ExperienceSpendController,
    /// `/ub equip`.
    pub equip: EquipProfileController,
    /// The name tags.
    pub nametags: NametagController,
}

impl UbTools {
    /// Nothing under way.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The name tags' frame, over the page's rows (the panel runs it first
    /// in its frame).
    pub fn tick_nametags(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        let settings = CatalogNametagSettings(&*call.settings);
        self.nametags.on_tick(call.host, &settings, elapsed_seconds);
    }

    /// The tinker run's frame (`TickTinkering`, before the command jump).
    pub fn tick_tinkering(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        self.tinker.manager_mut().on_tick(call, elapsed_seconds);
    }

    /// The prologue owners after the command jump, in the panel's order:
    /// the item giver, the counter, the vendor run (`vendor`, the plugin's,
    /// told whether it may act), the equipment profile, then the
    /// stack-or-cram pass; each may act only while none before it owns the
    /// character. `can_act` is false while a buff burst or the command
    /// jump holds it. Whether any of them owns the character this frame.
    pub fn tick_prologue(
        &mut self,
        call: &mut UbCall<'_>,
        elapsed_seconds: f64,
        can_act: bool,
        vendor: &mut dyn FnMut(&mut UbCall<'_>, bool) -> bool,
    ) -> bool {
        let give = self.give.tick(call, elapsed_seconds, can_act);
        let count = self.count.tick(call, elapsed_seconds, can_act && !give);
        let vendor = vendor(call, can_act && !give && !count);
        let equip = self
            .equip
            .tick(call, elapsed_seconds, can_act && !give && !count && !vendor);
        let stack_cram = self.stack_cram.tick(
            call,
            elapsed_seconds,
            can_act && !give && !count && !vendor && !equip,
        );
        give || count || vendor || equip || stack_cram
    }

    /// The tools that never own the character, after the prologue: the
    /// experience spender (it refuses to run while the macro is on) and
    /// `/ub clearbugged` (it only asks for descriptions).
    pub fn tick_after_prologue(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        self.xp.tick(call, elapsed_seconds);
        self.tick_clear_bugged(call, elapsed_seconds);
    }

    /// A session ended or began (`ResetSessionScopedControllers`, the UB
    /// tools' part): every run is put down without a word.
    pub fn reset_session(&mut self, call: &mut UbCall<'_>) {
        self.give.reset();
        self.stack_cram.reset();
        self.clear_bugged.reset();
        self.count.reset();
        self.equip.reset(&mut call.locks.borrow_mut());
        self.xp.reset();
    }
}

impl UbTools {
    /// `/ub clearbugged`: starts a run, its lines said as the panel's
    /// transcript lines.
    pub fn start_clear_bugged(&mut self, call: &mut UbCall<'_>) {
        let mut lines = Vec::new();
        self.clear_bugged
            .start(&mut *call.host, &mut |line| lines.push(line));
        for line in lines {
            call.write_transcript_line(&line);
        }
    }

    /// One frame of `/ub clearbugged` (not a prologue owner: it only asks for
    /// descriptions, one at a time, and never moves the character).
    pub fn tick_clear_bugged(
        &mut self,
        call: &mut crate::commands::UbCall<'_>,
        elapsed_seconds: f64,
    ) {
        let mut lines = Vec::new();
        self.clear_bugged
            .tick(&mut *call.host, elapsed_seconds, &mut |line| {
                lines.push(line)
            });
        for line in lines {
            call.write_transcript_line(&line);
        }
    }
}
