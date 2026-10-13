//! The points of interest on the countryside map (`LandscapeMarkerCatalog`),
//! read from files the player drops into the UB storage tree. The plugin
//! ships no data of its own: portal, lifestone, vendor and town lists are
//! community work that changes with the server, so an empty file with the
//! format written in it is placed where the player will find it.
//!
//! Every `.csv` under `mosstank/ub/maps/` is read. A line is one marker,
//! four fields separated by commas: `kind, name, north-south, east-west`.
//! The kind is Town, Outpost, Portal, Dungeon, Lifestone, Npc or Vendor, in
//! any case; the coordinates are map units as the in-game readout shows
//! them, north and east positive. A name may hold a comma inside double
//! quotes. Blank lines, `#` lines and lines that do not parse are skipped,
//! so one bad line never loses a file.

use mosstank_stores::storage::{PluginStorage, StorageError};
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::text::ordinal;

use super::view::MAX_COORDINATE;

/// What a point of interest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LandscapeMarkerKind {
    /// No kind; never a marker.
    Unknown,
    /// A town.
    Town,
    /// A lifestone.
    Lifestone,
    /// A vendor.
    Vendor,
    /// A non-player character.
    Npc,
    /// A portal.
    Portal,
    /// An outpost.
    Outpost,
    /// A dungeon.
    Dungeon,
}

/// The kinds by their names and numbers, as the enum declares them.
const KINDS: [(&str, LandscapeMarkerKind); 8] = [
    ("Unknown", LandscapeMarkerKind::Unknown),
    ("Town", LandscapeMarkerKind::Town),
    ("Lifestone", LandscapeMarkerKind::Lifestone),
    ("Vendor", LandscapeMarkerKind::Vendor),
    ("Npc", LandscapeMarkerKind::Npc),
    ("Portal", LandscapeMarkerKind::Portal),
    ("Outpost", LandscapeMarkerKind::Outpost),
    ("Dungeon", LandscapeMarkerKind::Dungeon),
];

/// How one kind of marker shows (`LandscapeMarkerDisplay`): the zoom its
/// icon draws from, the zoom its name draws from, and which piece of the
/// client's art is its icon. Both bands end at the closest zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandscapeMarkerDisplay {
    /// The widest zoom the icon draws at.
    pub min_marker_zoom: f64,
    /// The widest zoom the name draws at.
    pub min_label_zoom: f64,
    /// The client art the icon is.
    pub icon_surface_id: u32,
}

impl LandscapeMarkerDisplay {
    /// The closest zoom the icon draws at.
    pub const MAX_MARKER_ZOOM: f64 = 1.0;
    /// The closest zoom the name draws at.
    pub const MAX_LABEL_ZOOM: f64 = 1.0;

    /// Whether the icon draws at this zoom.
    #[must_use]
    pub fn shows_marker_at(&self, zoom: f64) -> bool {
        zoom >= self.min_marker_zoom && zoom <= Self::MAX_MARKER_ZOOM
    }

    /// Whether the name draws at this zoom.
    #[must_use]
    pub fn shows_label_at(&self, zoom: f64) -> bool {
        zoom >= self.min_label_zoom && zoom <= Self::MAX_LABEL_ZOOM
    }
}

/// One point of interest: what it is, what it is called, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct LandscapeMarker {
    /// Its kind.
    pub kind: LandscapeMarkerKind,
    /// Its name.
    pub name: String,
    /// Map units north.
    pub north_south: f64,
    /// Map units east.
    pub east_west: f64,
}

impl LandscapeMarker {
    /// A marker.
    #[must_use]
    pub fn new(kind: LandscapeMarkerKind, name: &str, north_south: f64, east_west: f64) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            north_south,
            east_west,
        }
    }
}

/// The folder under the UB tree (`mosstank_stores::ub_store::ROOT`) the
/// marker files live in.
pub const FOLDER: &str = "mosstank/ub/maps/";

/// The file written, empty but for the format, when the folder has none.
pub const SHIPPED_FILE: &str = "mosstank/ub/maps/markers.csv";

/// What the shipped file says, so the format travels with the installation.
pub const SHIPPED_FILE_TEXT: &str = "# MossTank countryside map markers.\n\
# One marker a line, four fields: kind, name, north-south, east-west\n\
#   kind: Town, Outpost, Portal, Dungeon, Lifestone, Npc or Vendor\n\
#   coordinates: map units as the in-game readout shows them, north and east positive\n\
#   a name holding a comma goes in double quotes\n\
# Every .csv file in this folder is read. Lines starting with # are skipped.\n\
# Example:\n\
#   Town, Holtburg, 42.1, 33.6\n\
#   Portal, \"Holtburg, Town Portal\", 42.5, 33.9\n";

/// How a kind of marker shows.
#[must_use]
pub fn display_of(kind: LandscapeMarkerKind) -> LandscapeMarkerDisplay {
    let (min_marker_zoom, min_label_zoom, icon_surface_id) = match kind {
        LandscapeMarkerKind::Unknown => (0.0, 0.0, 0x0600_3C70),
        LandscapeMarkerKind::Town => (0.0, 0.0, 0x0600_3C7E),
        LandscapeMarkerKind::Outpost => (0.1, 0.25, 0x0600_4D70),
        LandscapeMarkerKind::Portal => (0.1, 0.32, 0x0600_3C6B),
        LandscapeMarkerKind::Dungeon => (0.1, 0.32, 0x0600_5B57),
        LandscapeMarkerKind::Lifestone => (0.24, 0.4, 0x0600_24E1),
        LandscapeMarkerKind::Npc => (0.4, 0.8, 0x0600_3C36),
        LandscapeMarkerKind::Vendor => (0.4, 0.8, 0x0600_4E1F),
    };
    LandscapeMarkerDisplay {
        min_marker_zoom,
        min_label_zoom,
        icon_surface_id,
    }
}

/// The markers as last loaded.
#[derive(Debug, Clone, Default)]
pub struct LandscapeMarkerCatalog {
    markers: Vec<LandscapeMarker>,
    revision: i32,
}

impl LandscapeMarkerCatalog {
    /// None loaded yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The markers as last loaded, in file order.
    #[must_use]
    pub fn markers(&self) -> &[LandscapeMarker] {
        &self.markers
    }

    /// Rises by one on every load that changed the set.
    #[must_use]
    pub fn revision(&self) -> i32 {
        self.revision
    }

    /// Reads every marker file under the folder, writing the shipped file
    /// first when the folder has none, so the format is on disk to be
    /// followed; whether the set changed.
    ///
    /// # Errors
    /// The storage failed to list, read or write.
    pub fn load(&mut self, storage: &mut dyn PluginStorage) -> Result<bool, StorageError> {
        let mut loaded = Vec::new();
        if storage.is_available() {
            let mut files: Vec<String> = storage
                .list(FOLDER)?
                .into_iter()
                .filter(|key| ordinal::ends_with_ignore_case(key, ".csv"))
                .collect();
            files.sort();
            if files.is_empty() {
                // Never over a file that is already there: an empty listing
                // is not proof the folder is, and the markers a person put
                // in are not ours to lose.
                if storage.read_text(SHIPPED_FILE)?.is_none() {
                    storage.write_text(SHIPPED_FILE, SHIPPED_FILE_TEXT)?;
                }
                files.push(SHIPPED_FILE.to_owned());
            }
            for file in files {
                loaded.extend(parse(&storage.read_text(&file)?.unwrap_or_default()));
            }
        }
        if loaded == self.markers {
            return Ok(false);
        }
        self.markers = loaded;
        self.revision += 1;
        Ok(true)
    }
}

/// The markers in one file's text; lines that do not parse are skipped.
#[must_use]
pub fn parse(text: &str) -> Vec<LandscapeMarker> {
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(try_parse_line)
        .collect()
}

/// One line of the file, or `None` when it is not a marker.
#[must_use]
pub fn try_parse_line(line: &str) -> Option<LandscapeMarker> {
    let fields = split_fields(line);
    if fields.len() != 4 {
        return None;
    }
    let kind = parse_kind(fields[0].trim())?;
    if kind == LandscapeMarkerKind::Unknown {
        return None;
    }
    let name = fields[1].trim();
    if name.is_empty() {
        return None;
    }
    let north_south = number::parse_f64(fields[2].trim(), NumberStyles::FLOAT)?;
    let east_west = number::parse_f64(fields[3].trim(), NumberStyles::FLOAT)?;
    if north_south.abs() > MAX_COORDINATE || east_west.abs() > MAX_COORDINATE {
        return None;
    }
    Some(LandscapeMarker::new(kind, name, north_south, east_west))
}

/// The kind a field names: its name in any case, or its number
/// (`Enum.TryParse` with `ignoreCase`).
fn parse_kind(text: &str) -> Option<LandscapeMarkerKind> {
    if let Some((_, kind)) = KINDS
        .iter()
        .find(|(name, _)| ordinal::equals_ignore_case(name, text))
    {
        return Some(*kind);
    }
    let number = number::parse_i32(text, NumberStyles::INTEGER)?;
    usize::try_from(number)
        .ok()
        .and_then(|index| KINDS.get(index))
        .map(|(_, kind)| *kind)
}

fn split_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    for c in line.chars() {
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if c == ',' && !quoted {
            fields.push(std::mem::take(&mut field));
            continue;
        }
        field.push(c);
    }
    fields.push(field);
    fields
}
