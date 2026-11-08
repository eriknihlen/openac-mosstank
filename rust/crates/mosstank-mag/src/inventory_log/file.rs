//! The log file: `Mag-Tools/<world>/<character>.inventory.json` in the
//! plugin's own data, our own JSON (owner, 2026-11-08; Mag-Tools wrote
//! XML). Written from each record's kept text ([`write`]); read a few
//! items at a time ([`LogReader`]), since a character's file can hold a
//! couple of thousand items.
//!
//! ```json
//! {"version":1,"character":"Acdream","world":"ACE","saved_unix_ms":1762560000000,"items":[
//! {"id":2147483905,"name":"Test Ring",...},
//! ...
//! ]}
//! ```

use std::ops::Range;

use openac_plugin_compat::json::{self, JsonValue, system_text_json};

use super::record::ItemRecord;

/// The file's format version this build writes and reads.
pub const VERSION: i64 = 1;

/// The file's own members, beside its items.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogHeader {
    /// The format version ([`VERSION`]).
    pub version: i64,
    /// The character's name, as the game spells it.
    pub character: String,
    /// The world (server) it lives on.
    pub world: String,
    /// When the file was written, Unix milliseconds.
    pub saved_unix_ms: i64,
}

/// Why a file was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Not a log file at all, or a broken one (Mag-Tools: "Inventory file
    /// is corrupt.").
    Corrupt,
    /// A log of another format version (none: no version); it is never
    /// overwritten.
    UnknownVersion(Option<i64>),
}

/// The storage key of a character's log: `Mag-Tools/<world>/<name>.inventory.json`,
/// each name made safe for a key (no `:`, `\` or `/`, and no `.` that
/// would make `..` or end a folder or file name: `Mr.` is `Mr_`).
#[must_use]
pub fn storage_key(world: &str, character: &str) -> String {
    format!(
        "Mag-Tools/{}/{}.inventory.json",
        key_part(world),
        key_part(character)
    )
}

fn key_part(name: &str) -> String {
    let chars: Vec<char> = name
        .chars()
        .map(|c| {
            if matches!(c, ':' | '\\' | '/') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut part: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let next = chars.get(i + 1);
            if c == '.' && next.is_none_or(|&n| n == '.') {
                '_'
            } else {
                c
            }
        })
        .collect();
    if part.is_empty() {
        part.push('_');
    }
    part
}

/// The file: the header, then each record's kept JSON text, one a line.
#[must_use]
pub fn write<'a>(header: &LogHeader, items: impl Iterator<Item = &'a str>) -> String {
    let string = |text: &str| {
        system_text_json::to_string(
            &JsonValue::String(text.to_owned()),
            &system_text_json::Options::COMPACT,
        )
        .unwrap_or_default()
    };
    let mut out = format!(
        "{{\"version\":{},\"character\":{},\"world\":{},\"saved_unix_ms\":{},\"items\":[",
        header.version,
        string(&header.character),
        string(&header.world),
        header.saved_unix_ms
    );
    let mut first = true;
    for item in items {
        if !first {
            out.push(',');
        }
        first = false;
        out.push('\n');
        out.push_str(item);
    }
    out.push_str("\n]}\n");
    out
}

/// Every record of a file at once.
///
/// # Errors
/// The file is broken, or of another version.
pub fn read(text: &str) -> Result<(LogHeader, Vec<ItemRecord>), ReadError> {
    let mut reader = LogReader::new(text.to_owned())?;
    let items = reader.read_items(usize::MAX)?;
    Ok((reader.header, items))
}

/// A file read a few items at a time: [`LogReader::new`] finds where each
/// item is and reads the header; [`LogReader::read_items`] reads the next
/// ones.
#[derive(Debug, Clone)]
pub struct LogReader {
    text: String,
    header: LogHeader,
    items: Vec<Range<usize>>,
    next: usize,
}

impl LogReader {
    /// Finds the items (without reading them) and reads the header.
    ///
    /// # Errors
    /// The file is not a JSON object with an `items` array, its header is
    /// broken, or it is of another version.
    pub fn new(text: String) -> Result<Self, ReadError> {
        // The document without its items: the header's members.
        let (rest, items) = match lines(&text) {
            Some(found) => found,
            None => {
                let (array, items) = scan(text.as_bytes()).ok_or(ReadError::Corrupt)?;
                let rest = format!("{}[]{}", &text[..array.start], &text[array.end..]);
                (rest, items)
            }
        };
        let value = json::parse(&rest).map_err(|_| ReadError::Corrupt)?;
        let version = value.get("version").and_then(JsonValue::as_i64);
        if version != Some(VERSION) {
            return Err(ReadError::UnknownVersion(version));
        }
        let text_of = |name: &str| {
            value
                .get(name)
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .ok_or(ReadError::Corrupt)
        };
        let header = LogHeader {
            version: VERSION,
            character: text_of("character")?,
            world: text_of("world")?,
            saved_unix_ms: value
                .get("saved_unix_ms")
                .and_then(JsonValue::as_i64)
                .ok_or(ReadError::Corrupt)?,
        };
        Ok(Self {
            text,
            header,
            items,
            next: 0,
        })
    }

    /// The file's header.
    #[must_use]
    pub fn header(&self) -> &LogHeader {
        &self.header
    }

    /// How many items are not read yet.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.items.len() - self.next
    }

    /// Reads up to `count` more items.
    ///
    /// # Errors
    /// One of them is broken.
    pub fn read_items(&mut self, count: usize) -> Result<Vec<ItemRecord>, ReadError> {
        let end = self.next.saturating_add(count).min(self.items.len());
        let mut records = Vec::with_capacity(end - self.next);
        for range in &self.items[self.next..end] {
            let value = json::parse(&self.text[range.clone()]).map_err(|_| ReadError::Corrupt)?;
            records.push(ItemRecord::from_json(&value).ok_or(ReadError::Corrupt)?);
        }
        self.next = end;
        Ok(records)
    }
}

/// Where the top-level `items` array is (from its `[` to after its `]`),
/// and each of its elements; `None` when the text is not an object with
/// one. Only the structure is followed (brackets outside strings); each
/// part is read by the strict reader afterwards.
fn scan(b: &[u8]) -> Option<(Range<usize>, Vec<Range<usize>>)> {
    let mut i = skip_ws(b, 0);
    if b.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    let mut found = None;
    loop {
        i = skip_ws(b, i);
        match b.get(i)? {
            b'}' => break,
            b'"' => {}
            _ => return None,
        }
        let key_end = skip_string(b, i)?;
        let key = &b[i + 1..key_end - 1];
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return None;
        }
        i = skip_ws(b, i + 1);
        if key == b"items" && found.is_none() {
            let (end, elements) = scan_array(b, i)?;
            found = Some((i..end, elements));
            i = end;
        } else {
            i = skip_value(b, i)?;
        }
        i = skip_ws(b, i);
        match b.get(i)? {
            b',' => i += 1,
            b'}' => break,
            _ => return None,
        }
    }
    found
}

/// The array starting at `i`: the index after its `]`, and its elements.
fn scan_array(b: &[u8], mut i: usize) -> Option<(usize, Vec<Range<usize>>)> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    i = skip_ws(b, i + 1);
    let mut elements = Vec::new();
    if b.get(i) == Some(&b']') {
        return Some((i + 1, elements));
    }
    loop {
        i = skip_ws(b, i);
        let start = i;
        i = skip_value(b, i)?;
        elements.push(start..i);
        i = skip_ws(b, i);
        match b.get(i)? {
            b',' => i += 1,
            b']' => return Some((i + 1, elements)),
            _ => return None,
        }
    }
}

/// The index after the value starting at `i` (its structure only).
fn skip_value(b: &[u8], mut i: usize) -> Option<usize> {
    let mut depth = 0usize;
    loop {
        match *b.get(i)? {
            b'"' => {
                i = skip_string(b, i)?;
                if depth == 0 {
                    return Some(i);
                }
                continue;
            }
            b'[' | b'{' => depth += 1,
            b']' | b'}' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            b',' if depth == 0 => return Some(i),
            c if depth == 0 && c.is_ascii_whitespace() => return Some(i),
            _ => {}
        }
        i += 1;
    }
}

/// The index after the string whose `"` is at `i`.
fn skip_string(b: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    loop {
        match *b.get(i)? {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b
        .get(i)
        .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
    {
        i += 1;
    }
    i
}

/// The layout [`write`] gives, found by its line breaks alone (a search
/// far cheaper than following the structure): the header on the first
/// line, one item a line, `]}` on the last. The header's members (the
/// first line, its items array closed), and the items; `None` for any
/// other layout.
fn lines(text: &str) -> Option<(String, Vec<Range<usize>>)> {
    let first_end = text.find('\n')?;
    let first = text[..first_end].trim_end_matches('\r');
    if !first.starts_with('{') || !first.ends_with("\"items\":[") {
        return None;
    }
    let mut items = Vec::new();
    let mut start = first_end + 1;
    // Whether the last item read had its comma: all but the last have one.
    let mut comma = None;
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
        let line = text[start..end].trim_end_matches('\r');
        if line == "]}" {
            if comma == Some(true) || !text[end..].trim().is_empty() {
                return None;
            }
            return Some((format!("{first}]}}"), items));
        }
        if comma == Some(false) {
            return None;
        }
        let item = line.strip_suffix(',');
        comma = Some(item.is_some());
        let item = item.unwrap_or(line);
        if item.is_empty() || end == text.len() {
            return None;
        }
        items.push(start..start + item.len());
        start = end + 1;
    }
}
