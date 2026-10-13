//! The compatibility kit's ordinal comparisons and invariant casing
//! (`compat::text::{ordinal, casing}`), answered directly when every
//! character is ASCII.
//!
//! The kit folds each text to UTF-16 upper case before it compares (a new
//! buffer per call, each character looked up in .NET's tables), which is
//! the exact rule for any text; for ASCII text the rule is the ASCII upper
//! case, which needs neither. MossTank compares spell, monster and item
//! names, nearly all ASCII, thousands of times in a pass, and in the
//! client's interpreter the kit's way costs some hundreds of instructions a
//! character. Each function here answers exactly what the kit answers (the
//! tests compare them); a text with any other character goes to the kit.
//! The modules re-export the rest of the kit's, so they replace it in an
//! import.

/// `compat::text::ordinal`, its comparisons answered directly for ASCII.
pub mod ordinal {
    use std::cmp::Ordering;

    use openac_plugin_compat::text::ordinal as kit;
    pub use openac_plugin_compat::text::ordinal::*;

    fn both_ascii(a: &str, b: &str) -> bool {
        a.is_ascii() && b.is_ascii()
    }

    /// `string.CompareOrdinal(a, b)` (the sign).
    #[must_use]
    pub fn compare(a: &str, b: &str) -> Ordering {
        if both_ascii(a, b) {
            return a.as_bytes().cmp(b.as_bytes());
        }
        kit::compare(a, b)
    }

    /// `string.Compare(a, b, StringComparison.OrdinalIgnoreCase)` (the
    /// sign).
    #[must_use]
    pub fn compare_ignore_case(a: &str, b: &str) -> Ordering {
        if both_ascii(a, b) {
            return a
                .bytes()
                .map(|b| b.to_ascii_uppercase())
                .cmp(b.bytes().map(|b| b.to_ascii_uppercase()));
        }
        kit::compare_ignore_case(a, b)
    }

    /// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`.
    #[must_use]
    pub fn equals_ignore_case(a: &str, b: &str) -> bool {
        if both_ascii(a, b) {
            return a.eq_ignore_ascii_case(b);
        }
        kit::equals_ignore_case(a, b)
    }

    /// Where `needle` first (or last) starts in `haystack` ignoring ASCII
    /// case, in bytes (UTF-16 units: the texts are ASCII).
    fn find_ascii(haystack: &[u8], needle: &[u8], last: bool) -> Option<usize> {
        if needle.len() > haystack.len() {
            return None;
        }
        let at =
            |start: &usize| haystack[*start..*start + needle.len()].eq_ignore_ascii_case(needle);
        let starts = 0..=haystack.len() - needle.len();
        if last {
            starts.rev().find(at)
        } else {
            starts.clone().find(at)
        }
    }

    /// `haystack.IndexOf(needle, StringComparison.OrdinalIgnoreCase)`.
    #[must_use]
    pub fn index_of_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
        if both_ascii(haystack, needle) {
            return find_ascii(haystack.as_bytes(), needle.as_bytes(), false);
        }
        kit::index_of_ignore_case(haystack, needle)
    }

    /// `haystack.LastIndexOf(needle, StringComparison.OrdinalIgnoreCase)`.
    #[must_use]
    pub fn last_index_of_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
        if both_ascii(haystack, needle) {
            return find_ascii(haystack.as_bytes(), needle.as_bytes(), true);
        }
        kit::last_index_of_ignore_case(haystack, needle)
    }

    /// `text.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)`.
    #[must_use]
    pub fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
        if both_ascii(text, prefix) {
            return text.len() >= prefix.len()
                && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes());
        }
        kit::starts_with_ignore_case(text, prefix)
    }

    /// `text.EndsWith(suffix, StringComparison.OrdinalIgnoreCase)`.
    #[must_use]
    pub fn ends_with_ignore_case(text: &str, suffix: &str) -> bool {
        if both_ascii(text, suffix) {
            return text.len() >= suffix.len()
                && text.as_bytes()[text.len() - suffix.len()..]
                    .eq_ignore_ascii_case(suffix.as_bytes());
        }
        kit::ends_with_ignore_case(text, suffix)
    }

    /// `text.Contains(value, StringComparison.OrdinalIgnoreCase)`.
    #[must_use]
    pub fn contains_ignore_case(text: &str, value: &str) -> bool {
        index_of_ignore_case(text, value).is_some()
    }
}

/// `compat::text::casing`, its invariant casing answered directly for
/// ASCII.
pub mod casing {
    use openac_plugin_compat::text::casing as kit;
    pub use openac_plugin_compat::text::casing::*;

    /// `string.ToUpperInvariant`.
    #[must_use]
    pub fn to_upper_invariant(text: &str) -> String {
        if text.is_ascii() {
            return text.to_ascii_uppercase();
        }
        kit::to_upper_invariant(text)
    }

    /// `string.ToLowerInvariant`.
    #[must_use]
    pub fn to_lower_invariant(text: &str) -> String {
        if text.is_ascii() {
            return text.to_ascii_lowercase();
        }
        kit::to_lower_invariant(text)
    }
}

#[cfg(test)]
mod tests {
    use openac_plugin_compat::text::{casing as kit_casing, ordinal as kit};

    use super::{casing, ordinal};

    const TEXTS: [&str; 22] = [
        "",
        "a",
        "A",
        "ab",
        "Incantation of Frost Bolt VII",
        "incantation OF frost bolt vii",
        "Fire Vulnerability Other VI",
        " Vulnerability Other",
        "vulnerability",
        "Imperil Other",
        "imperil",
        "Straße",
        "STRASSE",
        "\u{17F}tar",
        "Star",
        "STAR",
        "\u{131}nk",
        "Ink",
        "Kelvin \u{212A}",
        "kelvin k",
        "Mosswart Kin",
        "[Z]_^`{",
    ];

    #[test]
    fn the_ascii_answers_are_the_kits() {
        for a in TEXTS {
            assert_eq!(
                casing::to_upper_invariant(a),
                kit_casing::to_upper_invariant(a)
            );
            assert_eq!(
                casing::to_lower_invariant(a),
                kit_casing::to_lower_invariant(a)
            );
            for b in TEXTS {
                let pair = format!("{a:?} {b:?}");
                assert_eq!(ordinal::compare(a, b), kit::compare(a, b), "{pair}");
                assert_eq!(
                    ordinal::compare_ignore_case(a, b),
                    kit::compare_ignore_case(a, b),
                    "{pair}"
                );
                assert_eq!(
                    ordinal::equals_ignore_case(a, b),
                    kit::equals_ignore_case(a, b),
                    "{pair}"
                );
                assert_eq!(
                    ordinal::index_of_ignore_case(a, b),
                    kit::index_of_ignore_case(a, b),
                    "{pair}"
                );
                assert_eq!(
                    ordinal::last_index_of_ignore_case(a, b),
                    kit::last_index_of_ignore_case(a, b),
                    "{pair}"
                );
                assert_eq!(
                    ordinal::starts_with_ignore_case(a, b),
                    kit::starts_with_ignore_case(a, b),
                    "{pair}"
                );
                assert_eq!(
                    ordinal::ends_with_ignore_case(a, b),
                    kit::ends_with_ignore_case(a, b),
                    "{pair}"
                );
                assert_eq!(
                    ordinal::contains_ignore_case(a, b),
                    kit::contains_ignore_case(a, b),
                    "{pair}"
                );
            }
        }
    }

    #[test]
    fn every_ascii_character_folds_as_the_kit_folds_it() {
        let all: String = (0u8..128).map(char::from).collect();
        assert_eq!(
            casing::to_upper_invariant(&all),
            kit_casing::to_upper_invariant(&all)
        );
        assert_eq!(
            casing::to_lower_invariant(&all),
            kit_casing::to_lower_invariant(&all)
        );
        for code in 0u8..128 {
            let c = char::from(code);
            assert_eq!(
                kit_casing::ordinal_ignore_case_key(c),
                c.to_ascii_uppercase()
            );
            for other in 0u8..128 {
                let (a, b) = (c.to_string(), char::from(other).to_string());
                assert_eq!(
                    ordinal::compare_ignore_case(&a, &b),
                    kit::compare_ignore_case(&a, &b)
                );
                assert_eq!(ordinal::compare(&a, &b), kit::compare(&a, &b));
            }
        }
    }
}
