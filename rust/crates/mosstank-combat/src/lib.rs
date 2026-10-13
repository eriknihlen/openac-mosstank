//! MossTank's fight.
//!
//! - [`gate`]: readying a combat stance (`CombatModeGate`).
//!
//! The client comes in through `mosstank-automation`; settings are handed
//! in on each call, so a profile load is seen at once.

use std::rc::Rc;

use ac_plugin_abi::combat::CombatMode;
use mosstank_automation::Automation;
use mosstank_meta::scheduler::MacroLogChannel;

pub mod gate;

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
