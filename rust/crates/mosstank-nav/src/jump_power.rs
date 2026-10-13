//! How a jump's hold time becomes the power it is asked of the client with
//! (`JumpPower`). A full charge takes one second, so every millisecond held
//! is a thousandth of it; the client leaves with exactly that power rather
//! than with whatever the frames it saw the key held for add up to.

/// The least power the client takes (`float.Epsilon`, the smallest positive
/// `f32`). A hold of nothing is a tap, and the client takes no jump of no
/// power at all, so a tap asks for the smallest power above it: a hop too
/// small to lift the character.
pub const LEAST: f32 = f32::from_bits(1);

/// The power a hold of `milliseconds` asks for: a thousandth of a full
/// charge per millisecond, at least [`LEAST`] and at most a full charge.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn from_milliseconds(milliseconds: i32) -> f32 {
    (milliseconds as f32 / 1000.0).clamp(LEAST, 1.0)
}

/// The seconds the client holds the charge for when asked for a hold of
/// `milliseconds`: the hold itself, up to the one second a full charge
/// takes.
#[must_use]
pub fn charge_seconds(milliseconds: i32) -> f64 {
    f64::from(milliseconds.clamp(0, 1000)) / 1000.0
}
