//! The folder tree the plugin owns, in one list, so the folders that are
//! made and the folders that are tested are the same; and the preparer that
//! makes it: the fixed part when the plugin starts, the per-character part
//! on each login. A folder that only appears once something is written into
//! it is a folder nobody can drop a profile into beforehand.

use std::collections::HashSet;

use mosstank_dotnet::collections::fold_ignore_case;
use mosstank_dotnet::text::casing;

use crate::directory;
use crate::storage::{PluginStorage, display_path};
use crate::ub_store::ROOT as UB_ROOT;

const AUTO_VENDOR_FOLDER: &str = "autovendor";
const EQUIP_FOLDER: &str = "equip";
const ITEM_GIVER_FOLDER: &str = "itemgiver";

/// The folders inside the shared profile folder: everything a person
/// edits, copies or shares, the whole UB tree among them.
#[must_use]
pub fn vtank_profile_folders() -> Vec<String> {
    let mut folders: Vec<String> = [
        directory::SETTINGS_FOLDER,
        directory::META_FOLDER,
        directory::NAV_FOLDER,
        directory::LOOT_FOLDER,
    ]
    .iter()
    .map(|folder| (*folder).to_owned())
    .collect();
    folders.push(UB_ROOT.to_owned());
    for folder in [
        "profiles",
        AUTO_VENDOR_FOLDER,
        EQUIP_FOLDER,
        ITEM_GIVER_FOLDER,
        "maps",
        "dungeonmaps",
        "dungeonmaps/visited",
    ] {
        folders.push(format!("{UB_ROOT}{folder}"));
    }
    folders
}

/// The folders inside the plugin's own storage that do not depend on who is
/// logged in: only state the plugin keeps for itself.
pub const PLUGIN_STORAGE_FOLDERS: [&str; 4] = [
    "expressions/global",
    "expressions/persistent",
    "profiles/macro",
    "profiles/macro/sidecar",
];

/// The key of an item giver profile by name (with or without `.utl`), in
/// the shared `itemgiver` folder.
#[must_use]
pub fn item_giver_profile_key(name: &str) -> String {
    let mut bare = name.trim();
    if bare.len() >= 4 && bare.is_char_boundary(bare.len() - 4) {
        let (head, tail) = bare.split_at(bare.len() - 4);
        if tail.eq_ignore_ascii_case(".utl") {
            bare = head;
        }
    }
    format!("{UB_ROOT}{ITEM_GIVER_FOLDER}/{bare}.utl")
}

/// The folders of one login inside the shared profile folder: the server
/// tier and, once the character is known, the character tier beneath it.
/// Nothing while the world is unknown.
#[must_use]
pub fn character_folders(server: Option<&str>, character: Option<&str>) -> Vec<String> {
    let world = safe(server);
    if world.is_empty() {
        return Vec::new();
    }
    let server_root = format!("{UB_ROOT}{world}/");
    let mut folders = vec![
        server_root.clone(),
        format!("{server_root}{AUTO_VENDOR_FOLDER}"),
        format!("{server_root}{EQUIP_FOLDER}"),
    ];
    let name = safe(character);
    if name.is_empty() {
        return folders;
    }
    let character_root = format!("{server_root}{name}/");
    folders.push(character_root.clone());
    folders.push(format!("{character_root}{AUTO_VENDOR_FOLDER}"));
    folders.push(format!("{character_root}{EQUIP_FOLDER}"));
    folders
}

/// A name from the world made fit for a folder: trimmed, every character
/// some system refuses in a file name an underscore; empty for blank.
#[must_use]
pub fn safe(value: Option<&str>) -> String {
    match value {
        Some(text) if !casing::is_white_space_only(text) => portable_file_name(text.trim()),
        _ => String::new(),
    }
}

/// `value` with every character some supported system refuses in a file
/// or folder name replaced by an underscore. The set is fixed, so a name is
/// spelled the same on every system.
#[must_use]
pub fn portable_file_name(value: &str) -> String {
    value
        .chars()
        .map(|c| if is_refused_anywhere(c) { '_' } else { c })
        .collect()
}

fn is_refused_anywhere(c: char) -> bool {
    c < ' ' || matches!(c, '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\' | '/')
}

/// A callback for one line of text.
pub type LineSink = Box<dyn FnMut(&str)>;

/// Makes the folder tree: the fixed part once, the per-character part on
/// each login that names a world or character not seen before. A folder
/// that cannot be made is not fatal: its path goes to the log once, one
/// chat line says so once, and the run goes on.
pub struct StorageFolderPreparer {
    log: LineSink,
    announce: LineSink,
    made: HashSet<String>,
    refused: HashSet<String>,
    announced_refusal: bool,
    start_done: bool,
}

impl std::fmt::Debug for StorageFolderPreparer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageFolderPreparer")
            .field("made", &self.made.len())
            .field("start_done", &self.start_done)
            .finish_non_exhaustive()
    }
}

/// The chat line said once when part of the tree could not be made.
pub const REFUSAL_ANNOUNCEMENT: &str =
    "MossTank could not create part of its folder structure; see the log for the paths.";

impl StorageFolderPreparer {
    /// A preparer that logs to `log` and tells the player through
    /// `announce`.
    pub fn new(log: impl FnMut(&str) + 'static, announce: impl FnMut(&str) + 'static) -> Self {
        Self {
            log: Box::new(log),
            announce: Box::new(announce),
            made: HashSet::new(),
            refused: HashSet::new(),
            announced_refusal: false,
            start_done: false,
        }
    }

    /// The folders that do not depend on a login, across both roots. Runs
    /// once for the life of the plugin.
    pub fn prepare_start(
        &mut self,
        vtank_profiles: &mut dyn PluginStorage,
        plugin_storage: &mut dyn PluginStorage,
    ) {
        if self.start_done {
            return;
        }
        self.start_done = true;
        self.prepare("vtank", vtank_profiles, &vtank_profile_folders());
        let plugin_folders: Vec<String> = PLUGIN_STORAGE_FOLDERS
            .iter()
            .map(|folder| (*folder).to_owned())
            .collect();
        self.prepare("storage", plugin_storage, &plugin_folders);
    }

    /// [`Self::prepare_start`] with both storages reached through one
    /// store host, one after the other.
    pub fn prepare_start_on(&mut self, host: &mut dyn crate::storage::StoreHost) {
        if self.start_done {
            return;
        }
        self.start_done = true;
        self.prepare(
            "vtank",
            host.storage(crate::storage::Root::Profiles),
            &vtank_profile_folders(),
        );
        let plugin_folders: Vec<String> = PLUGIN_STORAGE_FOLDERS
            .iter()
            .map(|folder| (*folder).to_owned())
            .collect();
        self.prepare(
            "storage",
            host.storage(crate::storage::Root::Plugin),
            &plugin_folders,
        );
    }

    /// The server and character tiers of the UB tree. Called on every
    /// login; only a new world or character does any work.
    pub fn prepare_character(
        &mut self,
        vtank_profiles: &mut dyn PluginStorage,
        server: Option<&str>,
        character: Option<&str>,
    ) {
        self.prepare(
            "vtank",
            vtank_profiles,
            &character_folders(server, character),
        );
    }

    fn prepare(&mut self, root: &str, storage: &mut dyn PluginStorage, folders: &[String]) {
        // No storage is not a failed folder: there is nowhere for the tree
        // to go and nothing to tell anyone.
        if !storage.is_available() {
            return;
        }
        for folder in folders {
            let remembered = fold_ignore_case(&format!("{root}|{folder}"));
            if self.made.contains(&remembered) {
                continue;
            }
            if self.create(storage, folder) {
                self.made.insert(remembered);
            }
        }
    }

    fn create(&mut self, storage: &mut dyn PluginStorage, folder: &str) -> bool {
        let path = path_of(storage, folder);
        match storage.ensure_directory(folder) {
            Ok(true) => true,
            Ok(false) => {
                self.refused_line(&format!("MossTank could not create the folder {path}."));
                false
            }
            Err(error) => {
                self.refused_line(&format!(
                    "MossTank could not create the folder {path}: {error}"
                ));
                false
            }
        }
    }

    /// One log line per path, one chat line for the whole run.
    fn refused_line(&mut self, line: &str) {
        if !self.refused.insert(fold_ignore_case(line)) {
            return;
        }
        (self.log)(line);
        if self.announced_refusal {
            return;
        }
        self.announced_refusal = true;
        (self.announce)(REFUSAL_ANNOUNCEMENT);
    }
}

fn path_of(storage: &mut dyn PluginStorage, folder: &str) -> String {
    let relative = folder.trim_end_matches('/');
    match storage.root_path() {
        Some(root) if !root.is_empty() => display_path(&root, relative),
        _ => relative.to_owned(),
    }
}
