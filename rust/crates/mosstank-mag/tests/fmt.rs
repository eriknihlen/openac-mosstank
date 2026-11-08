//! The .NET Framework number formatting the Mag pages print with, in the
//! en-US culture.
//!
//! Source: Mag-Tools `Shared/Util.cs` `NumberFormatter` and the views'
//! `Math.Round(..).ToString("F0")` cells (values worked out from .NET
//! Framework's rules: `Math.Round` halves to even, `N`/`F` take 15
//! significant digits and round half away from zero).

use mosstank_mag::fmt::*;

#[test]
fn round_half_even() {
    assert_eq!(round(62.5, 0), 62.0);
    assert_eq!(round(63.5, 0), 64.0);
    assert_eq!(round(0.25, 1), 0.2);
}

#[test]
fn grouped_thousands() {
    assert_eq!(grouped(1_234_567), "1,234,567");
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(-1234), "-1,234");
}

#[test]
fn n1_and_n2() {
    assert_eq!(n1(-6.0), "-6.0");
    assert_eq!(n1(1234.56), "1,234.6");
    assert_eq!(n1(0.05), "0.1");
    assert_eq!(
        n2(12.345),
        "12.35",
        "15 digits first, then half away from zero"
    );
    assert_eq!(n2(1234.5), "1,234.50");
}

#[test]
fn f0_f1() {
    assert_eq!(f0(round(62.5, 0)), "62");
    assert_eq!(f1(round(12.34, 1)), "12.3");
    assert_eq!(f1(0.0), "0.0");
    // The fixed formats never print a negative zero.
    assert_eq!(f0(-0.4), "0");
    assert_eq!(f1(-0.000_01), "0.0");
    assert_eq!(n1(-0.04), "0.0");
}

#[test]
fn g15_prints_negative_zero_as_zero() {
    assert_eq!(g15(-0.0), "0");
    assert_eq!(g15(round(-0.4, 0)), "0");
    assert_eq!(g15(12.5), "12.5");
    assert_eq!(g15(0.1 + 0.2), "0.3");
}

// Util.NumberFormatter(number, "#,##0", largest)
#[test]
fn number_formatter_shortens_past_the_largest() {
    assert_eq!(number_formatter(5, 99_999), "5");
    assert_eq!(number_formatter(99_999, 99_999), "99,999");
    assert_eq!(number_formatter(123_456, 99_999), "123k");
    // 123,456.789 -> "123,457" is 7 > 6 characters, so millions: "123M"
    assert_eq!(number_formatter(123_456_789, 99_999), "123M");
    assert_eq!(number_formatter(123_456_789_012, 99_999), "123G");
    // float division, rounded half away from zero: 2,500 / 1000 -> "3k"
    assert_eq!(number_formatter(2_500, 999), "3k");
}
