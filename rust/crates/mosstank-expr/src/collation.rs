//! `StringComparer.InvariantCulture.Compare`, approximated: listsort's
//! default order for two strings. .NET compares with ICU's root collation;
//! this keeps the parts profiles can see: spaces and punctuation before
//! digits before letters, letters compared without case first, then lower
//! case before upper case, then code unit order (register row EXPR-1).

use std::cmp::Ordering;

fn primary(c: char) -> (u8, u32) {
    if c.is_whitespace() {
        (0, u32::from(c))
    } else if c.is_numeric() {
        (2, c.to_digit(10).unwrap_or(u32::from(c)))
    } else if c.is_alphabetic() {
        (3, u32::from(mosstank_dotnet::lower_char(c)))
    } else {
        (1, u32::from(c))
    }
}

fn tertiary(c: char) -> u8 {
    u8::from(c.is_uppercase())
}

/// The invariant culture's order of two strings.
#[must_use]
pub fn compare_invariant(a: &str, b: &str) -> Ordering {
    let primaries = |text: &str| text.chars().map(primary).collect::<Vec<_>>();
    primaries(a)
        .cmp(&primaries(b))
        .then_with(|| {
            let cases = |text: &str| text.chars().map(tertiary).collect::<Vec<_>>();
            cases(a).cmp(&cases(b))
        })
        .then_with(|| mosstank_dotnet::compare_ordinal(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_before_letters_and_lower_case_first() {
        assert_eq!(compare_invariant("_", "a"), Ordering::Less);
        assert_eq!(compare_invariant("a", "b"), Ordering::Less);
        assert_eq!(compare_invariant("b", "B"), Ordering::Less);
        assert_eq!(compare_invariant("B", "c"), Ordering::Less);
        assert_eq!(compare_invariant("9", "a"), Ordering::Less);
    }
}
