//! VTank's game-information database (`gameinfodb.ugd`): monster species
//! and damage, ammunition, heal kits, grenades, drain and martyr spells,
//! crafting recipes. A settings-database text (see [`crate::usd`]).

use std::collections::HashMap;

use mosstank_data::damage::{MonsterDamageType, parse_damage_elements};
use mosstank_dotnet::compat::number::{NumberStyles, parse_f32, parse_f64};
use mosstank_dotnet::{is_white_space, to_upper_invariant, try_parse_int, try_parse_uint};

use crate::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};

/// The database's file name in the profile folder.
pub const FILE_NAME: &str = "gameinfodb.ugd";

/// The version the built-in database carries, and the only one a download
/// may have.
pub const BUILT_IN_VERSION: i32 = 9;

/// The bit that says a monster cannot be affected by magic.
pub const IMMUNE_TO_MAGIC_MASK: i32 = 2;

/// Where profile text is read from (the plugin's profile storage).
pub trait ProfileText {
    /// Whether the storage can be used at all.
    fn is_available(&self) -> bool {
        true
    }
    /// The text stored under `key`, if any.
    fn read_text(&self, key: &str) -> Option<String>;
}

/// Every table a game database has: its name, its columns and which column
/// it is looked up by (-1: none). The built-in database has exactly these,
/// empty.
pub const TABLES: [(&str, &[&str], i32); 14] = [
    (
        "AmmunitionOptions",
        &[
            "AmmoName",
            "LauncherType",
            "WieldReq",
            "Element",
            "Quality",
            "Special",
            "WieldReq2Skill",
            "WieldReq2Value",
        ],
        0,
    ),
    ("CooldownIDs", &["Itemname", "CooldownID"], 0),
    (
        "CraftInteractions",
        &[
            "UseItem1",
            "UseItem2",
            "ResultItem",
            "ResultCount",
            "SuccessMsg",
            "FailMsg",
            "ReqSkill",
            "ReqDiff",
            "ID",
        ],
        8,
    ),
    ("DBLastUpdateTime", &["Zero", "Time"], -1),
    ("DBVersion", &["VersionInt"], -1),
    (
        "DrainSpellOptions",
        &[
            "SpellID",
            "CastTimeMilliseconds",
            "EnemyDrainFactor",
            "EnemyDrainMaximumPoints",
            "ResultMultiplier",
        ],
        0,
    ),
    (
        "GrenadeOptions",
        &[
            "GrenName",
            "WieldReqType",
            "WieldReqAttribute",
            "WieldReqValue",
            "Spell",
            "Spellcraft",
        ],
        0,
    ),
    (
        "HealKits",
        &["KitName", "RestoreBonus", "SkillBonus", "WhichVital"],
        0,
    ),
    (
        "MartyrSpellOptions",
        &[
            "SpellID",
            "CastTimeMilliseconds",
            "SelfDrainFactor",
            "ResultMultiplier",
        ],
        0,
    ),
    ("MonsterDamageOverrides", &["Monster", "DamageString"], 0),
    ("MonsterImmunities", &["Monster", "ImmunityMask"], 0),
    ("SpeciesDamages", &["Species", "DamageString"], 0),
    (
        "SpeciesMembers",
        &["Monster", "Species", "MaximumHealth"],
        0,
    ),
    (
        "SpellQualityOverrides",
        &["SpellID", "Valid", "NewQuality", "NewFamily"],
        0,
    ),
];

/// The database a session has before anything is downloaded: every table,
/// empty, at the built-in version and with no update time, so the first
/// check asks for everything. Built here rather than shipped as a file.
#[must_use]
pub fn built_in() -> VtankDatabase {
    let mut database = VtankDatabase::default();
    for (name, columns, index_column) in TABLES {
        let table = VtankTable {
            column_names: columns.iter().map(|c| (*c).to_owned()).collect(),
            index_flags: (0..columns.len())
                .map(|column| i32::try_from(column).ok() == Some(index_column))
                .collect(),
            rows: Vec::new(),
        };
        database.tables.push((name.to_owned(), table));
    }
    add_row(
        &mut database,
        "DBLastUpdateTime",
        vec![VtankCell::int(0), VtankCell::int(0)],
    );
    add_row(
        &mut database,
        "DBVersion",
        vec![VtankCell::int(BUILT_IN_VERSION)],
    );
    database
}

fn add_row(database: &mut VtankDatabase, table: &str, cells: Vec<VtankCell>) {
    if let Some(table) = database.find_mut(table) {
        table.rows.push(VtankRow { cells });
    }
}

/// The profile folder's database when it reads and carries the built-in
/// database's version; the built-in one otherwise.
#[must_use]
pub fn load_base(profiles: &dyn ProfileText) -> VtankDatabase {
    let built_in = built_in();
    let Some(text) = profiles.read_text(FILE_NAME).filter(|t| !is_white_space(t)) else {
        return built_in;
    };
    match read(&text) {
        Some(file) if version(&file) == version(&built_in) => file,
        _ => built_in,
    }
}

/// The text read the way the reference client reads it, or `None` when its
/// layout does not read at all. A value that does not read as its kind
/// ends its table at the row before it, and the tables after it are lost.
#[must_use]
pub fn read(text: &str) -> Option<VtankDatabase> {
    let mut database = VtankDatabase::parse(text).ok()?;
    end_at_first_unreadable_value(&mut database);
    Some(database)
}

/// Whether every value in every table reads as the kind its tag says.
#[must_use]
pub fn every_value_reads(database: &VtankDatabase) -> bool {
    database
        .tables
        .iter()
        .all(|(_, table)| first_unreadable_row(table).is_none())
}

/// The `DBVersion` row's number; 1 when there is none to read.
#[must_use]
pub fn version(database: &VtankDatabase) -> i32 {
    database
        .find("DBVersion")
        .and_then(|table| table.rows.first())
        .and_then(|row| row.cells.first())
        .and_then(|cell| cell.scalar_text.as_deref())
        .and_then(try_parse_int)
        .unwrap_or(1)
}

fn end_at_first_unreadable_value(database: &mut VtankDatabase) {
    for index in 0..database.tables.len() {
        if let Some(row) = first_unreadable_row(&database.tables[index].1) {
            database.tables[index].1.rows.truncate(row);
            database.tables.truncate(index + 1);
            return;
        }
    }
}

fn first_unreadable_row(table: &VtankTable) -> Option<usize> {
    table
        .rows
        .iter()
        .position(|row| !row.cells.iter().all(reads))
}

/// A value reads when its text is what its tag promises, by the rules the
/// reference client converts it with.
fn reads(cell: &VtankCell) -> bool {
    let text = cell.scalar_text.as_deref().unwrap_or("");
    let float_styles = NumberStyles::FLOAT | NumberStyles::ALLOW_THOUSANDS;
    match cell.tag.as_str() {
        "d" => parse_f64(text, float_styles).is_some(),
        "f" => parse_f32(text, float_styles).is_some(),
        "i" => try_parse_int(text).is_some(),
        "u" => try_parse_uint(text).is_some(),
        "b" => mosstank_dotnet::parse_bool(text).is_ok(),
        _ => true,
    }
}

/// One `SpeciesMembers` row: a monster's species and maximum health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VtankSpeciesMember {
    /// The species.
    pub species: i32,
    /// The maximum health.
    pub maximum_health: i32,
}

/// One `HealKits` row.
#[derive(Debug, Clone, PartialEq)]
pub struct VtankHealKit {
    /// The kit's name.
    pub name: String,
    /// Its restore bonus.
    pub restore_bonus: f64,
    /// Its skill bonus.
    pub skill_bonus: i32,
    /// The vital it heals.
    pub vital: i32,
}

/// One `GrenadeOptions` row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct VtankGrenadeOption {
    pub name: String,
    pub wield_requirement_type: i32,
    pub wield_requirement_attribute: i32,
    pub wield_requirement_value: i32,
    pub spell_id: u32,
    pub spellcraft: i32,
}

/// One `DrainSpellOptions` row.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct VtankDrainSpellOption {
    pub spell_id: u32,
    pub cast_time_milliseconds: i32,
    pub enemy_drain_factor: f64,
    pub enemy_drain_maximum_points: i32,
    pub result_multiplier: f64,
}

/// One `MartyrSpellOptions` row.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct VtankMartyrSpellOption {
    pub spell_id: u32,
    pub cast_time_milliseconds: i32,
    pub self_drain_factor: f64,
    pub result_multiplier: f64,
}

/// One `AmmunitionOptions` row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct VtankAmmunitionOption {
    pub name: String,
    pub launcher_type: i32,
    pub wield_requirement: i32,
    pub element: i32,
    pub quality: i32,
    pub special_mask: i32,
    pub secondary_skill: u32,
    pub secondary_requirement: i32,
}

/// One `CraftInteractions` row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct VtankCraftRecipe {
    pub first_item: String,
    pub second_item: String,
    pub result_item: String,
    pub result_count: i32,
    pub required_skill: u32,
    pub difficulty: i32,
    pub id: i32,
}

/// The `CraftInteractions` table: what two items make when one is used on
/// the other. A database without the table has no recipes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VtankCraftDatabase {
    recipes: Vec<VtankCraftRecipe>,
    by_result: HashMap<String, Vec<VtankCraftRecipe>>,
}

impl VtankCraftDatabase {
    /// A recipe book.
    #[must_use]
    pub fn new(recipes: Vec<VtankCraftRecipe>) -> Self {
        let mut by_result: HashMap<String, Vec<VtankCraftRecipe>> = HashMap::new();
        for recipe in &recipes {
            by_result
                .entry(to_upper_invariant(&recipe.result_item))
                .or_default()
                .push(recipe.clone());
        }
        for list in by_result.values_mut() {
            // OrderBy is stable: equal ids keep the table's order.
            list.sort_by_key(|recipe| recipe.id);
        }
        Self { recipes, by_result }
    }

    /// Every recipe, in the table's own order.
    #[must_use]
    pub fn recipes(&self) -> &[VtankCraftRecipe] {
        &self.recipes
    }

    /// The recipes that make `result_name` (any case), by id.
    #[must_use]
    pub fn for_result(&self, result_name: &str) -> &[VtankCraftRecipe] {
        if is_white_space(result_name) {
            return &[];
        }
        self.by_result
            .get(&to_upper_invariant(result_name.trim()))
            .map_or(&[], Vec::as_slice)
    }
}

/// The game database as the plugin uses it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankGameInfoDatabase {
    /// Whether there is a database at all.
    pub is_loaded: bool,
    /// Damage preferences by monster name (keys upper-cased: any case).
    monster_damage_overrides: HashMap<String, Vec<MonsterDamageType>>,
    species_members: HashMap<String, VtankSpeciesMember>,
    monster_immunities: HashMap<String, i32>,
    /// Damage preferences by species.
    pub species_damages: HashMap<i32, Vec<MonsterDamageType>>,
    /// The ammunition table.
    pub ammunition_options: Vec<VtankAmmunitionOption>,
    /// The heal kits.
    pub heal_kits: Vec<VtankHealKit>,
    /// The grenades.
    pub grenade_options: Vec<VtankGrenadeOption>,
    /// The drain spells.
    pub drain_spell_options: Vec<VtankDrainSpellOption>,
    /// The martyr spells.
    pub martyr_spell_options: Vec<VtankMartyrSpellOption>,
    /// The recipes.
    pub crafts: VtankCraftDatabase,
    /// The database's own time (seconds since 1970), if it says.
    pub last_update_time: Option<i32>,
    /// The database's own version, if it says.
    pub version: Option<i32>,
}

fn key(name: &str) -> String {
    to_upper_invariant(name)
}

impl VtankGameInfoDatabase {
    /// No database: every lookup answers as with an unloaded one.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The profile folder's database, read as [`load_base`] reads it.
    #[must_use]
    pub fn load(storage: &dyn ProfileText) -> Self {
        if !storage.is_available() {
            return Self::empty();
        }
        Self::from_database(&load_base(storage))
    }

    /// The empty built-in database.
    #[must_use]
    pub fn load_default() -> Self {
        Self::from_database(&built_in())
    }

    /// A whole database text.
    ///
    /// # Errors
    /// The layout does not read.
    pub fn parse(text: &str) -> Result<Self, crate::FormatError> {
        Ok(Self::from_database(&VtankDatabase::parse(text)?))
    }

    /// The database's tables as the plugin uses them. A row with too few
    /// columns, or a value that is not the number it should be, is passed
    /// over on its own.
    #[must_use]
    pub fn from_database(database: &VtankDatabase) -> Self {
        let mut result = Self {
            is_loaded: true,
            last_update_time: read_cell_int(database, "DBLastUpdateTime", 1),
            version: read_cell_int(database, "DBVersion", 0),
            ..Self::default()
        };
        for (name, elements) in read_rows(database, "MonsterDamageOverrides", 2, |c| {
            Some((
                c[0].as_string().to_owned(),
                parse_damage_elements(c[1].as_string()),
            ))
        }) {
            result.monster_damage_overrides.insert(key(&name), elements);
        }
        for (name, member) in read_rows(database, "SpeciesMembers", 3, |c| {
            Some((
                c[0].as_string().to_owned(),
                VtankSpeciesMember {
                    species: c[1].as_int().ok()?,
                    maximum_health: c[2].as_int().ok()?,
                },
            ))
        }) {
            result.species_members.insert(key(&name), member);
        }
        for (name, mask) in read_rows(database, "MonsterImmunities", 2, |c| {
            Some((c[0].as_string().to_owned(), c[1].as_int().ok()?))
        }) {
            result.monster_immunities.insert(key(&name), mask);
        }
        for (species, elements) in read_rows(database, "SpeciesDamages", 2, |c| {
            Some((c[0].as_int().ok()?, parse_damage_elements(c[1].as_string())))
        }) {
            result.species_damages.insert(species, elements);
        }
        result.ammunition_options = read_rows(database, "AmmunitionOptions", 8, |c| {
            Some(VtankAmmunitionOption {
                name: c[0].as_string().to_owned(),
                launcher_type: c[1].as_int().ok()?,
                wield_requirement: c[2].as_int().ok()?,
                element: c[3].as_int().ok()?,
                quality: c[4].as_int().ok()?,
                special_mask: c[5].as_int().ok()?,
                secondary_skill: c[6].as_int().ok()?.cast_unsigned(),
                secondary_requirement: c[7].as_int().ok()?,
            })
        });
        result.heal_kits = read_rows(database, "HealKits", 4, |c| {
            Some(VtankHealKit {
                name: c[0].as_string().to_owned(),
                restore_bonus: c[1].as_double().ok()?,
                skill_bonus: c[2].as_int().ok()?,
                vital: c[3].as_int().ok()?,
            })
        });
        result.grenade_options = read_rows(database, "GrenadeOptions", 6, |c| {
            Some(VtankGrenadeOption {
                name: c[0].as_string().to_owned(),
                wield_requirement_type: c[1].as_int().ok()?,
                wield_requirement_attribute: c[2].as_int().ok()?,
                wield_requirement_value: c[3].as_int().ok()?,
                spell_id: c[4].as_int().ok()?.cast_unsigned(),
                spellcraft: c[5].as_int().ok()?,
            })
        });
        result.drain_spell_options = read_rows(database, "DrainSpellOptions", 5, |c| {
            Some(VtankDrainSpellOption {
                spell_id: c[0].as_int().ok()?.cast_unsigned(),
                cast_time_milliseconds: c[1].as_int().ok()?,
                enemy_drain_factor: c[2].as_double().ok()?,
                enemy_drain_maximum_points: c[3].as_int().ok()?,
                result_multiplier: c[4].as_double().ok()?,
            })
        });
        result.martyr_spell_options = read_rows(database, "MartyrSpellOptions", 4, |c| {
            Some(VtankMartyrSpellOption {
                spell_id: c[0].as_int().ok()?.cast_unsigned(),
                cast_time_milliseconds: c[1].as_int().ok()?,
                self_drain_factor: c[2].as_double().ok()?,
                result_multiplier: c[3].as_double().ok()?,
            })
        });
        let recipes = read_rows(database, "CraftInteractions", 9, |c| {
            Some(VtankCraftRecipe {
                first_item: c[0].as_string().to_owned(),
                second_item: c[1].as_string().to_owned(),
                result_item: c[2].as_string().to_owned(),
                result_count: c[3].as_int().ok()?,
                required_skill: c[6].as_int().ok()?.cast_unsigned(),
                difficulty: c[7].as_int().ok()?,
                id: c[8].as_int().ok()?,
            })
        });
        result.crafts = VtankCraftDatabase::new(recipes);
        result
    }

    /// How many monsters the species table lists.
    #[must_use]
    pub fn species_member_count(&self) -> usize {
        self.species_members.len()
    }

    fn member(&self, monster_name: &str) -> Option<VtankSpeciesMember> {
        if !self.is_loaded || is_white_space(monster_name) {
            return None;
        }
        self.species_members.get(&key(monster_name)).copied()
    }

    /// The monster's species, or -1 when there is no database or no row.
    #[must_use]
    pub fn species_of(&self, monster_name: &str) -> i32 {
        self.member(monster_name).map_or(-1, |m| m.species)
    }

    /// The monster's maximum health, or -1. This is the only source a
    /// monster's maximum health has: the client is never told it.
    #[must_use]
    pub fn maximum_health_of(&self, monster_name: &str) -> i32 {
        self.member(monster_name).map_or(-1, |m| m.maximum_health)
    }

    /// Whether the species table has a row for this monster at all (its
    /// maximum-health column may itself hold -1, "known monster, unknown
    /// ceiling").
    #[must_use]
    pub fn lists_monster(&self, monster_name: &str) -> bool {
        self.member(monster_name).is_some()
    }

    /// Whether the monster is listed as unaffectable by magic.
    #[must_use]
    pub fn is_immune_to_magic(&self, monster_name: &str) -> bool {
        self.is_loaded
            && !is_white_space(monster_name)
            && self
                .monster_immunities
                .get(&key(monster_name))
                .is_some_and(|mask| mask & IMMUNE_TO_MAGIC_MASK != 0)
    }

    /// The monster's damage preferences: its own row, else its species', else
    /// none.
    #[must_use]
    pub fn damage_preferences(&self, monster_name: &str) -> &[MonsterDamageType] {
        if !self.is_loaded || is_white_space(monster_name) {
            return &[];
        }
        if let Some(exact) = self.monster_damage_overrides.get(&key(monster_name)) {
            return exact;
        }
        self.species_members
            .get(&key(monster_name))
            .and_then(|member| self.species_damages.get(&member.species))
            .map_or(&[], Vec::as_slice)
    }
}

fn read_cell_int(database: &VtankDatabase, table: &str, column: usize) -> Option<i32> {
    database
        .find(table)?
        .rows
        .first()?
        .cells
        .get(column)?
        .scalar_text
        .as_deref()
        .and_then(try_parse_int)
}

fn read_rows<T>(
    database: &VtankDatabase,
    table: &str,
    columns: usize,
    read: impl Fn(&[VtankCell]) -> Option<T>,
) -> Vec<T> {
    let Some(table) = database.find(table) else {
        return Vec::new();
    };
    table
        .rows
        .iter()
        .filter(|row| row.cells.len() >= columns)
        .filter_map(|row| read(&row.cells))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Storage(String);

    impl ProfileText for Storage {
        fn read_text(&self, key: &str) -> Option<String> {
            (key == FILE_NAME).then(|| self.0.clone())
        }
    }

    fn excerpt() -> VtankDatabase {
        VtankDatabase::parse(&crate::fixtures::read_vtank("gameinfodb-excerpt.ugd")).unwrap()
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoFileTests.cs :: AFileOfAnotherVersionLoadsTheBuiltInDatabase
    #[test]
    fn a_file_of_another_version_loads_the_built_in_database() {
        let mut file = excerpt();
        file.find_mut("DBVersion").unwrap().rows[0].cells[0] = VtankCell::int(4);
        let database = VtankGameInfoDatabase::load(&Storage(file.render()));
        assert!(database.is_loaded);
        assert_eq!(database.version, Some(9));
        assert!(database.ammunition_options.is_empty());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoFileTests.cs :: AnUnreadableValueEndsItsTableAndTheTablesAfterIt
    #[test]
    fn an_unreadable_value_ends_its_table_and_the_tables_after_it() {
        let mut file = excerpt();
        file.find_mut("HealKits").unwrap().rows[1].cells[2] = VtankCell {
            scalar_text: Some("lots".to_owned()),
            ..VtankCell::tagged("i")
        };
        let database = VtankGameInfoDatabase::load(&Storage(file.render()));
        assert_eq!(database.heal_kits.len(), 1);
        assert_eq!(database.heal_kits[0].name, "Fixture Healing Kit");
        assert_eq!(database.ammunition_options.len(), 7);
        assert_eq!(database.grenade_options.len(), 3);
        assert!(database.martyr_spell_options.is_empty());
        assert_eq!(database.species_member_count(), 0);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoFileTests.cs :: ARowOfTheWrongKindIsPassedOverAlone
    #[test]
    fn a_row_of_the_wrong_kind_is_passed_over_alone() {
        let mut file = excerpt();
        file.find_mut("GrenadeOptions").unwrap().rows[1].cells[4] = VtankCell::string("a spell");
        let database = VtankGameInfoDatabase::load(&Storage(file.render()));
        let names: Vec<&str> = database
            .grenade_options
            .iter()
            .map(|g| g.name.as_str())
            .collect();
        assert_eq!(names, ["Iron Phial of Imperil", "Fixture Phial of Nothing"]);
        assert_eq!(database.martyr_spell_options.len(), 3);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoFileTests.cs :: TheBuiltInDatabaseStartsAtVersion9WithNoUpdateTime
    #[test]
    fn the_built_in_database_starts_at_version_9_with_no_update_time() {
        let database = VtankGameInfoDatabase::load_default();
        assert_eq!(database.version, Some(9));
        assert_eq!(database.last_update_time, Some(0));
        assert!(database.ammunition_options.is_empty());
    }

    #[test]
    fn the_excerpt_reads_every_table() {
        let database = VtankGameInfoDatabase::from_database(&excerpt());
        assert!(every_value_reads(&excerpt()));
        assert_eq!(database.version, Some(9));
        assert!(!database.ammunition_options.is_empty());
        assert!(!database.heal_kits.is_empty());
    }
}
