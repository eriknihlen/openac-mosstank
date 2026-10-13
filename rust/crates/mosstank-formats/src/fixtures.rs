//! The C# test fixtures, read in place (natively only; for tests).

use std::path::PathBuf;

/// `tests/AcDream.Plugins.MossTank.Tests/Fixtures` in the repository.
#[must_use]
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/AcDream.Plugins.MossTank.Tests/Fixtures")
}

/// A fixture's text as `File.ReadAllText` reads it (UTF-8, BOM detected).
///
/// # Panics
/// When the file cannot be read.
#[must_use]
pub fn read(relative: &str) -> String {
    let path = root().join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    openac_plugin_compat::text::read_all_text(&bytes)
}

/// A fixture under `Fixtures/vtank`.
#[must_use]
pub fn read_vtank(relative: &str) -> String {
    read(&format!("vtank/{relative}"))
}

/// The files under `Fixtures/<folder>` with extension `extension`, as
/// paths relative to `Fixtures`, sorted.
#[must_use]
pub fn list(folder: &str, extension: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root().join(folder)];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                let relative = path
                    .strip_prefix(root())
                    .expect("under the fixture root")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push(relative);
            }
        }
    }
    found.sort();
    found
}
