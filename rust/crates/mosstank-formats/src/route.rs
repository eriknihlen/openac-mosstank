//! The route model: waypoints and the navigation settings that hold them,
//! as the navigation controller runs them and the `.nav`, `.met` and `.af`
//! files hold them.

pub use ac_plugin_abi::world::Position;
use mosstank_dotnet::text::{casing, ordinal};
use openac_plugin_compat::number::format_f64;

/// How a route runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RouteMode {
    /// Loops back to the start.
    #[default]
    Circular,
    /// Runs forward, then backward.
    Linear,
    /// Follows a target.
    Target,
    /// Runs once; points are spent.
    Once,
}

impl RouteMode {
    /// The C# member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Circular => "Circular",
            Self::Linear => "Linear",
            Self::Target => "Target",
            Self::Once => "Once",
        }
    }
}

/// Why a route cannot step back to an earlier waypoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RouteStepBackBlock {
    /// It can.
    #[default]
    None,
    /// A follow route has no waypoints.
    FollowRoute,
    /// A once-through route spends its points and never goes back.
    OnceRoute,
    /// The route has no waypoints.
    EmptyRoute,
}

/// When the route hands a leg to the client's own pathing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClientPathing {
    /// The straight walk at every point; a stall is only said in chat.
    Never,
    /// The straight walk, and once per visit to a waypoint, when it has not
    /// covered ground for three seconds, the client walks that leg; the
    /// route takes the keys back however the client's walk ends.
    #[default]
    WhenStuck,
    /// Every leg goes to the client; a leg it cannot walk pauses the route.
    Always,
}

/// What a waypoint is (VTank's waypoint numbering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RouteWaypointType {
    /// 0: walk to a point.
    #[default]
    Point = 0,
    /// 1: use a portal by id (deprecated in VTank).
    Portal = 1,
    /// 2: cast a recall.
    Recall = 2,
    /// 3: wait.
    Pause = 3,
    /// 4: submit a chat line.
    ChatCommand = 4,
    /// 5: open a vendor.
    OpenVendor = 5,
    /// 6: use a portal (or NPC) by name.
    PortalByName = 6,
    /// 7: talk to an NPC.
    UseNpc = 7,
    /// 8: a checkpoint.
    Checkpoint = 8,
    /// 9: jump.
    Jump = 9,
}

impl RouteWaypointType {
    /// The type with VTank's number.
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::Point,
            1 => Self::Portal,
            2 => Self::Recall,
            3 => Self::Pause,
            4 => Self::ChatCommand,
            5 => Self::OpenVendor,
            6 => Self::PortalByName,
            7 => Self::UseNpc,
            8 => Self::Checkpoint,
            9 => Self::Jump,
            _ => return None,
        })
    }

    /// The C# member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Point => "Point",
            Self::Portal => "Portal",
            Self::Recall => "Recall",
            Self::Pause => "Pause",
            Self::ChatCommand => "ChatCommand",
            Self::OpenVendor => "OpenVendor",
            Self::PortalByName => "PortalByName",
            Self::UseNpc => "UseNpc",
            Self::Checkpoint => "Checkpoint",
            Self::Jump => "Jump",
        }
    }
}

/// The recalls a route knows by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(missing_docs)]
pub enum RouteRecallKind {
    #[default]
    PrimaryPortalRecall,
    SecondaryPortalRecall,
    LifestoneRecall,
    LifestoneSending,
    PortalRecall,
    RecallAphusLassel,
    RecallTheSanctuary,
    RecallToTheSingularityCaul,
    GlendenWoodRecall,
    AerlintheRecall,
    MountLetheRecall,
    UlgrimsRecall,
    BurRecall,
    ParadoxTouchedOlthoiInfestedAreaRecall,
    CallOfTheMhoireForge,
    ColosseumRecall,
    FacilityHubRecall,
    GearKnightInvasionAreaCampRecall,
    LostCityOfNeftetRecall,
    ReturnToTheKeep,
    RynthidRecall,
    ViridianRiseRecall,
    ViridianRiseGreatTreeRecall,
    CelestialHandStrongholdRecall,
    RadiantBloodStrongholdRecall,
    EldrytchWebStrongholdRecall,
    Marketplace,
}

impl RouteRecallKind {
    /// Every recall, in order.
    pub const ALL: [Self; 27] = [
        Self::PrimaryPortalRecall,
        Self::SecondaryPortalRecall,
        Self::LifestoneRecall,
        Self::LifestoneSending,
        Self::PortalRecall,
        Self::RecallAphusLassel,
        Self::RecallTheSanctuary,
        Self::RecallToTheSingularityCaul,
        Self::GlendenWoodRecall,
        Self::AerlintheRecall,
        Self::MountLetheRecall,
        Self::UlgrimsRecall,
        Self::BurRecall,
        Self::ParadoxTouchedOlthoiInfestedAreaRecall,
        Self::CallOfTheMhoireForge,
        Self::ColosseumRecall,
        Self::FacilityHubRecall,
        Self::GearKnightInvasionAreaCampRecall,
        Self::LostCityOfNeftetRecall,
        Self::ReturnToTheKeep,
        Self::RynthidRecall,
        Self::ViridianRiseRecall,
        Self::ViridianRiseGreatTreeRecall,
        Self::CelestialHandStrongholdRecall,
        Self::RadiantBloodStrongholdRecall,
        Self::EldrytchWebStrongholdRecall,
        Self::Marketplace,
    ];

    /// The C# member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PrimaryPortalRecall => "PrimaryPortalRecall",
            Self::SecondaryPortalRecall => "SecondaryPortalRecall",
            Self::LifestoneRecall => "LifestoneRecall",
            Self::LifestoneSending => "LifestoneSending",
            Self::PortalRecall => "PortalRecall",
            Self::RecallAphusLassel => "RecallAphusLassel",
            Self::RecallTheSanctuary => "RecallTheSanctuary",
            Self::RecallToTheSingularityCaul => "RecallToTheSingularityCaul",
            Self::GlendenWoodRecall => "GlendenWoodRecall",
            Self::AerlintheRecall => "AerlintheRecall",
            Self::MountLetheRecall => "MountLetheRecall",
            Self::UlgrimsRecall => "UlgrimsRecall",
            Self::BurRecall => "BurRecall",
            Self::ParadoxTouchedOlthoiInfestedAreaRecall => {
                "ParadoxTouchedOlthoiInfestedAreaRecall"
            }
            Self::CallOfTheMhoireForge => "CallOfTheMhoireForge",
            Self::ColosseumRecall => "ColosseumRecall",
            Self::FacilityHubRecall => "FacilityHubRecall",
            Self::GearKnightInvasionAreaCampRecall => "GearKnightInvasionAreaCampRecall",
            Self::LostCityOfNeftetRecall => "LostCityOfNeftetRecall",
            Self::ReturnToTheKeep => "ReturnToTheKeep",
            Self::RynthidRecall => "RynthidRecall",
            Self::ViridianRiseRecall => "ViridianRiseRecall",
            Self::ViridianRiseGreatTreeRecall => "ViridianRiseGreatTreeRecall",
            Self::CelestialHandStrongholdRecall => "CelestialHandStrongholdRecall",
            Self::RadiantBloodStrongholdRecall => "RadiantBloodStrongholdRecall",
            Self::EldrytchWebStrongholdRecall => "EldrytchWebStrongholdRecall",
            Self::Marketplace => "Marketplace",
        }
    }

    /// The name the game gives the recall spell.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::PrimaryPortalRecall => "Primary Portal Recall",
            Self::SecondaryPortalRecall => "Secondary Portal Recall",
            Self::LifestoneRecall => "Lifestone Recall",
            Self::LifestoneSending => "Lifestone Sending",
            Self::PortalRecall => "Portal Recall",
            Self::RecallAphusLassel => "Recall Aphus Lassel",
            Self::RecallTheSanctuary => "Recall the Sanctuary",
            Self::RecallToTheSingularityCaul => "Recall to the Singularity Caul",
            Self::GlendenWoodRecall => "Glenden Wood Recall",
            Self::AerlintheRecall => "Aerlinthe Recall",
            Self::MountLetheRecall => "Mount Lethe Recall",
            Self::UlgrimsRecall => "Ulgrim's Recall",
            Self::BurRecall => "Bur Recall",
            Self::ParadoxTouchedOlthoiInfestedAreaRecall => {
                "Paradox-touched Olthoi Infested Area Recall"
            }
            Self::CallOfTheMhoireForge => "Call of the Mhoire Forge",
            Self::ColosseumRecall => "Colosseum Recall",
            Self::FacilityHubRecall => "Facility Hub Recall",
            Self::GearKnightInvasionAreaCampRecall => "Gear Knight Invasion Area Camp Recall",
            Self::LostCityOfNeftetRecall => "Lost City of Neftet Recall",
            Self::ReturnToTheKeep => "Return to the Keep",
            Self::RynthidRecall => "Rynthid Recall",
            Self::ViridianRiseRecall => "Viridian Rise Recall",
            Self::ViridianRiseGreatTreeRecall => "Viridian Rise Great Tree Recall",
            Self::CelestialHandStrongholdRecall => "Celestial Hand Stronghold Recall",
            Self::RadiantBloodStrongholdRecall => "Radiant Blood Stronghold Recall",
            Self::EldrytchWebStrongholdRecall => "Eldrytch Web Stronghold Recall",
            Self::Marketplace => "Marketplace Recall",
        }
    }

    /// The short caption the route list shows.
    #[must_use]
    pub fn short_caption(self) -> &'static str {
        match self {
            Self::PrimaryPortalRecall => "Primary",
            Self::SecondaryPortalRecall => "Secondary",
            Self::LifestoneRecall => "LS",
            Self::LifestoneSending => "LS Sending",
            Self::PortalRecall => "Portal",
            Self::RecallAphusLassel => "Aphus",
            Self::RecallTheSanctuary => "Sanctuary",
            Self::RecallToTheSingularityCaul => "Caul",
            Self::GlendenWoodRecall => "GW",
            Self::AerlintheRecall => "Aerlinthe",
            Self::MountLetheRecall => "Mt. Lethe",
            Self::UlgrimsRecall => "Ulgrim's",
            Self::BurRecall => "Bur",
            Self::ParadoxTouchedOlthoiInfestedAreaRecall => "PtOIA",
            Self::CallOfTheMhoireForge => "Graveyard",
            Self::ColosseumRecall => "Colosseum",
            Self::FacilityHubRecall => "Fac. Hub",
            Self::GearKnightInvasionAreaCampRecall => "Gear K. Camp",
            Self::LostCityOfNeftetRecall => "Neftet",
            Self::ReturnToTheKeep => "Candeth",
            Self::RynthidRecall => "Rynthid",
            Self::ViridianRiseRecall => "VR Rocks",
            Self::ViridianRiseGreatTreeRecall => "VR Tree",
            Self::CelestialHandStrongholdRecall => "Soc. CH",
            Self::RadiantBloodStrongholdRecall => "Soc. RB",
            Self::EldrytchWebStrongholdRecall => "Soc. EW",
            Self::Marketplace => "Marketplace",
        }
    }

    /// The recall's spell id; the Marketplace has none (0: it is a command).
    #[must_use]
    pub fn spell_id(self) -> u32 {
        match self {
            Self::PrimaryPortalRecall => 48,
            Self::SecondaryPortalRecall => 2647,
            Self::LifestoneRecall => 1635,
            Self::LifestoneSending => 1636,
            Self::PortalRecall => 2645,
            Self::RecallAphusLassel => 2931,
            Self::RecallTheSanctuary => 2023,
            Self::RecallToTheSingularityCaul => 2943,
            Self::GlendenWoodRecall => 3865,
            Self::AerlintheRecall => 2041,
            Self::MountLetheRecall => 2813,
            Self::UlgrimsRecall => 2941,
            Self::BurRecall => 4084,
            Self::ParadoxTouchedOlthoiInfestedAreaRecall => 4198,
            Self::CallOfTheMhoireForge => 4128,
            Self::ColosseumRecall => 4213,
            Self::FacilityHubRecall => 5175,
            Self::GearKnightInvasionAreaCampRecall => 5330,
            Self::LostCityOfNeftetRecall => 5541,
            Self::ReturnToTheKeep => 4214,
            Self::RynthidRecall => 6150,
            Self::ViridianRiseRecall => 6321,
            Self::ViridianRiseGreatTreeRecall => 6322,
            Self::CelestialHandStrongholdRecall => 6325,
            Self::RadiantBloodStrongholdRecall => 6327,
            Self::EldrytchWebStrongholdRecall => 6326,
            Self::Marketplace => 0,
        }
    }

    /// A written recall name (the display name or the bare member name, any
    /// case) to its recall.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return None;
        }
        Self::ALL.into_iter().find(|candidate| {
            ordinal::equals_ignore_case(candidate.display_name(), trimmed)
                || ordinal::equals_ignore_case(candidate.name(), trimmed)
        })
    }

    /// A spell id back to its recall.
    #[must_use]
    pub fn from_spell_id(spell_id: u32) -> Option<Self> {
        if spell_id == 0 {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.spell_id() == spell_id)
    }
}

/// The direction a jump leaves in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RouteJumpDirection {
    /// Forward.
    #[default]
    Forward,
    /// Strafing left.
    StrafeLeft,
    /// Strafing right.
    StrafeRight,
    /// Backward (appended so saved routes keep their numbers).
    Backward,
}

impl RouteJumpDirection {
    /// The C# member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::StrafeLeft => "StrafeLeft",
            Self::StrafeRight => "StrafeRight",
            Self::Backward => "Backward",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::StrafeLeft => "Strafe Left",
            Self::StrafeRight => "Strafe Right",
            Self::Backward => "Backward",
            Self::Forward => "Forward",
        }
    }
}

/// Where a new waypoint goes in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RouteInsertMode {
    /// At the end.
    #[default]
    AddToEnd,
    /// Above the selected one.
    InsertAbove,
    /// Below the selected one.
    InsertBelow,
}

/// One waypoint.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteWaypoint {
    /// What it is.
    pub kind: RouteWaypointType,
    /// Where it is.
    pub position: Position,
    /// Where its target object stands (portal or NPC by name).
    pub reference_position: Position,
    /// Its target object's id.
    pub object_id: u32,
    /// Its target object's name.
    pub object_name: String,
    /// The object-class number the VTank NAV format carries, kept exactly as
    /// written for interchange.
    pub legacy_object_class: i32,
    /// The NAV format's "reference valid" flag, kept as written.
    pub legacy_reference_valid: bool,
    /// A chat command's text.
    pub text: String,
    /// A pause's length.
    pub duration_milliseconds: i32,
    /// A recall's kind.
    pub recall: RouteRecallKind,
    /// A recall's spell id.
    pub recall_spell_id: u32,
    /// A recall's spell name as written.
    pub recall_spell_name: String,
    /// A jump's heading.
    pub jump_heading_degrees: f64,
    /// The jump's shift flag as the route files store it: true holds shift,
    /// which in the game is the walk key, so the jump leaves at a walk;
    /// false leaves at the default run.
    pub jump_hold_shift: bool,
    /// How long a jump charges.
    pub jump_charge_milliseconds: i32,
    /// The direction a jump leaves in.
    pub jump_direction: RouteJumpDirection,
}

impl Default for RouteWaypoint {
    fn default() -> Self {
        Self {
            kind: RouteWaypointType::Point,
            position: Position::default(),
            reference_position: Position::default(),
            object_id: 0,
            object_name: String::new(),
            legacy_object_class: 0,
            legacy_reference_valid: true,
            text: String::new(),
            duration_milliseconds: 5000,
            recall: RouteRecallKind::PrimaryPortalRecall,
            recall_spell_id: 0,
            recall_spell_name: String::new(),
            jump_heading_degrees: 0.0,
            jump_hold_shift: false,
            jump_charge_milliseconds: 1000,
            jump_direction: RouteJumpDirection::Forward,
        }
    }
}

/// A point on the map as the route files carry it: east-west, north-south
/// and height in map units, outdoors, no cell.
#[must_use]
pub fn map_position(east_west: f64, north_south: f64, elevation: f64) -> Position {
    Position {
        cell_id: 0,
        east_west,
        north_south,
        elevation,
        heading_degrees: 0.0,
        is_outdoor: true,
    }
}

/// What a route needs from the spell catalogue to resolve a recall written
/// by name.
pub trait SpellLookup {
    /// The spell with exactly this name in the whole content table.
    fn find_by_name(&self, name: &str) -> Option<u32>;
    /// The known self buffs, attack spells and combat spells (id, name), in
    /// that order, for a host that does not carry the full table.
    fn known_spells(&self) -> Vec<(u32, String)>;
    /// A spell's name by id.
    fn name_of(&self, spell_id: u32) -> Option<String>;
}

/// A catalogue that knows no spells.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSpells;

impl SpellLookup for NoSpells {
    fn find_by_name(&self, _name: &str) -> Option<u32> {
        None
    }
    fn known_spells(&self) -> Vec<(u32, String)> {
        Vec::new()
    }
    fn name_of(&self, _spell_id: u32) -> Option<String> {
        None
    }
}

impl RouteWaypoint {
    /// A waypoint of `kind` at a map position.
    #[must_use]
    pub fn at(kind: RouteWaypointType, east_west: f64, north_south: f64, elevation: f64) -> Self {
        Self {
            kind,
            position: map_position(east_west, north_south, elevation),
            ..Self::default()
        }
    }

    /// The line the route list shows for this waypoint.
    #[must_use]
    pub fn display_text(&self) -> String {
        match self.kind {
            RouteWaypointType::Point => format!("Point: {}", format_position(&self.position)),
            RouteWaypointType::Portal | RouteWaypointType::PortalByName => {
                format!("Portal: {}", self.object_label())
            }
            RouteWaypointType::Recall => format!("Recall: {}", self.recall_label()),
            RouteWaypointType::Pause => format!(
                "Pause: {} seconds",
                format_f64(f64::from(self.duration_milliseconds) / 1000.0, "0.###")
                    .expect("a supported format")
            ),
            RouteWaypointType::ChatCommand => format!("Chat command: {}", self.text),
            RouteWaypointType::OpenVendor => {
                if self.object_id == 0 {
                    "Close Vendor".to_owned()
                } else {
                    format!("Open Vendor: {}", self.object_label())
                }
            }
            RouteWaypointType::UseNpc => format!("Use NPC: {}", self.object_label()),
            RouteWaypointType::Checkpoint => {
                format!("Checkpoint: {}", format_position(&self.position))
            }
            RouteWaypointType::Jump => format!(
                "Jump: {}d, {}ms{}, {}",
                format_f64(self.jump_heading_degrees, "0.0").expect("a supported format"),
                self.jump_charge_milliseconds,
                if self.jump_hold_shift { ", Shift" } else { "" },
                self.jump_direction.display_name()
            ),
        }
    }

    fn object_label(&self) -> String {
        if casing::is_white_space_only(&self.object_name) {
            format!("0x{:08X}", self.object_id)
        } else {
            self.object_name.clone()
        }
    }

    /// A recall's name: as written, else its spell id, else the recall's
    /// display name.
    #[must_use]
    pub fn recall_label(&self) -> String {
        if !casing::is_white_space_only(&self.recall_spell_name) {
            self.recall_spell_name.clone()
        } else if self.recall_spell_id != 0 {
            self.recall_spell_id.to_string()
        } else {
            self.recall.display_name().to_owned()
        }
    }

    /// Fills in a recall waypoint's spell from the name it was written with:
    /// the recall table first, then whatever the spell catalogue offers. It
    /// is called again when the waypoint runs, because a route can be read
    /// before the character's spell tables are available.
    pub fn resolve_recall_spell(&mut self, spells: Option<&dyn SpellLookup>) {
        if self.recall_spell_id != 0 {
            return;
        }
        let name = self.recall_spell_name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(kind) = RouteRecallKind::from_name(name) {
            self.recall = kind;
            // Marketplace has no spell: it stays at id 0 and is submitted as
            // a command.
            self.recall_spell_id = kind.spell_id();
            return;
        }
        self.recall_spell_id = resolve_catalog_spell_id_by_name(spells, name);
    }
}

/// The catalogue fallback: the whole content table by exact name, then the
/// known-spell lists for a host that does not carry the full table.
fn resolve_catalog_spell_id_by_name(spells: Option<&dyn SpellLookup>, name: &str) -> u32 {
    let Some(spells) = spells else {
        return 0;
    };
    if let Some(found) = spells.find_by_name(name).filter(|id| *id != 0) {
        return found;
    }
    spells
        .known_spells()
        .into_iter()
        .find(|(_, spell)| ordinal::equals_ignore_case(spell, name))
        .map_or(0, |(id, _)| id)
}

/// A position as the route list shows it: `(12.3N, 45.6E)`.
#[must_use]
pub fn format_position(value: &Position) -> String {
    let north_south = if value.north_south >= 0.0 { "N" } else { "S" };
    let east_west = if value.east_west >= 0.0 { "E" } else { "W" };
    format!(
        "({}{north_south}, {}{east_west})",
        format_f64(value.north_south.abs(), "0.###").expect("a supported format"),
        format_f64(value.east_west.abs(), "0.###").expect("a supported format"),
    )
}

/// The navigation settings: the route and how it is driven.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationSettings {
    /// Draw the loaded route in the world ("Show Nav Lines").
    pub show_nav_lines: bool,
    /// Navigation is on.
    pub enabled: bool,
    /// Navigation goes before looting.
    pub priority: bool,
    /// How the route runs.
    pub mode: RouteMode,
    /// The arrival distance.
    pub minimum_distance_meters: f64,
    /// The outer validity bound for a navigation rule, not a second arrival
    /// threshold.
    pub maximum_distance_meters: f64,
    /// How near a portal must be to be used.
    pub portal_use_distance_meters: f64,
    /// A follow route's target.
    pub follow_target_object_id: u32,
    /// A follow route's target's name.
    pub follow_target_name: String,
    /// Follow around corners.
    pub follow_around_corners: bool,
    /// When legs go to the client's pathing.
    pub client_pathing: ClientPathing,
    /// Open doors on the way.
    pub open_doors: bool,
    /// How near a door must be to be identified.
    pub door_identify_range_meters: f64,
    /// How near a door must be to be opened.
    pub door_open_range_meters: f64,
    /// The lockpick skill margin a locked door needs.
    pub door_lockpick_excess_threshold: i32,
    /// How many times a jump asked for by command charges before it gives
    /// up. A charge the client never turns into a jump is silent, so
    /// without a ceiling one refusal would hold the character for ever.
    pub jump_attempts: i32,
    /// The waypoints.
    pub waypoints: Vec<RouteWaypoint>,
}

impl Default for NavigationSettings {
    fn default() -> Self {
        Self {
            show_nav_lines: false,
            enabled: false,
            priority: false,
            mode: RouteMode::Circular,
            minimum_distance_meters: 2.0,
            maximum_distance_meters: 999_999.0 * 240.0,
            portal_use_distance_meters: 4.0,
            follow_target_object_id: 0,
            follow_target_name: String::new(),
            follow_around_corners: true,
            client_pathing: ClientPathing::WhenStuck,
            open_doors: false,
            door_identify_range_meters: 20.0,
            door_open_range_meters: 4.0,
            door_lockpick_excess_threshold: -50,
            jump_attempts: 3,
            waypoints: Vec::new(),
        }
    }
}

impl NavigationSettings {
    /// A route of `mode` with no waypoints, everything else default.
    #[must_use]
    pub fn with_mode(mode: RouteMode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: EveryRecallDisplayNameResolvesToItsOwnKind (the table half; the .af half is in af.rs)
    #[test]
    fn every_recall_display_name_resolves_to_its_own_kind() {
        for kind in RouteRecallKind::ALL {
            assert_eq!(RouteRecallKind::from_name(kind.display_name()), Some(kind));
            assert_eq!(RouteRecallKind::from_name(kind.name()), Some(kind));
        }
    }

    #[test]
    fn a_recall_named_in_any_case_resolves_its_spell() {
        let mut waypoint = RouteWaypoint {
            kind: RouteWaypointType::Recall,
            recall_spell_name: "lifestone recall".to_owned(),
            ..RouteWaypoint::default()
        };
        waypoint.resolve_recall_spell(None);
        assert_eq!(waypoint.recall, RouteRecallKind::LifestoneRecall);
        assert_eq!(waypoint.recall_spell_id, 1635);
    }

    #[test]
    fn display_text_formats_as_the_route_list_does() {
        let mut jump = RouteWaypoint::at(RouteWaypointType::Jump, 1.0, 2.0, 0.0);
        jump.jump_heading_degrees = 12.34;
        jump.jump_hold_shift = true;
        jump.jump_charge_milliseconds = 300;
        jump.jump_direction = RouteJumpDirection::StrafeLeft;
        assert_eq!(
            jump.display_text(),
            "Jump: 12.3d, 300ms, Shift, Strafe Left"
        );
        let point = RouteWaypoint::at(RouteWaypointType::Point, -45.6789, 12.3456, 0.0);
        assert_eq!(point.display_text(), "Point: (12.346N, 45.679W)");
        let pause = RouteWaypoint {
            kind: RouteWaypointType::Pause,
            duration_milliseconds: 2500,
            ..RouteWaypoint::default()
        };
        assert_eq!(pause.display_text(), "Pause: 2.5 seconds");
    }
}
