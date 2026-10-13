//! Restoring vitals: the planner and the controller.
#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, CastGate, SpellInfo};
use ac_plugin_abi::peers::NetworkClient;
use ac_plugin_abi::social::{FellowMember, FellowshipStatus};
use ac_plugin_abi::world::{ItemProperties, Position, WorldObject};
use mosstank_automation::cast_tracker::{CastRequest, SpellCastTracker};
use mosstank_automation::combat_text::log_text_type;
use mosstank_automation::locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_buffs::recharge::{
    self, FELLOW_VITALS_TRUST_SECONDS, VitalRechargeChoice, VitalRechargeMethod,
    VitalRechargeSourceKind,
};
use mosstank_buffs::recharge_controller::{VitalRechargeController, VitalTurn};
use mosstank_buffs::vitals::{self, Vitals};
use mosstank_data::spell_id;
use mosstank_formats::gameinfo::VtankHealKit;
use mosstank_formats::usd::{VtankCell, VtankRow};
use mosstank_settings::model::{
    AllSettings, CombatSettings, ConsumableCategory, VitalKind, VitalSettings,
};

/// The test file's client (`VitalRechargeTests.Surface`): every owned item
/// is in the object table and identified unless listed as unassessed; a
/// cast or a gate at an object is refused, as the C# interface's defaults.
struct Surface {
    peers: Vec<NetworkClient>,
    current_health: u32,
    max_health: u32,
    current_stamina: u32,
    max_stamina: u32,
    current_mana: u32,
    max_mana: u32,
    skills: Vec<Skill>,
    spells: Vec<SpellInfo>,
    lookup: Vec<SpellInfo>,
    items: Vec<InventoryItem>,
    unassessed: HashSet<u32>,
    items_busy: bool,
    in_fellowship: bool,
    fellows: Vec<FellowMember>,
    mode: CombatMode,
    last_cast_completion: CastCompletion,
    used_item_ids: Vec<u32>,
    last_item_completion: ItemUseCompletion,
    vitals_requests: Vec<bool>,
    cast_spell_ids: Vec<u32>,
    infos: Vec<String>,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            peers: Vec::new(),
            current_health: 100,
            max_health: 100,
            current_stamina: 100,
            max_stamina: 100,
            current_mana: 100,
            max_mana: 100,
            skills: Vec::new(),
            spells: Vec::new(),
            lookup: Vec::new(),
            items: Vec::new(),
            unassessed: HashSet::new(),
            items_busy: false,
            in_fellowship: false,
            fellows: Vec::new(),
            mode: CombatMode::Peace,
            last_cast_completion: CastCompletion::default(),
            used_item_ids: Vec::new(),
            last_item_completion: ItemUseCompletion::default(),
            vitals_requests: Vec::new(),
            cast_spell_ids: Vec::new(),
            infos: Vec::new(),
        }
    }
}

impl SpellCatalog for Surface {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        self.spells.clone()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.spells
            .iter()
            .chain(&self.lookup)
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
    }
}

impl Automation for Surface {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            health: (self.current_health, self.max_health, 0),
            stamina: (self.current_stamina, self.max_stamina, 0),
            mana: (self.current_mana, self.max_mana, 0),
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.skills.clone()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, target: Option<u32>) -> CastGate {
        if target.is_some() {
            CastGate::Refused
        } else {
            CastGate::Ready
        }
    }

    fn cast(&mut self, spell_id: u32, target: Option<u32>) -> bool {
        if target.is_some() {
            return false;
        }
        self.cast_spell_ids.push(spell_id);
        true
    }

    fn last_cast_completion(&mut self) -> CastCompletion {
        self.last_cast_completion.clone()
    }

    fn combat_mode(&mut self) -> CombatMode {
        self.mode
    }

    fn enter_combat_mode(&mut self, _mode: CombatMode) -> CombatResult {
        CombatResult {
            status: CombatStatus::Unavailable,
            notice: None,
        }
    }

    fn items_available(&mut self) -> bool {
        true
    }

    fn items_busy(&mut self) -> bool {
        self.items_busy
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.items.clone()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.last_item_completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.used_item_ids.push(object_id);
        ItemResult {
            status: ItemStatus::Started,
            notice: None,
        }
    }

    fn apply_item(&mut self, object_id: u32, _target_id: u32) -> ItemResult {
        self.used_item_ids.push(object_id);
        ItemResult {
            status: ItemStatus::Started,
            notice: None,
        }
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.items
            .iter()
            .any(|item| item.object_id == object_id)
            .then(ItemProperties::default)
    }

    fn objects_available(&mut self) -> bool {
        true
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        let item = self.items.iter().find(|item| item.object_id == object_id)?;
        Some(WorldObject {
            object_id: item.object_id,
            weenie_class_id: item.weenie_class_id,
            name: item.name.clone(),
            item_type: item.item_type,
            container_id: item.container_id,
            wielder_id: item.wielder_id,
            last_id_time: i32::from(!self.unassessed.contains(&item.object_id)),
            ..WorldObject::default()
        })
    }

    fn in_fellowship(&mut self) -> bool {
        self.in_fellowship
    }

    fn fellowship_members(&mut self) -> Vec<FellowMember> {
        self.fellows.clone()
    }

    fn request_fellow_vitals(&mut self, requested: bool) -> FellowshipStatus {
        self.vitals_requests.push(requested);
        FellowshipStatus::Accepted
    }

    fn network_available(&mut self) -> bool {
        true
    }

    fn network_clients(&mut self) -> Vec<NetworkClient> {
        self.peers.clone()
    }

    fn log_info(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }

    fn log_warn(&mut self, _text: &str) {}
}

fn skill(id: u32, current: u32) -> Skill {
    Skill {
        skill_id: id,
        training: Training::Trained,
        current,
        ..Skill::default()
    }
}

fn spell(id: u32, name: &str, family: u32, quality: i32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family,
        tier: 7,
        difficulty: quality,
        mana_cost: 10,
        school: 33,
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

fn item(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: 1,
        name: name.to_owned(),
        item_type: 0x80,
        container_id: 1,
        stack_size: 1,
        structure: 1,
        maximum_structure: 1,
        use_requires_skill: 21,
        ..InventoryItem::default()
    }
}

fn kit(id: u32, name: &str, booster: i32) -> InventoryItem {
    InventoryItem {
        booster_vital: booster,
        boost_value: 0,
        heal_kit_modifier: 1.2,
        public_flags: 0x0001_0000,
        ..item(id, name)
    }
}

/// Food needs no Healing skill, which is what tells it from a kit.
fn food(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        booster_vital: VitalKind::Health as i32,
        boost_value: 20,
        use_requires_skill: 0,
        ..item(id, name)
    }
}

fn caster(id: u32, name: &str, spell_id: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: 1,
        name: name.to_owned(),
        item_type: 0x8000,
        container_id: 1,
        stack_size: 1,
        spell_id,
        ..InventoryItem::default()
    }
}

fn fellow(id: u32, name: &str, health: u32, distance: f32) -> FellowMember {
    FellowMember {
        object_id: id,
        name: name.to_owned(),
        current_health: health,
        max_health: 100,
        current_stamina: 100,
        max_stamina: 100,
        current_mana: 100,
        max_mana: 100,
        distance,
        vitals_age_seconds: Some(0.0),
        ..FellowMember::default()
    }
}

fn peer(player_id: u32, name: &str, health: u32, east_west: f64) -> NetworkClient {
    NetworkClient {
        client_id: player_id + 1000,
        player_id,
        name: name.to_owned(),
        position: Position {
            east_west,
            is_outdoor: true,
            ..Position::default()
        },
        current_health: health,
        current_mana: 100,
        current_stamina: 100,
        max_health: 100,
        max_mana: 100,
        max_stamina: 100,
        ..NetworkClient::default()
    }
}

fn healers_heart_surface() -> Surface {
    Surface {
        mode: CombatMode::Magic,
        spells: vec![spell(300, "Adja's Grace", 900, 100)],
        lookup: vec![spell(spell_id::AdjaSGift, "Adja's Gift", 900, 100)],
        in_fellowship: true,
        skills: vec![skill(33, 400), skill(14, 400)],
        items: vec![item(30, "The Healer's Heart")],
        fellows: vec![fellow(71, "Hurt", 5, 10.0)],
        ..Surface::default()
    }
}

fn combat_with_consumable(name: &str) -> CombatSettings {
    let mut combat = CombatSettings::default();
    combat.consumable_names.insert(name.to_owned());
    combat
}

fn combat_with_caster(name: &str) -> CombatSettings {
    let mut combat = CombatSettings::default();
    combat.combat_item_names.insert(name.to_owned());
    combat
}

fn plan(
    vital: VitalKind,
    surface: &mut Surface,
    settings: &VitalSettings,
    combat: &CombatSettings,
) -> Option<VitalRechargeChoice> {
    recharge::try_plan(vital, surface, settings, combat, None)
}

fn plan_helper(
    surface: &mut Surface,
    settings: &VitalSettings,
    combat: &CombatSettings,
) -> Option<VitalRechargeChoice> {
    recharge::try_plan_helper(surface, settings, combat, None)
}

fn tick(
    controller: &mut VitalRechargeController,
    surface: &mut Surface,
    settings: &VitalSettings,
    combat: &CombatSettings,
    elapsed: f64,
    enabled: bool,
    no_target: bool,
    helpers: bool,
) -> bool {
    let mut turn = VitalTurn {
        automation: surface,
        settings,
        combat,
    };
    controller.tick(&mut turn, elapsed, enabled, no_target, helpers)
}

/// A self-recharge turn: enabled, a target in sight, the self rows.
fn self_tick(
    controller: &mut VitalRechargeController,
    surface: &mut Surface,
    settings: &VitalSettings,
    combat: &CombatSettings,
) -> bool {
    tick(
        controller, surface, settings, combat, 0.3, true, false, false,
    )
}

fn observe(
    controller: &mut VitalRechargeController,
    surface: &mut Surface,
    settings: &VitalSettings,
    combat: &CombatSettings,
    elapsed: f64,
) {
    let mut turn = VitalTurn {
        automation: surface,
        settings,
        combat,
    };
    controller.observe_pending_receipt(&mut turn, elapsed);
}

fn with_locks() -> (VitalRechargeController, SharedActionLocks) {
    let mut controller = VitalRechargeController::new();
    let locks = ActionLockTable::shared();
    controller.bind_action_locks(locks.clone());
    (controller, locks)
}

fn receipt(revision: i64, source_id: u32) -> ItemUseCompletion {
    ItemUseCompletion {
        revision,
        source_id,
        ..ItemUseCompletion::default()
    }
}

fn vitals_of(surface: &mut Surface) -> Vitals {
    Vitals::from(&surface.character())
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: RechargeCannotReplaceAnotherOwnersTrackedCast
#[test]
fn recharge_cannot_replace_another_owners_tracked_cast() {
    let mut surface = Surface {
        current_health: 50,
        mode: CombatMode::Magic,
        skills: vec![skill(33, 400)],
        spells: vec![spell(100, "Heal Self VII", 1, 300)],
        ..Surface::default()
    };
    let tracker = Rc::new(RefCell::new(SpellCastTracker::new()));
    tracker.borrow_mut().begin(&CastRequest::new(
        200,
        "Other spell",
        99,
        "Other target",
        false,
    ));
    let mut controller = VitalRechargeController::new();
    controller.bind_cast_tracker(tracker.clone());
    self_tick(
        &mut controller,
        &mut surface,
        &VitalSettings::default(),
        &CombatSettings::default(),
    );
    assert!(surface.cast_spell_ids.is_empty());
    assert_eq!(tracker.borrow().spell_id(), 200);
    assert!(!controller.cast_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: StaminaKitStillHonorsConfiguredMinimumSuccessChance
#[test]
fn stamina_kit_still_honors_configured_minimum_success_chance() {
    for (stamina, expected) in [(80, true), (10, false)] {
        let mut surface = Surface {
            current_stamina: stamina,
            max_stamina: 100,
            skills: vec![skill(21, 200)],
            items: vec![kit(10, "Greater Stamina Kit", VitalKind::Stamina as i32)],
            ..Surface::default()
        };
        let mut combat = combat_with_consumable("Greater Stamina Kit");
        combat.consumable_categories.insert(
            "Greater Stamina Kit".to_owned(),
            ConsumableCategory::StaminaKit,
        );
        assert_eq!(
            plan(
                VitalKind::Stamina,
                &mut surface,
                &VitalSettings::default(),
                &combat
            )
            .is_some(),
            expected,
            "stamina {stamina}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: RechargeUsesSpellResultEvenWhenUseReceiptCannotIdentifyTheCast
#[test]
fn recharge_uses_spell_result_even_when_use_receipt_cannot_identify_the_cast() {
    let mut heal = spell(100, "Heal Self VII", 1, 300);
    heal.saying = "test words".to_owned();
    let mut surface = Surface {
        current_health: 50,
        mode: CombatMode::Magic,
        skills: vec![skill(33, 400)],
        spells: vec![heal],
        ..Surface::default()
    };
    let settings = VitalSettings::default();
    let combat = CombatSettings::default();
    let tracker = Rc::new(RefCell::new(SpellCastTracker::new()));
    let mut controller = VitalRechargeController::new();
    controller.bind_cast_tracker(tracker.clone());
    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert!(controller.holds_pass());
    tracker
        .borrow_mut()
        .observe_chat(1, "test words", true, log_text_type::SPELLCASTING);
    tracker.borrow_mut().observe_chat(
        2,
        "You cast Heal Other VII on Stranger",
        false,
        log_text_type::MAGIC,
    );
    observe(&mut controller, &mut surface, &settings, &combat, 0.1);
    assert!(controller.holds_pass());
    tracker.borrow_mut().observe_chat(
        3,
        "You cast Heal Self VII on yourself",
        false,
        log_text_type::MAGIC,
    );
    observe(&mut controller, &mut surface, &settings, &combat, 0.1);
    assert!(!controller.holds_pass());
    assert_eq!(surface.last_cast_completion, CastCompletion::default());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AuthenticDefaultsAreExactNineThresholds
#[test]
fn authentic_defaults_are_exact_nine_thresholds() {
    let settings = VitalSettings::default();
    assert_eq!(settings.normal_health, 0.75);
    assert_eq!(settings.normal_stamina, 0.50);
    assert_eq!(settings.normal_mana, 0.50);
    assert_eq!(settings.no_target_health, 0.01);
    assert_eq!(settings.no_target_stamina, 0.01);
    assert_eq!(settings.no_target_mana, 0.01);
    assert_eq!(settings.helper_health, 0.20);
    assert_eq!(settings.helper_stamina, 0.01);
    assert_eq!(settings.helper_mana, 0.01);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: NoTargetTopoffExtendsRatherThanReplacesCombatThreshold
#[test]
fn no_target_topoff_extends_rather_than_replaces_combat_threshold() {
    let settings = VitalSettings {
        no_target_health: 0.90,
        ..VitalSettings::default()
    };
    let mut surface = Surface {
        current_health: 80,
        ..Surface::default()
    };
    let vitals = vitals_of(&mut surface);
    assert_eq!(
        vitals::decide_need(&vitals, &settings, false, (0, 0, 0)),
        None
    );
    assert_eq!(
        vitals::decide_need(&vitals, &settings, true, (0, 0, 0)),
        Some(VitalKind::Health)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: MagicHealthHandlerOrderMatchesOfficialDefaultTable
#[test]
fn magic_health_handler_order_matches_official_default_table() {
    use VitalRechargeMethod::{Food, Kit, ManaToHealth, RegularSpell, StaminaToHealth};
    assert_eq!(
        recharge::handlers(VitalKind::Health, true, 15, None, None),
        [StaminaToHealth, ManaToHealth, RegularSpell, Food, Kit]
    );
    assert_eq!(
        recharge::handlers(VitalKind::Health, true, 16, None, None),
        [Kit, StaminaToHealth, ManaToHealth, RegularSpell, Food]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: NonMagicHealthConversionExistsOnlyInOfficialEmergencyBand
#[test]
fn non_magic_health_conversion_exists_only_in_official_emergency_band() {
    use VitalRechargeMethod::{Food, Kit, RegularSpell, StaminaToHealth};
    assert_eq!(
        recharge::handlers(VitalKind::Health, false, 10, None, None),
        [Food, Kit, StaminaToHealth, RegularSpell]
    );
    assert_eq!(
        recharge::handlers(VitalKind::Health, false, 15, None, None),
        [Food, Kit, RegularSpell]
    );
    assert_eq!(
        recharge::handlers(VitalKind::Health, false, 16, None, None),
        [Kit, Food, RegularSpell]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: RechargeHandlerSetCanOverrideOneStanceVitalBand
#[test]
fn recharge_handler_set_can_override_one_stance_vital_band() {
    use VitalRechargeMethod::{Food, Kit, RegularSpell};
    let profile = "combat-health-critical=Regular Spell > Kit Recharge; \
                   magic-mana-normal=Recharge With Food > Regular Spell";
    assert_eq!(
        recharge::handlers(VitalKind::Health, false, 5, Some(profile), None),
        [RegularSpell, Kit]
    );
    assert_eq!(
        recharge::handlers(VitalKind::Mana, true, 50, Some(profile), None),
        [Food, RegularSpell]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: RechargeBoostAdjustmentTemporarilyRaisesNeed
#[test]
fn recharge_boost_adjustment_temporarily_raises_need() {
    let mut surface = Surface {
        current_health: 80,
        ..Surface::default()
    };
    let settings = VitalSettings {
        normal_health: 0.75,
        ..VitalSettings::default()
    };
    let vitals = vitals_of(&mut surface);
    assert_eq!(
        vitals::decide_need(&vitals, &settings, false, (0, 0, 0)),
        None
    );
    assert_eq!(
        vitals::decide_need(&vitals, &settings, false, (40, 0, 0)),
        Some(VitalKind::Health)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: IncantationHealthAliasParticipatesInRegularSpellHandler
#[test]
fn incantation_health_alias_participates_in_regular_spell_handler() {
    let mut surface = Surface {
        mode: CombatMode::Magic,
        current_health: 50,
        spells: vec![spell(
            spell_id::AdjaSIntervention,
            "Adja's Intervention",
            1,
            400,
        )],
        ..Surface::default()
    };
    let choice = plan(
        VitalKind::Health,
        &mut surface,
        &VitalSettings {
            minimum_heal_kit_success_chance: 100,
            ..VitalSettings::default()
        },
        &CombatSettings::default(),
    )
    .expect("a plan");
    assert_eq!(choice.spell_id, spell_id::AdjaSIntervention);
}

fn plentiful_kit_table(combat: &mut CombatSettings) {
    combat.heal_kits.insert(
        "Plentiful Healing Kit".to_owned(),
        VtankHealKit {
            name: "Plentiful Healing Kit".to_owned(),
            restore_bonus: 1.2,
            skill_bonus: 0,
            vital: 2,
        },
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AnUnappraisedKitIsUsedByItsProfileKindAndTheTable
#[test]
fn an_unappraised_kit_is_used_by_its_profile_kind_and_the_table() {
    let mut surface = Surface {
        mode: CombatMode::Peace,
        current_health: 50,
        skills: vec![skill(21, 400)],
        items: vec![item(10, "Plentiful Healing Kit")],
        ..Surface::default()
    };
    surface.unassessed.insert(10);
    let mut combat = combat_with_consumable("Plentiful Healing Kit");
    combat.consumable_categories.insert(
        "Plentiful Healing Kit".to_owned(),
        ConsumableCategory::HealthKit,
    );
    plentiful_kit_table(&mut combat);
    let choice = plan(
        VitalKind::Health,
        &mut surface,
        &VitalSettings::default(),
        &combat,
    )
    .expect("a plan");
    assert_eq!(choice.source_kind, VitalRechargeSourceKind::Kit);
    assert_eq!(choice.item_object_id, 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AnUnappraisedFoodItemIsUsedByItsProfileKind
#[test]
fn an_unappraised_food_item_is_used_by_its_profile_kind() {
    let mut surface = Surface {
        mode: CombatMode::Peace,
        current_health: 50,
        items: vec![item(11, "Bread")],
        ..Surface::default()
    };
    surface.unassessed.insert(11);
    let mut combat = combat_with_consumable("Bread");
    combat
        .consumable_categories
        .insert("Bread".to_owned(), ConsumableCategory::HealthFood);
    let choice = plan(
        VitalKind::Health,
        &mut surface,
        &VitalSettings::default(),
        &combat,
    )
    .expect("a plan");
    assert_eq!(choice.source_kind, VitalRechargeSourceKind::Food);
    assert_eq!(choice.item_object_id, 11);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: ImportedAssistKindsReachKitAndFoodRechargeSelection
#[test]
fn imported_assist_kinds_reach_kit_and_food_recharge_selection() {
    let mut database = mosstank_settings::defaults::create();
    let table = database.find_mut("AssistItems").expect("the table");
    table.rows.push(VtankRow {
        cells: vec![
            VtankCell::string("Plentiful Healing Kit"),
            VtankCell::int(0),
        ],
    });
    table.rows.push(VtankRow {
        cells: vec![VtankCell::string("Bread"), VtankCell::int(1)],
    });
    let mut combat = CombatSettings::default();
    plentiful_kit_table(&mut combat);
    let mut all = AllSettings {
        combat,
        ..AllSettings::default()
    };
    mosstank_settings::profile::load(&database.render(), &mut all, None).expect("loads");
    let mut combat = all.combat;
    mosstank_settings::items::assist_items::apply(&mut combat);

    let mut kit_surface = Surface {
        mode: CombatMode::Peace,
        current_health: 50,
        skills: vec![skill(21, 400)],
        items: vec![item(10, "Plentiful Healing Kit")],
        ..Surface::default()
    };
    kit_surface.unassessed.insert(10);
    let kit = plan(
        VitalKind::Health,
        &mut kit_surface,
        &VitalSettings::default(),
        &combat,
    )
    .expect("a kit");
    assert_eq!(kit.source_kind, VitalRechargeSourceKind::Kit);
    assert_eq!(kit.item_object_id, 10);

    let mut food_surface = Surface {
        mode: CombatMode::Peace,
        current_health: 50,
        items: vec![item(11, "Bread")],
        ..Surface::default()
    };
    food_surface.unassessed.insert(11);
    let food = plan(
        VitalKind::Health,
        &mut food_surface,
        &VitalSettings::default(),
        &combat,
    )
    .expect("food");
    assert_eq!(food.source_kind, VitalRechargeSourceKind::Food);
    assert_eq!(food.item_object_id, 11);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: MagicModeUsesProfiledViableKitBeforeRegularHealAboveEmergencyBand
#[test]
fn magic_mode_uses_profiled_viable_kit_before_regular_heal_above_emergency_band() {
    let mut surface = Surface {
        mode: CombatMode::Magic,
        current_health: 50,
        skills: vec![skill(21, 400)],
        items: vec![kit(10, "Plentiful Healing Kit", 2)],
        spells: vec![spell(100, "Heal Self VII", 1, 300)],
        ..Surface::default()
    };
    let combat = combat_with_consumable("Plentiful Healing Kit");
    let choice = plan(
        VitalKind::Health,
        &mut surface,
        &VitalSettings::default(),
        &combat,
    )
    .expect("a plan");
    assert_eq!(choice.source_kind, VitalRechargeSourceKind::Kit);
    assert_eq!(choice.item_object_id, 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: EmergencyMagicHealthPrefersStaminaConversionBeforeKit
#[test]
fn emergency_magic_health_prefers_stamina_conversion_before_kit() {
    let mut surface = Surface {
        mode: CombatMode::Magic,
        current_health: 10,
        skills: vec![skill(21, 400), skill(33, 400)],
        items: vec![kit(10, "Plentiful Healing Kit", 2)],
        spells: vec![
            spell(101, "Stamina to Health Self VII", 5, 300),
            spell(102, "Heal Self VII", 1, 300),
        ],
        ..Surface::default()
    };
    let combat = combat_with_consumable("Plentiful Healing Kit");
    let choice = plan(
        VitalKind::Health,
        &mut surface,
        &VitalSettings::default(),
        &combat,
    )
    .expect("a plan");
    assert_eq!(choice.source_kind, VitalRechargeSourceKind::LearnedSpell);
    assert_eq!(choice.spell_id, 101);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: DirectLearnedSpellWinsFinalQualityTieAgainstCasterItem
#[test]
fn direct_learned_spell_wins_final_quality_tie_against_caster_item() {
    let learned = spell(101, "Heal Self VII", 1, 300);
    let item_spell = spell(102, "Heal Self VII", 1, 300);
    let mut surface = Surface {
        mode: CombatMode::Magic,
        current_health: 50,
        spells: vec![learned.clone()],
        lookup: vec![item_spell.clone()],
        items: vec![caster(20, "Healing Lens", item_spell.spell_id)],
        ..Surface::default()
    };
    let combat = combat_with_caster("Healing Lens");
    let choice = plan(
        VitalKind::Health,
        &mut surface,
        &VitalSettings {
            minimum_heal_kit_success_chance: 100,
            ..VitalSettings::default()
        },
        &combat,
    )
    .expect("a plan");
    assert_eq!(choice.source_kind, VitalRechargeSourceKind::LearnedSpell);
    assert_eq!(choice.spell_id, learned.spell_id);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HelperChoosesLowestInRangeFellowAndStrongestFamilySpell
#[test]
fn helper_chooses_lowest_in_range_fellow_and_strongest_family_spell() {
    let basis = spell(spell_id::AdjaSGift, "Adja's Gift", 900, 100);
    let strong = spell(300, "Adja's Grace", 900, 350);
    let mut surface = Surface {
        mode: CombatMode::Magic,
        spells: vec![strong.clone()],
        lookup: vec![basis],
        in_fellowship: true,
        fellows: vec![
            fellow(70, "Near", 19, 10.0),
            fellow(71, "Lowest", 5, 20.0),
            fellow(72, "Out of range", 1, 100.0),
        ],
        ..Surface::default()
    };
    let choice = plan_helper(
        &mut surface,
        &VitalSettings::default(),
        &CombatSettings::default(),
    )
    .expect("a helper plan");
    assert_eq!(choice.vital, VitalKind::Health);
    assert_eq!(choice.target_object_id, 71);
    assert_eq!(choice.spell_id, strong.spell_id);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HelperHelpsNonFellowNetworkPeersOnlyWhenTurnedOn
#[test]
fn helper_helps_non_fellow_network_peers_only_when_turned_on() {
    let basis = spell(spell_id::AdjaSGift, "Adja's Gift", 900, 100);
    let mut surface = Surface {
        mode: CombatMode::Magic,
        lookup: vec![basis.clone()],
        spells: vec![basis],
        in_fellowship: false,
        peers: vec![
            peer(1, "Self", 1, 0.0),
            peer(80, "Near", 15, 0.05),
            peer(81, "Lowest", 5, 0.1),
            peer(82, "Far", 1, 1.0),
        ],
        ..Surface::default()
    };
    assert!(
        plan_helper(
            &mut surface,
            &VitalSettings::default(),
            &CombatSettings::default()
        )
        .is_none()
    );
    let choice = plan_helper(
        &mut surface,
        &VitalSettings {
            help_network_peers: true,
            ..VitalSettings::default()
        },
        &CombatSettings::default(),
    )
    .expect("a helper plan");
    assert_eq!(choice.vital, VitalKind::Health);
    assert_eq!(choice.target_object_id, 81);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AFellowWhoIsAlsoAPeerIsCountedOnce
#[test]
fn a_fellow_who_is_also_a_peer_is_counted_once() {
    let mut surface = Surface {
        in_fellowship: true,
        fellows: vec![fellow(80, "Both", 50, 10.0)],
        peers: vec![peer(80, "Both", 5, 0.0)],
        ..Surface::default()
    };
    let candidates = recharge::helper_candidates(
        &mut surface,
        &VitalSettings {
            help_network_peers: true,
            ..VitalSettings::default()
        },
        true,
    );
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].current_health, 50);
}

/// Revitalize Self and Revitalize Other share family 81; only the
/// component set keeps the pick on the Other line.
fn revitalize_line(other_tiers: &[(u32, &str, i32, i32)]) -> (SpellInfo, Vec<SpellInfo>) {
    let yew = (7, 26, 39, 51);
    let willow = (7, 26, 39, 61);
    let replenish = SpellInfo {
        component_set: yew,
        is_self_targeted: false,
        ..spell(spell_id::Replenish, "Replenish", 81, 300)
    };
    let mut known = vec![SpellInfo {
        component_set: willow,
        tier: 8,
        ..spell(0x10E1, "Incantation of Revitalize Self", 81, 400)
    }];
    for &(id, name, quality, tier) in other_tiers {
        known.push(SpellInfo {
            component_set: yew,
            tier,
            is_self_targeted: false,
            ..spell(id, name, 81, quality)
        });
    }
    (replenish, known)
}

fn winded_fellowship(known: Vec<SpellInfo>, replenish: SpellInfo, life: u32) -> Surface {
    Surface {
        mode: CombatMode::Magic,
        spells: known,
        lookup: vec![replenish],
        in_fellowship: true,
        skills: vec![skill(33, life)],
        fellows: vec![FellowMember {
            current_stamina: 5,
            ..fellow(71, "Winded", 100, 10.0)
        }],
        ..Surface::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HelperSpellStaysOnTheReferencesOwnLine
#[test]
fn helper_spell_stays_on_the_references_own_line() {
    let (replenish, known) = revitalize_line(&[(0x04A4, "Revitalize Other VI", 250, 6)]);
    let mut surface = winded_fellowship(known, replenish, 500);
    let choice = plan_helper(
        &mut surface,
        &VitalSettings {
            helper_stamina: 0.5,
            ..VitalSettings::default()
        },
        &CombatSettings::default(),
    )
    .expect("a helper plan");
    assert_eq!(choice.vital, VitalKind::Stamina);
    assert_eq!(choice.spell_id, 0x04A4);
    assert_eq!(choice.target_object_id, 71);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HelperTierWalkTracesItsPickAndRejections
#[test]
fn helper_tier_walk_traces_its_pick_and_rejections() {
    let (replenish, known) = revitalize_line(&[
        (0x10E0, "Incantation of Revitalize Other", 400, 8),
        (0x04A4, "Revitalize Other VI", 250, 6),
    ]);
    let mut surface = winded_fellowship(known, replenish, 300);
    let mut trace: Vec<String> = Vec::new();
    let mut sink = |line: &str| trace.push(line.to_owned());
    let choice = recharge::try_plan_helper(
        &mut surface,
        &VitalSettings {
            helper_stamina: 0.5,
            ..VitalSettings::default()
        },
        &CombatSettings::default(),
        Some(&mut sink),
    )
    .expect("a helper plan");
    assert_eq!(choice.spell_id, 0x04A4);
    assert_eq!(trace.len(), 1, "{trace:?}");
    let line = &trace[0];
    assert!(line.starts_with("Helping: Replenish"), "{line}");
    assert!(
        line.contains("picked Revitalize Other VI (gen 6)"),
        "{line}"
    );
    assert!(
        line.contains("Incantation of Revitalize Other skill 300 < 425"),
        "{line}"
    );
    assert!(
        line.contains("Incantation of Revitalize Self comp set"),
        "{line}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HelperTrustsOnlyFreshlyStreamedVitals
#[test]
fn helper_trusts_only_freshly_streamed_vitals() {
    let mut surface = Surface {
        mode: CombatMode::Magic,
        spells: vec![spell(300, "Adja's Grace", 900, 100)],
        lookup: vec![spell(spell_id::AdjaSGift, "Adja's Gift", 900, 100)],
        in_fellowship: true,
        skills: vec![skill(33, 400)],
        fellows: vec![
            FellowMember {
                vitals_age_seconds: Some(FELLOW_VITALS_TRUST_SECONDS),
                ..fellow(70, "Stale", 5, 10.0)
            },
            FellowMember {
                vitals_age_seconds: None,
                ..fellow(71, "Never", 5, 10.0)
            },
            FellowMember {
                vitals_age_seconds: Some(2.0),
                ..fellow(72, "Fresh", 15, 10.0)
            },
        ],
        ..Surface::default()
    };
    let choice = plan_helper(
        &mut surface,
        &VitalSettings::default(),
        &CombatSettings::default(),
    )
    .expect("a helper plan");
    assert_eq!(choice.target_object_id, 72);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HelperControllerHoldsTheVitalsSubscriptionWhileHelpingOthers
#[test]
fn helper_controller_holds_the_vitals_subscription_while_helping_others() {
    let mut surface = Surface {
        in_fellowship: true,
        ..Surface::default()
    };
    let mut settings = VitalSettings {
        help_others: true,
        ..VitalSettings::default()
    };
    let combat = CombatSettings::default();
    let mut helper = VitalRechargeController::new();
    let mut own = VitalRechargeController::new();

    tick(
        &mut own,
        &mut surface,
        &settings,
        &combat,
        0.3,
        true,
        true,
        false,
    );
    assert!(surface.vitals_requests.is_empty());

    tick(
        &mut helper,
        &mut surface,
        &settings,
        &combat,
        0.3,
        true,
        true,
        true,
    );
    tick(
        &mut helper,
        &mut surface,
        &settings,
        &combat,
        0.3,
        true,
        true,
        true,
    );
    assert_eq!(surface.vitals_requests, [true]);

    settings.help_others = false;
    tick(
        &mut helper,
        &mut surface,
        &settings,
        &combat,
        0.3,
        true,
        true,
        true,
    );
    assert_eq!(surface.vitals_requests, [true, false]);

    settings.help_others = true;
    tick(
        &mut helper,
        &mut surface,
        &settings,
        &combat,
        0.3,
        true,
        true,
        true,
    );
    tick(
        &mut helper,
        &mut surface,
        &settings,
        &combat,
        0.3,
        false,
        true,
        true,
    );
    assert_eq!(surface.vitals_requests, [true, false, true, false]);
}

fn hungry() -> (Surface, CombatSettings) {
    let surface = Surface {
        current_health: 20,
        max_health: 100,
        items: vec![food(10, "Bread")],
        ..Surface::default()
    };
    (surface, combat_with_consumable("Bread"))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AnItemRechargeHoldsTheItemSlotUntilTheServerAnswers
#[test]
fn an_item_recharge_holds_the_item_slot_until_the_server_answers() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, locks) = with_locks();

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert_eq!(surface.used_item_ids, [10]);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    locks.borrow_mut().advance(1.0);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    surface.last_item_completion = receipt(1, 10);
    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AnUnansweredUseStopsClaimingThePassWhenItsWindowRunsOut
#[test]
fn an_unanswered_use_stops_claiming_the_pass_when_its_window_runs_out() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, locks) = with_locks();

    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));
    assert_eq!(surface.used_item_ids, [10]);

    // Still inside the window: the use owns the character.
    locks.borrow_mut().advance(1.0);
    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));

    // Past it, with no answer: the slot is free and so is the pass.
    locks.borrow_mut().advance(1.0);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert!(!self_tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat
    ));
    assert_eq!(surface.used_item_ids, [10]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheNoTargetRowReadsItsOwnThresholdAndNotTheNormalOne
#[test]
fn the_no_target_row_reads_its_own_threshold_and_not_the_normal_one() {
    // Half health: below the normal threshold (75%), far above the
    // no-target one (1%).
    let (mut surface, combat) = hungry();
    surface.current_health = 50;
    let settings = VitalSettings::default();
    let (mut controller, _locks) = with_locks();

    tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat,
        0.3,
        true,
        true,
        false,
    );
    assert!(surface.used_item_ids.is_empty());

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert_eq!(surface.used_item_ids, [10]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AKitOrFoodUseIsReportedInFlightUntilTheServerAnswers
#[test]
fn a_kit_or_food_use_is_reported_in_flight_until_the_server_answers() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, _locks) = with_locks();
    assert!(!controller.item_use_in_flight());

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert_eq!(surface.used_item_ids, [10]);
    assert!(controller.item_use_in_flight());

    surface.last_item_completion = receipt(1, 10);
    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert!(!controller.item_use_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AKitHoldsThePassAndFoodDoesNot
#[test]
fn a_kit_holds_the_pass_and_food_does_not() {
    let settings = VitalSettings::default();
    let (mut food_surface, food_combat) = hungry();
    let (mut food, _food_locks) = with_locks();

    self_tick(&mut food, &mut food_surface, &settings, &food_combat);
    assert_eq!(food_surface.used_item_ids, [10]);
    assert!(food.item_use_in_flight());
    assert!(!food.holds_pass());

    let mut kit_surface = Surface {
        current_health: 20,
        max_health: 100,
        skills: vec![skill(21, 400)],
        items: vec![kit(11, "Plentiful Healing Kit", 2)],
        ..Surface::default()
    };
    let kit_combat = combat_with_consumable("Plentiful Healing Kit");
    let (mut kit, _kit_locks) = with_locks();
    assert!(!kit.holds_pass());

    self_tick(&mut kit, &mut kit_surface, &settings, &kit_combat);
    assert_eq!(kit_surface.used_item_ids, [11]);
    assert!(kit.holds_pass());

    kit_surface.last_item_completion = receipt(1, 11);
    observe(&mut kit, &mut kit_surface, &settings, &kit_combat, 0.05);
    assert!(!kit.holds_pass());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheServersAnswerIsReadOnTheFrameWithoutAPass
#[test]
fn the_servers_answer_is_read_on_the_frame_without_a_pass() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, _locks) = with_locks();

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert!(controller.item_use_in_flight());

    // No turn at all from here: the pass is held, only frames run.
    surface.last_item_completion = receipt(1, 10);
    observe(&mut controller, &mut surface, &settings, &combat, 0.05);
    assert!(!controller.item_use_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AReceiptForSomebodyElsesUseDoesNotEndThisOne
#[test]
fn a_receipt_for_somebody_elses_use_does_not_end_this_one() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, _locks) = with_locks();

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert!(controller.item_use_in_flight());

    // Somebody else's item answers first.
    surface.last_item_completion = receipt(1, 777);
    observe(&mut controller, &mut surface, &settings, &combat, 0.05);
    assert!(controller.item_use_in_flight());

    // And then this one's.
    surface.last_item_completion = receipt(2, 10);
    observe(&mut controller, &mut surface, &settings, &combat, 0.05);
    assert!(!controller.item_use_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheFrameAndTheTurnDoNotBothChargeTheSameSecondsToTheTimeout
#[test]
fn the_frame_and_the_turn_do_not_both_charge_the_same_seconds_to_the_timeout() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, _locks) = with_locks();

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert!(controller.item_use_in_flight());

    // Eight seconds of frames, then the turn is handed the same eight.
    for _ in 0..80 {
        observe(&mut controller, &mut surface, &settings, &combat, 0.1);
    }
    tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat,
        8.0,
        true,
        false,
        false,
    );

    // Sixteen seconds would have been past the give-up point; eight is not.
    assert!(controller.item_use_in_flight());
    assert!(!controller.status().contains("Timed out"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: AVitalBelowThresholdWithNothingToUseStillHoldsThePass
#[test]
fn a_vital_below_threshold_with_nothing_to_use_still_holds_the_pass() {
    let mut surface = Surface {
        current_health: 20,
        max_health: 100,
        ..Surface::default()
    };
    let settings = VitalSettings::default();
    let combat = CombatSettings::default();
    let (mut controller, _locks) = with_locks();

    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));
    assert!(controller.status().contains("No Health recharge available"));
    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: ALosingTickKeepsTheItemTheServerHasYetToAnswerFor
#[test]
fn a_losing_tick_keeps_the_item_the_server_has_yet_to_answer_for() {
    let (mut surface, combat) = hungry();
    let settings = VitalSettings::default();
    let (mut controller, locks) = with_locks();

    self_tick(&mut controller, &mut surface, &settings, &combat);
    assert_eq!(surface.used_item_ids, [10]);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    // Two passes some other rule won.
    tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat,
        0.3,
        false,
        false,
        false,
    );
    tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat,
        0.3,
        false,
        false,
        false,
    );
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert_eq!(surface.used_item_ids, [10]);

    surface.last_item_completion = receipt(1, 10);
    tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat,
        0.3,
        false,
        false,
        false,
    );
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: HealKitChanceUsesAuthenticLogisticDifficultyFormula
#[test]
fn heal_kit_chance_uses_authentic_logistic_difficulty_formula() {
    let mut surface = Surface {
        current_health: 50,
        ..Surface::default()
    };
    let chance = recharge::heal_kit_chance(
        90,
        10,
        &vitals_of(&mut surface),
        VitalKind::Health,
        CombatMode::Peace,
    );
    assert!((chance - 0.5).abs() < 1e-10, "{chance}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheHealersHeartMustBeInTheItemsProfileBeforeItIsUsed
#[test]
fn the_healers_heart_must_be_in_the_items_profile_before_it_is_used() {
    let mut surface = healers_heart_surface();
    let settings = VitalSettings {
        use_healers_heart: true,
        ..VitalSettings::default()
    };
    let bare =
        plan_helper(&mut surface, &settings, &CombatSettings::default()).expect("a helper plan");
    assert_eq!(bare.source_kind, VitalRechargeSourceKind::LearnedSpell);

    let profiled = combat_with_caster("The Healer's Heart");
    let used = plan_helper(&mut surface, &settings, &profiled).expect("a helper plan");
    assert_eq!(used.source_kind, VitalRechargeSourceKind::CasterItem);
    assert_eq!(used.name, "The Healer's Heart");
    assert_eq!(used.target_object_id, 71);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheLegendarySeedOutranksTheHealersHeart
#[test]
fn the_legendary_seed_outranks_the_healers_heart() {
    let mut surface = healers_heart_surface();
    surface.items.push(item(31, "Legendary Seed of Mornings"));
    let mut profiled = combat_with_caster("The Healer's Heart");
    profiled
        .combat_item_names
        .insert("Legendary Seed of Mornings".to_owned());
    let choice = plan_helper(
        &mut surface,
        &VitalSettings {
            use_healers_heart: true,
            ..VitalSettings::default()
        },
        &profiled,
    )
    .expect("a helper plan");
    assert_eq!(choice.name, "Legendary Seed of Mornings");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheHealersHeartNeedsLifeMagic245AndArcaneLore105
#[test]
fn the_healers_heart_needs_life_magic245_and_arcane_lore105() {
    let profiled = combat_with_caster("The Healer's Heart");
    let settings = VitalSettings {
        use_healers_heart: true,
        ..VitalSettings::default()
    };

    let mut short_on_life = healers_heart_surface();
    short_on_life.skills = vec![skill(33, 244), skill(14, 400)];
    let no_life = plan_helper(&mut short_on_life, &settings, &profiled).expect("a plan");
    assert_eq!(no_life.source_kind, VitalRechargeSourceKind::LearnedSpell);

    let mut short_on_lore = healers_heart_surface();
    short_on_lore.skills = vec![skill(33, 400), skill(14, 104)];
    let no_lore = plan_helper(&mut short_on_lore, &settings, &profiled).expect("a plan");
    assert_eq!(no_lore.source_kind, VitalRechargeSourceKind::LearnedSpell);

    let mut ready = healers_heart_surface();
    let ok = plan_helper(&mut ready, &settings, &profiled).expect("a plan");
    assert_eq!(ok.source_kind, VitalRechargeSourceKind::CasterItem);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheItemSlotGatesTheHealersHeart_NotTheHostsTransactionFlag
#[test]
fn the_item_slot_gates_the_healers_heart_not_the_hosts_transaction_flag() {
    let profiled = combat_with_caster("The Healer's Heart");
    let settings = VitalSettings {
        use_healers_heart: true,
        ..VitalSettings::default()
    };

    // The host flag is up; the heart is still the right pick.
    let mut busy = healers_heart_surface();
    busy.items_busy = true;
    let choice = plan_helper(&mut busy, &settings, &profiled).expect("a plan");
    assert_eq!(choice.source_kind, VitalRechargeSourceKind::CasterItem);

    // The item slot is what holds the row off.
    let mut ready = healers_heart_surface();
    let (mut controller, locks) = with_locks();
    locks.borrow_mut().arm(ActionLockKind::ItemUse, 1.0);
    assert!(!tick(
        &mut controller,
        &mut ready,
        &settings,
        &profiled,
        0.3,
        true,
        false,
        true
    ));
    assert!(ready.used_item_ids.is_empty());
}

fn kit_in(mode: CombatMode) -> (Surface, CombatSettings) {
    let surface = Surface {
        mode,
        current_health: 50,
        skills: vec![skill(21, 400)],
        items: vec![kit(10, "Plentiful Healing Kit", 2)],
        ..Surface::default()
    };
    (surface, combat_with_consumable("Plentiful Healing Kit"))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: KitsInMagicStanceAreAProfileChoice
#[test]
fn kits_in_magic_stance_are_a_profile_choice() {
    for (allowed, plans) in [(true, true), (false, false)] {
        let (mut surface, combat) = kit_in(CombatMode::Magic);
        let choice = plan(
            VitalKind::Health,
            &mut surface,
            &VitalSettings {
                use_kits_in_magic_mode: allowed,
                ..VitalSettings::default()
            },
            &combat,
        );
        assert_eq!(choice.is_some(), plans, "allowed {allowed}");
        if let Some(choice) = choice {
            assert_eq!(choice.source_kind, VitalRechargeSourceKind::Kit);
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: KitsOutsideMagicStanceAreUnaffectedByThatChoice
#[test]
fn kits_outside_magic_stance_are_unaffected_by_that_choice() {
    for allowed in [true, false] {
        let (mut surface, combat) = kit_in(CombatMode::Melee);
        let choice = plan(
            VitalKind::Health,
            &mut surface,
            &VitalSettings {
                use_kits_in_magic_mode: allowed,
                ..VitalSettings::default()
            },
            &combat,
        )
        .expect("a plan");
        assert_eq!(choice.source_kind, VitalRechargeSourceKind::Kit);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: GoingToPeaceForAKitIsAProfileChoice
#[test]
fn going_to_peace_for_a_kit_is_a_profile_choice() {
    for (go_to_peace, required) in [(true, Some(CombatMode::Peace)), (false, None)] {
        let (mut surface, combat) = kit_in(CombatMode::Melee);
        let choice = plan(
            VitalKind::Health,
            &mut surface,
            &VitalSettings {
                go_to_peace_mode_to_use_kits: go_to_peace,
                ..VitalSettings::default()
            },
            &combat,
        )
        .expect("a plan");
        assert_eq!(choice.source_kind, VitalRechargeSourceKind::Kit);
        assert_eq!(choice.required_mode, required);
    }
}

fn boosted(spell_name: &str, clear_on_cast: bool) -> (Surface, VitalSettings) {
    let surface = Surface {
        current_health: 50,
        max_health: 100,
        skills: vec![skill(33, 400)],
        spells: vec![spell(100, spell_name, 1, 300)],
        ..Surface::default()
    };
    let settings = VitalSettings {
        normal_health: 0.75,
        recharge_boost_amount: 40,
        recharge_boost_time_seconds: 5.0,
        clear_level_boost_flag_on_cast: clear_on_cast,
        ..VitalSettings::default()
    };
    (surface, settings)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheBoostAmountKeepsANeedAliveAboveTheThreshold
#[test]
fn the_boost_amount_keeps_a_need_alive_above_the_threshold() {
    let (mut surface, settings) = boosted(
        "Heal Self VII",
        VitalSettings::default().clear_level_boost_flag_on_cast,
    );
    let combat = CombatSettings::default();
    let (mut controller, _locks) = with_locks();

    // Peace stance with only a spell to recharge with: the stance is asked
    // for, and the wait for it is what arms the boost.
    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));

    // Back above the threshold, with nothing left to use.
    surface.current_health = 80;
    surface.spells.clear();
    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));
    assert!(
        controller.status().contains("No Health"),
        "{}",
        controller.status()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheBoostWindowIsTheProfilesOwnLength
#[test]
fn the_boost_window_is_the_profiles_own_length() {
    let (mut surface, settings) = boosted(
        "Heal Self VII",
        VitalSettings::default().clear_level_boost_flag_on_cast,
    );
    let combat = CombatSettings::default();
    let (mut controller, _locks) = with_locks();

    assert!(self_tick(&mut controller, &mut surface, &settings, &combat));

    surface.current_health = 80;
    surface.spells.clear();
    assert!(!tick(
        &mut controller,
        &mut surface,
        &settings,
        &combat,
        6.0,
        true,
        false,
        false
    ));
    assert_eq!(controller.status(), "Vitals ready");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: LandingTheBoostSpellEndsTheWindowOnlyWhenTheProfileSaysSo
#[test]
fn landing_the_boost_spell_ends_the_window_only_when_the_profile_says_so() {
    for (clear_on_cast, still_busy) in [(true, false), (false, true)] {
        let (mut surface, settings) = boosted("Adja's Intervention", clear_on_cast);
        let combat = CombatSettings::default();
        let (mut controller, _locks) = with_locks();

        // Peace stance: the stance is asked for and the boost is armed.
        assert!(self_tick(&mut controller, &mut surface, &settings, &combat));

        // In stance the cast goes out, and the server answers it.
        surface.mode = CombatMode::Magic;
        assert!(self_tick(&mut controller, &mut surface, &settings, &combat));
        surface.last_cast_completion = CastCompletion {
            revision: 1,
            spell_id: 100,
            ..CastCompletion::default()
        };
        assert!(self_tick(&mut controller, &mut surface, &settings, &combat));

        // Back above the threshold with nothing left to use: only a window
        // still standing can report work.
        surface.current_health = 80;
        surface.spells.clear();
        assert_eq!(
            self_tick(&mut controller, &mut surface, &settings, &combat),
            still_busy,
            "clear on cast {clear_on_cast}"
        );
    }
}

fn helper_reach(
    spell: SpellInfo,
    basis: SpellInfo,
    fellow: FellowMember,
    settings: &VitalSettings,
) -> Option<VitalRechargeChoice> {
    let mut surface = Surface {
        mode: CombatMode::Magic,
        spells: vec![spell],
        lookup: vec![basis],
        in_fellowship: true,
        fellows: vec![fellow],
        ..Surface::default()
    };
    plan_helper(&mut surface, settings, &CombatSettings::default())
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheHelpersHealthReachIsItsOwnProfileNumber
#[test]
fn the_helpers_health_reach_is_its_own_profile_number() {
    for (reach, helps) in [(50.0, true), (30.0, false)] {
        let choice = helper_reach(
            spell(300, "Adja's Grace", 900, 350),
            spell(spell_id::AdjaSGift, "Adja's Gift", 900, 100),
            fellow(71, "Hurt", 5, 40.0),
            &VitalSettings {
                helper_health_distance: reach,
                ..VitalSettings::default()
            },
        );
        assert_eq!(choice.is_some(), helps, "reach {reach}");
        if let Some(choice) = choice {
            assert_eq!(choice.vital, VitalKind::Health);
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheHelpersStaminaReachIsItsOwnProfileNumber
#[test]
fn the_helpers_stamina_reach_is_its_own_profile_number() {
    for (reach, helps) in [(50.0, true), (30.0, false)] {
        let choice = helper_reach(
            spell(301, "Replenish Greater", 81, 350),
            spell(spell_id::Replenish, "Replenish", 81, 100),
            FellowMember {
                current_stamina: 5,
                ..fellow(71, "Winded", 100, 40.0)
            },
            &VitalSettings {
                helper_stamina: 0.5,
                helper_health_distance: 50.0,
                helper_stamina_distance: reach,
                ..VitalSettings::default()
            },
        );
        assert_eq!(choice.is_some(), helps, "reach {reach}");
        if let Some(choice) = choice {
            assert_eq!(choice.vital, VitalKind::Stamina);
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalRechargeTests.cs :: TheHelpersManaReachIsItsOwnProfileNumber
#[test]
fn the_helpers_mana_reach_is_its_own_profile_number() {
    for (reach, helps) in [(50.0, true), (30.0, false)] {
        let choice = helper_reach(
            spell(302, "Gift of Essence Greater", 950, 350),
            spell(spell_id::GiftOfEssence, "Gift of Essence", 950, 100),
            FellowMember {
                current_mana: 5,
                ..fellow(71, "Drained", 100, 40.0)
            },
            &VitalSettings {
                helper_mana: 0.5,
                helper_health_distance: 50.0,
                helper_stamina_distance: 50.0,
                helper_mana_distance: reach,
                ..VitalSettings::default()
            },
        );
        assert_eq!(choice.is_some(), helps, "reach {reach}");
        if let Some(choice) = choice {
            assert_eq!(choice.vital, VitalKind::Mana);
        }
    }
}
