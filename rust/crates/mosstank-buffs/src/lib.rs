//! MossTank's buffs, vitals and mana.
//!
//! - [`profile`]: the buff lines (self buffs grouped by family, classified
//!   by what they raise).
//! - [`plan`]: the buff plan (which lines a pass casts, at which tier, in
//!   what order).
//! - [`castability`]: whether a tier can be cast now, read from the client.
//! - [`vitals`]: the vital plan (which vital needs recharging, the mana
//!   upkeep, the strongest spell by name stem).
//! - [`recharge`]: the ways a vital is restored and the helper's pick.
//! - [`recharge_controller`]: the recharge and helper rules, one use at a
//!   time, watched to the server's answer.
//!
//! The client comes in through `mosstank-automation`.

pub mod castability;
pub mod plan;
pub mod profile;
pub mod recharge;
pub mod recharge_controller;
pub mod vitals;
