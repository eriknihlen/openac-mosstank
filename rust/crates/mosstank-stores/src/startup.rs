//! What the plugin does with its files when it starts: the one-off copies
//! out of the old layouts, and the line each profile load produces naming
//! the file and the path it came off (the C#'s `MossTankPanel.FileLayout`).

use openac_plugin_compat::datetime::DateTime;

use crate::layout_migration;
use crate::profile_store::ProfileLoad;
use crate::storage::{Root, StoreHost, display_path};
use crate::ub_tree_migration;

/// Fills the plugin's folder from the old flat layout and copies the UB
/// tree across, once each; the lines to tell the player (also logged). A
/// failed copy leaves the old files where they were and is logged, so the
/// run goes on with an empty folder.
pub fn migrate_file_layout(host: &mut dyn StoreHost, now: DateTime) -> Vec<String> {
    let mut lines = Vec::new();
    let result = (|| -> Result<(), String> {
        let layout = layout_migration::run(host.storage(Root::Profiles), now)?;
        let root = host.storage(Root::Profiles).root_path();
        if let Some(line) = layout.describe(root.as_deref()) {
            host.log().info(&line);
            lines.push(line);
        }
        // The UB tree used to sit in the plugin's own storage; it belongs
        // beside the other profile files.
        let ub = ub_tree_migration::run(host)?;
        let root = host.storage(Root::Profiles).root_path();
        if let Some(line) = ub.describe(root.as_deref()) {
            host.log().info(&line);
            lines.push(line);
        }
        Ok(())
    })();
    if let Err(error) = result {
        host.log().warn(&format!(
            "MossTank could not copy its files into their own folder: {error}"
        ));
    }
    lines
}

/// The one line a load produces: what kind of profile, which file, and the
/// full path it came off (and why it did not read, when known). Nothing for
/// a profile that does not exist yet, or a load without a key.
#[must_use]
pub fn load_announcement(
    kind: &str,
    storage_key: Option<&str>,
    outcome: ProfileLoad,
    reason: Option<&str>,
    root_path: Option<&str>,
) -> Option<String> {
    let key = storage_key.filter(|key| !key.is_empty())?;
    let file = key.rfind('/').map_or(key, |slash| &key[slash + 1..]);
    let path = match root_path {
        Some(root) if !root.is_empty() => display_path(root, key),
        _ => format!("{key} (the client does not report where its profile directory is)"),
    };
    match outcome {
        ProfileLoad::Loaded => Some(format!("Loaded {kind} profile {file} from {path}")),
        ProfileLoad::Partial => Some(format!(
            "Loaded {kind} profile {file} from {path} up to an incomplete tail"
        )),
        ProfileLoad::Failed => Some(format!(
            "Could not load {kind} profile {file} from {path}{}",
            match reason {
                Some(reason)
                    if !openac_plugin_compat::text::casing::is_white_space_only(reason) =>
                {
                    format!(": {reason}")
                }
                _ => String::new(),
            }
        )),
        ProfileLoad::Missing => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryHost;

    #[test]
    fn a_load_names_the_file_and_its_path() {
        assert_eq!(
            load_announcement(
                "loot",
                Some("mosstank/loot/Keep.utl"),
                ProfileLoad::Loaded,
                None,
                Some("C:\\vtank")
            )
            .as_deref(),
            Some("Loaded loot profile Keep.utl from C:\\vtank\\mosstank\\loot\\Keep.utl")
        );
        assert_eq!(
            load_announcement(
                "meta",
                Some("mosstank/metas/A.af"),
                ProfileLoad::Failed,
                Some("line 1: bad"),
                None
            )
            .as_deref(),
            Some(
                "Could not load meta profile A.af from mosstank/metas/A.af (the client does not report where its profile directory is): line 1: bad"
            )
        );
        assert_eq!(
            load_announcement("route", Some("x"), ProfileLoad::Missing, None, None),
            None
        );
        assert_eq!(
            load_announcement("route", Some(""), ProfileLoad::Loaded, None, None),
            None
        );
    }

    #[test]
    fn the_first_start_copies_both_old_layouts_and_says_so() {
        let mut host = MemoryHost::on("Coldeve");
        host.profiles.insert("Keep.utl", "loot\r\n");
        host.plugin.insert("ub/settings.json", "{}");
        let lines = migrate_file_layout(&mut host, DateTime::utc_from_unix_millis(0));
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("1 loot"));
        assert!(lines[1].contains("1 UtilityBelt file(s)"));
        assert_eq!(
            host.profiles.get("mosstank/loot/Keep.utl"),
            Some("loot\r\n")
        );
        assert_eq!(host.profiles.get("mosstank/ub/settings.json"), Some("{}"));
        assert_eq!(host.log.infos.len(), 2);
        assert!(migrate_file_layout(&mut host, DateTime::utc_from_unix_millis(0)).is_empty());
    }
}
