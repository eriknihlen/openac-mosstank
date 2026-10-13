//! The live settings a VTank settings profile fills: combat, buffs, vitals,
//! inventory (with looting), navigation and the meta switch. Defaults are
//! VTank's shipped ones, as the C# plugin has them.
//!
//! Choice settings are kept as the number the profile stores (a C# enum
//! holds any number, and an out-of-range one must survive a load and a
//! save), with the named values as constants.

use ac_plugin_abi::combat::CombatTarget;
use mosstank_dotnet::collections::{
    DotNetDictionary, DotNetSet, IgnoreCaseStringMap, Ordinal, OrdinalStringMap, OrdinalStringSet,
};
use mosstank_formats::gameinfo::VtankHealKit;
use mosstank_formats::utl::{LootRule, VtankSalvageCombineSettings};

pub use mosstank_formats::route::NavigationSettings;

use crate::monster::{
    self, MonsterExpressionContext, MonsterFactTable, MonsterRule, MonsterValue,
    ResolvedMonsterRule,
};
use crate::options;

macro_rules! choice {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $value:ident = $number:expr),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub i32);

        impl $name {
            $($(#[$vmeta])* pub const $value: Self = Self($number);)*
        }
    };
}

choice!(
    /// How the next monster is picked (stored as this number plus one).
    TargetSelectionMethod {
        /// By distance.
        RANGE = 0,
        /// By angle.
        ANGLE = 1,
        /// By angle near, by distance far.
        BOTH = 2,
    }
);

choice!(
    /// How much debuffing comes before the attack.
    DebuffEachFirst {
        /// One monster.
        ONE = 1,
        /// Every monster of the same priority.
        PRIORITY = 2,
        /// Every monster.
        ALL = 3,
    }
);

choice!(
    /// What a debuff's source is chosen by.
    DebuffSelectionMethod {
        /// The spell's level.
        SPELL_LEVEL = 1,
        /// The caster's skill.
        SKILL = 2,
    }
);

choice!(
    /// Whether arcs are cast instead of bolts.
    UseArcsMode {
        /// Never.
        NO = 1,
        /// Beyond the arc range.
        AT_RANGE = 2,
        /// Always.
        YES = 3,
    }
);

choice!(
    /// Which distance counts monsters for summoning a pet.
    PetRangeMode {
        /// The attack distance.
        ATTACK_DISTANCE = 0,
        /// The pet's own range.
        CUSTOM = 1,
    }
);

choice!(
    /// Where melee attacks aim (the client's attack height).
    MeleeAttackHeight {
        /// High.
        HIGH = 1,
        /// Medium.
        MEDIUM = 2,
        /// Low.
        LOW = 3,
    }
);

/// What kind of consumable an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum ConsumableCategory {
    Other,
    HealthKit,
    HealthFood,
    StaminaKit,
    StaminaFood,
    ManaKit,
    ManaFood,
    Pea,
    AllPeas,
    Lockpick,
    ManaStone,
    ManaSource,
    Grenade,
    SplitComponent,
    BuffConsumable,
}

/// An item the profile's `AssistItems` table names, with its kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssistItem {
    /// The item's name.
    pub name: String,
    /// Its kind.
    pub category: ConsumableCategory,
}

/// A gem or food the profile uses for its buff.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GemFoodItem {
    /// The item's name.
    pub name: String,
    /// The spell it casts.
    pub spell_id: u32,
}

/// One item-enchant row: an item and a spell to keep on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuffItemEnchantRow {
    /// The item's name, when authored by name.
    pub item_name: String,
    /// The spell's name, when authored by name.
    pub spell_name: String,
    /// The item's object id, when profiled by id.
    pub object_id: Option<u32>,
    /// The spell's id, when profiled by id.
    pub spell_id: Option<u32>,
}

impl BuffItemEnchantRow {
    /// An authored row that deliberately casts nothing.
    #[must_use]
    pub fn casts_nothing(&self) -> bool {
        self.spell_id == Some(u32::MAX) || (self.spell_id.is_none() && self.spell_name.is_empty())
    }

    /// A row read from the profile's `BuffedItems` table (by ids).
    #[must_use]
    pub fn is_profiled_item_row(&self) -> bool {
        self.object_id.is_some() && self.spell_id.is_some()
    }
}

/// A `BuffedItems` row, by its two ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuffedItemKey {
    /// The item.
    pub object_id: u32,
    /// The spell.
    pub spell_id: u32,
}

/// The combat settings.
#[derive(Debug, Clone)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct CombatSettings {
    pub enabled: bool,
    pub hunt_skill_excess_over_difficulty: i32,
    pub maximum_range: f64,
    /// Monsters nearer than this are not valid targets.
    pub minimum_range: f64,
    pub approach_distance: f64,
    pub idle_peace_mode: bool,
    pub stop_macro_on_death: bool,
    pub jump_out_wand_casting: bool,
    pub do_jiggle: bool,
    pub selection_method: TargetSelectionMethod,
    pub target_select_angle_range: f64,
    pub target_lock: bool,
    pub attack_height: MeleeAttackHeight,
    pub attack_power: f32,
    pub auto_attack_power: bool,
    pub use_recklessness: bool,
    pub scan_interval_seconds: f64,
    pub debuff_each_first: DebuffEachFirst,
    pub debuff_selection_method: DebuffSelectionMethod,
    pub debuff_precast_seconds: f64,
    pub switch_wands_to_debuff: bool,
    /// Stock profiles never arc; the option only decides an exact tie.
    pub use_arcs: UseArcsMode,
    pub spell_range_fudge: f64,
    pub use_breakable_turn_to: bool,
    pub use_projectile_awareness: bool,
    pub collision_projectile_radius: f64,
    pub collision_step_distance: f64,
    pub show_collision_debug: bool,
    /// How many attempts one pass may make in a tick (read only while the
    /// flight check is on).
    pub maximum_collision_checks_per_tick: i32,
    /// The cap on samples along one flight path (MossTank's own number).
    pub collision_sample_budget: i32,
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
    pub pet_range_mode: PetRangeMode,
    pub pet_custom_range: f64,
    pub pet_monster_density: i32,
    pub pet_refill_count_idle: i32,
    pub pet_refill_count_normal: i32,
    pub allow_debuff_fallback: bool,
    pub use_special_ammo: i32,
    pub auto_fellow_management: bool,
    pub blacklisted_spell_components: String,
    pub combat_item_object_ids: DotNetSet<u32, Ordinal<u32>>,
    pub combat_item_order_ids: Vec<u32>,
    pub removed_combat_item_object_ids: DotNetSet<u32, Ordinal<u32>>,
    pub removed_buffed_item_rows: Vec<BuffedItemKey>,
    pub combat_item_names: OrdinalStringSet,
    pub combat_item_order: Vec<String>,
    /// The name each listed item had when last seen in the inventory.
    pub combat_item_names_by_id: DotNetDictionary<u32, String, Ordinal<u32>>,
    pub consumable_names: OrdinalStringSet,
    /// The game database's heal kits by name (any case).
    pub heal_kits: IgnoreCaseStringMap<VtankHealKit>,
    pub consumable_categories: OrdinalStringMap<ConsumableCategory>,
    pub imported_assist_items: Vec<AssistItem>,
    pub item_use_specifiers: DotNetDictionary<u32, i32, Ordinal<u32>>,
    pub rules: Vec<MonsterRule>,
    /// Where a rule's species and maximum health come from.
    pub monster_facts: MonsterFactTable,
    pub meta_state: String,
    pub dynamic_settings: IgnoreCaseStringMap<MonsterValue>,
}

impl Default for CombatSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            hunt_skill_excess_over_difficulty: 25,
            maximum_range: 5.0,
            minimum_range: 0.0,
            approach_distance: 0.0,
            idle_peace_mode: false,
            stop_macro_on_death: true,
            jump_out_wand_casting: false,
            do_jiggle: false,
            selection_method: TargetSelectionMethod::BOTH,
            target_select_angle_range: 5.0,
            target_lock: false,
            attack_height: MeleeAttackHeight::MEDIUM,
            attack_power: 0.5,
            auto_attack_power: true,
            use_recklessness: true,
            scan_interval_seconds: 0.25,
            debuff_each_first: DebuffEachFirst::ONE,
            debuff_selection_method: DebuffSelectionMethod::SKILL,
            debuff_precast_seconds: 5.0,
            switch_wands_to_debuff: false,
            use_arcs: UseArcsMode::NO,
            spell_range_fudge: 1.0,
            use_breakable_turn_to: true,
            use_projectile_awareness: true,
            collision_projectile_radius: 0.4,
            collision_step_distance: 0.7,
            show_collision_debug: false,
            maximum_collision_checks_per_tick: 500,
            collision_sample_budget: 500,
            arc_range: 5.0,
            ring_distance: 5.0,
            minimum_ring_targets: 4,
            delete_ghost_monsters: true,
            ghost_monster_spell_attempt_count: 200,
            blacklist_monster_attempt_count: 4,
            blacklist_monster_timeout_seconds: 120.0,
            delete_ghost_monsters_by_health_tracker: true,
            ghost_delete_health_tracker_seconds: 30.0,
            summon_pets: true,
            pet_range_mode: PetRangeMode::ATTACK_DISTANCE,
            pet_custom_range: 5.0,
            pet_monster_density: 1,
            pet_refill_count_idle: 3,
            pet_refill_count_normal: 1,
            allow_debuff_fallback: false,
            use_special_ammo: 0,
            auto_fellow_management: true,
            blacklisted_spell_components: String::new(),
            combat_item_object_ids: DotNetSet::new(),
            combat_item_order_ids: Vec::new(),
            removed_combat_item_object_ids: DotNetSet::new(),
            removed_buffed_item_rows: Vec::new(),
            combat_item_names: OrdinalStringSet::new(),
            combat_item_order: Vec::new(),
            combat_item_names_by_id: DotNetDictionary::new(),
            consumable_names: OrdinalStringSet::new(),
            heal_kits: IgnoreCaseStringMap::new(),
            consumable_categories: OrdinalStringMap::new(),
            imported_assist_items: Vec::new(),
            item_use_specifiers: DotNetDictionary::new(),
            rules: vec![MonsterRule::fresh_default()],
            monster_facts: MonsterFactTable::default(),
            meta_state: "Default".to_owned(),
            dynamic_settings: IgnoreCaseStringMap::new(),
        }
    }
}

impl CombatSettings {
    /// Which rule this monster matches. Its species and maximum health come
    /// from the fact table, not the live object.
    #[must_use]
    pub fn resolve_rule(&self, target: &CombatTarget) -> ResolvedMonsterRule {
        let species = self.monster_facts.species(&target.name);
        let setting = |name: &str| self.dynamic_settings.get(name).cloned();
        let context = MonsterExpressionContext {
            name: &target.name,
            type_id: target.weenie_class_id,
            species: &species,
            maximum_health: self.monster_facts.maximum_health(&target.name),
            range: target.distance,
            has_shield: target.has_shield,
            meta_state: &self.meta_state,
            setting: Some(&setting),
        };
        monster::resolve(&self.rules, &context)
    }
}

/// The buff settings. Defaults follow VTank's.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct BuffSettings {
    pub enabled: bool,
    pub idle_buff_topoff: bool,
    pub idle_buff_topoff_seconds: f64,
    /// Buffs are recast when under five minutes remain.
    pub rebuff_when_under_seconds: f64,
    pub buff_cast_recast_seconds: f64,
    pub buff_cast_recast_reset_seconds: f64,
    pub fast_cast_buffs: bool,
    pub random_helper_buffs: bool,
    pub random_helper_interval_seconds: f64,
    pub blacklisted_spell_components: String,
    pub skill_excess_over_difficulty: i32,
    pub buff_attributes: bool,
    pub buff_protections: bool,
    pub protection_elements: String,
    pub protection_profile_mode: i32,
    pub buff_auras: bool,
    pub buff_banes: bool,
    pub bane_elements: String,
    pub bane_profile_mode: i32,
    pub buff_regeneration: bool,
    pub buff_other: bool,
    pub buff_trained_skills_only: bool,
    pub buff_with_untrained_item_skill: i32,
    pub buff_with_untrained_creature_skill: i32,
    pub buff_with_untrained_life_skill: i32,
    pub extra_buff_spell_names: OrdinalStringSet,
    pub blacklisted_buff_family_names: OrdinalStringSet,
    pub extra_buff_spell_ids: Vec<u32>,
    pub anti_extra_buff_spell_ids: Vec<u32>,
    pub item_enchant_rows: Vec<BuffItemEnchantRow>,
    pub gem_food_items: Vec<GemFoodItem>,
}

impl Default for BuffSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_buff_topoff: false,
            idle_buff_topoff_seconds: 1200.0,
            rebuff_when_under_seconds: 300.0,
            buff_cast_recast_seconds: 30.0,
            buff_cast_recast_reset_seconds: 30.0,
            fast_cast_buffs: false,
            random_helper_buffs: false,
            random_helper_interval_seconds: 5.0,
            blacklisted_spell_components: String::new(),
            skill_excess_over_difficulty: 5,
            buff_attributes: true,
            buff_protections: true,
            protection_elements: "ALFCBPS".to_owned(),
            protection_profile_mode: 2,
            buff_auras: true,
            buff_banes: true,
            bane_elements: "ALFCBPS".to_owned(),
            bane_profile_mode: 2,
            buff_regeneration: true,
            buff_other: false,
            buff_trained_skills_only: true,
            buff_with_untrained_item_skill: 80,
            buff_with_untrained_creature_skill: 80,
            buff_with_untrained_life_skill: 80,
            extra_buff_spell_names: OrdinalStringSet::new(),
            blacklisted_buff_family_names: OrdinalStringSet::new(),
            extra_buff_spell_ids: Vec::new(),
            anti_extra_buff_spell_ids: Vec::new(),
            item_enchant_rows: Vec::new(),
            gem_food_items: Vec::new(),
        }
    }
}

/// A vital, by the number the client keys it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum VitalKind {
    Health = 2,
    Stamina = 4,
    Mana = 6,
}

/// One step of the recharge plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RechargeHandlerRow {
    /// 1 health, 2 stamina, 3 mana.
    pub vital: i32,
    /// The way of restoring it ("Kit Recharge", ...).
    pub handler_string: String,
    /// The band's bottom, percent.
    pub min_percent: i32,
    /// The band's top, percent.
    pub max_percent: i32,
    /// 1 magic mode, 2 any other combat mode.
    pub stance: i32,
}

impl RechargeHandlerRow {
    /// A row.
    #[must_use]
    pub fn new(vital: i32, handler: &str, min_percent: i32, max_percent: i32, stance: i32) -> Self {
        Self {
            vital,
            handler_string: handler.to_owned(),
            min_percent,
            max_percent,
            stance,
        }
    }

    /// The vital a row's number names.
    #[must_use]
    pub fn to_vital_kind(vital: i32) -> Option<VitalKind> {
        match vital {
            1 => Some(VitalKind::Health),
            2 => Some(VitalKind::Stamina),
            3 => Some(VitalKind::Mana),
            _ => None,
        }
    }
}

/// VTank's nine `Recharge-*` thresholds (normalised 0..=1 here; the profile
/// stores percentages) and the recharge options.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct VitalSettings {
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
    pub helper_health_distance: f64,
    pub helper_stamina_distance: f64,
    pub helper_mana_distance: f64,
    pub help_others: bool,
    /// Also help the peer network's characters outside the fellowship (a
    /// MossTank extension, off by default).
    pub help_network_peers: bool,
    pub use_healers_heart: bool,
    pub recharge_boost_time_seconds: f64,
    pub recharge_boost_amount: i32,
    pub clear_level_boost_flag_on_cast: bool,
    pub drop_to_peace_mode_retry_count: i32,
    pub recharge_handler_set: String,
    pub recharge_handler_rows: Vec<RechargeHandlerRow>,
    pub use_kits_in_magic_mode: bool,
    pub go_to_peace_mode_to_use_kits: bool,
    pub minimum_heal_kit_success_chance: i32,
    pub stamina_to_health_multiplier: f64,
    pub mana_to_health_multiplier: f64,
    pub cast_dispel_self: bool,
    pub use_dispel_items: bool,
    pub use_dispel_drum: bool,
}

impl Default for VitalSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            normal_health: 0.75,
            normal_stamina: 0.50,
            normal_mana: 0.50,
            no_target_health: 0.01,
            no_target_stamina: 0.01,
            no_target_mana: 0.01,
            helper_health: 0.20,
            helper_stamina: 0.01,
            helper_mana: 0.01,
            helper_health_distance: 59.6,
            helper_stamina_distance: 59.6,
            helper_mana_distance: 32.0,
            help_others: true,
            help_network_peers: false,
            use_healers_heart: true,
            recharge_boost_time_seconds: 5.0,
            recharge_boost_amount: 40,
            clear_level_boost_flag_on_cast: true,
            drop_to_peace_mode_retry_count: 34,
            recharge_handler_set: "RechargeHandlerSet".to_owned(),
            recharge_handler_rows: options::default_recharge_handlers(),
            use_kits_in_magic_mode: true,
            go_to_peace_mode_to_use_kits: false,
            minimum_heal_kit_success_chance: 95,
            stamina_to_health_multiplier: 1.9,
            mana_to_health_multiplier: 2.8,
            cast_dispel_self: false,
            use_dispel_items: false,
            use_dispel_drum: false,
        }
    }
}

impl VitalSettings {
    /// The threshold of one of the two self-recharge rows: the normal one,
    /// or the one for when there is nothing to fight. The two are never
    /// merged; their row order is what gives the second its meaning.
    #[must_use]
    pub fn threshold(&self, vital: VitalKind, no_target: bool) -> f64 {
        match (vital, no_target) {
            (VitalKind::Health, false) => self.normal_health,
            (VitalKind::Health, true) => self.no_target_health,
            (VitalKind::Stamina, false) => self.normal_stamina,
            (VitalKind::Stamina, true) => self.no_target_stamina,
            (VitalKind::Mana, false) => self.normal_mana,
            (VitalKind::Mana, true) => self.no_target_mana,
        }
    }

    /// The normal threshold.
    #[must_use]
    pub fn normal_threshold(&self, vital: VitalKind) -> f64 {
        self.threshold(vital, false)
    }
}

/// The looting settings (the loot profile's rules travel with them).
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct LootSettings {
    pub profile_active: bool,
    pub enabled: bool,
    pub external_classifier_id: String,
    pub priority_boost: bool,
    pub loot_all_corpses: bool,
    pub loot_fellow_corpses: bool,
    pub loot_only_rare_corpses: bool,
    /// Walk to this character's own rare corpse past the approach range (a
    /// MossTank extension, off by default).
    pub walk_to_own_rare_corpses: bool,
    pub read_unknown_scrolls: bool,
    pub combine_salvage: bool,
    pub mana_stone_loot_count: i32,
    pub mana_tank_minimum_mana: i32,
    pub corpse_approach_range: f64,
    pub corpse_minimum_approach_range: f64,
    pub corpse_open_timeout_seconds: f64,
    pub corpse_item_appearance_timeout_seconds: f64,
    pub corpse_item_identify_timeout_seconds: f64,
    pub blacklist_corpse_open_attempt_count: i32,
    pub blacklist_corpse_open_timeout_seconds: f64,
    pub corpse_cache_timeout_minutes: f64,
    pub corpse_loot_item_max_attempts: i32,
    pub scan_interval_seconds: f64,
    pub rules: Vec<LootRule>,
    pub salvage_combine: VtankSalvageCombineSettings,
}

impl Default for LootSettings {
    fn default() -> Self {
        Self {
            profile_active: true,
            enabled: false,
            external_classifier_id: String::new(),
            priority_boost: false,
            loot_all_corpses: false,
            loot_fellow_corpses: false,
            loot_only_rare_corpses: false,
            walk_to_own_rare_corpses: false,
            read_unknown_scrolls: true,
            combine_salvage: true,
            mana_stone_loot_count: 4,
            mana_tank_minimum_mana: 1000,
            corpse_approach_range: 40.0,
            corpse_minimum_approach_range: 3.36,
            corpse_open_timeout_seconds: 1.5,
            corpse_item_appearance_timeout_seconds: 6.0,
            corpse_item_identify_timeout_seconds: 60.0,
            blacklist_corpse_open_attempt_count: 30,
            blacklist_corpse_open_timeout_seconds: 200.0,
            corpse_cache_timeout_minutes: 60.0,
            corpse_loot_item_max_attempts: 20,
            scan_interval_seconds: 0.25,
            rules: Vec::new(),
            salvage_combine: VtankSalvageCombineSettings::default(),
        }
    }
}

impl LootSettings {
    /// Whether a kill holds the route still for the looting window: when
    /// looting is on and not only rare corpses are looted.
    #[must_use]
    pub fn holds_route_after_kill(&self) -> bool {
        self.enabled && !self.loot_only_rare_corpses
    }
}

/// The inventory settings.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct InventorySettings {
    pub mana_charges_when_off: bool,
    pub auto_stack: bool,
    pub auto_cram: bool,
    pub auto_craft_items: bool,
    pub arrowhead_fletch_difficulty_excess: i32,
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
    /// How far off, flat along the ground, a character may stand and still
    /// be given items.
    pub give_range_meters: f64,
    /// The wait between one hand-over and the next.
    pub give_delay_seconds: f64,
    /// How many times one item is offered again before it is written off.
    pub give_busy_retry_limit: i32,
    /// How many written-off items stop the whole hand-over.
    pub give_failure_limit: i32,
    pub loot: LootSettings,
}

impl Default for InventorySettings {
    fn default() -> Self {
        Self {
            mana_charges_when_off: true,
            auto_stack: true,
            auto_cram: false,
            auto_craft_items: true,
            arrowhead_fletch_difficulty_excess: 10,
            split_peas: true,
            critical_component_minimum: 4,
            normal_component_minimum: 20,
            idle_component_minimum: 20,
            idle_health_kit_count: 2,
            idle_stamina_kit_count: 2,
            idle_mana_kit_count: 2,
            idle_health_food_count: 15,
            idle_stamina_food_count: 15,
            idle_mana_food_count: 15,
            refill_worn_mana: true,
            refill_worn_mana_percent: 33,
            scan_interval_seconds: 0.25,
            give_range_meters: 15.0,
            give_delay_seconds: 0.0,
            give_busy_retry_limit: 10,
            give_failure_limit: 3,
            loot: LootSettings::default(),
        }
    }
}

/// The meta switch (the engine asks it every tick, so it lives here).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaSettings {
    /// Whether the meta runs.
    pub enabled: bool,
}

/// Every settings group a settings profile fills.
#[derive(Debug, Clone, Default)]
pub struct AllSettings {
    #[allow(missing_docs)]
    pub combat: CombatSettings,
    #[allow(missing_docs)]
    pub buffs: BuffSettings,
    #[allow(missing_docs)]
    pub vitals: VitalSettings,
    #[allow(missing_docs)]
    pub inventory: InventorySettings,
    #[allow(missing_docs)]
    pub navigation: NavigationSettings,
    #[allow(missing_docs)]
    pub meta: MetaSettings,
    /// The character's object id (0 when not known); its own id is not a
    /// profiled item.
    pub player_object_id: u32,
}
