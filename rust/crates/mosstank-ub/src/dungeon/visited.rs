//! The cells of one landblock the player has stood in
//! (`DungeonVisitedCells`), kept in the settings tree so the next visit to
//! the same dungeon starts with them shaded. One file per landblock, one
//! cell id per line in hexadecimal; a line that is not a cell id is skipped
//! rather than failing the file.

use std::collections::BTreeSet;

use mosstank_stores::storage::{PluginStorage, StorageError};
use mosstank_stores::ub_store::ROOT;
use openac_plugin_compat::number::{self, NumberStyles};

/// The cells walked in one landblock.
#[derive(Debug, Clone, Default)]
pub struct DungeonVisitedCells {
    cells: BTreeSet<u32>,
    landblock_id: u32,
    key: Option<String>,
}

impl DungeonVisitedCells {
    /// No landblock yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The landblock the set is of, with a zero low half; zero before any
    /// bind.
    #[must_use]
    pub fn landblock_id(&self) -> u32 {
        self.landblock_id
    }

    /// How many cells of this landblock have been walked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether none has.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The file a landblock's cells are kept in.
    #[must_use]
    pub fn key_for(landblock_id: u32) -> String {
        format!(
            "{ROOT}dungeonmaps/visited/{:04X}.txt",
            (landblock_id >> 16) & 0xFFFF
        )
    }

    /// Opens the set for a landblock, reading what an earlier session left.
    /// Binding the same landblock again is no change. A file that cannot be
    /// read is no file: the set starts empty.
    pub fn bind(&mut self, storage: &mut dyn PluginStorage, landblock_id: u32) {
        let landblock_id = landblock_id & 0xFFFF_0000;
        if landblock_id == self.landblock_id && self.key.is_some() {
            return;
        }
        self.landblock_id = landblock_id;
        let key = Self::key_for(landblock_id);
        self.cells.clear();
        let text = if storage.is_available() {
            storage.read_text(&key).ok().flatten()
        } else {
            None
        };
        self.key = Some(key);
        let Some(text) = text else {
            return;
        };
        for line in text.split('\n') {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(cell_id) = number::parse_u32(trimmed, NumberStyles::HEX_NUMBER)
                && (cell_id & 0xFFFF_0000) == landblock_id
            {
                self.cells.insert(cell_id);
            }
        }
    }

    /// Whether the player has stood in a cell.
    #[must_use]
    pub fn contains(&self, cell_id: u32) -> bool {
        self.cells.contains(&cell_id)
    }

    /// Records that the player stands in a cell; whether that is new. A
    /// cell of another landblock is not this set's and is refused.
    ///
    /// # Errors
    /// The cell is new but the file could not be written; it counts for the
    /// session all the same.
    pub fn mark(
        &mut self,
        storage: &mut dyn PluginStorage,
        cell_id: u32,
    ) -> Result<bool, StorageError> {
        if self.key.is_none()
            || (cell_id & 0xFFFF_0000) != self.landblock_id
            || !self.cells.insert(cell_id)
        {
            return Ok(false);
        }
        self.save(storage)?;
        Ok(true)
    }

    fn save(&self, storage: &mut dyn PluginStorage) -> Result<(), StorageError> {
        let Some(key) = &self.key else {
            return Ok(());
        };
        if !storage.is_available() {
            return Ok(());
        }
        let mut text = String::with_capacity(self.cells.len() * 9);
        for cell_id in &self.cells {
            text.push_str(&format!("{cell_id:08X}\n"));
        }
        storage.write_text(key, &text)
    }
}
