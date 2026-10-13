//! The settings document a new profile starts from, built from the option
//! catalogue and its details (see [`crate::options`]) in the settings
//! profile format, so a profile made here is one any reader of the format
//! can open. Nothing of it is shipped as a file.

use std::collections::HashMap;

use mosstank_dotnet::{double_to_int, round_away_from_zero};
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};
use openac_plugin_compat::text::{casing, ordinal};

use crate::model::RechargeHandlerRow;
use crate::monster::MonsterRule;
use crate::monster_table;
use crate::options::{self, VtankSettingValueType};

/// One labelled value of a choice setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtankEnumValue {
    /// The stored number.
    pub value: i32,
    /// Its label.
    pub label: String,
}

/// A fresh copy of the default document, for the caller to change.
#[must_use]
pub fn create() -> VtankDatabase {
    let mut database = VtankDatabase::default();
    add_table(&mut database, "AntiExtraBuffSpells", &["ExemplarId"], None);
    add_table(&mut database, "AssistItems", &["Object", "Type"], None);
    add_table(&mut database, "BuffedItems", &["Object", "Spell"], None);
    add_table(&mut database, "ExtraBuffSpells", &["ExemplarId"], None);
    let gems = add_table(&mut database, "GemFoodItems", &["Name", "Spell"], None);
    for (name, spell) in options::DEFAULT_GEM_FOOD_ITEMS {
        add_row(
            gems,
            vec![VtankCell::string(name), VtankCell::int(spell.cast_signed())],
        );
    }
    add_table(
        &mut database,
        "ItemUseSpecifiers",
        &["Object", "Uses"],
        Some(0),
    );
    add_table(
        &mut database,
        monster_table::TABLE_NAME,
        &monster_table::COLUMNS,
        Some(0),
    );
    monster_table::write(&mut database, &[MonsterRule::authentic_default()])
        .expect("the fresh table has only int cells it wrote");

    let mut settings_rows = Vec::new();
    let mut category_rows = Vec::new();
    for name in options::NAMES.iter().copied() {
        let kind = options::declared_type(name);
        settings_rows.push(VtankRow {
            cells: vec![
                VtankCell::string(name),
                default_value(name, kind),
                VtankCell::string(options::description_of(name)),
                VtankCell::int(kind as i32),
            ],
        });
        if let Some(page) = options::page_of(name) {
            category_rows.push(VtankRow {
                cells: vec![VtankCell::string(name), VtankCell::int(page.0)],
            });
        }
    }
    add_table(
        &mut database,
        "Settings",
        &["Setting", "Value", "Description", "SettingType"],
        Some(0),
    )
    .rows = settings_rows;
    add_table(
        &mut database,
        "SettingsCategories",
        &["Setting", "Categories"],
        Some(0),
    )
    .rows = category_rows;

    let choices = add_table(
        &mut database,
        "SettingsEnumInfo",
        &["Setting", "Value", "EnumValue"],
        None,
    );
    for (setting, value, label) in options::CHOICE_LABELS {
        add_row(
            choices,
            vec![
                VtankCell::string(setting),
                VtankCell::int(value),
                VtankCell::string(label),
            ],
        );
    }
    database
}

/// The recharge plan a new profile starts with.
#[must_use]
pub fn default_recharge_handler_rows() -> Vec<RechargeHandlerRow> {
    options::default_recharge_handlers()
}

/// Each setting's page bits, by upper-cased name (any case matches).
#[must_use]
pub fn setting_category_bitmasks() -> HashMap<String, i32> {
    options::PAGES
        .iter()
        .map(|(name, page)| (casing::to_upper_invariant(name), page.0))
        .collect()
}

/// The labelled values of a choice setting, in order (empty for any other).
#[must_use]
pub fn setting_enum_values(name: &str) -> Vec<VtankEnumValue> {
    options::CHOICE_LABELS
        .iter()
        .filter(|(setting, _, _)| ordinal::equals_ignore_case(setting, name))
        .map(|&(_, value, label)| VtankEnumValue {
            value,
            label: label.to_owned(),
        })
        .collect()
}

fn default_value(name: &str, kind: VtankSettingValueType) -> VtankCell {
    let value = options::default(name);
    match kind {
        VtankSettingValueType::Bool => VtankCell::bool(value.boolean),
        VtankSettingValueType::Int | VtankSettingValueType::Enum => {
            VtankCell::int(double_to_int(round_away_from_zero(value.number)))
        }
        #[allow(clippy::cast_possible_truncation)]
        VtankSettingValueType::Single => VtankCell::float(value.number as f32),
        VtankSettingValueType::String => VtankCell::string(&value.text),
        VtankSettingValueType::Custom => VtankCell::nested_table(recharge_handler_table()),
        VtankSettingValueType::Double => VtankCell::double(value.number),
    }
}

/// The one table-valued setting: the recharge plan, one row per step.
fn recharge_handler_table() -> VtankTable {
    let mut table = VtankTable {
        column_names: [
            "Vital",
            "HandlerString",
            "MinPercent",
            "MaxPercent",
            "Stance",
        ]
        .iter()
        .map(|&c| c.to_owned())
        .collect(),
        index_flags: vec![false; 5],
        rows: Vec::new(),
    };
    for row in options::default_recharge_handlers() {
        add_row(
            &mut table,
            vec![
                VtankCell::int(row.vital),
                VtankCell::string(&row.handler_string),
                VtankCell::int(row.min_percent),
                VtankCell::int(row.max_percent),
                VtankCell::int(row.stance),
            ],
        );
    }
    table
}

fn add_table<'a>(
    database: &'a mut VtankDatabase,
    name: &str,
    columns: &[&str],
    index_column: Option<usize>,
) -> &'a mut VtankTable {
    let table = VtankTable {
        column_names: columns.iter().map(|&c| c.to_owned()).collect(),
        index_flags: (0..columns.len())
            .map(|column| Some(column) == index_column)
            .collect(),
        rows: Vec::new(),
    };
    database.tables.push((name.to_owned(), table));
    &mut database.tables.last_mut().expect("just pushed").1
}

fn add_row(table: &mut VtankTable, cells: Vec<VtankCell>) {
    table.rows.push(VtankRow { cells });
}
