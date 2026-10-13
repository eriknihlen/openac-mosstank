//! The .NET behaviour MossTank's C# relies on beyond the SDK's
//! compatibility kit (`openac-plugin-compat`, re-exported as [`compat`]):
//!
//! - `int.TryParse(text, NumberStyles.Integer, InvariantCulture)` and friends,
//!   and the `Parse` forms with .NET's exception messages;
//! - `char.IsDigit` (the kit has no character categories yet);
//! - `StringReader.ReadLine` ([`LineReader`]);
//! - `Dictionary` and `HashSet` with .NET's enumeration order ([`collections`]).
//!
//! UTF-16 positions, ordinal comparison, invariant casing and the
//! invariant-culture order are the kit's (`compat::text`).

pub mod collections;

pub use openac_plugin_compat as compat;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::text::{ordinal, utf16};

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
    if ordinal::equals_ignore_case(trimmed, "True") {
        Ok(true)
    } else if ordinal::equals_ignore_case(trimmed, "False") {
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
        let (taken, _) = utf16::split_at(rest, units)?;
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

    // Expected values: .NET 10 `double.ToString(format, InvariantCulture)`.
    // `cstrf` and the settings texts print such formats; the kit has covered
    // custom formats with literal text since API 1.0.
    #[test]
    fn the_kit_prints_custom_formats_with_literal_text() {
        let format = |value, format| number::format_f64(value, format).unwrap();
        assert_eq!(format(3.5, "0.0 x"), "3.5 x");
        assert_eq!(format(3.5, "0.0 X"), "3.5 X");
        assert_eq!(format(-3.5, "~0.00"), "-~3.50");
        assert_eq!(format(f64::NAN, "0.0 x"), "NaN");
        assert_eq!(format(3.5, "0.00"), "3.50");
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
}
