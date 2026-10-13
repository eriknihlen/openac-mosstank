//! The buff lines: the character's self buffs grouped by family, each with
//! what it raises (read from its reference spell's description) and its
//! tiers strongest first (`BuffProfile`, `BuffLine`).

use std::sync::LazyLock;

use ac_plugin_abi::magic::SpellInfo;
use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use mosstank_dotnet::text::{casing, ordinal};

/// What a buff line raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BuffTargetKind {
    /// Not recognised.
    #[default]
    Unknown,
    /// A skill.
    Skill,
    /// An attribute.
    Attribute,
    /// A defensive self buff: the protections, and Armor Self.
    Protection,
    /// A weapon or caster aura.
    Aura,
    /// An armour bane.
    Bane,
    /// A vital's regeneration rate.
    Regeneration,
    /// Any other self-targeted duration buff.
    Other,
}

/// One buff line: a family, what it does, and its known tiers (strongest
/// first).
#[derive(Debug, Clone, PartialEq)]
pub struct BuffLine {
    /// The family.
    pub family: u32,
    /// What it raises.
    pub kind: BuffTargetKind,
    /// The attribute, skill, element or vital it raises.
    pub target_name: String,
    /// Its tiers, strongest first.
    pub tiers: Vec<SpellInfo>,
    /// A reference spell given from outside (the helper's basis spell).
    pub reference_override: Option<SpellInfo>,
}

impl BuffLine {
    /// A line with its own reference.
    #[must_use]
    pub fn new(
        family: u32,
        kind: BuffTargetKind,
        target_name: &str,
        tiers: Vec<SpellInfo>,
    ) -> Self {
        Self {
            family,
            kind,
            target_name: target_name.to_owned(),
            tiers,
            reference_override: None,
        }
    }

    /// The spell the line is classified and matched by: the override, else
    /// the weakest self-targeted tier (the lowest quality, then id).
    #[must_use]
    pub fn reference(&self) -> SpellInfo {
        self.reference_override
            .clone()
            .unwrap_or_else(|| select_reference(&self.tiers))
    }
}

/// The weakest self-targeted tier (a self tier always beats an other one),
/// the lowest quality and then the lowest id; a default spell for no tiers.
#[must_use]
pub fn select_reference(tiers: &[SpellInfo]) -> SpellInfo {
    let mut best: Option<&SpellInfo> = None;
    for tier in tiers {
        let Some(current) = best else {
            best = Some(tier);
            continue;
        };
        if tier.is_self_targeted != current.is_self_targeted {
            if tier.is_self_targeted {
                best = Some(tier);
            }
            continue;
        }
        if tier.quality() < current.quality()
            || (tier.quality() == current.quality() && tier.spell_id < current.spell_id)
        {
            best = Some(tier);
        }
    }
    best.cloned().unwrap_or_default()
}

fn pattern(text: &str) -> DotNetRegex {
    DotNetRegex::new(
        text,
        RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
    )
    .expect("a fixed pattern")
}

static INCREASES: LazyLock<DotNetRegex> = LazyLock::new(|| {
    pattern(r"^Increases (?:the caster's|your) (?<target>.+?)(?<skill>\s+skill)? by ")
});
static PROTECTION: LazyLock<DotNetRegex> =
    LazyLock::new(|| pattern(r"^Reduces damage (?:the caster|you) takes? from (?<target>.+?) by "));
static AURA: LazyLock<DotNetRegex> = LazyLock::new(|| {
    pattern(
        r"\b(a weapon's|weapon or magic caster|magic caster|missile weapon's|magic casting implement's)\b",
    )
});
static BANE: LazyLock<DotNetRegex> =
    LazyLock::new(|| pattern("Target yourself to cast this spell on all of your equipped"));
static HEALTH_REGEN: LazyLock<DotNetRegex> =
    LazyLock::new(|| pattern("natural healing rate|Health Regeneration Rate"));
static STAMINA_REGEN: LazyLock<DotNetRegex> =
    LazyLock::new(|| pattern("rate at which the caster regains Stamina|Stamina Regeneration Rate"));
static MANA_REGEN: LazyLock<DotNetRegex> =
    LazyLock::new(|| pattern("natural mana rate|Mana Regeneration Rate"));

/// The fixed patterns run on short descriptions: no step limit is reached.
fn is_match(regex: &DotNetRegex, text: &str) -> bool {
    matches!(regex.is_match(text), Ok(true))
}

const ATTRIBUTE_NAMES: [&str; 6] = [
    "Strength",
    "Endurance",
    "Quickness",
    "Coordination",
    "Focus",
    "Self",
];

/// What a description says a buff raises (`Classify`); `Unknown` and an
/// empty target when it says nothing recognised.
#[must_use]
pub fn classify(description: Option<&str>) -> (BuffTargetKind, String) {
    let unknown = (BuffTargetKind::Unknown, String::new());
    let Some(description) = description.filter(|d| !casing::is_white_space_only(d)) else {
        return unknown;
    };
    let ascii = description.is_ascii();
    let matches = |regex: &DotNetRegex, plain: fn(&str) -> bool| {
        if ascii {
            plain(description)
        } else {
            is_match(regex, description)
        }
    };
    if matches(&BANE, ascii_patterns::bane) {
        return (BuffTargetKind::Bane, "equipped armor".to_owned());
    }
    if matches(&HEALTH_REGEN, ascii_patterns::health_regen) {
        return (BuffTargetKind::Regeneration, "Health".to_owned());
    }
    if matches(&STAMINA_REGEN, ascii_patterns::stamina_regen) {
        return (BuffTargetKind::Regeneration, "Stamina".to_owned());
    }
    if matches(&MANA_REGEN, ascii_patterns::mana_regen) {
        return (BuffTargetKind::Regeneration, "Mana".to_owned());
    }
    if matches(&AURA, ascii_patterns::aura) {
        return (BuffTargetKind::Aura, "weapon".to_owned());
    }
    let protection = if ascii {
        ascii_patterns::protection(description).map(str::to_owned)
    } else {
        match PROTECTION.find(description) {
            Ok(Some(found)) => Some(found.group("target").value.to_owned()),
            _ => None,
        }
    };
    if let Some(target) = protection {
        return (BuffTargetKind::Protection, target.trim().to_owned());
    }
    let increases = if ascii {
        ascii_patterns::increases(description).map(|(target, skill)| (target.to_owned(), skill))
    } else {
        match INCREASES.find(description) {
            Ok(Some(found)) => Some((
                found.group("target").value.to_owned(),
                found.group("skill").success,
            )),
            _ => None,
        }
    };
    let Some((target, skill)) = increases else {
        return unknown;
    };
    let target = target.trim().to_owned();
    if target.is_empty() {
        return (BuffTargetKind::Unknown, target);
    }
    if skill {
        let target = if ordinal::equals_ignore_case(&target, "Assess Monster") {
            "Assess Creature".to_owned()
        } else {
            target
        };
        return (BuffTargetKind::Skill, target);
    }
    if ATTRIBUTE_NAMES
        .iter()
        .any(|name| ordinal::equals_ignore_case(name, &target))
    {
        return (BuffTargetKind::Attribute, target);
    }
    let kind = if ordinal::contains_ignore_case(&target, "armor") {
        BuffTargetKind::Protection
    } else {
        BuffTargetKind::Other
    };
    (kind, target)
}

/// [`classify`], and whether it recognised anything.
#[must_use]
pub fn try_parse_target(description: Option<&str>) -> Option<(BuffTargetKind, String)> {
    let (kind, target) = classify(description);
    (kind != BuffTargetKind::Unknown).then_some((kind, target))
}

/// The buff lines of the character's self buffs: grouped by family (in the
/// order families first appear), instantaneous spells left out, tiers
/// strongest first in a total order, each line classified by its reference
/// (an unrecognised one is `Other`, named after its reference).
#[must_use]
pub fn build(known_self_buffs: &[SpellInfo]) -> Vec<BuffLine> {
    let mut by_family: Vec<(u32, Vec<SpellInfo>)> = Vec::new();
    for spell in known_self_buffs {
        // Instantaneous spells (vital transfers, heals) are not buffs, and
        // share families across unrelated lines.
        if spell.duration_seconds <= 0.0 {
            continue;
        }
        match by_family
            .iter_mut()
            .find(|(family, _)| *family == spell.family)
        {
            Some((_, tiers)) => tiers.push(spell.clone()),
            None => by_family.push((spell.family, vec![spell.clone()])),
        }
    }
    by_family
        .into_iter()
        .map(|(family, mut tiers)| {
            tiers.sort_by(|a, b| {
                b.tier
                    .cmp(&a.tier)
                    .then_with(|| b.quality().cmp(&a.quality()))
                    .then_with(|| a.spell_id.cmp(&b.spell_id))
            });
            let reference = select_reference(&tiers);
            let (mut kind, mut target) = classify(Some(&reference.description));
            if kind == BuffTargetKind::Unknown {
                // Nothing self-targeted is dropped for being unrecognised.
                kind = BuffTargetKind::Other;
                target = reference.name.clone();
            }
            BuffLine::new(family, kind, &target, tiers)
        })
        .collect()
}

/// The buff lines of the last spell list, rebuilt only when the list's
/// contents change (`BuffProfileCache`); each pass still plans from live
/// skills, enchantments and settings.
#[derive(Debug, Clone)]
pub struct BuffProfileCache {
    spells: Vec<SpellInfo>,
    lines: std::rc::Rc<[BuffLine]>,
}

impl Default for BuffProfileCache {
    fn default() -> Self {
        Self {
            spells: Vec::new(),
            lines: std::rc::Rc::from(Vec::new()),
        }
    }
}

impl BuffProfileCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The lines of `spells`: the cached ones while the contents are equal
    /// (compared by value, so an entry replaced in place is seen), else
    /// built afresh.
    pub fn capture(&mut self, spells: &[SpellInfo]) -> std::rc::Rc<[BuffLine]> {
        if spells != self.spells.as_slice() {
            self.spells = spells.to_vec();
            self.lines = std::rc::Rc::from(build(spells));
        }
        std::rc::Rc::clone(&self.lines)
    }
}

/// The fixed patterns of [`classify`] matched by hand on an ASCII
/// description, as the patterns match it (case ignored, `.` any character
/// but a line feed, `\s` white space, `\b` a word boundary).
///
/// The descriptions are nearly all ASCII, and in the client's interpreter
/// the regex engine costs some hundreds of thousands of instructions a
/// description (and tens of millions to build each pattern once); a
/// description with any other character goes to the patterns. The tests
/// hold both to the same answers.
mod ascii_patterns {
    /// Where `needle` first occurs in `text` at or after `from`, case
    /// ignored.
    fn find(text: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        if needle.len() > text.len() {
            return None;
        }
        let Some(&first) = needle.first() else {
            return (from <= text.len()).then_some(from);
        };
        let first = first.to_ascii_lowercase();
        (from..=text.len() - needle.len()).find(|&at| {
            text[at].to_ascii_lowercase() == first
                && text[at..at + needle.len()].eq_ignore_ascii_case(needle)
        })
    }

    fn contains(text: &str, needle: &str) -> bool {
        find(text.as_bytes(), needle.as_bytes(), 0).is_some()
    }

    /// Whether `text` has `literal` at `at`, case ignored.
    fn at(text: &[u8], at: usize, literal: &[u8]) -> bool {
        text.len() >= at + literal.len()
            && text[at..at + literal.len()].eq_ignore_ascii_case(literal)
    }

    fn is_word(byte: Option<&u8>) -> bool {
        byte.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    }

    /// `\b` at `index`.
    fn boundary(text: &[u8], index: usize) -> bool {
        let before = index.checked_sub(1).and_then(|i| text.get(i));
        is_word(before) != is_word(text.get(index))
    }

    /// `BANE`.
    pub(super) fn bane(text: &str) -> bool {
        contains(
            text,
            "Target yourself to cast this spell on all of your equipped",
        )
    }

    /// `HEALTH_REGEN`.
    pub(super) fn health_regen(text: &str) -> bool {
        contains(text, "natural healing rate") || contains(text, "Health Regeneration Rate")
    }

    /// `STAMINA_REGEN`.
    pub(super) fn stamina_regen(text: &str) -> bool {
        contains(text, "rate at which the caster regains Stamina")
            || contains(text, "Stamina Regeneration Rate")
    }

    /// `MANA_REGEN`.
    pub(super) fn mana_regen(text: &str) -> bool {
        contains(text, "natural mana rate") || contains(text, "Mana Regeneration Rate")
    }

    /// `AURA`: one of the phrases with a word boundary at both ends.
    pub(super) fn aura(text: &str) -> bool {
        let bytes = text.as_bytes();
        [
            "a weapon's",
            "weapon or magic caster",
            "magic caster",
            "missile weapon's",
            "magic casting implement's",
        ]
        .iter()
        .any(|phrase| {
            let phrase = phrase.as_bytes();
            let mut from = 0;
            while let Some(start) = find(bytes, phrase, from) {
                if boundary(bytes, start) && boundary(bytes, start + phrase.len()) {
                    return true;
                }
                from = start + 1;
            }
            false
        })
    }

    /// The shortest run of characters from `start`, at least one and no
    /// line feed, after which `rest` matches (`(?<target>.+?)` then
    /// `rest`): its end and what `rest` answered.
    fn lazy_run<T>(
        text: &[u8],
        start: usize,
        rest: impl Fn(usize) -> Option<T>,
    ) -> Option<(usize, T)> {
        let mut end = start;
        while end < text.len() && text[end] != b'\n' {
            end += 1;
            if let Some(found) = rest(end) {
                return Some((end, found));
            }
        }
        None
    }

    /// `PROTECTION`: `^Reduces damage (?:the caster|you) takes? from
    /// (?<target>.+?) by `; the target.
    pub(super) fn protection(text: &str) -> Option<&str> {
        let bytes = text.as_bytes();
        let mut index = b"Reduces damage ".len();
        if !at(bytes, 0, b"Reduces damage ") {
            return None;
        }
        if at(bytes, index, b"the caster") {
            index += b"the caster".len();
        } else if at(bytes, index, b"you") {
            index += b"you".len();
        } else {
            return None;
        }
        if !at(bytes, index, b" take") {
            return None;
        }
        index += b" take".len();
        if at(bytes, index, b"s") {
            index += 1;
        }
        if !at(bytes, index, b" from ") {
            return None;
        }
        let start = index + b" from ".len();
        let (end, ()) = lazy_run(bytes, start, |end| at(bytes, end, b" by ").then_some(()))?;
        Some(&text[start..end])
    }

    /// `INCREASES`: `^Increases (?:the caster's|your) (?<target>.+?)
    /// (?<skill>\s+skill)? by `; the target and whether `skill` matched.
    pub(super) fn increases(text: &str) -> Option<(&str, bool)> {
        let bytes = text.as_bytes();
        if !at(bytes, 0, b"Increases ") {
            return None;
        }
        let mut index = b"Increases ".len();
        if at(bytes, index, b"the caster's") {
            index += b"the caster's".len();
        } else if at(bytes, index, b"your") {
            index += b"your".len();
        } else {
            return None;
        }
        if !at(bytes, index, b" ") {
            return None;
        }
        let start = index + 1;
        let (end, skill) = lazy_run(bytes, start, |end| {
            // The skill group first (greedy `\s+`: a word cannot start
            // with white space, so only the whole run can be followed by
            // `skill`), then without it.
            let mut after = end;
            while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                after += 1;
            }
            if after > end && at(bytes, after, b"skill") && at(bytes, after + 5, b" by ") {
                return Some(true);
            }
            at(bytes, end, b" by ").then_some(false)
        })?;
        Some((&text[start..end], skill))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Descriptions as the client words them, and near misses.
    fn corpus() -> Vec<String> {
        let mut texts: Vec<String> = [
            "Increases the caster's Strength by 40 points.",
            "Increases the caster's War Magic skill by 40 points.",
            "Increases your Run skill by 10 points.",
            "Increases your Run  skill by 10 points.",
            "Increases your Run\tskill by 10 points.",
            "Increases your Run \n skill by 10 points.",
            "Increases the caster's skill by 5.",
            "Increases the caster's  skill by 5.",
            "Increases the caster's Assess Monster skill by 15 points.",
            "Increases the caster's a by b by c",
            "Increases the caster's a skill by b skill by c",
            "INCREASES THE CASTER'S FOCUS BY 3",
            "increases YOUR self by 3",
            "Increases the caster's X\nY by 3",
            "Increases the caster's  by 3",
            "Increases the caster's by 3",
            "Increases the casters Strength by 40",
            "Increases the caster's natural armor by 200 points.",
            "Increases the caster's Mana Conversion skillful by 3 skill by 4",
            "Reduces damage the caster takes from Fire by 50%.",
            "Reduces damage you take from Acid by 5",
            "reduces damage YOU TAKE FROM blades BY 5",
            "Reduces damage you takes from Cold by 5",
            "Reduces damage the caster take from  by 5",
            "Reduces damage the caster takes from \nFire by 5",
            "Reduces damage the caster takes from Fire by",
            "Reduces damage the caster takess from Fire by 5",
            "Increases the caster's natural healing rate by 30%.",
            "Increases the rate at which the caster regains Stamina by 30%.",
            "Increases the caster's natural mana rate by 30%.",
            "Health Regeneration Rate up",
            "Increases a weapon's Attack Skill by 10.",
            "Increases aweapon's speed.",
            "Increases the weapon or magic caster's defense.",
            "magic casters",
            "a magic caster",
            "_magic caster",
            "magic caster_",
            "magic caster",
            "Increases a missile weapon's damage.",
            "a magic casting implement's mana",
            "Target yourself to cast this spell on all of your equipped armor.",
            "target YOURSELF to cast this spell on all of your EQUIPPED",
            "Some other thing that the spell does.",
            "",
        ]
        .iter()
        .map(|text| (*text).to_owned())
        .collect();
        let more: Vec<String> = texts
            .iter()
            .flat_map(|text| {
                [
                    format!(" {text}"),
                    format!("{text} by 2"),
                    text.to_lowercase(),
                ]
            })
            .collect();
        texts.extend(more);
        texts
    }

    // Each hand matcher answers what its pattern answers.
    #[test]
    fn the_hand_matchers_answer_as_the_patterns() {
        let found = |regex: &DotNetRegex, text: &str, group: &str| match regex.find(text) {
            Ok(Some(found)) => Some(found.group(group).value.to_owned()),
            _ => None,
        };
        for text in corpus() {
            assert!(text.is_ascii());
            assert_eq!(
                ascii_patterns::bane(&text),
                is_match(&BANE, &text),
                "{text:?}"
            );
            assert_eq!(
                ascii_patterns::health_regen(&text),
                is_match(&HEALTH_REGEN, &text),
                "{text:?}"
            );
            assert_eq!(
                ascii_patterns::stamina_regen(&text),
                is_match(&STAMINA_REGEN, &text),
                "{text:?}"
            );
            assert_eq!(
                ascii_patterns::mana_regen(&text),
                is_match(&MANA_REGEN, &text),
                "{text:?}"
            );
            assert_eq!(
                ascii_patterns::aura(&text),
                is_match(&AURA, &text),
                "{text:?}"
            );
            assert_eq!(
                ascii_patterns::protection(&text).map(str::to_owned),
                found(&PROTECTION, &text, "target"),
                "{text:?}"
            );
            let skill = match INCREASES.find(&text) {
                Ok(Some(found)) => Some(found.group("skill").success),
                _ => None,
            };
            assert_eq!(
                ascii_patterns::increases(&text).map(|(target, skill)| (target.to_owned(), skill)),
                found(&INCREASES, &text, "target").zip(skill),
                "{text:?}"
            );
        }
    }
}
