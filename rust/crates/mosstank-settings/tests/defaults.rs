//! The document a new settings profile starts from, built from the option
//! catalogue and details, against the C# tests' cases.

use std::path::{Path, PathBuf};

use mosstank_settings::model::{AllSettings, GemFoodItem, VitalSettings};
use mosstank_settings::monster::MonsterRuleActions;
use mosstank_settings::options::{self, VtankSettingValueType};
use mosstank_settings::{defaults, monster_table, profile};
use openac_plugin_compat::text::casing;

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDefaultSettingsDatabaseTests.cs :: EverySettingHasARowWithItsCatalogueDefault
#[test]
#[allow(clippy::float_cmp, clippy::cast_possible_truncation)]
fn every_setting_has_a_row_with_its_catalogue_default() {
    let database = defaults::create();
    let settings = database.find("Settings").unwrap();
    let names: Vec<&str> = settings
        .rows
        .iter()
        .map(|row| row.cells[0].as_string())
        .collect();
    assert_eq!(names, *options::NAMES);
    for row in &settings.rows {
        let name = row.cells[0].as_string();
        let kind = options::declared_type(name);
        let expected = options::default(name);
        let value = &row.cells[1];
        match kind {
            VtankSettingValueType::Bool => {
                assert_eq!(
                    (value.tag.as_str(), value.as_bool()),
                    ("b", expected.boolean)
                );
            }
            VtankSettingValueType::Int | VtankSettingValueType::Enum => {
                assert_eq!(
                    (value.tag.as_str(), value.as_int().unwrap()),
                    ("i", expected.number as i32)
                );
            }
            VtankSettingValueType::Double => {
                assert_eq!(
                    (value.tag.as_str(), value.as_double().unwrap()),
                    ("d", expected.number)
                );
            }
            VtankSettingValueType::String => {
                assert_eq!(
                    (value.tag.as_str(), value.as_string()),
                    ("s", expected.text.as_str())
                );
            }
            VtankSettingValueType::Custom => assert_eq!(value.tag, "TABLE"),
            VtankSettingValueType::Single => {}
        }
        assert_eq!(row.cells[3].as_int(), Ok(kind as i32));
        assert_eq!(row.cells[2].as_string(), options::description_of(name));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDefaultSettingsDatabaseTests.cs :: ANewProfileSeedsTheDefaultListsAndRules
#[test]
fn a_new_profile_seeds_the_default_lists_and_rules() {
    let database = defaults::create();
    let mut target = AllSettings {
        vitals: VitalSettings {
            recharge_handler_rows: Vec::new(),
            ..VitalSettings::default()
        },
        ..AllSettings::default()
    };

    profile::apply_seed_database(&database, &mut target).unwrap();

    assert_eq!(
        target.vitals.recharge_handler_rows,
        options::default_recharge_handlers()
    );
    assert_eq!(target.vitals.recharge_handler_rows.len(), 26);
    let gems: Vec<GemFoodItem> = options::DEFAULT_GEM_FOOD_ITEMS
        .iter()
        .map(|&(name, spell_id)| GemFoodItem {
            name: name.to_owned(),
            spell_id,
        })
        .collect();
    assert_eq!(target.buffs.gem_food_items, gems);
    let rules = monster_table::try_read(&database, None).unwrap().unwrap();
    assert_eq!(rules.len(), 1);
    assert!(rules[0].is_default());
    assert_eq!(*rules[0].actions(), MonsterRuleActions::fresh_row());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDefaultSettingsDatabaseTests.cs :: ThePageTableIsTheOneTheOptionsFilterUses
#[test]
fn the_page_table_is_the_one_the_options_filter_uses() {
    let database = defaults::create();
    let pages = database.find("SettingsCategories").unwrap();
    let mut from_table: Vec<(String, i32)> = pages
        .rows
        .iter()
        .map(|row| {
            (
                casing::to_upper_invariant(row.cells[0].as_string()),
                row.cells[1].as_int().unwrap(),
            )
        })
        .collect();
    from_table.sort();
    let mut expected: Vec<(String, i32)> =
        defaults::setting_category_bitmasks().into_iter().collect();
    expected.sort();
    assert_eq!(from_table, expected);
}

fn rust_sources(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDefaultSettingsDatabaseTests.cs :: ThePluginEmbedsNoOtherProgramsFiles
#[test]
fn the_plugin_embeds_no_other_programs_files() {
    // The C# embeds one resource, its own experience-cost table. The Rust
    // embeds that and the two `.af` header texts (string constants in the
    // C#; metaf's credit block, see NOTICE.md) and its own twelve window
    // markups (`mosstank::panel`, files of the C# plugin); nothing else.
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = Vec::new();
    rust_sources(&crates, &mut sources);
    let mut embedded: Vec<String> = Vec::new();
    for source in sources {
        if source.components().any(|c| c.as_os_str() == "xtask") {
            continue;
        }
        let text = std::fs::read_to_string(&source).unwrap();
        for macro_name in ["include_str!(\"", "include_bytes!(\"", "markup!(\""] {
            for (start, _) in text.match_indices(macro_name) {
                let rest = &text[start + macro_name.len()..];
                let path = &rest[..rest.find('"').unwrap()];
                // Skip this test's own spelling of the macros.
                if !path.is_empty() {
                    embedded.push(path.rsplit('/').next().unwrap().to_owned());
                }
            }
        }
    }
    embedded.sort();
    assert_eq!(
        embedded,
        [
            "ExperienceCosts.tsv",
            "meta_header.txt",
            "mosstank-action-history.xml",
            "mosstank-advanced.xml",
            "mosstank-buffpicker.xml",
            "mosstank-loot-editor.xml",
            "mosstank-metaeditor.xml",
            "mosstank-remote.xml",
            "mosstank-ub-clients.xml",
            "mosstank-ub-color.xml",
            "mosstank-ub-dungeon.xml",
            "mosstank-ub-list.xml",
            "mosstank-ub-map.xml",
            "mosstank.xml",
            "nav_header.txt",
        ]
    );
}
