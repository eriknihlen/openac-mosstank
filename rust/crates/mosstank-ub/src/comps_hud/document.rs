//! A character's Comps HUD as it is kept in the plugin's storage: where the
//! strip stands, its layout, and the items it counts, in order, each with
//! its icon and warn level.
//!
//! `{ "X", "Y", "Layout", "Entries": [{ "Name", "IconId", "Warn" }] }`.
//! The first form had neither `Layout` nor `Warn`: a list saved then reads
//! as a column whose items never warn, and the next change writes it in
//! this form.

use mosstank_stores::stj::{self, JsonError, Object};
use openac_plugin_compat::json::{self, JsonValue};

use super::layout::HudLayout;
use super::{ComponentHudEntry, DEFAULT_X, DEFAULT_Y};

/// A character's HUD as it is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Document {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) layout: HudLayout,
    pub(super) entries: Vec<ComponentHudEntry>,
}

/// Reads a kept HUD; `None` for a JSON `null`. A missing entry list is
/// none, a `null` entry is skipped, a missing name is no name, a missing or
/// unknown layout is the column, and a missing or negative warn level is
/// none.
pub(super) fn read(text: &str) -> Result<Option<Document>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "Document")? else {
        return Ok(None);
    };
    let o = o.case_sensitive();
    let rows = o
        .objects("Entries", None, "ComponentHudEntry")?
        .unwrap_or_default();
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows.into_iter().flatten() {
        let row = row.case_sensitive();
        entries.push(ComponentHudEntry {
            name: row.string("Name", "")?.unwrap_or_default(),
            icon_id: row.uint("IconId", 0)?,
            warn: row.int("Warn", 0)?.max(0),
        });
    }
    let layout = o
        .string("Layout", "")?
        .and_then(|name| HudLayout::from_saved_name(&name))
        .unwrap_or_default();
    Ok(Some(Document {
        x: o.int("X", DEFAULT_X)?,
        y: o.int("Y", DEFAULT_Y)?,
        layout,
        entries,
    }))
}

/// Writes a HUD in the current form.
pub(super) fn write(document: &Document) -> String {
    stj::write(&JsonValue::object([
        ("X", stj::int(document.x)),
        ("Y", stj::int(document.y)),
        ("Layout", stj::string(document.layout.saved_name())),
        (
            "Entries",
            JsonValue::array(document.entries.iter().map(|entry| {
                JsonValue::object([
                    ("Name", stj::string(&entry.name)),
                    ("IconId", stj::uint(entry.icon_id)),
                    ("Warn", stj::int(entry.warn)),
                ])
            })),
        ),
    ]))
}
