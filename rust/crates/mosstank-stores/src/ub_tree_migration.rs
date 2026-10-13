//! The one-time copy of the UB tree from the plugin's own storage, where an
//! earlier version wrote it at `ub/`, into the shared profile folder at
//! `mosstank/ub/`. It only copies; a destination already holding a file is
//! left as it is, which makes a second run a no-op.

use openac_plugin_compat::text::ordinal;

use crate::storage::{PluginStorage, StorageError, display_path};
use crate::ub_store::ROOT;

/// The prefix the tree used to sit under, inside the plugin's own storage.
pub const LEGACY_ROOT: &str = "ub/";

/// What one pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UbTreeMigrationResult {
    /// Files copied.
    pub copied: u32,
    /// Files whose destination already held one, left alone.
    pub already_present: u32,
}

impl UbTreeMigrationResult {
    /// The one line a person reads, `None` when nothing was copied.
    #[must_use]
    pub fn describe(&self, root_path: Option<&str>) -> Option<String> {
        if self.copied == 0 {
            return None;
        }
        let tree = ROOT.trim_end_matches('/');
        let place = match root_path {
            Some(root) if !root.is_empty() => display_path(root, tree),
            _ => tree.to_owned(),
        };
        let kept = if self.already_present > 0 {
            format!(
                " {} file(s) already there were left alone.",
                self.already_present
            )
        } else {
            String::new()
        };
        Some(format!(
            "MossTank copied {} UtilityBelt file(s) into {place} (the originals were left where they were).{kept}",
            self.copied
        ))
    }
}

/// Copies every file under the old prefix to the same relative path under
/// the new root.
///
/// # Errors
/// Either storage cannot be listed, read or written.
pub fn run(
    plugin_storage: &mut dyn PluginStorage,
    vtank_profiles: &mut dyn PluginStorage,
) -> Result<UbTreeMigrationResult, StorageError> {
    let mut result = UbTreeMigrationResult::default();
    if !plugin_storage.is_available() || !vtank_profiles.is_available() {
        return Ok(result);
    }
    let mut keys: Vec<String> = plugin_storage
        .list("")?
        .into_iter()
        .filter(|key| ordinal::starts_with_ignore_case(key, LEGACY_ROOT))
        .collect();
    keys.sort_by(|a, b| ordinal::compare(a, b));
    for key in keys {
        let relative = &key[LEGACY_ROOT.len()..];
        if relative.is_empty() {
            continue;
        }
        let destination = format!("{ROOT}{relative}");
        if vtank_profiles.read_text(&destination)?.is_some() {
            result.already_present += 1;
            continue;
        }
        let Some(content) = plugin_storage.read_text(&key)? else {
            continue;
        };
        vtank_profiles.write_text(&destination, &content)?;
        result.copied += 1;
    }
    Ok(result)
}
