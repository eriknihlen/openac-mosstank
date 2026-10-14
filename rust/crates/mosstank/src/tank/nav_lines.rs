//! "Show Nav Lines" (`MossTankPanel.NavLines`): the loaded route drawn in
//! the world through one of the client's line layers, rebuilt only when
//! something the drawing depends on changes.

use std::hash::{DefaultHasher, Hash, Hasher};

use ac_plugin_abi::world::Position;
use mosstank_formats::route::NavigationSettings;
use mosstank_nav::route_view::route_lines;
use openac_plugin::ui::LineLayer;

use super::TankCx;
use crate::client::TankClient;

/// The route's line layer and what its lines were drawn from.
#[derive(Debug, Default)]
pub(crate) struct NavLines {
    /// Asked of the client the first time the lines are shown.
    layer: Option<LineLayer>,
    /// What the lines on the layer were drawn from; none while nothing is
    /// drawn.
    signature: Option<u64>,
}

impl NavLines {
    /// Takes the lines away (`ClearNavLines`); the layer is kept.
    pub(crate) fn clear(&mut self, client: &mut dyn TankClient) {
        if let Some(layer) = self.layer {
            client.set_world_lines(layer, &[]);
        }
        self.signature = None;
    }
}

/// What the drawing depends on: the route's mode, its arrival radius,
/// whether the player is outdoors, the point the route is heading for, and
/// every point's kind and position.
fn signature(navigation: &NavigationSettings, is_outdoor: bool, current_waypoint: i32) -> u64 {
    fn position(hasher: &mut DefaultHasher, p: &Position) {
        p.cell_id.hash(hasher);
        p.east_west.to_bits().hash(hasher);
        p.north_south.to_bits().hash(hasher);
        p.elevation.to_bits().hash(hasher);
        p.heading_degrees.to_bits().hash(hasher);
        p.is_outdoor.hash(hasher);
    }
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(&navigation.mode).hash(&mut hasher);
    navigation
        .minimum_distance_meters
        .to_bits()
        .hash(&mut hasher);
    is_outdoor.hash(&mut hasher);
    current_waypoint.hash(&mut hasher);
    for point in &navigation.waypoints {
        std::mem::discriminant(&point.kind).hash(&mut hasher);
        position(&mut hasher, &point.position);
    }
    hasher.finish()
}

impl TankCx<'_> {
    /// `UpdateNavLines`: draws the loaded route while "Show Nav Lines" is
    /// on and the player is in the world, and takes it away otherwise.
    pub(crate) fn update_nav_lines(&mut self) {
        let snapshot = self.client.rules().navigation_snapshot();
        let core = &mut *self.core;
        let navigation = &core.settings.navigation;
        if !navigation.show_nav_lines || !snapshot.is_available || snapshot.is_portal_space {
            if core.nav_lines.signature.is_some() {
                core.nav_lines.clear(self.client);
            }
            return;
        }
        let layer = match core.nav_lines.layer {
            Some(layer) => layer,
            None => *core.nav_lines.layer.insert(self.client.create_line_layer()),
        };
        let is_outdoor = snapshot.position.is_outdoor;
        let current = core.navigation.current_waypoint_index();
        let signature = signature(navigation, is_outdoor, current);
        if core.nav_lines.signature == Some(signature) {
            return;
        }
        core.nav_lines.signature = Some(signature);
        let lines = route_lines(navigation, current, is_outdoor);
        self.client.set_world_lines(layer, &lines);
    }
}
