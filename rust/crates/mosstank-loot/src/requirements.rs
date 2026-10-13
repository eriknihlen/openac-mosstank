//! VTClassic's typed loot requirements, executed as an AND set
//! (`VtankLootRequirementEvaluator`): what each requirement type reads off
//! the item, its appraisal and the character, which of them can decide
//! before an appraisal, and the keys the client cannot supply.
//!
//! A requirement that does not read (a truncated payload, a number that is
//! not one, a pattern that does not compile, a value out of range) is a
//! non-match with the reason in words, as the C#'s caught exceptions are.

use ac_plugin_abi::items::{InventoryItem, Palette};
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_automation::Automation;
use mosstank_dotnet::{
    ARITHMETIC_OVERFLOW, parse_bool, try_parse_double, try_parse_int, try_parse_uint,
};
use mosstank_formats::utl::{LootRule, VtankLootRequirement};
use openac_plugin_compat::regex::{DotNetRegex, RegexError, RegexOptions};

/// Why a requirement could not be read (the C#'s exception message).
pub type RequirementError = String;

type Outcome<T> = Result<T, RequirementError>;

const VTANK_INT_BASE: u32 = 218_103_808;
const VTANK_DOUBLE_BASE: u32 = 167_772_160;
const PLURAL_NAME_KEY: u32 = 184_549_376;

/// The item key whose bit 0 is the "magical" icon highlight.
const ICON_HIGHLIGHT_KEY: u32 = VTANK_INT_BASE + 16;

/// Spells that add to an int key: (spell, key, bonus).
const INT_SPELL_BONUSES: [(u32, u32, i32); 8] = [
    (2598, VTANK_INT_BASE + 34, 2),
    (2586, VTANK_INT_BASE + 34, 4),
    (4661, VTANK_INT_BASE + 34, 7),
    (6089, VTANK_INT_BASE + 34, 10),
    (2604, 28, 20),
    (2592, 28, 40),
    (4667, 28, 60),
    (6095, 28, 80),
];

/// Spells that change a double key: (spell, key, change, bonus). Change and
/// bonus are two separate authored fields. Only the bonus is ever applied;
/// the change truncated to an int selects the operation, so anything in
/// [1, 2) multiplies and everything else adds.
const DOUBLE_SPELL_BONUSES: [(u32, u32, f64, f64); 19] = [
    (3251, 152, 0.01, 0.01),
    (3250, 152, 0.03, 0.03),
    (4670, 152, 0.05, 0.05),
    (6098, 152, 0.07, 0.07),
    (2603, VTANK_DOUBLE_BASE + 12, 0.03, 0.03),
    (2591, VTANK_DOUBLE_BASE + 12, 0.05, 0.05),
    (4666, VTANK_DOUBLE_BASE + 12, 0.07, 0.07),
    (6094, VTANK_DOUBLE_BASE + 12, 0.09, 0.09),
    (2600, 29, 0.03, 0.03),
    (3985, 29, 0.04, 0.04),
    (2588, 29, 0.05, 0.05),
    (4663, 29, 0.07, 0.07),
    (6091, 29, 0.09, 0.09),
    (3201, 144, 1.05, 1.05),
    (3199, 144, 1.10, 1.10),
    (3202, 144, 1.15, 1.15),
    (3200, 144, 1.20, 1.20),
    (6086, 144, 1.25, 1.25),
    (6087, 144, 1.30, 1.30),
];

/// The int keys whose value the server sends with the object itself; every
/// other key only arrives with an appraisal.
const NON_IDENTIFIED_INT_KEYS: [u32; 31] = [
    10,
    19,
    91,
    92,
    131,
    VTANK_INT_BASE,
    VTANK_INT_BASE + 1,
    VTANK_INT_BASE + 2,
    VTANK_INT_BASE + 4,
    VTANK_INT_BASE + 5,
    VTANK_INT_BASE + 6,
    VTANK_INT_BASE + 7,
    VTANK_INT_BASE + 9,
    VTANK_INT_BASE + 12,
    VTANK_INT_BASE + 13,
    VTANK_INT_BASE + 14,
    VTANK_INT_BASE + 15,
    VTANK_INT_BASE + 16,
    VTANK_INT_BASE + 17,
    VTANK_INT_BASE + 18,
    VTANK_INT_BASE + 20,
    VTANK_INT_BASE + 24,
    VTANK_INT_BASE + 25,
    VTANK_INT_BASE + 26,
    VTANK_INT_BASE + 27,
    VTANK_INT_BASE + 35,
    VTANK_INT_BASE + 36,
    VTANK_INT_BASE + 37,
    VTANK_INT_BASE + 38,
    VTANK_INT_BASE + 41,
    VTANK_INT_BASE + 42,
];

/// Int keys the reference macro fills from the server that the host does
/// not hand a plugin, so a rule on one reads zero here where VTank would
/// read the object's value.
const UNSUPPORTED_INT_KEYS: [(u32, &str); 12] = [
    (95, "RadarBlipColor"),
    (133, "RadarVisibility"),
    (VTANK_INT_BASE + 12, "Monarch"),
    (VTANK_INT_BASE + 20, "HookMask"),
    (VTANK_INT_BASE + 24, "CreateFlags1"),
    (VTANK_INT_BASE + 25, "CreateFlags2"),
    (VTANK_INT_BASE + 38, "Unknown8000000"),
    (VTANK_INT_BASE + 39, "PhysicsDataFlags"),
    (VTANK_INT_BASE + 43, "EquippedBy"),
    (VTANK_INT_BASE + 44, "LastAttacker"),
    (VTANK_INT_BASE + 45, "AllegianceObject"),
    (VTANK_INT_BASE + 46, "OwnedBy"),
];

/// The palette slots each named armor colour region reads.
const ARMOR_COLOR_SLOTS: [(&str, &[i32]); 63] = [
    ("Amuli Coat (Chest)", &[0]),
    ("Amuli Coat (Collar/Shoulder)", &[1, 2]),
    ("Amuli Coat (Arms/Trim)", &[3, 4, 5, 6, 7]),
    ("Amuli Legs (Base)", &[0, 1]),
    ("Amuli Legs (Trim)", &[2, 3]),
    ("Celdon (Base)", &[0]),
    ("Celdon (Veins)", &[1, 2]),
    ("Chiran Coat (Base/Arms)", &[0, 1]),
    ("Chiran Coat (Stripes)", &[2, 3, 4]),
    ("Chiran Legs (Girth)", &[1]),
    ("Chiran Legs (Legs)", &[2, 3]),
    ("Chiran Legs (Trim)", &[0]),
    ("Chiran Helm (Horns)", &[0]),
    ("Chiran Helm (Base)", &[1]),
    ("Haebrean BP (Chest) *", &[0]),
    ("Haebrean BP (Ornaments)", &[1]),
    ("Haebrean BP (Trim)", &[2]),
    ("Haebrean Girth (Base) *", &[0]),
    ("Haebrean Girth (Belt/Scales)", &[1, 2]),
    ("Haebrean Helm (Base)", &[0]),
    ("Haebrean Helm (Mask)", &[1]),
    ("Haebrean Pauldrons (Base) *", &[0]),
    ("Haebrean Pauldrons (Ornaments)", &[1]),
    ("Lorica BP (Veins)", &[0, 1]),
    ("Lorica BP (Base)", &[2, 3]),
    ("Lorica BP (Neck/Trim) *", &[4]),
    ("Lorica Legs (Base)", &[0]),
    ("Lorica Legs (Knees/Belt/Crotch) *", &[1, 2]),
    ("Lorica Legs (Legs) *", &[3]),
    ("Nariyid BP (Circle/Lines)", &[0, 1]),
    ("Nariyid BP (Base)", &[2]),
    ("Nariyid BP (Shoulders)", &[3]),
    ("Nariyid Girth (Base) *", &[0]),
    ("Nariyid Girth (Belt/Lines)", &[2]),
    ("Nariyid Girth (Ornaments)", &[3]),
    ("Nariyid Sleeves (Shoulders)", &[0]),
    ("Nariyid Sleeves (Upper Arm)", &[1, 2]),
    ("Nariyid Sleeves (Lower Arm)", &[3]),
    ("Olthoi BP (Base)", &[0]),
    ("Olthoi BP (Veins)", &[1]),
    ("Olthoi Alduressa Legs (Girth: Base)", &[0, 1, 2]),
    ("Olthoi Alduressa Legs (Girth: Lines)", &[3]),
    ("Olthoi Alduressa Legs (Legs: Lines)", &[4, 5]),
    ("Olthoi Amuli Coat (Base) *", &[0, 1]),
    ("Olthoi Amuli Coat (Trim)", &[2]),
    ("Olthoi Amuli Coat (Shoulders)", &[3]),
    ("Olthoi Amuli Legs (Trim)", &[6, 7, 8]),
    ("Olthoi Koujia Kabuton (Base)", &[0]),
    ("Olthoi Koujia Kabuton (Horns)", &[1]),
    ("Olthoi Koujia Legs (Base)", &[0, 1, 2]),
    ("Olthoi Koujia Legs (Sides/Shins)", &[3, 4, 5]),
    ("Scalemail Cuirass (Base)", &[0]),
    ("Scalemail Cuirass (Bumps)", &[1]),
    ("Scalemail Cuirass (Belt)", &[2]),
    ("Tenassa Legs (Line at Side)", &[0]),
    ("Tenassa Legs (Base)", &[1]),
    ("Tenassa Legs (Hilight)", &[2]),
    ("Tenassa BP (Shoulders)", &[0]),
    ("Tenassa BP (Base)", &[1]),
    ("Yoroi Cuirass (Base)", &[0, 1]),
    ("Yoroi Cuirass (Belt)", &[2]),
    ("Yoroi Girth (Base)", &[0]),
    ("Yoroi Girth (Belt)", &[1]),
];

/// The key a requirement reads when it is one MossTank cannot supply, with
/// that key's name.
#[must_use]
pub fn try_unsupported_key(requirement: &VtankLootRequirement) -> Option<(u32, &'static str)> {
    if !matches!(requirement.kind, 2 | 3 | 11 | 12 | 13 | 2003) {
        return None;
    }
    let values = lines(&requirement.payload);
    if values.len() <= 1 {
        return None;
    }
    let key = try_parse_uint(values[1])?;
    UNSUPPORTED_INT_KEYS
        .iter()
        .find(|(unsupported, _)| *unsupported == key)
        .map(|(_, name)| (key, *name))
}

/// One warning naming every enabled rule that reads a key MossTank cannot
/// supply, or `None` when there is none. Such a rule is decided on a zero,
/// so without the warning it would fail without a word.
#[must_use]
pub fn unsupported_key_warning<'a>(
    profile_name: &str,
    rules: impl IntoIterator<Item = &'a LootRule>,
) -> Option<String> {
    const NAMED: usize = 5;
    let mut uses = Vec::new();
    for rule in rules {
        if is_disabled(rule) {
            continue;
        }
        for requirement in &rule.vtank_requirements {
            if let Some((key, name)) = try_unsupported_key(requirement) {
                uses.push(format!("\"{}\" reads {name} ({key})", net_trim(&rule.name)));
            }
        }
    }
    if uses.is_empty() {
        return None;
    }
    let mut named = uses
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if uses.len() > NAMED {
        named.push_str(&format!("; and {} more", uses.len() - NAMED));
    }
    Some(format!(
        "Loot profile {profile_name}: {} enabled rule requirement(s) read a value MossTank \
         cannot get from the client yet, so they will not match as in VTank: {named}.",
        uses.len()
    ))
}

/// VTClassic's "disabled" requirement: a rule carrying it set to true never
/// matches.
fn is_disabled(rule: &LootRule) -> bool {
    rule.vtank_requirements.iter().any(|requirement| {
        requirement.kind == 9999
            && parse_bool(net_trim(lines(&requirement.payload)[0])).unwrap_or(false)
    })
}

/// Whether an int key arrives only with an appraisal.
#[must_use]
pub fn is_identified_int_key(key: u32) -> bool {
    !NON_IDENTIFIED_INT_KEYS.contains(&key)
}

/// Whether a string key arrives only with an appraisal.
#[must_use]
pub fn is_identified_string_key(key: u32) -> bool {
    key != 1 && key != PLURAL_NAME_KEY
}

/// Whether a double key arrives only with an appraisal.
#[must_use]
pub fn is_identified_double_key(key: u32) -> bool {
    key != VTANK_DOUBLE_BASE + 8 && key != VTANK_DOUBLE_BASE + 9
}

/// True when the server has flagged the item as magical, which is the only
/// pre-appraisal signal that it is expected to carry spells.
#[must_use]
pub fn is_magical(item: &InventoryItem, properties: &ItemProperties) -> bool {
    // The highlight key reads the item's effects, which cannot fail to
    // convert unless they overflow an int; an overflow reads as no flag.
    int_value(ICON_HIGHLIGHT_KEY, item, properties).is_ok_and(|value| value & 1 != 0)
}

/// Whether one requirement can decide without appraisal data, and when it
/// can, whether it matches: `(has_decision, is_match)`. A requirement that
/// cannot decide leaves the rule open.
#[must_use]
pub fn early_match(
    requirement: &VtankLootRequirement,
    item: &InventoryItem,
    properties: &ItemProperties,
    host: Option<&mut (dyn Automation + '_)>,
) -> (bool, bool) {
    early_match_inner(requirement, item, properties, host).unwrap_or((true, false))
}

fn early_match_inner(
    requirement: &VtankLootRequirement,
    item: &InventoryItem,
    properties: &ItemProperties,
    host: Option<&mut (dyn Automation + '_)>,
) -> Outcome<(bool, bool)> {
    let values = lines(&requirement.payload);
    let has_decision = match requirement.kind {
        // Spell requirements: undecided only while the item claims to be
        // magical, because then spell data is still to come.
        0 | 8 | 9 => return Ok((!is_magical(item, properties), false)),
        // Weapon-damage requirements: undecided only for the weapon class
        // the requirement can apply to.
        10 | 2008 => return Ok((item.object_class != ObjectClass::MeleeWeapon, false)),
        2000 | 2006 => {
            return Ok((
                item.object_class != ObjectClass::MeleeWeapon
                    && item.object_class != ObjectClass::MissileWeapon,
                false,
            ));
        }
        2001 => return Ok((item.object_class != ObjectClass::MissileWeapon, false)),
        // Buffed and rating requirements can never decide early.
        2003 | 2005 | 2007 => return Ok((false, false)),
        // Retired, disabled, and preserved-unknown requirements are a
        // decided non-match whatever the item is.
        6 | 9999 | -1 => return Ok((true, false)),
        1 => !is_identified_string_key(u32_at(&values, 1)?),
        2 | 3 | 11..=13 => !is_identified_int_key(u32_at(&values, 1)?),
        4 | 5 => !is_identified_double_key(u32_at(&values, 1)?),
        _ => true,
    };
    let is_match = has_decision && matches_one(requirement, item, properties, host)?;
    Ok((has_decision, is_match))
}

/// Whether every requirement matches; with the reason when one could not
/// be read (and the answer is then no).
#[must_use]
pub fn is_match(
    requirements: &[VtankLootRequirement],
    item: &InventoryItem,
    properties: &ItemProperties,
    mut host: Option<&mut (dyn Automation + '_)>,
) -> (bool, Option<RequirementError>) {
    for requirement in requirements {
        match matches_one(requirement, item, properties, host.as_deref_mut()) {
            Ok(true) => {}
            Ok(false) => return (false, None),
            Err(error) => return (false, Some(error)),
        }
    }
    (true, None)
}

fn matches_one(
    requirement: &VtankLootRequirement,
    item: &InventoryItem,
    properties: &ItemProperties,
    host: Option<&mut (dyn Automation + '_)>,
) -> Outcome<bool> {
    let values = lines(&requirement.payload);
    let values = values.as_slice();
    Ok(match requirement.kind {
        0 => {
            let pattern = rx(values, 0)?;
            let mut any = false;
            for name in spell_names(item, host) {
                if regex_match(&pattern, &name)? {
                    any = true;
                    break;
                }
            }
            any
        }
        1 => {
            let pattern = rx(values, 0)?;
            let text = string_value(u32_at(values, 1)?, item, properties);
            regex_match(&pattern, &text)?
        }
        2 => int_value(u32_at(values, 1)?, item, properties)? <= i32_at(values, 0)?,
        3 => int_value(u32_at(values, 1)?, item, properties)? >= i32_at(values, 0)?,
        #[allow(clippy::cast_possible_truncation)]
        4 => {
            (double_value(u32_at(values, 1)?, item, properties) as f32)
                <= (f64_at(values, 0)? as f32)
        }
        #[allow(clippy::cast_possible_truncation)]
        5 => {
            (double_value(u32_at(values, 1)?, item, properties) as f32)
                >= (f64_at(values, 0)? as f32)
        }
        // VTClassic deliberately retired this requirement; its own Match
        // method always returns false.
        6 => false,
        7 => i32::from(item.object_class.tag()) == i32_at(values, 0)?,
        8 => item.appraised_spell_ids.len() as i64 >= i64::from(i32_at(values, 0)?),
        9 => spell_match(values, item, host)?,
        10 => minimum_damage(item) >= f64_at(values, 0)?,
        11 => (int_value(u32_at(values, 1)?, item, properties)? & i32_at(values, 0)?) > 0,
        12 => int_value(u32_at(values, 1)?, item, properties)? == i32_at(values, 0)?,
        13 => int_value(u32_at(values, 1)?, item, properties)? != i32_at(values, 0)?,
        14 => color_match(values, &item.palettes)?,
        15 => armor_color_match(values, &item.palettes)?,
        16 => slot_color_match(values, &item.palettes)?,
        17 => exact_palette(values, &item.palettes)?,
        1000 => character_skill(host, u32_at(values, 1)?, true)? >= i32_at(values, 0)?,
        1001 => host.map_or(0, |host| host.character().main_pack_free_slots) >= i32_at(values, 0)?,
        1002 => host.map_or(0, |host| host.character().level) >= i32_at(values, 0)?,
        1003 => host.map_or(0, |host| host.character().level) <= i32_at(values, 0)?,
        1004 => character_base_skill_range(values, host)?,
        2000 => buffed_median_damage(item, properties)? >= f64_at(values, 0)?,
        2001 => buffed_missile_damage(item, properties)? >= f64_at(values, 0)?,
        2003 => f64::from(buffed_int(u32_at(values, 1)?, item, properties)?) >= f64_at(values, 0)?,
        #[allow(clippy::cast_possible_truncation)]
        2005 => {
            (buffed_double(u32_at(values, 1)?, item, properties) as f32)
                >= (f64_at(values, 0)? as f32)
        }
        2006 => buffed_tinked_damage(item, properties)? >= f64_at(values, 0)?,
        2007 => f64::from(total_ratings(item, properties)?) >= f64_at(values, 0)?,
        2008 => can_reach_target(values, item, properties)?,
        9999 => !bool_at(values, 0)?,
        _ => false,
    })
}

fn spell_match(
    values: &[&str],
    item: &InventoryItem,
    host: Option<&mut (dyn Automation + '_)>,
) -> Outcome<bool> {
    let include = rx(values, 0)?;
    let exclude = rx(values, 1)?;
    let exclude_empty = net_trim(value(values, 1)?).is_empty();
    let required = i32_at(values, 2)?;
    let mut count = 0;
    for name in spell_names(item, host) {
        if regex_match(&include, &name)? && (exclude_empty || !regex_match(&exclude, &name)?) {
            count += 1;
            if count >= required {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn color_match(values: &[&str], palettes: &[Palette]) -> Outcome<bool> {
    for palette in palettes {
        if similar_color(values, palette)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn armor_color_match(values: &[&str], palettes: &[Palette]) -> Outcome<bool> {
    let region = value(values, 5)?;
    let Some((_, slots)) = ARMOR_COLOR_SLOTS.iter().find(|(name, _)| *name == region) else {
        return Ok(false);
    };
    for &slot in *slots {
        if let Some(palette) = palette_at(palettes, slot)
            && similar_color(values, palette)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn slot_color_match(values: &[&str], palettes: &[Palette]) -> Outcome<bool> {
    let slot = i32_at(values, 5)?;
    match palette_at(palettes, slot) {
        Some(palette) => similar_color(values, palette),
        None => Ok(false),
    }
}

fn exact_palette(values: &[&str], palettes: &[Palette]) -> Outcome<bool> {
    let slot = i32_at(values, 0)?;
    let expected = u32_at(values, 1)? & 0x00FF_FFFF;
    Ok(palette_at(palettes, slot)
        .is_some_and(|palette| palette.palette_id & 0x00FF_FFFF == expected))
}

fn palette_at(palettes: &[Palette], slot: i32) -> Option<&Palette> {
    usize::try_from(slot)
        .ok()
        .and_then(|index| palettes.get(index))
}

fn checked_byte(value: i32) -> Outcome<u8> {
    u8::try_from(value).map_err(|_| ARITHMETIC_OVERFLOW.to_owned())
}

fn similar_color(values: &[&str], palette: &Palette) -> Outcome<bool> {
    let target = hsv(
        checked_byte(i32_at(values, 0)?)?,
        checked_byte(i32_at(values, 1)?)?,
        checked_byte(i32_at(values, 2)?)?,
    );
    let actual = hsv(palette.red, palette.green, palette.blue);
    if (actual.0 - target.0).abs() > f64_at(values, 3)? {
        return Ok(false);
    }
    let saturation = actual.1 - target.1;
    let brightness = actual.2 - target.2;
    Ok((saturation * saturation + brightness * brightness).sqrt() <= f64_at(values, 4)?)
}

/// Hue (degrees), saturation and value of a colour.
fn hsv(red: u8, green: u8, blue: u8) -> (f64, f64, f64) {
    let (red, green, blue) = (i32::from(red), i32::from(green), i32::from(blue));
    let maximum = red.max(green.max(blue));
    let minimum = red.min(green.min(blue));
    let delta = maximum - minimum;
    let hue = if delta == 0 {
        0.0
    } else if maximum == red {
        let hue = 60.0 * f64::from(green - blue) / f64::from(delta);
        if hue < 0.0 { hue + 360.0 } else { hue }
    } else if maximum == green {
        (60.0 * f64::from(blue - red) / f64::from(delta)) + 120.0
    } else {
        (60.0 * f64::from(red - green) / f64::from(delta)) + 240.0
    };
    let saturation = if maximum == 0 {
        0.0
    } else {
        1.0 - (f64::from(minimum) / f64::from(maximum))
    };
    (hue, saturation, f64::from(maximum) / 255.0)
}

fn spell_names(item: &InventoryItem, host: Option<&mut (dyn Automation + '_)>) -> Vec<String> {
    let Some(host) = host else {
        return Vec::new();
    };
    item.appraised_spell_ids
        .iter()
        .filter_map(|&spell_id| host.spells().try_get(spell_id))
        .map(|spell| spell.name)
        .collect()
}

fn character_skill(
    host: Option<&mut (dyn Automation + '_)>,
    skill_id: u32,
    buffed: bool,
) -> Outcome<i32> {
    let Some(skill) = host.and_then(|host| host.try_get_skill(skill_id)) else {
        return Ok(0);
    };
    i32::try_from(if buffed { skill.current } else { skill.base })
        .map_err(|_| ARITHMETIC_OVERFLOW.to_owned())
}

fn character_base_skill_range(
    values: &[&str],
    host: Option<&mut (dyn Automation + '_)>,
) -> Outcome<bool> {
    let level = character_skill(host, u32_at(values, 0)?, false)?;
    Ok(level >= i32_at(values, 1)? && level <= i32_at(values, 2)?)
}

fn string_value(key: u32, item: &InventoryItem, properties: &ItemProperties) -> String {
    match key {
        1 => item.name.clone(),
        // The name for more than one, which the object itself carries.
        PLURAL_NAME_KEY => item.plural_name.clone(),
        _ => properties
            .strings
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, text)| text.clone())
            .unwrap_or_default(),
    }
}

fn checked_int(value: u32) -> Outcome<i32> {
    i32::try_from(value).map_err(|_| ARITHMETIC_OVERFLOW.to_owned())
}

/// A value the server sends with the object only when the object has one.
/// The item snapshot has no "absent" encoding, so zero stands for "the
/// server sent none", which is what the reference macro sees too.
fn present(field: i32) -> Option<i32> {
    (field != 0).then_some(field)
}

#[allow(clippy::cast_possible_wrap)]
fn unchecked(value: u32) -> i32 {
    value as i32
}

fn property_int(properties: &ItemProperties, key: u32) -> Option<i32> {
    properties.int(key)
}

/// A number the object itself carries and an appraisal may repeat: the
/// appraised value when there is one, otherwise the object's own.
fn object_int(key: u32, field: i32, properties: &ItemProperties) -> Option<i32> {
    property_int(properties, key).or_else(|| present(field))
}

/// An icon layer as the reference macro stores it: the id as the server
/// sends it, without the `0x06000000` every icon id carries. A rare's
/// backdrop is `0x06005B0C`, and loot profiles test for 23308 (`0x5B0C`).
/// An item without the layer has no value at all.
fn icon_layer(icon_id: u32) -> Outcome<Option<i32>> {
    const ICON_ID_PREFIX: u32 = 0x0600_0000;
    if icon_id == 0 {
        return Ok(None);
    }
    let layer = if icon_id >= ICON_ID_PREFIX {
        icon_id - ICON_ID_PREFIX
    } else {
        icon_id
    };
    checked_int(layer).map(Some)
}

fn try_int_value(
    key: u32,
    item: &InventoryItem,
    properties: &ItemProperties,
) -> Outcome<Option<i32>> {
    Ok(match key {
        5 => Some(item.burden),
        // The slot a worn item is in, as the object itself reports it.
        10 => present(unchecked(item.equipped_location)),
        19 => Some(item.value),
        // Uses left and the most uses, which the object itself carries.
        91 => object_int(91, item.maximum_structure, properties),
        92 => object_int(92, item.structure, properties),
        107 => Some(item.item_current_mana),
        108 => Some(item.item_maximum_mana),
        131 => Some(checked_int(item.material_type)?),
        VTANK_INT_BASE => Some(checked_int(item.weenie_class_id)?),
        // The icon as the object itself sends it: without the 0x06000000
        // every icon id carries, like the icon layers.
        k if k == VTANK_INT_BASE + 1 => icon_layer(item.icon_id)?,
        // The container holding the item or, for an item someone wields,
        // the wielder.
        k if k == VTANK_INT_BASE + 2 => {
            if item.container_id != 0 {
                present(unchecked(item.container_id))
            } else {
                present(unchecked(item.wielder_id))
            }
        }
        k if k == VTANK_INT_BASE + 4 => Some(item.items_capacity),
        k if k == VTANK_INT_BASE + 5 => Some(item.containers_capacity),
        k if k == VTANK_INT_BASE + 6 => Some(item.stack_size),
        k if k == VTANK_INT_BASE + 7 => Some(item.maximum_stack_size),
        // Only ever -1, the mark of an item someone wields.
        k if k == VTANK_INT_BASE + 9 => (item.wielder_id != 0).then_some(-1),
        // The body parts a worn item covers, as the object itself sends
        // them: a shirt is 104 (chest and both arm sections), pants 22.
        k if k == VTANK_INT_BASE + 13 => present(unchecked(item.coverage_mask)),
        k if k == VTANK_INT_BASE + 14 => Some(checked_int(item.valid_locations)?),
        // Melee weapon, missile weapon, ammunition, shield.
        k if k == VTANK_INT_BASE + 15 => present(item.combat_use),
        ICON_HIGHLIGHT_KEY => Some(checked_int(item.effects)?),
        // The ammunition a launcher takes or an arrow or bolt is.
        k if k == VTANK_INT_BASE + 17 => present(unchecked(item.ammo_type)),
        // What kind of object a targeted-use item may be used on.
        k if k == VTANK_INT_BASE + 18 => present(unchecked(item.target_type)),
        // The item-type bits and the object-description bits that head
        // every object the server creates.
        k if k == VTANK_INT_BASE + 26 => present(unchecked(item.item_type)),
        k if k == VTANK_INT_BASE + 27 => present(unchecked(item.public_flags)),
        // The weapon's speed rating from its appraisal; a weapon never
        // appraised, or anything else, has none.
        k if k == VTANK_INT_BASE + 31 => {
            properties.weapon.as_ref().map(|weapon| weapon.weapon_time)
        }
        k if k == VTANK_INT_BASE + 32 => Some(item.weapon_skill),
        k if k == VTANK_INT_BASE + 33 => Some(item.damage_type),
        k if k == VTANK_INT_BASE + 34 => Some(item.damage),
        // How the item may be used, including whether it needs a target.
        k if k == VTANK_INT_BASE + 35 => present(unchecked(item.useability)),
        k if k == VTANK_INT_BASE + 41 => icon_layer(item.icon_overlay_id)?,
        k if k == VTANK_INT_BASE + 42 => icon_layer(item.icon_underlay_id)?,
        // Everything else is an appraised property under its own id,
        // workmanship (105) among them: the whole number an appraisal
        // shows, not the fraction the object itself carries. The reference
        // macro never fills AssociatedSpell, Wielder, WieldingSlot or Flags,
        // so those read nothing here either.
        _ => property_int(properties, key),
    })
}

fn int_value(key: u32, item: &InventoryItem, properties: &ItemProperties) -> Outcome<i32> {
    Ok(try_int_value(key, item, properties)?.unwrap_or(0))
}

fn try_double_value(key: u32, item: &InventoryItem, properties: &ItemProperties) -> Option<f64> {
    let armor = |number: fn(&ac_plugin_abi::world::ArmorProfile) -> f32| {
        properties
            .armor
            .as_ref()
            .map(|armor| f64::from(number(armor)))
    };
    let weapon = |number: fn(&ac_plugin_abi::world::WeaponProfile) -> f64| {
        properties.weapon.as_ref().map(number)
    };
    match key {
        // The seven protections come from the armor numbers an appraisal
        // reports, as multipliers on incoming damage (1.2 = takes 20% more).
        // The reference macro's order is slash, pierce, bludgeon, acid,
        // lightning, fire, cold.
        VTANK_DOUBLE_BASE => armor(|a| a.slash_mod),
        k if k == VTANK_DOUBLE_BASE + 1 => armor(|a| a.pierce_mod),
        k if k == VTANK_DOUBLE_BASE + 2 => armor(|a| a.bludgeon_mod),
        k if k == VTANK_DOUBLE_BASE + 3 => armor(|a| a.acid_mod),
        k if k == VTANK_DOUBLE_BASE + 4 => armor(|a| a.electric_mod),
        k if k == VTANK_DOUBLE_BASE + 5 => armor(|a| a.fire_mod),
        k if k == VTANK_DOUBLE_BASE + 6 => armor(|a| a.cold_mod),
        // How close, in metres, the character must be to use the item.
        k if k == VTANK_DOUBLE_BASE + 8 => {
            (item.use_radius != 0.0).then_some(f64::from(item.use_radius))
        }
        k if k == VTANK_DOUBLE_BASE + 9 => Some(f64::from(item.workmanship)),
        k if k == VTANK_DOUBLE_BASE + 11 => Some(item.damage_variance),
        // Attack bonus, range and damage bonus exist only as the weapon
        // numbers an appraisal reports, and only for a weapon.
        k if k == VTANK_DOUBLE_BASE + 12 => weapon(|w| w.weapon_offense),
        k if k == VTANK_DOUBLE_BASE + 13 => weapon(|w| w.max_velocity),
        k if k == VTANK_DOUBLE_BASE + 14 => weapon(|w| w.damage_mod),
        _ => raw_float(properties, key),
    }
}

fn raw_float(properties: &ItemProperties, key: u32) -> Option<f64> {
    properties
        .floats
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, value)| *value)
}

fn double_value(key: u32, item: &InventoryItem, properties: &ItemProperties) -> f64 {
    try_double_value(key, item, properties).unwrap_or(0.0)
}

fn buffed_int(key: u32, item: &InventoryItem, properties: &ItemProperties) -> Outcome<i32> {
    let mut value = int_value(key, item, properties)?;
    if !int_key_exists(key, item, properties)? {
        return Ok(value);
    }
    for spell_id in &item.appraised_spell_ids {
        if let Some((_, bonus_key, bonus)) = INT_SPELL_BONUSES
            .iter()
            .find(|(spell, _, _)| spell == spell_id)
            && *bonus_key == key
            && *bonus != 0
        {
            value = value.wrapping_add(*bonus);
        }
    }
    Ok(value)
}

fn buffed_double(key: u32, item: &InventoryItem, properties: &ItemProperties) -> f64 {
    let mut value = double_value(key, item, properties);
    if !double_key_exists(key, item, properties) {
        return value;
    }
    for spell_id in &item.appraised_spell_ids {
        // A bonus of zero is inert: it is what marks a table row that
        // carries only an operation and no amount.
        let Some((_, bonus_key, change, bonus)) = DOUBLE_SPELL_BONUSES
            .iter()
            .find(|(spell, ..)| spell == spell_id)
        else {
            continue;
        };
        if *bonus_key != key || *bonus == 0.0 {
            continue;
        }
        value = if mosstank_dotnet::double_to_int(*change) == 1 {
            value * bonus
        } else {
            value + bonus
        };
    }
    value
}

// The buffed getters only add a spell bonus when the item carries a base
// value for that key at all; an item with no such property keeps the
// caller's default. A key projected out of the item snapshot has no
// "absent" encoding, so the default value is the only signal there is.
fn int_key_exists(key: u32, item: &InventoryItem, properties: &ItemProperties) -> Outcome<bool> {
    if property_int(properties, key).is_some() {
        return Ok(true);
    }
    Ok(try_int_value(key, item, properties)?.is_some_and(|value| value != 0))
}

fn double_key_exists(key: u32, item: &InventoryItem, properties: &ItemProperties) -> bool {
    raw_float(properties, key).is_some()
        || try_double_value(key, item, properties).is_some_and(|value| value != 0.0)
}

fn minimum_damage(item: &InventoryItem) -> f64 {
    f64::from(item.damage) - (item.damage_variance * f64::from(item.damage))
}

fn buffed_median_damage(item: &InventoryItem, properties: &ItemProperties) -> Outcome<f64> {
    let maximum = f64::from(buffed_int(VTANK_INT_BASE + 34, item, properties)?);
    let minimum = maximum - (item.damage_variance * maximum);
    Ok((minimum + maximum) / 2.0)
}

fn buffed_missile_damage(item: &InventoryItem, properties: &ItemProperties) -> Outcome<f64> {
    Ok(
        f64::from(buffed_int(VTANK_INT_BASE + 34, item, properties)?)
            + (((buffed_double(VTANK_DOUBLE_BASE + 14, item, properties) - 1.0) * 100.0) / 3.0)
            + f64::from(buffed_int(204, item, properties)?),
    )
}

/// How many tinks the item has left, as the tinkering rules count them.
fn tinks_left(item: &InventoryItem, properties: &ItemProperties) -> Outcome<i32> {
    let mut tinks = (10 - int_value(171, item, properties)?).max(0);
    if int_value(179, item, properties)? == 0 {
        tinks -= 1;
    }
    if int_value(131, item, properties)? == 0 {
        tinks = 0;
    }
    Ok(tinks)
}

fn buffed_tinked_damage(item: &InventoryItem, properties: &ItemProperties) -> Outcome<f64> {
    let mut variance = item.damage_variance;
    let mut maximum = buffed_int(VTANK_INT_BASE + 34, item, properties)?;
    let tinks = tinks_left(item, properties)?;
    for _ in 1..=tinks {
        let iron = damage_over_time(maximum + 25, variance);
        let granite = damage_over_time(maximum + 24, variance * 0.8);
        if iron >= granite {
            maximum += 1;
        } else {
            variance *= 0.8;
        }
    }
    Ok(damage_over_time(maximum + 24, variance))
}

fn total_ratings(item: &InventoryItem, properties: &ItemProperties) -> Outcome<i32> {
    Ok(item.gear_damage
        + item.gear_damage_resistance
        + item.gear_critical_chance
        + item.gear_critical_resistance
        + item.gear_critical_damage
        + item.gear_critical_damage_resistance
        + int_value(376, item, properties)?
        + int_value(379, item, properties)?)
}

fn can_reach_target(
    values: &[&str],
    item: &InventoryItem,
    properties: &ItemProperties,
) -> Outcome<bool> {
    let target_damage = f64_at(values, 0)?;
    let target_defense = f64_at(values, 1)?;
    let target_attack = f64_at(values, 2)?;
    let mut defense = buffed_double(29, item, properties);
    let mut attack = buffed_double(VTANK_DOUBLE_BASE + 12, item, properties);
    let mut variance = item.damage_variance;
    let mut maximum = buffed_int(VTANK_INT_BASE + 34, item, properties)?;
    let tinks = tinks_left(item, properties)?;
    for _ in 1..=tinks {
        if defense < target_defense {
            defense += 0.01;
        } else if attack < target_attack {
            attack += 0.01;
        } else if damage_over_time(maximum + 25, variance)
            >= damage_over_time(maximum + 24, variance * 0.8)
        {
            maximum += 1;
        } else {
            variance *= 0.8;
        }
    }
    Ok(damage_over_time(maximum + 24, variance) >= target_damage
        && defense >= target_defense
        && attack >= target_attack)
}

fn damage_over_time(maximum: i32, variance: f64) -> f64 {
    f64::from(maximum) * ((0.9 * (2.0 - variance) / 2.0) + 0.2)
}

/// The payload's lines: any line break ends a line.
fn lines(payload: &str) -> Vec<&str> {
    // `\r\n` first, then a lone `\r`: both are one break.
    payload
        .split("\r\n")
        .flat_map(|part| part.split(['\r', '\n']))
        .collect()
}

/// `string.Trim()`: .NET's white space at both ends.
fn net_trim(text: &str) -> &str {
    text.trim_matches(mosstank_dotnet::text::casing::is_white_space)
}

fn value<'a>(values: &[&'a str], index: usize) -> Outcome<&'a str> {
    values
        .get(index)
        .copied()
        .ok_or_else(|| "A VTClassic loot requirement is truncated.".to_owned())
}

fn rx(values: &[&str], index: usize) -> Outcome<DotNetRegex> {
    DotNetRegex::new(value(values, index)?, RegexOptions::CULTURE_INVARIANT)
        .map_err(|error| error.to_string())
}

/// A match; a pattern that runs past the kit's step limit (where .NET's
/// 100 ms timeout would throw) is a failure like any other (LOOT-1).
fn regex_match(pattern: &DotNetRegex, text: &str) -> Outcome<bool> {
    pattern
        .is_match(text)
        .map_err(|error: RegexError| error.to_string())
}

fn i32_at(values: &[&str], index: usize) -> Outcome<i32> {
    try_parse_int(value(values, index)?).ok_or_else(|| "A VTClassic integer is invalid.".to_owned())
}

fn u32_at(values: &[&str], index: usize) -> Outcome<u32> {
    try_parse_uint(value(values, index)?).ok_or_else(|| "A VTClassic key is invalid.".to_owned())
}

fn f64_at(values: &[&str], index: usize) -> Outcome<f64> {
    try_parse_double(&value(values, index)?.replace(',', "."))
        .ok_or_else(|| "A VTClassic number is invalid.".to_owned())
}

fn bool_at(values: &[&str], index: usize) -> Outcome<bool> {
    parse_bool(value(values, index)?).map_err(|_| "A VTClassic boolean is invalid.".to_owned())
}
