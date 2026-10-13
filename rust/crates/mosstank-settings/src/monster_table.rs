//! The settings profile's `MyMonsters` table: one row per monster rule,
//! 21 columns, read into [`MonsterRule`]s and written back in place so an
//! untouched profile renders byte for byte as it was read.

use mosstank_data::damage::MonsterDamageType;
use mosstank_data::damage::vtank_element::{from_monster_damage_type, to_monster_damage_type};
use mosstank_formats::FormatError;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};

use crate::monster::{MonsterActionFlags, MonsterRule, MonsterRuleActions, secondary_equip};

/// The table's name.
pub const TABLE_NAME: &str = "MyMonsters";
/// How the fallback row is named in the file.
pub const DEFAULT_ROW_NAME: &str = "<DEFAULT>";
const COLUMN_COUNT: usize = 21;

/// The columns, in order.
pub const COLUMNS: [&str; COLUMN_COUNT] = [
    "MonsterName",
    "AttackPriority",
    "DamageType",
    "WeaponToUse",
    "Imperil",
    "Vuln",
    "Yield",
    "GravityW",
    "Attack",
    "Ring",
    "Broadside",
    "Fester",
    "WeakeningCurse",
    "FesteringCurse",
    "Corruption",
    "DestructiveCurse",
    "Corrosion",
    "Streak",
    "SecondaryVuln",
    "SecondaryEquip",
    "PetDamageType",
];

const BOOL_COLUMNS: [(usize, MonsterActionFlags); 13] = [
    (4, MonsterActionFlags::IMPERIL),
    (5, MonsterActionFlags::VULNERABILITY),
    (6, MonsterActionFlags::YIELD),
    (7, MonsterActionFlags::GRAVITY_WELL),
    (9, MonsterActionFlags::RING),
    (10, MonsterActionFlags::BROADSIDE),
    (11, MonsterActionFlags::FESTER),
    (12, MonsterActionFlags::WEAKENING_CURSE),
    (13, MonsterActionFlags::FESTERING_CURSE),
    (14, MonsterActionFlags::CORRUPTION),
    (15, MonsterActionFlags::DESTRUCTIVE_CURSE),
    (16, MonsterActionFlags::CORROSION),
    (17, MonsterActionFlags::STREAK),
];

/// The rules in the table, or `None` when the table is missing, too narrow,
/// or has a short row. A spec that does not parse becomes an ignored row
/// and its warning goes to `warn`.
///
/// # Errors
/// A number column does not hold a number (as `int.Parse` refuses it).
pub fn try_read(
    database: &VtankDatabase,
    mut warn: Option<&mut dyn FnMut(&str)>,
) -> Result<Option<Vec<MonsterRule>>, FormatError> {
    let Some(table) = database.find(TABLE_NAME) else {
        return Ok(None);
    };
    if table.column_names.len() < COLUMN_COUNT {
        return Ok(None);
    }
    let mut rules = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        if row.cells.len() < COLUMN_COUNT {
            return Ok(None);
        }
        let cells = &row.cells;
        let expression = cells[0].as_string();
        let mut flags = MonsterActionFlags::NONE;
        for (column, flag) in BOOL_COLUMNS {
            if cells[column].as_bool() {
                flags |= flag;
            }
        }
        if cells[8].as_bool() {
            flags |= MonsterActionFlags::ATTACK;
        }
        let weapon = cells[3].as_int()?;
        let offhand = cells[19].as_int()?;
        let actions = MonsterRuleActions {
            flags,
            priority: cells[1].as_int()?,
            damage_type: to_monster_damage_type(cells[2].as_int()?, MonsterDamageType::Auto),
            extra_vulnerability: to_monster_damage_type(
                cells[18].as_int()?,
                MonsterDamageType::Auto,
            ),
            pet_damage_type: to_monster_damage_type(
                cells[20].as_int()?,
                MonsterDamageType::PlayerAuto,
            ),
            // Zero asks for caster handling and -1 keeps the automatic
            // choice; every other signed spelling is an object id.
            weapon_object_id: if weapon == 0 || weapon == -1 {
                0
            } else {
                weapon.cast_unsigned()
            },
            weapon_to_use_raw: weapon,
            // Only the four values from Auto through None are modes; any
            // other value keeps its object-id bits.
            offhand_object_id: if !(secondary_equip::AUTO..secondary_equip::LISTED_TYPES_END)
                .contains(&offhand)
            {
                offhand.cast_unsigned()
            } else {
                0
            },
            secondary_equip_raw: offhand,
            ..MonsterRuleActions::default()
        };
        let (rule, parse_error) = MonsterRule::compile(expression, actions);
        if let (Some(error), Some(warn)) = (parse_error, warn.as_deref_mut()) {
            warn(&error);
        }
        rules.push(rule);
    }
    Ok(Some(rules))
}

/// Writes the rules into the table, reusing each existing row in place and
/// changing only the cells whose value differs, so untouched rows render as
/// they were read. A file that never had the table keeps none while the
/// only rule is an untouched fresh default row.
///
/// # Errors
/// An existing number cell does not hold a number.
pub fn write(database: &mut VtankDatabase, rules: &[MonsterRule]) -> Result<(), FormatError> {
    if database.find(TABLE_NAME).is_none() {
        if rules.is_empty()
            || (rules.len() == 1
                && rules[0].is_default()
                && *rules[0].actions() == MonsterRuleActions::fresh_row())
        {
            return Ok(());
        }
        let table = VtankTable {
            column_names: COLUMNS.iter().map(|&c| c.to_owned()).collect(),
            index_flags: (0..COLUMN_COUNT).map(|column| column == 0).collect(),
            rows: Vec::new(),
        };
        database.tables.push((TABLE_NAME.to_owned(), table));
    }
    let table = database
        .find_mut(TABLE_NAME)
        .expect("the table was found or added");

    let mut existing = std::mem::take(&mut table.rows).into_iter();
    for rule in rules {
        let mut row = match existing.next() {
            Some(row) if row.cells.len() >= COLUMN_COUNT => row,
            _ => new_row(),
        };
        let actions = rule.actions();
        set_string(
            &mut row,
            0,
            if rule.is_default() {
                DEFAULT_ROW_NAME
            } else {
                rule.expression()
            },
        );
        set_int(&mut row, 1, actions.priority)?;
        set_int(&mut row, 2, from_monster_damage_type(actions.damage_type))?;
        let weapon = if actions.weapon_object_id != 0 {
            actions.weapon_object_id.cast_signed()
        } else if actions.weapon_to_use_raw > 0 {
            -1
        } else {
            actions.weapon_to_use_raw
        };
        set_int(&mut row, 3, weapon)?;
        for (column, flag) in BOOL_COLUMNS {
            set_bool(&mut row, column, actions.flags.intersects(flag));
        }
        set_bool(
            &mut row,
            8,
            actions.flags.intersects(MonsterActionFlags::ATTACK),
        );
        set_int(
            &mut row,
            18,
            from_monster_damage_type(actions.extra_vulnerability),
        )?;
        // The row stores the raw mode, so AutoShield, AutoWeapon and None
        // survive a save; only a stale object id falls back to Auto.
        let offhand = if actions.offhand_object_id != 0 {
            actions.offhand_object_id.cast_signed()
        } else if actions.secondary_equip_raw >= secondary_equip::LISTED_TYPES_END {
            secondary_equip::AUTO
        } else {
            actions.secondary_equip_raw
        };
        set_int(&mut row, 19, offhand)?;
        set_int(
            &mut row,
            20,
            from_monster_damage_type(actions.pet_damage_type),
        )?;
        table.rows.push(row);
    }
    Ok(())
}

fn new_row() -> VtankRow {
    VtankRow {
        cells: (0..COLUMN_COUNT).map(|_| VtankCell::int(0)).collect(),
    }
}

fn set_string(row: &mut VtankRow, column: usize, value: &str) {
    let cell = &mut row.cells[column];
    if cell.tag != "s" || cell.as_string() != value {
        *cell = VtankCell::string(value);
    }
}

fn set_int(row: &mut VtankRow, column: usize, value: i32) -> Result<(), FormatError> {
    let cell = &mut row.cells[column];
    if cell.tag != "i" || cell.as_int()? != value {
        *cell = VtankCell::int(value);
    }
    Ok(())
}

fn set_bool(row: &mut VtankRow, column: usize, value: bool) {
    let cell = &mut row.cells[column];
    if cell.tag != "b" || cell.as_bool() != value {
        *cell = VtankCell::bool(value);
    }
}
