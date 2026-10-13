//! The VTank settings profile (`.usd`): its `Settings` table read into the
//! live settings and written back from them, with the monster rules and the
//! item tables beside it. A save changes only the cells whose value
//! changed, so an untouched profile renders byte for byte as it was read.

use mosstank_dotnet::{double_to_int, double_to_string, round_away_from_zero};
use mosstank_formats::FormatError;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};
use openac_plugin_compat::text::casing;

use crate::items::{
    assist_items, buff_exemplars, gem_food_items, item_use_specifiers, profiled_item_ids,
};
use crate::model::{
    AllSettings, DebuffEachFirst, DebuffSelectionMethod, MeleeAttackHeight, PetRangeMode,
    RechargeHandlerRow, TargetSelectionMethod, UseArcsMode,
};
use crate::monster::MonsterValue;
use crate::options::{self, VtankSettingValueType};
use crate::{defaults, monster_table};

const SETTINGS_TABLE: &str = "Settings";

/// Metres per unit of the distances a profile stores.
const METERS_PER_UNIT: f64 = 240.0;

/// .NET's `ArgumentOutOfRangeException` message for a list index.
const INDEX_OUT_OF_RANGE: &str = "Index was out of range. Must be non-negative and less than the size of the collection. (Parameter 'index')";

fn cell_at(row: &VtankRow, column: Option<usize>) -> Result<&VtankCell, FormatError> {
    column
        .and_then(|column| row.cells.get(column))
        .ok_or_else(|| FormatError(INDEX_OUT_OF_RANGE.to_owned()))
}

/// Parses a profile and puts it into the live settings.
///
/// # Errors
/// The text does not parse, the `Settings` table is missing or malformed,
/// or a value does not read as its setting's type.
pub fn load(
    text: &str,
    target: &mut AllSettings,
    warn: Option<&mut dyn FnMut(&str)>,
) -> Result<VtankDatabase, FormatError> {
    let database = VtankDatabase::parse(text)?;
    apply_database(&database, target, warn)?;
    Ok(database)
}

/// Puts a parsed profile into the live settings: its settings, its monster
/// rules (when it has any) and every item table beside them.
///
/// # Errors
/// As [`load`].
pub fn apply_database(
    database: &VtankDatabase,
    target: &mut AllSettings,
    warn: Option<&mut dyn FnMut(&str)>,
) -> Result<(), FormatError> {
    apply_seed_database(database, target)?;
    if let Some(rules) = monster_table::try_read(database, warn)?
        && !rules.is_empty()
    {
        target.combat.rules = rules;
    }
    Ok(())
}

/// The same without the monster rules, for a profile seeded from the
/// defaults (those take their rules from the record saved beside the
/// profile). Every item table is read, each clearing what it owns first.
///
/// # Errors
/// The `Settings` table is missing or malformed, or a value does not read.
pub fn apply_seed_database(
    database: &VtankDatabase,
    target: &mut AllSettings,
) -> Result<(), FormatError> {
    let settings = database
        .find(SETTINGS_TABLE)
        .ok_or_else(|| FormatError("missing required 'Settings' table.".to_owned()))?;
    let (Some(name_column), Some(value_column)) = (
        settings.column_index("Setting"),
        settings.column_index("Value"),
    ) else {
        return Err(FormatError(
            "'Settings' table is missing Setting/Value columns.".to_owned(),
        ));
    };
    for row in &settings.rows {
        let name = cell_at(row, Some(name_column))?.as_string();
        apply(name, cell_at(row, Some(value_column))?, target)?;
    }

    profiled_item_ids::read(
        database,
        &mut target.combat,
        &mut target.buffs,
        target.player_object_id,
    );
    item_use_specifiers::read(database, &mut target.combat);
    assist_items::read(database, &mut target.combat);
    gem_food_items::read(database, &mut target.buffs);
    buff_exemplars::read(database, &mut target.buffs);
    Ok(())
}

/// Writes the live settings into the document (changing only cells whose
/// value differs) with the rules and item tables, and renders it.
///
/// # Errors
/// The `Settings` table is missing or malformed, or an item table lacks
/// its columns.
pub fn save(document: &mut VtankDatabase, source: &mut AllSettings) -> Result<String, FormatError> {
    let settings = document
        .find_mut(SETTINGS_TABLE)
        .ok_or_else(|| FormatError("missing required 'Settings' table.".to_owned()))?;
    let name_column = settings.column_index("Setting");
    let value_column = settings.column_index("Value");
    for row in &mut settings.rows {
        let name = cell_at(row, name_column)?.as_string().to_owned();
        let Some(captured) = capture(&name, source) else {
            continue;
        };
        let current = cell_at(row, value_column)?;
        if values_equal(current, &captured)? {
            continue;
        }
        let column = value_column.expect("cell_at found the column");
        row.cells[column] = captured;
    }
    monster_table::write(document, &source.combat.rules)?;
    profiled_item_ids::write(document, &mut source.combat)?;
    assist_items::write(document, &source.combat)?;
    gem_food_items::write(document, &source.buffs)?;
    buff_exemplars::write(document, &source.buffs)?;
    Ok(document.render())
}

/// A new profile: the default document with the live settings saved into
/// it.
///
/// # Errors
/// As [`save`] (the default document always has its tables).
pub fn create_new(source: &mut AllSettings) -> Result<VtankDatabase, FormatError> {
    let mut database = defaults::create();
    save(&mut database, source)?;
    Ok(database)
}

/// A number as the cell the setting's declared type calls for.
fn num(name: &str, value: f64) -> VtankCell {
    match options::declared_type(name) {
        VtankSettingValueType::Bool => VtankCell::bool(value != 0.0),
        VtankSettingValueType::Int | VtankSettingValueType::Enum => {
            VtankCell::int(double_to_int(round_away_from_zero(value)))
        }
        #[allow(clippy::cast_possible_truncation)]
        VtankSettingValueType::Single => VtankCell::float(value as f32),
        VtankSettingValueType::String => VtankCell::string(&double_to_string(value)),
        _ => VtankCell::double(value),
    }
}

#[allow(clippy::float_cmp)]
fn values_equal(a: &VtankCell, b: &VtankCell) -> Result<bool, FormatError> {
    if a.tag != b.tag {
        return Ok(false);
    }
    Ok(match a.tag.as_str() {
        "b" => a.as_bool() == b.as_bool(),
        "s" => a.as_string() == b.as_string(),
        "i" => a.as_int()? == b.as_int()?,
        "u" => a.as_uint()? == b.as_uint()?,
        "d" | "f" => a.as_double()? == b.as_double()?,
        _ => false,
    })
}

/// Puts one Settings row's value into the live settings; a name the
/// catalogue does not know is left alone.
///
/// # Errors
/// The value does not read as the setting's type.
#[allow(clippy::too_many_lines)]
pub fn apply(raw_name: &str, cell: &VtankCell, s: &mut AllSettings) -> Result<(), FormatError> {
    let Some(canonical) = options::canonical(raw_name) else {
        return Ok(());
    };
    let name = casing::to_lower_invariant(canonical);
    let c = &mut s.combat;
    let b = &mut s.buffs;
    let v = &mut s.vitals;
    let i = &mut s.inventory;
    let n = &mut s.navigation;
    match name.as_str() {
        "enablelooting" => i.loot.enabled = cell.as_bool(),
        "enablenav" => n.enabled = cell.as_bool(),
        "enablebuffing" => b.enabled = cell.as_bool(),
        "enablecombat" => c.enabled = cell.as_bool(),
        "spelldiffexcessthreshold-hunt" => c.hunt_skill_excess_over_difficulty = cell.as_int()?,
        "spelldiffexcessthreshold-buff" => b.skill_excess_over_difficulty = cell.as_int()?,
        "arrowheadfletchdiffexcessthreshold" => {
            i.arrowhead_fletch_difficulty_excess = cell.as_int()?;
        }
        "recharge-norm-hitp" => v.normal_health = cell.as_double()? / 100.0,
        "recharge-norm-stam" => v.normal_stamina = cell.as_double()? / 100.0,
        "recharge-norm-mana" => v.normal_mana = cell.as_double()? / 100.0,
        "recharge-notarg-hitp" => v.no_target_health = cell.as_double()? / 100.0,
        "recharge-notarg-stam" => v.no_target_stamina = cell.as_double()? / 100.0,
        "recharge-notarg-mana" => v.no_target_mana = cell.as_double()? / 100.0,
        "recharge-helper-hitp" => v.helper_health = cell.as_double()? / 100.0,
        "recharge-helper-stam" => v.helper_stamina = cell.as_double()? / 100.0,
        "recharge-helper-mana" => v.helper_mana = cell.as_double()? / 100.0,
        "dohelp" => v.help_others = cell.as_bool(),
        "attackdistance" => c.maximum_range = cell.as_double()? * METERS_PER_UNIT,
        "attackminimumdistance" => c.minimum_range = cell.as_double()? * METERS_PER_UNIT,
        "approachdistance" => c.approach_distance = cell.as_double()? * METERS_PER_UNIT,
        "ringdistance" => c.ring_distance = cell.as_double()? * METERS_PER_UNIT,
        "corpseapproachrange-max" => {
            i.loot.corpse_approach_range = cell.as_double()? * METERS_PER_UNIT;
        }
        "corpseapproachrange-min" => {
            i.loot.corpse_minimum_approach_range = cell.as_double()? * METERS_PER_UNIT;
        }
        "navclosestoprange" => n.minimum_distance_meters = cell.as_double()? * METERS_PER_UNIT,
        "navfarstoprange" => n.maximum_distance_meters = cell.as_double()? * METERS_PER_UNIT,
        "useportaldistance" => n.portal_use_distance_meters = cell.as_double()? * METERS_PER_UNIT,
        "helperdistancehitp" => v.helper_health_distance = cell.as_double()? * METERS_PER_UNIT,
        "helperdistancestam" => v.helper_stamina_distance = cell.as_double()? * METERS_PER_UNIT,
        "helperdistancemana" => v.helper_mana_distance = cell.as_double()? * METERS_PER_UNIT,
        "minimumringtargets" => c.minimum_ring_targets = cell.as_int()?,
        "defaultmeleeattackheight" => c.attack_height = MeleeAttackHeight(cell.as_int()?),
        "castdispelself" => v.cast_dispel_self = cell.as_bool(),
        "usedispelitems" => v.use_dispel_items = cell.as_bool(),
        "autocram" => i.auto_cram = cell.as_bool(),
        "autostack" => i.auto_stack = cell.as_bool(),
        "readunknownscrolls" => i.loot.read_unknown_scrolls = cell.as_bool(),
        "usedispeldrum" => v.use_dispel_drum = cell.as_bool(),
        "switchwandstodebuff" => c.switch_wands_to_debuff = cell.as_bool(),
        "autocraftitems" => i.auto_craft_items = cell.as_bool(),
        "usehealersheart" => v.use_healers_heart = cell.as_bool(),
        "jumpoutwandcasting" => c.jump_out_wand_casting = cell.as_bool(),
        "lootallcorpses" => i.loot.loot_all_corpses = cell.as_bool(),
        "lootfellowcorpses" => i.loot.loot_fellow_corpses = cell.as_bool(),
        "dojiggle" => c.do_jiggle = cell.as_bool(),
        "randomhelperbuffs" => b.random_helper_buffs = cell.as_bool(),
        "randomhelperintervalseconds" => b.random_helper_interval_seconds = cell.as_double()?,
        "idlepeacemode" => c.idle_peace_mode = cell.as_bool(),
        "targetlock" => c.target_lock = cell.as_bool(),
        "stopmacroondeath" => c.stop_macro_on_death = cell.as_bool(),
        "usearcs" => c.use_arcs = UseArcsMode(cell.as_int()?),
        "arcrange" => c.arc_range = cell.as_double()? * METERS_PER_UNIT,
        "targetselectmethod" => {
            c.selection_method = TargetSelectionMethod(cell.as_int()?.wrapping_sub(1));
        }
        "targetselectanglerange" => {
            c.target_select_angle_range = cell.as_double()? * METERS_PER_UNIT;
        }
        "idlebufftopoff" => b.idle_buff_topoff = cell.as_bool(),
        "idlebufftopofftimeseconds" => b.idle_buff_topoff_seconds = cell.as_double()?,
        "rebufftimeremainingseconds" => b.rebuff_when_under_seconds = cell.as_double()?,
        "refillwornmana" => i.refill_worn_mana = cell.as_bool(),
        "refillwornmana-item-manapercent" => i.refill_worn_mana_percent = cell.as_int()?,
        "buffprofile-prots" => b.protection_elements = cell.as_string().to_owned(),
        "buffprofile-banes" => b.bane_elements = cell.as_string().to_owned(),
        "buffprofile_prots" => b.protection_profile_mode = cell.as_int()?,
        "buffprofile_banes" => b.bane_profile_mode = cell.as_int()?,
        "debuffeachfirst" => c.debuff_each_first = DebuffEachFirst(cell.as_int()?),
        "autoattackpower" => c.auto_attack_power = cell.as_bool(),
        "lootpriorityboost" => i.loot.priority_boost = cell.as_bool(),
        "corpsecachetimeoutminutes" => i.loot.corpse_cache_timeout_minutes = cell.as_double()?,
        "corpseitemappearancetimeoutseconds" => {
            i.loot.corpse_item_appearance_timeout_seconds = cell.as_double()?;
        }
        "corpseitemidtimeoutseconds" => {
            i.loot.corpse_item_identify_timeout_seconds = cell.as_double()?;
        }
        "debuffselectionmethod" => {
            c.debuff_selection_method = DebuffSelectionMethod(cell.as_int()?);
        }
        "manastonelootcount" => i.loot.mana_stone_loot_count = cell.as_int()?,
        "manatankminimummana" => i.loot.mana_tank_minimum_mana = cell.as_int()?,
        "splitpeas" => i.split_peas = cell.as_bool(),
        "spellcompmin-critical" => i.critical_component_minimum = cell.as_int()?,
        "spellcompmin-normal" => i.normal_component_minimum = cell.as_int()?,
        "spellcompmin-idle" => i.idle_component_minimum = cell.as_int()?,
        "rechargeboosttimeseconds" => v.recharge_boost_time_seconds = cell.as_double()?,
        "rechargeboostamount" => v.recharge_boost_amount = cell.as_int()?,
        "usespecialammo" => c.use_special_ammo = cell.as_int()?,
        "opendoors" => n.open_doors = cell.as_bool(),
        "dooridrange" => n.door_identify_range_meters = cell.as_double()? * METERS_PER_UNIT,
        "dooropenrange" => n.door_open_range_meters = cell.as_double()? * METERS_PER_UNIT,
        "doorlockpickdiffexcessthreshold" => n.door_lockpick_excess_threshold = cell.as_int()?,
        "manachargeswhenoff" => i.mana_charges_when_off = cell.as_bool(),
        "autofellowmanagement" => c.auto_fellow_management = cell.as_bool(),
        "minimumhealkitsuccesschance" => v.minimum_heal_kit_success_chance = cell.as_int()?,
        "usekitsinmagicmode" => v.use_kits_in_magic_mode = cell.as_bool(),
        "staminatohealthmultiplier" => v.stamina_to_health_multiplier = cell.as_double()?,
        "manatohealthmultiplier" => v.mana_to_health_multiplier = cell.as_double()?,
        "navpriorityboost" => n.priority = cell.as_bool(),
        "deleteghostmonsters" => c.delete_ghost_monsters = cell.as_bool(),
        "ghostmonsterspellattemptcount" => c.ghost_monster_spell_attempt_count = cell.as_int()?,
        "blacklistmonsterattemptcount" => c.blacklist_monster_attempt_count = cell.as_int()?,
        "blacklistmonstertimeoutseconds" => {
            c.blacklist_monster_timeout_seconds = cell.as_double()?;
        }
        "combinesalvage" => i.loot.combine_salvage = cell.as_bool(),
        "lootonlyrarecorpses" => i.loot.loot_only_rare_corpses = cell.as_bool(),
        "deleteghostmonstersbyhptracker" => {
            c.delete_ghost_monsters_by_health_tracker = cell.as_bool();
        }
        "ghostdeletehptrackerseconds" => {
            c.ghost_delete_health_tracker_seconds = cell.as_double()?;
        }
        "gotopeacemodetousekits" => v.go_to_peace_mode_to_use_kits = cell.as_bool(),
        "userecklessness" => c.use_recklessness = cell.as_bool(),
        "debuffprecastseconds" => c.debuff_precast_seconds = cell.as_double()?,
        "clearlevelboostflagoncast" => v.clear_level_boost_flag_on_cast = cell.as_bool(),
        "idlecraftcount_healthkits" => i.idle_health_kit_count = cell.as_int()?,
        "idlecraftcount_stamkits" => i.idle_stamina_kit_count = cell.as_int()?,
        "idlecraftcount_manakits" => i.idle_mana_kit_count = cell.as_int()?,
        "idlecraftcount_healthfood" => i.idle_health_food_count = cell.as_int()?,
        "idlecraftcount_stamfood" => i.idle_stamina_food_count = cell.as_int()?,
        "idlecraftcount_manafood" => i.idle_mana_food_count = cell.as_int()?,
        "buffcastrecast_seconds" => b.buff_cast_recast_seconds = cell.as_double()?,
        "buffcastrecastreset_seconds" => b.buff_cast_recast_reset_seconds = cell.as_double()?,
        "enablemeta" => s.meta.enabled = cell.as_bool(),
        "blacklistedspellcomps" => {
            b.blacklisted_spell_components = cell.as_string().to_owned();
            c.blacklisted_spell_components = cell.as_string().to_owned();
        }
        "droptopeacemoderetrycount" => v.drop_to_peace_mode_retry_count = cell.as_int()?,
        "followaroundcorners" => n.follow_around_corners = cell.as_bool(),
        "blacklistcorpseopenattemptcount" => {
            i.loot.blacklist_corpse_open_attempt_count = cell.as_int()?;
        }
        "blacklistcorpseopentimeoutseconds" => {
            i.loot.blacklist_corpse_open_timeout_seconds = cell.as_double()?;
        }
        "summonpets" => c.summon_pets = cell.as_bool(),
        "petrangemode" => c.pet_range_mode = PetRangeMode(cell.as_int()?),
        "petcustomrange" => c.pet_custom_range = cell.as_double()? * METERS_PER_UNIT,
        "petrefillcount-idle" => c.pet_refill_count_idle = cell.as_int()?,
        "petrefillcount-normal" => c.pet_refill_count_normal = cell.as_int()?,
        "corpseopentimeoutseconds" => i.loot.corpse_open_timeout_seconds = cell.as_double()?,
        "petmonsterdensity" => c.pet_monster_density = cell.as_int()?,
        "corpselootitemmaxattempts" => i.loot.corpse_loot_item_max_attempts = cell.as_int()?,
        "fastcastbuffs" => b.fast_cast_buffs = cell.as_bool(),
        "usebreakableturnto" => c.use_breakable_turn_to = cell.as_bool(),
        "useprojectileawareness" => c.use_projectile_awareness = cell.as_bool(),
        "collisionprojectileradius" => c.collision_projectile_radius = cell.as_double()?,
        "collisionstepdistance" => c.collision_step_distance = cell.as_double()?,
        "showcollisiondebug" => c.show_collision_debug = cell.as_bool(),
        "maximumcollisioncheckspertick" => c.maximum_collision_checks_per_tick = cell.as_int()?,
        "spellrangefudge" => c.spell_range_fudge = cell.as_double()?,
        "buffwithuntrained-item" => b.buff_with_untrained_item_skill = cell.as_int()?,
        "buffwithuntrained-creature" => b.buff_with_untrained_creature_skill = cell.as_int()?,
        "buffwithuntrained-life" => b.buff_with_untrained_life_skill = cell.as_int()?,
        "allowdebufffallback" => c.allow_debuff_fallback = cell.as_bool(),
        // Kept, and read back, but drives nothing: the reference reads it
        // nowhere either.
        "whoyougonnacall" => {
            c.dynamic_settings.insert(
                options::UNUSED_SETTING.to_owned(),
                MonsterValue::from_boolean(cell.as_bool()),
            );
        }
        "rechargehandlerset" => {
            if cell.tag == "TABLE"
                && let Some(table) = &cell.table
            {
                v.recharge_handler_rows = parse_recharge_handler_set(table)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The cell one setting is saved as, `None` for a name the catalogue does
/// not know and for the recharge plan (which has no write path in the
/// reference client either).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn capture(raw_name: &str, s: &AllSettings) -> Option<VtankCell> {
    let canonical = options::canonical(raw_name)?;
    let name = casing::to_lower_invariant(canonical);
    let n_ = |value: f64| num(&name, value);
    let per_unit = |meters: f64| num(&name, meters / METERS_PER_UNIT);
    let percent = |fraction: f64| num(&name, fraction * 100.0);
    let int = |value: i32| num(&name, f64::from(value));
    let flag = VtankCell::bool;
    let c = &s.combat;
    let b = &s.buffs;
    let v = &s.vitals;
    let i = &s.inventory;
    let n = &s.navigation;
    Some(match name.as_str() {
        "enablelooting" => flag(i.loot.enabled),
        "enablenav" => flag(n.enabled),
        "enablebuffing" => flag(b.enabled),
        "enablecombat" => flag(c.enabled),
        "spelldiffexcessthreshold-hunt" => int(c.hunt_skill_excess_over_difficulty),
        "spelldiffexcessthreshold-buff" => int(b.skill_excess_over_difficulty),
        "arrowheadfletchdiffexcessthreshold" => int(i.arrowhead_fletch_difficulty_excess),
        "recharge-norm-hitp" => percent(v.normal_health),
        "recharge-norm-stam" => percent(v.normal_stamina),
        "recharge-norm-mana" => percent(v.normal_mana),
        "recharge-notarg-hitp" => percent(v.no_target_health),
        "recharge-notarg-stam" => percent(v.no_target_stamina),
        "recharge-notarg-mana" => percent(v.no_target_mana),
        "recharge-helper-hitp" => percent(v.helper_health),
        "recharge-helper-stam" => percent(v.helper_stamina),
        "recharge-helper-mana" => percent(v.helper_mana),
        "dohelp" => flag(v.help_others),
        "attackdistance" => per_unit(c.maximum_range),
        "attackminimumdistance" => per_unit(c.minimum_range),
        "approachdistance" => per_unit(c.approach_distance),
        "ringdistance" => per_unit(c.ring_distance),
        "corpseapproachrange-max" => per_unit(i.loot.corpse_approach_range),
        "corpseapproachrange-min" => per_unit(i.loot.corpse_minimum_approach_range),
        "navclosestoprange" => per_unit(n.minimum_distance_meters),
        "navfarstoprange" => per_unit(n.maximum_distance_meters),
        "useportaldistance" => per_unit(n.portal_use_distance_meters),
        "helperdistancehitp" => per_unit(v.helper_health_distance),
        "helperdistancestam" => per_unit(v.helper_stamina_distance),
        "helperdistancemana" => per_unit(v.helper_mana_distance),
        "minimumringtargets" => int(c.minimum_ring_targets),
        "defaultmeleeattackheight" => int(c.attack_height.0),
        "castdispelself" => flag(v.cast_dispel_self),
        "usedispelitems" => flag(v.use_dispel_items),
        "autocram" => flag(i.auto_cram),
        "autostack" => flag(i.auto_stack),
        "readunknownscrolls" => flag(i.loot.read_unknown_scrolls),
        "usedispeldrum" => flag(v.use_dispel_drum),
        "switchwandstodebuff" => flag(c.switch_wands_to_debuff),
        "autocraftitems" => flag(i.auto_craft_items),
        "usehealersheart" => flag(v.use_healers_heart),
        "jumpoutwandcasting" => flag(c.jump_out_wand_casting),
        "lootallcorpses" => flag(i.loot.loot_all_corpses),
        "lootfellowcorpses" => flag(i.loot.loot_fellow_corpses),
        "dojiggle" => flag(c.do_jiggle),
        "randomhelperbuffs" => flag(b.random_helper_buffs),
        "randomhelperintervalseconds" => n_(b.random_helper_interval_seconds),
        "idlepeacemode" => flag(c.idle_peace_mode),
        "targetlock" => flag(c.target_lock),
        "stopmacroondeath" => flag(c.stop_macro_on_death),
        "usearcs" => int(c.use_arcs.0),
        "arcrange" => per_unit(c.arc_range),
        "targetselectmethod" => int(c.selection_method.0.wrapping_add(1)),
        "targetselectanglerange" => per_unit(c.target_select_angle_range),
        "idlebufftopoff" => flag(b.idle_buff_topoff),
        "idlebufftopofftimeseconds" => n_(b.idle_buff_topoff_seconds),
        "rebufftimeremainingseconds" => n_(b.rebuff_when_under_seconds),
        "refillwornmana" => flag(i.refill_worn_mana),
        "refillwornmana-item-manapercent" => int(i.refill_worn_mana_percent),
        "buffprofile-prots" => VtankCell::string(&b.protection_elements),
        "buffprofile-banes" => VtankCell::string(&b.bane_elements),
        "buffprofile_prots" => int(b.protection_profile_mode),
        "buffprofile_banes" => int(b.bane_profile_mode),
        "debuffeachfirst" => int(c.debuff_each_first.0),
        "autoattackpower" => flag(c.auto_attack_power),
        "lootpriorityboost" => flag(i.loot.priority_boost),
        "corpsecachetimeoutminutes" => n_(i.loot.corpse_cache_timeout_minutes),
        "corpseitemappearancetimeoutseconds" => n_(i.loot.corpse_item_appearance_timeout_seconds),
        "corpseitemidtimeoutseconds" => n_(i.loot.corpse_item_identify_timeout_seconds),
        "debuffselectionmethod" => int(c.debuff_selection_method.0),
        "manastonelootcount" => int(i.loot.mana_stone_loot_count),
        "manatankminimummana" => int(i.loot.mana_tank_minimum_mana),
        "splitpeas" => flag(i.split_peas),
        "spellcompmin-critical" => int(i.critical_component_minimum),
        "spellcompmin-normal" => int(i.normal_component_minimum),
        "spellcompmin-idle" => int(i.idle_component_minimum),
        "rechargeboosttimeseconds" => n_(v.recharge_boost_time_seconds),
        "rechargeboostamount" => int(v.recharge_boost_amount),
        "usespecialammo" => int(c.use_special_ammo),
        "opendoors" => flag(n.open_doors),
        "dooridrange" => per_unit(n.door_identify_range_meters),
        "dooropenrange" => per_unit(n.door_open_range_meters),
        "doorlockpickdiffexcessthreshold" => int(n.door_lockpick_excess_threshold),
        "manachargeswhenoff" => flag(i.mana_charges_when_off),
        "autofellowmanagement" => flag(c.auto_fellow_management),
        "minimumhealkitsuccesschance" => int(v.minimum_heal_kit_success_chance),
        "usekitsinmagicmode" => flag(v.use_kits_in_magic_mode),
        "staminatohealthmultiplier" => n_(v.stamina_to_health_multiplier),
        "manatohealthmultiplier" => n_(v.mana_to_health_multiplier),
        "navpriorityboost" => flag(n.priority),
        "deleteghostmonsters" => flag(c.delete_ghost_monsters),
        "ghostmonsterspellattemptcount" => int(c.ghost_monster_spell_attempt_count),
        "blacklistmonsterattemptcount" => int(c.blacklist_monster_attempt_count),
        "blacklistmonstertimeoutseconds" => n_(c.blacklist_monster_timeout_seconds),
        "combinesalvage" => flag(i.loot.combine_salvage),
        "lootonlyrarecorpses" => flag(i.loot.loot_only_rare_corpses),
        "deleteghostmonstersbyhptracker" => flag(c.delete_ghost_monsters_by_health_tracker),
        "ghostdeletehptrackerseconds" => n_(c.ghost_delete_health_tracker_seconds),
        "gotopeacemodetousekits" => flag(v.go_to_peace_mode_to_use_kits),
        "userecklessness" => flag(c.use_recklessness),
        "debuffprecastseconds" => n_(c.debuff_precast_seconds),
        "clearlevelboostflagoncast" => flag(v.clear_level_boost_flag_on_cast),
        "idlecraftcount_healthkits" => int(i.idle_health_kit_count),
        "idlecraftcount_stamkits" => int(i.idle_stamina_kit_count),
        "idlecraftcount_manakits" => int(i.idle_mana_kit_count),
        "idlecraftcount_healthfood" => int(i.idle_health_food_count),
        "idlecraftcount_stamfood" => int(i.idle_stamina_food_count),
        "idlecraftcount_manafood" => int(i.idle_mana_food_count),
        "buffcastrecast_seconds" => n_(b.buff_cast_recast_seconds),
        "buffcastrecastreset_seconds" => n_(b.buff_cast_recast_reset_seconds),
        "blacklistedspellcomps" => VtankCell::string(&b.blacklisted_spell_components),
        "droptopeacemoderetrycount" => int(v.drop_to_peace_mode_retry_count),
        "followaroundcorners" => flag(n.follow_around_corners),
        "blacklistcorpseopenattemptcount" => int(i.loot.blacklist_corpse_open_attempt_count),
        "blacklistcorpseopentimeoutseconds" => n_(i.loot.blacklist_corpse_open_timeout_seconds),
        "summonpets" => flag(c.summon_pets),
        "petrangemode" => int(c.pet_range_mode.0),
        "petcustomrange" => per_unit(c.pet_custom_range),
        "petrefillcount-idle" => int(c.pet_refill_count_idle),
        "petrefillcount-normal" => int(c.pet_refill_count_normal),
        "corpseopentimeoutseconds" => n_(i.loot.corpse_open_timeout_seconds),
        "petmonsterdensity" => int(c.pet_monster_density),
        "corpselootitemmaxattempts" => int(i.loot.corpse_loot_item_max_attempts),
        "fastcastbuffs" => flag(b.fast_cast_buffs),
        "usebreakableturnto" => flag(c.use_breakable_turn_to),
        "useprojectileawareness" => flag(c.use_projectile_awareness),
        "collisionprojectileradius" => n_(c.collision_projectile_radius),
        "collisionstepdistance" => n_(c.collision_step_distance),
        "showcollisiondebug" => flag(c.show_collision_debug),
        "maximumcollisioncheckspertick" => int(c.maximum_collision_checks_per_tick),
        "spellrangefudge" => n_(c.spell_range_fudge),
        "buffwithuntrained-item" => int(b.buff_with_untrained_item_skill),
        "buffwithuntrained-creature" => int(b.buff_with_untrained_creature_skill),
        "buffwithuntrained-life" => int(b.buff_with_untrained_life_skill),
        "allowdebufffallback" => flag(c.allow_debuff_fallback),
        "whoyougonnacall" => flag(
            c.dynamic_settings
                .get(options::UNUSED_SETTING)
                .is_none_or(|kept| kept.boolean),
        ),
        "enablemeta" => flag(s.meta.enabled),
        _ => return None,
    })
}

/// The recharge plan's nested table, row by row; empty when a column is
/// missing.
///
/// # Errors
/// A row is short, or a number cell does not read.
pub fn parse_recharge_handler_set(
    table: &VtankTable,
) -> Result<Vec<RechargeHandlerRow>, FormatError> {
    let (Some(vital), Some(handler), Some(min), Some(max), Some(stance)) = (
        table.column_index("Vital"),
        table.column_index("HandlerString"),
        table.column_index("MinPercent"),
        table.column_index("MaxPercent"),
        table.column_index("Stance"),
    ) else {
        return Ok(Vec::new());
    };
    table
        .rows
        .iter()
        .map(|row| {
            Ok(RechargeHandlerRow {
                vital: cell_at(row, Some(vital))?.as_int()?,
                handler_string: cell_at(row, Some(handler))?.as_string().to_owned(),
                min_percent: cell_at(row, Some(min))?.as_int()?,
                max_percent: cell_at(row, Some(max))?.as_int()?,
                stance: cell_at(row, Some(stance))?.as_int()?,
            })
        })
        .collect()
}
