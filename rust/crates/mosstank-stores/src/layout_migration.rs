//! The first-run copy from the flat layout older versions wrote into the
//! shared profile folder, into the plugin's own `mosstank` folder. It only
//! ever copies: the shared folder is not this plugin's to tidy, and a file
//! already at its new name is left as it is, which makes a second run a
//! no-op.

use std::collections::HashSet;

use mosstank_dotnet::collections::fold_ignore_case;
use mosstank_dotnet::text::ordinal;
use openac_plugin_compat::datetime::DateTime;

use crate::directory::{self, HIDDEN_PREFIX};
use crate::storage::{PluginStorage, StorageError, display_path};

/// What one pass copied into each folder, and the twins set aside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutMigrationResult {
    /// Settings files and bindings.
    pub settings: u32,
    /// Loot profiles.
    pub loot: u32,
    /// Routes.
    pub navs: u32,
    /// Metas.
    pub metas: u32,
    /// Older twins kept under a dated name.
    pub twins_kept: u32,
}

impl LayoutMigrationResult {
    /// Every file copied.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.settings + self.loot + self.navs + self.metas
    }

    /// The one line a person reads, `None` when nothing was copied.
    #[must_use]
    pub fn describe(&self, root_path: Option<&str>) -> Option<String> {
        if self.total() == 0 {
            return None;
        }
        let mut parts = Vec::new();
        if self.settings > 0 {
            parts.push(format!("{} settings", self.settings));
        }
        if self.loot > 0 {
            parts.push(format!("{} loot", self.loot));
        }
        if self.navs > 0 {
            parts.push(format!("{} nav", self.navs));
        }
        if self.metas > 0 {
            parts.push(format!("{} meta", self.metas));
        }
        let place = match root_path {
            Some(root) if !root.is_empty() => display_path(root, directory::ROOT),
            _ => directory::ROOT.to_owned(),
        };
        let twins = if self.twins_kept > 0 {
            format!(
                " {} duplicate character file(s) were kept alongside.",
                self.twins_kept
            )
        } else {
            String::new()
        };
        Some(format!(
            "MossTank copied {} file(s) into {place} (the originals were left where they were).{twins}",
            parts.join(", ")
        ))
    }
}

fn bare_name(key: &str) -> &str {
    key.rfind('/').map_or(key, |slash| &key[slash + 1..])
}

/// Copies every flat-layout file into the plugin's folder.
///
/// # Errors
/// The storage cannot be listed, read or written.
pub fn run(
    storage: &mut dyn PluginStorage,
    now: DateTime,
) -> Result<LayoutMigrationResult, StorageError> {
    let mut result = LayoutMigrationResult::default();
    if !storage.is_available() {
        return Ok(result);
    }
    // What this run itself wrote into the new layout: only such a file may
    // be displaced by its marked twin.
    let mut written_this_run: HashSet<String> = HashSet::new();
    // A file whose name carries the display marker goes last, so of two
    // spellings of one character the marked (live) one lands on top.
    let mut keys = storage.list("")?;
    keys.sort_by(|a, b| {
        has_display_marker(a)
            .cmp(&has_display_marker(b))
            .then_with(|| ordinal::compare_ignore_case(a, b))
    });
    for key in keys {
        let Some(target) = destination(&key) else {
            continue;
        };
        let Some(kept_twin) = try_copy(storage, &key, &target, now, &mut written_this_run)? else {
            continue;
        };
        if kept_twin {
            result.twins_kept += 1;
        }
        if target.starts_with(directory::SETTINGS_FOLDER) {
            result.settings += 1;
        } else if target.starts_with(directory::LOOT_FOLDER) {
            result.loot += 1;
        } else if target.starts_with(directory::NAV_FOLDER) {
            result.navs += 1;
        } else {
            result.metas += 1;
        }
    }
    Ok(result)
}

/// Where one old-layout key belongs, `None` when it is not this plugin's to
/// move (anything already in its folder, side files, other tools'
/// subfolders).
fn destination(key: &str) -> Option<String> {
    if ordinal::starts_with_ignore_case(key, &format!("{}/", directory::ROOT)) {
        return None;
    }
    let bare = bare_name(key);
    if bare.is_empty() || directory::is_side_file(bare) {
        return None;
    }
    let folder = if key.len() == bare.len() {
        ""
    } else {
        &key[..key.len() - bare.len() - 1]
    };
    if ordinal::equals_ignore_case(folder, directory::LEGACY_NAV_FOLDER) {
        return Some(format!("{}/{}", directory::NAV_FOLDER, canonical(bare)));
    }
    if ordinal::equals_ignore_case(folder, directory::LEGACY_META_FOLDER) {
        return Some(format!("{}/{}", directory::META_FOLDER, canonical(bare)));
    }
    if !folder.is_empty() {
        return None; // someone else's subfolder.
    }
    if ordinal::ends_with_ignore_case(bare, ".usd") || ordinal::ends_with_ignore_case(bare, ".cdf")
    {
        return Some(format!(
            "{}/{}",
            directory::SETTINGS_FOLDER,
            canonical(bare)
        ));
    }
    if ordinal::ends_with_ignore_case(bare, ".utl") {
        return Some(format!("{}/{}", directory::LOOT_FOLDER, canonical(bare)));
    }
    if ordinal::ends_with_ignore_case(bare, ".af") {
        // The flat layout told routes and metas apart by a name marker.
        return Some(if directory::is_legacy_flat_route_file_name(bare) {
            format!(
                "{}/{}",
                directory::NAV_FOLDER,
                canonical(&directory::strip_legacy_nav_marker(bare))
            )
        } else {
            format!("{}/{}", directory::META_FOLDER, canonical(bare))
        });
    }
    None
}

fn has_display_marker(key: &str) -> bool {
    let bare = bare_name(key);
    canonical(bare) != bare
}

/// The name in the new layout: without the display marker the world puts
/// in front of some characters' names (after the separator in a binding
/// file, named `<server>_<character>`).
fn canonical(bare: &str) -> String {
    if let Some(rest) = bare.strip_prefix(&format!("{HIDDEN_PREFIX}+")) {
        return format!("{HIDDEN_PREFIX}{rest}");
    }
    if ordinal::ends_with_ignore_case(bare, ".cdf")
        && let Some(marker) = bare.find("_+")
    {
        let mut name = bare.to_owned();
        name.remove(marker + 1);
        return name;
    }
    bare.to_owned()
}

/// Copies one file unless its destination is taken; a taken destination
/// this key is the marked twin of is kept under a dated name. `None` when
/// nothing was copied, else whether a twin was kept.
fn try_copy(
    storage: &mut dyn PluginStorage,
    source_key: &str,
    destination_key: &str,
    now: DateTime,
    written_this_run: &mut HashSet<String>,
) -> Result<Option<bool>, StorageError> {
    let Some(content) = storage.read_text(source_key)? else {
        return Ok(None);
    };
    if let Some(existing) = storage.read_text(destination_key)? {
        let is_twin =
            !ordinal::equals_ignore_case(bare_name(source_key), bare_name(destination_key));
        if !is_twin || !written_this_run.contains(&fold_ignore_case(destination_key)) {
            return Ok(None); // already here before this run: the live file.
        }
        let date = now
            .format("yyyy-MM-dd")
            .expect("a fixed date format the kit prints");
        let kept_key = format!("{destination_key}{}{date}", directory::TWIN_SUFFIX);
        if storage.read_text(&kept_key)?.is_some() {
            return Ok(None); // an earlier run already resolved this pair.
        }
        storage.write_text(&kept_key, &existing)?;
        storage.write_text(destination_key, &content)?;
        return Ok(Some(true));
    }
    storage.write_text(destination_key, &content)?;
    written_this_run.insert(fold_ignore_case(destination_key));
    Ok(Some(false))
}
