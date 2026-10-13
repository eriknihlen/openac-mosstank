//! The bane profile (`BuffElementProfile`): which elements' banes a
//! profile mode or letters ask for, and each bane's tier-I name.

/// An element a bane guards against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Element {
    Pierce = 0,
    Bludgeon = 1,
    Slash = 2,
    Acid = 3,
    Lightning = 4,
    Cold = 5,
    Fire = 6,
    Physical = 99,
}

/// The banes a profile mode asks for (1: the custom letters).
#[must_use]
pub fn banes(mode: i32, custom_letters: Option<&str>) -> Vec<Element> {
    let letters = match mode {
        1 => custom_letters.unwrap_or(""),
        2 => "All",
        3 => "None",
        4 => "B",
        5 => "BPS",
        6 => "BPSA",
        7 => "BPSAC",
        8 => "ALFC",
        _ => "???",
    };
    parse(Some(letters))
}

/// The elements a letter string names (`All`, `None`, or letters).
#[must_use]
pub fn parse(letters: Option<&str>) -> Vec<Element> {
    let mut result = Vec::with_capacity(7);
    let Some(letters) = letters else {
        return result;
    };
    if letters == "All" {
        result.extend([
            Element::Acid,
            Element::Bludgeon,
            Element::Cold,
            Element::Fire,
            Element::Lightning,
            Element::Pierce,
            Element::Slash,
        ]);
        return result;
    }
    if letters == "None" {
        return result;
    }
    for letter in letters.chars() {
        let element = match letter {
            'A' => Element::Acid,
            'L' => Element::Lightning,
            'F' => Element::Fire,
            'C' => Element::Cold,
            'B' => Element::Bludgeon,
            'P' => Element::Pierce,
            'S' => Element::Slash,
            _ => continue,
        };
        if !result.contains(&element) {
            result.push(element);
        }
    }
    result
}

/// The tier-I bane spell of an element.
#[must_use]
pub fn bane_spell_name(element: Element) -> &'static str {
    match element {
        Element::Acid => "Acid Bane I",
        Element::Bludgeon => "Bludgeon Bane I",
        Element::Cold => "Frost Bane I",
        Element::Fire => "Flame Bane I",
        Element::Lightning => "Lightning Bane I",
        Element::Physical => "Impenetrability I",
        Element::Pierce => "Piercing Bane I",
        Element::Slash => "Blade Bane I",
    }
}
