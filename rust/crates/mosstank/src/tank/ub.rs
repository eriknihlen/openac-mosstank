//! The `/ub` word, through the UB tab's dispatcher ([`mosstank_ub::commands`]).

use super::TankCx;

/// Runs one line typed under `/ub`.
pub(crate) fn execute(cx: &mut TankCx<'_>, input: &str) {
    let _ = (cx, input);
}
