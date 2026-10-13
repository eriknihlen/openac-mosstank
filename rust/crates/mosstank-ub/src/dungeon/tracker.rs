//! The objects on the map for one landblock (`DungeonObjectTracker`), kept
//! up to date from the world-object capture and the player's own snapshot,
//! and saying whether anything about their placement changed so the map is
//! repainted only then. The player is placed from the navigation snapshot,
//! which is the client's own position, rather than from the capture.

use std::collections::HashSet;

use ac_plugin_abi::world::{Floorplan, ObjectClass, Position, WorldObject};
use mosstank_dotnet::collections::{DotNetDictionary, Ordinal};

use super::markers::{self, DungeonMarkerKind};
use super::shade::DungeonLayerShade;
use super::{Vec3, to_landblock_local};

/// How far a mover must go before it is placed again. The reference's
/// threshold is twenty-five of the units its distance helper returns, which
/// are metres times two hundred and forty: just over a tenth of a metre.
pub const UPDATE_DISTANCE_METERS: f64 = 25.0 / 240.0;

/// One thing placed on the map: where it is, which storey that puts it on,
/// and how it is to be marked.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonTrackedObject {
    /// The object's id.
    pub id: u32,
    /// The label, already trimmed for the map.
    pub name: String,
    /// Which settings rows draw it.
    pub kind: DungeonMarkerKind,
    /// The object's own class.
    pub object_class: ObjectClass,
    /// The icon drawn when the rows ask for one.
    pub icon_id: u32,
    /// Whether it is placed again as it moves.
    pub is_mover: bool,
    /// Where it was last placed, in landblock-local metres.
    pub position: Vec3,
    /// The storey band its height falls in.
    pub layer_z: f32,
    /// Whether it has been placed at all yet.
    pub is_placed: bool,
}

impl DungeonTrackedObject {
    fn new(
        id: u32,
        name: String,
        kind: DungeonMarkerKind,
        object_class: ObjectClass,
        icon_id: u32,
        is_mover: bool,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            object_class,
            icon_id,
            is_mover,
            position: Vec3::default(),
            layer_z: 0.0,
            is_placed: false,
        }
    }

    /// Places the object, or moves it if it has gone further than
    /// `threshold` metres from where it was; whether anything changed. A
    /// thing that does not move keeps its first place whatever a later
    /// capture says.
    fn place(&mut self, position: Vec3, threshold: f64) -> bool {
        if self.is_placed
            && (!self.is_mover || f64::from(self.position.distance(position)) <= threshold)
        {
            return false;
        }
        self.position = position;
        self.layer_z = DungeonLayerShade::band_of(position.z);
        self.is_placed = true;
        true
    }
}

/// The objects on the map for one landblock.
#[derive(Debug, Clone, Default)]
pub struct DungeonObjectTracker {
    objects: DotNetDictionary<u32, DungeonTrackedObject, Ordinal<u32>>,
    seen: HashSet<u32>,
    landblock_id: u32,
}

impl DungeonObjectTracker {
    /// An empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything placed on the map, in no particular order.
    pub fn objects(&self) -> impl Iterator<Item = &DungeonTrackedObject> {
        self.objects.values()
    }

    /// How many objects are on the map.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether nothing is on the map.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Forgets everything, as when the map moves to another landblock.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.landblock_id = 0;
    }

    /// Brings the placements up to date with a capture; whether any
    /// placement changed. Objects carried, wielded, without a position or in
    /// another landblock are not on this map.
    pub fn update(
        &mut self,
        capture: &[WorldObject],
        plan: &Floorplan,
        own: &Position,
        self_id: u32,
        self_name: &str,
    ) -> bool {
        let mut changed = false;
        if self.landblock_id != plan.landblock_id {
            changed = !self.objects.is_empty();
            self.objects.clear();
            self.landblock_id = plan.landblock_id;
        }
        self.seen.clear();

        if self_id != 0 && (own.cell_id & 0xFFFF_0000) == plan.landblock_id {
            self.seen.insert(self_id);
            if !self.objects.contains_key(&self_id) {
                self.objects.insert(
                    self_id,
                    DungeonTrackedObject::new(
                        self_id,
                        self_name.to_owned(),
                        DungeonMarkerKind::You,
                        ObjectClass::Player,
                        0,
                        true,
                    ),
                );
                changed = true;
            }
            if let Some(you) = self.objects.get_mut(&self_id) {
                changed |= you.place(to_landblock_local(own), UPDATE_DISTANCE_METERS);
            }
        }

        for subject in capture {
            let Some(position) = &subject.position else {
                continue;
            };
            if subject.object_id == self_id
                || subject.wielder_id != 0
                || subject.container_id != 0
                || (position.cell_id & 0xFFFF_0000) != plan.landblock_id
            {
                continue;
            }
            self.seen.insert(subject.object_id);
            if !self.objects.contains_key(&subject.object_id) {
                self.objects.insert(
                    subject.object_id,
                    DungeonTrackedObject::new(
                        subject.object_id,
                        markers::label_of(subject),
                        markers::classify(subject, self_id, self_name),
                        subject.object_class,
                        subject.icon_id,
                        markers::is_mover(subject.object_class),
                    ),
                );
                changed = true;
            }
            if let Some(tracked) = self.objects.get_mut(&subject.object_id) {
                changed |= tracked.place(to_landblock_local(position), UPDATE_DISTANCE_METERS);
            }
        }

        if self.objects.len() != self.seen.len() {
            let seen = &self.seen;
            let before = self.objects.len();
            self.objects.retain(|id, _| seen.contains(id));
            changed |= self.objects.len() != before;
        }
        changed
    }
}
