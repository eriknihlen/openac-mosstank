//! `/ub propertydump`: everything the client knows about the selected
//! object, as one message in the reference's words.
//!
//! The message is the object's identity and description values, one
//! `Name = value` line each, then its properties by kind (strings,
//! integers, booleans, floats), each kind under its own heading and only
//! when it has any, then the spells on it. A property is shown as its name,
//! its number in brackets and its value; within a kind the properties sort
//! by name in the culture-sensitive order, and a number with no name is its
//! own name and sorts as that text.
//!
//! The names are data ([`property_names.tsv`](property_names.tsv): kind,
//! number, name), taken mechanically from the reference's property tables;
//! they are not edited by hand.

use std::cmp::Ordering;

use ac_plugin_abi::world::WorldObject;
use openac_plugin_compat::text::collation;

use super::{UbCall, formatted, invariant, selected_object, spell_name};

const PROPERTY_NAMES: &str = include_str!("property_names.tsv");

/// The integers an object carries itself are read under keys from here up,
/// as loot rules and meta expressions read them.
const DESCRIPTION_KEY_BASE: u32 = 0x0D00_0000;
/// Every icon id carries this; the icon key reads the id without it.
const ICON_ID_PREFIX: u32 = 0x0600_0000;
/// The string key of an object's name.
const NAME_STRING_KEY: u32 = 1;

/// A kind of property, as its row in the name table names it.
#[derive(Debug, Clone, Copy)]
enum Kind {
    String,
    Int,
    Bool,
    Float,
}

impl Kind {
    fn table(self) -> &'static str {
        match self {
            Kind::String => "string",
            Kind::Int => "int",
            Kind::Bool => "bool",
            Kind::Float => "float",
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Kind::String => "String Values:",
            Kind::Int => "Int Values:",
            Kind::Bool => "Bool Values:",
            Kind::Float => "Float Values:",
        }
    }
}

/// The name of property `key` of a kind; its number when the table has no
/// name for it.
fn property_name(kind: Kind, key: u32) -> String {
    let table = kind.table();
    PROPERTY_NAMES
        .lines()
        .filter_map(|row| {
            let mut fields = row.split('\t');
            Some((fields.next()?, fields.next()?, fields.next()?))
        })
        .find(|(row_kind, number, _)| *row_kind == table && number.parse() == Ok(key))
        .map_or_else(|| key.to_string(), |(_, _, name)| name.to_owned())
}

/// `/ub propertydump`.
pub(super) fn dump_selected_properties(call: &mut UbCall<'_>) {
    let Some(item) = selected_object(call, "propertydump") else {
        return;
    };
    let properties = call.host.properties(item.object_id).unwrap_or_default();
    let mut lines = description_lines(&item);

    let mut strings = properties.strings.clone();
    if !strings.iter().any(|(key, _)| *key == NAME_STRING_KEY) {
        strings.push((NAME_STRING_KEY, item.name.clone()));
    }
    section(&mut lines, Kind::String, &strings, Clone::clone);

    let mut ints = properties.ints.clone();
    for (key, value) in description_ints(&item) {
        if !ints.iter().any(|(known, _)| *known == key) {
            ints.push((key, value));
        }
    }
    section(&mut lines, Kind::Int, &ints, i32::to_string);

    section(&mut lines, Kind::Bool, &properties.bools, |value| {
        if *value { "True" } else { "False" }.to_owned()
    });
    section(&mut lines, Kind::Float, &properties.floats, |value| {
        invariant(*value)
    });

    if !item.spell_ids.is_empty() {
        lines.push(format!("Spells ({}):", item.spell_ids.len()));
        for &spell_id in &item.spell_ids {
            let name = spell_name(call, spell_id);
            lines.push(format!("  {name} ({})", spell_id.cast_signed()));
        }
    }

    call.write_ub_message(&format!("Property Dump for {}", item.name), lines);
}

/// The lines before the properties: who the object is and what it is.
fn description_lines(item: &WorldObject) -> Vec<String> {
    let id = item.object_id;
    let class_id = item.weenie_class_id;
    let flags = item
        .header
        .as_ref()
        .and_then(|header| header.weenie_header_flags)
        .unwrap_or(0);
    vec![
        format!("Id = {} (0x{id:08X})", id.cast_signed()),
        format!("Type = {} (0x{class_id:08X})", class_id.cast_signed()),
        format!("Name = {}", item.name),
        format!("ActiveSpellCount = {}", item.active_spell_ids.len()),
        format!("Category = {}", item.item_type.cast_signed()),
        format!("Coordinates = {}", coordinates(item)),
        format!("GameDataFlags1 = {}", flags.cast_signed()),
        format!(
            "HasIdData = {}",
            if item.has_appraisal_data {
                "True"
            } else {
                "False"
            }
        ),
        format!("LastIdTime = {}", item.last_id_time),
        format!(
            "ObjectClass = {:?} ({})",
            item.object_class,
            item.object_class.tag()
        ),
    ]
}

/// Where the object is on the map, one decimal each: `1.2S, 3.5E`. An
/// object with no place in the world has no coordinates, and the text is
/// empty.
fn coordinates(item: &WorldObject) -> String {
    let Some(position) = &item.position else {
        return String::new();
    };
    let ns = if position.north_south >= 0.0 {
        "N"
    } else {
        "S"
    };
    let ew = if position.east_west >= 0.0 { "E" } else { "W" };
    format!(
        "{}{ns}, {}{ew}",
        formatted(position.north_south.abs(), "0.0"),
        formatted(position.east_west.abs(), "0.0")
    )
}

/// The integers the object carries itself, under the keys loot rules and
/// meta expressions read them by, each only when the object has a value:
/// zero stands for "the server sent none". The class id is always there.
fn description_ints(item: &WorldObject) -> Vec<(u32, i32)> {
    let key = |offset: u32| DESCRIPTION_KEY_BASE + offset;
    let icon = if item.icon_id >= ICON_ID_PREFIX {
        item.icon_id - ICON_ID_PREFIX
    } else {
        item.icon_id
    };
    let holder = if item.container_id != 0 {
        item.container_id
    } else {
        item.wielder_id
    };
    let behavior = item
        .header
        .as_ref()
        .and_then(|header| header.object_description_flags)
        .unwrap_or(0);
    let landcell = item
        .position
        .as_ref()
        .map(|position| position.cell_id.cast_signed());
    let present = |value: i32| (value != 0).then_some(value);
    [
        (0, Some(item.weenie_class_id.cast_signed())),
        (1, present(icon.cast_signed())),
        (2, present(holder.cast_signed())),
        (3, landcell),
        (4, present(item.items_capacity)),
        (5, present(item.containers_capacity)),
        // Only ever -1, the mark of an item someone wields.
        (9, (item.wielder_id != 0).then_some(-1)),
        (13, present(item.coverage_mask.cast_signed())),
        (26, present(item.item_type.cast_signed())),
        (27, present(behavior.cast_signed())),
    ]
    .into_iter()
    .filter_map(|(offset, value)| Some((key(offset), value?)))
    .collect()
}

/// A kind's heading and one line a property, by name, when there are any.
fn section<T>(
    lines: &mut Vec<String>,
    kind: Kind,
    values: &[(u32, T)],
    text: impl Fn(&T) -> String,
) {
    if values.is_empty() {
        return;
    }
    let mut named: Vec<(String, u32, &T)> = values
        .iter()
        .map(|(key, value)| (property_name(kind, *key), *key, value))
        .collect();
    named.sort_by(|(a, ..), (b, ..)| by_name(a, b));
    lines.push(kind.heading().to_owned());
    for (name, key, value) in named {
        lines.push(format!("  {name}({}) = {}", key.cast_signed(), text(value)));
    }
}

/// The culture-sensitive order of two names.
fn by_name(a: &str, b: &str) -> Ordering {
    collation::compare(a, b)
}
