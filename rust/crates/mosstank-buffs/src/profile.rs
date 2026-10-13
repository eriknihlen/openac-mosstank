//! The buff lines: the character's self buffs grouped by family, each with
//! what it raises (read from its reference spell's description) and its
//! tiers strongest first (`BuffProfile`, `BuffLine`).

use std::sync::LazyLock;

use ac_plugin_abi::magic::SpellInfo;
use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use openac_plugin_compat::text::{casing, ordinal};

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
    if is_match(&BANE, description) {
        return (BuffTargetKind::Bane, "equipped armor".to_owned());
    }
    if is_match(&HEALTH_REGEN, description) {
        return (BuffTargetKind::Regeneration, "Health".to_owned());
    }
    if is_match(&STAMINA_REGEN, description) {
        return (BuffTargetKind::Regeneration, "Stamina".to_owned());
    }
    if is_match(&MANA_REGEN, description) {
        return (BuffTargetKind::Regeneration, "Mana".to_owned());
    }
    if is_match(&AURA, description) {
        return (BuffTargetKind::Aura, "weapon".to_owned());
    }
    if let Ok(Some(found)) = PROTECTION.find(description) {
        return (
            BuffTargetKind::Protection,
            found.group("target").value.trim().to_owned(),
        );
    }
    let Ok(Some(found)) = INCREASES.find(description) else {
        return unknown;
    };
    let target = found.group("target").value.trim().to_owned();
    if target.is_empty() {
        return (BuffTargetKind::Unknown, target);
    }
    if found.group("skill").success {
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
