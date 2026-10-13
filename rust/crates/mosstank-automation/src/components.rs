//! The profile's blacklist of spell components (`SpellComponentPolicy`):
//! a spell is off when any of its formula's components is named in the
//! setting, by name, by component id or by weenie class id (decimal, or hex
//! written `0x...`).

use ac_plugin_abi::magic::SpellInfo;
use mosstank_dotnet::is_decimal_digit;
use mosstank_dotnet::text::{casing, ordinal};

use crate::spells::SpellCatalog;

/// Whether the spell needs a component the setting blacklists.
pub fn uses_blacklisted_component(
    catalog: &mut dyn SpellCatalog,
    spell: &SpellInfo,
    setting: &str,
) -> bool {
    if casing::is_white_space_only(setting) || spell.formula_component_ids.is_empty() {
        return false;
    }
    for &component_id in &spell.formula_component_ids {
        if contains_number(setting, component_id) {
            return true;
        }
        let Some(component) = catalog.try_get_component(component_id) else {
            continue;
        };
        if !component.name.is_empty() && ordinal::contains_ignore_case(setting, &component.name) {
            return true;
        }
        if contains_number(setting, component.weenie_class_id) {
            return true;
        }
    }
    false
}

/// The number written in decimal and standing alone among digits, or in
/// hex after `0x` (ignoring case).
fn contains_number(setting: &str, value: u32) -> bool {
    if value == 0 {
        return false;
    }
    contains_delimited(setting, &value.to_string())
        || ordinal::contains_ignore_case(setting, &format!("0x{value:X}"))
}

/// The digits occur with no decimal digit right before or after them.
fn contains_delimited(text: &str, token: &str) -> bool {
    let mut start = 0;
    while let Some(found) = text[start..].find(token) {
        let begin = start + found;
        let end = begin + token.len();
        let left = text[..begin]
            .chars()
            .next_back()
            .is_none_or(|c| !is_decimal_digit(c));
        let right = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_decimal_digit(c));
        if left && right {
            return true;
        }
        start = end;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::contains_delimited;

    #[test]
    fn a_number_inside_a_longer_one_is_not_named() {
        assert!(contains_delimited("7, 26", "7"));
        assert!(!contains_delimited("17, 260", "26"));
        assert!(contains_delimited("17, 260 26", "26"));
    }
}
