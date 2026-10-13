//! The .NET string behaviour MossTank's C# relies on, beyond the SDK's
//! compatibility kit (`openac-plugin-compat`, re-exported as [`compat`]):
//!
//! - lengths and offsets in UTF-16 code units, as `string.Length` and
//!   `Substring` count them (profile files carry such lengths);
//! - ordinal and ordinal-ignore-case comparison;
//! - invariant casing, one character to one character as .NET maps them;
//! - `int.TryParse(text, NumberStyles.Integer, InvariantCulture)` and friends;
//! - `Dictionary` and `HashSet` with .NET's enumeration order ([`collections`]).

pub mod collections;

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

/// The first code point of each run of ten decimal digits (Unicode
/// category Nd) in the Basic Multilingual Plane.
const DECIMAL_DIGIT_ZEROS: [u32; 37] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10,
];

/// `char.IsDigit` on one UTF-16 code unit: a decimal digit of any script
/// (category Nd). A character outside the Basic Multilingual Plane is two
/// code units to .NET, neither of them a digit, so it never is one here.
// TODO(compat): the kit's character categories will replace this table.
#[must_use]
pub fn is_decimal_digit(c: char) -> bool {
    let code = u32::from(c);
    DECIMAL_DIGIT_ZEROS
        .iter()
        .any(|&zero| (zero..zero + 10).contains(&code))
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

/// `value.ToString(format, CultureInfo.InvariantCulture)` for a double,
/// adding to the compatibility kit the custom formats that only put literal
/// text before or after their digits ("0.0 x", "~0.00 m"). A literal before
/// the digits follows the minus sign, as in .NET. Anything else the kit does
/// not print is still refused.
///
/// # Errors
/// What the kit refuses, unless the format is digits with literal text
/// around them.
pub fn format_double(value: f64, format: &str) -> Result<String, number::FormatError> {
    let refused = match number::format_f64(value, format) {
        Err(error @ number::FormatError::Unsupported(_)) => error,
        other => return other,
    };
    let placeholder = |c: char| matches!(c, '0' | '#' | '.' | ',');
    let (Some(start), Some(end)) = (format.find(placeholder), format.rfind(placeholder)) else {
        return Err(refused);
    };
    let (prefix, core, suffix) = (&format[..start], &format[start..=end], &format[end + 1..]);
    // Characters with a meaning of their own in a custom format, and an
    // exponent: not plain literal text.
    let special = |c: char| matches!(c, '%' | '\u{2030}' | ';' | '\\' | '\'' | '"' | 'E' | 'e');
    if !core.chars().all(placeholder) || prefix.chars().any(special) || suffix.chars().any(special)
    {
        return Err(refused);
    }
    let digits = number::format_f64(value, core)?;
    if !value.is_finite() {
        // NaN and the infinities print as their symbol alone.
        return Ok(digits);
    }
    Ok(match digits.strip_prefix('-') {
        Some(magnitude) => format!("-{prefix}{magnitude}{suffix}"),
        None => format!("{prefix}{digits}{suffix}"),
    })
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

/// The message of .NET's `FormatException` for a number that does not
/// parse.
#[must_use]
pub fn format_exception_message(text: &str) -> String {
    format!("The input string '{text}' was not in a correct format.")
}

/// The message of .NET's `OverflowException` from a checked conversion.
pub const ARITHMETIC_OVERFLOW: &str = "Arithmetic operation resulted in an overflow.";

/// `int.Parse(text, CultureInfo.InvariantCulture)`, with .NET's messages.
///
/// # Errors
/// The text is not an integer, or does not fit.
pub fn parse_int(text: &str) -> Result<i32, String> {
    if let Some(value) = try_parse_int(text) {
        return Ok(value);
    }
    if number::parse_f64(text, NumberStyles::INTEGER).is_some() {
        Err("Value was either too large or too small for an Int32.".to_owned())
    } else {
        Err(format_exception_message(text))
    }
}

/// `uint.Parse(text, CultureInfo.InvariantCulture)`, with .NET's messages.
///
/// # Errors
/// The text is not an unsigned integer, or does not fit.
pub fn parse_uint(text: &str) -> Result<u32, String> {
    if let Some(value) = try_parse_uint(text) {
        return Ok(value);
    }
    if number::parse_f64(text, NumberStyles::INTEGER).is_some() {
        Err("Value was either too large or too small for a UInt32.".to_owned())
    } else {
        Err(format_exception_message(text))
    }
}

/// `double.Parse(text, NumberStyles.Float, CultureInfo.InvariantCulture)`,
/// with .NET's message.
///
/// # Errors
/// The text is not a number.
pub fn parse_double(text: &str) -> Result<f64, String> {
    number::parse_f64(text, NumberStyles::FLOAT).ok_or_else(|| format_exception_message(text))
}

/// `bool.Parse(text)`: `True` or `False` in any case, surrounding white
/// space and NULs ignored.
///
/// # Errors
/// Anything else, with .NET's message.
pub fn parse_bool(text: &str) -> Result<bool, String> {
    let trimmed = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if eq_ignore_case(trimmed, "True") {
        Ok(true)
    } else if eq_ignore_case(trimmed, "False") {
        Ok(false)
    } else {
        Err(format!(
            "String '{text}' was not recognized as a valid Boolean."
        ))
    }
}

/// `checked((int)value)`: truncation toward zero, an error outside the
/// range or for NaN.
///
/// # Errors
/// .NET's overflow message.
#[allow(clippy::cast_possible_truncation)]
pub fn checked_double_to_int(value: f64) -> Result<i32, String> {
    let truncated = value.trunc();
    if truncated.is_nan() || truncated < f64::from(i32::MIN) || truncated > f64::from(i32::MAX) {
        Err(ARITHMETIC_OVERFLOW.to_owned())
    } else {
        Ok(truncated as i32)
    }
}

/// `checked((int)Math.Round(value, MidpointRounding.AwayFromZero))`.
///
/// # Errors
/// .NET's overflow message.
pub fn round_to_int_checked(value: f64) -> Result<i32, String> {
    checked_double_to_int(round_away_from_zero(value))
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

    // Expected values: .NET 10 `double.ToString(format, InvariantCulture)`.
    #[test]
    fn a_custom_format_may_carry_literal_text_around_its_digits() {
        assert_eq!(format_double(3.5, "0.0 x").unwrap(), "3.5 x");
        assert_eq!(format_double(3.5, "0.0 X").unwrap(), "3.5 X");
        assert_eq!(format_double(-3.5, "~0.00").unwrap(), "-~3.50");
        assert_eq!(format_double(f64::NAN, "0.0 x").unwrap(), "NaN");
        assert_eq!(format_double(3.5, "0.00").unwrap(), "3.50");
        assert!(format_double(3.5, "0 x 0").is_err());
    }

    #[test]
    fn decimal_digits_of_every_script_in_the_basic_plane() {
        assert!(is_decimal_digit('7'));
        assert!(is_decimal_digit('\u{0663}'));
        assert!(is_decimal_digit('\u{FF19}'));
        assert!(!is_decimal_digit('\u{00B2}'));
        assert!(!is_decimal_digit('\u{2167}'));
        assert!(!is_decimal_digit('\u{1D7CE}'));
        assert!(!is_decimal_digit('a'));
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
