//! The countryside map: the client's own map art on a canvas, with the
//! character on it, the points of interest the player listed, coordinate
//! rulers and the coordinate at the middle.
//!
//! - [`view`]: the view arithmetic (`LandscapeMapView`).
//! - [`markers`]: the marker files and how each kind shows
//!   (`LandscapeMarkerCatalog`).
//! - [`placer`]: where each marker's name goes (`MapLabelPlacer`).
//! - [`controller`]: the map on a canvas (`LandscapeMapController`).

pub mod controller;
pub mod markers;
pub mod placer;
pub mod view;
