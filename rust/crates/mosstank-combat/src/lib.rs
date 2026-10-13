//! MossTank's fight.
//!
//! - [`controller`]: the fight (`CombatController`).
//! - [`health`]: the fought monster's health (`MonsterHealthTracker`).
//! - [`mover`]: the close-in mover and the heading helpers (`NavigationMover`).
//! - [`gate`]: readying a combat stance (`CombatModeGate`).
//! - [`debuff`]: the debuff chain, the debuff spells and what has landed.
//! - [`dispel`]: clearing vulnerabilities (`DispelController`).
//! - [`drain`]: the drain arm's planner (`CorpseDrainPlan`).
//! - [`failure`]: the monsters given up on (`CombatFailureTracker`).
//! - [`item_debuff`]: where a debuff can come from (`CombatItemDebuffPlanner`).
//! - [`power`]: the automatic attack power (`AutoAttackPower`).
//! - [`selection`]: which monster to fight (`CombatTargetSelector`).
//!
//! The client comes in through `mosstank-automation`; settings are handed
//! in on each call, so a profile load is seen at once.

use std::rc::Rc;

use ac_plugin_abi::combat::CombatMode;
use mosstank_automation::Automation;
pub use mosstank_meta::scheduler::MacroLogChannel;

pub mod controller;
pub mod debuff;
pub mod dispel;
pub mod drain;
pub mod failure;
pub mod gate;
pub mod health;
pub mod item_debuff;
pub mod mover;
pub mod pets;
pub mod power;
pub mod selection;

pub use gate::CombatModeGate;

/// Where a controller's macro log lines go (the C#'s `Log` callbacks).
pub type MacroLog = Rc<dyn Fn(MacroLogChannel, &str)>;

/// A combat mode's name, as .NET prints the enum.
#[must_use]
pub fn mode_name(mode: CombatMode) -> &'static str {
    match mode {
        CombatMode::Unknown => "Unknown",
        CombatMode::Peace => "Peace",
        CombatMode::Melee => "Melee",
        CombatMode::Missile => "Missile",
        CombatMode::Magic => "Magic",
    }
}

/// A system line in chat, with MossTank's tag (`MossTankChat.Post`).
pub fn post(automation: &mut dyn Automation, text: &str) {
    if text.starts_with("[MossTank]") {
        automation.post_system_message(text);
    } else {
        automation.post_system_message(&format!("[MossTank] {text}"));
    }
}

/// A flight shape as the path query names it (the data tables use the
/// world module's twin of the same enum).
#[must_use]
pub fn query_path_kind(
    kind: ac_plugin_abi::world::ProjectilePathKind,
) -> ac_plugin_abi::projectile::ProjectilePathKind {
    use ac_plugin_abi::projectile::ProjectilePathKind as Query;
    use ac_plugin_abi::world::ProjectilePathKind as Shape;
    match kind {
        Shape::Straight => Query::Straight,
        Shape::Arc => Query::Arc,
        Shape::Missile => Query::Missile,
    }
}
