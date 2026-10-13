//! VTank's game-information database (`gameinfodb.ugd`): monster species
//! and damage, ammunition, heal kits, grenades, drain and martyr spells,
//! crafting recipes. A settings-database text (see [`crate::usd`]).

use std::collections::HashMap;
use std::rc::Rc;

use mosstank_data::damage::{MonsterDamageType, parse_damage_elements};
use mosstank_dotnet::collections::LookupMap;
use mosstank_dotnet::compat::number::{NumberStyles, parse_f32, parse_f64, parse_i32};
use mosstank_dotnet::text::casing;
use mosstank_dotnet::{try_parse_int, try_parse_uint};

use crate::usd::{
    CellValue, TableSet, VtankCell, VtankDatabase, VtankDatabaseRef, VtankRow, VtankTable,
};

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
    match BaseDatabase::read(read_base_text(profiles).as_deref()) {
        BaseDatabase::File(file) => file.to_database(),
        BaseDatabase::BuiltIn(built_in) => built_in,
    }
}

/// The profile folder's database text, when there is one that is not
/// blank.
#[must_use]
pub fn read_base_text(profiles: &dyn ProfileText) -> Option<String> {
    profiles
        .read_text(FILE_NAME)
        .filter(|t| !casing::is_white_space_only(t))
}

/// The database [`load_base`] reads, the file's read in place.
#[derive(Debug, Clone, PartialEq)]
pub enum BaseDatabase<'a> {
    /// The profile folder's file.
    File(VtankDatabaseRef<'a>),
    /// The built-in database.
    BuiltIn(VtankDatabase),
}

impl<'a> BaseDatabase<'a> {
    /// The database [`load_base`] reads from the folder's text `text`
    /// ([`read_base_text`]).
    #[must_use]
    pub fn read(text: Option<&'a str>) -> Self {
        let built_in = built_in();
        match text.and_then(read_in_place) {
            Some(file) if version(&file) == version(&built_in) => Self::File(file),
            _ => Self::BuiltIn(built_in),
        }
    }

    /// The database as the plugin uses it.
    #[must_use]
    pub fn game_info(&self) -> VtankGameInfoDatabase {
        match self {
            Self::File(file) => VtankGameInfoDatabase::from_database(file),
            Self::BuiltIn(built_in) => VtankGameInfoDatabase::from_database(built_in),
        }
    }

    /// Its [`last_update_time`].
    #[must_use]
    pub fn last_update_time(&self) -> Option<i32> {
        match self {
            Self::File(file) => last_update_time(file),
            Self::BuiltIn(built_in) => last_update_time(built_in),
        }
    }
}

/// The profile folder's database as the session last read it, by its text.
///
/// The session reads the file at start, again when the character's
/// profiles load, and again for the update check at login; the database a
/// text reads as depends on nothing else, so a text read before is not
/// parsed again (the file is some two megabytes).
#[derive(Debug, Default)]
pub struct GameInfoMemo {
    last: Option<(Option<String>, VtankGameInfoDatabase, Option<i32>)>,
}

/// Whether two texts are the same, eight bytes at a time (the file is some
/// two megabytes; the plugin's byte-by-byte comparison costs ten times
/// as many instructions).
fn same_text(a: Option<&str>, b: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return a.is_none() && b.is_none();
    };
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let (mut left, mut right) = (a.chunks_exact(8), b.chunks_exact(8));
    let words = |chunk: &[u8]| u64::from_ne_bytes(chunk.try_into().expect("eight bytes"));
    (&mut left)
        .zip(&mut right)
        .all(|(x, y)| words(x) == words(y))
        && left.remainder() == right.remainder()
}

impl GameInfoMemo {
    /// What [`BaseDatabase::read`] makes of the folder's text `text`
    /// ([`read_base_text`]): the database as the plugin uses it and its
    /// [`last_update_time`].
    pub fn read(&mut self, text: Option<String>) -> (VtankGameInfoDatabase, Option<i32>) {
        if let Some((last, database, time)) = &self.last
            && same_text(last.as_deref(), text.as_deref())
        {
            return (database.clone(), *time);
        }
        let base = BaseDatabase::read(text.as_deref());
        let (database, time) = (base.game_info(), base.last_update_time());
        self.last = Some((text, database.clone(), time));
        (database, time)
    }

    /// [`VtankGameInfoDatabase::load`], through this memo.
    pub fn load(&mut self, storage: &dyn ProfileText) -> VtankGameInfoDatabase {
        if !storage.is_available() {
            return VtankGameInfoDatabase::empty();
        }
        self.read(read_base_text(storage)).0
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

/// [`read`], the text read in place.
#[must_use]
pub fn read_in_place(text: &str) -> Option<VtankDatabaseRef<'_>> {
    let mut database = VtankDatabaseRef::parse(text).ok()?;
    end_at_first_unreadable_value(&mut database);
    Some(database)
}

/// Whether every value in every table reads as the kind its tag says.
#[must_use]
pub fn every_value_reads<D: TableSet>(database: &D) -> bool {
    (0..database.table_count())
        .all(|index| first_unreadable_row(&database.rows_of(index)).is_none())
}

/// The `DBVersion` row's number; 1 when there is none to read.
#[must_use]
pub fn version<D: TableSet>(database: &D) -> i32 {
    database
        .index_of("DBVersion")
        .and_then(|index| database.rows_of(index).first().copied())
        .and_then(|row| row.first())
        .and_then(CellValue::scalar_text)
        .and_then(try_parse_int)
        .unwrap_or(1)
}

/// The `DBLastUpdateTime` row's time, in seconds since 1970, or `None` when
/// it has none.
#[must_use]
pub fn last_update_time<D: TableSet>(database: &D) -> Option<i32> {
    let index = database.index_of("DBLastUpdateTime")?;
    let rows = database.rows_of(index);
    let row = rows.first()?;
    if row.len() < 2 {
        return None;
    }
    parse_i32(row[1].scalar_text().unwrap_or(""), NumberStyles::INTEGER)
}

fn end_at_first_unreadable_value<D: TableSet>(database: &mut D) {
    for index in 0..database.table_count() {
        if let Some(row) = first_unreadable_row(&database.rows_of(index)) {
            database.end_at(index, row);
            return;
        }
    }
}

fn first_unreadable_row<C: CellValue>(rows: &[&[C]]) -> Option<usize> {
    rows.iter().position(|row| !row.iter().all(reads))
}

/// A value reads when its text is what its tag promises, by the rules the
/// reference client converts it with.
fn reads<C: CellValue>(cell: &C) -> bool {
    let text = cell.scalar_text().unwrap_or("");
    let float_styles = NumberStyles::FLOAT | NumberStyles::ALLOW_THOUSANDS;
    match cell.tag() {
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
    /// The book, shared by its copies: it never changes once read.
    book: Rc<CraftBook>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct CraftBook {
    recipes: Vec<VtankCraftRecipe>,
    /// Each recipe's place in `recipes` with the hash of its result's upper
    /// case, ordered by hash, then id, then place: a result's recipes are a
    /// run of one hash, by id (the table's order among equal ids).
    by_result: Vec<(u64, usize)>,
}

/// The hash of `text`'s invariant upper case, built without the text.
fn upper_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut add = |unit: u32| {
        hash = (hash ^ u64::from(unit)).wrapping_mul(0x0000_0100_0000_01b3);
    };
    if text.is_ascii() {
        for byte in text.bytes() {
            add(u32::from(byte.to_ascii_uppercase()));
        }
    } else {
        for c in text.chars() {
            add(u32::from(upper_char(c)));
        }
    }
    hash
}

/// A character's invariant upper case (`to_upper_invariant` of it).
fn upper_char(c: char) -> char {
    mosstank_dotnet::compat::text::casing::to_upper_char(c)
}

/// Whether two texts have the same invariant upper case.
fn same_upper(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a.chars().map(upper_char).eq(b.chars().map(upper_char))
}

impl VtankCraftDatabase {
    /// A recipe book.
    #[must_use]
    pub fn new(recipes: Vec<VtankCraftRecipe>) -> Self {
        let mut by_result: Vec<(u64, usize)> = recipes
            .iter()
            .enumerate()
            .map(|(index, recipe)| (upper_hash(&recipe.result_item), index))
            .collect();
        // OrderBy is stable: equal ids keep the table's order.
        by_result.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0)
                .then(recipes[a.1].id.cmp(&recipes[b.1].id))
                .then(a.1.cmp(&b.1))
        });
        Self {
            book: Rc::new(CraftBook { recipes, by_result }),
        }
    }

    /// Every recipe, in the table's own order.
    #[must_use]
    pub fn recipes(&self) -> &[VtankCraftRecipe] {
        &self.book.recipes
    }

    /// The recipes that make `result_name` (any case), by id.
    #[must_use]
    pub fn for_result(&self, result_name: &str) -> Vec<&VtankCraftRecipe> {
        if casing::is_white_space_only(result_name) {
            return Vec::new();
        }
        let name = result_name.trim();
        let hash = upper_hash(name);
        let by_result = &self.book.by_result;
        let start = by_result.partition_point(|(at, _)| *at < hash);
        by_result[start..]
            .iter()
            .take_while(|(at, _)| *at == hash)
            .map(|&(_, index)| &self.book.recipes[index])
            .filter(|recipe| same_upper(&recipe.result_item, name))
            .collect()
    }
}

/// The game database as the plugin uses it. Every part of the macro that
/// reads it holds a copy; the large tables (the monsters and the recipes)
/// never change once read, so the copies share them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankGameInfoDatabase {
    /// Whether there is a database at all.
    pub is_loaded: bool,
    /// The monster tables, shared by the copies.
    monsters: Rc<MonsterTables>,
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

/// The monster tables, keys upper-cased (any case matches).
#[derive(Debug, Default, PartialEq)]
struct MonsterTables {
    /// Damage preferences by monster name.
    damage_overrides: LookupMap<String, Vec<MonsterDamageType>>,
    species_members: LookupMap<String, VtankSpeciesMember>,
    immunities: LookupMap<String, i32>,
}

fn key(name: &str) -> String {
    casing::to_upper_invariant(name)
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
        BaseDatabase::read(read_base_text(storage).as_deref()).game_info()
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
    pub fn from_database<D: TableSet>(database: &D) -> Self {
        let mut monsters = MonsterTables::default();
        let mut result = Self {
            is_loaded: true,
            last_update_time: read_cell_int(database, "DBLastUpdateTime", 1),
            version: read_cell_int(database, "DBVersion", 0),
            ..Self::default()
        };
        monsters
            .damage_overrides
            .extend(read_rows(database, "MonsterDamageOverrides", 2, |c| {
                Some((
                    key(c[0].as_string()),
                    parse_damage_elements(c[1].as_string()),
                ))
            }));
        monsters
            .species_members
            .extend(read_rows(database, "SpeciesMembers", 3, |c| {
                Some((
                    key(c[0].as_string()),
                    VtankSpeciesMember {
                        species: c[1].as_int().ok()?,
                        maximum_health: c[2].as_int().ok()?,
                    },
                ))
            }));
        monsters
            .immunities
            .extend(read_rows(database, "MonsterImmunities", 2, |c| {
                Some((key(c[0].as_string()), c[1].as_int().ok()?))
            }));
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
        result.monsters = Rc::new(monsters);
        result
    }

    /// The species table's row for a monster (any case), if any.
    #[must_use]
    pub fn species_member(&self, monster_name: &str) -> Option<VtankSpeciesMember> {
        self.monsters
            .species_members
            .get(&key(monster_name))
            .copied()
    }

    /// How many monsters have damage preferences of their own.
    #[must_use]
    pub fn monster_damage_override_count(&self) -> usize {
        self.monsters.damage_overrides.len()
    }

    /// How many monsters the immunity table lists.
    #[must_use]
    pub fn monster_immunity_count(&self) -> usize {
        self.monsters.immunities.len()
    }

    /// How many monsters the species table lists.
    #[must_use]
    pub fn species_member_count(&self) -> usize {
        self.monsters.species_members.len()
    }

    fn member(&self, monster_name: &str) -> Option<VtankSpeciesMember> {
        if !self.is_loaded || casing::is_white_space_only(monster_name) {
            return None;
        }
        self.monsters
            .species_members
            .get(&key(monster_name))
            .copied()
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
            && !casing::is_white_space_only(monster_name)
            && self
                .monsters
                .immunities
                .get(&key(monster_name))
                .is_some_and(|mask| mask & IMMUNE_TO_MAGIC_MASK != 0)
    }

    /// The monster's damage preferences: its own row, else its species', else
    /// none.
    #[must_use]
    pub fn damage_preferences(&self, monster_name: &str) -> &[MonsterDamageType] {
        if !self.is_loaded || casing::is_white_space_only(monster_name) {
            return &[];
        }
        if let Some(exact) = self.monsters.damage_overrides.get(&key(monster_name)) {
            return exact;
        }
        self.monsters
            .species_members
            .get(&key(monster_name))
            .and_then(|member| self.species_damages.get(&member.species))
            .map_or(&[], Vec::as_slice)
    }
}

fn read_cell_int<D: TableSet>(database: &D, table: &str, column: usize) -> Option<i32> {
    let index = database.index_of(table)?;
    database
        .rows_of(index)
        .first()?
        .get(column)?
        .scalar_text()
        .and_then(try_parse_int)
}

fn read_rows<D: TableSet, T>(
    database: &D,
    table: &str,
    columns: usize,
    read: impl Fn(&[D::Cell]) -> Option<T>,
) -> Vec<T> {
    let Some(index) = database.index_of(table) else {
        return Vec::new();
    };
    database
        .rows_of(index)
        .into_iter()
        .filter(|row| row.len() >= columns)
        .filter_map(read)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The recipe index finds what upper-casing every result and sorting
    // by id finds.
    #[test]
    fn recipes_by_result_are_found_as_upper_cased_keys_find_them() {
        let recipe = |result: &str, id: i32, first: &str| VtankCraftRecipe {
            first_item: first.to_owned(),
            second_item: String::new(),
            result_item: result.to_owned(),
            result_count: 1,
            required_skill: 0,
            difficulty: 0,
            id,
        };
        let recipes = vec![
            recipe("Iron Ingot", 5, "a"),
            recipe("IRON INGOT", 2, "b"),
            recipe("iron ingot", 5, "c"),
            recipe(" Iron Ingot", 1, "d"),
            recipe("{131}ron Ingot", 3, "e"),
            recipe("Stra{df}e", 4, "f"),
            recipe("STRA{df}E", 1, "g"),
            recipe("", 9, "h"),
        ];
        let book = VtankCraftDatabase::new(recipes.clone());
        for name in [
            "iron ingot",
            "  IRON INGOT  ",
            " Iron Ingot",
            "stra{df}e",
            "STRASSE",
            "",
            "   ",
            "nothing",
        ] {
            let expected: Vec<&str> = if casing::is_white_space_only(name) {
                Vec::new()
            } else {
                let key = casing::to_upper_invariant(name.trim());
                let mut found: Vec<&VtankCraftRecipe> = recipes
                    .iter()
                    .filter(|r| casing::to_upper_invariant(&r.result_item) == key)
                    .collect();
                found.sort_by_key(|r| r.id);
                found.iter().map(|r| r.first_item.as_str()).collect()
            };
            let actual: Vec<&str> = book
                .for_result(name)
                .iter()
                .map(|r| r.first_item.as_str())
                .collect();
            assert_eq!(actual, expected, "{name:?}");
        }
    }

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
    fn texts_are_the_same_exactly_when_their_bytes_are() {
        let texts = [
            "",
            "a",
            "abcdefgh",
            "abcdefghi",
            "abcdefgx",
            "abcdefghx",
            "xbcdefghi",
        ];
        for a in texts {
            for b in texts {
                assert_eq!(same_text(Some(a), Some(b)), a == b, "{a:?} {b:?}");
            }
            assert!(!same_text(Some(a), None));
        }
        assert!(same_text(None, None));
        // Past one block of thirty-two bytes, a difference anywhere.
        let long = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+-";
        for length in [31, 32, 33, 40, 64] {
            let text = &long[..length];
            assert!(same_text(Some(text), Some(text)));
            for at in 0..length {
                let mut other = text.as_bytes().to_vec();
                other[at] = b'!';
                let other = String::from_utf8(other).unwrap();
                assert!(!same_text(Some(text), Some(&other)), "{length} {at}");
            }
        }
    }

    // The memo reads a text once and answers the same for it again.
    #[test]
    fn the_memo_answers_what_a_fresh_read_answers() {
        let mut memo = GameInfoMemo::default();
        let mut old = excerpt();
        old.find_mut("DBVersion").unwrap().rows[0].cells[0] = VtankCell::int(4);
        for text in [
            Some(excerpt().render()),
            Some(excerpt().render()),
            Some(old.render()),
            None,
            Some(excerpt().render()),
        ] {
            let base = BaseDatabase::read(text.as_deref());
            let expected = (base.game_info(), base.last_update_time());
            assert_eq!(memo.read(text), expected);
        }
        assert_eq!(
            memo.load(&Storage(excerpt().render())),
            VtankGameInfoDatabase::load(&Storage(excerpt().render()))
        );
    }

    // The database read in place is the database read whole, for a file
    // that reads, one whose value does not, and one of another version.
    #[test]
    fn the_database_read_in_place_is_the_database_read_whole() {
        let mut texts = vec![excerpt().render()];
        let mut bad = excerpt();
        bad.find_mut("HealKits").unwrap().rows[1].cells[2] = VtankCell {
            scalar_text: Some("lots".to_owned()),
            ..VtankCell::tagged("i")
        };
        texts.push(bad.render());
        let mut old = excerpt();
        old.find_mut("DBVersion").unwrap().rows[0].cells[0] = VtankCell::int(4);
        texts.push(old.render());
        texts.push(String::new());
        texts.push(
            "2
broken"
                .to_owned(),
        );
        for text in &texts {
            let whole = read(text);
            let in_place = read_in_place(text);
            assert_eq!(in_place.as_ref().map(VtankDatabaseRef::to_database), whole);
            if let (Some(whole), Some(in_place)) = (whole, in_place) {
                assert_eq!(
                    VtankGameInfoDatabase::from_database(&in_place),
                    VtankGameInfoDatabase::from_database(&whole)
                );
                assert_eq!(last_update_time(&in_place), last_update_time(&whole));
                assert_eq!(every_value_reads(&in_place), every_value_reads(&whole));
            }
        }
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
