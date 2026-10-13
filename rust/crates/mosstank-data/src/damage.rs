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
        let Some(raw) = mosstank_dotnet::try_parse_int(part) else {
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

/// The element numbers VTank's profiles store (`MyMonsters` columns, the
/// ammunition table), which are not [`MonsterDamageType`]'s own.
pub mod vtank_element {
    use super::MonsterDamageType;

    #[allow(missing_docs)]
    pub const PIERCE: i32 = 0;
    #[allow(missing_docs)]
    pub const BLUDGEON: i32 = 1;
    #[allow(missing_docs)]
    pub const SLASH: i32 = 2;
    #[allow(missing_docs)]
    pub const ACID: i32 = 3;
    #[allow(missing_docs)]
    pub const LIGHTNING: i32 = 4;
    #[allow(missing_docs)]
    pub const COLD: i32 = 5;
    #[allow(missing_docs)]
    pub const FIRE: i32 = 6;
    #[allow(missing_docs)]
    pub const HARM: i32 = 7;
    #[allow(missing_docs)]
    pub const AUTO: i32 = 8;
    #[allow(missing_docs)]
    pub const VOID: i32 = 9;
    #[allow(missing_docs)]
    pub const DRAIN_AUTO: i32 = 10;
    #[allow(missing_docs)]
    pub const PRISMATIC: i32 = 11;
    #[allow(missing_docs)]
    pub const RANDOM: i32 = 12;
    #[allow(missing_docs)]
    pub const FISTS: i32 = 13;
    #[allow(missing_docs)]
    pub const NONE: i32 = 98;
    #[allow(missing_docs)]
    pub const PHYSICAL: i32 = 99;
    /// The old database id for prismatic, read as prismatic.
    pub const PRISMATIC_DATABASE_ENTRY_OLD: i32 = 100;
    /// The player's own automatic choice.
    pub const P_AUTO: i32 = 101;

    /// The damage type a stored number names, `fallback` for any other.
    #[must_use]
    pub fn to_monster_damage_type(value: i32, fallback: MonsterDamageType) -> MonsterDamageType {
        match value {
            PIERCE => MonsterDamageType::Pierce,
            BLUDGEON => MonsterDamageType::Bludgeon,
            SLASH => MonsterDamageType::Slash,
            ACID => MonsterDamageType::Acid,
            LIGHTNING => MonsterDamageType::Electric,
            COLD => MonsterDamageType::Cold,
            FIRE => MonsterDamageType::Fire,
            HARM => MonsterDamageType::Harm,
            AUTO => MonsterDamageType::Auto,
            VOID => MonsterDamageType::VoidBasic,
            DRAIN_AUTO => MonsterDamageType::DrainAuto,
            PRISMATIC | PRISMATIC_DATABASE_ENTRY_OLD => MonsterDamageType::Prismatic,
            RANDOM => MonsterDamageType::Random,
            FISTS => MonsterDamageType::Fists,
            NONE => MonsterDamageType::None,
            PHYSICAL => MonsterDamageType::Physical,
            P_AUTO => MonsterDamageType::PlayerAuto,
            _ => fallback,
        }
    }

    /// The number a damage type is stored as; nether stores as void. (The C#
    /// takes a fallback for numbers outside its enum, which a Rust value
    /// cannot be.)
    #[must_use]
    pub fn from_monster_damage_type(value: MonsterDamageType) -> i32 {
        match value {
            MonsterDamageType::Pierce => PIERCE,
            MonsterDamageType::Bludgeon => BLUDGEON,
            MonsterDamageType::Slash => SLASH,
            MonsterDamageType::Acid => ACID,
            MonsterDamageType::Electric => LIGHTNING,
            MonsterDamageType::Cold => COLD,
            MonsterDamageType::Fire => FIRE,
            MonsterDamageType::Harm => HARM,
            MonsterDamageType::Auto => AUTO,
            MonsterDamageType::VoidBasic | MonsterDamageType::Nether => VOID,
            MonsterDamageType::DrainAuto => DRAIN_AUTO,
            MonsterDamageType::Prismatic => PRISMATIC,
            MonsterDamageType::Random => RANDOM,
            MonsterDamageType::Fists => FISTS,
            MonsterDamageType::None => NONE,
            MonsterDamageType::Physical => PHYSICAL,
            MonsterDamageType::PlayerAuto => P_AUTO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: DamageElementNumbersAreVtanksNotMossTanks
    #[test]
    fn damage_element_numbers_are_vtanks_not_mosstanks() {
        use vtank_element::{from_monster_damage_type, to_monster_damage_type};
        let auto = MonsterDamageType::Auto;
        assert_eq!(from_monster_damage_type(MonsterDamageType::Electric), 4);
        assert_eq!(from_monster_damage_type(MonsterDamageType::VoidBasic), 9);
        assert_eq!(from_monster_damage_type(MonsterDamageType::None), 98);
        assert_eq!(from_monster_damage_type(MonsterDamageType::PlayerAuto), 101);
        assert_eq!(to_monster_damage_type(4, auto), MonsterDamageType::Electric);
        assert_eq!(
            to_monster_damage_type(101, auto),
            MonsterDamageType::PlayerAuto
        );
        // The legacy prismatic database id folds onto Prismatic.
        assert_eq!(
            to_monster_damage_type(100, auto),
            MonsterDamageType::Prismatic
        );
    }

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
