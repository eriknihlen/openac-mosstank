//! What the combat tracker keeps (Mag-Tools' `CombatInfo`, `AetheriaInfo`,
//! `CloakInfo`): per source and target, the attacks and damage by attack
//! type and element, and the surges. The maps keep their insertion order,
//! which the stats file is written in.

/// How an attack was made (Mag-Tools' `AttackType`; its names, misspelling
/// included, are the stats file's element names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttackType {
    /// Not known (a killing blow).
    Unknown,
    /// Melee or missile.
    MeleeMissle,
    /// A spell.
    Magic,
}

impl AttackType {
    /// Every attack type, in the enum's order.
    pub const ALL: [Self; 3] = [Self::Unknown, Self::MeleeMissle, Self::Magic];

    /// The name the stats file writes (`Enum.ToString`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::MeleeMissle => "MeleeMissle",
            Self::Magic => "Magic",
        }
    }

    /// The attack type a stats file names (`Enum.Parse`, case-sensitive).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// The damage's element (Mag-Tools' `DamageElement`; Nether is counted as
/// Typeless).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageElement {
    /// None: a Gossamer Flesh cast.
    None,
    /// Not known: a failed attack, a kill, a line naming no element.
    Unknown,
    /// Typeless: nether, health, stamina and mana damage.
    Typeless,
    /// Slashing.
    Slash,
    /// Piercing.
    Pierce,
    /// Bludgeoning.
    Bludge,
    /// Fire.
    Fire,
    /// Cold.
    Cold,
    /// Acid.
    Acid,
    /// Electric.
    Electric,
}

impl DamageElement {
    /// Every element, in the enum's order.
    pub const ALL: [Self; 10] = [
        Self::None,
        Self::Unknown,
        Self::Typeless,
        Self::Slash,
        Self::Pierce,
        Self::Bludge,
        Self::Fire,
        Self::Cold,
        Self::Acid,
        Self::Electric,
    ];

    /// The name the stats file writes (`Enum.ToString`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Unknown => "Unknown",
            Self::Typeless => "Typeless",
            Self::Slash => "Slash",
            Self::Pierce => "Pierce",
            Self::Bludge => "Bludge",
            Self::Fire => "Fire",
            Self::Cold => "Cold",
            Self::Acid => "Acid",
            Self::Electric => "Electric",
        }
    }

    /// The element a stats file names (`Enum.Parse`, case-sensitive).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|element| element.name() == name)
    }
}

/// One attack or failure the chat told of (Mag-Tools' `CombatEventArgs`,
/// with the fields the tracker uses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatEvent {
    /// Who attacked.
    pub source: String,
    /// Who was attacked.
    pub target: String,
    /// How.
    pub attack_type: AttackType,
    /// The damage's element.
    pub element: DamageElement,
    /// It was evaded or resisted.
    pub is_failed: bool,
    /// A critical hit (the line says `Critical hit!` anywhere).
    pub is_crit: bool,
    /// The line told of a kill.
    pub is_killing_blow: bool,
    /// The damage, 0 when the line gives none (or none that reads).
    pub damage: i32,
}

impl CombatEvent {
    /// A hit for `damage`.
    #[must_use]
    pub fn damage(
        source: &str,
        target: &str,
        attack_type: AttackType,
        element: DamageElement,
        is_crit: bool,
        damage: i32,
    ) -> Self {
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
            attack_type,
            element,
            is_failed: false,
            is_crit,
            is_killing_blow: false,
            damage,
        }
    }

    /// An evaded or resisted attack.
    #[must_use]
    pub fn failed(source: &str, target: &str, attack_type: AttackType) -> Self {
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
            attack_type,
            element: DamageElement::Unknown,
            is_failed: true,
            is_crit: false,
            is_killing_blow: false,
            damage: 0,
        }
    }

    /// A kill by `source`.
    #[must_use]
    pub fn killing_blow(source: &str, target: &str) -> Self {
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
            attack_type: AttackType::Unknown,
            element: DamageElement::Unknown,
            is_failed: false,
            is_crit: false,
            is_killing_blow: true,
            damage: 0,
        }
    }
}

/// An aetheria or cloak surge the chat told of (Mag-Tools' two
/// `SurgeEventArgs`; the type is not kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurgeEvent {
    /// Whose aetheria or cloak surged ("" when the line does not say).
    pub source: String,
    /// On whom.
    pub target: String,
}

impl SurgeEvent {
    /// A surge of `source`'s on `target`.
    #[must_use]
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
        }
    }
}

/// The attacks of one attack type with one element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DamageByElement {
    /// Every attack, kills and failures included.
    pub total_attacks: i32,
    /// The evaded and resisted ones.
    pub failed_attacks: i32,
    /// The critical hits.
    pub crits: i32,
    /// The damage of the hits that were not critical.
    pub total_normal_damage: i32,
    /// The largest of those.
    pub max_normal_damage: i32,
    /// The damage of the critical hits.
    pub total_crit_damage: i32,
    /// The largest of those.
    pub max_crit_damage: i32,
}

impl DamageByElement {
    /// All the damage.
    #[must_use]
    pub fn damage(&self) -> i32 {
        self.total_normal_damage
            .wrapping_add(self.total_crit_damage)
    }
}

/// An insertion-ordered map (Mag-Tools' `Dictionary`, which enumerates in
/// insertion order while nothing is removed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedMap<K, V> {
    entries: Vec<(K, V)>,
}

impl<K, V> Default for OrderedMap<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K: PartialEq + Copy, V: Default> OrderedMap<K, V> {
    /// The value at `key`, added (at the end) when missing.
    pub fn entry(&mut self, key: K) -> &mut V {
        let at = match self.entries.iter().position(|(k, _)| *k == key) {
            Some(at) => at,
            None => {
                self.entries.push((key, V::default()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[at].1
    }

    /// `Dictionary.Add`: false (nothing added) when the key is there.
    pub fn add(&mut self, key: K, value: V) -> bool {
        if self.get(key).is_some() {
            return false;
        }
        self.entries.push((key, value));
        true
    }

    /// The value at `key`.
    #[must_use]
    pub fn get(&self, key: K) -> Option<&V> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// The entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// The values in insertion order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// No entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The sum and largest values over some element buckets.
fn totals<'a>(buckets: impl Iterator<Item = &'a DamageByElement>) -> DamageByElement {
    let mut sum = DamageByElement::default();
    for bucket in buckets {
        sum.total_attacks = sum.total_attacks.wrapping_add(bucket.total_attacks);
        sum.failed_attacks = sum.failed_attacks.wrapping_add(bucket.failed_attacks);
        sum.crits = sum.crits.wrapping_add(bucket.crits);
        sum.total_normal_damage = sum
            .total_normal_damage
            .wrapping_add(bucket.total_normal_damage);
        sum.total_crit_damage = sum.total_crit_damage.wrapping_add(bucket.total_crit_damage);
        sum.max_normal_damage = sum.max_normal_damage.max(bucket.max_normal_damage);
        sum.max_crit_damage = sum.max_crit_damage.max(bucket.max_crit_damage);
    }
    sum
}

/// The attacks of one attack type, by element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DamageByAttackType {
    /// By element, in the order first seen.
    pub by_element: OrderedMap<DamageElement, DamageByElement>,
}

impl DamageByAttackType {
    /// The sums over the elements (the largest values for the two maxima).
    #[must_use]
    pub fn totals(&self) -> DamageByElement {
        totals(self.by_element.values())
    }
}

/// The attacks of one source on one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatInfo {
    /// Who attacked.
    pub source: String,
    /// Who was attacked.
    pub target: String,
    /// The kills.
    pub killing_blows: i32,
    /// By attack type, in the order first seen.
    pub by_attack_type: OrderedMap<AttackType, DamageByAttackType>,
}

impl CombatInfo {
    /// An empty info of `source` on `target`.
    #[must_use]
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
            killing_blows: 0,
            by_attack_type: OrderedMap::default(),
        }
    }

    /// `AddFromCombatEventArgs`: every event is one attack in its attack
    /// type's element bucket (kills and failures too).
    pub fn add(&mut self, event: &CombatEvent) {
        if event.is_killing_blow {
            self.killing_blows = self.killing_blows.wrapping_add(1);
        }
        let bucket = self
            .by_attack_type
            .entry(event.attack_type)
            .by_element
            .entry(event.element);
        bucket.total_attacks = bucket.total_attacks.wrapping_add(1);
        if event.is_failed {
            bucket.failed_attacks = bucket.failed_attacks.wrapping_add(1);
        }
        if event.is_crit {
            bucket.crits = bucket.crits.wrapping_add(1);
            bucket.total_crit_damage = bucket.total_crit_damage.wrapping_add(event.damage);
            if event.damage > bucket.max_crit_damage {
                bucket.max_crit_damage = event.damage;
            }
        } else {
            bucket.total_normal_damage = bucket.total_normal_damage.wrapping_add(event.damage);
            if event.damage > bucket.max_normal_damage {
                bucket.max_normal_damage = event.damage;
            }
        }
    }

    /// The sums over every attack type and element (the largest values
    /// for the two maxima).
    #[must_use]
    pub fn totals(&self) -> DamageByElement {
        totals(
            self.by_attack_type
                .values()
                .flat_map(|by_type| by_type.by_element.values()),
        )
    }

    /// All the damage.
    #[must_use]
    pub fn damage(&self) -> i32 {
        // The same as the sums' (wrapping sums add up in any order),
        // without working out the other five.
        self.by_attack_type
            .values()
            .flat_map(|by_type| by_type.by_element.values())
            .fold(0_i32, |sum, bucket| sum.wrapping_add(bucket.damage()))
    }
}

/// The surges of one source on one target (Mag-Tools' `AetheriaInfo` and
/// `CloakInfo`, which are the same).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurgeInfo {
    /// Whose aetheria or cloak.
    pub source: String,
    /// On whom.
    pub target: String,
    /// How many surges.
    pub total_surges: i32,
}

impl SurgeInfo {
    /// No surges of `source` on `target` yet.
    #[must_use]
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
            total_surges: 0,
        }
    }
}
