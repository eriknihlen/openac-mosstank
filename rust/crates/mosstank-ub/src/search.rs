//! Finding the one object a typed name stands for (`UbObjectSearch`), in the
//! order the reference searches: a number or "selected" first, then the
//! packs, the open container and finally the nearest matching thing on the
//! ground.

use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_dotnet::try_parse_uint;
use mosstank_meta::host::MetaHost;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::text::ordinal;

/// Where a command that names an object is allowed to look. The command
/// flags spell the same three: no flag searches everything, `i` the packs,
/// `l` the landscape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbSearchScope {
    /// Packs first, then the open container, then the landscape.
    All,
    /// Only what the character is carrying.
    Inventory,
    /// Only what is standing in the world.
    Landscape,
}

/// The object a name stands for, or `None` when nothing answers to it.
///
/// `name` is the whole name, part of one when `partial`, a decimal or
/// hexadecimal object id, or the word "selected". `exclude_object_id` is an
/// object the search must skip: the item already picked as the first half
/// of "use A on B", so that "use taper on taper" is two tapers.
pub fn try_find(
    host: &mut dyn MetaHost,
    name: &str,
    scope: UbSearchScope,
    partial: bool,
    exclude_object_id: u32,
) -> Option<WorldObject> {
    let text = name.trim();
    if text.is_empty() {
        return None;
    }
    let objects = host.objects();
    if let Some(found) = find_by_special_name(host, &objects, text) {
        return Some(found);
    }
    let open_container = host.open_container();
    match scope {
        UbSearchScope::Inventory => first(
            objects.iter().filter(|o| o.is_owned),
            text,
            partial,
            exclude_object_id,
        ),
        UbSearchScope::Landscape => {
            let player = host.navigation();
            nearest(
                &player,
                objects.iter().filter(|o| o.is_landscape),
                text,
                partial,
                exclude_object_id,
            )
        }
        UbSearchScope::All => first(
            objects.iter().filter(|o| o.is_owned),
            text,
            partial,
            exclude_object_id,
        )
        .or_else(|| {
            if open_container == 0 {
                None
            } else {
                first(
                    objects.iter().filter(|o| o.container_id == open_container),
                    text,
                    partial,
                    exclude_object_id,
                )
            }
        })
        .or_else(|| {
            let player = host.navigation();
            nearest(
                &player,
                objects.iter().filter(|o| o.is_landscape),
                text,
                partial,
                exclude_object_id,
            )
        }),
    }
}

/// The nearest object of one of `classes` whose name answers to `name`. A
/// blank name matches every object of those classes, which is how "the
/// closest portal" is asked for.
pub fn try_find_nearest(
    host: &mut dyn MetaHost,
    name: &str,
    partial: bool,
    classes: &[ObjectClass],
) -> Option<WorldObject> {
    let text = name.trim();
    let objects = host.objects();
    if !text.is_empty()
        && let Some(found) = find_by_special_name(host, &objects, text)
        && classes.contains(&found.object_class)
    {
        return Some(found);
    }
    let own = host.character().map_or(0, |info| info.id);
    let player = host.navigation();
    let mut best: Option<(&WorldObject, f64)> = None;
    for candidate in &objects {
        if candidate.object_id == own || !classes.contains(&candidate.object_class) {
            continue;
        }
        if !matches(&candidate.name, text, partial) {
            continue;
        }
        let distance = distance(&player, candidate);
        if best.is_some_and(|(_, best_distance)| distance >= best_distance) {
            continue;
        }
        best = Some((candidate, distance));
    }
    best.map(|(found, _)| found.clone())
}

/// The fellowship member a name stands for, the way the fellow commands
/// look: an object id (decimal or hexadecimal) or the word "selected" may
/// name anyone on the roster, the character included; a name, whole or
/// `partial`, names the nearest other member it matches, and a blank name
/// the nearest other member.
pub fn try_find_fellow(
    host: &mut dyn MetaHost,
    roster: &[FellowMember],
    name: &str,
    partial: bool,
) -> Option<FellowMember> {
    let text = name.trim();
    if let Some(found) = fellow_by_id(roster, text) {
        return Some(found);
    }
    // The reference matches the word itself, so a character called
    // "Selected" can still be named by typing it with its capital.
    if text == "selected"
        && let Some(selected) = host.selection().selected
        && selected != 0
        && let Some(found) = fellow(roster, selected)
    {
        return Some(found);
    }
    let own = host.character().map_or(0, |info| info.id);
    let mut best: Option<&FellowMember> = None;
    for candidate in roster {
        if candidate.object_id == own || !matches(&candidate.name, text, partial) {
            continue;
        }
        if best.is_some_and(|current| current.distance <= candidate.distance) {
            continue;
        }
        best = Some(candidate);
    }
    best.cloned()
}

fn fellow_by_id(roster: &[FellowMember], text: &str) -> Option<FellowMember> {
    if text.is_empty() {
        return None;
    }
    if let Some(id) = try_parse_uint(text)
        && let Some(found) = fellow(roster, id)
    {
        return Some(found);
    }
    parse_hex(text).and_then(|id| fellow(roster, id))
}

fn fellow(roster: &[FellowMember], object_id: u32) -> Option<FellowMember> {
    roster
        .iter()
        .find(|candidate| candidate.object_id == object_id)
        .cloned()
}

/// Flat ground distance from the character, in metres. An object with no
/// position of its own is on the character, so it is zero away.
#[must_use]
pub fn distance(player: &NavigationSnapshot, candidate: &WorldObject) -> f64 {
    match &candidate.position {
        Some(position) if player.is_available => {
            player.position.horizontal_distance_metres(position)
        }
        _ => 0.0,
    }
}

/// A hexadecimal number with or without its `0x`
/// (`uint.TryParse(hex, NumberStyles.HexNumber)`).
#[must_use]
pub fn parse_hex(text: &str) -> Option<u32> {
    let hex = if text.len() >= 2 && ordinal::starts_with_ignore_case(text, "0x") {
        &text[2..]
    } else {
        text
    };
    number::parse_u32(hex, NumberStyles::HEX_NUMBER)
}

fn find_by_special_name(
    host: &mut dyn MetaHost,
    objects: &[WorldObject],
    text: &str,
) -> Option<WorldObject> {
    if let Some(id) = try_parse_uint(text)
        && let Some(found) = by_id(objects, id)
    {
        return Some(found);
    }
    if let Some(id) = parse_hex(text)
        && let Some(found) = by_id(objects, id)
    {
        return Some(found);
    }
    if ordinal::equals_ignore_case(text, "selected")
        && let Some(selected) = host.selection().selected
        && selected != 0
    {
        return by_id(objects, selected);
    }
    None
}

fn by_id(objects: &[WorldObject], object_id: u32) -> Option<WorldObject> {
    objects
        .iter()
        .find(|candidate| candidate.object_id == object_id)
        .cloned()
}

fn first<'a>(
    candidates: impl Iterator<Item = &'a WorldObject>,
    text: &str,
    partial: bool,
    exclude_object_id: u32,
) -> Option<WorldObject> {
    let mut ordered: Vec<&WorldObject> = candidates.collect();
    ordered.sort_by_key(|candidate| candidate.object_id);
    ordered
        .into_iter()
        .find(|candidate| {
            candidate.object_id != exclude_object_id && matches(&candidate.name, text, partial)
        })
        .cloned()
}

fn nearest<'a>(
    player: &NavigationSnapshot,
    candidates: impl Iterator<Item = &'a WorldObject>,
    text: &str,
    partial: bool,
    exclude_object_id: u32,
) -> Option<WorldObject> {
    let mut best: Option<(&WorldObject, f64)> = None;
    for candidate in candidates {
        if candidate.object_id == exclude_object_id || !matches(&candidate.name, text, partial) {
            continue;
        }
        let distance = distance(player, candidate);
        if best.is_some_and(|(_, best_distance)| distance >= best_distance) {
            continue;
        }
        best = Some((candidate, distance));
    }
    best.map(|(found, _)| found.clone())
}

fn matches(name: &str, text: &str, partial: bool) -> bool {
    text.is_empty()
        || if partial {
            ordinal::contains_ignore_case(name, text)
        } else {
            ordinal::equals_ignore_case(name, text)
        }
}
