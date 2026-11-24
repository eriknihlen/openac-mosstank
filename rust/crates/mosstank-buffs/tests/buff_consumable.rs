//! The buff rule's consumables: a gem the profile's GemFoodItems names and
//! a buff consumable added by hand, appraised or not, with their shared
//! cooldown, the buff they give already on, and the appraisals the rule
//! asks for.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{ActiveEnchantment, Skill};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::world::{ItemProperties, WorldObject};
use mosstank_automation::locks::ActionLockTable;
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_buffs::appraisal;
use mosstank_buffs::buff_rule::{BuffRuleHost, BuffSelfRule, BuffTurn};
use mosstank_combat::MacroLogChannel;
use mosstank_settings::model::{
    BuffSettings, CombatSettings, ConsumableCategory, GemFoodItem, VitalSettings,
};

const CHARACTER: u32 = 0x5000_0001;
const PEARL: u32 = 0x8000_0101;
const SECOND_PEARL: u32 = 0x8000_0102;
const STONE: u32 = 0x8000_0201;
const PEARL_NAME: &str = "Black Luster Pearl";
const STONE_NAME: &str = "Hand Stone";
const PEARL_SPELL: u32 = 3800;
const PEARL_FAMILY: u32 = 600;
const STONE_SPELL: u32 = 3810;
const STONE_FAMILY: u32 = 601;
const PEARL_COOLDOWN: u32 = 7;

fn spell(spell_id: u32, family: u32, name: &str) -> SpellInfo {
    SpellInfo {
        spell_id,
        name: name.to_owned(),
        family,
        tier: 1,
        duration_seconds: 3600.0,
        is_self_targeted: true,
        ..SpellInfo::default()
    }
}

fn item(object_id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id,
        name: name.to_owned(),
        container_id: CHARACTER,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

/// The spell table, with its shared cooldowns.
#[derive(Default)]
struct Catalog {
    spells: Vec<SpellInfo>,
    cooldowns: HashMap<u32, f64>,
}

impl SpellCatalog for Catalog {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.spells
            .iter()
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
    }

    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        self.cooldowns.get(&cooldown_id).copied().unwrap_or(0.0)
    }
}

/// The client: items appraised as the test sets them; an appraisal is
/// answered only when the test answers it.
#[derive(Default)]
struct Surface {
    inventory: Vec<InventoryItem>,
    assessed: HashSet<u32>,
    properties: HashMap<u32, ItemProperties>,
    enchantments: Vec<ActiveEnchantment>,
    catalog: Catalog,
    identify_requests: Vec<u32>,
    identify_status: Option<ItemStatus>,
    used: Vec<u32>,
    completion: ItemUseCompletion,
    infos: Vec<String>,
}

impl Surface {
    fn new() -> Self {
        let mut surface = Self::default();
        surface.catalog.spells = vec![
            spell(PEARL_SPELL, PEARL_FAMILY, "Burning Spirit"),
            spell(STONE_SPELL, STONE_FAMILY, "A Stone's Effect"),
        ];
        surface
    }

    /// The server answers the appraisal of `object_id`.
    fn answer_appraisal(&mut self, object_id: u32, ints: &[(u32, i32)], spells: &[u32]) {
        self.assessed.insert(object_id);
        self.properties.insert(
            object_id,
            ItemProperties {
                ints: ints.to_vec(),
                ..ItemProperties::default()
            },
        );
        if let Some(item) = self
            .inventory
            .iter_mut()
            .find(|item| item.object_id == object_id)
        {
            item.appraised_spell_ids = spells.to_vec();
        }
    }

    fn buff_on(&mut self, spell_id: u32, family: u32) {
        self.enchantments.push(ActiveEnchantment {
            spell_id,
            family,
            tier: 1,
            seconds_remaining: 3000.0,
        });
    }

    fn info_count(&self, needle: &str) -> usize {
        self.infos
            .iter()
            .filter(|line| line.contains(needle))
            .count()
    }
}

impl Automation for Surface {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: CHARACTER,
            name: "Tester".to_owned(),
            health: (100, 100, 0),
            stamina: (100, 100, 0),
            mana: (100, 100, 0),
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        Vec::new()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        &mut self.catalog
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

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.inventory.clone()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.used.push(object_id);
        ItemResult {
            status: ItemStatus::Started,
            notice: None,
        }
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult {
            status: ItemStatus::Unavailable,
            notice: None,
        }
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.properties.get(&object_id).cloned()
    }

    fn objects_available(&mut self) -> bool {
        true
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        let item = self
            .inventory
            .iter()
            .find(|item| item.object_id == object_id)?;
        Some(WorldObject {
            object_id,
            name: item.name.clone(),
            container_id: item.container_id,
            is_owned: true,
            has_appraisal_data: self.assessed.contains(&object_id),
            last_id_time: i32::from(self.assessed.contains(&object_id)),
            ..WorldObject::default()
        })
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.identify_requests.push(object_id);
        ItemResult {
            status: self.identify_status.unwrap_or(ItemStatus::Started),
            notice: None,
        }
    }

    fn timed_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.enchantments.clone()
    }

    fn post_system_message(&mut self, _text: &str) {}

    fn log_info(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }

    fn log_warn(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }
}

/// The owner: the macro runs, everything said is dropped.
struct Host;

impl BuffRuleHost for Host {
    fn macro_enabled(&self) -> bool {
        true
    }
    fn set_status(&mut self, _status: &str) {}
    fn announce(&mut self, _automation: &mut dyn Automation, _text: &str) {}
    fn log(&mut self, _channel: MacroLogChannel, _message: &str) {}
    fn mirror_to_log(&mut self, _channel: MacroLogChannel, _message: &str) {}
    fn capture_selection(&mut self, _automation: &mut dyn Automation) {}
    fn restore_selection(&mut self, _automation: &mut dyn Automation) {}
    fn stop_from_buff_rule(&mut self, _automation: &mut dyn Automation, _status: &str) {}
    fn begin_fast_cast(&mut self, _automation: &mut dyn Automation, _spell: &SpellInfo) {}
}

/// The settings: the pearl in GemFoodItems, the stone a buff consumable
/// added by hand.
struct Settings {
    buffs: BuffSettings,
    combat: CombatSettings,
    vitals: VitalSettings,
}

impl Settings {
    fn new() -> Self {
        let buffs = BuffSettings {
            gem_food_items: vec![GemFoodItem {
                name: PEARL_NAME.to_owned(),
                spell_id: PEARL_SPELL,
            }],
            ..BuffSettings::default()
        };
        let mut combat = CombatSettings::default();
        combat.consumable_names.insert(STONE_NAME.to_owned());
        combat
            .consumable_categories
            .insert(STONE_NAME.to_owned(), ConsumableCategory::BuffConsumable);
        Self {
            buffs,
            combat,
            vitals: VitalSettings::default(),
        }
    }
}

fn bound_rule() -> BuffSelfRule {
    let mut rule = BuffSelfRule::new();
    rule.bind_consumables(ActionLockTable::shared());
    rule
}

/// One turn of the rule with the turn its own.
fn tick(rule: &mut BuffSelfRule, surface: &mut Surface, settings: &Settings) {
    let mut host = Host;
    let mut turn = BuffTurn {
        automation: surface,
        settings: &settings.buffs,
        combat: &settings.combat,
        vitals: &settings.vitals,
        host: &mut host,
    };
    rule.tick(&mut turn, true, false);
}

fn advance(rule: &mut BuffSelfRule, surface: &mut Surface, seconds: f64) {
    let mut host = Host;
    rule.advance(surface, &mut host, seconds);
}

/// A gem the profile names carries its spell in the profile: it is used
/// with no appraisal, as the reference uses it (a looted, never-appraised
/// gem was passed over for good).
#[test]
fn an_unassessed_configured_gem_is_used_when_its_buff_is_missing() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME)];
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);

    assert_eq!(surface.used, [PEARL]);
}

/// A gem whose buff is on asks for its appraisal (one question), is not
/// used, and says why once.
#[test]
fn an_unassessed_gem_whose_buff_is_on_is_appraised_not_used_and_said_once() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME)];
    surface.buff_on(PEARL_SPELL, PEARL_FAMILY);
    let settings = Settings::new();
    let mut rule = bound_rule();

    for _ in 0..5 {
        tick(&mut rule, &mut surface, &settings);
        advance(&mut rule, &mut surface, 0.3);
    }

    assert!(surface.used.is_empty());
    assert_eq!(surface.identify_requests, [PEARL]);
    assert_eq!(
        surface.info_count("not used: its buff (Burning Spirit) is already active"),
        1
    );
}

/// A buff consumable added by hand takes its spell from its appraisal: the
/// rule asks for it, and uses the item once it has come.
#[test]
fn an_unassessed_hand_added_consumable_is_appraised_then_used() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(STONE, STONE_NAME)];
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [STONE]);
    assert!(surface.used.is_empty());
    assert_eq!(surface.info_count("not assessed yet"), 1);

    surface.answer_appraisal(STONE, &[], &[STONE_SPELL]);
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);

    assert_eq!(surface.used, [STONE]);
    assert_eq!(surface.identify_requests, [STONE]);
}

/// An appraised gem whose buff is on is not used.
#[test]
fn an_assessed_gem_whose_buff_is_on_is_not_used() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME)];
    surface.answer_appraisal(PEARL, &[], &[PEARL_SPELL]);
    surface.buff_on(PEARL_SPELL, PEARL_FAMILY);
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);

    assert!(surface.used.is_empty());
    assert!(surface.identify_requests.is_empty());
}

/// The shared cooldown the appraisal names (property 280) holds the gem.
#[test]
fn the_appraised_shared_cooldown_holds_the_gem() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME)];
    surface.answer_appraisal(PEARL, &[(280, 7)], &[PEARL_SPELL]);
    surface.catalog.cooldowns.insert(PEARL_COOLDOWN, 120.0);
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);
    assert!(surface.used.is_empty());
    assert_eq!(
        surface.info_count("not used: its shared cooldown is running"),
        1
    );

    surface.catalog.cooldowns.clear();
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.used, [PEARL]);
}

/// The shared cooldown the item came with holds an unappraised gem too.
#[test]
fn the_shared_cooldown_sent_with_the_item_holds_an_unassessed_gem() {
    let mut surface = Surface::new();
    let mut pearl = item(PEARL, PEARL_NAME);
    pearl.shared_cooldown_id = PEARL_COOLDOWN;
    surface.inventory = vec![pearl];
    surface.catalog.cooldowns.insert(PEARL_COOLDOWN, 120.0);
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);

    assert!(surface.used.is_empty());
}

/// An item no list names is neither appraised nor used.
#[test]
fn an_unconfigured_item_is_neither_appraised_nor_used() {
    let mut surface = Surface::new();
    let mut other = item(0x8000_0301, "Some Other Gem");
    other.appraised_spell_ids = vec![PEARL_SPELL];
    surface.inventory = vec![other];
    let settings = Settings::new();
    let mut rule = bound_rule();

    for _ in 0..5 {
        tick(&mut rule, &mut surface, &settings);
        advance(&mut rule, &mut surface, 0.3);
    }

    assert!(surface.used.is_empty());
    assert!(surface.identify_requests.is_empty());
}

/// One question at a time: no second request while one is out, and an
/// unanswered one holds its item back before it is asked again.
#[test]
fn appraisal_requests_are_one_at_a_time_and_held_back_when_unanswered() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME), item(SECOND_PEARL, PEARL_NAME)];
    surface.buff_on(PEARL_SPELL, PEARL_FAMILY);
    let settings = Settings::new();
    let mut rule = bound_rule();

    // Within the reply window: the first question stays the only one.
    let mut elapsed = 0.0;
    while elapsed < appraisal::REPLY_SECONDS - 1.0 {
        tick(&mut rule, &mut surface, &settings);
        advance(&mut rule, &mut surface, 0.5);
        elapsed += 0.5;
    }
    assert_eq!(surface.identify_requests, [PEARL]);

    // Unanswered past the window: the next pearl is asked about, the first
    // held back.
    advance(&mut rule, &mut surface, 2.0);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [PEARL, SECOND_PEARL]);

    // The second is not answered either; the first is still held back.
    advance(&mut rule, &mut surface, appraisal::REPLY_SECONDS + 1.0);
    tick(&mut rule, &mut surface, &settings);
    advance(&mut rule, &mut surface, 0.5);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [PEARL, SECOND_PEARL]);

    // Once the hold is over, the first is asked about again.
    advance(&mut rule, &mut surface, appraisal::UNANSWERED_HOLD_SECONDS);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [PEARL, SECOND_PEARL, PEARL]);
}

/// A request the client does not take is not counted as out, and the item
/// waits a little before it is asked again.
#[test]
fn a_request_the_client_does_not_take_is_retried_after_a_pause() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(STONE, STONE_NAME)];
    surface.identify_status = Some(ItemStatus::Busy);
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);
    advance(&mut rule, &mut surface, 0.5);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [STONE]);

    surface.identify_status = None;
    advance(&mut rule, &mut surface, appraisal::NOT_TAKEN_HOLD_SECONDS);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [STONE, STONE]);
}

/// An item that leaves the inventory while its appraisal is out lets the
/// question go at once: the next configured item is asked about on the
/// next pass, not after the reply window.
#[test]
fn an_item_leaving_mid_request_frees_the_request_at_once() {
    const SECOND_STONE: u32 = 0x8000_0202;
    let mut surface = Surface::new();
    surface.inventory = vec![item(STONE, STONE_NAME), item(SECOND_STONE, STONE_NAME)];
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [STONE]);

    surface.inventory.retain(|item| item.object_id != STONE);
    advance(&mut rule, &mut surface, 0.5);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [STONE, SECOND_STONE]);

    surface.answer_appraisal(SECOND_STONE, &[], &[STONE_SPELL]);
    advance(&mut rule, &mut surface, 0.5);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.used, [SECOND_STONE]);
    assert_eq!(surface.identify_requests, [STONE, SECOND_STONE]);
}

/// A reason is said once, and said again each time it changes.
#[test]
fn a_new_line_is_said_when_the_skip_reason_changes() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME)];
    surface.answer_appraisal(PEARL, &[(280, 7)], &[PEARL_SPELL]);
    surface.buff_on(PEARL_SPELL, PEARL_FAMILY);
    let settings = Settings::new();
    let mut rule = bound_rule();
    let active = "not used: its buff (Burning Spirit) is already active";
    let cooling = "not used: its shared cooldown is running";

    tick(&mut rule, &mut surface, &settings);
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(
        (surface.info_count(active), surface.info_count(cooling)),
        (1, 0)
    );

    surface.catalog.cooldowns.insert(PEARL_COOLDOWN, 120.0);
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(
        (surface.info_count(active), surface.info_count(cooling)),
        (1, 1)
    );

    surface.catalog.cooldowns.clear();
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(
        (surface.info_count(active), surface.info_count(cooling)),
        (2, 1)
    );
    assert!(surface.used.is_empty());
}

/// A hand-added consumable's line says the assessment is requested only
/// while a request is out.
#[test]
fn the_line_says_requested_only_while_a_request_is_out() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(STONE, STONE_NAME)];
    surface.identify_status = Some(ItemStatus::Busy);
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);
    advance(&mut rule, &mut surface, 0.5);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.info_count("waiting to request its assessment"), 1);
    assert_eq!(surface.info_count("its assessment is requested"), 0);

    surface.identify_status = None;
    advance(&mut rule, &mut surface, appraisal::NOT_TAKEN_HOLD_SECONDS);
    tick(&mut rule, &mut surface, &settings);
    advance(&mut rule, &mut surface, 0.5);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.identify_requests, [STONE, STONE]);
    assert_eq!(surface.info_count("its assessment is requested"), 1);
}

/// The server refuses the first use of a never-appraised gem: the gem is
/// held back, its appraisal is asked for meanwhile, and once the hold is
/// over the gem is used again, this time with success.
#[test]
fn a_refused_first_use_is_followed_by_the_appraisal_and_a_good_use() {
    let mut surface = Surface::new();
    surface.inventory = vec![item(PEARL, PEARL_NAME)];
    let settings = Settings::new();
    let mut rule = bound_rule();

    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.used, [PEARL]);

    // The server answers the use with an error.
    surface.completion = ItemUseCompletion {
        revision: 1,
        source_id: PEARL,
        target_id: 0,
        weenie_error: 0x0001,
    };
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.used, [PEARL]);
    assert_eq!(surface.identify_requests, [PEARL]);

    surface.answer_appraisal(PEARL, &[], &[PEARL_SPELL]);
    advance(&mut rule, &mut surface, 0.3);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.used, [PEARL]);

    // Past the hold a failed use earns, it is used again and succeeds.
    advance(&mut rule, &mut surface, 30.0);
    tick(&mut rule, &mut surface, &settings);
    assert_eq!(surface.used, [PEARL, PEARL]);
    surface.completion = ItemUseCompletion {
        revision: 2,
        source_id: PEARL,
        target_id: 0,
        weenie_error: 0,
    };
    surface.buff_on(PEARL_SPELL, PEARL_FAMILY);
    advance(&mut rule, &mut surface, 0.3);
    for _ in 0..5 {
        tick(&mut rule, &mut surface, &settings);
        advance(&mut rule, &mut surface, 0.3);
    }
    assert_eq!(surface.used, [PEARL, PEARL]);
    assert_eq!(surface.identify_requests, [PEARL]);
}
