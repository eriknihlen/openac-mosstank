//! Where a debuff can come from (`CombatItemDebuffPlanner`): a learned
//! spell, a profiled wand's spell, a profiled weapon's proc, or a grenade
//! the game database lists; ordered by the profile's debuff selection.

use std::collections::HashSet;

use ac_plugin_abi::character::Skill;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::ProjectilePathKind;
use mosstank_automation::SpellCatalog;
use mosstank_data::attack_spells::{find_skill, projectile_shape_for, range_meters};
use mosstank_dotnet::text::ordinal;
use mosstank_formats::gameinfo::VtankGrenadeOption;
use mosstank_settings::model::{CombatSettings, DebuffSelectionMethod};
use mosstank_settings::monster::MonsterRuleActions;

use crate::debuff::{self, DebuffIdentity};

/// What a debuff is cast from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatDebuffSourceKind {
    /// A spell the character knows.
    LearnedSpell,
    /// A wand's own spell.
    CasterItem,
    /// A weapon's proc.
    ProcWeapon,
    /// A grenade.
    Grenade,
}

/// One way to land a debuff.
#[derive(Debug, Clone, PartialEq)]
pub struct CombatDebuffSource {
    /// The debuff.
    pub identity: DebuffIdentity,
    /// The spell.
    pub spell: SpellInfo,
    /// What it is cast from.
    pub kind: CombatDebuffSourceKind,
    /// The item, 0 for a learned spell.
    pub item_object_id: u32,
    /// The skill it is cast with.
    pub source_skill: i32,
    /// Its place in the chain.
    pub action_order: i32,
    /// The flight shape to test the way with (`None`: no flight).
    pub path_kind: Option<ProjectilePathKind>,
}

impl CombatDebuffSource {
    /// Whether it is cast from an item.
    #[must_use]
    pub fn uses_item(&self) -> bool {
        self.kind != CombatDebuffSourceKind::LearnedSpell
    }
}

/// Whether a debuff is still owed (the C#'s `isDue`).
pub type IsDue<'a> = dyn FnMut(DebuffIdentity, &SpellInfo) -> bool + 'a;

const MELEE_WEAPON: u32 = 0x0000_0001;
const MISSILE_WEAPON: u32 = 0x0000_0100;
const CASTER: u32 = 0x0000_8000;
const WAR_MAGIC_SKILL: u32 = 34;
const VOID_MAGIC_SKILL: u32 = 43;
/// A GrenadeOptions wield requirement on a skill.
const SKILL_WIELD_REQUIREMENT: i32 = 2;

/// What the planner reads.
#[allow(missing_debug_implementations)]
pub struct DebuffPlanInput<'a> {
    /// The combat settings.
    pub settings: &'a CombatSettings,
    /// The character's skills.
    pub skills: &'a [Skill],
    /// The spell table and spellbook.
    pub spells: &'a mut dyn SpellCatalog,
    /// What the character carries.
    pub items: &'a [InventoryItem],
    /// The game database's grenades.
    pub grenades: &'a [VtankGrenadeOption],
    /// How far the monster is, metres (0: do not test reach).
    pub target_distance: f64,
}

/// Every source for every debuff the rule asks for that `is_due` admits.
pub fn candidates(
    actions: &MonsterRuleActions,
    input: &mut DebuffPlanInput<'_>,
    is_due: &mut dyn FnMut(DebuffIdentity, &SpellInfo) -> bool,
) -> Vec<CombatDebuffSource> {
    let required = debuff::required(actions);
    if required.is_empty() {
        return Vec::new();
    }
    collect(&required, input, Some(is_due), None)
}

/// Every source for one debuff.
pub fn sources(
    identity: DebuffIdentity,
    input: &mut DebuffPlanInput<'_>,
    log: Option<&mut dyn FnMut(&str)>,
) -> Vec<CombatDebuffSource> {
    let mut required = HashSet::new();
    required.insert(identity);
    collect(&required, input, None, log)
}

fn collect(
    required: &HashSet<DebuffIdentity>,
    input: &mut DebuffPlanInput<'_>,
    mut is_due: Option<&mut IsDue<'_>>,
    mut log: Option<&mut (dyn FnMut(&str) + '_)>,
) -> Vec<CombatDebuffSource> {
    let mut result = Vec::new();
    for spell in input.spells.known_combat_spells() {
        // A debuff the character cannot reach the monster with is not a
        // choice at all.
        if input.target_distance > 0.0
            && f64::from(range_meters(
                &spell,
                input.skills,
                input.settings.spell_range_fudge,
            )) < input.target_distance
        {
            continue;
        }
        let skill = current_skill(input.skills, spell.school);
        let shape = projectile_shape_for(spell.family);
        add_if_required(
            &mut result,
            required,
            spell,
            CombatDebuffSourceKind::LearnedSpell,
            0,
            skill,
            is_due.as_deref_mut(),
            shape,
        );
    }

    for item in input.items {
        let profiled = input
            .settings
            .combat_item_object_ids
            .contains(&item.object_id)
            || input.settings.combat_item_names.contains(&item.name);
        let consumable = input.settings.consumable_names.contains(&item.name);
        if !profiled && !consumable {
            continue;
        }
        let text = format!("{} [{}]", item.name, item.object_id);
        if let Some(log) = log.as_deref_mut() {
            log(&format!("Find debuff choice ({text}): Begin"));
        }
        let before = result.len();
        if profiled {
            add_profile_item(
                &mut result,
                required,
                item,
                input.spells,
                is_due.as_deref_mut(),
            );
        }
        if consumable {
            add_grenade(
                &mut result,
                required,
                item,
                input.grenades,
                input.skills,
                input.spells,
                is_due.as_deref_mut(),
            );
        }
        if result.len() == before {
            if let Some(log) = log.as_deref_mut() {
                log(&format!(
                    "Find debuff choice ({text}): Stop, wrong object type"
                ));
            }
            continue;
        }
        if let Some(log) = log.as_deref_mut() {
            let quality = result.last().map_or(0, |source| source.spell.quality());
            log(&format!(
                "Find debuff choice ({text}): Item set to be used, quality {quality}."
            ));
            log(&format!("Find debuff choice ({text}): Item tests done."));
        }
    }

    let selection = input.settings.debuff_selection_method;
    result.sort_by(|left, right| compare(left, right, selection));
    result
}

fn add_profile_item(
    result: &mut Vec<CombatDebuffSource>,
    required: &HashSet<DebuffIdentity>,
    item: &InventoryItem,
    spells: &mut dyn SpellCatalog,
    is_due: Option<&mut IsDue<'_>>,
) {
    if item.item_type & CASTER != 0
        && item.spell_id != 0
        && let Some(caster_spell) = spells.try_get(item.spell_id)
    {
        // A wand throws its own spell, so the spell's shape stands.
        let shape = projectile_shape_for(caster_spell.family);
        add_if_required(
            result,
            required,
            caster_spell,
            CombatDebuffSourceKind::CasterItem,
            item.object_id,
            item.item_spellcraft,
            is_due,
            shape,
        );
        return;
    }

    if item.item_type & (MELEE_WEAPON | MISSILE_WEAPON) == 0 {
        return;
    }
    for &spell_id in &item.appraised_spell_ids {
        let Some(proc_spell) = spells.try_get(spell_id) else {
            continue;
        };
        if !proc_spell.is_offensive
            || proc_spell.is_untargeted
            || proc_spell.school == WAR_MAGIC_SKILL
            || proc_spell.school == VOID_MAGIC_SKILL
        {
            continue;
        }
        // A launcher's proc flies the way its ammunition flies; a melee proc
        // has no flight.
        let shape = (item.item_type & MISSILE_WEAPON != 0).then_some(ProjectilePathKind::Missile);
        add_if_required(
            result,
            required,
            proc_spell,
            CombatDebuffSourceKind::ProcWeapon,
            item.object_id,
            item.item_spellcraft,
            is_due,
            shape,
        );
        // The first qualifying item spell is the one used.
        return;
    }
}

fn add_grenade(
    result: &mut Vec<CombatDebuffSource>,
    required: &HashSet<DebuffIdentity>,
    item: &InventoryItem,
    grenades: &[VtankGrenadeOption],
    skills: &[Skill],
    spells: &mut dyn SpellCatalog,
    is_due: Option<&mut IsDue<'_>>,
) {
    if item.item_type & MISSILE_WEAPON == 0 || item.combat_use != 0 {
        return;
    }
    let Some(grenade) = find_grenade(grenades, &item.name) else {
        return;
    };
    #[allow(clippy::cast_sign_loss)]
    if grenade.wield_requirement_type == SKILL_WIELD_REQUIREMENT
        && current_skill(skills, grenade.wield_requirement_attribute as u32)
            < grenade.wield_requirement_value
    {
        return;
    }
    // A row whose spell this client does not know is no grenade at all.
    let Some(spell) = spells.try_get(grenade.spell_id) else {
        return;
    };
    add_if_required(
        result,
        required,
        spell,
        CombatDebuffSourceKind::Grenade,
        item.object_id,
        grenade.spellcraft,
        is_due,
        Some(ProjectilePathKind::Missile),
    );
}

/// The first GrenadeOptions row with the name, any case.
fn find_grenade<'g>(
    grenades: &'g [VtankGrenadeOption],
    name: &str,
) -> Option<&'g VtankGrenadeOption> {
    grenades
        .iter()
        .find(|option| ordinal::equals_ignore_case(&option.name, name))
}

#[allow(clippy::too_many_arguments)]
fn add_if_required(
    result: &mut Vec<CombatDebuffSource>,
    required: &HashSet<DebuffIdentity>,
    spell: SpellInfo,
    kind: CombatDebuffSourceKind,
    item_object_id: u32,
    source_skill: i32,
    is_due: Option<&mut IsDue<'_>>,
    path_kind: Option<ProjectilePathKind>,
) {
    let Some((identity, action_order)) = debuff::classify(&spell) else {
        return;
    };
    if !required.contains(&identity) {
        return;
    }
    if let Some(is_due) = is_due
        && !is_due(identity, &spell)
    {
        return;
    }
    result.push(CombatDebuffSource {
        identity,
        spell,
        kind,
        item_object_id,
        source_skill,
        action_order,
        path_kind,
    });
}

fn compare(
    left: &CombatDebuffSource,
    right: &CombatDebuffSource,
    selection: DebuffSelectionMethod,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let skill = right.source_skill.cmp(&left.source_skill);
    let quality = right.spell.quality().cmp(&left.spell.quality());
    let first = if selection == DebuffSelectionMethod::SKILL {
        skill.then(quality)
    } else {
        quality.then(skill)
    };
    if first != Ordering::Equal {
        return first;
    }
    let left_direct = left.kind == CombatDebuffSourceKind::LearnedSpell;
    let right_direct = right.kind == CombatDebuffSourceKind::LearnedSpell;
    if left_direct != right_direct {
        return if left_direct {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    left.action_order
        .cmp(&right.action_order)
        .then(left.item_object_id.cmp(&right.item_object_id))
}

/// The character's current skill, 0 when it has none.
#[must_use]
pub fn current_skill(skills: &[Skill], skill_id: u32) -> i32 {
    find_skill(skills, skill_id).map_or(0, |skill| i32::try_from(skill.current).unwrap_or(i32::MAX))
}
