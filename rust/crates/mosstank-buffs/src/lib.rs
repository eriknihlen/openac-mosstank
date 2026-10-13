//! MossTank's buffs, vitals and mana.
//!
//! - [`profile`]: the buff lines (self buffs grouped by family, classified
//!   by what they raise).
//! - [`plan`]: the buff plan (which lines a pass casts, at which tier, in
//!   what order).
//! - [`vitals`]: the vital plan (which vital needs recharging, the mana
//!   upkeep, the strongest spell by name stem).
//!
//! Pure: the client comes in through `mosstank-automation`.

pub mod plan;
pub mod profile;
pub mod vitals;
