//! Damage types as the monster rules and the game database name them.

/// A damage type (the C# `MonsterDamageType`; profiles store the number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(missing_docs)]
pub enum MonsterDamageType {
    #[default]
    Auto = 0,
    Slash,
    Pierce,
    Bludgeon,
    Cold,
    Fire,
    Acid,
    Electric,
    Nether,
    VoidBasic,
    DrainAuto,
    Harm,
    None,
    PlayerAuto,
    Prismatic,
    Random,
    Fists,
    Physical,
}

impl MonsterDamageType {
    /// Every type, in number order.
    pub const ALL: [Self; 18] = [
        Self::Auto,
        Self::Slash,
        Self::Pierce,
        Self::Bludgeon,
        Self::Cold,
        Self::Fire,
        Self::Acid,
        Self::Electric,
        Self::Nether,
        Self::VoidBasic,
        Self::DrainAuto,
        Self::Harm,
        Self::None,
        Self::PlayerAuto,
        Self::Prismatic,
        Self::Random,
        Self::Fists,
        Self::Physical,
    ];

    /// The type with this stored number.
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        usize::try_from(value)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
    }
}

/// The order the seven elements are tried in when a monster lists none.
pub const UNLISTED_ELEMENT_ORDER: [MonsterDamageType; 7] = [
    MonsterDamageType::Pierce,
    MonsterDamageType::Bludgeon,
    MonsterDamageType::Slash,
    MonsterDamageType::Acid,
    MonsterDamageType::Electric,
    MonsterDamageType::Cold,
    MonsterDamageType::Fire,
];

/// A game-database element list (`3;6;0`): its elements in order, each
/// once; numbers that are not elements are left out.
#[must_use]
pub fn parse_damage_elements(text: &str) -> Vec<MonsterDamageType> {
    let mut result = Vec::new();
    if text.trim().is_empty() {
        return result;
    }
    for part in text.split(';') {
        // int.TryParse with the default styles: white space and a sign.
        let Some(raw) = openac_plugin_compat::number::parse_i32(
            part,
            openac_plugin_compat::number::NumberStyles::INTEGER,
        ) else {
            continue;
        };
        let mapped = match raw {
            0 => MonsterDamageType::Pierce,
            1 => MonsterDamageType::Bludgeon,
            2 => MonsterDamageType::Slash,
            3 => MonsterDamageType::Acid,
            4 => MonsterDamageType::Electric,
            5 => MonsterDamageType::Cold,
            6 => MonsterDamageType::Fire,
            _ => continue,
        };
        if !result.contains(&mapped) {
            result.push(mapped);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_lists_keep_order_and_drop_repeats_and_strangers() {
        assert_eq!(
            parse_damage_elements("6;3; 6;x;9;0"),
            [
                MonsterDamageType::Fire,
                MonsterDamageType::Acid,
                MonsterDamageType::Pierce
            ]
        );
        assert!(parse_damage_elements("  ").is_empty());
    }
}
