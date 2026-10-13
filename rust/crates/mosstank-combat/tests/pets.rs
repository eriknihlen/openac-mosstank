//! Combat pets, ported from the C#'s `PetAutomationTests` and
//! `SummonPetRuleTests`.

use std::collections::HashMap;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus, CombatTarget};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::magic::{CastGate, ComponentInfo, SpellInfo};
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::surface::SUMMONING_COOLDOWN_ID;
use mosstank_automation::{Automation, RoomAhead, SpellCatalog};
use mosstank_combat::pets::{
    PET_ROOM_AHEAD_METERS, PetRefillRule, PetSummonChoice, REFILL_REFUSAL_RETRY_SECONDS,
    SUMMON_ANSWER_WAIT_SECONDS, SUMMON_REFUSAL_RETRY_SECONDS, SummonPetRule, SummonTurn,
    device_damage_type, select_pet, try_select_refill,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_meta::scheduler::MacroPassContext;
use mosstank_settings::model::{CombatSettings, PetRangeMode};
use mosstank_settings::monster::{MonsterRule, MonsterRuleActions};

// Essences as the catalog knows them.
const COLD: u32 = 49387;
const FIRE: u32 = 49380;
const ACID: u32 = 49366;
const ELECTRIC: u32 = 49373;
/// The spirit a refill tops an essence up with; the refill knows it by name.
const ENCAPSULATED_SPIRIT_CLASS: u32 = 49485;

/// How the fake client answers an appraisal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Appraisals {
    /// `PetAutomationTests.ItemAutomation`: an item with a structure ceiling
    /// is an essence, with its charges and level in the table.
    Essences,
    /// `SummonPetRuleTests.Automation`: an item whose ceiling is 50 carries
    /// the shared summoning cooldown, nothing else.
    Cooldown,
}

#[derive(Default)]
struct Cooldowns(HashMap<u32, f64>);

impl SpellCatalog for Cooldowns {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }
    fn try_get(&mut self, _spell_id: u32) -> Option<SpellInfo> {
        None
    }
    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        self.0.get(&cooldown_id).copied().unwrap_or(0.0)
    }
    fn try_get_component(&mut self, _component_id: u32) -> Option<ComponentInfo> {
        None
    }
}

/// The client.
struct Client {
    appraisals: Appraisals,
    items: Vec<InventoryItem>,
    /// The shared cooldown sent with each item itself, over the item's own.
    sent_cooldowns: HashMap<u32, u32>,
    /// Items this session has not appraised.
    unappraised: Vec<u32>,
    /// Property tables by object id, over the default ones.
    ints: HashMap<u32, Vec<(u32, i32)>>,
    targets: Vec<CombatTarget>,
    capture_calls: Vec<f32>,
    room: RoomAhead,
    asked_room: Vec<f32>,
    cooldowns: Cooldowns,
    summoning_training: Training,
    completion: ItemUseCompletion,
    use_status: ItemStatus,
    apply_status: ItemStatus,
    uses: Vec<u32>,
    applies: Vec<(u32, u32)>,
}

impl Client {
    fn new(appraisals: Appraisals, items: Vec<InventoryItem>) -> Self {
        Self {
            appraisals,
            items,
            sent_cooldowns: HashMap::new(),
            unappraised: Vec::new(),
            ints: HashMap::new(),
            targets: Vec::new(),
            capture_calls: Vec::new(),
            room: RoomAhead::Clear,
            asked_room: Vec::new(),
            cooldowns: Cooldowns::default(),
            summoning_training: Training::Trained,
            completion: ItemUseCompletion::default(),
            use_status: ItemStatus::Started,
            apply_status: ItemStatus::Started,
            uses: Vec::new(),
            applies: Vec::new(),
        }
    }
}

impl Automation for Client {
    fn is_available(&mut self) -> bool {
        true
    }
    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            ..CharacterInfo::default()
        }
    }
    fn skills(&mut self) -> Vec<Skill> {
        vec![Skill {
            skill_id: 54,
            name: "Summoning".to_owned(),
            training: self.summoning_training,
            current: 300,
            ..Skill::default()
        }]
    }
    fn spells(&mut self) -> &mut dyn SpellCatalog {
        &mut self.cooldowns
    }
    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::Unavailable
    }
    fn cast(&mut self, _spell_id: u32, _target: Option<u32>) -> bool {
        false
    }
    fn combat_mode(&mut self) -> CombatMode {
        CombatMode::Peace
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
    /// The host folds the shared cooldown an appraisal carries into the
    /// item it reports, as it does the one sent with the item itself.
    fn owned_items(&mut self) -> Vec<InventoryItem> {
        let mut items = self.items.clone();
        for item in &mut items {
            if let Some(&sent) = self.sent_cooldowns.get(&item.object_id) {
                item.shared_cooldown_id = sent;
            }
            if item.shared_cooldown_id == 0
                && let Some(cooldown) = self
                    .item_properties(item.object_id)
                    .and_then(|properties| properties.int(280))
            {
                item.shared_cooldown_id = u32::try_from(cooldown).unwrap();
            }
        }
        items
    }
    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.completion.clone()
    }
    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.uses.push(object_id);
        ItemResult::of(self.use_status)
    }
    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.applies.push((object_id, target_id));
        ItemResult::of(self.apply_status)
    }
    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        let item = self.items.iter().find(|item| item.object_id == object_id)?;
        let ints = if let Some(ints) = self.ints.get(&object_id) {
            ints.clone()
        } else {
            match self.appraisals {
                Appraisals::Essences => {
                    let mut ints = Vec::new();
                    if item.maximum_structure > 0 && !self.unappraised.contains(&object_id) {
                        ints.push((280, 213));
                        ints.push((92, item.structure));
                        ints.push((91, item.maximum_structure));
                        if item.spell_id != 0 {
                            ints.push((369, i32::try_from(item.spell_id).unwrap()));
                        }
                    }
                    ints
                }
                Appraisals::Cooldown if item.maximum_structure == 50 => vec![(280, 213)],
                Appraisals::Cooldown => Vec::new(),
            }
        };
        Some(ItemProperties {
            ints,
            ..ItemProperties::default()
        })
    }
    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        self.capture_calls.push(maximum_metres);
        self.targets.clone()
    }
    fn check_room_ahead(&mut self, distance_meters: f32) -> RoomAhead {
        self.asked_room.push(distance_meters);
        self.room
    }
    fn post_system_message(&mut self, _text: &str) {}
    fn log_info(&mut self, _text: &str) {}
    fn log_warn(&mut self, _text: &str) {}
}

fn pass(elapsed: f64) -> MacroPassContext {
    MacroPassContext::new(elapsed, true)
}

fn item(id: u32, weenie_class_id: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id,
        name: format!("Item {id}"),
        container_id: 1,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn target(id: u32, name: &str, distance: f32) -> CombatTarget {
    CombatTarget {
        object_id: id,
        name: name.to_owned(),
        weenie_class_id: 1,
        distance,
        is_health_known: true,
        health_fraction: 1.0,
        ..CombatTarget::default()
    }
}

fn default_rule(pet: MonsterDamageType) -> MonsterRule {
    MonsterRule::new(
        "DEFAULT",
        MonsterRuleActions {
            damage_type: MonsterDamageType::Auto,
            pet_damage_type: pet,
            ..MonsterRuleActions::default()
        },
    )
    .unwrap()
}

// --- PetAutomationTests -------------------------------------------------

/// `PetAutomationTests.Settings`.
fn settings(pet: MonsterDamageType) -> CombatSettings {
    let mut settings = CombatSettings {
        summon_pets: true,
        maximum_range: 10.0,
        ..CombatSettings::default()
    };
    settings.rules = vec![default_rule(pet)];
    for id in 1..=20 {
        settings.combat_item_object_ids.insert(id);
        settings.combat_item_order_ids.push(id);
    }
    settings
}

/// An essence as a client sees it (`PetAutomationTests.Device`): the level
/// it asks for rides in the spell id slot for the fake to pick up.
fn device(id: u32, wcid: u32) -> InventoryItem {
    device_with(id, wcid, 0, 100, 0, 50, 50)
}

fn device_with(
    id: u32,
    wcid: u32,
    mastery: i32,
    required_skill: i32,
    required_level: u32,
    structure: i32,
    maximum: i32,
) -> InventoryItem {
    InventoryItem {
        name: format!("Essence {id}"),
        summoning_mastery: mastery,
        use_requires_skill: 54,
        use_requires_skill_level: required_skill,
        structure,
        maximum_structure: maximum,
        spell_id: required_level,
        ..item(id, wcid)
    }
}

fn spirit(id: u32, stack: i32) -> InventoryItem {
    InventoryItem {
        name: "Encapsulated Spirit".to_owned(),
        stack_size: stack,
        ..item(id, ENCAPSULATED_SPIRIT_CLASS)
    }
}

/// The essences' client: every essence carries the summoning cooldown
/// sent with it.
fn essences(items: Vec<InventoryItem>) -> Client {
    let mut client = Client::new(Appraisals::Essences, items);
    for item in &client.items {
        if item.maximum_structure > 0 {
            client
                .sent_cooldowns
                .insert(item.object_id, SUMMONING_COOLDOWN_ID);
        }
    }
    client
}

fn character(mastery: i32, level: i32) -> CharacterInfo {
    CharacterInfo {
        id: 1,
        level,
        summoning_mastery: mastery,
        ..CharacterInfo::default()
    }
}

fn select(
    client: &mut Client,
    targets: &[CombatTarget],
    settings: &CombatSettings,
    attack: MonsterDamageType,
    preferences: &[MonsterDamageType],
) -> PetSummonChoice {
    let owned = client.owned_items();
    let preferences = preferences.to_vec();
    select_pet(
        client,
        &owned,
        targets,
        &character(3, 275),
        300,
        settings,
        &mut |_, _| attack,
        &|_| preferences.clone(),
        &mut |_, _| {},
    )
}

fn plain(
    client: &mut Client,
    targets: &[CombatTarget],
    settings: &CombatSettings,
) -> PetSummonChoice {
    select(client, targets, settings, MonsterDamageType::None, &[])
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: AMonsterTakesThePickOnlyByBeingNearerAndRankedHigher
#[test]
fn a_monster_takes_the_pick_only_by_being_nearer_and_ranked_higher() {
    let mut settings = settings(MonsterDamageType::Auto);
    let mut client = essences(vec![device(1, COLD)]);
    let targets = [target(10, "First", 6.0), target(11, "Nearer", 2.0)];

    assert_eq!(plain(&mut client, &targets, &settings).target.object_id, 10);

    settings.rules.insert(
        0,
        MonsterRule::new(
            "name#Nearer",
            MonsterRuleActions {
                priority: 5,
                pet_damage_type: MonsterDamageType::Auto,
                ..MonsterRuleActions::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(plain(&mut client, &targets, &settings).target.object_id, 11);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: TheDensityCountsOnlyMonstersInRangeThatWantAPet
#[test]
fn the_density_counts_only_monsters_in_range_that_want_a_pet() {
    let mut settings = settings(MonsterDamageType::Auto);
    settings.pet_monster_density = 2;
    settings.pet_range_mode = PetRangeMode::CUSTOM;
    settings.pet_custom_range = 10.0;
    settings.rules.insert(
        0,
        MonsterRule::new(
            "name#NoPet",
            MonsterRuleActions {
                pet_damage_type: MonsterDamageType::None,
                ..MonsterRuleActions::default()
            },
        )
        .unwrap(),
    );
    let mut client = essences(vec![device(1, COLD)]);

    assert!(
        plain(
            &mut client,
            &[
                target(10, "Target", 4.0),
                target(11, "NoPet", 3.0),
                target(12, "Far", 20.0)
            ],
            &settings
        )
        .is_none()
    );
    assert!(
        !plain(
            &mut client,
            &[target(10, "Target", 4.0), target(12, "Other", 9.0)],
            &settings
        )
        .is_none()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: AnEssenceOutOfReachIsPassedOverForOneThatIsNot
#[test]
fn an_essence_out_of_reach_is_passed_over_for_one_that_is_not() {
    let settings = settings(MonsterDamageType::Cold);
    // Every one of these suits the monster better than the last.
    let mut client = essences(vec![
        device_with(1, COLD, 0, 100, 200, 50, 50),
        device_with(2, COLD, 0, 400, 0, 50, 50),
        device_with(3, COLD, 0, 100, 0, 0, 50),
        device_with(4, COLD, 2, 100, 0, 50, 50),
        device(5, FIRE),
    ]);
    let owned = client.owned_items();
    let mut warnings: Vec<String> = Vec::new();

    let choice = select_pet(
        &mut client,
        &owned,
        &[target(10, "Target", 4.0)],
        &character(3, 150),
        300,
        &settings,
        &mut |_, _| MonsterDamageType::None,
        &|_| Vec::new(),
        &mut |_, text| warnings.push(text.to_owned()),
    );

    assert_eq!(choice.device.object_id, 5);
    assert!(
        warnings
            .iter()
            .any(|text| text.contains("wrong summoning mastery"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: ANamedElementIsFirstChoiceButNeverTheOnlyOne
#[test]
fn a_named_element_is_first_choice_but_never_the_only_one() {
    use MonsterDamageType as D;
    let targets = [target(10, "Target", 4.0)];
    let cold = settings(D::Cold);
    let mut client = essences(vec![device(1, ACID), device(2, FIRE), device(3, COLD)]);
    assert_eq!(
        select(&mut client, &targets, &cold, D::Fire, &[])
            .device
            .object_id,
        3
    );
    // No cold essence: the attack's own element comes next ...
    let mut client = essences(vec![device(1, ACID), device(2, FIRE)]);
    assert_eq!(
        select(&mut client, &targets, &cold, D::Fire, &[])
            .device
            .object_id,
        2
    );
    // ... then the monster's listed weaknesses, in their order ...
    assert_eq!(
        select(
            &mut client,
            &targets,
            &cold,
            D::Slash,
            &[D::Electric, D::Acid, D::Fire]
        )
        .device
        .object_id,
        1
    );
    // ... and with nothing to go by, the first usable one still summons.
    assert_eq!(
        select(&mut client, &targets, &cold, D::Slash, &[])
            .device
            .object_id,
        1
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: PlayerAutoFollowsTheAttackThenTheWeaknessList
#[test]
fn player_auto_follows_the_attack_then_the_weakness_list() {
    use MonsterDamageType as D;
    let targets = [target(10, "Target", 4.0)];
    let mut client = essences(vec![device(1, ACID), device(2, ELECTRIC), device(3, FIRE)]);
    let settings = settings(D::PlayerAuto);

    assert_eq!(
        select(&mut client, &targets, &settings, D::Fire, &[D::Electric])
            .device
            .object_id,
        3
    );
    assert_eq!(
        select(
            &mut client,
            &targets,
            &settings,
            D::Cold,
            &[D::Electric, D::Acid]
        )
        .device
        .object_id,
        2
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: AutoGoesByTheWeaknessListAlone
#[test]
fn auto_goes_by_the_weakness_list_alone() {
    use MonsterDamageType as D;
    let mut client = essences(vec![device(1, ACID), device(2, FIRE)]);
    assert_eq!(
        select(
            &mut client,
            &[target(10, "Target", 4.0)],
            &settings(D::Auto),
            D::Acid,
            &[D::Fire, D::Acid]
        )
        .device
        .object_id,
        2
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: AnEqualRankGoesToTheHigherLevelRequirementThenTheItemsPageOrder
#[test]
fn an_equal_rank_goes_to_the_higher_level_requirement_then_the_items_page_order() {
    let mut settings = settings(MonsterDamageType::Auto);
    settings.combat_item_order_ids = vec![3, 1, 2];
    let targets = [target(10, "Target", 4.0)];
    let mut client = essences(vec![
        device_with(1, COLD, 0, 100, 50, 50, 50),
        device_with(2, COLD, 0, 100, 80, 50, 50),
        device_with(3, FIRE, 0, 100, 50, 50, 50),
    ]);
    assert_eq!(plain(&mut client, &targets, &settings).device.object_id, 2);

    let mut client = essences(vec![
        device_with(1, COLD, 0, 100, 50, 50, 50),
        device_with(3, FIRE, 0, 100, 50, 50, 50),
    ]);
    assert_eq!(plain(&mut client, &targets, &settings).device.object_id, 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: AnEssenceTheTableDoesNotKnowStrikesAsABludgeon
#[test]
fn an_essence_the_table_does_not_know_strikes_as_a_bludgeon() {
    let mut client = essences(vec![device(1, COLD), device(2, 99999)]);
    assert_eq!(
        plain(
            &mut client,
            &[target(10, "Target", 4.0)],
            &settings(MonsterDamageType::Bludgeon)
        )
        .device
        .object_id,
        2
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: OnlyItemsOnTheItemsPageAreConsidered
#[test]
fn only_items_on_the_items_page_are_considered() {
    let mut settings = settings(MonsterDamageType::Auto);
    settings.combat_item_object_ids.clear();
    settings.combat_item_order_ids.clear();
    let mut client = essences(vec![device(1, COLD)]);
    assert!(plain(&mut client, &[target(10, "Target", 4.0)], &settings).is_none());
}

fn refill(client: &mut Client, settings: &CombatSettings, threshold: i32) -> Option<(u32, u32)> {
    let owned = client.owned_items();
    try_select_refill(client, &owned, settings, threshold)
        .map(|(device, spirit)| (device.object_id, spirit.object_id))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: TheRefillTakesTheFirstLowEssenceAndTheSmallestSpiritStack
#[test]
fn the_refill_takes_the_first_low_essence_and_the_smallest_spirit_stack() {
    let mut settings = settings(MonsterDamageType::Cold);
    // Nothing about the refill depends on summoning being wanted.
    settings.summon_pets = false;
    let mut client = essences(vec![
        // Under the threshold but already full: nothing to add.
        device_with(1, COLD, 0, 100, 0, 2, 2),
        device_with(2, FIRE, 0, 100, 0, 3, 50),
        device_with(3, ELECTRIC, 0, 100, 0, 1, 50),
        spirit(4, 12),
        spirit(5, 3),
    ]);

    assert_eq!(refill(&mut client, &settings, 3), Some((2, 5)));
    assert_eq!(
        refill(&mut client, &settings, 1).map(|(device, _)| device),
        Some(3)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: TheRefillKnowsAnUnappraisedEssenceByTheCooldownSentWithIt
#[test]
fn the_refill_knows_an_unappraised_essence_by_the_cooldown_sent_with_it() {
    let settings = settings(MonsterDamageType::Cold);
    let mut client = essences(vec![
        device_with(1, COLD, 0, 100, 0, 1, 50),
        device_with(2, FIRE, 0, 100, 0, 1, 50),
        spirit(3, 1),
    ]);
    client.sent_cooldowns.insert(1, 0);
    client.unappraised = vec![1, 2];

    assert_eq!(
        refill(&mut client, &settings, 3).map(|(device, _)| device),
        Some(2)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: TheRefillNeedsALowEssenceAndASpirit
#[test]
fn the_refill_needs_a_low_essence_and_a_spirit() {
    let settings = settings(MonsterDamageType::Cold);
    let mut client = essences(vec![device_with(1, COLD, 0, 100, 0, 4, 50), spirit(2, 1)]);
    assert_eq!(refill(&mut client, &settings, 3), None);

    let mut client = essences(vec![device_with(1, COLD, 0, 100, 0, 1, 50)]);
    assert_eq!(refill(&mut client, &settings, 3), None);
}

/// `PetAutomationTests.RefillRule`: the summon tests' client holding one
/// low essence and a spirit.
fn refill_rule() -> (PetRefillRule, Client, CombatSettings) {
    let mut client = Client::new(
        Appraisals::Cooldown,
        vec![device_with(1, COLD, 0, 100, 0, 1, 50), spirit(2, 1)],
    );
    client.sent_cooldowns.insert(1, SUMMONING_COOLDOWN_ID);
    (
        PetRefillRule::new(),
        client,
        settings(MonsterDamageType::Cold),
    )
}

fn tick_refill(
    rule: &mut PetRefillRule,
    client: &mut Client,
    settings: &CombatSettings,
    elapsed: f64,
) -> bool {
    rule.tick(client, settings, 3, &mut |_| true, pass(elapsed))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: ARefusedRefillGivesUpTheTurnAndWaitsBeforeAskingAgain
#[test]
fn a_refused_refill_gives_up_the_turn_and_waits_before_asking_again() {
    let (mut rule, mut client, settings) = refill_rule();
    client.apply_status = ItemStatus::Refused;

    assert!(!tick_refill(&mut rule, &mut client, &settings, 0.3));
    assert_eq!(client.applies.len(), 1);
    assert!(!tick_refill(&mut rule, &mut client, &settings, 0.3));
    assert_eq!(client.applies.len(), 1);

    client.apply_status = ItemStatus::Started;
    assert!(tick_refill(
        &mut rule,
        &mut client,
        &settings,
        REFILL_REFUSAL_RETRY_SECONDS
    ));
    assert_eq!(client.applies.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: ABusyRefillKeepsTheTurnButWaitsBeforeAskingAgain
#[test]
fn a_busy_refill_keeps_the_turn_but_waits_before_asking_again() {
    let (mut rule, mut client, settings) = refill_rule();
    client.apply_status = ItemStatus::Busy;

    assert!(tick_refill(&mut rule, &mut client, &settings, 0.3));
    assert_eq!(client.applies.len(), 1);
    assert!(tick_refill(&mut rule, &mut client, &settings, 0.3));
    assert_eq!(client.applies.len(), 1);

    client.apply_status = ItemStatus::Started;
    assert!(tick_refill(
        &mut rule,
        &mut client,
        &settings,
        REFILL_REFUSAL_RETRY_SECONDS
    ));
    assert_eq!(client.applies.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/PetAutomationTests.cs :: Catalog_MapsAuthenticDeviceWcids
#[test]
fn catalog_maps_authentic_device_wcids() {
    use MonsterDamageType as D;
    for (wcid, expected) in [
        (48886, D::Bludgeon),
        (49366, D::Acid),
        (49380, D::Fire),
        (49387, D::Cold),
        (49373, D::Electric),
    ] {
        assert_eq!(device_damage_type(wcid), expected, "{wcid}");
    }
}

// --- SummonPetRuleTests -------------------------------------------------

/// `SummonPetRuleTests.Settings`.
fn summon_settings() -> CombatSettings {
    let mut settings = CombatSettings {
        enabled: true,
        summon_pets: true,
        maximum_range: 10.0,
        ..CombatSettings::default()
    };
    settings.rules = vec![default_rule(MonsterDamageType::Cold)];
    for id in 1..=20 {
        settings.combat_item_object_ids.insert(id);
    }
    settings
}

/// An essence as a client sees it (`SummonPetRuleTests.Device`): the server
/// never sends the pet class, only the cooldown its appraisal carries.
fn summon_device(id: u32, wcid: u32, structure: i32) -> InventoryItem {
    InventoryItem {
        use_requires_skill: 54,
        use_requires_skill_level: 100,
        structure,
        maximum_structure: 50,
        ..item(id, wcid)
    }
}

/// `SummonPetRuleTests.Ready`.
fn ready() -> Client {
    let mut client = Client::new(Appraisals::Cooldown, vec![summon_device(1, 49387, 50)]);
    client.targets = vec![target(10, "Drudge", 4.0)];
    client
}

fn valid(
    rule: &mut SummonPetRule,
    client: &mut Client,
    settings: &CombatSettings,
    elapsed: f64,
) -> bool {
    let mut attack = |_: &mut dyn Automation, _: &CombatTarget| MonsterDamageType::None;
    let mut warn = |_: &mut dyn Automation, _: &str| {};
    let mut turn = SummonTurn {
        automation: client,
        settings,
        attack_element: &mut attack,
        damage_preferences: &|_| Vec::new(),
        warn: &mut warn,
    };
    rule.valid_now(&mut turn, pass(elapsed))
}

fn fresh_valid(client: &mut Client, settings: &CombatSettings) -> bool {
    valid(&mut SummonPetRule::new(), client, settings, 0.3)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: SummonsWithMonstersInRangeAndNoSelectedTarget
#[test]
fn summons_with_monsters_in_range_and_no_selected_target() {
    let mut client = ready();
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();

    // The predicate picks and issues nothing; the turn uses the device.
    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    assert!(client.uses.is_empty());
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: AnEssenceIsKnownByTheHostsPetDeviceFlag
#[test]
fn an_essence_is_known_by_the_hosts_pet_device_flag() {
    let mut client = ready();
    let settings = summon_settings();
    assert!(fresh_valid(&mut client, &settings));

    client.ints.insert(1, vec![(280, 1)]);
    assert!(!fresh_valid(&mut client, &settings));

    client.items[0].pet_class = 49388;
    assert!(fresh_valid(&mut client, &settings));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: AnUnappraisedEssenceIsKnownByTheCooldownSentWithIt
#[test]
fn an_unappraised_essence_is_known_by_the_cooldown_sent_with_it() {
    let mut client = ready();
    let settings = summon_settings();
    client.ints.insert(1, Vec::new());
    client.sent_cooldowns.insert(1, SUMMONING_COOLDOWN_ID);
    let mut rule = SummonPetRule::new();

    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1]);

    client.sent_cooldowns.insert(1, 5);
    assert!(!fresh_valid(&mut client, &settings));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: TheNextSummonWaitsForTheCooldownAlone
#[test]
fn the_next_summon_waits_for_the_cooldown_alone() {
    let mut client = ready();
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();

    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1]);
    rule.set_running(&mut client, false);

    client.cooldowns.0.insert(213, 45.0);
    assert!(!valid(&mut rule, &mut client, &settings, 20.0));

    client.cooldowns.0.insert(213, 0.0);
    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1, 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: ARefusedSummonWaitsBeforeItIsTriedAgain
#[test]
fn a_refused_summon_waits_before_it_is_tried_again() {
    let mut client = ready();
    client.use_status = ItemStatus::Refused;
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();

    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1]);
    rule.set_running(&mut client, false);

    assert!(!valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1]);
    rule.set_running(&mut client, false);

    client.use_status = ItemStatus::Started;
    assert!(valid(
        &mut rule,
        &mut client,
        &settings,
        SUMMON_REFUSAL_RETRY_SECONDS
    ));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1, 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: ASummonTheServerFailsWaitsForItsAnswerAndThenTheRetryClock
#[test]
fn a_summon_the_server_fails_waits_for_its_answer_and_then_the_retry_clock() {
    let mut client = ready();
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();

    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    rule.set_running(&mut client, false);
    assert!(!valid(&mut rule, &mut client, &settings, 0.3));

    // The server answers the summon with an error and starts no cooldown.
    client.completion = ItemUseCompletion {
        revision: 1,
        source_id: 1,
        target_id: 0,
        weenie_error: 0x0402,
    };
    assert!(!valid(&mut rule, &mut client, &settings, 0.3));
    assert!(!valid(&mut rule, &mut client, &settings, 0.3));
    assert!(valid(
        &mut rule,
        &mut client,
        &settings,
        SUMMON_REFUSAL_RETRY_SECONDS
    ));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1, 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: ASummonThatWentThroughEndsTheWaitOnItsOwnAnswer
#[test]
fn a_summon_that_went_through_ends_the_wait_on_its_own_answer() {
    let mut client = ready();
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();
    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    rule.set_running(&mut client, false);

    client.completion = ItemUseCompletion {
        revision: 1,
        source_id: 7,
        ..ItemUseCompletion::default()
    };
    assert!(!valid(&mut rule, &mut client, &settings, 0.3));

    client.completion = ItemUseCompletion {
        revision: 2,
        source_id: 1,
        ..ItemUseCompletion::default()
    };
    assert!(valid(&mut rule, &mut client, &settings, 0.3));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: ASummonWhoseAnswerNeverComesStopsWaitingForIt
#[test]
fn a_summon_whose_answer_never_comes_stops_waiting_for_it() {
    let mut client = ready();
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();
    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    rule.set_running(&mut client, false);

    assert!(!valid(
        &mut rule,
        &mut client,
        &settings,
        SUMMON_ANSWER_WAIT_SECONDS - 1.0
    ));
    assert!(valid(&mut rule, &mut client, &settings, 1.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: NoRoomAheadForThePetIsInvalid
#[test]
fn no_room_ahead_for_the_pet_is_invalid() {
    let mut client = ready();
    client.room = RoomAhead::Blocked;
    let settings = summon_settings();
    let mut rule = SummonPetRule::new();

    assert!(!valid(&mut rule, &mut client, &settings, 0.3));
    assert_eq!(client.asked_room, [PET_ROOM_AHEAD_METERS]);
    rule.set_running(&mut client, true);
    assert!(client.uses.is_empty());

    client.room = RoomAhead::Unknown;
    assert!(valid(&mut rule, &mut client, &settings, 0.3));

    client.room = RoomAhead::Clear;
    assert!(valid(&mut rule, &mut client, &settings, 0.3));
    rule.set_running(&mut client, true);
    assert_eq!(client.uses, [1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: TheRoomCheckComesAfterThePetIsChosen
#[test]
fn the_room_check_comes_after_the_pet_is_chosen() {
    let mut client = ready();
    client.targets.clear();
    assert!(!fresh_valid(&mut client, &summon_settings()));
    assert!(client.asked_room.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: CombatDisabledIsInvalid
#[test]
fn combat_disabled_is_invalid() {
    let mut client = ready();
    let mut settings = summon_settings();
    settings.enabled = false;
    assert!(!fresh_valid(&mut client, &settings));
    assert!(client.uses.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: SummonPetsOffIsInvalid
#[test]
fn summon_pets_off_is_invalid() {
    let mut client = ready();
    let mut settings = summon_settings();
    settings.summon_pets = false;
    assert!(!fresh_valid(&mut client, &settings));
    assert!(client.capture_calls.is_empty());
    assert!(client.uses.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: UntrainedSummoningIsInvalid
#[test]
fn untrained_summoning_is_invalid() {
    let mut client = ready();
    client.summoning_training = Training::Untrained;
    assert!(!fresh_valid(&mut client, &summon_settings()));
    assert!(client.uses.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: TheSharedSummoningCooldownIsInvalid
#[test]
fn the_shared_summoning_cooldown_is_invalid() {
    let mut client = ready();
    client.cooldowns.0.insert(213, 30.0);
    assert!(!fresh_valid(&mut client, &summon_settings()));
    assert!(client.uses.is_empty());

    client.cooldowns.0.insert(213, 0.0);
    assert!(fresh_valid(&mut client, &summon_settings()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: NoMonstersInRangeIsInvalid
#[test]
fn no_monsters_in_range_is_invalid() {
    let mut client = ready();
    client.targets.clear();
    assert!(!fresh_valid(&mut client, &summon_settings()));
    assert!(client.uses.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: MonsterDensityBelowTheSettingIsInvalid
#[test]
fn monster_density_below_the_setting_is_invalid() {
    let mut client = ready();
    let mut settings = summon_settings();
    settings.pet_monster_density = 2;
    assert!(!fresh_valid(&mut client, &settings));
    assert!(client.uses.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: ASelectedTargetDoesNotRefuseTheSummon
#[test]
fn a_selected_target_does_not_refuse_the_summon() {
    // The reference rule asks nothing about a selected target: it fires off
    // the in-range monster census alone.
    let mut client = ready();
    assert!(fresh_valid(&mut client, &summon_settings()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SummonPetRuleTests.cs :: IndependentTrackNeverRefills
#[test]
fn independent_track_never_refills() {
    let mut client = ready();
    client.items = vec![
        summon_device(1, 49387, 0),
        InventoryItem {
            name: "Encapsulated Spirit".to_owned(),
            structure: 1,
            maximum_structure: 1,
            ..item(2, ENCAPSULATED_SPIRIT_CLASS)
        },
    ];
    assert!(!fresh_valid(&mut client, &summon_settings()));
    assert!(client.applies.is_empty());
    assert!(client.uses.is_empty());
}
