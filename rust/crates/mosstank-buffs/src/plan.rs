//! The buff plan: which buff lines a pass casts, at which tier, in what
//! order (`BuffPlan`). VTank's rules: every attribute, every trained skill,
//! the protections the profile allows, the regeneration rates, extra
//! exemplars added and anti exemplars removed; a family already in force at
//! an equal tier with time left is skipped; the strongest tier the casting
//! skill carries with the profile's margin; creature spells first, then
//! item, then life, cheapest first within each.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill, Training};
use ac_plugin_abi::magic::SpellInfo;
use mosstank_automation::SpellCatalog;
use mosstank_dotnet::text::ordinal;
use mosstank_settings::model::BuffSettings;

use crate::profile::{BuffLine, BuffTargetKind};

/// The creature enchantment skill.
pub const CREATURE_ENCHANTMENT_SKILL: u32 = 31;
/// The item enchantment skill.
pub const ITEM_ENCHANTMENT_SKILL: u32 = 32;
/// The life magic skill.
pub const LIFE_MAGIC_SKILL: u32 = 33;

/// A tier passed over, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct BuffTierRejection {
    /// The tier.
    pub spell: SpellInfo,
    /// Why (`comp set`, `skill 300 < 355`, ...).
    pub reason: String,
}

/// Whether a tier can be cast now, beyond skill (scarabs, blacklisted
/// components), and where a pass reports what it chose (`IBuffCastability`).
pub trait BuffCastability {
    /// Whether the tier can be cast, and why not.
    fn is_castable(&mut self, tier: &SpellInfo) -> (bool, Option<String>);

    /// The spell table, when the walk reads the client (so a plan can take
    /// its exemplars from it without a second borrow of the client).
    fn spell_catalog(&mut self) -> Option<&mut dyn SpellCatalog> {
        None
    }

    /// The once-per-run "buff SKIPPED." line.
    fn note_no_castable_tier(&mut self, line: &BuffLine);

    /// The tier a line resolved to (or none), with every tier passed over.
    fn note_tier_pick(
        &mut self,
        line: &BuffLine,
        pick: Option<&SpellInfo>,
        rejections: &[BuffTierRejection],
    ) {
        let _ = (line, pick, rejections);
    }
}

/// What a plan may be asked beyond the settings (`BuffPlan.Build`'s
/// optional parameters).
#[derive(Default)]
#[allow(missing_debug_implementations)]
pub struct BuildOptions<'a> {
    /// Recast everything, as the Buff button does.
    pub force: bool,
    /// The rebuff threshold in place of the profile's, seconds.
    pub rebuff_when_under_seconds: Option<f64>,
    /// The character's level, for the untrained-school ceilings.
    pub character_level: i32,
    /// Spells treated as run out.
    pub forced_spell_ids: Option<&'a HashSet<u32>>,
    /// Scarabs and components, and the trace.
    pub castability: Option<&'a mut dyn BuffCastability>,
    /// The spell table, for exemplars by id or name.
    pub spell_catalog: Option<&'a mut dyn SpellCatalog>,
    /// More extra exemplars beside the profile's.
    pub additional_extra_spell_ids: Option<&'a [u32]>,
}

fn is_trained(skill: &Skill) -> bool {
    matches!(skill.training, Training::Trained | Training::Specialized)
}

/// The spells a pass casts, in order.
#[must_use]
pub fn build(
    lines: &[BuffLine],
    skills: &[Skill],
    attributes: &[Attribute],
    active: &[ActiveEnchantment],
    settings: &BuffSettings,
    mut options: BuildOptions<'_>,
) -> Vec<SpellInfo> {
    if !settings.enabled {
        return Vec::new();
    }
    let trained_skill = |name: &str| {
        skills.iter().any(|skill| {
            (!settings.buff_trained_skills_only || is_trained(skill))
                && ordinal::equals_ignore_case(&skill.name, name)
        })
    };
    let has_attribute = |name: &str| {
        attributes
            .iter()
            .any(|attribute| ordinal::equals_ignore_case(&attribute.name, name))
    };

    let mut in_force: HashMap<u32, Vec<(i32, f64)>> = HashMap::new();
    for enchantment in active {
        if enchantment.family == 0 {
            continue;
        }
        let forced = options.force
            || options
                .forced_spell_ids
                .is_some_and(|ids| ids.contains(&enchantment.spell_id));
        let seconds = if forced {
            0.0
        } else {
            enchantment.seconds_remaining
        };
        in_force
            .entry(enchantment.family)
            .or_default()
            .push((enchantment.tier, seconds));
    }

    let mut skill_levels: HashMap<u32, u32> = HashMap::new();
    for skill in skills {
        skill_levels.insert(skill.skill_id, skill.current);
    }

    let mut extra_ids: Vec<u32> = settings.extra_buff_spell_ids.clone();
    if let Some(more) = options.additional_extra_spell_ids {
        extra_ids.extend_from_slice(more);
    }
    let extra_names: Vec<String> = settings.extra_buff_spell_names.iter().cloned().collect();
    let extra_families =
        resolve_families(lines, &extra_ids, &extra_names, catalog_of(&mut options));
    let blacklisted_names: Vec<String> = settings
        .blacklisted_buff_family_names
        .iter()
        .cloned()
        .collect();
    let excluded_families = resolve_families(
        lines,
        &settings.anti_extra_buff_spell_ids,
        &blacklisted_names,
        catalog_of(&mut options),
    );

    let mut plan: Vec<(i32, SpellInfo)> = Vec::new();
    for line in lines {
        let school = line.tiers.first().map_or(0, |tier| tier.school);
        let school_available =
            is_school_available(school, skills, settings, options.character_level);
        let wanted = match line.kind {
            BuffTargetKind::Attribute => {
                school_available && settings.buff_attributes && has_attribute(&line.target_name)
            }
            BuffTargetKind::Skill => {
                school_available
                    && (trained_skill(&line.target_name) || is_magic_school_name(&line.target_name))
            }
            BuffTargetKind::Protection => {
                school_available && settings.buff_protections && profile_allows(line, settings)
            }
            BuffTargetKind::Regeneration => school_available && settings.buff_regeneration,
            BuffTargetKind::Other => school_available && settings.buff_other,
            BuffTargetKind::Aura | BuffTargetKind::Bane | BuffTargetKind::Unknown => false,
        };
        let extra = extra_families.contains(&line.family);
        if !school_available || (!wanted && !extra) || excluded_families.contains(&line.family) {
            continue;
        }
        if !extra && !is_exemplar_family(line, catalog_of(&mut options)) {
            continue;
        }

        let pick = try_pick_tier(
            line,
            &skill_levels,
            settings.skill_excess_over_difficulty,
            options.castability.as_deref_mut(),
        );
        let threshold = options
            .rebuff_when_under_seconds
            .unwrap_or(settings.rebuff_when_under_seconds);
        if let Some(held) = in_force.get(&line.family)
            && is_covered(held, pick.as_ref().map_or(-1, |pick| pick.tier), threshold)
        {
            continue;
        }
        let Some(pick) = pick else {
            if let Some(castability) = options.castability.as_deref_mut() {
                castability.note_no_castable_tier(line);
            }
            continue;
        };
        plan.push((cast_rank(line, &pick), pick));
    }

    plan.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.mana_cost.cmp(&b.1.mana_cost))
            .then_with(|| a.1.spell_id.cmp(&b.1.spell_id))
    });
    plan.into_iter().map(|(_, spell)| spell).collect()
}

/// The families exemplar ids and names stand for: through the spell table
/// when it has the spell, else the lines holding a tier of that id or name.
fn resolve_families(
    lines: &[BuffLine],
    exemplar_ids: &[u32],
    names: &[String],
    mut catalog: Option<&mut (dyn SpellCatalog + '_)>,
) -> HashSet<u32> {
    let mut families = HashSet::new();
    for &exemplar_id in exemplar_ids {
        if let Some(exemplar) = catalog
            .as_deref_mut()
            .and_then(|catalog| catalog.try_get(exemplar_id))
        {
            families.insert(exemplar.family);
            continue;
        }
        for line in lines {
            if line.tiers.iter().any(|tier| tier.spell_id == exemplar_id) {
                families.insert(line.family);
            }
        }
    }
    for name in names {
        if let Some(exemplar) = catalog
            .as_deref_mut()
            .and_then(|catalog| catalog.try_find_by_name(name, false))
        {
            families.insert(exemplar.family);
            continue;
        }
        for line in lines {
            if line.tiers.iter().any(|tier| tier.name == *name) {
                families.insert(line.family);
            }
        }
    }
    families
}

const ATTRIBUTE_EXEMPLARS: [(&str, &str); 6] = [
    ("Strength", "Strength Self I"),
    ("Endurance", "Endurance Self I"),
    ("Coordination", "Coordination Self I"),
    ("Quickness", "Quickness Self I"),
    ("Focus", "Focus Self I"),
    ("Self", "Willpower Self I"),
];

const REGENERATION_EXEMPLARS: [(&str, &str); 3] = [
    ("Health", "Regeneration Self I"),
    ("Stamina", "Rejuvenation Self I"),
    ("Mana", "Mana Renewal Self I"),
];

const SKILL_EXEMPLARS: [(&str, &str); 39] = [
    ("Life Magic", "Life Magic Mastery Self I"),
    ("Melee Defense", "Invulnerability Self I"),
    ("Missile Defense", "Impregnability Self I"),
    ("Arcane Lore", "Arcane Enlightenment Self I"),
    ("Magic Defense", "Magic Resistance Self I"),
    ("Mana Conversion", "Mana Conversion Mastery Self I"),
    ("Item Tinkering", "Item Tinkering Expertise Self I"),
    ("Assess Person", "Person Attunement Self I"),
    ("Deception", "Deception Mastery Self I"),
    ("Healing", "Healing Mastery Self I"),
    ("Jump", "Jumping Mastery Self I"),
    ("Lockpick", "Lockpick Mastery Self I"),
    ("Run", "Sprint Self I"),
    ("Assess Creature", "Monster Attunement Self I"),
    ("Weapon Tinkering", "Weapon Tinkering Expertise Self I"),
    ("Armor Tinkering", "Armor Tinkering Expertise Self I"),
    (
        "Magic Item Tinkering",
        "Magic Item Tinkering Expertise Self I",
    ),
    (
        "Creature Enchantment",
        "Creature Enchantment Mastery Self I",
    ),
    ("Item Enchantment", "Item Enchantment Mastery Self I"),
    ("War Magic", "War Magic Mastery Self I"),
    ("Leadership", "Leadership Mastery Self I"),
    ("Loyalty", "Fealty Self I"),
    ("Fletching", "Fletching Mastery Self I"),
    ("Alchemy", "Alchemy Mastery Self I"),
    ("Cooking", "Cooking Mastery Self I"),
    ("Salvaging", "Arcanum Salvaging Self I"),
    ("Two Handed Combat", "Two Handed Combat Mastery Self I"),
    ("Gearcraft", "Gear Craft Mastery Self I"),
    ("Void Magic", "Void Magic Mastery Self I"),
    ("Heavy Weapons", "Heavy Weapon Mastery Self I"),
    ("Light Weapons", "Light Weapon Mastery Self I"),
    ("Finesse Weapons", "Finesse Weapon Mastery Self I"),
    ("Missile Weapons", "Missile Weapon Mastery Self I"),
    ("Shield", "Shield Mastery Self I"),
    ("Dual Wield", "Dual Wield Mastery Self I"),
    ("Recklessness", "Recklessness Mastery Self I"),
    ("Sneak Attack", "Sneak Attack Mastery Self I"),
    ("Dirty Fighting", "Dirty Fighting Mastery Self II"),
    ("Summoning", "Summoning Mastery Self I"),
];

/// Whether the line is the family VTank buffs for its target: the family of
/// the target's named tier-one spell. A line with no named spell, or whose
/// named spell the table lacks, is kept.
fn is_exemplar_family(line: &BuffLine, catalog: Option<&mut (dyn SpellCatalog + '_)>) -> bool {
    let Some(catalog) = catalog else {
        return true;
    };
    let names: &[(&str, &str)] = match line.kind {
        BuffTargetKind::Attribute => &ATTRIBUTE_EXEMPLARS,
        BuffTargetKind::Skill => &SKILL_EXEMPLARS,
        BuffTargetKind::Regeneration => &REGENERATION_EXEMPLARS,
        _ => return true,
    };
    let Some((_, exemplar_name)) = names
        .iter()
        .find(|(target, _)| ordinal::equals_ignore_case(target, &line.target_name))
    else {
        return true;
    };
    match catalog.try_find_by_name(exemplar_name, false) {
        Some(exemplar) => exemplar.family == line.family,
        None => true,
    }
}

fn is_covered(entries: &[(i32, f64)], quality: i32, threshold_seconds: f64) -> bool {
    entries
        .iter()
        .any(|&(tier, seconds)| tier >= quality && seconds >= threshold_seconds)
}

fn profile_allows(line: &BuffLine, settings: &BuffSettings) -> bool {
    let enabled: &str = match settings.protection_profile_mode {
        1 => &settings.protection_elements,
        2 => "ALFCBPS",
        4 => "B",
        5 => "BPS",
        6 => "BPSA",
        7 => "ALFC",
        8 => "BPSAC",
        _ => "",
    };
    match element_code(line) {
        None => true,
        Some(element) => enabled.contains(element),
    }
}

fn element_code(line: &BuffLine) -> Option<char> {
    let first = line.tiers.first();
    let text = format!(
        "{} {} {}",
        line.target_name,
        first.map_or("", |tier| tier.name.as_str()),
        first.map_or("", |tier| tier.description.as_str())
    );
    let has = |word: &str| ordinal::contains_ignore_case(&text, word);
    if has("acid") {
        Some('A')
    } else if has("lightning") || has("electric") {
        Some('L')
    } else if has("fire") {
        Some('F')
    } else if has("cold") || has("frost") {
        Some('C')
    } else if has("bludgeon") {
        Some('B')
    } else if has("pierc") {
        Some('P')
    } else if has("slash") {
        Some('S')
    } else {
        None
    }
}

fn is_magic_school_name(name: &str) -> bool {
    ["Item Enchantment", "Creature Enchantment", "Life Magic"]
        .iter()
        .any(|school| ordinal::equals_ignore_case(school, name))
}

/// Whether a school can be cast from: always unless it is one of the three
/// buff schools, which need training or a character level within the
/// profile's untrained ceiling for that school.
#[must_use]
pub fn is_school_available(
    school: u32,
    skills: &[Skill],
    settings: &BuffSettings,
    character_level: i32,
) -> bool {
    if !matches!(
        school,
        ITEM_ENCHANTMENT_SKILL | CREATURE_ENCHANTMENT_SKILL | LIFE_MAGIC_SKILL
    ) {
        return true;
    }
    if skills
        .iter()
        .any(|skill| skill.skill_id == school && is_trained(skill))
    {
        return true;
    }
    let limit = match school {
        ITEM_ENCHANTMENT_SKILL => settings.buff_with_untrained_item_skill,
        CREATURE_ENCHANTMENT_SKILL => settings.buff_with_untrained_creature_skill,
        LIFE_MAGIC_SKILL => settings.buff_with_untrained_life_skill,
        _ => i32::MAX,
    };
    character_level <= limit
}

/// A pick's place in the cast order: creature (enchantment skill, Focus,
/// Self, Endurance, the rest), item, life (regeneration last), then the
/// rest.
#[must_use]
pub fn cast_rank(line: &BuffLine, pick: &SpellInfo) -> i32 {
    let school = match pick.school {
        CREATURE_ENCHANTMENT_SKILL => 0,
        ITEM_ENCHANTMENT_SKILL => 1,
        LIFE_MAGIC_SKILL => 2,
        _ => 3,
    };
    let within = match school {
        0 => creature_order(line),
        2 => i32::from(line.kind == BuffTargetKind::Regeneration),
        _ => 0,
    };
    school * 10 + within
}

fn creature_order(line: &BuffLine) -> i32 {
    let named = |name: &str| ordinal::equals_ignore_case(&line.target_name, name);
    if line.kind == BuffTargetKind::Skill && named("Creature Enchantment") {
        return 0;
    }
    if line.kind == BuffTargetKind::Attribute {
        if named("Focus") {
            return 1;
        }
        if named("Self") {
            return 2;
        }
        if named("Endurance") {
            return 3;
        }
    }
    4
}

/// The tier a line casts: the strongest that stays on the reference's line
/// (school, fellowship, untargeted, component set) and that the casting
/// skill carries with the margin and the castability allows. When none
/// does and the school's skill is unknown, the weakest such tier.
pub fn try_pick_tier(
    line: &BuffLine,
    skill_levels: &HashMap<u32, u32>,
    skill_excess_over_difficulty: i32,
    mut castability: Option<&mut (dyn BuffCastability + '_)>,
) -> Option<SpellInfo> {
    if line.tiers.is_empty() {
        return None;
    }
    let reference = line.reference();
    let mut rejections: Vec<BuffTierRejection> = Vec::new();
    let tracing = castability.is_some();
    let reject = |rejections: &mut Vec<BuffTierRejection>, tier: &SpellInfo, reason: String| {
        if tracing {
            rejections.push(BuffTierRejection {
                spell: tier.clone(),
                reason,
            });
        }
    };

    let mut weakest: Option<&SpellInfo> = None;
    for tier in &line.tiers {
        if let Some(reason) = mismatch(tier, &reference) {
            reject(&mut rejections, tier, reason.to_owned());
            continue;
        }
        weakest = Some(tier);
        let level = (tier.school != 0)
            .then(|| skill_levels.get(&tier.school).copied())
            .flatten();
        let Some(level) = level else {
            reject(&mut rejections, tier, "skill unknown".to_owned());
            continue;
        };
        let needed = i64::from(tier.difficulty) + i64::from(skill_excess_over_difficulty);
        if i64::from(level) < needed {
            reject(&mut rejections, tier, format!("skill {level} < {needed}"));
            continue;
        }
        if let Some(castability) = castability.as_deref_mut() {
            let (castable, reason) = castability.is_castable(tier);
            if !castable {
                reject(
                    &mut rejections,
                    tier,
                    reason.unwrap_or_else(|| "not castable".to_owned()),
                );
                continue;
            }
        }
        if let Some(castability) = castability.as_deref_mut() {
            castability.note_tier_pick(line, Some(tier), &rejections);
        }
        return Some(tier.clone());
    }

    let Some(weakest) = weakest else {
        // Nothing in the family shares the reference's line.
        if let Some(castability) = castability.as_deref_mut() {
            castability.note_tier_pick(line, None, &rejections);
        }
        return None;
    };
    if weakest.school != 0 && skill_levels.contains_key(&weakest.school) {
        // The school is known, and even the weakest tier is out of reach.
        if let Some(castability) = castability.as_deref_mut() {
            castability.note_tier_pick(line, None, &rejections);
        }
        return None;
    }
    if let Some(castability) = castability.as_deref_mut()
        && !castability.is_castable(weakest).0
    {
        castability.note_tier_pick(line, None, &rejections);
        return None;
    }
    if let Some(castability) = castability {
        castability.note_tier_pick(line, Some(weakest), &rejections);
    }
    Some(weakest.clone())
}

/// Why a tier is not on the reference's line, if it is not.
fn mismatch(tier: &SpellInfo, reference: &SpellInfo) -> Option<&'static str> {
    if tier.school != reference.school
        || tier.is_fellowship != reference.is_fellowship
        || tier.is_untargeted != reference.is_untargeted
    {
        return Some("unknown");
    }
    if tier.component_set != reference.component_set {
        return Some("comp set");
    }
    None
}

/// The plan's spell table: the one handed in, else the castability's.
fn catalog_of<'b>(options: &'b mut BuildOptions<'_>) -> Option<&'b mut dyn SpellCatalog> {
    if options.spell_catalog.is_some() {
        return options
            .spell_catalog
            .as_deref_mut()
            .map(|catalog| catalog as _);
    }
    options
        .castability
        .as_deref_mut()
        .and_then(BuffCastability::spell_catalog)
}
