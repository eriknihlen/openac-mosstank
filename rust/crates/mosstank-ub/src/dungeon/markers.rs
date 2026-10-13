//! The eleven kinds of mark the dungeon map draws, each with its own row of
//! settings under `DungeonMaps.Display.Markers`, and how a world object is
//! sorted into one and named for the map (`DungeonMarkers`).

use ac_plugin_abi::world::{ObjectClass, WorldObject};

/// A kind of mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DungeonMarkerKind {
    /// The character.
    You,
    /// Other players.
    Others,
    /// Things that can be picked up.
    Items,
    /// Monsters.
    Monsters,
    /// Non-player characters and shopkeepers.
    NPCs,
    /// The character's own corpse.
    MyCorpse,
    /// Everybody else's.
    OtherCorpses,
    /// Portals.
    Portals,
    /// Containers.
    Containers,
    /// Doors.
    Doors,
    /// Anything the list does not name.
    EverythingElse,
}

/// Every kind, in the order the settings rows list them.
pub const KINDS: [DungeonMarkerKind; 11] = [
    DungeonMarkerKind::You,
    DungeonMarkerKind::Others,
    DungeonMarkerKind::Items,
    DungeonMarkerKind::Monsters,
    DungeonMarkerKind::NPCs,
    DungeonMarkerKind::MyCorpse,
    DungeonMarkerKind::OtherCorpses,
    DungeonMarkerKind::Portals,
    DungeonMarkerKind::Containers,
    DungeonMarkerKind::Doors,
    DungeonMarkerKind::EverythingElse,
];

/// How one kind of mark is drawn, as the settings rows say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonMarkerStyle {
    /// Whether the kind is drawn at all.
    pub enabled: bool,
    /// The object's icon rather than a dot.
    pub use_icon: bool,
    /// The object's name beside the mark.
    pub show_label: bool,
    /// The dot's and the label's colour, as `#AARRGGBB`.
    pub color: u32,
    /// How many pixels across the mark is.
    pub size: i32,
}

impl DungeonMarkerStyle {
    /// A style.
    #[must_use]
    pub const fn new(
        enabled: bool,
        use_icon: bool,
        show_label: bool,
        color: u32,
        size: i32,
    ) -> Self {
        Self {
            enabled,
            use_icon,
            show_label,
            color,
            size,
        }
    }
}

/// The kind's name as the settings rows spell it.
#[must_use]
pub fn group_name(kind: DungeonMarkerKind) -> &'static str {
    match kind {
        DungeonMarkerKind::You => "You",
        DungeonMarkerKind::Others => "Others",
        DungeonMarkerKind::Items => "Items",
        DungeonMarkerKind::Monsters => "Monsters",
        DungeonMarkerKind::NPCs => "NPCs",
        DungeonMarkerKind::MyCorpse => "MyCorpse",
        DungeonMarkerKind::OtherCorpses => "OtherCorpses",
        DungeonMarkerKind::Portals => "Portals",
        DungeonMarkerKind::Containers => "Containers",
        DungeonMarkerKind::Doors => "Doors",
        DungeonMarkerKind::EverythingElse => "EverythingElse",
    }
}

/// Which kind of mark an object gets. A player is you or somebody else by
/// id; a corpse is yours by its name; shopkeepers count as NPCs. A thing
/// that can be picked up is an item; anything the list does not name is
/// drawn, if at all, under the catch-all kind.
#[must_use]
pub fn classify(subject: &WorldObject, self_id: u32, self_name: &str) -> DungeonMarkerKind {
    use ObjectClass as C;
    match subject.object_class {
        C::Player if subject.object_id == self_id => DungeonMarkerKind::You,
        C::Player => DungeonMarkerKind::Others,
        C::Monster => DungeonMarkerKind::Monsters,
        C::Npc | C::Vendor => DungeonMarkerKind::NPCs,
        C::Portal => DungeonMarkerKind::Portals,
        C::Corpse if subject.name == format!("Corpse of {self_name}") => {
            DungeonMarkerKind::MyCorpse
        }
        C::Corpse => DungeonMarkerKind::OtherCorpses,
        C::Door => DungeonMarkerKind::Doors,
        C::Container => DungeonMarkerKind::Containers,
        C::MeleeWeapon
        | C::Armor
        | C::Clothing
        | C::Jewelry
        | C::Food
        | C::Money
        | C::Misc
        | C::MissileWeapon
        | C::Gem
        | C::SpellComponent
        | C::Key
        | C::TradeNote
        | C::ManaStone
        | C::Plant
        | C::BaseCooking
        | C::BaseAlchemy
        | C::BaseFletching
        | C::CraftedCooking
        | C::CraftedAlchemy
        | C::CraftedFletching
        | C::HealingKit
        | C::Lockpick
        | C::WandStaffOrb
        | C::Bundle
        | C::Book
        | C::Journal
        | C::Foci
        | C::Salvage
        | C::Ust
        | C::Scroll => DungeonMarkerKind::Items,
        _ => DungeonMarkerKind::EverythingElse,
    }
}

/// Whether an object of this class moves about, and so is placed again as
/// it goes; everything else keeps the place it was first seen at.
#[must_use]
pub fn is_mover(object_class: ObjectClass) -> bool {
    matches!(
        object_class,
        ObjectClass::Player | ObjectClass::Monster | ObjectClass::CombatPet
    )
}

/// The name written beside the mark: a portal or an NPC loses the words
/// "Portal to" and "Portal", so the label is the place.
#[must_use]
pub fn label_of(subject: &WorldObject) -> String {
    if matches!(subject.object_class, ObjectClass::Portal | ObjectClass::Npc) {
        subject
            .name
            .replace("Portal to ", "")
            .replace(" Portal", "")
    } else {
        subject.name.clone()
    }
}
