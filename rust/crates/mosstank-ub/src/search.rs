//! Finding the object a typed name stands for (`UbObjectSearch`) from the
//! UB commands: the search itself is `mosstank-vendor`'s
//! ([`mosstank_vendor::object_search`]); these read what it looks through
//! from the client once and hand it over.

use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_meta::host::MetaHost;
use mosstank_vendor::object_search::{self, SearchView};

pub use mosstank_vendor::object_search::{UbSearchScope, distance};

/// What a search looks through, read from the client once.
struct Snapshot {
    objects: Vec<WorldObject>,
    self_id: u32,
    player: ac_plugin_abi::navigation::NavigationSnapshot,
    selected: Option<u32>,
    open_container: u32,
}

impl Snapshot {
    fn read(host: &mut dyn MetaHost) -> Self {
        Self {
            objects: host.objects(),
            self_id: host.character().map_or(0, |info| info.id),
            player: host.navigation(),
            selected: host.selection().selected,
            open_container: host.open_container(),
        }
    }

    fn view(&self) -> SearchView<'_> {
        SearchView {
            objects: &self.objects,
            self_id: self.self_id,
            player: &self.player,
            selected: self.selected,
            open_container: self.open_container,
        }
    }
}

/// The object a name stands for: the whole name (part of one when
/// `partial`), a decimal or hexadecimal object id, or the word "selected";
/// `exclude_object_id` is skipped (the first half of "use A on B").
pub fn try_find(
    host: &mut dyn MetaHost,
    name: &str,
    scope: UbSearchScope,
    partial: bool,
    exclude_object_id: u32,
) -> Option<WorldObject> {
    if name.trim().is_empty() {
        return None;
    }
    let snapshot = Snapshot::read(host);
    object_search::try_find(
        &snapshot.view(),
        Some(name),
        scope,
        partial,
        exclude_object_id,
        &mut |candidate| name_with_material(host, candidate),
    )
    .cloned()
}

/// The `MaterialType` int property: what an object is made of.
const MATERIAL_TYPE_PROPERTY: u32 = 131;

/// The name the game shows for an object, material first: the client keeps
/// a salvage bag's material apart from its name "Salvage", and "Steel
/// Salvage" is what a player types.
pub fn name_with_material(host: &mut dyn MetaHost, object: &WorldObject) -> String {
    match host.int_property(object.object_id, MATERIAL_TYPE_PROPERTY) {
        Some(material) => mosstank_data::material::with_material(&object.name, material),
        None => object.name.clone(),
    }
}

/// The nearest object of one of `classes` whose name answers to `name`; a
/// blank name matches every object of those classes.
pub fn try_find_nearest(
    host: &mut dyn MetaHost,
    name: &str,
    partial: bool,
    classes: &[ObjectClass],
) -> Option<WorldObject> {
    let snapshot = Snapshot::read(host);
    object_search::try_find_nearest(&snapshot.view(), Some(name), partial, classes).cloned()
}

/// The fellowship member a name stands for.
pub fn try_find_fellow(
    host: &mut dyn MetaHost,
    roster: &[FellowMember],
    name: &str,
    partial: bool,
) -> Option<FellowMember> {
    let self_id = host.character().map_or(0, |info| info.id);
    let selected = host.selection().selected;
    object_search::try_find_fellow(roster, Some(name), partial, self_id, selected).cloned()
}

/// A hexadecimal number with or without its `0x`
/// (`uint.TryParse(hex, NumberStyles.HexNumber)`).
#[must_use]
pub fn parse_hex(text: &str) -> Option<u32> {
    let hex =
        if text.len() >= 2 && mosstank_dotnet::text::ordinal::starts_with_ignore_case(text, "0x") {
            &text[2..]
        } else {
            text
        };
    openac_plugin_compat::number::parse_u32(
        hex,
        openac_plugin_compat::number::NumberStyles::HEX_NUMBER,
    )
}
