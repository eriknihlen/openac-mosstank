//! VTank's settings database text (`.usd` settings profiles, `.ast`, the
//! game-information tables): an ordered set of named tables whose cells are
//! tagged values, one value per line, lines ended with `\r\n`.
//!
//! ```text
//! <table count>
//! <table name>
//! <column count> <column names...> <index flags y/n...>
//! <row count> <cells, row by row: tag line, then the value>
//! ```
//!
//! Cell tags: `d` double, `i` int, `u` uint, `f` float, `s` string, `b`
//! bool (`True`/`False`), `TABLE` a nested table, `ba` a blob (its length in
//! UTF-16 code units, then that many characters, line breaks included). An
//! unknown tag is a cell with no value line.

use mosstank_dotnet::text::ordinal;
use mosstank_dotnet::{format_g15, try_parse_int};
use openac_plugin_compat::text::utf16;

use crate::FormatError;

/// The scalar tags that carry one value line.
const KNOWN_SCALAR_TAGS: [&str; 6] = ["d", "i", "u", "f", "s", "b"];

/// One cell: its tag and, by tag, its value text, nested table or blob.
#[derive(Debug, Clone, PartialEq)]
pub struct VtankCell {
    /// The raw type tag: `d`/`i`/`u`/`f`/`s`/`b`/`TABLE`/`ba`/other.
    pub tag: String,
    /// Raw text of the single value line, for every scalar tag except `ba`.
    pub scalar_text: Option<String>,
    /// The nested table of a `TABLE` cell.
    pub table: Option<Box<VtankTable>>,
    /// The characters of a `ba` cell.
    pub blob_text: Option<String>,
}

impl VtankCell {
    /// A cell with only a tag.
    #[must_use]
    pub fn tagged(tag: &str) -> Self {
        Self {
            tag: tag.to_owned(),
            scalar_text: None,
            table: None,
            blob_text: None,
        }
    }

    fn scalar(tag: &str, text: String) -> Self {
        Self {
            scalar_text: Some(text),
            ..Self::tagged(tag)
        }
    }

    /// A `d` cell, written with 15 significant digits.
    #[must_use]
    pub fn double(value: f64) -> Self {
        Self::scalar("d", format_double(value))
    }

    /// An `i` cell.
    #[must_use]
    pub fn int(value: i32) -> Self {
        Self::scalar("i", value.to_string())
    }

    /// A `u` cell.
    #[must_use]
    pub fn uint(value: u32) -> Self {
        Self::scalar("u", value.to_string())
    }

    /// An `f` cell: the float widened to double, written as a `d` is.
    #[must_use]
    pub fn float(value: f32) -> Self {
        Self::scalar("f", format_double(f64::from(value)))
    }

    /// An `s` cell; line feeds are dropped (a value is one line).
    #[must_use]
    pub fn string(value: &str) -> Self {
        Self::scalar("s", value.replace('\n', ""))
    }

    /// A `b` cell.
    #[must_use]
    pub fn bool(value: bool) -> Self {
        Self::scalar("b", if value { "True" } else { "False" }.to_owned())
    }

    /// A `TABLE` cell.
    #[must_use]
    pub fn nested_table(table: VtankTable) -> Self {
        Self {
            table: Some(Box::new(table)),
            ..Self::tagged("TABLE")
        }
    }

    fn text_or_zero(&self) -> &str {
        self.scalar_text.as_deref().unwrap_or("0")
    }

    /// The value as a double (`double.Parse`, `Float` style).
    ///
    /// # Errors
    /// The text is not a number.
    pub fn as_double(&self) -> Result<f64, FormatError> {
        let text = self.text_or_zero();
        mosstank_dotnet::try_parse_double(text).ok_or_else(|| not_a_number(text))
    }

    /// The value as an int (`int.Parse`, `Integer` style), with .NET's
    /// messages.
    ///
    /// # Errors
    /// The text is not an integer, or is out of range.
    pub fn as_int(&self) -> Result<i32, FormatError> {
        mosstank_dotnet::parse_int(self.text_or_zero()).map_err(FormatError)
    }

    /// The value as a uint.
    ///
    /// # Errors
    /// The text is not an unsigned integer in range.
    pub fn as_uint(&self) -> Result<u32, FormatError> {
        mosstank_dotnet::parse_uint(self.text_or_zero()).map_err(FormatError)
    }

    /// The value as a float.
    ///
    /// # Errors
    /// The text is not a number.
    pub fn as_float(&self) -> Result<f32, FormatError> {
        let text = self.text_or_zero();
        mosstank_dotnet::try_parse_float(text).ok_or_else(|| not_a_number(text))
    }

    /// The value text, empty when there is none.
    #[must_use]
    pub fn as_string(&self) -> &str {
        self.scalar_text.as_deref().unwrap_or("")
    }

    /// True only for the exact text `True`.
    #[must_use]
    pub fn as_bool(&self) -> bool {
        self.scalar_text.as_deref() == Some("True")
    }

    /// Appends the cell as written.
    pub fn write_to(&self, out: &mut String) {
        append_line(out, &self.tag);
        if self.tag == "TABLE" {
            if let Some(table) = &self.table {
                table.write_to(out);
            }
        } else if self.tag == "ba" {
            let blob = self.blob_text.as_deref().unwrap_or("");
            append_line(out, &utf16::len(blob).to_string());
            out.push_str(blob);
        } else if KNOWN_SCALAR_TAGS.contains(&self.tag.as_str()) {
            append_line(out, self.as_string());
        }
    }
}

fn not_a_number(text: &str) -> FormatError {
    FormatError(format!(
        "The input string '{text}' was not in a correct format."
    ))
}

/// A double as the files write it: `G15`.
#[must_use]
pub fn format_double(value: f64) -> String {
    format_g15(value)
}

/// Appends `text` and the files' line break.
pub fn append_line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push_str("\r\n");
}

/// One row: a cell per column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankRow {
    /// The cells, in column order.
    pub cells: Vec<VtankCell>,
}

impl VtankRow {
    /// Appends the row as written.
    pub fn write_to(&self, out: &mut String) {
        for cell in &self.cells {
            cell.write_to(out);
        }
    }
}

/// One table: column names, index flags and rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankTable {
    /// Column names, in order.
    pub column_names: Vec<String>,
    /// Whether each column is indexed (`y`).
    pub index_flags: Vec<bool>,
    /// The rows.
    pub rows: Vec<VtankRow>,
}

impl VtankTable {
    /// The index of the column named exactly `name`.
    #[must_use]
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.column_names.iter().position(|column| column == name)
    }

    /// Reads a table at the cursor.
    ///
    /// # Errors
    /// The text ends early or a count is not a number.
    pub fn read(cursor: &mut VtankLineCursor<'_>) -> Result<Self, FormatError> {
        let mut table = Self::default();
        let column_count = cursor.read_int()?;
        for _ in 0..column_count {
            table.column_names.push(cursor.read_line()?.to_owned());
        }
        for _ in 0..column_count {
            table.index_flags.push(cursor.read_line()? == "y");
        }
        let row_count = cursor.read_int()?;
        for _ in 0..row_count {
            let mut row = VtankRow::default();
            for _ in 0..column_count {
                row.cells.push(read_cell(cursor)?);
            }
            table.rows.push(row);
        }
        Ok(table)
    }

    /// Appends the table as written.
    pub fn write_to(&self, out: &mut String) {
        append_line(out, &self.column_names.len().to_string());
        for column in &self.column_names {
            append_line(out, column);
        }
        for flag in &self.index_flags {
            append_line(out, if *flag { "y" } else { "n" });
        }
        append_line(out, &self.rows.len().to_string());
        for row in &self.rows {
            row.write_to(out);
        }
    }
}

/// The whole settings database: an ordered set of named tables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankDatabase {
    /// The tables with their names, in file order.
    pub tables: Vec<(String, VtankTable)>,
}

impl VtankDatabase {
    /// The first table named exactly `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&VtankTable> {
        self.tables
            .iter()
            .find(|(table_name, _)| table_name == name)
            .map(|(_, table)| table)
    }

    /// The first table named exactly `name`, to change.
    pub fn find_mut(&mut self, name: &str) -> Option<&mut VtankTable> {
        self.tables
            .iter_mut()
            .find(|(table_name, _)| table_name == name)
            .map(|(_, table)| table)
    }

    /// Parses a database.
    ///
    /// # Errors
    /// The text ends early or a count is not a number.
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        let mut cursor = VtankLineCursor::new(text);
        let mut database = Self::default();
        let table_count = cursor.read_int()?;
        for _ in 0..table_count {
            let name = cursor.read_line()?.to_owned();
            let table = VtankTable::read(&mut cursor)?;
            database.tables.push((name, table));
        }
        Ok(database)
    }

    /// The database as written: tables in ordinal name order.
    #[must_use]
    pub fn render(&self) -> String {
        let mut ordered: Vec<&(String, VtankTable)> = self.tables.iter().collect();
        ordered.sort_by(|a, b| ordinal::compare(&a.0, &b.0));
        let mut out = String::new();
        append_line(&mut out, &ordered.len().to_string());
        for (name, table) in ordered {
            append_line(&mut out, name);
            table.write_to(&mut out);
        }
        out
    }
}

/// Reads one cell at the cursor.
///
/// # Errors
/// The text ends early or a blob length is not a number.
pub fn read_cell(cursor: &mut VtankLineCursor<'_>) -> Result<VtankCell, FormatError> {
    let tag = cursor.read_line()?;
    Ok(match tag {
        "TABLE" => VtankCell::nested_table(VtankTable::read(cursor)?),
        "ba" => {
            let length = cursor.read_int()?;
            VtankCell {
                blob_text: Some(cursor.read_blob(length)?.to_owned()),
                ..VtankCell::tagged(tag)
            }
        }
        "d" | "i" | "u" | "f" | "s" | "b" => {
            let text = cursor.read_line()?.to_owned();
            VtankCell::scalar(tag, text)
        }
        _ => VtankCell::tagged(tag),
    })
}

/// Reads the database text line by line, counting lines for messages. A
/// line ends at `\n`; a `\r` before it is dropped.
#[derive(Debug, Clone)]
pub struct VtankLineCursor<'a> {
    text: &'a str,
    position: usize,
    line_number: usize,
}

impl<'a> VtankLineCursor<'a> {
    /// A cursor at the start of `text`.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            position: 0,
            line_number: 1,
        }
    }

    /// The byte offset of what has not been read yet.
    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    /// The text read between byte offset `start` and now.
    #[must_use]
    pub fn read_since(&self, start: usize) -> &'a str {
        &self.text[start..self.position]
    }

    /// The number of the next line (from 1).
    #[must_use]
    pub fn line_number(&self) -> usize {
        self.line_number
    }

    /// The next line.
    ///
    /// # Errors
    /// The text has ended.
    pub fn read_line(&mut self) -> Result<&'a str, FormatError> {
        if self.position >= self.text.len() {
            return Err(FormatError(format!(
                "line {}: unexpected end of file (expected another line).",
                self.line_number
            )));
        }
        let rest = &self.text[self.position..];
        // A plain scan: lines are short, and the general search costs more
        // to set up than it saves on them.
        let (mut line, consumed) = match rest.bytes().position(|byte| byte == b'\n') {
            Some(newline) => (&rest[..newline], newline + 1),
            None => (rest, rest.len()),
        };
        self.position += consumed;
        if let Some(stripped) = line.strip_suffix('\r') {
            line = stripped;
        }
        self.line_number += 1;
        Ok(line)
    }

    /// The next line as an integer.
    ///
    /// # Errors
    /// The text has ended or the line is not an integer.
    pub fn read_int(&mut self) -> Result<i32, FormatError> {
        let line = self.read_line()?;
        try_parse_int(line).ok_or_else(|| {
            FormatError(format!(
                "line {}: expected an integer, got '{line}'.",
                self.line_number - 1
            ))
        })
    }

    /// The next `length` UTF-16 code units, line breaks included.
    ///
    /// # Errors
    /// Fewer characters remain.
    pub fn read_blob(&mut self, length: i32) -> Result<&'a str, FormatError> {
        let Ok(length) = usize::try_from(length) else {
            return Ok("");
        };
        if length == 0 {
            return Ok("");
        }
        let rest = &self.text[self.position..];
        let Some((blob, _)) = utf16::split_at(rest, length) else {
            return Err(FormatError(format!(
                "line {}: blob of {length} characters exceeds remaining input.",
                self.line_number
            )));
        };
        self.position += blob.len();
        self.line_number += blob.matches('\n').count();
        Ok(blob)
    }
}

/// What the readers of a database ask of a cell, whether it keeps its text
/// ([`VtankCell`]) or reads it in place ([`VtankCellRef`]).
pub trait CellValue {
    /// The raw type tag.
    fn tag(&self) -> &str;

    /// The text of the single value line, for a scalar tag.
    fn scalar_text(&self) -> Option<&str>;

    /// As [`VtankCell::as_string`].
    fn as_string(&self) -> &str {
        self.scalar_text().unwrap_or("")
    }

    /// As [`VtankCell::as_int`].
    ///
    /// # Errors
    /// The text is not an integer, or is out of range.
    fn as_int(&self) -> Result<i32, FormatError> {
        mosstank_dotnet::parse_int(self.scalar_text().unwrap_or("0")).map_err(FormatError)
    }

    /// As [`VtankCell::as_double`].
    ///
    /// # Errors
    /// The text is not a number.
    fn as_double(&self) -> Result<f64, FormatError> {
        let text = self.scalar_text().unwrap_or("0");
        mosstank_dotnet::try_parse_double(text).ok_or_else(|| not_a_number(text))
    }
}

impl CellValue for VtankCell {
    fn tag(&self) -> &str {
        &self.tag
    }

    fn scalar_text(&self) -> Option<&str> {
        self.scalar_text.as_deref()
    }
}

/// What the readers of a database ask of its tables, whether it keeps its
/// text ([`VtankDatabase`]) or reads it in place ([`VtankDatabaseRef`]).
pub trait TableSet {
    /// Its cells.
    type Cell: CellValue;

    /// How many tables it has.
    fn table_count(&self) -> usize;

    /// The rows of table `index`, in order, each its cells.
    fn rows_of(&self, index: usize) -> Vec<&[Self::Cell]>;

    /// The index of the first table named exactly `name`.
    fn index_of(&self, name: &str) -> Option<usize>;

    /// Keeps only the first `rows` rows of table `index` and no table after
    /// it.
    fn end_at(&mut self, index: usize, rows: usize);
}

impl TableSet for VtankDatabase {
    type Cell = VtankCell;

    fn table_count(&self) -> usize {
        self.tables.len()
    }

    fn rows_of(&self, index: usize) -> Vec<&[VtankCell]> {
        self.tables
            .get(index)
            .map(|(_, table)| table.rows.iter().map(|row| row.cells.as_slice()).collect())
            .unwrap_or_default()
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.tables
            .iter()
            .position(|(table_name, _)| table_name == name)
    }

    fn end_at(&mut self, index: usize, rows: usize) {
        if let Some((_, table)) = self.tables.get_mut(index) {
            table.rows.truncate(rows);
        }
        self.tables.truncate(index + 1);
    }
}

/// A cell read in place: [`VtankCell`] borrowing the text it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct VtankCellRef<'a> {
    /// The raw type tag.
    pub tag: &'a str,
    /// The single value line, for every scalar tag except `ba`.
    pub scalar_text: Option<&'a str>,
    /// The nested table of a `TABLE` cell.
    pub table: Option<Box<VtankTableRef<'a>>>,
    /// The characters of a `ba` cell.
    pub blob_text: Option<&'a str>,
}

impl<'a> VtankCellRef<'a> {
    fn tagged(tag: &'a str) -> Self {
        Self {
            tag,
            scalar_text: None,
            table: None,
            blob_text: None,
        }
    }

    /// The cell keeping its own text.
    #[must_use]
    pub fn to_cell(&self) -> VtankCell {
        VtankCell {
            tag: self.tag.to_owned(),
            scalar_text: self.scalar_text.map(str::to_owned),
            table: self.table.as_ref().map(|table| Box::new(table.to_table())),
            blob_text: self.blob_text.map(str::to_owned),
        }
    }
}

impl CellValue for VtankCellRef<'_> {
    fn tag(&self) -> &str {
        self.tag
    }

    fn scalar_text(&self) -> Option<&str> {
        self.scalar_text
    }
}

/// Reads one cell at the cursor in place, as [`read_cell`] reads it.
///
/// # Errors
/// As [`read_cell`].
pub fn read_cell_ref<'a>(
    cursor: &mut VtankLineCursor<'a>,
) -> Result<VtankCellRef<'a>, FormatError> {
    let tag = cursor.read_line()?;
    Ok(match tag {
        "TABLE" => VtankCellRef {
            table: Some(Box::new(VtankTableRef::read(cursor)?)),
            ..VtankCellRef::tagged(tag)
        },
        "ba" => {
            let length = cursor.read_int()?;
            VtankCellRef {
                blob_text: Some(cursor.read_blob(length)?),
                ..VtankCellRef::tagged(tag)
            }
        }
        "d" | "i" | "u" | "f" | "s" | "b" => VtankCellRef {
            scalar_text: Some(cursor.read_line()?),
            ..VtankCellRef::tagged(tag)
        },
        _ => VtankCellRef::tagged(tag),
    })
}

/// A table read in place: [`VtankTable`] borrowing the text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankTableRef<'a> {
    /// Column names, in order.
    pub column_names: Vec<&'a str>,
    /// Whether each column is indexed (`y`).
    pub index_flags: Vec<bool>,
    /// How many cells a row has (the column count read).
    width: usize,
    /// How many rows it has.
    row_count: usize,
    /// The cells, row after row, each row in column order.
    cells: Vec<VtankCellRef<'a>>,
}

impl<'a> VtankTableRef<'a> {
    /// Reads a table at the cursor in place, as [`VtankTable::read`] reads
    /// it.
    ///
    /// # Errors
    /// As [`VtankTable::read`].
    pub fn read(cursor: &mut VtankLineCursor<'a>) -> Result<Self, FormatError> {
        let mut table = Self::default();
        let column_count = cursor.read_int()?;
        for _ in 0..column_count {
            table.column_names.push(cursor.read_line()?);
        }
        for _ in 0..column_count {
            table.index_flags.push(cursor.read_line()? == "y");
        }
        let row_count = cursor.read_int()?;
        table.width = usize::try_from(column_count).unwrap_or(0);
        for _ in 0..row_count {
            for _ in 0..column_count {
                table.cells.push(read_cell_ref(cursor)?);
            }
            table.row_count += 1;
        }
        Ok(table)
    }

    /// The table keeping its own text.
    #[must_use]
    pub fn to_table(&self) -> VtankTable {
        VtankTable {
            column_names: self.column_names.iter().map(|&n| n.to_owned()).collect(),
            index_flags: self.index_flags.clone(),
            rows: self
                .rows()
                .into_iter()
                .map(|cells| VtankRow {
                    cells: cells.iter().map(VtankCellRef::to_cell).collect(),
                })
                .collect(),
        }
    }

    /// The rows, each its cells in column order.
    #[must_use]
    pub fn rows(&self) -> Vec<&[VtankCellRef<'a>]> {
        if self.width == 0 {
            return vec![&[]; self.row_count];
        }
        self.cells.chunks(self.width).collect()
    }

    /// Keeps the first `rows` rows.
    fn truncate(&mut self, rows: usize) {
        self.row_count = self.row_count.min(rows);
        self.cells.truncate(self.row_count * self.width);
    }
}

/// A database read in place: [`VtankDatabase`] borrowing the text, for a
/// large text read only to be looked at (the game database is some two
/// megabytes and a hundred thousand cells).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VtankDatabaseRef<'a> {
    /// The tables with their names, in file order.
    pub tables: Vec<(&'a str, VtankTableRef<'a>)>,
}

impl<'a> VtankDatabaseRef<'a> {
    /// Parses a database in place, as [`VtankDatabase::parse`] parses it.
    ///
    /// # Errors
    /// As [`VtankDatabase::parse`].
    pub fn parse(text: &'a str) -> Result<Self, FormatError> {
        let mut cursor = VtankLineCursor::new(text);
        let mut database = Self::default();
        let table_count = cursor.read_int()?;
        for _ in 0..table_count {
            let name = cursor.read_line()?;
            let table = VtankTableRef::read(&mut cursor)?;
            database.tables.push((name, table));
        }
        Ok(database)
    }

    /// The database keeping its own text.
    #[must_use]
    pub fn to_database(&self) -> VtankDatabase {
        VtankDatabase {
            tables: self
                .tables
                .iter()
                .map(|(name, table)| ((*name).to_owned(), table.to_table()))
                .collect(),
        }
    }
}

impl<'a> TableSet for VtankDatabaseRef<'a> {
    type Cell = VtankCellRef<'a>;

    fn table_count(&self) -> usize {
        self.tables.len()
    }

    fn rows_of(&self, index: usize) -> Vec<&[VtankCellRef<'a>]> {
        self.tables
            .get(index)
            .map(|(_, table)| table.rows())
            .unwrap_or_default()
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.tables
            .iter()
            .position(|(table_name, _)| *table_name == name)
    }

    fn end_at(&mut self, index: usize, rows: usize) {
        if let Some((_, table)) = self.tables.get_mut(index) {
            table.truncate(rows);
        }
        self.tables.truncate(index + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMBEDDED_BLOB: &str = "line1\r\nline2";

    fn single_ba_cell_document(blob: &str) -> String {
        let header = [
            "1",
            "T",
            "1",
            "Col1",
            "n",
            "1",
            "ba",
            &utf16::len(blob).to_string(),
        ]
        .join("\r\n");
        format!("{header}\r\n{blob}")
    }

    // Read in place, every document reads as the owned parse reads it, and
    // a broken one fails with the same message.
    #[test]
    fn a_database_read_in_place_is_the_database_parsed() {
        let mut documents: Vec<String> = [
            "gameinfodb-excerpt.ugd",
            "defaultsettings.usd",
            "owner-a.usd",
            "owner-b.usd",
            "owner-c.usd",
            "owner-a.ast",
        ]
        .iter()
        .map(|name| crate::fixtures::read_vtank(name))
        .collect();
        documents.push(single_ba_cell_document(EMBEDDED_BLOB));
        let excerpt = documents[0].clone();
        for cut in [0, 1, 3, 40, 200, excerpt.len() / 2, excerpt.len() - 3] {
            documents.push(excerpt[..cut].to_owned());
        }
        documents.push(excerpt.replacen("14", "x", 1));
        documents.push(single_ba_cell_document("short").replace(
            "
5
", "
50
",
        ));
        documents.push(
            "1
T
1
C
n
2
TABLE
1
D
y
1
i
5
zz
i
"
            .to_owned(),
        );
        for document in &documents {
            let in_place = VtankDatabaseRef::parse(document).map(|database| database.to_database());
            assert_eq!(in_place, VtankDatabase::parse(document), "{document:.60?}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankUsdDocumentTests.cs :: BaBlobWithEmbeddedCrlfRoundTripsExactCharacterCount
    #[test]
    fn ba_blob_with_embedded_crlf_round_trips_exact_character_count() {
        let document = single_ba_cell_document(EMBEDDED_BLOB);
        let parsed = VtankDatabase::parse(&document).unwrap();
        let cell = &parsed.find("T").unwrap().rows[0].cells[0];
        assert_eq!(cell.tag, "ba");
        assert_eq!(cell.blob_text.as_deref(), Some(EMBEDDED_BLOB));
        assert_eq!(parsed.render(), document);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankUsdDocumentTests.cs :: BaBlobLengthCountsEmbeddedCrAndLfAsSeparateCharacters
    #[test]
    fn ba_blob_length_counts_embedded_cr_and_lf_as_separate_characters() {
        let lf_only = "line1\nline2";
        let crlf = "line1\r\nline2";
        assert_eq!(lf_only.len() + 1, crlf.len());
        let parsed_lf = VtankDatabase::parse(&single_ba_cell_document(lf_only)).unwrap();
        let parsed_crlf = VtankDatabase::parse(&single_ba_cell_document(crlf)).unwrap();
        assert_eq!(
            parsed_lf.find("T").unwrap().rows[0].cells[0]
                .blob_text
                .as_deref(),
            Some(lf_only)
        );
        assert_eq!(
            parsed_crlf.find("T").unwrap().rows[0].cells[0]
                .blob_text
                .as_deref(),
            Some(crlf)
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankUsdDocumentTests.cs :: UnrecognizedTagConsumesOnlyItsOwnLineNotTheNextCellsValue
    #[test]
    fn unrecognized_tag_consumes_only_its_own_line_not_the_next_cells_value() {
        let document = [
            "1", "T", "2", "Col1", "Col2", "n", "n", "1", "0", "s", "hello", "",
        ]
        .join("\r\n");
        let parsed = VtankDatabase::parse(&document).unwrap();
        let row = &parsed.find("T").unwrap().rows[0];
        assert_eq!(row.cells[0].tag, "0");
        assert_eq!(row.cells[0].scalar_text, None);
        assert_eq!(row.cells[1].tag, "s");
        assert_eq!(row.cells[1].as_string(), "hello");
        assert_eq!(parsed.render(), document);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankUsdDocumentTests.cs :: RenderEmitsTablesInNameOrderRegardlessOfInsertionOrder
    #[test]
    fn render_emits_tables_in_name_order_regardless_of_insertion_order() {
        let mut database = VtankDatabase::default();
        for name in ["Zebra", "Apple", "Mango"] {
            database
                .tables
                .push((name.to_owned(), VtankTable::default()));
        }
        let reparsed = VtankDatabase::parse(&database.render()).unwrap();
        let names: Vec<&str> = reparsed.tables.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Apple", "Mango", "Zebra"]);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankUsdDocumentTests.cs :: StringCellStripsEmbeddedNewlineAtConstructionNotJustOnWrite
    #[test]
    fn string_cell_strips_embedded_newline_at_construction_not_just_on_write() {
        let cell = VtankCell::string("line1\nline2");
        assert_eq!(cell.as_string(), "line1line2");
        let mut out = String::new();
        cell.write_to(&mut out);
        assert_eq!(out, "s\r\nline1line2\r\n");
    }
}
