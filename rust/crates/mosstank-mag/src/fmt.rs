//! .NET Framework number formatting, as Mag-Tools printed its numbers, in
//! the en-US culture (Mag-Tools used the Windows culture; the port fixes
//! en-US).
//!
//! The rules: `Math.Round` halves to even; the fixed and number formats
//! (`F1`, `N1`) take a double to fifteen significant digits and round that
//! half away from zero, never printing a negative zero; a double joined
//! into a string prints fifteen significant digits; a custom format
//! (`#,##0`) on a `float` starts from seven significant digits and rounds
//! half away from zero.

use openac_plugin_compat::number::{self, MidpointRounding, framework};

/// `Math.Round(value, digits)`: halves to even.
///
/// # Panics
/// When `digits` is above 15 (.NET throws).
#[must_use]
pub fn round(value: f64, digits: u32) -> f64 {
    number::round_digits(value, digits, MidpointRounding::ToEven)
}

/// `value.ToString("#,##0")` for a whole number: `1,234,567`.
#[must_use]
pub fn grouped(value: i64) -> String {
    number::format_i64(value, "#,##0").expect("#,##0 is a supported format")
}

/// .NET Framework's fixed or number format of a double.
fn framework_fixed(value: f64, format: &str) -> String {
    framework::fixed(value, format).expect("the fixed formats are supported")
}

/// `value.ToString("N1")`: one decimal, grouped.
#[must_use]
pub fn n1(value: f64) -> String {
    framework_fixed(value, "N1")
}

/// `value.ToString("N2")`: two decimals, grouped.
#[must_use]
pub fn n2(value: f64) -> String {
    framework_fixed(value, "N2")
}

/// `value.ToString("F0")`: no decimals, not grouped.
#[must_use]
pub fn f0(value: f64) -> String {
    framework_fixed(value, "F0")
}

/// `value.ToString("F1")`: one decimal, not grouped.
#[must_use]
pub fn f1(value: f64) -> String {
    framework_fixed(value, "F1")
}

/// `value.ToString()` for a double (`G`, fifteen significant digits); a
/// negative zero prints `0`.
#[must_use]
pub fn g15(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    number::format_f64(value, "G15").expect("G15 is a supported format")
}

/// `((float)value).ToString("#,##0")`.
fn float_grouped(value: f32) -> String {
    number::format_f32(value, "#,##0").expect("#,##0 is a supported format")
}

/// `Util.NumberFormatter(number, "#,##0", largest)`: the number grouped
/// while it is at most `largest`; past it, in thousands (`k`), millions
/// (`M`) or billions (`G`), the first that is no wider than `largest`
/// grouped (the suffix not counted).
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn number_formatter(number: i64, largest: i64) -> String {
    if number <= largest {
        return grouped(number);
    }
    let spaces = grouped(largest).len();
    let as_float = number as f32;
    let thousands = float_grouped(as_float / 1000.0);
    if thousands.len() <= spaces {
        return thousands + "k";
    }
    let millions = float_grouped(as_float / 1_000_000.0);
    if millions.len() <= spaces {
        return millions + "M";
    }
    float_grouped(as_float / 1_000_000_000.0) + "G"
}
