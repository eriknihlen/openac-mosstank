//! The looting tests' client (`LootingTests.Automation`, `.Host`,
//! `.ClassifierRegistry`): every command is recorded, and the loot surface,
//! the pack, the appraisal slot and the chat are what the test sets.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{
    AppraisalState, InventoryCompletion, InventoryItem, ItemResult, ItemStatus, ItemUseCompletion,
    LootAction as PublicLootAction, LootClassification, LootContext, LootedItem, Palette,
};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::navigation::{MovementIntent, NavigationSnapshot, NavigationStatus};
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, Position, WorldObject};
use mosstank_automation::locks::{ActionLockTable, SharedActionLocks};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_formats::utl::{LootAction, LootRule, VtankLootRequirement};
use mosstank_loot::LootController;
use mosstank_loot::host::{LootCorpse, LootHost};
use mosstank_settings::model::LootSettings;

/// The character's id.
pub const PLAYER: u32 = 0x5000_0001;

/// `Item(id, name, wcid)`: an item with a stack of one and nothing else.
pub fn item(id: u32, name: &str, wcid: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: wcid,
        name: name.to_owned(),
        stack_size: 1,
        container_slot: 0,
        ..InventoryItem::default()
    }
}

/// `Scroll(id, spellName, spellId)`: a writable item carrying the spell it
/// teaches, classed as a scroll.
pub fn scroll(id: u32, spell_name: &str, spell_id: u32) -> InventoryItem {
    InventoryItem {
        item_type: 0x0000_2000,
        spell_id,
        object_class: ObjectClass::Scroll,
        ..item(id, spell_name, 88)
    }
}

/// `Requirement(type, lines)`: the lines joined and ended with `\r\n`.
pub fn requirement(kind: i32, lines: &[&str]) -> VtankLootRequirement {
    VtankLootRequirement {
        kind,
        payload: format!("{}\r\n", lines.join("\r\n")),
    }
}

/// `VtankRule(name, action, requirements)`.
pub fn vtank_rule(
    name: &str,
    action: LootAction,
    requirements: Vec<VtankLootRequirement>,
) -> LootRule {
    let mut rule = LootRule::named(name, "*");
    rule.action = action;
    rule.vtank_requirements = requirements;
    rule
}

/// `new LootRule { Name, Expression, Action, ... }`.
pub fn rule(name: &str, expression: &str, action: LootAction) -> LootRule {
    let mut rule = LootRule::named(name, expression);
    rule.action = action;
    rule
}

/// A rule of the default name with this expression (keep).
pub fn expression_rule(expression: &str) -> LootRule {
    let mut rule = LootRule::default();
    rule.set_expression(expression);
    rule
}

/// `Place(east, north, heading)`: metres east and north of nowhere.
pub fn place(east_meters: f64, north_meters: f64, heading_degrees: f32) -> Position {
    Position {
        cell_id: 1,
        east_west: east_meters / 240.0,
        north_south: north_meters / 240.0,
        elevation: 0.0,
        heading_degrees,
        is_outdoor: true,
    }
}

/// A described corpse of this character's kill, at a distance, with no
/// place (`new PluginLootContainer(...) { IsIdentified, LongDescription }`).
pub fn chain_corpse(id: u32, distance: f32) -> LootCorpse {
    LootCorpse {
        is_identified: true,
        long_description: "Killed by Tester.".to_owned(),
        ..LootCorpse::new(id, "Corpse", distance)
    }
}

/// `Corpse(id, distance)` of the approach tests: described, this
/// character's kill, placed due north.
pub fn approach_corpse(id: u32, distance: f32) -> LootCorpse {
    LootCorpse {
        position: Some(Position {
            cell_id: 0x7F7F_0001,
            east_west: 0.0,
            north_south: f64::from(distance) / 240.0,
            elevation: 0.0,
            heading_degrees: 0.0,
            is_outdoor: true,
        }),
        ..chain_corpse(id, distance)
    }
}

/// `OwnRareCorpse(id, distance)`.
pub fn own_rare_corpse(id: u32, distance: f32) -> LootCorpse {
    LootCorpse {
        long_description: "Killed by Tester. This corpse generated a rare item!".to_owned(),
        ..approach_corpse(id, distance)
    }
}

/// An undescribed corpse.
pub fn undescribed(corpse: LootCorpse) -> LootCorpse {
    LootCorpse {
        is_identified: false,
        ..corpse
    }
}

/// `new PluginSpellInfo(id, name, family, tier, difficulty, mana, duration,
/// school, ...)`.
pub fn spell(id: u32, name: &str, difficulty: i32, school: u32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: 1,
        tier: 1,
        difficulty,
        mana_cost: 10,
        school,
        ..SpellInfo::default()
    }
}

/// `new PluginSkillInfo(id, name, training, current)`.
pub fn skill(id: u32, name: &str, current: u32) -> Skill {
    Skill {
        skill_id: id,
        name: name.to_owned(),
        current,
        ..Skill::default()
    }
}

/// A chat line with this sequence and text.
pub fn chat(sequence: u64, text: &str) -> ChatLine {
    ChatLine {
        sequence,
        text: text.to_owned(),
        ..ChatLine::default()
    }
}

/// Properties with these string values.
pub fn strings(values: &[(u32, &str)]) -> ItemProperties {
    ItemProperties {
        strings: values.iter().map(|(k, v)| (*k, (*v).to_owned())).collect(),
        ..ItemProperties::default()
    }
}

/// A palette entry.
pub fn palette(palette_id: u32, red: u8, green: u8, blue: u8) -> Palette {
    Palette {
        palette_id,
        offset: 0,
        length: 1,
        red,
        green,
        blue,
    }
}

/// The test file's classifier registry (`ClassifierRegistry`): one id,
/// one answer.
#[derive(Debug, Clone)]
pub struct ClassifierRegistry {
    pub id: String,
    pub result: LootClassification,
    pub classification_count: usize,
    pub looted: Vec<LootedItem>,
    pub removed: Vec<u32>,
}

impl ClassifierRegistry {
    pub fn new(id: &str, result: LootClassification) -> Self {
        Self {
            id: id.to_owned(),
            result,
            classification_count: 0,
            looted: Vec::new(),
            removed: Vec::new(),
        }
    }
}

/// `new PluginLootClassification(matched, action, ruleName, priority,
/// keepCount)`.
pub fn classification(
    action: PublicLootAction,
    rule_name: &str,
    priority: i32,
    keep_count: i32,
) -> LootClassification {
    LootClassification {
        matched: true,
        action,
        rule_name: rule_name.to_owned(),
        priority,
        keep_count,
    }
}

/// The test file's client.
#[derive(Debug)]
pub struct World {
    pub navigation_position: Position,
    /// Whether the navigation snapshot says the world is there (the
    /// corpse walk's `ApproachAutomation`).
    pub navigation_available: bool,
    /// Every movement intent the client was asked to hold.
    pub intents: Vec<MovementIntent>,
    /// How often the held movement was let go.
    pub movement_clears: usize,
    pub is_owned_inventory_complete: bool,
    pub items_busy: bool,
    pub name: String,
    pub character_level: i32,
    pub main_pack_slots: i32,
    pub requested: u32,
    pub current: u32,
    pub corpses: Vec<LootCorpse>,
    pub contents: Vec<InventoryItem>,
    pub owned: Vec<InventoryItem>,
    pub inventory_completion: InventoryCompletion,
    pub appraisal_state: AppraisalState,
    pub skills: Vec<Skill>,
    pub known_spell: Option<SpellInfo>,
    pub spell_learned: bool,
    pub members: Vec<FellowMember>,
    pub chat_messages: Vec<ChatLine>,
    pub opened: Vec<u32>,
    pub closed: Vec<u32>,
    pub used: Vec<u32>,
    pub use_completion: ItemUseCompletion,
    pub applied: Vec<(u32, u32)>,
    pub apply_results: VecDeque<ItemResult>,
    /// Models what a drain does to the pack: the stone comes back charged
    /// and the donor is gone, once [`World::settle_mana_drains`] runs.
    pub mana_drain_consumes_donor: bool,
    pub unsettled_drains: Vec<(u32, u32)>,
    pub picked: Vec<u32>,
    pub pickup_results: VecDeque<ItemResult>,
    pub identified: Vec<u32>,
    pub salvaged: Vec<(u32, u32)>,
    pub sold: Vec<u32>,
    pub vendor_id: u32,
    pub unassessed: HashSet<u32>,
    pub item_capacity: i32,
    pub corpse_capture_count: usize,
    pub item_properties: HashMap<u32, ItemProperties>,
    pub unreadable_properties: HashSet<u32>,
    pub open_result: ItemStatus,
    /// An accepted open is the client's walk to the corpse first: nothing
    /// is requested and the client is busy until the walk ends (the test
    /// ends it).
    pub open_walks_first: bool,
    pub one_description_at_a_time: bool,
    pub never_answers: HashSet<u32>,
    pub presented_object_id: u32,
    pub classifier: Option<ClassifierRegistry>,
    pub infos: Vec<String>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            navigation_position: Position::default(),
            navigation_available: false,
            intents: Vec::new(),
            movement_clears: 0,
            is_owned_inventory_complete: true,
            items_busy: false,
            name: "Tester".to_owned(),
            character_level: 0,
            main_pack_slots: 0,
            requested: 0,
            current: 0,
            corpses: Vec::new(),
            contents: Vec::new(),
            owned: Vec::new(),
            inventory_completion: InventoryCompletion::default(),
            appraisal_state: AppraisalState::default(),
            skills: Vec::new(),
            known_spell: None,
            spell_learned: false,
            members: Vec::new(),
            chat_messages: Vec::new(),
            opened: Vec::new(),
            closed: Vec::new(),
            used: Vec::new(),
            use_completion: ItemUseCompletion::default(),
            applied: Vec::new(),
            apply_results: VecDeque::new(),
            mana_drain_consumes_donor: false,
            unsettled_drains: Vec::new(),
            picked: Vec::new(),
            pickup_results: VecDeque::new(),
            identified: Vec::new(),
            salvaged: Vec::new(),
            sold: Vec::new(),
            vendor_id: 0,
            unassessed: HashSet::new(),
            item_capacity: 0,
            corpse_capture_count: 0,
            item_properties: HashMap::new(),
            unreadable_properties: HashSet::new(),
            open_result: ItemStatus::Started,
            open_walks_first: false,
            one_description_at_a_time: false,
            never_answers: HashSet::new(),
            presented_object_id: 0,
            classifier: None,
            infos: Vec::new(),
        }
    }
}

impl World {
    /// A world with these corpses.
    pub fn with_corpses(corpses: Vec<LootCorpse>) -> Self {
        Self {
            corpses,
            ..Self::default()
        }
    }

    /// The server answers the drains that went out.
    pub fn settle_mana_drains(&mut self) {
        for (stone, donor) in std::mem::take(&mut self.unsettled_drains) {
            self.owned = self
                .owned
                .iter()
                .filter(|item| item.object_id != donor)
                .map(|item| {
                    if item.object_id == stone {
                        InventoryItem {
                            effects: item.effects | 1,
                            ..item.clone()
                        }
                    } else {
                        item.clone()
                    }
                })
                .collect();
            self.use_completion = ItemUseCompletion {
                revision: self.use_completion.revision + 1,
                source_id: stone,
                target_id: donor,
                weenie_error: 0,
            };
        }
    }

    /// The host giving up on a question the server never answered.
    pub fn release_stalled_description(&mut self) {
        let awaiting = self.appraisal_state.awaiting_id;
        if awaiting == 0 {
            return;
        }
        self.appraisal_state = AppraisalState {
            revision: self.appraisal_state.revision + 1,
            awaiting_id: 0,
            last_abandoned_id: awaiting,
            ..self.appraisal_state.clone()
        };
    }

    /// An appraisal response landing: the completion signal advances; the
    /// examination window's target only when presented.
    pub fn complete_appraisal(&mut self, object_id: u32, present_in_ui: bool) {
        self.appraisal_state = AppraisalState {
            revision: self.appraisal_state.revision + 1,
            awaiting_id: 0,
            current_id: object_id,
            ..self.appraisal_state.clone()
        };
        if present_in_ui {
            self.presented_object_id = object_id;
        }
    }

    /// `new PluginAppraisalState(revision, awaiting, current)`.
    pub fn set_appraisal(&mut self, revision: i64, awaiting: u32, current: u32) {
        self.appraisal_state = AppraisalState {
            revision,
            awaiting_id: awaiting,
            current_id: current,
            last_abandoned_id: 0,
            current_object_unsuccessful: false,
        };
    }

    /// `new PluginInventoryCompletion(revision, Pickup, item, 0)`.
    pub fn picked_up(&mut self, revision: i64, object_id: u32) {
        self.inventory_completion = InventoryCompletion {
            revision,
            kind: ac_plugin_abi::items::InventoryCommandKind::Pickup,
            source_id: object_id,
            weenie_error: 0,
        };
    }

    fn properties_of(&self, object_id: u32) -> Option<ItemProperties> {
        if self.unreadable_properties.contains(&object_id) {
            return None;
        }
        Some(
            self.item_properties
                .get(&object_id)
                .cloned()
                .unwrap_or_default(),
        )
    }
}

impl SpellCatalog for World {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.known_spell
            .clone()
            .filter(|spell| spell.spell_id == spell_id)
    }
}

impl Automation for World {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: PLAYER,
            name: self.name.clone(),
            level: self.character_level,
            main_pack_free_slots: self.main_pack_slots,
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.skills.clone()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::default()
    }

    fn cast(&mut self, _spell_id: u32, _target: Option<u32>) -> bool {
        false
    }

    fn combat_mode(&mut self) -> CombatMode {
        CombatMode::default()
    }

    fn enter_combat_mode(&mut self, _mode: CombatMode) -> CombatResult {
        CombatResult {
            status: CombatStatus::default(),
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
        self.owned.clone()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.use_completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.used.push(object_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.applied.push((object_id, target_id));
        let answer = self
            .apply_results
            .pop_front()
            .unwrap_or_else(|| ItemResult::of(ItemStatus::Started));
        if answer.accepted() && self.mana_drain_consumes_donor {
            self.unsettled_drains.push((object_id, target_id));
        }
        answer
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.properties_of(object_id)
    }

    fn objects_available(&mut self) -> bool {
        true
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        if object_id == PLAYER {
            return Some(WorldObject {
                object_id: PLAYER,
                weenie_class_id: 1,
                name: self.name.clone(),
                object_class: ObjectClass::Player,
                items_capacity: self.item_capacity,
                ..WorldObject::default()
            });
        }
        if let Some(item) = self.owned.iter().find(|item| item.object_id == object_id) {
            return Some(WorldObject {
                object_id: item.object_id,
                weenie_class_id: item.weenie_class_id,
                name: item.name.clone(),
                object_class: ObjectClass::Unknown,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                last_id_time: i32::from(!self.unassessed.contains(&item.object_id)),
                ..WorldObject::default()
            });
        }
        self.corpses
            .iter()
            .find(|corpse| corpse.object_id == object_id)
            .map(|corpse| WorldObject {
                object_id: corpse.object_id,
                weenie_class_id: corpse.weenie_class_id,
                name: corpse.name.clone(),
                object_class: ObjectClass::Unknown,
                last_id_time: i32::from(corpse.is_identified),
                ..WorldObject::default()
            })
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.loot_identify(object_id)
    }

    fn fellowship_members(&mut self) -> Vec<FellowMember> {
        self.members.clone()
    }

    fn in_fellowship(&mut self) -> bool {
        !self.members.is_empty()
    }

    fn position(&mut self) -> Position {
        self.navigation_position.clone()
    }

    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        NavigationSnapshot {
            is_available: self.navigation_available,
            position: self.navigation_position.clone(),
            ..NavigationSnapshot::default()
        }
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.intents.push(intent);
        NavigationStatus::Accepted
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.movement_clears += 1;
        NavigationStatus::Accepted
    }

    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.chat_messages
            .iter()
            .filter(|message| message.sequence > after_sequence)
            .cloned()
            .collect()
    }

    fn post_system_message(&mut self, _text: &str) {}

    fn log_info(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }

    fn log_warn(&mut self, _text: &str) {}
}

impl LootHost for World {
    fn loot_available(&mut self) -> bool {
        true
    }

    fn requested_container_id(&mut self) -> u32 {
        self.requested
    }

    fn current_container_id(&mut self) -> u32 {
        self.current
    }

    fn current_contents_ready(&mut self) -> bool {
        true
    }

    fn appraisal(&mut self) -> AppraisalState {
        self.appraisal_state.clone()
    }

    fn last_inventory_completion(&mut self) -> InventoryCompletion {
        self.inventory_completion.clone()
    }

    fn loot_corpses(&mut self, maximum_distance: f32) -> Vec<LootCorpse> {
        self.corpse_capture_count += 1;
        self.corpses
            .iter()
            .filter(|corpse| corpse.distance <= maximum_distance)
            .cloned()
            .collect()
    }

    fn loot_contents(&mut self) -> Vec<InventoryItem> {
        self.contents.clone()
    }

    fn loot_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.properties_of(object_id)
    }

    fn loot_open(&mut self, container_id: u32) -> ItemResult {
        if self.open_walks_first && self.items_busy {
            return ItemResult::of(ItemStatus::Busy);
        }
        self.opened.push(container_id);
        if self.open_result != ItemStatus::Started {
            return ItemResult::of(self.open_result);
        }
        if self.open_walks_first {
            self.items_busy = true;
            return ItemResult::of(ItemStatus::Started);
        }
        self.requested = container_id;
        ItemResult::of(ItemStatus::Started)
    }

    fn loot_close(&mut self, container_id: u32) -> ItemResult {
        self.closed.push(container_id);
        ItemResult::of(ItemStatus::Started)
    }

    /// The host's one-question-at-a-time description channel, when
    /// modelled: while an answer is outstanding every other question is
    /// answered busy.
    fn loot_identify(&mut self, object_id: u32) -> ItemResult {
        if self.one_description_at_a_time && self.appraisal_state.awaiting_id != 0 {
            return ItemResult::of(ItemStatus::Busy);
        }
        self.identified.push(object_id);
        let last_abandoned = if self.appraisal_state.last_abandoned_id == object_id {
            0
        } else {
            self.appraisal_state.last_abandoned_id
        };
        self.appraisal_state = AppraisalState {
            revision: self.appraisal_state.revision + 1,
            awaiting_id: object_id,
            last_abandoned_id: last_abandoned,
            ..self.appraisal_state.clone()
        };
        if self.one_description_at_a_time && !self.never_answers.contains(&object_id) {
            self.complete_appraisal(object_id, false);
        }
        ItemResult::of(ItemStatus::Started)
    }

    fn loot_pickup(&mut self, object_id: u32) -> ItemResult {
        self.picked.push(object_id);
        self.pickup_results
            .pop_front()
            .unwrap_or_else(|| ItemResult::of(ItemStatus::Started))
    }

    fn owned_inventory_complete(&mut self) -> bool {
        self.is_owned_inventory_complete
    }

    fn salvage(&mut self, tool_id: u32, item_ids: &[u32]) -> ItemResult {
        for item_id in item_ids {
            self.salvaged.push((tool_id, *item_id));
        }
        ItemResult::of(ItemStatus::Started)
    }

    fn sell(&mut self, object_id: u32) -> ItemResult {
        self.sold.push(object_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.vendor_id
    }

    fn is_spell_known(&mut self, spell_id: u32) -> bool {
        self.spell_learned
            && self
                .known_spell
                .as_ref()
                .is_some_and(|spell| spell.spell_id == spell_id)
    }

    fn objects(&mut self) -> Vec<WorldObject> {
        self.owned
            .iter()
            .map(|item| WorldObject {
                object_id: item.object_id,
                weenie_class_id: item.weenie_class_id,
                name: item.name.clone(),
                object_class: item.object_class,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                ..WorldObject::default()
            })
            .collect()
    }

    fn classify(
        &mut self,
        classifier_id: &str,
        _context: LootContext,
    ) -> Option<LootClassification> {
        let registry = self.classifier.as_mut()?;
        if !registry.id.eq_ignore_ascii_case(classifier_id) {
            return None;
        }
        registry.classification_count += 1;
        Some(registry.result.clone())
    }

    fn notify_looted(&mut self, classifier_id: &str, item: LootedItem) -> bool {
        let Some(registry) = self.classifier.as_mut() else {
            return false;
        };
        if !registry.id.eq_ignore_ascii_case(classifier_id) {
            return false;
        }
        registry.looted.push(item);
        true
    }

    fn notify_item_removed(&mut self, classifier_id: &str, object_id: u32) -> bool {
        let Some(registry) = self.classifier.as_mut() else {
            return false;
        };
        if !registry.id.eq_ignore_ascii_case(classifier_id) {
            return false;
        }
        registry.removed.push(object_id);
        true
    }

    fn log_error(&mut self, _text: &str) {}
}

/// The lines a looter writes, kept for the test.
pub type Lines = Rc<RefCell<Vec<String>>>;

/// Points the looter's log at a list the test reads.
pub fn capture_log(controller: &mut LootController) -> Lines {
    let lines: Lines = Rc::default();
    let sink = Rc::clone(&lines);
    controller.log = Some(Box::new(move |line: &str| {
        sink.borrow_mut().push(line.to_owned())
    }));
    lines
}

/// A shared lock table bound to the looter.
pub fn bind_locks(controller: &mut LootController) -> SharedActionLocks {
    let locks = ActionLockTable::shared();
    controller.bind_action_locks(Rc::clone(&locks));
    locks
}

/// `new LootSettings { Enabled = true }` with these rules.
pub fn enabled_settings(rules: Vec<LootRule>) -> LootSettings {
    LootSettings {
        enabled: true,
        rules,
        ..LootSettings::default()
    }
}

/// Whether any line contains `fragment`.
pub fn contains_line(lines: &Lines, fragment: &str) -> bool {
    lines.borrow().iter().any(|line| line.contains(fragment))
}

/// The lines containing `fragment`.
pub fn lines_with(lines: &Lines, fragment: &str) -> Vec<String> {
    lines
        .borrow()
        .iter()
        .filter(|line| line.contains(fragment))
        .cloned()
        .collect()
}
