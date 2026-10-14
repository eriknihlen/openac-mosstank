//! When the profile pickers list their folders again (register UI-4).
//!
//! The C# pickers are menus whose items are read when the window is built
//! and each time a menu opens. The plugin's windows are pushed values, and a
//! push a tick listed four folders of hundreds of files each (30-40 M
//! instructions with a real player's folder). Here a picker's names, and the
//! selected route and meta it shows by them, are worked out again only when
//! what they come from may have changed: the plugin wrote to the folder, a
//! window attached, or the store behind it changed; and, for the files
//! other clients write, each folder is listed again every few seconds, one
//! folder at a time. In between, the value pushed last stands. The meta
//! grid and the loot rows are kept by their own revisions the same way.

use std::hash::{DefaultHasher, Hash, Hasher};

use super::super::TankCx;

/// The folders the pickers list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Folder {
    Settings,
    Routes,
    Loot,
    Metas,
    UbProfiles,
}

impl Folder {
    const ALL: [Self; 5] = [
        Self::Settings,
        Self::Routes,
        Self::Loot,
        Self::Metas,
        Self::UbProfiles,
    ];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or_default()
    }

    /// The folder a key of the profile folder is in; `None` for one the
    /// pickers do not list alone (the root, say).
    fn of_key(key: &str) -> Option<Self> {
        use mosstank_dotnet::text::ordinal::starts_with_ignore_case;
        use mosstank_stores::directory::{LOOT_FOLDER, META_FOLDER, NAV_FOLDER, SETTINGS_FOLDER};
        let key = key.replace('\\', "/");
        let inside = |folder: &str| {
            starts_with_ignore_case(&key, folder)
                && (key.len() == folder.len() || key.as_bytes()[folder.len()] == b'/')
        };
        [
            (SETTINGS_FOLDER, Self::Settings),
            (NAV_FOLDER, Self::Routes),
            (LOOT_FOLDER, Self::Loot),
            (META_FOLDER, Self::Metas),
            (
                mosstank_stores::ub_store::ROOT.trim_end_matches('/'),
                Self::UbProfiles,
            ),
        ]
        .into_iter()
        .find(|(folder, _)| inside(folder))
        .map(|(_, folder)| folder)
    }
}

/// How often each folder is listed again for what other clients wrote,
/// seconds.
pub(crate) const RELIST_SECONDS: f64 = 5.0;

/// The folders' listing generations: a picker's value is worked out again
/// when its folder's generation moved.
#[derive(Debug, Clone, Default)]
pub(crate) struct Listings {
    generations: [u64; Folder::ALL.len()],
    since_relist: f64,
    next: usize,
}

impl Listings {
    /// Every folder is listed again at the next push.
    pub(crate) fn relist_all(&mut self) {
        for generation in &mut self.generations {
            *generation += 1;
        }
    }

    /// The folder the plugin wrote `key` in is listed again at the next
    /// push; a key outside the pickers' folders, every folder.
    pub(crate) fn relist_key(&mut self, key: &str) {
        match Folder::of_key(key) {
            Some(folder) => self.generations[folder.index()] += 1,
            None => self.relist_all(),
        }
    }

    /// A frame of `elapsed_seconds`: every [`RELIST_SECONDS`], each folder
    /// in turn is listed again (one a push, so a push lists one folder).
    pub(crate) fn advance(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return;
        }
        self.since_relist += elapsed_seconds;
        #[allow(clippy::cast_precision_loss)]
        let interval = RELIST_SECONDS / Folder::ALL.len() as f64;
        if self.since_relist >= interval {
            self.since_relist = 0.0;
            self.generations[self.next] += 1;
            self.next = (self.next + 1) % Folder::ALL.len();
        }
    }

    fn generation(&self, folder: Folder) -> u64 {
        self.generations[folder.index()]
    }
}

/// What a costly value is worked out from ([`TankCx::value_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueKey {
    /// The same number, the same value.
    pub key: u64,
    /// The value lists a folder: a push lists one folder at most.
    pub listing: bool,
}

fn key(folder_generation: u64, inputs: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    folder_generation.hash(&mut hasher);
    inputs.hash(&mut hasher);
    hasher.finish()
}

impl TankCx<'_> {
    /// What a costly value is worked out from, as one number: the same
    /// number, the same value. The pickers by their folder's listing and
    /// their store; the meta grid by its profile's revision; the loot rows
    /// by theirs. `None` for a binding worked out at every push.
    pub(crate) fn value_key(&self, name: &str) -> Option<ValueKey> {
        let core = &*self.core;
        let listings = &core.panel.listings;
        let generation = |folder| listings.generation(folder);
        let listed = |key| ValueKey { key, listing: true };
        let kept = |key| ValueKey {
            key,
            listing: false,
        };
        Some(match name {
            "MacroProfileNames" => listed(key(
                generation(Folder::Settings),
                core.profiles.picker_inputs(),
            )),
            "RouteProfileNames" => {
                listed(key(generation(Folder::Routes), &core.embedded_route_label))
            }
            "SelectedRouteProfile" => listed(key(
                generation(Folder::Routes),
                (
                    &core.embedded_route_label,
                    core.route_profiles.selected_key(),
                ),
            )),
            "LootProfileNames" => listed(key(
                generation(Folder::Loot),
                core.loot_profiles.has_active_profile(),
            )),
            "MetaProfileNames" => listed(key(generation(Folder::Metas), ())),
            "SelectedMetaProfile" => listed(key(
                generation(Folder::Metas),
                core.meta_profiles.selected_key(),
            )),
            "UbProfileNames" => listed(key(generation(Folder::UbProfiles), ())),
            "MetaRows"
            | "MetaDeleteColumn"
            | "MetaMoveUpIcons"
            | "MetaMoveDownIcons"
            | "MetaStateColumn"
            | "MetaConditionColumn"
            | "MetaActionColumn"
            | "MetaCurrentStateNames" => kept(key(self.meta.profile_revision(), ())),
            "LootRuleRows" => kept(key(core.panel.loot_rule_rows_revision, ())),
            _ => return None,
        })
    }
}
