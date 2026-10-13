//! The .NET string behaviour MossTank's C# relies on, beyond the SDK's
//! compatibility kit (`openac-plugin-compat`, re-exported as [`compat`]):
//!
//! - lengths and offsets in UTF-16 code units, as `string.Length` and
//!   `Substring` count them (profile files carry such lengths);
//! - ordinal and ordinal-ignore-case comparison;
//! - invariant casing, one character to one character as .NET maps them;
//! - `int.TryParse(text, NumberStyles.Integer, InvariantCulture)` and friends.

use std::cmp::Ordering;

pub use openac_plugin_compat as compat;
use openac_plugin_compat::number::{self, NumberStyles};

/// `string.Length`: the length in UTF-16 code units.
#[must_use]
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The byte offset in `text` of UTF-16 offset `units`, or `None` when
/// `units` is past the end or falls between the two halves of a surrogate
/// pair (a split C# would make and Rust strings cannot hold).
#[must_use]
pub fn byte_offset_of_utf16(text: &str, units: usize) -> Option<usize> {
    let mut counted = 0usize;
    for (offset, c) in text.char_indices() {
        if counted == units {
            return Some(offset);
        }
        counted += c.len_utf16();
        if counted > units {
            return None;
        }
    }
    (counted == units).then_some(text.len())
}

/// Splits `text` after `units` UTF-16 code units (`Substring(0, units)` and
/// the rest), or `None` as [`byte_offset_of_utf16`].
#[must_use]
pub fn split_at_utf16(text: &str, units: usize) -> Option<(&str, &str)> {
    byte_offset_of_utf16(text, units).map(|offset| text.split_at(offset))
}

/// `string.CompareOrdinal`: by UTF-16 code unit.
#[must_use]
pub fn compare_ordinal(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `StringComparer.OrdinalIgnoreCase.Compare`: by UTF-16 code unit after
/// the invariant upper-case mapping of each character.
#[must_use]
pub fn compare_ordinal_ignore_case(a: &str, b: &str) -> Ordering {
    let upper = |text: &str| -> Vec<u16> { to_upper_invariant(text).encode_utf16().collect() };
    upper(a).cmp(&upper(b))
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`.
#[must_use]
pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars().map(upper_char).eq(b.chars().map(upper_char))
}

/// `char.ToUpperInvariant`: one character to one character; a character
/// whose full upper case is longer (`ß`) stays as it is, as in .NET.
#[must_use]
pub fn upper_char(c: char) -> char {
    let mut mapped = c.to_uppercase();
    match (mapped.next(), mapped.next()) {
        (Some(upper), None) => upper,
        _ => c,
    }
}

/// `char.ToLowerInvariant`, one character to one character.
#[must_use]
pub fn lower_char(c: char) -> char {
    let mut mapped = c.to_lowercase();
    match (mapped.next(), mapped.next()) {
        (Some(lower), None) => lower,
        _ => c,
    }
}

/// `string.ToUpperInvariant`.
#[must_use]
pub fn to_upper_invariant(text: &str) -> String {
    text.chars().map(upper_char).collect()
}

/// `string.ToLowerInvariant`.
#[must_use]
pub fn to_lower_invariant(text: &str) -> String {
    text.chars().map(lower_char).collect()
}

/// `string.IsNullOrWhiteSpace` for a present string.
#[must_use]
pub fn is_white_space(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// `int.TryParse(text, NumberStyles.Integer, CultureInfo.InvariantCulture)`.
#[must_use]
pub fn try_parse_int(text: &str) -> Option<i32> {
    number::parse_i32(text, NumberStyles::INTEGER)
}

/// `uint.TryParse(text, NumberStyles.Integer, CultureInfo.InvariantCulture)`.
#[must_use]
pub fn try_parse_uint(text: &str) -> Option<u32> {
    number::parse_u32(text, NumberStyles::INTEGER)
}

/// `long.TryParse(text, NumberStyles.Integer, CultureInfo.InvariantCulture)`.
#[must_use]
pub fn try_parse_long(text: &str) -> Option<i64> {
    number::parse_i64(text, NumberStyles::INTEGER)
}

/// `double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture)`.
#[must_use]
pub fn try_parse_double(text: &str) -> Option<f64> {
    number::parse_f64(text, NumberStyles::FLOAT)
}

/// `float.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture)`.
#[must_use]
pub fn try_parse_float(text: &str) -> Option<f32> {
    number::parse_f32(text, NumberStyles::FLOAT)
}

/// `value.ToString("G15", CultureInfo.InvariantCulture)`.
#[must_use]
pub fn format_g15(value: f64) -> String {
    number::format_f64(value, "G15").expect("G15 is a supported format")
}

/// `value.ToString(CultureInfo.InvariantCulture)` for a double.
#[must_use]
pub fn double_to_string(value: f64) -> String {
    number::to_string_f64(value)
}

/// `Math.Round(value, MidpointRounding.AwayFromZero)`.
#[must_use]
pub fn round_away_from_zero(value: f64) -> f64 {
    number::round_mode(value, number::MidpointRounding::AwayFromZero)
}

/// `(int)value`: truncation toward zero, saturating at the ends and NaN
/// giving 0, as .NET 9 and later convert (and as Rust's `as` does).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn double_to_int(value: f64) -> i32 {
    value as i32
}

/// Reads lines as `StringReader.ReadLine` does: a line ends at `\r\n`, `\r`
/// or `\n`, and the break is not part of the line.
#[derive(Debug, Clone)]
pub struct LineReader<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> LineReader<'a> {
    /// A reader at the start of `text`.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    /// Whether everything has been read.
    #[must_use]
    pub fn at_end(&self) -> bool {
        self.position >= self.text.len()
    }

    /// The next line, or `None` at the end.
    pub fn read_line(&mut self) -> Option<&'a str> {
        if self.at_end() {
            return None;
        }
        let rest = &self.text[self.position..];
        let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
        let line = &rest[..end];
        let mut consumed = end;
        let bytes = rest.as_bytes();
        if consumed < bytes.len() && bytes[consumed] == b'\r' {
            consumed += 1;
        }
        if consumed < bytes.len() && bytes[consumed] == b'\n' {
            consumed += 1;
        }
        self.position += consumed;
        Some(line)
    }

    /// The next `units` UTF-16 code units as they are, or `None` when fewer
    /// remain.
    pub fn read_utf16(&mut self, units: usize) -> Option<&'a str> {
        let rest = &self.text[self.position..];
        let (taken, _) = split_at_utf16(rest, units)?;
        self.position += taken.len();
        Some(taken)
    }

    /// What has not been read yet.
    #[must_use]
    pub fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_count_surrogate_pairs_as_two() {
        let text = "a\u{1F600}b";
        assert_eq!(utf16_len(text), 4);
        assert_eq!(byte_offset_of_utf16(text, 1), Some(1));
        assert_eq!(byte_offset_of_utf16(text, 2), None);
        assert_eq!(byte_offset_of_utf16(text, 3), Some(5));
        assert_eq!(byte_offset_of_utf16(text, 4), Some(6));
        assert_eq!(byte_offset_of_utf16(text, 5), None);
    }

    #[test]
    fn line_reader_ends_lines_at_crlf_cr_and_lf() {
        let mut reader = LineReader::new("a\r\nb\rc\nd");
        assert_eq!(reader.read_line(), Some("a"));
        assert_eq!(reader.read_line(), Some("b"));
        assert_eq!(reader.read_line(), Some("c"));
        assert_eq!(reader.read_line(), Some("d"));
        assert_eq!(reader.read_line(), None);
    }

    #[test]
    fn invariant_casing_keeps_one_character_per_character() {
        assert_eq!(to_upper_invariant("straße"), "STRAßE");
        assert!(eq_ignore_case("Adja's Gift", "ADJA'S GIFT"));
        assert_eq!(
            compare_ordinal_ignore_case("apple", "Banana"),
            Ordering::Less
        );
        assert_eq!(compare_ordinal("Zebra", "apple"), Ordering::Less);
    }
}
