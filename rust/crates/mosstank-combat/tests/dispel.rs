//! Clearing vulnerabilities.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{ActiveEnchantment, Skill, Training};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, CastGate, SpellInfo, TrackedEnchantment};
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_combat::dispel::{DispelController, DispelTurn};
use mosstank_data::spell_id;
use mosstank_settings::model::{CombatSettings, VitalSettings};

const SELF_DISPEL: u32 = spell_id::EradicateLifeMagicSelf;

/// The test file's client.
#[derive(Default)]
struct Surface {
    skills: HashMap<u32, Skill>,
    active: Vec<ActiveEnchantment>,
    spell_lookup: Vec<SpellInfo>,
    known_spell_ids: HashSet<u32>,
    inventory: Vec<InventoryItem>,
    mode: Option<CombatMode>,
    cast_completion: CastCompletion,
    item_completion: ItemUseCompletion,
    targeted_cast: (u32, u32),
    used_item: u32,
    applied_item: (u32, u32),
    members: Vec<FellowMember>,
    tracked_by_target: HashMap<u32, Vec<TrackedEnchantment>>,
}

impl SpellCatalog for Surface {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.spell_lookup
            .iter()
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
    }

    fn is_known(&mut self, spell_id: u32) -> bool {
        self.known_spell_ids.contains(&spell_id)
    }
}

impl Automation for Surface {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            health: (100, 100, 100),
            stamina: (100, 100, 100),
            mana: (100, 100, 100),
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.skills.values().cloned().collect()
    }

    fn try_get_skill(&mut self, skill_id: u32) -> Option<Skill> {
        self.skills.get(&skill_id).cloned()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::Ready
    }

    fn cast(&mut self, spell_id: u32, target: Option<u32>) -> bool {
        match target {
            Some(target) => {
                self.targeted_cast = (spell_id, target);
                true
            }
            None => false,
        }
    }

    fn last_cast_completion(&mut self) -> CastCompletion {
        self.cast_completion.clone()
    }

    fn combat_mode(&mut self) -> CombatMode {
        self.mode.unwrap_or(CombatMode::Unknown)
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.mode = Some(mode);
        CombatResult::of(CombatStatus::ModeChangeSent)
    }

    fn items_available(&mut self) -> bool {
        true
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.inventory.clone()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.item_completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.used_item = object_id;
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.applied_item = (object_id, target_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }

    fn in_fellowship(&mut self) -> bool {
        !self.members.is_empty()
    }

    fn fellowship_members(&mut self) -> Vec<FellowMember> {
        self.members.clone()
    }

    fn active_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.active.clone()
    }

    fn tracked_enchantments(&mut self, target_id: u32) -> Vec<TrackedEnchantment> {
        self.tracked_by_target
            .get(&target_id)
            .cloned()
            .unwrap_or_default()
    }

    fn post_system_message(&mut self, _text: &str) {}

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}

fn vulnerability(id: u32, difficulty: i32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: "Fire Vulnerability Other VII".to_owned(),
        family: 7,
        tier: 7,
        difficulty,
        duration_seconds: 300.0,
        school: 32,
        is_debuff: true,
        is_offensive: true,
        ..SpellInfo::default()
    }
}

fn self_dispel_spell() -> SpellInfo {
    SpellInfo {
        spell_id: SELF_DISPEL,
        name: "Eradicate Life Magic Self".to_owned(),
        family: 8,
        tier: 7,
        difficulty: 400,
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
        name: name.to_owned(),
        item_type: 0x80,
        container_id: 1,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn vulnerable() -> Vec<ActiveEnchantment> {
    vec![ActiveEnchantment {
        spell_id: 100,
        family: 7,
        tier: 7,
        seconds_remaining: 120.0,
    }]
}

fn tick(
    controller: &mut DispelController,
    surface: &mut Surface,
    settings: &VitalSettings,
    elapsed: f64,
) -> bool {
    let combat = CombatSettings::default();
    tick_with(controller, surface, settings, &combat, elapsed)
}

fn tick_with(
    controller: &mut DispelController,
    surface: &mut Surface,
    settings: &VitalSettings,
    combat: &CombatSettings,
    elapsed: f64,
) -> bool {
    let mut turn = DispelTurn {
        automation: surface,
        settings,
        combat,
    };
    controller.tick(&mut turn, elapsed, true)
}

fn completion(revision: i64, spell_id: u32, target_id: u32) -> CastCompletion {
    CastCompletion {
        revision,
        spell_id,
        target_id,
        weenie_error: 0,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DispelControllerTests.cs :: LearnedSelfDispelWithChorizitePrecedesDispelItems
#[test]
fn learned_self_dispel_with_chorizite_precedes_dispel_items() {
    let mut surface = Surface {
        active: vulnerable(),
        spell_lookup: vec![vulnerability(100, 400), self_dispel_spell()],
        known_spell_ids: HashSet::from([SELF_DISPEL]),
        inventory: vec![item(10, "Chorizite"), item(11, "Rune of Dispel")],
        mode: Some(CombatMode::Magic),
        ..Surface::default()
    };
    let settings = VitalSettings {
        cast_dispel_self: true,
        use_dispel_items: true,
        ..VitalSettings::default()
    };
    let mut controller = DispelController::new();

    assert!(tick(&mut controller, &mut surface, &settings, 0.0));
    assert_eq!(surface.targeted_cast, (SELF_DISPEL, 1));
    assert_eq!(surface.used_item, 0);

    assert!(tick(&mut controller, &mut surface, &settings, 1.0));
    assert_eq!(controller.status(), "Casting Eradicate Life Magic Self");
    surface.cast_completion = completion(1, SELF_DISPEL, 1);
    assert!(tick(&mut controller, &mut surface, &settings, 0.05));
    assert_eq!(
        controller.status(),
        "Dispel completed: Eradicate Life Magic Self"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DispelControllerTests.cs :: TheServersAnswerToADispelIsReadOnTheFrameWithoutAPass
#[test]
fn the_servers_answer_to_a_dispel_is_read_on_the_frame_without_a_pass() {
    let mut surface = Surface {
        active: vulnerable(),
        spell_lookup: vec![vulnerability(100, 400), self_dispel_spell()],
        known_spell_ids: HashSet::from([SELF_DISPEL]),
        inventory: vec![item(10, "Chorizite")],
        mode: Some(CombatMode::Magic),
        ..Surface::default()
    };
    let settings = VitalSettings {
        cast_dispel_self: true,
        ..VitalSettings::default()
    };
    let mut controller = DispelController::new();

    assert!(tick(&mut controller, &mut surface, &settings, 0.0));
    assert!(controller.cast_in_flight());

    // No tick from here: the pass is held, only frames run.
    surface.cast_completion = completion(1, SELF_DISPEL, 1);
    controller.observe_pending_receipt(&mut surface, 0.05);

    assert!(!controller.cast_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DispelControllerTests.cs :: ASelfDispelCastIsReportedInFlightUntilTheServerAnswers
#[test]
fn a_self_dispel_cast_is_reported_in_flight_until_the_server_answers() {
    let mut surface = Surface {
        active: vulnerable(),
        spell_lookup: vec![vulnerability(100, 400), self_dispel_spell()],
        known_spell_ids: HashSet::from([SELF_DISPEL]),
        inventory: vec![item(10, "Chorizite")],
        mode: Some(CombatMode::Magic),
        ..Surface::default()
    };
    let settings = VitalSettings {
        cast_dispel_self: true,
        ..VitalSettings::default()
    };
    let mut controller = DispelController::new();
    assert!(!controller.cast_in_flight());

    assert!(tick(&mut controller, &mut surface, &settings, 0.0));
    assert_eq!(surface.targeted_cast, (SELF_DISPEL, 1));
    assert!(controller.cast_in_flight());

    surface.cast_completion = completion(1, SELF_DISPEL, 1);
    assert!(tick(&mut controller, &mut surface, &settings, 0.05));
    assert!(!controller.cast_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DispelControllerTests.cs :: MissingChoriziteFallsThroughToOfficialDispelItemOrder
#[test]
fn missing_chorizite_falls_through_to_official_dispel_item_order() {
    let mut surface = Surface {
        active: vulnerable(),
        spell_lookup: vec![vulnerability(100, 375), self_dispel_spell()],
        known_spell_ids: HashSet::from([SELF_DISPEL]),
        inventory: vec![
            item(20, "Condensed Dispel Potion"),
            item(21, "Black Market Gem of Dispelling"),
        ],
        mode: Some(CombatMode::Magic),
        ..Surface::default()
    };
    let settings = VitalSettings {
        cast_dispel_self: true,
        use_dispel_items: true,
        ..VitalSettings::default()
    };
    let mut controller = DispelController::new();

    assert!(tick(&mut controller, &mut surface, &settings, 0.0));

    assert_eq!(surface.used_item, 21);
    assert_eq!(surface.targeted_cast, (0, 0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DispelControllerTests.cs :: DispelItemsIgnoreVulnerabilitiesAboveTheirDifficultyLimit
#[test]
fn dispel_items_ignore_vulnerabilities_above_their_difficulty_limit() {
    let mut surface = Surface {
        active: vulnerable(),
        spell_lookup: vec![vulnerability(100, 401)],
        inventory: vec![item(11, "Rune of Dispel")],
        mode: Some(CombatMode::Magic),
        ..Surface::default()
    };
    let settings = VitalSettings {
        use_dispel_items: true,
        ..VitalSettings::default()
    };
    let mut controller = DispelController::new();

    assert!(!tick(&mut controller, &mut surface, &settings, 0.0));
    assert_eq!(surface.used_item, 0);
    assert_eq!(controller.status(), "Dispel idle");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/DispelControllerTests.cs :: AttenuatedAwakenerDispelsTheHighestScoringNearbyFellow
#[test]
fn attenuated_awakener_dispels_the_highest_scoring_nearby_fellow() {
    // The C# fixture gives the fire spell a quality override of 300 and the
    // cold one 320; the tracked entries carry those qualities.
    let fire = vulnerability(100, 350);
    let cold = SpellInfo {
        spell_id: 101,
        name: "Cold Vulnerability Other VII".to_owned(),
        family: 8,
        tier: 7,
        difficulty: 350,
        duration_seconds: 300.0,
        school: 32,
        is_debuff: true,
        is_offensive: true,
        ..SpellInfo::default()
    };
    let tracked = |target: u32, spell: &SpellInfo, quality: i32| TrackedEnchantment {
        target_id: target,
        spell_id: spell.spell_id,
        family: spell.family,
        quality,
        is_untargeted: spell.is_untargeted,
        seconds_remaining: 120.0,
    };
    let fellow = |id: u32, name: &str, distance: f32| FellowMember {
        object_id: id,
        name: name.to_owned(),
        current_health: 100,
        max_health: 100,
        current_stamina: 100,
        max_stamina: 100,
        current_mana: 100,
        max_mana: 100,
        distance,
        ..FellowMember::default()
    };
    let mut drum = item(50, "Attenuated Awakener");
    drum.equipped_location = 1;
    let skill = |id: u32, current: u32| Skill {
        skill_id: id,
        training: Training::Trained,
        current,
        base: current,
        ..Skill::default()
    };
    let mut surface = Surface {
        spell_lookup: vec![fire.clone(), cold.clone()],
        inventory: vec![drum],
        mode: Some(CombatMode::Magic),
        skills: HashMap::from([(31, skill(31, 300)), (14, skill(14, 110))]),
        members: vec![
            fellow(2, "One vuln", 40.0),
            fellow(3, "Two vulns", 50.0),
            fellow(4, "Too far", 51.0),
        ],
        tracked_by_target: HashMap::from([
            (2, vec![tracked(2, &fire, 300)]),
            (3, vec![tracked(3, &fire, 300), tracked(3, &cold, 320)]),
            (4, vec![tracked(4, &fire, 300), tracked(4, &cold, 320)]),
        ]),
        ..Surface::default()
    };
    // The scan covers the Items profile, not the whole inventory.
    let mut profile = CombatSettings::default();
    profile
        .combat_item_names
        .insert("Attenuated Awakener".to_owned());
    let settings = VitalSettings {
        use_dispel_drum: true,
        ..VitalSettings::default()
    };
    let mut controller = DispelController::new();

    assert!(tick_with(
        &mut controller,
        &mut surface,
        &settings,
        &profile,
        0.0
    ));
    assert_eq!(surface.applied_item, (50, 3));
    assert_eq!(
        controller.status(),
        "Using Attenuated Awakener on Two vulns"
    );
}
