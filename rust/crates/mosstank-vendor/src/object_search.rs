//! Finding the one object a typed name stands for (`UbObjectSearch`), in
//! the order the reference searches: a number or "selected" first, then
//! the packs, the open container, and finally the nearest matching thing on
//! the ground. The functions work on what the client was asked for once
//! ([`SearchView`]).

use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use openac_plugin_compat::number::{NumberStyles, parse_u32};
use openac_plugin_compat::text::ordinal;

/// Where a command that names an object may look; the reference's command
/// flags spell the same three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbSearchScope {
    /// Packs first, then the open container, then the landscape.
    All,
    /// Only what the character carries.
    Inventory,
    /// Only what stands in the world.
    Landscape,
}

/// What a search looks through.
#[derive(Debug, Clone, Copy)]
pub struct SearchView<'a> {
    /// Every object the client knows.
    pub objects: &'a [WorldObject],
    /// The character's own object id.
    pub self_id: u32,
    /// Where the character stands.
    pub player: &'a NavigationSnapshot,
    /// The selected object, if any.
    pub selected: Option<u32>,
    /// The container open on screen, 0 for none.
    pub open_container: u32,
}

fn trim(name: Option<&str>) -> &str {
    name.map_or("", str::trim)
}

fn matches(name: &str, text: &str, partial: bool) -> bool {
    text.is_empty()
        || if partial {
            ordinal::contains_ignore_case(name, text)
        } else {
            ordinal::equals_ignore_case(name, text)
        }
}

/// An id typed in decimal or in hexadecimal (with or without `0x`).
fn ids(text: &str) -> (Option<u32>, Option<u32>) {
    let decimal = parse_u32(text, NumberStyles::INTEGER);
    let hex_text = if ordinal::starts_with_ignore_case(text, "0x") {
        &text[2..]
    } else {
        text
    };
    let hex = parse_u32(hex_text, NumberStyles::HEX_NUMBER);
    (decimal, hex)
}

fn by_id(objects: &[WorldObject], object_id: u32) -> Option<&WorldObject> {
    objects
        .iter()
        .find(|candidate| candidate.object_id == object_id)
}

fn find_by_special_name<'a>(view: &SearchView<'a>, text: &str) -> Option<&'a WorldObject> {
    let (decimal, hex) = ids(text);
    if let Some(found) = decimal.and_then(|id| by_id(view.objects, id)) {
        return Some(found);
    }
    if let Some(found) = hex.and_then(|id| by_id(view.objects, id)) {
        return Some(found);
    }
    if ordinal::equals_ignore_case(text, "selected")
        && let Some(selected) = view.selected.filter(|selected| *selected != 0)
    {
        return by_id(view.objects, selected);
    }
    None
}

/// Flat ground distance from the character, metres. An object with no
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

fn first<'a>(
    candidates: impl Iterator<Item = &'a WorldObject>,
    text: &str,
    partial: bool,
    exclude_object_id: u32,
) -> Option<&'a WorldObject> {
    let mut ordered: Vec<&WorldObject> = candidates.collect();
    ordered.sort_by_key(|candidate| candidate.object_id);
    ordered.into_iter().find(|candidate| {
        candidate.object_id != exclude_object_id && matches(&candidate.name, text, partial)
    })
}

fn nearest<'a>(
    player: &NavigationSnapshot,
    candidates: impl Iterator<Item = &'a WorldObject>,
    text: &str,
    partial: bool,
    exclude_object_id: u32,
) -> Option<&'a WorldObject> {
    let mut best: Option<&WorldObject> = None;
    let mut best_distance = f64::MAX;
    for candidate in candidates {
        if candidate.object_id == exclude_object_id || !matches(&candidate.name, text, partial) {
            continue;
        }
        let distance = distance(player, candidate);
        if best.is_some() && distance >= best_distance {
            continue;
        }
        best = Some(candidate);
        best_distance = distance;
    }
    best
}

/// The object a name stands for: the whole name (part of one when
/// `partial`), a decimal or hexadecimal object id, or the word "selected".
/// `exclude_object_id` is an object the search must skip (the first half
/// of "use A on B").
#[must_use]
pub fn try_find<'a>(
    view: &SearchView<'a>,
    name: Option<&str>,
    scope: UbSearchScope,
    partial: bool,
    exclude_object_id: u32,
) -> Option<&'a WorldObject> {
    let text = trim(name);
    if text.is_empty() {
        return None;
    }
    if let Some(found) = find_by_special_name(view, text) {
        return Some(found);
    }
    let objects = view.objects;
    let owned = || objects.iter().filter(|object| object.is_owned);
    let landscape = || objects.iter().filter(|object| object.is_landscape);
    match scope {
        UbSearchScope::Inventory => first(owned(), text, partial, exclude_object_id),
        UbSearchScope::Landscape => {
            nearest(view.player, landscape(), text, partial, exclude_object_id)
        }
        UbSearchScope::All => first(owned(), text, partial, exclude_object_id)
            .or_else(|| {
                (view.open_container != 0)
                    .then(|| {
                        first(
                            objects
                                .iter()
                                .filter(|object| object.container_id == view.open_container),
                            text,
                            partial,
                            exclude_object_id,
                        )
                    })
                    .flatten()
            })
            .or_else(|| nearest(view.player, landscape(), text, partial, exclude_object_id)),
    }
}

/// The nearest object of one of `classes` whose name answers to `name`. A
/// blank name matches every object of those classes, which is how "the
/// closest vendor" is asked for.
#[must_use]
pub fn try_find_nearest<'a>(
    view: &SearchView<'a>,
    name: Option<&str>,
    partial: bool,
    classes: &[ObjectClass],
) -> Option<&'a WorldObject> {
    let text = trim(name);
    if !text.is_empty()
        && let Some(found) = find_by_special_name(view, text)
        && classes.contains(&found.object_class)
    {
        return Some(found);
    }
    let mut best: Option<&WorldObject> = None;
    let mut best_distance = f64::MAX;
    for candidate in view.objects {
        if candidate.object_id == view.self_id || !classes.contains(&candidate.object_class) {
            continue;
        }
        if !matches(&candidate.name, text, partial) {
            continue;
        }
        let distance = distance(view.player, candidate);
        if best.is_some() && distance >= best_distance {
            continue;
        }
        best = Some(candidate);
        best_distance = distance;
    }
    best
}

/// The fellowship member a name stands for, the way the reference's
/// fellow commands look: an object id (decimal or hexadecimal) or the word
/// "selected" may name anyone on the roster, the character included; a
/// name, whole or `partial`, names the nearest other member it matches, and
/// a blank name the nearest other member.
#[must_use]
pub fn try_find_fellow<'a>(
    roster: &'a [FellowMember],
    name: Option<&str>,
    partial: bool,
    self_id: u32,
    selected: Option<u32>,
) -> Option<&'a FellowMember> {
    let text = trim(name);
    let by_id = |object_id: u32| {
        roster
            .iter()
            .find(|candidate| candidate.object_id == object_id)
    };
    if !text.is_empty() {
        let (decimal, hex) = ids(text);
        if let Some(found) = decimal.and_then(by_id) {
            return Some(found);
        }
        if let Some(found) = hex.and_then(by_id) {
            return Some(found);
        }
    }
    // The reference matches the word itself, so a character called
    // "Selected" can still be named by typing it with its capital.
    if text == "selected"
        && let Some(found) = selected.filter(|selected| *selected != 0).and_then(by_id)
    {
        return Some(found);
    }
    let mut best: Option<&FellowMember> = None;
    for candidate in roster {
        if candidate.object_id == self_id || !matches(&candidate.name, text, partial) {
            continue;
        }
        if best.is_some_and(|current| current.distance <= candidate.distance) {
            continue;
        }
        best = Some(candidate);
    }
    best
}
