//! The item tables a settings profile carries beside its settings: the
//! profiled weapons and their buffs (`BuffedItems`), which hands an item
//! is used in (`ItemUseSpecifiers`), the consumables by kind (`AssistItems`),
//! the gems and food used for their buff (`GemFoodItems`), and the extra
//! and suppressed buffs (`ExtraBuffSpells`, `AntiExtraBuffSpells`).
//!
//! Each reader first clears what it owns, so loading a profile without the
//! table leaves nothing of the previous profile's behind. Each writer
//! changes only the rows it understands and has to, so unknown columns,
//! rows it cannot read and untouched rows render as they were read.

use std::collections::HashSet;

use mosstank_formats::FormatError;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};

use crate::model::{
    AssistItem, BuffItemEnchantRow, BuffSettings, CombatSettings, ConsumableCategory, GemFoodItem,
};

/// Two cells of a row as ints, or `None` when the row is too short or either
/// does not read (`int.Parse` refuses it).
fn read_two_ints(row: &VtankRow, first: usize, second: usize) -> Option<(i32, i32)> {
    let a = row.cells.get(first)?.as_int().ok()?;
    let b = row.cells.get(second)?.as_int().ok()?;
    Some((a, b))
}

fn new_table(database: &mut VtankDatabase, name: &str, columns: &[&str]) {
    let table = VtankTable {
        column_names: columns.iter().map(|&c| c.to_owned()).collect(),
        index_flags: vec![false; columns.len()],
        rows: Vec::new(),
    };
    database.tables.push((name.to_owned(), table));
}

fn filled_row(table: &VtankTable, fill: i32) -> VtankRow {
    VtankRow {
        cells: (0..table.column_names.len())
            .map(|_| VtankCell::int(fill))
            .collect(),
    }
}

/// The profiled weapons and their buffs: `BuffedItems`, one row per item
/// and spell (an item with no spell has -1 there; an object id of -1 is a
/// spell for every item).
pub mod profiled_item_ids {
    use super::*;

    const TABLE_NAME: &str = "BuffedItems";

    /// Reads the profiled items into the combat settings (in table order,
    /// each once, never the character itself) and their spell rows into the
    /// buff settings.
    pub fn read(
        database: &VtankDatabase,
        settings: &mut CombatSettings,
        buffs: &mut BuffSettings,
        player_object_id: u32,
    ) {
        settings.combat_item_order_ids.clear();
        settings.combat_item_object_ids.clear();
        settings.removed_combat_item_object_ids.clear();
        buffs
            .item_enchant_rows
            .retain(|row| !row.is_profiled_item_row());

        let Some(table) = database.find(TABLE_NAME) else {
            return;
        };
        let (Some(object_column), Some(spell_column)) =
            (table.column_index("Object"), table.column_index("Spell"))
        else {
            return;
        };
        for row in &table.rows {
            let Some((raw_id, raw_spell)) = read_two_ints(row, object_column, spell_column) else {
                continue;
            };
            let casts = raw_spell > 0 || raw_spell == -1;
            if raw_id == -1 {
                if casts {
                    buffs
                        .item_enchant_rows
                        .push(enchant_row(u32::MAX, raw_spell));
                }
                continue;
            }
            let id = raw_id.cast_unsigned();
            if id != player_object_id && settings.combat_item_object_ids.insert(id) {
                settings.combat_item_order_ids.push(id);
            }
            if casts {
                buffs.item_enchant_rows.push(enchant_row(id, raw_spell));
            }
        }
    }

    fn enchant_row(object_id: u32, raw_spell: i32) -> BuffItemEnchantRow {
        BuffItemEnchantRow {
            item_name: String::new(),
            spell_name: String::new(),
            object_id: Some(object_id),
            spell_id: Some(raw_spell.cast_unsigned()),
        }
    }

    fn read_object_id(row: &VtankRow, column: usize) -> Option<u32> {
        Some(row.cells.get(column)?.as_int().ok()?.cast_unsigned())
    }

    /// Removes the rows of the items the settings dropped and adds the
    /// newly profiled items: one row per spell the item is buffed with, or
    /// the item with no spell when it carries none, as VTank writes an item.
    ///
    /// # Errors
    /// The table lacks its `Object` or `Spell` column.
    pub fn write(
        database: &mut VtankDatabase,
        settings: &mut CombatSettings,
        buffs: &BuffSettings,
    ) -> Result<(), FormatError> {
        if database.find(TABLE_NAME).is_none() {
            if settings.combat_item_order_ids.is_empty() {
                return Ok(());
            }
            new_table(database, TABLE_NAME, &["Object", "Spell"]);
        }
        let table = database.find_mut(TABLE_NAME).expect("found or added");
        let (Some(object_column), Some(spell_column)) =
            (table.column_index("Object"), table.column_index("Spell"))
        else {
            return Err(FormatError(
                "'BuffedItems' table is missing Object/Spell columns.".to_owned(),
            ));
        };

        if !settings.removed_combat_item_object_ids.is_empty() {
            let removed = &settings.removed_combat_item_object_ids;
            table.rows.retain(|row| {
                !read_object_id(row, object_column).is_some_and(|id| removed.contains(&id))
            });
            settings.removed_combat_item_object_ids.clear();
        }

        let mut present: HashSet<u32> = table
            .rows
            .iter()
            .filter_map(|row| read_object_id(row, object_column))
            .collect();
        for &id in &settings.combat_item_order_ids {
            if id == 0 || id == u32::MAX || !present.insert(id) {
                continue;
            }
            let mut wrote_spell = false;
            for spell in &buffs.item_enchant_rows {
                if !spell.is_profiled_item_row() || spell.object_id != Some(id) {
                    continue;
                }
                let spell_id = spell.spell_id.expect("a profiled row has a spell");
                add_row(
                    table,
                    object_column,
                    spell_column,
                    id,
                    spell_id.cast_signed(),
                );
                wrote_spell = true;
            }
            if !wrote_spell {
                add_row(table, object_column, spell_column, id, -1);
            }
        }
        Ok(())
    }

    fn add_row(
        table: &mut VtankTable,
        object_column: usize,
        spell_column: usize,
        object_id: u32,
        spell_id: i32,
    ) {
        let mut row = filled_row(table, -1);
        row.cells[object_column] = VtankCell::int(object_id.cast_signed());
        row.cells[spell_column] = VtankCell::int(spell_id);
        table.rows.push(row);
    }
}

/// Which hands an item is used in: `ItemUseSpecifiers`.
pub mod item_use_specifiers {
    use super::*;

    const TABLE_NAME: &str = "ItemUseSpecifiers";

    /// Reads the table into the combat settings (a later row for the same
    /// item wins).
    pub fn read(database: &VtankDatabase, settings: &mut CombatSettings) {
        settings.item_use_specifiers.clear();
        let Some(table) = database.find(TABLE_NAME) else {
            return;
        };
        let (Some(object_column), Some(uses_column)) =
            (table.column_index("Object"), table.column_index("Uses"))
        else {
            return;
        };
        for row in &table.rows {
            if let Some((object, uses)) = read_two_ints(row, object_column, uses_column) {
                settings
                    .item_use_specifiers
                    .insert(object.cast_unsigned(), uses);
            }
        }
    }

    /// The uses the profile gives an item; 3 when it says nothing.
    #[must_use]
    pub fn uses_for(settings: &CombatSettings, object_id: u32) -> i32 {
        settings
            .item_use_specifiers
            .get(&object_id)
            .copied()
            .unwrap_or(BOTH_HANDS)
    }

    /// Never used.
    pub const DO_NOT_USE: i32 = 0;
    /// Used in the right hand.
    pub const RIGHT_HAND: i32 = 1;
    /// Used in the left hand.
    pub const LEFT_HAND: i32 = 2;
    /// Used in either hand (what an item without a row gets).
    pub const BOTH_HANDS: i32 = 3;

    /// The Items page's label for a uses value; `None` for a value that
    /// names no use.
    #[must_use]
    pub fn label(uses: i32) -> Option<&'static str> {
        Some(match uses {
            DO_NOT_USE => "DONOTUSE",
            RIGHT_HAND => "RightHand",
            LEFT_HAND => "LeftHand",
            BOTH_HANDS => "BothHands",
            _ => return None,
        })
    }

    /// The use after `uses` in the Items page's cycle (in value order,
    /// back to the first after the last; a value that names no use goes to
    /// the first).
    #[must_use]
    pub fn cycle_next(uses: i32) -> i32 {
        match uses {
            DO_NOT_USE | RIGHT_HAND | LEFT_HAND => uses + 1,
            _ => DO_NOT_USE,
        }
    }

    /// Writes the table from the settings: a row whose item the settings
    /// no longer give a value goes, a row whose value changed takes the new
    /// one where it stands, and an item without a row gets one at the end.
    /// Rows that do not read are left as they are.
    ///
    /// # Errors
    /// The table lacks its `Object` or `Uses` column.
    pub fn write(
        database: &mut VtankDatabase,
        settings: &CombatSettings,
    ) -> Result<(), FormatError> {
        let wanted = &settings.item_use_specifiers;
        if database.find(TABLE_NAME).is_none() {
            if wanted.is_empty() {
                return Ok(());
            }
            new_table(database, TABLE_NAME, &["Object", "Uses"]);
            let table = database.find_mut(TABLE_NAME).expect("just added");
            // The item column is the table's index, as a fresh profile's.
            table.index_flags[0] = true;
        }
        let table = database.find_mut(TABLE_NAME).expect("found or added");
        let (Some(object_column), Some(uses_column)) =
            (table.column_index("Object"), table.column_index("Uses"))
        else {
            return Err(FormatError(
                "'ItemUseSpecifiers' table is missing Object/Uses columns.".to_owned(),
            ));
        };
        let mut present: HashSet<u32> = HashSet::new();
        for index in (0..table.rows.len()).rev() {
            let Some((object, uses)) =
                read_two_ints(&table.rows[index], object_column, uses_column)
            else {
                continue;
            };
            let object = object.cast_unsigned();
            match wanted.get(&object) {
                None => {
                    table.rows.remove(index);
                }
                // The last row for an item is the one read; an earlier one
                // for the same item is left as it is.
                Some(&value) => {
                    if present.insert(object) && value != uses {
                        table.rows[index].cells[uses_column] = VtankCell::int(value);
                    }
                }
            }
        }
        for (&object, &uses) in wanted.iter() {
            if !present.insert(object) {
                continue;
            }
            let mut row = filled_row(table, 0);
            row.cells[object_column] = VtankCell::int(object.cast_signed());
            row.cells[uses_column] = VtankCell::int(uses);
            table.rows.push(row);
        }
        Ok(())
    }
}

/// The consumables by kind: `AssistItems`.
pub mod assist_items {
    use super::*;

    const TABLE_NAME: &str = "AssistItems";

    /// The kind a stored type number names (`Other` for none).
    #[must_use]
    pub fn try_map(kind: i32) -> Option<ConsumableCategory> {
        Some(match kind {
            0 => ConsumableCategory::HealthKit,
            1 => ConsumableCategory::HealthFood,
            2 => ConsumableCategory::StaminaKit,
            3 => ConsumableCategory::StaminaFood,
            4 => ConsumableCategory::ManaKit,
            5 => ConsumableCategory::ManaFood,
            6 => ConsumableCategory::ManaSource,
            7 => ConsumableCategory::Grenade,
            8 => ConsumableCategory::ManaStone,
            9 => ConsumableCategory::Pea,
            10 => ConsumableCategory::Lockpick,
            11 => ConsumableCategory::AllPeas,
            _ => return None,
        })
    }

    fn try_unmap(category: ConsumableCategory) -> Option<i32> {
        Some(match category {
            ConsumableCategory::HealthKit => 0,
            ConsumableCategory::HealthFood => 1,
            ConsumableCategory::StaminaKit => 2,
            ConsumableCategory::StaminaFood => 3,
            ConsumableCategory::ManaKit => 4,
            ConsumableCategory::ManaFood => 5,
            ConsumableCategory::ManaSource => 6,
            ConsumableCategory::Grenade => 7,
            ConsumableCategory::ManaStone => 8,
            ConsumableCategory::Pea => 9,
            ConsumableCategory::Lockpick => 10,
            ConsumableCategory::AllPeas => 11,
            _ => return None,
        })
    }

    fn unmap(category: ConsumableCategory) -> i32 {
        try_unmap(category).unwrap_or(-1)
    }

    fn try_read(row: &VtankRow, name: usize, kind: usize) -> Option<AssistItem> {
        let name_cell = row.cells.get(name)?;
        let kind_cell = row.cells.get(kind)?;
        let item_name = name_cell.as_string();
        if item_name.is_empty() {
            return None;
        }
        let category = try_map(kind_cell.as_int().ok()?)?;
        Some(AssistItem {
            name: item_name.to_owned(),
            category,
        })
    }

    /// Reads the table into the imported list (not yet into the live
    /// consumables; see [`apply`]).
    pub fn read(database: &VtankDatabase, settings: &mut CombatSettings) {
        settings.imported_assist_items.clear();
        let Some(table) = database.find(TABLE_NAME) else {
            return;
        };
        let (Some(name), Some(kind)) = (table.column_index("Object"), table.column_index("Type"))
        else {
            return;
        };
        settings.imported_assist_items.extend(
            table
                .rows
                .iter()
                .filter_map(|row| try_read(row, name, kind)),
        );
    }

    /// Makes the imported items live consumables of their kinds.
    pub fn apply(settings: &mut CombatSettings) {
        for item in &settings.imported_assist_items {
            settings.consumable_names.insert(item.name.clone());
            settings
                .consumable_categories
                .insert(item.name.clone(), item.category);
        }
    }

    /// The items the table should hold: the imported ones whose kind is
    /// still wanted, then every live consumable with a kind; each once, in
    /// that order, and only kinds the table can store.
    fn desired(settings: &CombatSettings) -> Vec<AssistItem> {
        let imported = &settings.imported_assist_items;
        let still_wanted = imported.iter().filter(|item| {
            settings
                .consumable_categories
                .get(&item.name)
                .is_none_or(|current| {
                    imported
                        .iter()
                        .any(|other| other.name == item.name && other.category == *current)
                })
        });
        let live = settings.consumable_names.iter().filter_map(|name| {
            settings
                .consumable_categories
                .get(name)
                .map(|&category| AssistItem {
                    name: name.clone(),
                    category,
                })
        });
        let mut desired: Vec<AssistItem> = Vec::new();
        for item in still_wanted.cloned().chain(live) {
            if try_unmap(item.category).is_some() && !desired.contains(&item) {
                desired.push(item);
            }
        }
        desired
    }

    /// Updates only rows whose kind is understood: rows already right stay
    /// as they are, rows of a wanted name take its kind where they stand,
    /// missing items are added and unwanted ones removed.
    ///
    /// # Errors
    /// The table lacks its `Object` or `Type` column.
    pub fn write(
        database: &mut VtankDatabase,
        settings: &CombatSettings,
    ) -> Result<(), FormatError> {
        let desired = desired(settings);
        if database.find(TABLE_NAME).is_none() {
            if desired.is_empty() {
                return Ok(());
            }
            new_table(database, TABLE_NAME, &["Object", "Type"]);
        }
        let table = database.find_mut(TABLE_NAME).expect("found or added");
        let (Some(name), Some(kind)) = (table.column_index("Object"), table.column_index("Type"))
        else {
            return Err(FormatError(
                "'AssistItems' table is missing Object/Type columns.".to_owned(),
            ));
        };

        let exact: Vec<AssistItem> = table
            .rows
            .iter()
            .filter_map(|row| try_read(row, name, kind))
            .filter(|item| desired.contains(item))
            .collect();
        // The groups of one name, in order of first appearance.
        let mut groups: Vec<(&str, Vec<&AssistItem>)> = Vec::new();
        for item in &desired {
            match groups.iter_mut().find(|(key, _)| *key == item.name) {
                Some((_, members)) => members.push(item),
                None => groups.push((&item.name, vec![item])),
            }
        }
        for (key, group) in groups {
            // The rows carrying this name that no wanted kind covers yet.
            let mut rows: Vec<usize> = table
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| {
                    try_read(row, name, kind).is_some_and(|existing| {
                        existing.name == key && !desired.contains(&existing)
                    })
                })
                .map(|(index, _)| index)
                .collect();
            if let [only] = group.as_slice() {
                // One kind wanted for a name speaks for every row carrying
                // it: they are all re-kinded where they stand.
                if rows.is_empty() && !exact.contains(only) {
                    add_row(table, name, kind, only);
                }
                for index in rows {
                    table.rows[index].cells[kind] = VtankCell::int(unmap(only.category));
                }
                continue;
            }
            // Several kinds under one name: one row goes to each kind.
            for item in group.into_iter().filter(|item| !exact.contains(item)) {
                if rows.is_empty() {
                    add_row(table, name, kind, item);
                    continue;
                }
                let index = rows.remove(0);
                table.rows[index].cells[kind] = VtankCell::int(unmap(item.category));
            }
        }
        table
            .rows
            .retain(|row| try_read(row, name, kind).is_none_or(|item| desired.contains(&item)));
        Ok(())
    }

    fn add_row(table: &mut VtankTable, name: usize, kind: usize, item: &AssistItem) {
        let mut row = filled_row(table, -1);
        row.cells[name] = VtankCell::string(&item.name);
        row.cells[kind] = VtankCell::int(unmap(item.category));
        table.rows.push(row);
    }
}

/// The gems and food used for their buff: `GemFoodItems`.
pub mod gem_food_items {
    use super::*;

    const TABLE_NAME: &str = "GemFoodItems";

    fn try_read(row: &VtankRow, name: usize, spell: usize) -> Option<GemFoodItem> {
        let name_cell = row.cells.get(name)?;
        let spell_cell = row.cells.get(spell)?;
        let item_name = name_cell.as_string();
        let spell_id = spell_cell.as_int().ok()?;
        if item_name.is_empty() || spell_id <= 0 {
            return None;
        }
        Some(GemFoodItem {
            name: item_name.to_owned(),
            spell_id: spell_id.cast_unsigned(),
        })
    }

    /// Reads the table into the buff settings.
    pub fn read(database: &VtankDatabase, settings: &mut BuffSettings) {
        settings.gem_food_items.clear();
        let Some(table) = database.find(TABLE_NAME) else {
            return;
        };
        let (Some(name), Some(spell)) = (table.column_index("Name"), table.column_index("Spell"))
        else {
            return;
        };
        settings.gem_food_items.extend(
            table
                .rows
                .iter()
                .filter_map(|row| try_read(row, name, spell)),
        );
    }

    /// Matches each row to an entry, first by name and spell, then by name
    /// alone; a matched row keeps its cells (its spell updated), an
    /// unmatched readable row goes, and the entries left over are added.
    ///
    /// # Errors
    /// The table lacks its `Name` or `Spell` column.
    pub fn write(database: &mut VtankDatabase, settings: &BuffSettings) -> Result<(), FormatError> {
        if database.find(TABLE_NAME).is_none() {
            if settings.gem_food_items.is_empty() {
                return Ok(());
            }
            new_table(database, TABLE_NAME, &["Name", "Spell"]);
        }
        let table = database.find_mut(TABLE_NAME).expect("found or added");
        let (Some(name), Some(spell)) = (table.column_index("Name"), table.column_index("Spell"))
        else {
            return Err(FormatError(
                "'GemFoodItems' table is missing Name/Spell columns.".to_owned(),
            ));
        };
        let mut remaining = settings.gem_food_items.clone();
        let mut matches: Vec<Option<GemFoodItem>> = vec![None; table.rows.len()];
        for (index, row) in table.rows.iter().enumerate() {
            let Some(source) = try_read(row, name, spell) else {
                continue;
            };
            if let Some(entry) = remaining.iter().position(|entry| *entry == source) {
                matches[index] = Some(remaining.remove(entry));
            }
        }
        for (index, row) in table.rows.iter().enumerate() {
            if matches[index].is_some() {
                continue;
            }
            let Some(source) = try_read(row, name, spell) else {
                continue;
            };
            if let Some(entry) = remaining.iter().position(|entry| entry.name == source.name) {
                matches[index] = Some(remaining.remove(entry));
            }
        }
        for index in (0..table.rows.len()).rev() {
            let Some(source) = try_read(&table.rows[index], name, spell) else {
                continue;
            };
            match &matches[index] {
                None => {
                    table.rows.remove(index);
                }
                Some(entry) if entry.spell_id != source.spell_id => {
                    table.rows[index].cells[spell] = VtankCell::int(entry.spell_id.cast_signed());
                }
                Some(_) => {}
            }
        }
        for entry in remaining {
            let mut row = filled_row(table, 0);
            row.cells[name] = VtankCell::string(&entry.name);
            row.cells[spell] = VtankCell::int(entry.spell_id.cast_signed());
            table.rows.push(row);
        }
        Ok(())
    }
}

/// The extra buffs to keep up and the buffs never to cast:
/// `ExtraBuffSpells` and `AntiExtraBuffSpells`, by exemplar spell id.
pub mod buff_exemplars {
    use super::*;

    const EXTRA: &str = "ExtraBuffSpells";
    const ANTI_EXTRA: &str = "AntiExtraBuffSpells";

    /// Reads both tables into the buff settings.
    pub fn read(database: &VtankDatabase, settings: &mut BuffSettings) {
        read_table(database, EXTRA, &mut settings.extra_buff_spell_ids);
        read_table(
            database,
            ANTI_EXTRA,
            &mut settings.anti_extra_buff_spell_ids,
        );
    }

    /// Writes both tables.
    ///
    /// # Errors
    /// A table lacks its `ExemplarId` column.
    pub fn write(database: &mut VtankDatabase, settings: &BuffSettings) -> Result<(), FormatError> {
        write_table(database, EXTRA, &settings.extra_buff_spell_ids)?;
        write_table(database, ANTI_EXTRA, &settings.anti_extra_buff_spell_ids)
    }

    fn try_get_id(row: &VtankRow, column: usize) -> Option<u32> {
        let cell = row.cells.get(column)?;
        if cell.tag == "u" {
            let id = cell.as_uint().ok()?;
            return (id != 0).then_some(id);
        }
        let raw = cell.as_int().ok()?;
        (raw != 0).then(|| raw.cast_unsigned())
    }

    fn read_table(database: &VtankDatabase, table_name: &str, ids: &mut Vec<u32>) {
        ids.clear();
        let Some(table) = database.find(table_name) else {
            return;
        };
        let Some(exemplar) = table.column_index("ExemplarId") else {
            return;
        };
        for row in &table.rows {
            if let Some(id) = try_get_id(row, exemplar)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
    }

    fn write_table(
        database: &mut VtankDatabase,
        table_name: &str,
        ids: &[u32],
    ) -> Result<(), FormatError> {
        if database.find(table_name).is_none() {
            if ids.is_empty() {
                return Ok(());
            }
            new_table(database, table_name, &["ExemplarId"]);
        }
        let table = database.find_mut(table_name).expect("found or added");
        let Some(exemplar) = table.column_index("ExemplarId") else {
            return Err(FormatError(format!(
                "'{table_name}' table is missing ExemplarId column."
            )));
        };
        let selected: HashSet<u32> = ids.iter().copied().collect();
        let mut present: HashSet<u32> = HashSet::new();
        for index in (0..table.rows.len()).rev() {
            let Some(id) = try_get_id(&table.rows[index], exemplar) else {
                continue;
            };
            if !selected.contains(&id) {
                table.rows.remove(index);
                continue;
            }
            present.insert(id);
        }
        for &id in ids {
            if !present.insert(id) {
                continue;
            }
            let mut row = filled_row(table, 0);
            row.cells[exemplar] = VtankCell::int(id.cast_signed());
            table.rows.push(row);
        }
        Ok(())
    }
}
