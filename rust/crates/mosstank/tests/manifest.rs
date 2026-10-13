//! The plugin's `plugin.json` (beside the workspace, packed with the
//! `.wasm`): read as the file itself, never a copy that could drift.

use openac_plugin_compat::json::{self, JsonValue};

fn manifest() -> JsonValue {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin.json");
    json::parse(&std::fs::read_to_string(path).expect("plugin.json beside the workspace"))
        .expect("a manifest that parses")
}

fn text(manifest: &JsonValue, name: &str) -> String {
    manifest
        .get(name)
        .and_then(JsonValue::as_str)
        .unwrap_or_else(|| panic!("{name}"))
        .to_owned()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankManifestTests.cs :: TheBuiltPluginFolderCarriesItsManifestAndEntryAssembly
/// The entry is the module this crate builds (`[lib]` of `mosstank`).
#[test]
fn the_manifest_names_the_module_this_crate_builds() {
    assert_eq!(text(&manifest(), "entry"), "mosstank.wasm");
    assert_eq!(env!("CARGO_CRATE_NAME"), "manifest");
    assert_eq!(env!("CARGO_PKG_NAME"), "mosstank");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankManifestTests.cs :: TheBuiltManifestDeclaresWhatAnInstallRequires
/// The WebAssembly manifest has no `kinds` or `hosts`: every plugin runs
/// headless and graphical alike.
#[test]
fn the_manifest_declares_what_an_install_requires() {
    let manifest = manifest();
    assert_eq!(text(&manifest, "id"), "acdream.mosstank");
    assert_eq!(text(&manifest, "displayName"), "MossTank");
    assert!(!text(&manifest, "minHostVersion").trim().is_empty());
    let api = ac_plugin_abi::ApiVersion::parse(&text(&manifest, "apiVersion")).unwrap();
    assert!(ac_plugin_abi::is_supported(api));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankManifestTests.cs :: TheManifestVersionMatchesTheBuiltAssemblyVersion
#[test]
fn the_manifest_version_matches_the_crate_version() {
    assert_eq!(text(&manifest(), "version"), env!("CARGO_PKG_VERSION"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankManifestTests.cs :: TheWindowTitleCarriesTheManifestVersion
#[test]
fn the_window_title_carries_the_manifest_version() {
    assert_eq!(text(&manifest(), "version"), mosstank::VERSION);
    let main = mosstank::panel::WINDOWS[0].markup;
    assert!(!main.contains("title=\"MossTank"));
}
