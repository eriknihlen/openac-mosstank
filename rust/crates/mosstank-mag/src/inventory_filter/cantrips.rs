//! The cantrip checkboxes: the website's six groups and 47 names (the
//! owner's MosswartOverlord inventory page, `constants.ts`, without the
//! `Legendary ` prefix), each matched at a tier or better.

/// A cantrip's tier, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CantripTier {
    /// Minor.
    Minor,
    /// Major.
    Major,
    /// Epic.
    Epic,
    /// Legendary, the default.
    #[default]
    Legendary,
}

impl CantripTier {
    /// Every tier, lowest first.
    pub const ALL: [Self; 4] = [Self::Minor, Self::Major, Self::Epic, Self::Legendary];

    /// Its name, as the spell names begin.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Minor => "Minor",
            Self::Major => "Major",
            Self::Epic => "Epic",
            Self::Legendary => "Legendary",
        }
    }
}

/// One group of cantrips.
#[derive(Debug, Clone, Copy)]
pub struct CantripGroup {
    /// The group's name.
    pub name: &'static str,
    /// Its cantrips' names, without a tier.
    pub cantrips: &'static [&'static str],
}

/// The groups, in the website's order. A cantrip's index is its place in
/// them all, counted from 0 ([`cantrip_index`]).
pub const GROUPS: &[CantripGroup] = &[
    CantripGroup {
        name: "Attributes",
        cantrips: &[
            "Strength",
            "Endurance",
            "Quickness",
            "Coordination",
            "Willpower",
            "Focus",
        ],
    },
    CantripGroup {
        name: "Weapon skills",
        cantrips: &[
            "Heavy Weapon Aptitude",
            "Light Weapon Aptitude",
            "Finesse Weapon Aptitude",
            "Missile Weapon Aptitude",
            "Two Handed Combat Aptitude",
            "Dual Wield Aptitude",
            "Shield Aptitude",
            "Sneak Attack Prowess",
            "Dirty Fighting Prowess",
            "Recklessness Prowess",
            "Defender",
            "Blood Thirst",
        ],
    },
    CantripGroup {
        name: "Magic",
        cantrips: &[
            "War Magic Aptitude",
            "Void Magic Aptitude",
            "Creature Enchantment Aptitude",
            "Item Enchantment Aptitude",
            "Life Magic Aptitude",
            "Mana Conversion Prowess",
            "Arcane Prowess",
            "Hermetic Link",
            "Spirit Thirst",
            "Magic Resistance",
        ],
    },
    CantripGroup {
        name: "Utility",
        cantrips: &[
            "Summoning Prowess",
            "Healing Prowess",
            "Leadership",
            "Deception Prowess",
            "Person Attunement",
            "Magic Item Tinkering Expertise",
        ],
    },
    CantripGroup {
        name: "Defense",
        cantrips: &[
            "Invulnerability",
            "Impenetrability",
            "Impregnability",
            "Armor",
        ],
    },
    CantripGroup {
        name: "Wards & Banes",
        cantrips: &[
            "Flame Ward",
            "Frost Ward",
            "Acid Ward",
            "Storm Ward",
            "Slashing Ward",
            "Piercing Ward",
            "Bludgeoning Ward",
            "Piercing Bane",
            "Storm Bane",
        ],
    },
];

/// Every cantrip's name in index order.
pub fn names() -> impl Iterator<Item = &'static str> {
    GROUPS
        .iter()
        .flat_map(|group| group.cantrips.iter().copied())
}

/// The cantrip of `index`.
#[must_use]
pub fn cantrip_name(index: usize) -> Option<&'static str> {
    names().nth(index)
}

/// The index of the cantrip named `name` (ignoring case).
#[must_use]
pub fn cantrip_index(name: &str) -> Option<usize> {
    names().position(|cantrip| cantrip.eq_ignore_ascii_case(name))
}

/// The cantrip a spell is, and its tier: its name is `"<Tier> <Cantrip>"`
/// exactly, ignoring case.
#[must_use]
pub fn cantrip_of(spell_name: &str) -> Option<(usize, CantripTier)> {
    let (prefix, rest) = spell_name.split_once(' ')?;
    let tier = CantripTier::ALL
        .into_iter()
        .find(|tier| tier.name().eq_ignore_ascii_case(prefix))?;
    cantrip_index(rest).map(|index| (index, tier))
}

/// The spell is the cantrip of `index` at `tier` or a higher one.
#[must_use]
pub fn spell_matches(spell_name: &str, index: usize, tier: CantripTier) -> bool {
    cantrip_of(spell_name).is_some_and(|(found, found_tier)| found == index && found_tier >= tier)
}
