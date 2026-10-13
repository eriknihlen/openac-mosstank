//! The JSON documents older MossTank versions kept a settings profile in
//! (`profiles/macro/<hash>.json`, the roster `profiles/index.json`, and the
//! single `profile.json` before that), read once to migrate them into a
//! `.usd` and its side-car; and the store's preferences document.

use mosstank_dotnet::collections::OrdinalStringSet;
use mosstank_dotnet::text::casing;
use mosstank_settings::model::{
    BuffSettings, CombatSettings, ConsumableCategory, DebuffEachFirst, DebuffSelectionMethod,
    InventorySettings, MeleeAttackHeight, PetRangeMode, TargetSelectionMethod, UseArcsMode,
    VitalSettings,
};
use mosstank_settings::monster::MonsterRule;
use openac_plugin_compat::json::{self, JsonValue};

use crate::sidecar::{
    DynamicSettingDocument, DynamicSettings, LootRuleDocument, MonsterRuleDocument,
};
use crate::stj::{self, JsonError, Object};

/// The meta engine's own interval limits and default, in milliseconds.
pub mod meta_interval {
    /// The fastest the meta is looked at.
    pub const MINIMUM_MILLISECONDS: i32 = 50;
    /// The slowest.
    pub const MAXIMUM_MILLISECONDS: i32 = 2000;
    /// The default: `Math.Round(0.293 * 1000)`.
    pub const DEFAULT_MILLISECONDS: i32 = 293;
}

/// The store's own preferences (`PreferencesDocument`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferencesDocument {
    /// The picker shows only this character's profiles.
    pub mine_only: bool,
    /// How often the meta is looked at.
    pub meta_interval_milliseconds: i32,
    /// How long a game database stays fresh, in hours.
    pub game_db_check_interval_hours: i32,
}

impl Default for PreferencesDocument {
    fn default() -> Self {
        Self {
            mine_only: true,
            meta_interval_milliseconds: meta_interval::DEFAULT_MILLISECONDS,
            game_db_check_interval_hours: 6,
        }
    }
}

impl PreferencesDocument {
    /// The text the C# writes.
    #[must_use]
    pub fn write(&self) -> String {
        stj::write(&JsonValue::object([
            ("MineOnly", JsonValue::Bool(self.mine_only)),
            (
                "MetaIntervalMilliseconds",
                stj::int(self.meta_interval_milliseconds),
            ),
            (
                "GameDbCheckIntervalHours",
                stj::int(self.game_db_check_interval_hours),
            ),
        ]))
    }

    /// Reads one; `None` for JSON `null`.
    ///
    /// # Errors
    /// Not JSON, or a value of the wrong kind.
    pub fn read(text: &str) -> Result<Option<Self>, JsonError> {
        let document = json::parse(text).map_err(|error| error.to_string())?;
        let Some(o) = Object::of(&document, "$", "PreferencesDocument")? else {
            return Ok(None);
        };
        let defaults = Self::default();
        Ok(Some(Self {
            mine_only: o.bool("MineOnly", defaults.mine_only)?,
            meta_interval_milliseconds: o.int(
                "MetaIntervalMilliseconds",
                defaults.meta_interval_milliseconds,
            )?,
            game_db_check_interval_hours: o.int(
                "GameDbCheckIntervalHours",
                defaults.game_db_check_interval_hours,
            )?,
        }))
    }
}

/// One row of the old roster: a named profile and who made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRosterEntry {
    /// The profile's name.
    pub name: Option<String>,
    /// The character that made it.
    pub owner: Option<String>,
}

/// The old roster of named profiles (`LegacyRosterDocument`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LegacyRosterDocument {
    /// Its rows (`null` is `None`; a `null` row stays `None`).
    pub profiles: Option<Vec<Option<LegacyRosterEntry>>>,
}

impl LegacyRosterDocument {
    /// The text the C# writes.
    #[must_use]
    pub fn write(&self) -> String {
        stj::write(&JsonValue::object([(
            "Profiles",
            self.profiles.as_ref().map_or(JsonValue::Null, |rows| {
                JsonValue::Array(
                    rows.iter()
                        .map(|row| {
                            row.as_ref().map_or(JsonValue::Null, |row| {
                                JsonValue::object([
                                    ("Name", stj::string_or_null(row.name.as_deref())),
                                    ("Owner", stj::string_or_null(row.owner.as_deref())),
                                ])
                            })
                        })
                        .collect(),
                )
            }),
        )]))
    }

    /// Reads one; `None` for JSON `null`.
    ///
    /// # Errors
    /// Not JSON, or a value of the wrong kind.
    pub fn read(text: &str) -> Result<Option<Self>, JsonError> {
        let document = json::parse(text).map_err(|error| error.to_string())?;
        let Some(o) = Object::of(&document, "$", "LegacyRosterDocument")? else {
            return Ok(None);
        };
        let profiles = match o.objects("Profiles", Some(Vec::new()), "LegacyRosterEntry")? {
            None => None,
            Some(rows) => Some(
                rows.iter()
                    .map(|row| {
                        row.map(|row| -> Result<_, JsonError> {
                            Ok(LegacyRosterEntry {
                                name: row.string("Name", "")?,
                                owner: row.string("Owner", "")?,
                            })
                        })
                        .transpose()
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        };
        Ok(Some(Self { profiles }))
    }
}

fn read_rules(o: &Object<'_>) -> Result<Option<Vec<Option<MonsterRuleDocument>>>, JsonError> {
    if o.get("Rules").is_none() {
        return Ok(Some(vec![Some(MonsterRuleDocument::from_rule(
            &MonsterRule::fresh_default(),
        ))]));
    }
    match o.objects("Rules", None, "MonsterRuleDocument")? {
        None => Ok(None),
        Some(items) => items
            .iter()
            .map(|item| item.as_ref().map(MonsterRuleDocument::read).transpose())
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
    }
}

fn read_dynamic(o: &Object<'_>) -> Result<Option<DynamicSettings>, JsonError> {
    crate::sidecar::read_dynamic_settings(o, "DynamicSettings")
}

/// The old combat part (`LegacyCombatProfileDocument`).
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct LegacyCombatProfileDocument {
    pub enabled: bool,
    pub maximum_range: f64,
    pub approach_distance: f64,
    pub idle_peace_mode: bool,
    pub selection_method: i32,
    pub target_select_angle_range: f64,
    pub target_lock: bool,
    pub attack_height: i32,
    pub attack_power: f32,
    pub auto_attack_power: bool,
    pub use_recklessness: bool,
    pub scan_interval_seconds: f64,
    pub debuff_each_first: i32,
    pub debuff_selection_method: i32,
    pub debuff_precast_seconds: f64,
    pub switch_wands_to_debuff: bool,
    pub use_arcs: i32,
    pub arc_range: f64,
    pub ring_distance: f64,
    pub minimum_ring_targets: i32,
    pub delete_ghost_monsters: bool,
    pub ghost_monster_spell_attempt_count: i32,
    pub blacklist_monster_attempt_count: i32,
    pub blacklist_monster_timeout_seconds: f64,
    pub delete_ghost_monsters_by_health_tracker: bool,
    pub ghost_delete_health_tracker_seconds: f64,
    pub summon_pets: bool,
    pub pet_range_mode: i32,
    pub pet_custom_range: f64,
    pub pet_monster_density: i32,
    pub pet_refill_count_idle: i32,
    pub pet_refill_count_normal: i32,
    pub meta_state: Option<String>,
    pub dynamic_settings: Option<DynamicSettings>,
    pub rules: Option<Vec<Option<MonsterRuleDocument>>>,
}

impl LegacyCombatProfileDocument {
    /// What the live settings would have saved.
    #[must_use]
    pub fn capture(value: &CombatSettings) -> Self {
        Self {
            enabled: value.enabled,
            maximum_range: value.maximum_range,
            approach_distance: value.approach_distance,
            idle_peace_mode: value.idle_peace_mode,
            selection_method: value.selection_method.0,
            target_select_angle_range: value.target_select_angle_range,
            target_lock: value.target_lock,
            attack_height: value.attack_height.0,
            attack_power: value.attack_power,
            auto_attack_power: value.auto_attack_power,
            use_recklessness: value.use_recklessness,
            scan_interval_seconds: value.scan_interval_seconds,
            debuff_each_first: value.debuff_each_first.0,
            debuff_selection_method: value.debuff_selection_method.0,
            debuff_precast_seconds: value.debuff_precast_seconds,
            switch_wands_to_debuff: value.switch_wands_to_debuff,
            use_arcs: value.use_arcs.0,
            arc_range: value.arc_range,
            ring_distance: value.ring_distance,
            minimum_ring_targets: value.minimum_ring_targets,
            delete_ghost_monsters: value.delete_ghost_monsters,
            ghost_monster_spell_attempt_count: value.ghost_monster_spell_attempt_count,
            blacklist_monster_attempt_count: value.blacklist_monster_attempt_count,
            blacklist_monster_timeout_seconds: value.blacklist_monster_timeout_seconds,
            delete_ghost_monsters_by_health_tracker: value.delete_ghost_monsters_by_health_tracker,
            ghost_delete_health_tracker_seconds: value.ghost_delete_health_tracker_seconds,
            summon_pets: value.summon_pets,
            pet_range_mode: value.pet_range_mode.0,
            pet_custom_range: value.pet_custom_range,
            pet_monster_density: value.pet_monster_density,
            pet_refill_count_idle: value.pet_refill_count_idle,
            pet_refill_count_normal: value.pet_refill_count_normal,
            meta_state: Some(value.meta_state.clone()),
            dynamic_settings: Some(
                value
                    .dynamic_settings
                    .iter()
                    .map(|(name, setting)| {
                        (
                            name.clone(),
                            Some(DynamicSettingDocument::from_value(setting)),
                        )
                    })
                    .collect(),
            ),
            rules: Some(
                value
                    .rules
                    .iter()
                    .map(|rule| Some(MonsterRuleDocument::from_rule(rule)))
                    .collect(),
            ),
        }
    }

    fn read(o: &Object<'_>) -> Result<Self, JsonError> {
        Ok(Self {
            enabled: o.bool("Enabled", true)?,
            maximum_range: o.double("MaximumRange", 5.0)?,
            approach_distance: o.double("ApproachDistance", 0.0)?,
            idle_peace_mode: o.bool("IdlePeaceMode", false)?,
            selection_method: o.int("SelectionMethod", TargetSelectionMethod::BOTH.0)?,
            target_select_angle_range: o.double("TargetSelectAngleRange", 5.0)?,
            target_lock: o.bool("TargetLock", false)?,
            attack_height: o.int("AttackHeight", MeleeAttackHeight::MEDIUM.0)?,
            attack_power: o.float("AttackPower", 0.5)?,
            auto_attack_power: o.bool("AutoAttackPower", true)?,
            use_recklessness: o.bool("UseRecklessness", true)?,
            scan_interval_seconds: o.double("ScanIntervalSeconds", 0.25)?,
            debuff_each_first: o.int("DebuffEachFirst", DebuffEachFirst::ONE.0)?,
            debuff_selection_method: o
                .int("DebuffSelectionMethod", DebuffSelectionMethod::SKILL.0)?,
            debuff_precast_seconds: o.double("DebuffPrecastSeconds", 5.0)?,
            switch_wands_to_debuff: o.bool("SwitchWandsToDebuff", false)?,
            use_arcs: o.int("UseArcs", UseArcsMode::NO.0)?,
            arc_range: o.double("ArcRange", 5.0)?,
            ring_distance: o.double("RingDistance", 5.0)?,
            minimum_ring_targets: o.int("MinimumRingTargets", 4)?,
            delete_ghost_monsters: o.bool("DeleteGhostMonsters", true)?,
            ghost_monster_spell_attempt_count: o.int("GhostMonsterSpellAttemptCount", 200)?,
            blacklist_monster_attempt_count: o.int("BlacklistMonsterAttemptCount", 4)?,
            blacklist_monster_timeout_seconds: o.double("BlacklistMonsterTimeoutSeconds", 120.0)?,
            delete_ghost_monsters_by_health_tracker: o
                .bool("DeleteGhostMonstersByHealthTracker", true)?,
            ghost_delete_health_tracker_seconds: o
                .double("GhostDeleteHealthTrackerSeconds", 30.0)?,
            summon_pets: o.bool("SummonPets", true)?,
            pet_range_mode: o.int("PetRangeMode", PetRangeMode::ATTACK_DISTANCE.0)?,
            pet_custom_range: o.double("PetCustomRange", 5.0)?,
            pet_monster_density: o.int("PetMonsterDensity", 1)?,
            pet_refill_count_idle: o.int("PetRefillCountIdle", 3)?,
            pet_refill_count_normal: o.int("PetRefillCountNormal", 1)?,
            meta_state: o.string("MetaState", "Default")?,
            dynamic_settings: read_dynamic(o)?,
            rules: read_rules(o)?,
        })
    }

    /// Puts it into the live settings, held in range; a rule that does not
    /// compile is logged and left out, and a default rule is always there.
    pub fn apply(&self, value: &mut CombatSettings, warn: &mut dyn FnMut(&str)) {
        value.enabled = self.enabled;
        value.maximum_range = self.maximum_range.clamp(2.0, 100.0);
        value.approach_distance = self.approach_distance.clamp(0.0, 100.0);
        value.idle_peace_mode = self.idle_peace_mode;
        value.selection_method = TargetSelectionMethod(self.selection_method);
        value.target_select_angle_range = self
            .target_select_angle_range
            .clamp(2.0, value.maximum_range);
        value.target_lock = self.target_lock;
        value.attack_height = MeleeAttackHeight(self.attack_height);
        value.attack_power = self.attack_power.clamp(0.0, 1.0);
        value.auto_attack_power = self.auto_attack_power;
        value.use_recklessness = self.use_recklessness;
        value.scan_interval_seconds = self.scan_interval_seconds.clamp(0.05, 5.0);
        value.debuff_each_first = DebuffEachFirst(self.debuff_each_first);
        value.debuff_selection_method = DebuffSelectionMethod(self.debuff_selection_method);
        value.debuff_precast_seconds = self.debuff_precast_seconds.clamp(0.0, 60.0);
        value.switch_wands_to_debuff = self.switch_wands_to_debuff;
        value.use_arcs = UseArcsMode(self.use_arcs);
        value.arc_range = self.arc_range.clamp(1.0, 100.0);
        value.ring_distance = self.ring_distance.clamp(1.0, 100.0);
        value.minimum_ring_targets = self.minimum_ring_targets.clamp(1, 25);
        value.delete_ghost_monsters = self.delete_ghost_monsters;
        value.ghost_monster_spell_attempt_count =
            self.ghost_monster_spell_attempt_count.clamp(1, 1000);
        value.blacklist_monster_attempt_count = self.blacklist_monster_attempt_count.clamp(1, 20);
        value.blacklist_monster_timeout_seconds =
            self.blacklist_monster_timeout_seconds.clamp(1.0, 3600.0);
        value.delete_ghost_monsters_by_health_tracker =
            self.delete_ghost_monsters_by_health_tracker;
        value.ghost_delete_health_tracker_seconds =
            self.ghost_delete_health_tracker_seconds.clamp(1.0, 300.0);
        value.summon_pets = self.summon_pets;
        value.pet_range_mode = PetRangeMode(self.pet_range_mode);
        value.pet_custom_range = self.pet_custom_range.clamp(1.0, 100.0);
        value.pet_monster_density = self.pet_monster_density.clamp(1, 25);
        value.pet_refill_count_idle = self.pet_refill_count_idle.clamp(0, 3);
        value.pet_refill_count_normal = self.pet_refill_count_normal.clamp(0, 3);
        value.meta_state = match self.meta_state.as_deref() {
            Some(state) if !casing::is_white_space_only(state) => state.to_owned(),
            _ => "Default".to_owned(),
        };
        value.dynamic_settings.clear();
        for (name, setting) in self.dynamic_settings.iter().flatten() {
            if let Some(setting) = setting {
                value
                    .dynamic_settings
                    .insert(name.clone(), setting.to_value());
            }
        }
        value.rules.clear();
        for rule in self.rules.iter().flatten().flatten() {
            match rule.to_rule() {
                Ok(rule) => value.rules.push(rule),
                Err(error) => warn(&format!(
                    "MossTank could not restore a legacy monster rule: {error}"
                )),
            }
        }
        if !value.rules.iter().any(MonsterRule::is_default) {
            value.rules.push(MonsterRule::fresh_default());
        }
    }
}

/// The old buff part (`LegacyBuffProfileDocument`).
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct LegacyBuffProfileDocument {
    pub enabled: bool,
    pub idle_buff_topoff: bool,
    pub idle_buff_topoff_seconds: f64,
    pub rebuff_when_under_seconds: f64,
    pub skill_excess_over_difficulty: i32,
    pub buff_attributes: bool,
    pub buff_protections: bool,
    pub buff_auras: bool,
    pub buff_banes: bool,
    pub buff_regeneration: bool,
    pub buff_other: bool,
    pub buff_trained_skills_only: bool,
    pub buff_cast_recast_seconds: f64,
    pub buff_cast_recast_reset_seconds: f64,
    pub fast_cast_buffs: bool,
    pub random_helper_buffs: bool,
    pub random_helper_interval_seconds: f64,
    pub blacklisted_spell_components: Option<String>,
    pub protection_elements: Option<String>,
    pub protection_profile_mode: i32,
    pub bane_elements: Option<String>,
    pub bane_profile_mode: i32,
}

impl LegacyBuffProfileDocument {
    /// What the live settings would have saved.
    #[must_use]
    pub fn capture(value: &BuffSettings) -> Self {
        Self {
            enabled: value.enabled,
            idle_buff_topoff: value.idle_buff_topoff,
            idle_buff_topoff_seconds: value.idle_buff_topoff_seconds,
            rebuff_when_under_seconds: value.rebuff_when_under_seconds,
            skill_excess_over_difficulty: value.skill_excess_over_difficulty,
            buff_attributes: value.buff_attributes,
            buff_protections: value.buff_protections,
            buff_auras: value.buff_auras,
            buff_banes: value.buff_banes,
            buff_regeneration: value.buff_regeneration,
            buff_other: value.buff_other,
            buff_trained_skills_only: value.buff_trained_skills_only,
            buff_cast_recast_seconds: value.buff_cast_recast_seconds,
            buff_cast_recast_reset_seconds: value.buff_cast_recast_reset_seconds,
            fast_cast_buffs: value.fast_cast_buffs,
            random_helper_buffs: value.random_helper_buffs,
            random_helper_interval_seconds: value.random_helper_interval_seconds,
            blacklisted_spell_components: Some(value.blacklisted_spell_components.clone()),
            protection_elements: Some(value.protection_elements.clone()),
            protection_profile_mode: value.protection_profile_mode,
            bane_elements: Some(value.bane_elements.clone()),
            bane_profile_mode: value.bane_profile_mode,
        }
    }

    fn read(o: &Object<'_>) -> Result<Self, JsonError> {
        Ok(Self {
            enabled: o.bool("Enabled", true)?,
            idle_buff_topoff: o.bool("IdleBuffTopoff", false)?,
            idle_buff_topoff_seconds: o.double("IdleBuffTopoffSeconds", 1200.0)?,
            rebuff_when_under_seconds: o.double("RebuffWhenUnderSeconds", 300.0)?,
            skill_excess_over_difficulty: o.int("SkillExcessOverDifficulty", 5)?,
            buff_attributes: o.bool("BuffAttributes", true)?,
            buff_protections: o.bool("BuffProtections", true)?,
            buff_auras: o.bool("BuffAuras", true)?,
            buff_banes: o.bool("BuffBanes", true)?,
            buff_regeneration: o.bool("BuffRegeneration", true)?,
            buff_other: o.bool("BuffOther", false)?,
            buff_trained_skills_only: o.bool("BuffTrainedSkillsOnly", true)?,
            buff_cast_recast_seconds: o.double("BuffCastRecastSeconds", 30.0)?,
            buff_cast_recast_reset_seconds: o.double("BuffCastRecastResetSeconds", 30.0)?,
            fast_cast_buffs: o.bool("FastCastBuffs", false)?,
            random_helper_buffs: o.bool("RandomHelperBuffs", false)?,
            random_helper_interval_seconds: o.double("RandomHelperIntervalSeconds", 5.0)?,
            blacklisted_spell_components: o.string("BlacklistedSpellComponents", "")?,
            protection_elements: o.string("ProtectionElements", "ALFCBPS")?,
            protection_profile_mode: o.int("ProtectionProfileMode", 2)?,
            bane_elements: o.string("BaneElements", "ALFCBPS")?,
            bane_profile_mode: o.int("BaneProfileMode", 2)?,
        })
    }

    /// Puts it into the live settings, held in range.
    pub fn apply(&self, value: &mut BuffSettings) {
        value.enabled = self.enabled;
        value.idle_buff_topoff = self.idle_buff_topoff;
        value.idle_buff_topoff_seconds = self.idle_buff_topoff_seconds.clamp(30.0, 7200.0);
        value.rebuff_when_under_seconds = self.rebuff_when_under_seconds.clamp(30.0, 1800.0);
        value.skill_excess_over_difficulty = self.skill_excess_over_difficulty.clamp(-100, 100);
        value.buff_attributes = self.buff_attributes;
        value.buff_protections = self.buff_protections;
        value.buff_auras = self.buff_auras;
        value.buff_banes = self.buff_banes;
        value.buff_regeneration = self.buff_regeneration;
        value.buff_other = self.buff_other;
        value.buff_trained_skills_only = self.buff_trained_skills_only;
        value.buff_cast_recast_seconds = self.buff_cast_recast_seconds.clamp(0.0, 3600.0);
        value.buff_cast_recast_reset_seconds =
            self.buff_cast_recast_reset_seconds.clamp(0.0, 3600.0);
        value.fast_cast_buffs = self.fast_cast_buffs;
        value.random_helper_buffs = self.random_helper_buffs;
        value.random_helper_interval_seconds =
            self.random_helper_interval_seconds.clamp(0.25, 3600.0);
        value.blacklisted_spell_components = self
            .blacklisted_spell_components
            .clone()
            .unwrap_or_default();
        value.protection_elements = self
            .protection_elements
            .clone()
            .unwrap_or_else(|| "ALFCBPS".to_owned());
        value.protection_profile_mode = self.protection_profile_mode.clamp(1, 8);
        value.bane_elements = self
            .bane_elements
            .clone()
            .unwrap_or_else(|| "ALFCBPS".to_owned());
        value.bane_profile_mode = self.bane_profile_mode.clamp(1, 8);
    }
}

/// The old vitals part (`LegacyVitalProfileDocument`).
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct LegacyVitalProfileDocument {
    pub enabled: bool,
    pub normal_health: f64,
    pub normal_stamina: f64,
    pub normal_mana: f64,
    pub no_target_health: f64,
    pub no_target_stamina: f64,
    pub no_target_mana: f64,
    pub helper_health: f64,
    pub helper_stamina: f64,
    pub helper_mana: f64,
    pub help_others: bool,
}

impl LegacyVitalProfileDocument {
    /// What the live settings would have saved.
    #[must_use]
    pub fn capture(value: &VitalSettings) -> Self {
        Self {
            enabled: value.enabled,
            normal_health: value.normal_health,
            normal_stamina: value.normal_stamina,
            normal_mana: value.normal_mana,
            no_target_health: value.no_target_health,
            no_target_stamina: value.no_target_stamina,
            no_target_mana: value.no_target_mana,
            helper_health: value.helper_health,
            helper_stamina: value.helper_stamina,
            helper_mana: value.helper_mana,
            help_others: value.help_others,
        }
    }

    fn read(o: &Object<'_>) -> Result<Self, JsonError> {
        Ok(Self {
            enabled: o.bool("Enabled", true)?,
            normal_health: o.double("NormalHealth", 0.75)?,
            normal_stamina: o.double("NormalStamina", 0.50)?,
            normal_mana: o.double("NormalMana", 0.50)?,
            no_target_health: o.double("NoTargetHealth", 0.01)?,
            no_target_stamina: o.double("NoTargetStamina", 0.01)?,
            no_target_mana: o.double("NoTargetMana", 0.01)?,
            helper_health: o.double("HelperHealth", 0.20)?,
            helper_stamina: o.double("HelperStamina", 0.01)?,
            helper_mana: o.double("HelperMana", 0.01)?,
            help_others: o.bool("HelpOthers", true)?,
        })
    }

    /// Puts it into the live settings, each fraction held in 0..=1.
    pub fn apply(&self, value: &mut VitalSettings) {
        let clamp = |fraction: f64| fraction.clamp(0.0, 1.0);
        value.enabled = self.enabled;
        value.normal_health = clamp(self.normal_health);
        value.normal_stamina = clamp(self.normal_stamina);
        value.normal_mana = clamp(self.normal_mana);
        value.no_target_health = clamp(self.no_target_health);
        value.no_target_stamina = clamp(self.no_target_stamina);
        value.no_target_mana = clamp(self.no_target_mana);
        value.helper_health = clamp(self.helper_health);
        value.helper_stamina = clamp(self.helper_stamina);
        value.helper_mana = clamp(self.helper_mana);
        value.help_others = self.help_others;
    }
}

/// The old inventory part (`LegacyInventoryProfileDocument`).
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct LegacyInventoryProfileDocument {
    pub mana_charges_when_off: bool,
    pub auto_stack: bool,
    pub auto_cram: bool,
    pub auto_craft_items: bool,
    pub split_peas: bool,
    pub critical_component_minimum: i32,
    pub normal_component_minimum: i32,
    pub idle_component_minimum: i32,
    pub idle_health_kit_count: i32,
    pub idle_stamina_kit_count: i32,
    pub idle_mana_kit_count: i32,
    pub idle_health_food_count: i32,
    pub idle_stamina_food_count: i32,
    pub idle_mana_food_count: i32,
    pub refill_worn_mana: bool,
    pub refill_worn_mana_percent: i32,
    pub scan_interval_seconds: f64,
    pub enable_looting: bool,
    pub loot_classifier_id: Option<String>,
    pub loot_priority_boost: bool,
    pub loot_all_corpses: bool,
    pub loot_fellow_corpses: bool,
    pub loot_only_rare_corpses: bool,
    pub read_unknown_scrolls: bool,
    pub combine_salvage: bool,
    pub mana_stone_loot_count: i32,
    pub mana_tank_minimum_mana: i32,
    pub corpse_approach_range: f64,
    pub corpse_open_timeout_seconds: f64,
    pub blacklist_corpse_open_attempt_count: i32,
    pub blacklist_corpse_open_timeout_seconds: f64,
    pub corpse_cache_timeout_minutes: f64,
    pub corpse_loot_item_max_attempts: i32,
    pub loot_scan_interval_seconds: f64,
    pub loot_rules: Option<Vec<Option<LootRuleDocument>>>,
}

impl LegacyInventoryProfileDocument {
    /// What the live settings would have saved.
    #[must_use]
    pub fn capture(value: &InventorySettings) -> Self {
        let loot = &value.loot;
        Self {
            mana_charges_when_off: value.mana_charges_when_off,
            auto_stack: value.auto_stack,
            auto_cram: value.auto_cram,
            auto_craft_items: value.auto_craft_items,
            split_peas: value.split_peas,
            critical_component_minimum: value.critical_component_minimum,
            normal_component_minimum: value.normal_component_minimum,
            idle_component_minimum: value.idle_component_minimum,
            idle_health_kit_count: value.idle_health_kit_count,
            idle_stamina_kit_count: value.idle_stamina_kit_count,
            idle_mana_kit_count: value.idle_mana_kit_count,
            idle_health_food_count: value.idle_health_food_count,
            idle_stamina_food_count: value.idle_stamina_food_count,
            idle_mana_food_count: value.idle_mana_food_count,
            refill_worn_mana: value.refill_worn_mana,
            refill_worn_mana_percent: value.refill_worn_mana_percent,
            scan_interval_seconds: value.scan_interval_seconds,
            enable_looting: loot.enabled,
            loot_classifier_id: Some(loot.external_classifier_id.clone()),
            loot_priority_boost: loot.priority_boost,
            loot_all_corpses: loot.loot_all_corpses,
            loot_fellow_corpses: loot.loot_fellow_corpses,
            loot_only_rare_corpses: loot.loot_only_rare_corpses,
            read_unknown_scrolls: loot.read_unknown_scrolls,
            combine_salvage: loot.combine_salvage,
            mana_stone_loot_count: loot.mana_stone_loot_count,
            mana_tank_minimum_mana: loot.mana_tank_minimum_mana,
            corpse_approach_range: loot.corpse_approach_range,
            corpse_open_timeout_seconds: loot.corpse_open_timeout_seconds,
            blacklist_corpse_open_attempt_count: loot.blacklist_corpse_open_attempt_count,
            blacklist_corpse_open_timeout_seconds: loot.blacklist_corpse_open_timeout_seconds,
            corpse_cache_timeout_minutes: loot.corpse_cache_timeout_minutes,
            corpse_loot_item_max_attempts: loot.corpse_loot_item_max_attempts,
            loot_scan_interval_seconds: loot.scan_interval_seconds,
            loot_rules: Some(
                loot.rules
                    .iter()
                    .map(|rule| Some(LootRuleDocument::from_rule(rule)))
                    .collect(),
            ),
        }
    }

    fn read(o: &Object<'_>) -> Result<Self, JsonError> {
        let loot_rules = if o.get("LootRules").is_none() {
            Some(Vec::new())
        } else {
            match o.objects("LootRules", None, "LootRuleDocument")? {
                None => None,
                Some(items) => Some(
                    items
                        .iter()
                        .map(|item| item.as_ref().map(LootRuleDocument::read).transpose())
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            }
        };
        Ok(Self {
            mana_charges_when_off: o.bool("ManaChargesWhenOff", true)?,
            auto_stack: o.bool("AutoStack", true)?,
            auto_cram: o.bool("AutoCram", false)?,
            auto_craft_items: o.bool("AutoCraftItems", true)?,
            split_peas: o.bool("SplitPeas", true)?,
            critical_component_minimum: o.int("CriticalComponentMinimum", 4)?,
            normal_component_minimum: o.int("NormalComponentMinimum", 20)?,
            idle_component_minimum: o.int("IdleComponentMinimum", 20)?,
            idle_health_kit_count: o.int("IdleHealthKitCount", 2)?,
            idle_stamina_kit_count: o.int("IdleStaminaKitCount", 2)?,
            idle_mana_kit_count: o.int("IdleManaKitCount", 2)?,
            idle_health_food_count: o.int("IdleHealthFoodCount", 15)?,
            idle_stamina_food_count: o.int("IdleStaminaFoodCount", 15)?,
            idle_mana_food_count: o.int("IdleManaFoodCount", 15)?,
            refill_worn_mana: o.bool("RefillWornMana", true)?,
            refill_worn_mana_percent: o.int("RefillWornManaPercent", 33)?,
            scan_interval_seconds: o.double("ScanIntervalSeconds", 0.25)?,
            enable_looting: o.bool("EnableLooting", false)?,
            loot_classifier_id: o.string("LootClassifierId", "")?,
            loot_priority_boost: o.bool("LootPriorityBoost", false)?,
            loot_all_corpses: o.bool("LootAllCorpses", false)?,
            loot_fellow_corpses: o.bool("LootFellowCorpses", false)?,
            loot_only_rare_corpses: o.bool("LootOnlyRareCorpses", false)?,
            read_unknown_scrolls: o.bool("ReadUnknownScrolls", true)?,
            combine_salvage: o.bool("CombineSalvage", true)?,
            mana_stone_loot_count: o.int("ManaStoneLootCount", 4)?,
            mana_tank_minimum_mana: o.int("ManaTankMinimumMana", 1000)?,
            corpse_approach_range: o.double("CorpseApproachRange", 40.0)?,
            corpse_open_timeout_seconds: o.double("CorpseOpenTimeoutSeconds", 1.5)?,
            blacklist_corpse_open_attempt_count: o.int("BlacklistCorpseOpenAttemptCount", 30)?,
            blacklist_corpse_open_timeout_seconds: o
                .double("BlacklistCorpseOpenTimeoutSeconds", 200.0)?,
            corpse_cache_timeout_minutes: o.double("CorpseCacheTimeoutMinutes", 60.0)?,
            corpse_loot_item_max_attempts: o.int("CorpseLootItemMaxAttempts", 20)?,
            loot_scan_interval_seconds: o.double("LootScanIntervalSeconds", 0.25)?,
            loot_rules,
        })
    }

    /// Puts it into the live settings, held in range.
    pub fn apply(&self, value: &mut InventorySettings) {
        value.mana_charges_when_off = self.mana_charges_when_off;
        value.auto_stack = self.auto_stack;
        value.auto_cram = self.auto_cram;
        value.auto_craft_items = self.auto_craft_items;
        value.split_peas = self.split_peas;
        value.critical_component_minimum = self.critical_component_minimum.clamp(0, 1000);
        value.normal_component_minimum = self.normal_component_minimum.clamp(0, 1000);
        value.idle_component_minimum = self.idle_component_minimum.clamp(0, 1000);
        value.idle_health_kit_count = self.idle_health_kit_count.clamp(0, 1000);
        value.idle_stamina_kit_count = self.idle_stamina_kit_count.clamp(0, 1000);
        value.idle_mana_kit_count = self.idle_mana_kit_count.clamp(0, 1000);
        value.idle_health_food_count = self.idle_health_food_count.clamp(0, 1000);
        value.idle_stamina_food_count = self.idle_stamina_food_count.clamp(0, 1000);
        value.idle_mana_food_count = self.idle_mana_food_count.clamp(0, 1000);
        value.refill_worn_mana = self.refill_worn_mana;
        value.refill_worn_mana_percent = self.refill_worn_mana_percent.clamp(0, 99);
        value.scan_interval_seconds = self.scan_interval_seconds.clamp(0.05, 10.0);
        let loot = &mut value.loot;
        loot.enabled = self.enable_looting;
        loot.external_classifier_id = self
            .loot_classifier_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_owned();
        loot.priority_boost = self.loot_priority_boost;
        loot.loot_all_corpses = self.loot_all_corpses;
        loot.loot_fellow_corpses = self.loot_fellow_corpses;
        loot.loot_only_rare_corpses = self.loot_only_rare_corpses;
        loot.read_unknown_scrolls = self.read_unknown_scrolls;
        loot.combine_salvage = self.combine_salvage;
        loot.mana_stone_loot_count = self.mana_stone_loot_count.clamp(0, 100);
        loot.mana_tank_minimum_mana = self.mana_tank_minimum_mana.clamp(1, i32::MAX);
        loot.corpse_approach_range = self.corpse_approach_range.clamp(0.0, 100.0);
        loot.corpse_open_timeout_seconds = self.corpse_open_timeout_seconds.clamp(0.25, 30.0);
        loot.blacklist_corpse_open_attempt_count =
            self.blacklist_corpse_open_attempt_count.clamp(1, 1000);
        loot.blacklist_corpse_open_timeout_seconds = self
            .blacklist_corpse_open_timeout_seconds
            .clamp(1.0, 3600.0);
        loot.corpse_cache_timeout_minutes = self.corpse_cache_timeout_minutes.clamp(1.0, 1440.0);
        loot.corpse_loot_item_max_attempts = self.corpse_loot_item_max_attempts.clamp(1, 100);
        loot.scan_interval_seconds = self.loot_scan_interval_seconds.clamp(0.05, 5.0);
        loot.rules.clear();
        for rule in self.loot_rules.iter().flatten().flatten() {
            loot.rules.push(rule.to_rule());
        }
    }
}

/// A whole old profile (`LegacyProfileDocument`).
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct LegacyProfileDocument {
    pub version: i32,
    pub item_names: Option<Vec<Option<String>>>,
    pub consumable_names: Option<Vec<Option<String>>>,
    pub consumable_categories: Option<Vec<(String, ConsumableCategory)>>,
    pub no_buff_item_names: Option<Vec<Option<String>>>,
    pub combat: Option<LegacyCombatProfileDocument>,
    pub buffs: Option<LegacyBuffProfileDocument>,
    pub vitals: Option<LegacyVitalProfileDocument>,
    pub inventory: Option<LegacyInventoryProfileDocument>,
}

fn sub<T>(
    o: &Object<'_>,
    name: &str,
    read: impl Fn(&Object<'_>) -> Result<T, JsonError>,
) -> Result<Option<T>, JsonError> {
    match o.get(name) {
        None => Ok(None),
        Some(value) => match Object::of(value, name, name)? {
            None => Ok(None),
            Some(inner) => read(&inner).map(Some),
        },
    }
}

impl LegacyProfileDocument {
    /// Reads one; `None` for JSON `null`.
    ///
    /// # Errors
    /// Not JSON, or a value of the wrong kind.
    pub fn read(text: &str) -> Result<Option<Self>, JsonError> {
        let document = json::parse(text).map_err(|error| error.to_string())?;
        let Some(o) = Object::of(&document, "$", "LegacyProfileDocument")? else {
            return Ok(None);
        };
        let consumable_categories =
            match o.map("ConsumableCategories", "Dictionary`2[String,ConsumableCategory]")? {
                None => None,
                Some(entries) => Some(
                    entries
                        .into_iter()
                        .map(|(name, value)| {
                            let number = stj::int_value(value, "ConsumableCategories")?;
                            crate::sidecar::consumable_from(number)
                                .map(|category| (name.to_owned(), category))
                                .ok_or_else(|| {
                                    format!("The JSON value could not be converted to an enum: {number} at ConsumableCategories")
                                })
                        })
                        .collect::<Result<Vec<_>, JsonError>>()?,
                ),
            };
        Ok(Some(Self {
            version: o.int("Version", 6)?,
            item_names: o.strings("ItemNames")?,
            consumable_names: o.strings("ConsumableNames")?,
            consumable_categories,
            no_buff_item_names: o.strings("NoBuffItemNames")?,
            combat: sub(&o, "Combat", LegacyCombatProfileDocument::read)?,
            buffs: sub(&o, "Buffs", LegacyBuffProfileDocument::read)?,
            vitals: sub(&o, "Vitals", LegacyVitalProfileDocument::read)?,
            inventory: sub(&o, "Inventory", LegacyInventoryProfileDocument::read)?,
        }))
    }

    /// Puts the old profile into the live settings; a part it lacks puts
    /// that group's defaults in.
    pub fn apply(
        &self,
        combat: &mut CombatSettings,
        buffs: &mut BuffSettings,
        vitals: &mut VitalSettings,
        inventory: &mut InventorySettings,
        no_buff_item_names: &mut OrdinalStringSet,
        warn: &mut dyn FnMut(&str),
    ) {
        self.combat
            .clone()
            .unwrap_or_else(|| LegacyCombatProfileDocument::capture(&CombatSettings::default()))
            .apply(combat, warn);
        self.buffs
            .clone()
            .unwrap_or_else(|| LegacyBuffProfileDocument::capture(&BuffSettings::default()))
            .apply(buffs);
        self.vitals
            .clone()
            .unwrap_or_else(|| LegacyVitalProfileDocument::capture(&VitalSettings::default()))
            .apply(vitals);
        self.inventory
            .clone()
            .unwrap_or_else(|| {
                LegacyInventoryProfileDocument::capture(&InventorySettings::default())
            })
            .apply(inventory);
        crate::sidecar::replace_names(&mut combat.combat_item_names, self.item_names.as_deref());
        crate::sidecar::replace_order_names(
            &mut combat.combat_item_order,
            self.item_names.as_deref(),
        );
        combat.combat_item_object_ids.clear();
        combat.combat_item_order_ids.clear();
        combat.removed_combat_item_object_ids.clear();
        crate::sidecar::replace_names(
            &mut combat.consumable_names,
            self.consumable_names.as_deref(),
        );
        combat.consumable_categories.clear();
        for (item_name, category) in self.consumable_categories.iter().flatten() {
            if combat.consumable_names.contains(item_name.as_str()) {
                combat
                    .consumable_categories
                    .insert(item_name.clone(), *category);
            }
        }
        crate::sidecar::replace_names(no_buff_item_names, self.no_buff_item_names.as_deref());
    }
}
