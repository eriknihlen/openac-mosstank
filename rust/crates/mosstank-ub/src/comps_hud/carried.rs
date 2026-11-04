//! What the character carries, as the Edit items window lists it: one row
//! per distinct name among the objects the character owns (in a pack or
//! worn), with the first one's icon and the HUD's count of that name, and
//! the kinds the window filters by.
//!
//! The kinds come from what the client says an object is (its object
//! class) and the plugin's own reading of a pea: a spell component whose
//! name ends in " Pea", as the Consumables page tells a pea from a
//! component.

use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_dotnet::text::ordinal;

/// A kind of item the Edit items window filters by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CarriedKind {
    /// Everything.
    #[default]
    All,
    /// Spell components other than peas.
    SpellComponents,
    /// Peas and salvage.
    PeasAndSalvage,
    /// Healing kits.
    HealingKits,
    /// Food and potions (one object class).
    FoodAndPotions,
    /// Anything else.
    Other,
}

impl CarriedKind {
    /// The kinds in the order the filter offers them.
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::SpellComponents,
        Self::PeasAndSalvage,
        Self::HealingKits,
        Self::FoodAndPotions,
        Self::Other,
    ];

    /// What the filter calls it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::SpellComponents => "Spell components",
            Self::PeasAndSalvage => "Peas and salvage",
            Self::HealingKits => "Healing kits",
            Self::FoodAndPotions => "Food and potions",
            Self::Other => "Other",
        }
    }

    /// The kind the filter calls `label`, ignoring case.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| ordinal::equals_ignore_case(kind.label(), label.trim()))
    }

    /// The kind of an object (never [`Self::All`]).
    #[must_use]
    pub fn of(object: &WorldObject) -> Self {
        match object.object_class {
            ObjectClass::SpellComponent if object.name.ends_with(" Pea") => Self::PeasAndSalvage,
            ObjectClass::SpellComponent => Self::SpellComponents,
            ObjectClass::Salvage => Self::PeasAndSalvage,
            ObjectClass::HealingKit => Self::HealingKits,
            ObjectClass::Food => Self::FoodAndPotions,
            _ => Self::Other,
        }
    }

    /// Whether an item of kind `kind` passes this filter.
    #[must_use]
    pub fn admits(self, kind: Self) -> bool {
        self == Self::All || self == kind
    }
}

/// One name the character carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarriedItem {
    /// The name, exactly.
    pub name: String,
    /// The first such object's icon.
    pub icon_id: u32,
    /// How many: the HUD's count of the name.
    pub count: i32,
    /// The first such object's kind.
    pub kind: CarriedKind,
}

/// Every name the character owns something of, sorted by name ignoring
/// case.
#[must_use]
pub fn gather(objects: &[WorldObject]) -> Vec<CarriedItem> {
    let mut items: Vec<CarriedItem> = Vec::new();
    for object in objects
        .iter()
        .filter(|object| object.is_owned && !object.name.is_empty())
    {
        let count = object.stack_size.max(1);
        match items.iter_mut().find(|item| item.name == object.name) {
            Some(item) => {
                item.count = item.count.wrapping_add(count);
                if item.icon_id == 0 {
                    item.icon_id = object.icon_id;
                }
            }
            None => items.push(CarriedItem {
                name: object.name.clone(),
                icon_id: object.icon_id,
                count,
                kind: CarriedKind::of(object),
            }),
        }
    }
    items.sort_by(|a, b| {
        ordinal::compare_ignore_case(&a.name, &b.name)
            .then_with(|| ordinal::compare(&a.name, &b.name))
    });
    items
}

/// The rows the window shows: those of `kind` whose name holds `search`,
/// ignoring case (a blank search holds every name).
#[must_use]
pub fn filter<'a>(
    items: &'a [CarriedItem],
    search: &str,
    kind: CarriedKind,
) -> Vec<&'a CarriedItem> {
    let search = search.trim();
    items
        .iter()
        .filter(|item| kind.admits(item.kind))
        .filter(|item| search.is_empty() || ordinal::contains_ignore_case(&item.name, search))
        .collect()
}

/// How many of an item the character carries: every owned object of that
/// exact name, adding its stack size, or one for an object that has none.
#[must_use]
pub fn count_owned(objects: &[WorldObject], name: &str) -> i32 {
    objects
        .iter()
        .filter(|item| item.is_owned && item.name == name)
        .fold(0i32, |count, item| {
            count.wrapping_add(item.stack_size.max(1))
        })
}
