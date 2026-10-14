//! The macro's options (`GetMetaOption`, `SetMetaOption`): the one place
//! an option is read and written, by `/vt opt`, the meta's `GetOpt` and
//! `SetOpt`, a death and the autostart alike. Distances are kept in metres
//! and spoken in VTank's map units (240 metres to the unit).

use mosstank_dotnet::text::casing;
use mosstank_expr::{ExpressionValue, Fault};
use mosstank_settings::model::{
    DebuffEachFirst, DebuffSelectionMethod, MeleeAttackHeight, PetRangeMode, TargetSelectionMethod,
    UseArcsMode,
};
use mosstank_settings::monster::expression::{MonsterValue, MonsterValueKind};
use mosstank_settings::options::{self, VtankSettingValueType};
use openac_plugin_compat::number::{self, NumberStyles};

use super::{Core, TankCx};

/// Metres per VTank map unit.
const METERS_PER_UNIT: f64 = 240.0;

/// An option's value as an expression value (`ToExpressionValue`).
pub(crate) fn to_expression_value(value: &MonsterValue) -> ExpressionValue {
    match value.kind {
        MonsterValueKind::Number => ExpressionValue::Number(value.number),
        MonsterValueKind::Boolean => ExpressionValue::Boolean(value.boolean),
        MonsterValueKind::Text => ExpressionValue::string(&value.text),
    }
}

/// An expression value as an option's value (`ToMonsterValue`).
pub(crate) fn to_monster_value(value: &ExpressionValue) -> MonsterValue {
    match value {
        ExpressionValue::Boolean(_) => MonsterValue::from_boolean(value.is_truthy()),
        ExpressionValue::Number(number) => MonsterValue::from_number(*number),
        _ => MonsterValue::from_text(&value.to_display_string()),
    }
}

/// `NormalizeElementProfile`: the element letters present, in VTank's
/// order.
fn normalize_element_profile(value: &str) -> String {
    const ORDER: &str = "ALFCBPS";
    let upper = casing::to_upper_invariant(value);
    ORDER
        .chars()
        .filter(|element| upper.contains(*element))
        .collect()
}

/// A distance in map units, clamped as a .NET `float` (`checked((float)(v *
/// 240d))` then `Math.Clamp` on floats).
fn single_meters(
    value: &ExpressionValue,
    name: &str,
    minimum: f32,
    maximum: f32,
) -> Result<f64, Fault> {
    #[allow(clippy::cast_possible_truncation)]
    let meters = (value.as_number(name)? * METERS_PER_UNIT) as f32;
    Ok(f64::from(meters.clamp(minimum, maximum)))
}

/// A plain .NET `float`, clamped.
fn single(value: &ExpressionValue, name: &str, minimum: f32, maximum: f32) -> Result<f64, Fault> {
    #[allow(clippy::cast_possible_truncation)]
    let number = value.as_number(name)? as f32;
    Ok(f64::from(number.clamp(minimum, maximum)))
}

fn int(value: &ExpressionValue, name: &str, minimum: i32, maximum: i32) -> Result<i32, Fault> {
    Ok(value.as_int32(name)?.clamp(minimum, maximum))
}

fn double(value: &ExpressionValue, name: &str, minimum: f64, maximum: f64) -> Result<f64, Fault> {
    Ok(value.as_number(name)?.clamp(minimum, maximum))
}

pub(crate) fn number(value: impl Into<f64>) -> ExpressionValue {
    ExpressionValue::Number(value.into())
}

pub(crate) fn boolean(value: bool) -> ExpressionValue {
    ExpressionValue::Boolean(value)
}

impl Core {
    /// An option's value (`GetMetaOption`).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn get_meta_option(&self, meta_enabled: bool, name: &str) -> ExpressionValue {
        let key = name.trim();
        let combat = &self.settings.combat;
        let buffs = &self.settings.buffs;
        let vitals = &self.settings.vitals;
        let inventory = &self.settings.inventory;
        let loot = &inventory.loot;
        let navigation = &self.settings.navigation;
        match casing::to_lower_invariant(key).as_str() {
            "enablebuffing" => boolean(buffs.enabled),
            "enablecombat" => boolean(combat.enabled),
            "enablenav" => boolean(navigation.enabled),
            "enablelooting" => boolean(loot.enabled),
            "walktorarecorpse" | "walktoownrarecorpses" | "walktoowncorpses" => {
                boolean(loot.walk_to_own_rare_corpses)
            }
            "shownavlines" => boolean(navigation.show_nav_lines),
            "enablemeta" => boolean(meta_enabled),
            "spelldiffexcessthreshold-hunt" => number(combat.hunt_skill_excess_over_difficulty),
            "spelldiffexcessthreshold-buff" => number(buffs.skill_excess_over_difficulty),
            "arrowheadfletchdiffexcessthreshold" => {
                number(inventory.arrowhead_fletch_difficulty_excess)
            }
            "dohelp" => boolean(vitals.help_others),
            "attackdistance" => number(combat.maximum_range / METERS_PER_UNIT),
            "attackminimumdistance" => number(combat.minimum_range / METERS_PER_UNIT),
            "approachdistance" => number(combat.approach_distance / METERS_PER_UNIT),
            "ringdistance" => number(combat.ring_distance / METERS_PER_UNIT),
            "arcrange" => number(combat.arc_range / METERS_PER_UNIT),
            "targetselectanglerange" => number(combat.target_select_angle_range / METERS_PER_UNIT),
            "corpseapproachrange-max" => number(loot.corpse_approach_range / METERS_PER_UNIT),
            "corpseapproachrange-min" => {
                number(loot.corpse_minimum_approach_range / METERS_PER_UNIT)
            }
            "navclosestoprange" => number(navigation.minimum_distance_meters / METERS_PER_UNIT),
            "navfarstoprange" => number(navigation.maximum_distance_meters / METERS_PER_UNIT),
            "useportaldistance" => number(navigation.portal_use_distance_meters / METERS_PER_UNIT),
            "helperdistancehitp" => number(vitals.helper_health_distance / METERS_PER_UNIT),
            "helperdistancestam" => number(vitals.helper_stamina_distance / METERS_PER_UNIT),
            "helperdistancemana" => number(vitals.helper_mana_distance / METERS_PER_UNIT),
            "minimumringtargets" => number(combat.minimum_ring_targets),
            "defaultmeleeattackheight" => number(combat.attack_height.0),
            "targetlock" => boolean(combat.target_lock),
            "idlepeacemode" => boolean(combat.idle_peace_mode),
            "stopmacroondeath" => boolean(combat.stop_macro_on_death),
            "jumpoutwandcasting" => boolean(combat.jump_out_wand_casting),
            "dojiggle" => boolean(combat.do_jiggle),
            "randomhelperbuffs" => boolean(buffs.random_helper_buffs),
            "randomhelperintervalseconds" => number(buffs.random_helper_interval_seconds),
            "idlebufftopoff" => boolean(buffs.idle_buff_topoff),
            "idlebufftopofftimeseconds" => number(buffs.idle_buff_topoff_seconds),
            "buffprofile-prots" => ExpressionValue::string(&buffs.protection_elements),
            "buffprofile-banes" => ExpressionValue::string(&buffs.bane_elements),
            "buffprofile_prots" => number(buffs.protection_profile_mode),
            "buffprofile_banes" => number(buffs.bane_profile_mode),
            "targetselectmethod" => number(combat.selection_method.0 + 1),
            "autoattackpower" => boolean(combat.auto_attack_power),
            "userecklessness" => boolean(combat.use_recklessness),
            "debuffeachfirst" => number(combat.debuff_each_first.0),
            "debuffselectionmethod" => number(combat.debuff_selection_method.0),
            "debuffprecastseconds" => number(combat.debuff_precast_seconds),
            "switchwandstodebuff" => boolean(combat.switch_wands_to_debuff),
            "usearcs" => number(combat.use_arcs.0),
            "deleteghostmonsters" => boolean(combat.delete_ghost_monsters),
            "ghostmonsterspellattemptcount" => number(combat.ghost_monster_spell_attempt_count),
            "blacklistmonsterattemptcount" => number(combat.blacklist_monster_attempt_count),
            "blacklistmonstertimeoutseconds" => number(combat.blacklist_monster_timeout_seconds),
            "deleteghostmonstersbyhptracker" => {
                boolean(combat.delete_ghost_monsters_by_health_tracker)
            }
            "ghostdeletehptrackerseconds" => number(combat.ghost_delete_health_tracker_seconds),
            "summonpets" => boolean(combat.summon_pets),
            "petrangemode" => number(combat.pet_range_mode.0),
            "petcustomrange" => number(combat.pet_custom_range / METERS_PER_UNIT),
            "petmonsterdensity" => number(combat.pet_monster_density),
            "petrefillcount-idle" => number(combat.pet_refill_count_idle),
            "petrefillcount-normal" => number(combat.pet_refill_count_normal),
            "opendoors" => boolean(navigation.open_doors),
            "dooridrange" => number(navigation.door_identify_range_meters / METERS_PER_UNIT),
            "dooropenrange" => number(navigation.door_open_range_meters / METERS_PER_UNIT),
            "doorlockpickdiffexcessthreshold" => number(navigation.door_lockpick_excess_threshold),
            "navpriorityboost" => boolean(navigation.priority),
            "followaroundcorners" => boolean(navigation.follow_around_corners),
            "autofellowmanagement" => boolean(combat.auto_fellow_management),
            "autostack" => boolean(inventory.auto_stack),
            "autocram" => boolean(inventory.auto_cram),
            "autocraftitems" => boolean(inventory.auto_craft_items),
            "splitpeas" => boolean(inventory.split_peas),
            "spellcompmin-critical" => number(inventory.critical_component_minimum),
            "spellcompmin-normal" => number(inventory.normal_component_minimum),
            "spellcompmin-idle" => number(inventory.idle_component_minimum),
            "idlecraftcount_healthkits" => number(inventory.idle_health_kit_count),
            "idlecraftcount_stamkits" => number(inventory.idle_stamina_kit_count),
            "idlecraftcount_manakits" => number(inventory.idle_mana_kit_count),
            "idlecraftcount_healthfood" => number(inventory.idle_health_food_count),
            "idlecraftcount_stamfood" => number(inventory.idle_stamina_food_count),
            "idlecraftcount_manafood" => number(inventory.idle_mana_food_count),
            "refillwornmana" => boolean(inventory.refill_worn_mana),
            "manachargeswhenoff" => boolean(inventory.mana_charges_when_off),
            "refillwornmana-item-manapercent" => number(inventory.refill_worn_mana_percent),
            "readunknownscrolls" => boolean(loot.read_unknown_scrolls),
            "lootallcorpses" => boolean(loot.loot_all_corpses),
            "lootfellowcorpses" => boolean(loot.loot_fellow_corpses),
            "lootpriorityboost" => boolean(loot.priority_boost),
            "lootonlyrarecorpses" => boolean(loot.loot_only_rare_corpses),
            "combinesalvage" => boolean(loot.combine_salvage),
            "manastonelootcount" => number(loot.mana_stone_loot_count),
            "manatankminimummana" => number(loot.mana_tank_minimum_mana),
            "corpsecachetimeoutminutes" => number(loot.corpse_cache_timeout_minutes),
            "corpseitemappearancetimeoutseconds" => {
                number(loot.corpse_item_appearance_timeout_seconds)
            }
            "corpseitemidtimeoutseconds" => number(loot.corpse_item_identify_timeout_seconds),
            "corpseopentimeoutseconds" => number(loot.corpse_open_timeout_seconds),
            "blacklistcorpseopenattemptcount" => number(loot.blacklist_corpse_open_attempt_count),
            "blacklistcorpseopentimeoutseconds" => {
                number(loot.blacklist_corpse_open_timeout_seconds)
            }
            "corpselootitemmaxattempts" => number(loot.corpse_loot_item_max_attempts),
            "minimumhealkitsuccesschance" => number(vitals.minimum_heal_kit_success_chance),
            "usehealersheart" => boolean(vitals.use_healers_heart),
            "rechargeboosttimeseconds" => number(vitals.recharge_boost_time_seconds),
            "rechargeboostamount" => number(vitals.recharge_boost_amount),
            "clearlevelboostflagoncast" => boolean(vitals.clear_level_boost_flag_on_cast),
            "castdispelself" => boolean(vitals.cast_dispel_self),
            "usedispelitems" => boolean(vitals.use_dispel_items),
            "usedispeldrum" => boolean(vitals.use_dispel_drum),
            "usekitsinmagicmode" => boolean(vitals.use_kits_in_magic_mode),
            "gotopeacemodetousekits" => boolean(vitals.go_to_peace_mode_to_use_kits),
            "staminatohealthmultiplier" => number(vitals.stamina_to_health_multiplier),
            "manatohealthmultiplier" => number(vitals.mana_to_health_multiplier),
            "recharge-norm-hitp" => number(vitals.normal_health * 100.0),
            "recharge-norm-stam" => number(vitals.normal_stamina * 100.0),
            "recharge-norm-mana" => number(vitals.normal_mana * 100.0),
            "recharge-notarg-hitp" => number(vitals.no_target_health * 100.0),
            "recharge-notarg-stam" => number(vitals.no_target_stamina * 100.0),
            "recharge-notarg-mana" => number(vitals.no_target_mana * 100.0),
            "recharge-helper-hitp" => number(vitals.helper_health * 100.0),
            "recharge-helper-stam" => number(vitals.helper_stamina * 100.0),
            "recharge-helper-mana" => number(vitals.helper_mana * 100.0),
            "rebufftimeremainingseconds" => number(buffs.rebuff_when_under_seconds),
            "buffcastrecast_seconds" => number(buffs.buff_cast_recast_seconds),
            "buffcastrecastreset_seconds" => number(buffs.buff_cast_recast_reset_seconds),
            "blacklistedspellcomps" => ExpressionValue::string(&buffs.blacklisted_spell_components),
            "droptopeacemoderetrycount" => number(vitals.drop_to_peace_mode_retry_count),
            "fastcastbuffs" => boolean(buffs.fast_cast_buffs),
            "usebreakableturnto" => boolean(combat.use_breakable_turn_to),
            "useprojectileawareness" => boolean(combat.use_projectile_awareness),
            "collisionprojectileradius" => number(combat.collision_projectile_radius),
            "collisionstepdistance" => number(combat.collision_step_distance),
            "showcollisiondebug" => boolean(combat.show_collision_debug),
            "maximumcollisioncheckspertick" => number(combat.maximum_collision_checks_per_tick),
            "usespecialammo" => number(combat.use_special_ammo),
            "spellrangefudge" => number(combat.spell_range_fudge),
            "buffwithuntrained-item" => number(buffs.buff_with_untrained_item_skill),
            "buffwithuntrained-creature" => number(buffs.buff_with_untrained_creature_skill),
            "buffwithuntrained-life" => number(buffs.buff_with_untrained_life_skill),
            "allowdebufffallback" => boolean(combat.allow_debuff_fallback),
            "rechargehandlerset" => ExpressionValue::string(&vitals.recharge_handler_set),
            _ => combat.dynamic_settings.get(key).map_or_else(
                || to_expression_value(&options::default(key)),
                to_expression_value,
            ),
        }
    }

    /// The panel's one option-write path (`SetMetaOption`): false for a name
    /// that is no option. `EnableMeta` is written to the settings; the
    /// caller starts or stops the engine (register META-5).
    ///
    /// # Errors
    /// A value of the wrong type for the option.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn set_option(
        &mut self,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<bool, Fault> {
        if !options::is_known(name) {
            return Ok(false);
        }
        let canonical = options::canonical(name).unwrap_or(name).to_owned();
        let key = casing::to_lower_invariant(&canonical);
        let truthy = value.is_truthy();
        {
            let combat = &mut self.settings.combat;
            let buffs = &mut self.settings.buffs;
            let vitals = &mut self.settings.vitals;
            let inventory = &mut self.settings.inventory;
            let navigation = &mut self.settings.navigation;
            match key.as_str() {
                "enablebuffing" => buffs.enabled = truthy,
                "enablecombat" => combat.enabled = truthy,
                "enablenav" => navigation.enabled = truthy,
                "enablelooting" => inventory.loot.enabled = truthy,
                "walktorarecorpse" => inventory.loot.walk_to_own_rare_corpses = truthy,
                // Drawn by `TankCx::set_meta_option` and every frame.
                "shownavlines" => navigation.show_nav_lines = truthy,
                "enablemeta" => self.settings.meta.enabled = truthy,
                "spelldiffexcessthreshold-hunt" => {
                    combat.hunt_skill_excess_over_difficulty =
                        int(value, "SpellDiffExcessThreshold-Hunt", -100, 500)?;
                }
                "arrowheadfletchdiffexcessthreshold" => {
                    inventory.arrowhead_fletch_difficulty_excess =
                        int(value, "ArrowheadFletchDiffExcessThreshold", -100, 500)?;
                }
                "dohelp" => vitals.help_others = truthy,
                "attackdistance" => {
                    combat.maximum_range = single_meters(value, "AttackDistance", 1.0, 100.0)?;
                }
                "attackminimumdistance" => {
                    combat.minimum_range =
                        single_meters(value, "AttackMinimumDistance", 0.0, 100.0)?;
                }
                "approachdistance" => {
                    combat.approach_distance =
                        single_meters(value, "ApproachDistance", 0.0, 100.0)?;
                }
                "ringdistance" => {
                    combat.ring_distance = single_meters(value, "RingDistance", 1.0, 100.0)?;
                }
                "arcrange" => combat.arc_range = single_meters(value, "ArcRange", 1.0, 100.0)?,
                "targetselectanglerange" => {
                    combat.target_select_angle_range =
                        single_meters(value, "TargetSelectAngleRange", 1.0, 100.0)?;
                }
                // Zero turns the walk to a corpse off outright.
                "corpseapproachrange-max" => {
                    inventory.loot.corpse_approach_range =
                        single_meters(value, "CorpseApproachRange-Max", 0.0, 100.0)?;
                }
                "corpseapproachrange-min" => {
                    inventory.loot.corpse_minimum_approach_range =
                        single_meters(value, "CorpseApproachRange-Min", 0.0, 100.0)?;
                }
                // Kept as given, so it reads back unchanged; the navigator
                // bounds it where it is used.
                "navclosestoprange" => {
                    navigation.minimum_distance_meters =
                        value.as_number("NavCloseStopRange")? * METERS_PER_UNIT;
                }
                "navfarstoprange" => {
                    navigation.maximum_distance_meters = (value.as_number("NavFarStopRange")?
                        * METERS_PER_UNIT)
                        .max(navigation.minimum_distance_meters)
                        .min(240_000_000.0);
                }
                "useportaldistance" => {
                    navigation.portal_use_distance_meters =
                        (value.as_number("UsePortalDistance")? * METERS_PER_UNIT).clamp(0.5, 50.0);
                }
                "helperdistancehitp" => {
                    vitals.helper_health_distance =
                        single_meters(value, "HelperDistanceHitP", 1.0, 100.0)?;
                }
                "helperdistancestam" => {
                    vitals.helper_stamina_distance =
                        single_meters(value, "HelperDistanceStam", 1.0, 100.0)?;
                }
                "helperdistancemana" => {
                    vitals.helper_mana_distance =
                        single_meters(value, "HelperDistanceMana", 1.0, 100.0)?;
                }
                "minimumringtargets" => {
                    combat.minimum_ring_targets = int(value, "MinimumRingTargets", 1, 25)?;
                }
                "defaultmeleeattackheight" => {
                    combat.attack_height =
                        MeleeAttackHeight(int(value, "DefaultMeleeAttackHeight", 1, 3)?);
                }
                "targetlock" => combat.target_lock = truthy,
                "idlepeacemode" => combat.idle_peace_mode = truthy,
                "stopmacroondeath" => combat.stop_macro_on_death = truthy,
                "jumpoutwandcasting" => combat.jump_out_wand_casting = truthy,
                "dojiggle" => combat.do_jiggle = truthy,
                "randomhelperbuffs" => buffs.random_helper_buffs = truthy,
                "randomhelperintervalseconds" => {
                    buffs.random_helper_interval_seconds =
                        double(value, "RandomHelperIntervalSeconds", 0.25, 3600.0)?;
                }
                "idlebufftopoff" => buffs.idle_buff_topoff = truthy,
                "idlebufftopofftimeseconds" => {
                    buffs.idle_buff_topoff_seconds =
                        double(value, "IdleBuffTopoffTimeSeconds", 30.0, 7200.0)?;
                }
                "buffprofile-prots" => {
                    buffs.protection_elements =
                        normalize_element_profile(&value.to_display_string());
                }
                "buffprofile-banes" => {
                    buffs.bane_elements = normalize_element_profile(&value.to_display_string());
                }
                "buffprofile_prots" => {
                    buffs.protection_profile_mode = int(value, "BuffProfile_Prots", 1, 8)?;
                }
                "buffprofile_banes" => {
                    buffs.bane_profile_mode = int(value, "BuffProfile_Banes", 1, 8)?;
                }
                "targetselectmethod" => {
                    combat.selection_method = TargetSelectionMethod(
                        (value.as_int32("TargetSelectMethod")?.saturating_sub(1)).clamp(0, 2),
                    );
                }
                "autoattackpower" => combat.auto_attack_power = truthy,
                "userecklessness" => combat.use_recklessness = truthy,
                "debuffeachfirst" => {
                    combat.debuff_each_first =
                        DebuffEachFirst(int(value, "DebuffEachFirst", 1, 3)?);
                }
                "debuffselectionmethod" => {
                    combat.debuff_selection_method =
                        DebuffSelectionMethod(int(value, "DebuffSelectionMethod", 1, 2)?);
                }
                "debuffprecastseconds" => {
                    combat.debuff_precast_seconds =
                        double(value, "DebuffPrecastSeconds", 0.0, 60.0)?;
                }
                "switchwandstodebuff" => combat.switch_wands_to_debuff = truthy,
                "usearcs" => combat.use_arcs = UseArcsMode(int(value, "UseArcs", 1, 3)?),
                "deleteghostmonsters" => combat.delete_ghost_monsters = truthy,
                "ghostmonsterspellattemptcount" => {
                    combat.ghost_monster_spell_attempt_count =
                        int(value, "GhostMonsterSpellAttemptCount", 1, 1000)?;
                }
                "blacklistmonsterattemptcount" => {
                    combat.blacklist_monster_attempt_count =
                        int(value, "BlacklistMonsterAttemptCount", 1, 20)?;
                }
                "blacklistmonstertimeoutseconds" => {
                    combat.blacklist_monster_timeout_seconds =
                        double(value, "BlacklistMonsterTimeoutSeconds", 1.0, 3600.0)?;
                }
                "deleteghostmonstersbyhptracker" => {
                    combat.delete_ghost_monsters_by_health_tracker = truthy;
                }
                "ghostdeletehptrackerseconds" => {
                    combat.ghost_delete_health_tracker_seconds =
                        double(value, "GhostDeleteHPTrackerSeconds", 1.0, 300.0)?;
                }
                "summonpets" => combat.summon_pets = truthy,
                "petrangemode" => {
                    combat.pet_range_mode = PetRangeMode(int(value, "PetRangeMode", 0, 1)?);
                }
                "petcustomrange" => {
                    combat.pet_custom_range = single_meters(value, "PetCustomRange", 1.0, 100.0)?;
                }
                "petmonsterdensity" => {
                    combat.pet_monster_density = int(value, "PetMonsterDensity", 1, 25)?;
                }
                "petrefillcount-idle" => {
                    combat.pet_refill_count_idle = int(value, "PetRefillCount-Idle", 0, 3)?;
                }
                "petrefillcount-normal" => {
                    combat.pet_refill_count_normal = int(value, "PetRefillCount-Normal", 0, 3)?;
                }
                "opendoors" => navigation.open_doors = truthy,
                "dooridrange" => {
                    navigation.door_identify_range_meters =
                        (value.as_number("DoorIDRange")? * METERS_PER_UNIT).clamp(1.0, 100.0);
                }
                "dooropenrange" => {
                    navigation.door_open_range_meters = (value.as_number("DoorOpenRange")?
                        * METERS_PER_UNIT)
                        .max(0.5)
                        .min(navigation.door_identify_range_meters);
                }
                "doorlockpickdiffexcessthreshold" => {
                    navigation.door_lockpick_excess_threshold =
                        int(value, "DoorLockpickDiffExcessThreshold", -500, 500)?;
                }
                "navpriorityboost" => navigation.priority = truthy,
                "followaroundcorners" => navigation.follow_around_corners = truthy,
                "autofellowmanagement" => combat.auto_fellow_management = truthy,
                "autostack" => inventory.auto_stack = truthy,
                "autocram" => inventory.auto_cram = truthy,
                "autocraftitems" => inventory.auto_craft_items = truthy,
                "splitpeas" => inventory.split_peas = truthy,
                "spellcompmin-critical" => {
                    inventory.critical_component_minimum =
                        int(value, "SpellCompMin-Critical", 0, 1000)?;
                }
                "spellcompmin-normal" => {
                    inventory.normal_component_minimum =
                        int(value, "SpellCompMin-Normal", 0, 1000)?;
                }
                "spellcompmin-idle" => {
                    inventory.idle_component_minimum = int(value, "SpellCompMin-Idle", 0, 1000)?;
                }
                "idlecraftcount_healthkits" => {
                    inventory.idle_health_kit_count =
                        int(value, "IdleCraftCount_HealthKits", 0, 1000)?;
                }
                "idlecraftcount_stamkits" => {
                    inventory.idle_stamina_kit_count =
                        int(value, "IdleCraftCount_StamKits", 0, 1000)?;
                }
                "idlecraftcount_manakits" => {
                    inventory.idle_mana_kit_count = int(value, "IdleCraftCount_ManaKits", 0, 1000)?;
                }
                "idlecraftcount_healthfood" => {
                    inventory.idle_health_food_count =
                        int(value, "IdleCraftCount_HealthFood", 0, 1000)?;
                }
                "idlecraftcount_stamfood" => {
                    inventory.idle_stamina_food_count =
                        int(value, "IdleCraftCount_StamFood", 0, 1000)?;
                }
                "idlecraftcount_manafood" => {
                    inventory.idle_mana_food_count =
                        int(value, "IdleCraftCount_ManaFood", 0, 1000)?;
                }
                "refillwornmana" => inventory.refill_worn_mana = truthy,
                "manachargeswhenoff" => inventory.mana_charges_when_off = truthy,
                "refillwornmana-item-manapercent" => {
                    inventory.refill_worn_mana_percent =
                        int(value, "RefillWornMana-Item-ManaPercent", 0, 100)?;
                }
                "readunknownscrolls" => inventory.loot.read_unknown_scrolls = truthy,
                "lootallcorpses" => inventory.loot.loot_all_corpses = truthy,
                "lootfellowcorpses" => inventory.loot.loot_fellow_corpses = truthy,
                "lootpriorityboost" => inventory.loot.priority_boost = truthy,
                "lootonlyrarecorpses" => inventory.loot.loot_only_rare_corpses = truthy,
                "combinesalvage" => inventory.loot.combine_salvage = truthy,
                "manastonelootcount" => {
                    inventory.loot.mana_stone_loot_count =
                        int(value, "ManaStoneLootCount", 0, 1000)?;
                }
                "manatankminimummana" => {
                    inventory.loot.mana_tank_minimum_mana =
                        int(value, "ManaTankMinimumMana", 1, i32::MAX)?;
                }
                "corpsecachetimeoutminutes" => {
                    inventory.loot.corpse_cache_timeout_minutes =
                        double(value, "CorpseCacheTimeoutMinutes", 1.0, 1440.0)?;
                }
                "corpseitemappearancetimeoutseconds" => {
                    inventory.loot.corpse_item_appearance_timeout_seconds =
                        double(value, "CorpseItemAppearanceTimeoutSeconds", 0.0, 300.0)?;
                }
                "corpseitemidtimeoutseconds" => {
                    inventory.loot.corpse_item_identify_timeout_seconds =
                        double(value, "CorpseItemIDTimeoutSeconds", 1.0, 600.0)?;
                }
                "corpseopentimeoutseconds" => {
                    inventory.loot.corpse_open_timeout_seconds =
                        double(value, "CorpseOpenTimeoutSeconds", 0.1, 60.0)?;
                }
                "blacklistcorpseopenattemptcount" => {
                    inventory.loot.blacklist_corpse_open_attempt_count =
                        int(value, "BlacklistCorpseOpenAttemptCount", 1, 1000)?;
                }
                "blacklistcorpseopentimeoutseconds" => {
                    inventory.loot.blacklist_corpse_open_timeout_seconds =
                        double(value, "BlacklistCorpseOpenTimeoutSeconds", 1.0, 3600.0)?;
                }
                "corpselootitemmaxattempts" => {
                    inventory.loot.corpse_loot_item_max_attempts =
                        int(value, "CorpseLootItemMaxAttempts", 1, 1000)?;
                }
                "minimumhealkitsuccesschance" => {
                    vitals.minimum_heal_kit_success_chance =
                        int(value, "MinimumHealKitSuccessChance", 0, 100)?;
                }
                "usehealersheart" => {
                    vitals.use_healers_heart = truthy;
                    self.vital_recharge.reset();
                    self.vital_helper_recharge.reset();
                }
                "rechargeboosttimeseconds" => {
                    vitals.recharge_boost_time_seconds =
                        double(value, "RechargeBoostTimeSeconds", 0.0, 300.0)?;
                }
                "rechargeboostamount" => {
                    vitals.recharge_boost_amount = int(value, "RechargeBoostAmount", 0, 1000)?;
                }
                "clearlevelboostflagoncast" => vitals.clear_level_boost_flag_on_cast = truthy,
                "castdispelself" => {
                    vitals.cast_dispel_self = truthy;
                    self.dispel.reset();
                }
                "usedispelitems" => {
                    vitals.use_dispel_items = truthy;
                    self.dispel.reset();
                }
                "usedispeldrum" => {
                    vitals.use_dispel_drum = truthy;
                    self.dispel.reset();
                }
                "usekitsinmagicmode" => vitals.use_kits_in_magic_mode = truthy,
                "gotopeacemodetousekits" => vitals.go_to_peace_mode_to_use_kits = truthy,
                "staminatohealthmultiplier" => {
                    vitals.stamina_to_health_multiplier =
                        double(value, "StaminaToHealthMultiplier", 0.0, 10.0)?;
                }
                "manatohealthmultiplier" => {
                    vitals.mana_to_health_multiplier =
                        double(value, "ManaToHealthMultiplier", 0.0, 10.0)?;
                }
                "recharge-norm-hitp" => {
                    vitals.normal_health =
                        (value.as_number("Recharge-Norm-HitP")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-norm-stam" => {
                    vitals.normal_stamina =
                        (value.as_number("Recharge-Norm-Stam")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-norm-mana" => {
                    vitals.normal_mana =
                        (value.as_number("Recharge-Norm-Mana")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-notarg-hitp" => {
                    vitals.no_target_health =
                        (value.as_number("Recharge-NoTarg-HitP")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-notarg-stam" => {
                    vitals.no_target_stamina =
                        (value.as_number("Recharge-NoTarg-Stam")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-notarg-mana" => {
                    vitals.no_target_mana =
                        (value.as_number("Recharge-NoTarg-Mana")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-helper-hitp" => {
                    vitals.helper_health =
                        (value.as_number("Recharge-Helper-HitP")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-helper-stam" => {
                    vitals.helper_stamina =
                        (value.as_number("Recharge-Helper-Stam")? / 100.0).clamp(0.0, 1.0);
                }
                "recharge-helper-mana" => {
                    vitals.helper_mana =
                        (value.as_number("Recharge-Helper-Mana")? / 100.0).clamp(0.0, 1.0);
                }
                "spelldiffexcessthreshold-buff" => {
                    buffs.skill_excess_over_difficulty =
                        int(value, "SpellDiffExcessThreshold-Buff", -100, 100)?;
                }
                "rebufftimeremainingseconds" => {
                    buffs.rebuff_when_under_seconds =
                        double(value, "RebuffTimeRemainingSeconds", 0.0, 3600.0)?;
                }
                "buffcastrecast_seconds" => {
                    buffs.buff_cast_recast_seconds =
                        double(value, "BuffCastRecast_Seconds", 0.0, 3600.0)?;
                }
                "buffcastrecastreset_seconds" => {
                    buffs.buff_cast_recast_reset_seconds =
                        double(value, "BuffCastRecastReset_Seconds", 0.0, 3600.0)?;
                }
                "blacklistedspellcomps" => {
                    buffs.blacklisted_spell_components = value.to_display_string();
                    combat
                        .blacklisted_spell_components
                        .clone_from(&buffs.blacklisted_spell_components);
                }
                "droptopeacemoderetrycount" => {
                    vitals.drop_to_peace_mode_retry_count =
                        int(value, "DropToPeaceModeRetryCount", 1, 1000)?;
                }
                "fastcastbuffs" => buffs.fast_cast_buffs = truthy,
                "usebreakableturnto" => combat.use_breakable_turn_to = truthy,
                "useprojectileawareness" => combat.use_projectile_awareness = truthy,
                "collisionprojectileradius" => {
                    combat.collision_projectile_radius =
                        single(value, "CollisionProjectileRadius", 0.0, 10.0)?;
                }
                "collisionstepdistance" => {
                    combat.collision_step_distance =
                        single(value, "CollisionStepDistance", 0.01, 10.0)?;
                }
                "showcollisiondebug" => combat.show_collision_debug = truthy,
                "maximumcollisioncheckspertick" => {
                    combat.maximum_collision_checks_per_tick =
                        int(value, "MaximumCollisionChecksPerTick", 1, 100_000)?;
                }
                "usespecialammo" => {
                    combat.use_special_ammo = int(value, "UseSpecialAmmo", 0, 3)?;
                }
                "spellrangefudge" => {
                    combat.spell_range_fudge = single(value, "SpellRangeFudge", 0.0, 75.0)?;
                }
                "buffwithuntrained-item" => {
                    buffs.buff_with_untrained_item_skill =
                        int(value, "BuffWithUntrained-Item", 0, 275)?;
                }
                "buffwithuntrained-creature" => {
                    buffs.buff_with_untrained_creature_skill =
                        int(value, "BuffWithUntrained-Creature", 0, 275)?;
                }
                "buffwithuntrained-life" => {
                    buffs.buff_with_untrained_life_skill =
                        int(value, "BuffWithUntrained-Life", 0, 275)?;
                }
                "allowdebufffallback" => combat.allow_debuff_fallback = truthy,
                "rechargehandlerset" => vitals.recharge_handler_set = value.to_display_string(),
                _ => {}
            }
        }
        if canonical == options::WALK_TO_RARE_CORPSE {
            // Older side-cars may hold either previous command name.
            self.settings
                .combat
                .dynamic_settings
                .remove("WalkToOwnRareCorpses");
            self.settings
                .combat
                .dynamic_settings
                .remove("WalkToOwnCorpses");
        }
        self.settings
            .combat
            .dynamic_settings
            .insert(canonical, to_monster_value(value));
        if !self.applying_profile_options {
            // Saved once the call has made its changes (register STORE-7).
            self.settings_save_due = true;
        }
        Ok(true)
    }
}

impl TankCx<'_> {
    /// An option's value.
    pub(crate) fn get_meta_option(&self, name: &str) -> ExpressionValue {
        self.core.get_meta_option(self.meta.enabled(), name)
    }

    /// Sets an option; the meta engine follows `EnableMeta` at once.
    ///
    /// # Errors
    /// A value of the wrong type for the option.
    pub(crate) fn set_meta_option(
        &mut self,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<bool, Fault> {
        let written = self.core.set_option(name, value)?;
        if written {
            let canonical = options::canonical(name).map(casing::to_lower_invariant);
            match canonical.as_deref() {
                Some("enablemeta") => self.meta.set_enabled(value.is_truthy()),
                // The route's lines follow the switch at once.
                Some("shownavlines") => self.update_nav_lines(),
                _ => {}
            }
        }
        Ok(written)
    }
}

/// `bool.TryParse`.
pub(crate) fn parse_bool(text: &str) -> Option<bool> {
    let text = text.trim_matches(|c: char| casing::is_white_space(c) || c == '\0');
    if mosstank_dotnet::text::ordinal::equals_ignore_case(text, "true") {
        Some(true)
    } else if mosstank_dotnet::text::ordinal::equals_ignore_case(text, "false") {
        Some(false)
    } else {
        None
    }
}

/// `TryParseOptionValue`: a typed value as the option's declared type reads
/// it; `None` when it does not.
pub(crate) fn try_parse_option_value(
    source: &str,
    declared: Option<VtankSettingValueType>,
) -> Option<ExpressionValue> {
    if source.is_empty() {
        return None;
    }
    match declared {
        Some(VtankSettingValueType::Bool) => parse_bool(source).map(ExpressionValue::Boolean),
        Some(VtankSettingValueType::Int | VtankSettingValueType::Enum) => {
            number::parse_i32(source, NumberStyles::INTEGER)
                .map(|integer| ExpressionValue::Number(f64::from(integer)))
        }
        Some(VtankSettingValueType::Double | VtankSettingValueType::Single) => {
            number::parse_f64(source, NumberStyles::FLOAT).map(ExpressionValue::Number)
        }
        Some(VtankSettingValueType::String) => Some(ExpressionValue::string(source)),
        _ => Some(
            parse_bool(source)
                .map(ExpressionValue::Boolean)
                .or_else(|| {
                    number::parse_f64(source, NumberStyles::FLOAT).map(ExpressionValue::Number)
                })
                .unwrap_or_else(|| ExpressionValue::string(source)),
        ),
    }
}

/// `DeclaredClrTypeName`.
pub(crate) fn declared_clr_type_name(declared: VtankSettingValueType) -> &'static str {
    match declared {
        VtankSettingValueType::Bool => "System.Boolean",
        VtankSettingValueType::Int | VtankSettingValueType::Enum => "System.Int32",
        VtankSettingValueType::Double => "System.Double",
        VtankSettingValueType::Single => "System.Single",
        VtankSettingValueType::String => "System.String",
        _ => "System.Object",
    }
}
