//! A character's Comps HUD as it is kept in the plugin's storage: where the
//! strip stands, its layout, and the items it counts, in order, each with
//! its icon and warn level.
//!
//! `{ "X", "Y", "Layout", "Entries": [{ "Name", "IconId", "Warn" }] }`.
//! The first form had neither `Layout` nor `Warn`: a list saved then reads
//! as a column whose items never warn, and the next change writes it in
//! this form.
//!
//! A file that is only partly readable gives what it can (an entry without
//! a readable name is left out, any other value that does not read is its
//! default) and names each problem, so the caller can keep the original
//! before the list is written again.

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

/// A kept HUD as far as it read, and what did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Read {
    pub(super) document: Document,
    /// Each part that did not read, in plain words; empty for a whole file.
    pub(super) problems: Vec<String>,
}

/// Reads a kept HUD.
///
/// # Errors
/// The text is not JSON, or not an object (a JSON `null` included).
pub(super) fn read(text: &str) -> Result<Read, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "Document")? else {
        return Err("the file holds no list (null)".to_owned());
    };
    let o = o.case_sensitive();
    let mut problems = Vec::new();
    let mut or_default = |value: Result<i32, JsonError>, default: i32| {
        value.unwrap_or_else(|error| {
            problems.push(error);
            default
        })
    };
    let x = or_default(o.int("X", DEFAULT_X), DEFAULT_X);
    let y = or_default(o.int("Y", DEFAULT_Y), DEFAULT_Y);
    let layout = match o.string("Layout", "") {
        Ok(Some(name)) if name.is_empty() => HudLayout::default(),
        Ok(Some(name)) => HudLayout::from_saved_name(&name).unwrap_or_else(|| {
            problems.push(format!("$.Layout: '{name}' is not a layout"));
            HudLayout::default()
        }),
        Ok(None) => HudLayout::default(),
        Err(error) => {
            problems.push(error);
            HudLayout::default()
        }
    };
    let mut entries = Vec::new();
    match o.get("Entries") {
        None | Some(JsonValue::Null) => {}
        Some(JsonValue::Array(rows)) => {
            for (index, row) in rows.iter().enumerate() {
                let path = format!("$.Entries[{index}]");
                match Object::of(row, &path, "ComponentHudEntry") {
                    Ok(Some(row)) => {
                        if let Some(entry) = read_entry(&row.case_sensitive(), &mut problems) {
                            entries.push(entry);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => problems.push(error),
                }
            }
        }
        Some(_) => problems.push("$.Entries: not a list".to_owned()),
    }
    Ok(Read {
        document: Document {
            x,
            y,
            layout,
            entries,
        },
        problems,
    })
}

/// One entry: `None` (and a problem) when its name does not read; an icon
/// or warn level that does not read is none.
fn read_entry(row: &Object<'_>, problems: &mut Vec<String>) -> Option<ComponentHudEntry> {
    let name = match row.string("Name", "") {
        Ok(name) => name.unwrap_or_default(),
        Err(error) => {
            problems.push(error);
            return None;
        }
    };
    let icon_id = row.uint("IconId", 0).unwrap_or_else(|error| {
        problems.push(error);
        0
    });
    let warn = row.int("Warn", 0).unwrap_or_else(|error| {
        problems.push(error);
        0
    });
    Some(ComponentHudEntry {
        name,
        icon_id,
        warn: warn.max(0),
    })
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
