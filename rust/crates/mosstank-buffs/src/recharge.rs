//! Restoring a vital (`VitalRechargePlanner`): the ways a vital is
//! restored in the order the profile (or VTank's default table) lists them
//! for the stance and how low it is, each way's pick (a learned spell or a
//! caster's, a healing kit, food), and the helper: a hurt fellow or network
//! peer in reach, healed with the Healer's Heart or the strongest castable
//! tier of the helper spell's own line.
//!
//! [`crate::recharge_controller`] drives these, one use at a time.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::Position;
use mosstank_automation::Automation;
use mosstank_automation::components::uses_blacklisted_component;
use mosstank_automation::surface::is_assessed;
use mosstank_data::spell_id;
use mosstank_settings::model::{
    CombatSettings, ConsumableCategory, RechargeHandlerRow, VitalKind, VitalSettings,
};
use openac_plugin_compat::number;
use openac_plugin_compat::text::{casing, ordinal};

use crate::castability::{AutomationCastability, CastabilityReport};
use crate::plan::{BuffTierRejection, try_pick_tier};
use crate::profile::{BuffLine, BuffTargetKind};
use crate::vitals::{self, HEAL_SELF_STEM, REVITALIZE_STEM, STAMINA_TO_MANA_STEM, Vitals};

const HEALING_SKILL: u32 = 21;
const HEALING_KIT_PUBLIC_FLAG: u32 = 0x0001_0000;
const CASTER_ITEM_TYPE: u32 = 0x0000_8000;
const LIFE_MAGIC_SKILL: u32 = 33;
const ARCANE_LORE_SKILL: u32 = 14;

/// A fellow's vitals are trusted this long after the server last streamed
/// them, seconds; older samples (or none) mean "unknown", never "still
/// low".
pub const FELLOW_VITALS_TRUST_SECONDS: f64 = 10.0;

/// Where a recharge comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VitalRechargeSourceKind {
    /// A spell the character knows.
    #[default]
    LearnedSpell,
    /// A spell cast from a wand, orb or the like.
    CasterItem,
    /// A healing kit.
    Kit,
    /// Food or a potion.
    Food,
}

/// One recharge to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VitalRechargeChoice {
    /// The vital it restores.
    pub vital: VitalKind,
    /// Where it comes from.
    pub source_kind: VitalRechargeSourceKind,
    /// The spell's or the item's name.
    pub name: String,
    /// The spell (0 for an item use).
    pub spell_id: u32,
    /// The item (0 for a learned spell).
    pub item_object_id: u32,
    /// The stance it needs, if any.
    pub required_mode: Option<CombatMode>,
    /// Who it is cast or used on (0 for the character itself).
    pub target_object_id: u32,
}

impl VitalRechargeChoice {
    fn new(
        vital: VitalKind,
        source_kind: VitalRechargeSourceKind,
        name: &str,
        spell_id: u32,
        item_object_id: u32,
        required_mode: Option<CombatMode>,
    ) -> Self {
        Self {
            vital,
            source_kind,
            name: name.to_owned(),
            spell_id,
            item_object_id,
            required_mode,
            target_object_id: 0,
        }
    }

    /// Whether it uses an item (anything but a learned spell).
    #[must_use]
    pub fn uses_item(&self) -> bool {
        self.source_kind != VitalRechargeSourceKind::LearnedSpell
    }
}

/// A way of restoring a vital.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum VitalRechargeMethod {
    RegularSpell,
    StaminaToHealth,
    ManaToHealth,
    HealthToStamina,
    HealthToMana,
    Kit,
    Food,
}

impl VitalRechargeMethod {
    fn name(self) -> &'static str {
        match self {
            Self::RegularSpell => "RegularSpell",
            Self::StaminaToHealth => "StaminaToHealth",
            Self::ManaToHealth => "ManaToHealth",
            Self::HealthToStamina => "HealthToStamina",
            Self::HealthToMana => "HealthToMana",
            Self::Kit => "Kit",
            Self::Food => "Food",
        }
    }
}

/// A vital's name as the C# prints it.
#[must_use]
pub fn vital_name(vital: VitalKind) -> &'static str {
    match vital {
        VitalKind::Health => "Health",
        VitalKind::Stamina => "Stamina",
        VitalKind::Mana => "Mana",
    }
}

/// The vital a handler row's number names (1 health, 2 stamina, 3 mana).
#[must_use]
pub fn to_vital_kind(vital: i32) -> Option<VitalKind> {
    match vital {
        1 => Some(VitalKind::Health),
        2 => Some(VitalKind::Stamina),
        3 => Some(VitalKind::Mana),
        _ => None,
    }
}

/// A handler row's number for a vital.
#[must_use]
pub fn from_vital_kind(vital: VitalKind) -> i32 {
    match vital {
        VitalKind::Health => 1,
        VitalKind::Stamina => 2,
        VitalKind::Mana => 3,
    }
}

fn vitals_of(automation: &mut dyn Automation) -> Vitals {
    Vitals::from(&automation.character())
}

fn true_false(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// The ways to try for a vital, in order, and the first that has something
/// to use (`TryPlan`). `trace` receives the walk's reasons.
pub fn try_plan(
    vital: VitalKind,
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    combat: &CombatSettings,
    mut trace: Option<&mut dyn FnMut(&str)>,
) -> Option<VitalRechargeChoice> {
    let mode = automation.combat_mode();
    let percent = vitals::percent(&vitals_of(automation), vital);
    let rows = (!settings.recharge_handler_rows.is_empty())
        .then_some(settings.recharge_handler_rows.as_slice());
    let methods = handlers(
        vital,
        mode == CombatMode::Magic,
        percent,
        Some(&settings.recharge_handler_set),
        rows,
    );
    let items = automation.owned_items();

    if let Some(trace) = trace.as_deref_mut() {
        trace_kits(
            trace, vital, automation, settings, combat, &items, mode, percent, &methods,
        );
    }

    for method in methods {
        if let Some(choice) = try_handler(method, vital, automation, settings, combat, &items) {
            if let Some(trace) = trace.as_deref_mut() {
                trace(&format!(
                    "Recharge selected: handler={}, source={:?}, name={}",
                    method.name(),
                    choice.source_kind,
                    choice.name
                ));
            }
            return Some(choice);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn trace_kits(
    trace: &mut dyn FnMut(&str),
    vital: VitalKind,
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    combat: &CombatSettings,
    items: &[InventoryItem],
    mode: CombatMode,
    percent: i32,
    methods: &[VitalRechargeMethod],
) {
    let character = automation.character();
    let skill = automation.try_get_skill(HEALING_SKILL);
    let has_healing = skill.is_some();
    let healing = skill.unwrap_or_default();
    let vitals = Vitals::from(&character);
    let methods: Vec<&str> = methods.iter().map(|m| m.name()).collect();
    trace(&format!(
        "Recharge check: vital={}, percent={percent}, mode={mode:?}, handlers={}, health={}/{}, stamina={}/{}, Healing={}/{:?}, skillPresent={}, kitsInMagic={}, minimumKitChance={}, healthThresholds={}/{}",
        vital_name(vital),
        methods.join(","),
        vitals.health.0,
        vitals.health.1,
        vitals.stamina.0,
        vitals.stamina.1,
        healing.current,
        healing.training,
        true_false(has_healing),
        true_false(settings.use_kits_in_magic_mode),
        settings.minimum_heal_kit_success_chance,
        number::to_string_f64(settings.normal_health),
        number::to_string_f64(settings.no_target_health),
    ));
    let trained = matches!(healing.training, Training::Trained | Training::Specialized);
    for item in items {
        let configured = combat.consumable_names.contains(item.name.as_str());
        if !configured
            && item.public_flags & HEALING_KIT_PUBLIC_FLAG == 0
            && !ordinal::contains_ignore_case(&item.name, "kit")
        {
            continue;
        }
        let chance =
            heal_kit_chance(healing.current, item.boost_value, &vitals, vital, mode) * 100.0;
        let rejection = if !configured {
            "not configured"
        } else if !is_assessed(automation, item.object_id) {
            "awaiting assessment"
        } else if mode == CombatMode::Magic && !settings.use_kits_in_magic_mode {
            "kits disabled in magic"
        } else if vital != VitalKind::Stamina && vitals.stamina.0 < 15 {
            "stamina below 15"
        } else if !has_healing || !trained {
            "Healing not trained"
        } else if item.booster_vital != vital as i32 {
            "vital does not match"
        } else if item.public_flags & HEALING_KIT_PUBLIC_FLAG == 0 {
            "missing healer flag"
        } else if i64::from(item.use_requires_skill_level) > i64::from(healing.current) {
            "skill level too low"
        } else if item.use_requires_skill_specialized != 0
            && healing.training != Training::Specialized
        {
            "specialization required"
        } else if chance < f64::from(settings.minimum_heal_kit_success_chance) {
            "success chance below minimum"
        } else {
            "eligible"
        };
        trace(&format!(
            "Kit check: {} (0x{:08X}), result={rejection}, configured={}, flags=0x{:08X}, boosterVital={}, bonus={}, modifier={}, uses={}, requiredSkill={}, requiredLevel={}, requiredSpec={}, chance={}%",
            item.name,
            item.object_id,
            true_false(configured),
            item.public_flags,
            item.booster_vital,
            item.boost_value,
            number::to_string_f64(item.heal_kit_modifier),
            item.structure,
            item.use_requires_skill,
            item.use_requires_skill_level,
            item.use_requires_skill_specialized,
            number::format_f64(chance, "F2").expect("a fixed format"),
        ));
    }
}

/// The fellow (or peer) to help and how (`TryPlanHelper`): health, then
/// stamina, then mana, each the lowest in its own reach below its own
/// threshold. `trace` receives the helper spell walk's pick and
/// rejections in the buff pass's wording.
pub fn try_plan_helper(
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    combat: &CombatSettings,
    trace: Option<&mut dyn FnMut(&str)>,
) -> Option<VitalRechargeChoice> {
    let in_fellowship = automation.in_fellowship();
    if !settings.help_others || (!in_fellowship && !settings.help_network_peers) {
        return None;
    }
    let mut report = HelperReport { trace };
    let members = helper_candidates(automation, settings, in_fellowship);
    for (vital, threshold, distance, base_spell) in [
        (
            VitalKind::Health,
            settings.helper_health,
            settings.helper_health_distance,
            spell_id::AdjaSGift,
        ),
        (
            VitalKind::Stamina,
            settings.helper_stamina,
            settings.helper_stamina_distance,
            spell_id::Replenish,
        ),
        (
            VitalKind::Mana,
            settings.helper_mana,
            settings.helper_mana_distance,
            spell_id::GiftOfEssence,
        ),
    ] {
        #[allow(clippy::cast_possible_truncation)]
        let Some(fellow) = lowest(&members, vital, threshold, distance as f32) else {
            continue;
        };
        if vital == VitalKind::Health
            && let Some(choice) = try_healers_heart(automation, settings, combat, fellow)
        {
            return Some(choice);
        }
        let Some(basis) = automation.spells().try_get(base_spell) else {
            continue;
        };
        let Some(spell) = try_resolve_helper_spell(automation, &basis, combat, &mut report) else {
            continue;
        };
        let mut choice = VitalRechargeChoice::new(
            vital,
            VitalRechargeSourceKind::LearnedSpell,
            &spell.name,
            spell.spell_id,
            0,
            Some(CombatMode::Magic),
        );
        choice.target_object_id = fellow.object_id;
        return Some(choice);
    }
    None
}

/// The Healer's Heart (or the Legendary Seed of Mornings, which outranks
/// it) on a hurt fellow, when the profile lists it and the character has the
/// Life Magic (245) and Arcane Lore (105) it needs.
fn try_healers_heart(
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    combat: &CombatSettings,
    target: &FellowMember,
) -> Option<VitalRechargeChoice> {
    // The item-use slot is the caller's gate, as the reference gates this
    // row: never the host's inventory transaction state, which a cast of
    // its own raises.
    if !settings.use_healers_heart {
        return None;
    }
    let life = automation.try_get_skill(LIFE_MAGIC_SKILL)?;
    if life.current < 245 {
        return None;
    }
    let lore = automation.try_get_skill(ARCANE_LORE_SKILL)?;
    if lore.current < 105 {
        return None;
    }
    let character_id = automation.character().id;
    let mut selected: Option<InventoryItem> = None;
    let mut rank = 0;
    for item in automation.owned_items() {
        if !combat.combat_item_names.contains(item.name.as_str()) {
            continue;
        }
        if item.container_id != character_id && item.wielder_id != character_id {
            continue;
        }
        let candidate_rank = match item.name.as_str() {
            "Legendary Seed of Mornings" => 2,
            "The Healer's Heart" => 1,
            _ => 0,
        };
        if candidate_rank <= rank {
            continue;
        }
        selected = Some(item);
        rank = candidate_rank;
    }
    let selected = selected.filter(|item| item.object_id != 0)?;
    let mut choice = VitalRechargeChoice::new(
        VitalKind::Health,
        VitalRechargeSourceKind::CasterItem,
        &selected.name,
        0,
        selected.object_id,
        None,
    );
    choice.target_object_id = target.object_id;
    Some(choice)
}

/// The ways to try, in order (`Handlers`): the profile's rows for this
/// vital, stance and percentage when it has any that match, else the
/// handler set's entry for the context, else VTank's default table.
#[must_use]
pub fn handlers(
    vital: VitalKind,
    magic_mode: bool,
    current_percent: i32,
    handler_set: Option<&str>,
    handler_rows: Option<&[RechargeHandlerRow]>,
) -> Vec<VitalRechargeMethod> {
    use VitalRechargeMethod::{Food, Kit, ManaToHealth, RegularSpell, StaminaToHealth};
    if let Some(rows) = handler_rows.filter(|rows| !rows.is_empty()) {
        let from_rows = handlers_from_rows(vital, magic_mode, current_percent, rows);
        if !from_rows.is_empty() {
            return from_rows;
        }
    }

    let defaults = if magic_mode {
        match vital {
            VitalKind::Health if current_percent <= 15 => {
                vec![StaminaToHealth, ManaToHealth, RegularSpell, Food, Kit]
            }
            VitalKind::Health => vec![Kit, StaminaToHealth, ManaToHealth, RegularSpell, Food],
            VitalKind::Stamina => vec![Kit, RegularSpell, Food],
            VitalKind::Mana => vec![Kit, Food, RegularSpell],
        }
    } else {
        match vital {
            VitalKind::Health if current_percent <= 10 => {
                vec![Food, Kit, StaminaToHealth, RegularSpell]
            }
            VitalKind::Health if current_percent <= 15 => vec![Food, Kit, RegularSpell],
            VitalKind::Health => vec![Kit, Food, RegularSpell],
            VitalKind::Stamina | VitalKind::Mana => vec![Kit, Food, RegularSpell],
        }
    };

    try_parse_handler_set(
        handler_set,
        &handler_context(vital, magic_mode, current_percent),
    )
    .unwrap_or(defaults)
}

fn handlers_from_rows(
    vital: VitalKind,
    magic_mode: bool,
    current_percent: i32,
    rows: &[RechargeHandlerRow],
) -> Vec<VitalRechargeMethod> {
    let vital_code = from_vital_kind(vital);
    let stance = if magic_mode { 1 } else { 2 };
    rows.iter()
        .filter(|row| {
            row.vital == vital_code
                && row.stance == stance
                && current_percent >= row.min_percent
                && current_percent <= row.max_percent
        })
        .filter_map(|row| parse_handler_token(&row.handler_string))
        .collect()
}

/// A handler's name, spaces and hyphens dropped, ignoring case.
fn parse_handler_token(source: &str) -> Option<VitalRechargeMethod> {
    let normalized = casing::to_lower_invariant(&source.replace([' ', '-'], ""));
    Some(match normalized.as_str() {
        "regularspell" => VitalRechargeMethod::RegularSpell,
        "staminatohealth" => VitalRechargeMethod::StaminaToHealth,
        "manatohealth" => VitalRechargeMethod::ManaToHealth,
        "healthtostamina" => VitalRechargeMethod::HealthToStamina,
        "healthtomana" => VitalRechargeMethod::HealthToMana,
        "kit" | "kitrecharge" => VitalRechargeMethod::Kit,
        "food" | "rechargewithfood" => VitalRechargeMethod::Food,
        _ => return None,
    })
}

/// `stance-vital-band`: `magic`/`combat`, the vital, and for health
/// `critical` (combat only, 10 % or less), `low` (15 % or less) or
/// `normal`.
fn handler_context(vital: VitalKind, magic_mode: bool, current_percent: i32) -> String {
    let stance = if magic_mode { "magic" } else { "combat" };
    let vital_name = casing::to_lower_invariant(vital_name(vital));
    let band = if vital != VitalKind::Health {
        "normal"
    } else if magic_mode {
        if current_percent <= 15 {
            "low"
        } else {
            "normal"
        }
    } else if current_percent <= 10 {
        "critical"
    } else if current_percent <= 15 {
        "low"
    } else {
        "normal"
    };
    format!("{stance}-{vital_name}-{band}")
}

/// The handler set: one list for every context, or `context=list` entries
/// separated by `;` (a `critical` or `low` context falls back to its
/// `normal` entry); a list separates handlers with `,`, `>` or `|`.
fn try_parse_handler_set(source: Option<&str>, context: &str) -> Option<Vec<VitalRechargeMethod>> {
    let source = source.filter(|s| !casing::is_white_space_only(s))?;
    if ordinal::equals_ignore_case(source, "RechargeHandlerSet") {
        return None;
    }

    let mut selected = source.to_owned();
    if source.contains('=') {
        selected = String::new();
        let fallback_context = if context.ends_with("-critical") || context.ends_with("-low") {
            let dash = context.rfind('-').unwrap_or(0);
            format!("{}-normal", &context[..dash])
        } else {
            String::new()
        };
        for segment in source.split(';').map(dotnet_trim).filter(|s| !s.is_empty()) {
            let Some(equals) = segment.find('=').filter(|&at| at > 0) else {
                continue;
            };
            let key = dotnet_trim(&segment[..equals]);
            if ordinal::equals_ignore_case(key, context) {
                selected = segment[equals + 1..].to_owned();
                break;
            }
            if selected.is_empty()
                && !fallback_context.is_empty()
                && ordinal::equals_ignore_case(key, &fallback_context)
            {
                selected = segment[equals + 1..].to_owned();
            }
        }
    }
    if casing::is_white_space_only(&selected) {
        return None;
    }

    let parsed: Vec<VitalRechargeMethod> = selected
        .split([',', '>', '|'])
        .map(dotnet_trim)
        .filter(|token| !token.is_empty())
        .filter_map(parse_handler_token)
        .collect();
    (!parsed.is_empty()).then_some(parsed)
}

/// `string.Trim()`: .NET's white space at both ends.
fn dotnet_trim(text: &str) -> &str {
    text.trim_matches(casing::is_white_space)
}

fn try_handler(
    method: VitalRechargeMethod,
    vital: VitalKind,
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    combat: &CombatSettings,
    items: &[InventoryItem],
) -> Option<VitalRechargeChoice> {
    match method {
        VitalRechargeMethod::Kit => {
            return try_kit(vital, automation, settings, combat, items);
        }
        VitalRechargeMethod::Food => return try_food(vital, automation, combat, items),
        _ => {}
    }

    let (stem, source_vital) = spell_stem(method, vital);
    if stem.is_empty() {
        return None;
    }
    if let Some(source) = source_vital
        && vital != VitalKind::Health
        && vitals::is_below_normal(&vitals_of(automation), settings, source)
    {
        return None;
    }
    let choice = try_spell(vital, stem, automation, combat, items)?;
    if matches!(
        method,
        VitalRechargeMethod::StaminaToHealth | VitalRechargeMethod::ManaToHealth
    ) && !conversion_worthwhile(method, choice.spell_id, automation, settings)
    {
        return None;
    }
    Some(choice)
}

fn spell_stem(method: VitalRechargeMethod, vital: VitalKind) -> (&'static str, Option<VitalKind>) {
    match method {
        VitalRechargeMethod::RegularSpell => match vital {
            VitalKind::Health => (HEAL_SELF_STEM, None),
            VitalKind::Stamina => (REVITALIZE_STEM, None),
            VitalKind::Mana => (STAMINA_TO_MANA_STEM, Some(VitalKind::Stamina)),
        },
        VitalRechargeMethod::StaminaToHealth => ("Stamina to Health", Some(VitalKind::Stamina)),
        VitalRechargeMethod::ManaToHealth => ("Mana to Health", Some(VitalKind::Mana)),
        VitalRechargeMethod::HealthToStamina => ("Health to Stamina", Some(VitalKind::Health)),
        VitalRechargeMethod::HealthToMana => ("Health to Mana", Some(VitalKind::Health)),
        VitalRechargeMethod::Kit | VitalRechargeMethod::Food => ("", None),
    }
}

/// The strongest spell by the stem, learned or on a profiled caster the
/// client has identified; a learned spell wins a tie.
fn try_spell(
    vital: VitalKind,
    stem: &str,
    automation: &mut dyn Automation,
    combat: &CombatSettings,
    items: &[InventoryItem],
) -> Option<VitalRechargeChoice> {
    let mut best: Option<(SpellInfo, u32)> = None;

    for spell in automation.spells().known_self_buffs() {
        if !matches_stem(&spell.name, stem) {
            continue;
        }
        if uses_blacklisted_component(
            automation.spells(),
            &spell,
            &combat.blacklisted_spell_components,
        ) {
            continue;
        }
        if !can_cast(automation, &spell) {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(current, item)| better(&spell, 0, current, *item))
        {
            best = Some((spell, 0));
        }
    }

    for item in items {
        if !combat.combat_item_object_ids.contains(&item.object_id)
            && !combat.combat_item_names.contains(item.name.as_str())
        {
            continue;
        }
        if item.item_type & CASTER_ITEM_TYPE == 0 {
            continue;
        }
        if !is_assessed(automation, item.object_id) {
            continue;
        }
        for spell_id in item_spell_ids(item) {
            let Some(spell) = automation.spells().try_get(spell_id) else {
                continue;
            };
            if !matches_stem(&spell.name, stem) {
                continue;
            }
            if uses_blacklisted_component(
                automation.spells(),
                &spell,
                &combat.blacklisted_spell_components,
            ) {
                continue;
            }
            if best.as_ref().is_none_or(|(current, current_item)| {
                better(&spell, item.object_id, current, *current_item)
            }) {
                best = Some((spell, item.object_id));
            }
        }
    }

    let (spell, item) = best?;
    let source = if item == 0 {
        VitalRechargeSourceKind::LearnedSpell
    } else {
        VitalRechargeSourceKind::CasterItem
    };
    Some(VitalRechargeChoice::new(
        vital,
        source,
        &spell.name,
        spell.spell_id,
        item,
        Some(CombatMode::Magic),
    ))
}

fn kit_category_for(vital: VitalKind) -> ConsumableCategory {
    match vital {
        VitalKind::Stamina => ConsumableCategory::StaminaKit,
        VitalKind::Mana => ConsumableCategory::ManaKit,
        VitalKind::Health => ConsumableCategory::HealthKit,
    }
}

fn food_category_for(vital: VitalKind) -> ConsumableCategory {
    match vital {
        VitalKind::Stamina => ConsumableCategory::StaminaFood,
        VitalKind::Mana => ConsumableCategory::ManaFood,
        VitalKind::Health => ConsumableCategory::HealthFood,
    }
}

/// The profiled kit for the vital that restores the most (the fewest uses
/// left on a tie), with the Healing skill to carry its success chance.
fn try_kit(
    vital: VitalKind,
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    combat: &CombatSettings,
    items: &[InventoryItem],
) -> Option<VitalRechargeChoice> {
    let mode = automation.combat_mode();
    if mode == CombatMode::Magic && !settings.use_kits_in_magic_mode {
        return None;
    }
    let vitals = vitals_of(automation);
    if vital != VitalKind::Stamina && vitals.stamina.0 < 15 {
        return None;
    }
    let healing = automation.try_get_skill(HEALING_SKILL)?;
    if !matches!(healing.training, Training::Trained | Training::Specialized) {
        return None;
    }

    let mut best: Option<(&InventoryItem, f64)> = None;
    for item in items {
        if !combat.consumable_names.contains(item.name.as_str()) {
            continue;
        }
        // The reference never appraises a kit: its kind comes from the
        // profile and its bonuses from the game-info table, by name. An
        // appraised kit is judged by its own properties; an unappraised one
        // by the profile's kind and the table.
        let (skill_bonus, restore_bonus) = if is_assessed(automation, item.object_id) {
            if item.booster_vital != vital as i32
                || item.public_flags & HEALING_KIT_PUBLIC_FLAG == 0
                || i64::from(item.use_requires_skill_level) > i64::from(healing.current)
                || (item.use_requires_skill_specialized != 0
                    && healing.training != Training::Specialized)
            {
                continue;
            }
            (item.boost_value, item.heal_kit_modifier)
        } else {
            if combat.consumable_categories.get(item.name.as_str())
                != Some(&kit_category_for(vital))
            {
                continue;
            }
            match combat.heal_kits.get(item.name.as_str()) {
                Some(row) => (row.skill_bonus, row.restore_bonus),
                None => (0, 1.0),
            }
        };
        if heal_kit_chance(healing.current, skill_bonus, &vitals, vital, mode) * 100.0
            < f64::from(settings.minimum_heal_kit_success_chance)
        {
            continue;
        }
        let wins = match best {
            None => true,
            Some((current, current_restore)) => {
                restore_bonus > current_restore
                    || (restore_bonus == current_restore && item.structure < current.structure)
            }
        };
        if wins {
            best = Some((item, restore_bonus));
        }
    }
    let (best, _) = best?;
    Some(VitalRechargeChoice::new(
        vital,
        VitalRechargeSourceKind::Kit,
        &best.name,
        0,
        best.object_id,
        settings
            .go_to_peace_mode_to_use_kits
            .then_some(CombatMode::Peace),
    ))
}

/// The first profiled food for the vital.
fn try_food(
    vital: VitalKind,
    automation: &mut dyn Automation,
    combat: &CombatSettings,
    items: &[InventoryItem],
) -> Option<VitalRechargeChoice> {
    for item in items {
        if !combat.consumable_names.contains(item.name.as_str()) {
            continue;
        }
        // As with kits: an appraised item by its properties, an unappraised
        // one by the kind the profile gives it.
        let eligible = if is_assessed(automation, item.object_id) {
            item.booster_vital == vital as i32
                && item.public_flags & HEALING_KIT_PUBLIC_FLAG == 0
                && i64::from(item.use_requires_skill) != i64::from(HEALING_SKILL)
        } else {
            combat.consumable_categories.get(item.name.as_str()) == Some(&food_category_for(vital))
        };
        if eligible {
            return Some(VitalRechargeChoice::new(
                vital,
                VitalRechargeSourceKind::Food,
                &item.name,
                0,
                item.object_id,
                None,
            ));
        }
    }
    None
}

/// A healing kit's success chance: a logistic curve over the Healing skill
/// plus the kit's bonus against twice (2.2 times outside peace) the points
/// missing, rounded up.
#[must_use]
pub fn heal_kit_chance(
    healing_skill: u32,
    skill_bonus: i32,
    vitals: &Vitals,
    vital: VitalKind,
    mode: CombatMode,
) -> f64 {
    let (current, maximum) = vitals.read(vital);
    let multiplier = if mode == CombatMode::Peace { 2.0 } else { 2.2 };
    let missing = (f64::from(maximum) - f64::from(current)).max(0.0);
    let difficulty = (multiplier * missing).ceil();
    1.0 - 1.0
        / (1.0 + (0.03 * (f64::from(healing_skill) + f64::from(skill_bonus) - difficulty)).exp())
}

/// Whether turning stamina or mana into health beats the ordinary heal.
fn conversion_worthwhile(
    method: VitalRechargeMethod,
    conversion_spell_id: u32,
    automation: &mut dyn Automation,
    settings: &VitalSettings,
) -> bool {
    let Some(conversion) = automation.spells().try_get(conversion_spell_id) else {
        return false;
    };

    let mut ordinary_heal = 0;
    for spell in automation.spells().known_self_buffs() {
        if !matches_stem(&spell.name, HEAL_SELF_STEM) || !can_cast(automation, &spell) {
            continue;
        }
        ordinary_heal = ordinary_heal.max(estimated_ordinary_heal(&spell.name));
    }

    let vitals = vitals_of(automation);
    let missing = saturate(i64::from(vitals.health.1) - i64::from(vitals.health.0)).max(0);
    if ordinary_heal > missing {
        return false;
    }

    let stamina_source = method == VitalRechargeMethod::StaminaToHealth;
    let source_current = if stamina_source {
        saturate(i64::from(vitals.stamina.0))
    } else {
        saturate(i64::from(vitals.mana.0)).saturating_sub(30)
    }
    .max(0);
    let converted = missing.min(estimated_transfer(&conversion.name, source_current));
    let multiplier = if stamina_source {
        settings.stamina_to_health_multiplier
    } else {
        settings.mana_to_health_multiplier
    };
    let ordinary = f64::from(ordinary_heal) * multiplier;
    ordinary < f64::from(missing) && ordinary < f64::from(converted)
}

/// A count past the 32-bit range (the C#'s `checked` conversion throws
/// there) held at its edge.
fn saturate(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

/// What a heal spell restores, by name, for the conversion test.
#[must_use]
pub fn estimated_ordinary_heal(spell_name: &str) -> i32 {
    match spell_name {
        "Heal Self I" => 17,
        "Heal Self II" => 25,
        "Heal Self III" => 32,
        "Heal Self IV" => 45,
        "Heal Self V" => 67,
        "Heal Self VI" => 87,
        "Adja's Intervention" => 115,
        "Incantation of Heal Self" => 135,
        _ => 10,
    }
}

/// What a vital transfer moves, by its level (the name's numeral): a
/// multiple of the source, capped for the four lowest levels.
#[must_use]
pub fn estimated_transfer(spell_name: &str, source_current: i32) -> i32 {
    let (multiplier, cap) = if spell_name.ends_with(" I") {
        (0.9, 50)
    } else if spell_name.ends_with(" II") {
        (1.0, 100)
    } else if spell_name.ends_with(" III") {
        (1.1, 150)
    } else if spell_name.ends_with(" IV") {
        (1.2, 200)
    } else if spell_name.ends_with(" V") {
        (1.35, i32::MAX)
    } else if spell_name.ends_with(" VI") {
        (1.5, i32::MAX)
    } else {
        (1.75, i32::MAX)
    };
    #[allow(clippy::cast_possible_truncation)]
    let moved = (f64::from(source_current) * multiplier).floor() as i32;
    cap.min(moved)
}

/// Who the helper may cast on (`HelperCandidates`): the fellows, plus, when
/// the profile helps network peers, the network's other clients who are not
/// fellows, described as a fellow would be (their last reported vitals,
/// their distance from here in metres).
pub fn helper_candidates(
    automation: &mut dyn Automation,
    settings: &VitalSettings,
    in_fellowship: bool,
) -> Vec<FellowMember> {
    let fellows = if in_fellowship {
        automation.fellowship_members()
    } else {
        Vec::new()
    };
    if !settings.help_network_peers || !automation.network_available() {
        return fellows;
    }

    let mut known: HashSet<u32> = fellows.iter().map(|fellow| fellow.object_id).collect();
    let mut candidates = fellows;
    let character = automation.character();
    let here = automation.position();
    for peer in automation.network_clients() {
        if peer.player_id == 0 || peer.player_id == character.id || !known.insert(peer.player_id) {
            continue;
        }
        if !character.world.is_empty()
            && !peer.world_name.is_empty()
            && !ordinal::equals_ignore_case(&character.world, &peer.world_name)
        {
            continue;
        }
        candidates.push(FellowMember {
            object_id: peer.player_id,
            name: peer.name.clone(),
            current_health: peer.current_health,
            max_health: peer.max_health,
            current_stamina: peer.current_stamina,
            max_stamina: peer.max_stamina,
            current_mana: peer.current_mana,
            max_mana: peer.max_mana,
            distance: peer_distance(&here, &peer.position),
            share_loot: false,
            vitals_age_seconds: Some(0.0),
        });
    }
    candidates
}

fn peer_distance(here: &Position, there: &Position) -> f32 {
    let flat = here.horizontal_distance_metres(there);
    let rise = (here.elevation - there.elevation) * 240.0;
    let metres = (flat * flat + rise * rise).sqrt();
    #[allow(clippy::cast_possible_truncation)]
    if metres.is_finite() {
        metres as f32
    } else {
        f32::MAX
    }
}

/// The member lowest on the vital below the threshold, within reach, with
/// vitals streamed recently enough to trust.
fn lowest(
    members: &[FellowMember],
    vital: VitalKind,
    threshold: f64,
    maximum_distance: f32,
) -> Option<&FellowMember> {
    let mut best: Option<&FellowMember> = None;
    let mut best_fraction = f64::INFINITY;
    for member in members {
        let Some(age) = member.vitals_age_seconds else {
            continue;
        };
        if age >= FELLOW_VITALS_TRUST_SECONDS || member.distance > maximum_distance {
            continue;
        }
        let (current, maximum) = match vital {
            VitalKind::Health => (member.current_health, member.max_health),
            VitalKind::Stamina => (member.current_stamina, member.max_stamina),
            VitalKind::Mana => (member.current_mana, member.max_mana),
        };
        if maximum == 0 {
            continue;
        }
        let fraction = f64::from(current) / f64::from(maximum);
        if fraction < threshold && fraction < best_fraction {
            best = Some(member);
            best_fraction = fraction;
        }
    }
    best
}

/// The helper spell walk's report: its warnings, and its pick in the buff
/// pass's tier-trace wording.
struct HelperReport<'t> {
    trace: Option<&'t mut dyn FnMut(&str)>,
}

impl CastabilityReport for HelperReport<'_> {
    fn warn(&mut self, text: &str) {
        if let Some(trace) = self.trace.as_deref_mut() {
            trace(text);
        }
    }

    fn note_tier_pick(
        &mut self,
        line: &BuffLine,
        pick: Option<&SpellInfo>,
        rejections: &[BuffTierRejection],
    ) {
        if let Some(trace) = self.trace.as_deref_mut() {
            trace(&format_helper_trace(line, pick, rejections));
        }
    }
}

/// The helper spell: the best castable tier of the reference's family that
/// stays on the reference's own line (a family holds both the Self and the
/// Other line; only the component set tells them apart), with the hunting
/// skill margin.
fn try_resolve_helper_spell(
    automation: &mut dyn Automation,
    reference: &SpellInfo,
    combat: &CombatSettings,
    report: &mut HelperReport<'_>,
) -> Option<SpellInfo> {
    let mut tiers: Vec<SpellInfo> = automation
        .spells()
        .known_self_buffs()
        .into_iter()
        .filter(|candidate| candidate.family == reference.family)
        .collect();
    tiers.sort_by(|a, b| {
        b.tier
            .cmp(&a.tier)
            .then_with(|| b.quality().cmp(&a.quality()))
            .then_with(|| a.spell_id.cmp(&b.spell_id))
    });

    let mut skill_levels: HashMap<u32, u32> = HashMap::new();
    for skill in automation.skills() {
        skill_levels.insert(skill.skill_id, skill.current);
    }

    let owned = if automation.items_available() {
        automation.owned_items()
    } else {
        Vec::new()
    };
    let mut castability = AutomationCastability::new(
        automation,
        owned,
        &combat.blacklisted_spell_components,
        report,
    );
    let mut line = BuffLine::new(
        reference.family,
        BuffTargetKind::Other,
        &reference.name,
        tiers,
    );
    line.reference_override = Some(reference.clone());
    try_pick_tier(
        &line,
        &skill_levels,
        combat.hunt_skill_excess_over_difficulty,
        Some(&mut castability),
    )
}

/// The pick, then every rejected tier above it with its reason (all of them
/// when nothing was picked).
fn format_helper_trace(
    line: &BuffLine,
    pick: Option<&SpellInfo>,
    rejections: &[BuffTierRejection],
) -> String {
    let floor = pick.map_or(i32::MIN, |spell| spell.tier);
    let higher: Vec<String> = rejections
        .iter()
        .filter(|rejection| rejection.spell.tier > floor)
        .map(|rejection| format!("{} {}", rejection.spell.name, rejection.reason))
        .collect();
    let picked = match pick {
        Some(spell) => format!("picked {} (gen {})", spell.name, spell.tier),
        None => "picked nothing".to_owned(),
    };
    let rejected = if higher.is_empty() {
        "none".to_owned()
    } else {
        higher.join(", ")
    };
    format!(
        "Helping: {} \u{2014} {picked}; rejected: {rejected}",
        line.reference().name
    )
}

fn can_cast(automation: &mut dyn Automation, spell: &SpellInfo) -> bool {
    spell.school == 0
        || automation
            .try_get_skill(spell.school)
            .is_none_or(|skill| i64::from(skill.current) >= i64::from(spell.difficulty))
}

fn better(
    candidate: &SpellInfo,
    candidate_item: u32,
    current: &SpellInfo,
    current_item: u32,
) -> bool {
    if candidate.quality() != current.quality() {
        return candidate.quality() > current.quality();
    }
    // A direct learned spell wins the final source tie.
    if (candidate_item == 0) != (current_item == 0) {
        return candidate_item == 0;
    }
    candidate_item < current_item
}

/// The name holds the stem (ignoring case), or is the stem's level-boost
/// spell.
fn matches_stem(name: &str, stem: &str) -> bool {
    if ordinal::contains_ignore_case(name, stem) {
        return true;
    }
    match stem {
        HEAL_SELF_STEM => ordinal::equals_ignore_case(name, "Adja's Intervention"),
        REVITALIZE_STEM => ordinal::equals_ignore_case(name, "Robustification"),
        STAMINA_TO_MANA_STEM => ordinal::equals_ignore_case(name, "Meditative Trance"),
        _ => false,
    }
}

fn item_spell_ids(item: &InventoryItem) -> Vec<u32> {
    let mut ids = Vec::new();
    if item.spell_id != 0 {
        ids.push(item.spell_id);
    }
    ids.extend(
        item.appraised_spell_ids
            .iter()
            .copied()
            .filter(|&id| id != 0 && id != item.spell_id),
    );
    ids
}
