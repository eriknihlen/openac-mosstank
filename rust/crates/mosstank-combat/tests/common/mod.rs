//! The combat tests' client (`CombatControllerTests.FakeAutomation`), with
//! the fixtures the C# file builds its cases from.
#![allow(dead_code, clippy::struct_excessive_bools)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill, Training};
use ac_plugin_abi::combat::{
    AttackHeight, CombatMode, CombatResult, CombatSnapshot, CombatStatus, CombatTarget,
};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentObservation, EquipmentPlacement, EquipmentResult, EquipmentStatus,
    InventoryItem, ItemResult, ItemStatus, ItemUseCompletion,
};
use ac_plugin_abi::magic::{CastCompletion, CastGate, SpellInfo, TrackedEnchantment};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::projectile::{
    ProjectilePathKind, ProjectilePathQuery, ProjectilePathResult, ProjectilePathStatus,
};
use ac_plugin_abi::social::{BusyState, SelectionAction};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::ui::ProjectileSample;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, Position, WorldObject};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_combat::MacroLog;
use mosstank_meta::scheduler::MacroLogChannel;
use openac_plugin_compat::text::ordinal;

/// The other clients on this computer (`FakePeerNetwork`).
#[derive(Debug, Default)]
pub struct FakePeers {
    pub clients: Vec<NetworkClient>,
    pub casts: Vec<PeerCast>,
    pub capture_requests: Vec<i64>,
    pub attempts: Vec<(u32, u32, i32)>,
    pub successes: Vec<(u32, u32, i32, f64)>,
    pub client_capture_count: i32,
}

/// The client.
#[derive(Debug)]
pub struct FakeAutomation {
    pub is_available: bool,
    pub peers: FakePeers,
    pub reported: Vec<(u32, u32, f64)>,
    pub chat_input_active: bool,
    pub busy_state: BusyState,
    pub combat_snapshot: CombatSnapshot,
    pub targets: Vec<CombatTarget>,
    pub known_attack_spells: Vec<SpellInfo>,
    pub known_combat_spells: Vec<SpellInfo>,
    pub known_combat_spell_reads: i32,
    pub spell_lookup: Vec<SpellInfo>,
    pub last_cast_completion: CastCompletion,
    pub last_begin_target: u32,
    pub unattackable_targets: HashSet<u32>,
    pub begin_targets: Vec<u32>,
    pub tracks_attack_requests: bool,
    pub begin_count: i32,
    pub release_count: i32,
    pub abort_count: i32,
    pub last_targeted_cast: (u32, u32),
    pub last_untargeted_cast: u32,
    pub cast_spell_ids: Vec<u32>,
    pub equipment_items: Vec<EquipmentItem>,
    pub unowned_equipment_ids: HashSet<u32>,
    pub world_only_object_ids: HashSet<u32>,
    pub last_equip_object_id: u32,
    pub item_entries: Vec<InventoryItem>,
    pub last_used_item: u32,
    pub last_applied_item: (u32, u32),
    pub apply_count: i32,
    pub last_item_completion: ItemUseCompletion,
    pub chat_messages: Vec<ChatLine>,
    pub last_begin_power: f32,
    pub last_begin_height: AttackHeight,
    pub navigation_snapshot: NavigationSnapshot,
    pub ignore_mode_changes: bool,
    pub mode_change_requests: i32,
    pub navigation_objects: HashMap<u32, NavigationObject>,
    pub movement_intents: Vec<MovementIntent>,
    pub clear_movement_count: i32,
    pub projectile_path: ProjectilePathResult,
    pub last_projectile_target: u32,
    pub last_projectile_height: AttackHeight,
    pub last_projectile_kind: ProjectilePathKind,
    pub projectile_path_checks: i32,
    pub last_projectile_radius: f32,
    pub last_projectile_step_distance: f32,
    pub projectile_paths: HashMap<u32, ProjectilePathResult>,
    pub shown_projectile_debug_samples: Vec<ProjectileSample>,
    pub selection_actions: Vec<SelectionAction>,
    pub call_log: Vec<String>,
    pub simulate_async_equip: bool,
    pub pending_equip_object_id: Option<u32>,
    pub equipment_available: bool,
    pub observations: Vec<EquipmentObservation>,
    pub capture_owned_equipment_count: i32,
    pub capture_owned_items_count: i32,
    pub defer_mode_confirmation: bool,
    pub withhold_mode_echo: bool,
    pub pending_mode: Option<CombatMode>,
    pub mode_command_unavailable: bool,
    pub armed_target: u32,
    pub unreleasable_targets: HashSet<u32>,
    pub dismissed_ghosts: Vec<u32>,
    pub ghost_dismissal_accepted: bool,
    pub faced_headings: Vec<f32>,
    pub is_casting: bool,
    pub missing_component_spell_ids: HashSet<u32>,
    pub cast_gates: HashMap<u32, CastGate>,
    pub posted_system_messages: Vec<String>,
    pub character_name: String,
    pub world: String,
    pub character_skills: Vec<Skill>,
    pub selected: Option<u32>,
    pub infos: Vec<String>,
}

impl Default for FakeAutomation {
    fn default() -> Self {
        Self {
            is_available: true,
            peers: FakePeers::default(),
            reported: Vec::new(),
            chat_input_active: false,
            busy_state: BusyState::default(),
            combat_snapshot: CombatSnapshot::default(),
            targets: Vec::new(),
            known_attack_spells: Vec::new(),
            known_combat_spells: Vec::new(),
            known_combat_spell_reads: 0,
            spell_lookup: Vec::new(),
            last_cast_completion: CastCompletion::default(),
            last_begin_target: 0,
            unattackable_targets: HashSet::new(),
            begin_targets: Vec::new(),
            tracks_attack_requests: false,
            begin_count: 0,
            release_count: 0,
            abort_count: 0,
            last_targeted_cast: (0, 0),
            last_untargeted_cast: 0,
            cast_spell_ids: Vec::new(),
            equipment_items: Vec::new(),
            unowned_equipment_ids: HashSet::new(),
            world_only_object_ids: HashSet::new(),
            last_equip_object_id: 0,
            item_entries: Vec::new(),
            last_used_item: 0,
            last_applied_item: (0, 0),
            apply_count: 0,
            last_item_completion: ItemUseCompletion::default(),
            chat_messages: Vec::new(),
            last_begin_power: 0.0,
            last_begin_height: AttackHeight::Medium,
            navigation_snapshot: NavigationSnapshot::default(),
            ignore_mode_changes: false,
            mode_change_requests: 0,
            navigation_objects: HashMap::new(),
            movement_intents: Vec::new(),
            clear_movement_count: 0,
            projectile_path: ProjectilePathResult {
                status: ProjectilePathStatus::Clear,
                ..ProjectilePathResult::default()
            },
            last_projectile_target: 0,
            last_projectile_height: AttackHeight::Medium,
            last_projectile_kind: ProjectilePathKind::Straight,
            projectile_path_checks: 0,
            last_projectile_radius: 0.0,
            last_projectile_step_distance: 0.0,
            projectile_paths: HashMap::new(),
            shown_projectile_debug_samples: Vec::new(),
            selection_actions: Vec::new(),
            call_log: Vec::new(),
            simulate_async_equip: false,
            pending_equip_object_id: None,
            equipment_available: true,
            observations: Vec::new(),
            capture_owned_equipment_count: 0,
            capture_owned_items_count: 0,
            defer_mode_confirmation: false,
            withhold_mode_echo: false,
            pending_mode: None,
            mode_command_unavailable: false,
            armed_target: 0,
            unreleasable_targets: HashSet::new(),
            dismissed_ghosts: Vec::new(),
            ghost_dismissal_accepted: true,
            faced_headings: Vec::new(),
            is_casting: false,
            missing_component_spell_ids: HashSet::new(),
            cast_gates: HashMap::new(),
            posted_system_messages: Vec::new(),
            character_name: "Fixture".to_owned(),
            world: "FixtureWorld".to_owned(),
            character_skills: vec![skill(54, "Summoning", Training::Trained, 300)],
            selected: None,
            infos: Vec::new(),
        }
    }
}

impl FakeAutomation {
    /// The power bar reaching the top.
    pub fn fill_power_bar(&mut self) {
        self.combat_snapshot.power_bar_level = 1.0;
    }

    /// A placement event, as the host raises it.
    pub fn emit_placement(&mut self, observation: EquipmentObservation) {
        self.observations.push(observation);
    }

    /// The server takes the equip that was asked for.
    pub fn confirm_pending_equip(&mut self) {
        let Some(object_id) = self.pending_equip_object_id.take() else {
            return;
        };
        self.apply_confirmed_equip(object_id);
    }

    fn apply_confirmed_equip(&mut self, object_id: u32) {
        if !self
            .equipment_items
            .iter()
            .any(|item| item.object_id == object_id)
        {
            return;
        }
        let previous = self.equipment_items.clone();
        self.equipment_items = mark_equipped(&previous, object_id);
        for before in &previous {
            let after = self
                .equipment_items
                .iter()
                .find(|item| item.object_id == before.object_id)
                .map_or(0, |item| item.equipped_location);
            if before.equipped_location != 0 && after == 0 {
                self.observations.push(EquipmentObservation {
                    object_id: before.object_id,
                    equipped_location: 0,
                    is_removal: true,
                    is_initial_placement: false,
                });
            }
        }
        let location = self
            .equipment_items
            .iter()
            .find(|item| item.object_id == object_id)
            .map_or(0, |item| item.equipped_location);
        self.observations.push(EquipmentObservation {
            object_id,
            equipped_location: location,
            is_removal: false,
            is_initial_placement: false,
        });
    }

    /// The server takes the stance change that was asked for.
    pub fn confirm_pending_mode_change(&mut self) {
        let Some(mode) = self.pending_mode.take() else {
            return;
        };
        self.echo_mode(mode);
    }

    fn echo_mode(&mut self, mode: CombatMode) {
        let snapshot = &mut self.combat_snapshot;
        snapshot.mode = mode;
        snapshot.qualified_self_motion_revision += 1;
        snapshot.qualified_self_motion_age_seconds = 0.0;
        snapshot.server_mode = if snapshot.server_mode == CombatMode::Unknown {
            CombatMode::Unknown
        } else {
            mode
        };
    }
}

/// A slot holds one thing: wielding an item takes whatever was there out.
fn mark_equipped(items: &[EquipmentItem], object_id: u32) -> Vec<EquipmentItem> {
    let slot = items
        .iter()
        .find(|item| item.object_id == object_id)
        .map_or(0, |item| item.valid_locations);
    items
        .iter()
        .map(|item| {
            let mut item = item.clone();
            if item.object_id == object_id {
                item.equipped_location = slot;
            } else if item.equipped_location & slot != 0 {
                item.equipped_location = 0;
            }
            item
        })
        .collect()
}

impl SpellCatalog for FakeAutomation {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        if let Some(spell) = self
            .known_attack_spells
            .iter()
            .find(|spell| spell.spell_id == spell_id)
        {
            return Some(spell.clone());
        }
        self.known_combat_spells
            .iter()
            .chain(&self.spell_lookup)
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
    }

    fn known_attack_spells(&mut self) -> Vec<SpellInfo> {
        self.known_attack_spells.clone()
    }

    fn known_combat_spells(&mut self) -> Vec<SpellInfo> {
        self.known_combat_spell_reads += 1;
        self.known_combat_spells.clone()
    }
}

impl Automation for FakeAutomation {
    fn is_available(&mut self) -> bool {
        self.is_available
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            name: self.character_name.clone(),
            world: self.world.clone(),
            summoning_mastery: 3,
            health: (100, 100, 100),
            stamina: (100, 100, 100),
            mana: (100, 100, 100),
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.character_skills.clone()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, spell_id: u32, _target: Option<u32>) -> CastGate {
        self.cast_gates
            .get(&spell_id)
            .copied()
            .unwrap_or(CastGate::Ready)
    }

    fn cast(&mut self, spell_id: u32, target: Option<u32>) -> bool {
        match target {
            Some(target) => self.last_targeted_cast = (spell_id, target),
            None => self.last_untargeted_cast = spell_id,
        }
        self.cast_spell_ids.push(spell_id);
        true
    }

    fn has_components(&mut self, spell_id: u32) -> bool {
        !self.missing_component_spell_ids.contains(&spell_id)
    }

    fn last_cast_completion(&mut self) -> CastCompletion {
        self.last_cast_completion.clone()
    }

    fn combat_mode(&mut self) -> CombatMode {
        self.combat_snapshot.mode
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.mode_change_requests += 1;
        self.call_log
            .push(format!("EnterMode:{}", mosstank_combat::mode_name(mode)));
        if self.mode_command_unavailable {
            return CombatResult::of(CombatStatus::Unavailable);
        }
        if self.ignore_mode_changes {
            return CombatResult::of(CombatStatus::ModeChangeSent);
        }
        if self.defer_mode_confirmation {
            self.pending_mode = Some(mode);
            return CombatResult::of(CombatStatus::ModeChangeSent);
        }
        if self.withhold_mode_echo {
            self.combat_snapshot.mode = mode;
            self.pending_mode = Some(mode);
            return CombatResult::of(CombatStatus::ModeChangeSent);
        }
        self.echo_mode(mode);
        CombatResult::of(CombatStatus::ModeChangeSent)
    }

    fn items_available(&mut self) -> bool {
        true
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.capture_owned_items_count += 1;
        self.item_entries.clone()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.last_item_completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.last_used_item = object_id;
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.last_applied_item = (object_id, target_id);
        self.apply_count += 1;
        ItemResult::of(ItemStatus::Started)
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.item_entries
            .iter()
            .any(|item| item.object_id == object_id)
            .then(ItemProperties::default)
    }

    fn objects_available(&mut self) -> bool {
        true
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        if let Some(item) = self
            .equipment_items
            .iter()
            .find(|item| item.object_id == object_id)
        {
            return Some(WorldObject {
                object_id: item.object_id,
                name: item.name.clone(),
                object_class: item.object_class,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                last_id_time: 1,
                is_owned: !self.unowned_equipment_ids.contains(&item.object_id),
                ..WorldObject::default()
            });
        }
        if let Some(target) = self.targets.iter().find(|t| t.object_id == object_id) {
            return Some(WorldObject {
                object_id: target.object_id,
                weenie_class_id: target.weenie_class_id,
                name: target.name.clone(),
                object_class: ObjectClass::Unknown,
                ..WorldObject::default()
            });
        }
        if self.world_only_object_ids.contains(&object_id) {
            let inventory = self
                .item_entries
                .iter()
                .find(|item| item.object_id == object_id);
            return Some(WorldObject {
                object_id,
                name: inventory.map_or_else(|| "Inventory item".to_owned(), |i| i.name.clone()),
                object_class: ObjectClass::Unknown,
                item_type: inventory.map_or(0, |i| i.item_type),
                container_id: 1,
                last_id_time: 1,
                is_owned: true,
                ..WorldObject::default()
            });
        }
        None
    }

    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.chat_messages
            .iter()
            .filter(|message| message.sequence > after_sequence)
            .cloned()
            .collect()
    }

    fn post_system_message(&mut self, text: &str) {
        self.posted_system_messages.push(text.to_owned());
    }

    fn chat_input_active(&mut self) -> bool {
        self.chat_input_active
    }

    fn attributes(&mut self) -> Vec<Attribute> {
        Vec::new()
    }

    fn timed_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        Vec::new()
    }

    fn is_casting(&mut self) -> bool {
        self.is_casting
    }

    fn combat_snapshot(&mut self) -> CombatSnapshot {
        self.combat_snapshot.clone()
    }

    fn hostile_targets(&mut self, _maximum_metres: f32) -> Vec<CombatTarget> {
        self.targets.clone()
    }

    fn begin_attack(&mut self, target_id: u32, height: AttackHeight, power: f32) -> CombatResult {
        let snapshot = &self.combat_snapshot;
        if (snapshot.request_in_progress
            || snapshot.server_response_pending
            || snapshot.repeat_attack_in_progress)
            && self.armed_target == target_id
        {
            return CombatResult::of(CombatStatus::Busy);
        }
        self.begin_targets.push(target_id);
        if self.unattackable_targets.contains(&target_id) {
            return CombatResult {
                status: CombatStatus::Refused,
                notice: Some(
                    "The target cannot be attacked: it is out of sight or out of play.".to_owned(),
                ),
            };
        }
        self.last_begin_target = target_id;
        self.last_begin_power = power;
        self.last_begin_height = height;
        self.begin_count += 1;
        self.armed_target = target_id;
        if self.tracks_attack_requests {
            let snapshot = &mut self.combat_snapshot;
            snapshot.request_in_progress = true;
            snapshot.build_in_progress = true;
            snapshot.power_bar_level = 0.0;
            snapshot.server_response_pending = false;
            snapshot.repeat_attack_in_progress = false;
        }
        CombatResult::of(CombatStatus::Started)
    }

    fn release_attack(&mut self) -> CombatResult {
        self.release_count += 1;
        if self.unreleasable_targets.contains(&self.armed_target) {
            self.armed_target = 0;
            if self.tracks_attack_requests {
                let snapshot = &mut self.combat_snapshot;
                snapshot.request_in_progress = false;
                snapshot.build_in_progress = false;
                snapshot.server_response_pending = false;
                snapshot.power_bar_level = 0.0;
            }
            return CombatResult {
                status: CombatStatus::Refused,
                notice: Some(
                    "The target cannot be attacked: it is out of sight or out of play.".to_owned(),
                ),
            };
        }
        if self.tracks_attack_requests {
            let snapshot = &mut self.combat_snapshot;
            snapshot.request_in_progress = false;
            snapshot.build_in_progress = false;
            snapshot.server_response_pending = true;
        }
        CombatResult::of(CombatStatus::Released)
    }

    fn abort_attack(&mut self) -> CombatResult {
        self.abort_count += 1;
        self.armed_target = 0;
        if self.tracks_attack_requests {
            let snapshot = &mut self.combat_snapshot;
            snapshot.request_in_progress = false;
            snapshot.build_in_progress = false;
            snapshot.server_response_pending = false;
            snapshot.repeat_attack_in_progress = false;
            snapshot.power_bar_level = 0.0;
        }
        CombatResult::of(CombatStatus::Stopped)
    }

    fn dismiss_ghost(&mut self, target_id: u32) -> CombatResult {
        self.dismissed_ghosts.push(target_id);
        CombatResult::of(if self.ghost_dismissal_accepted {
            CombatStatus::Stopped
        } else {
            CombatStatus::Unavailable
        })
    }

    fn combat_control(&mut self, acquire: bool) -> bool {
        acquire
    }

    fn equipment_available(&mut self) -> bool {
        self.equipment_available
    }

    fn equipment_busy(&mut self) -> bool {
        self.simulate_async_equip && self.pending_equip_object_id.is_some()
    }

    /// Held first, then by name, then by id: an order that turns over the
    /// moment something is wielded.
    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        self.capture_owned_equipment_count += 1;
        let mut projected = self.equipment_items.clone();
        projected.sort_by(|left, right| {
            (right.equipped_location != 0)
                .cmp(&(left.equipped_location != 0))
                .then_with(|| ordinal::compare(&left.name, &right.name))
                .then(left.object_id.cmp(&right.object_id))
        });
        projected
    }

    fn equipment_placements(&mut self) -> Vec<EquipmentPlacement> {
        self.equipment_items
            .iter()
            .map(|item| EquipmentPlacement {
                object_id: item.object_id,
                equipped_location: item.equipped_location,
            })
            .collect()
    }

    fn take_equipment_observations(&mut self) -> Vec<EquipmentObservation> {
        std::mem::take(&mut self.observations)
    }

    fn equip(&mut self, object_id: u32) -> EquipmentResult {
        self.last_equip_object_id = object_id;
        self.call_log.push(format!("Equip:{object_id:08X}"));
        if self.simulate_async_equip {
            self.pending_equip_object_id = Some(object_id);
        } else {
            self.apply_confirmed_equip(object_id);
        }
        EquipmentResult {
            status: EquipmentStatus::Started,
            notice: None,
        }
    }

    fn equip_secondary(&mut self, object_id: u32) -> EquipmentResult {
        self.last_equip_object_id = object_id;
        self.call_log
            .push(format!("EquipSecondary:{object_id:08X}"));
        self.apply_confirmed_equip(object_id);
        EquipmentResult {
            status: EquipmentStatus::Started,
            notice: None,
        }
    }

    fn move_item(&mut self, object_id: u32, _container_id: u32) -> ItemResult {
        self.call_log.push(format!("Move:{object_id:08X}"));
        for item in &mut self.equipment_items {
            if item.object_id == object_id {
                item.equipped_location = 0;
            }
        }
        self.observations.push(EquipmentObservation {
            object_id,
            equipped_location: 0,
            is_removal: true,
            is_initial_placement: false,
        });
        ItemResult::of(ItemStatus::Started)
    }

    fn busy_state(&mut self) -> BusyState {
        self.busy_state.clone()
    }

    fn projectiles_available(&mut self) -> bool {
        true
    }

    fn evaluate_projectile_path(&mut self, query: &ProjectilePathQuery) -> ProjectilePathResult {
        self.last_projectile_target = query.target_id;
        self.last_projectile_height = query.target_height;
        self.last_projectile_kind = query.kind;
        self.last_projectile_radius = query.projectile_radius;
        self.last_projectile_step_distance = query.step_distance;
        self.projectile_path_checks += 1;
        self.projectile_paths
            .get(&query.target_id)
            .cloned()
            .unwrap_or_else(|| self.projectile_path.clone())
    }

    fn show_projectile_samples(&mut self, samples: &[ProjectileSample]) {
        self.shown_projectile_debug_samples = samples.to_vec();
    }

    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        self.navigation_snapshot.clone()
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        self.navigation_objects.get(&object_id).cloned()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.movement_intents.push(intent);
        NavigationStatus::Accepted
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.clear_movement_count += 1;
        NavigationStatus::Accepted
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.faced_headings.push(heading_degrees);
        NavigationStatus::Accepted
    }

    fn selected_object(&mut self) -> Option<u32> {
        self.selected
    }

    fn select(&mut self, _object_id: u32) -> bool {
        true
    }

    fn selection_step(&mut self, action: SelectionAction) -> bool {
        self.selection_actions.push(action);
        true
    }

    fn tracked_enchantments(&mut self, _target_id: u32) -> Vec<TrackedEnchantment> {
        Vec::new()
    }

    fn report_cast(&mut self, target_id: u32, spell_id: u32, duration_seconds: f64) -> bool {
        self.reported.push((target_id, spell_id, duration_seconds));
        true
    }

    fn network_available(&mut self) -> bool {
        true
    }

    fn network_clients(&mut self) -> Vec<NetworkClient> {
        self.peers.client_capture_count += 1;
        self.peers.clients.clone()
    }

    fn announce_cast_attempt(&mut self, target_id: u32, spell_id: u32, skill: i32) -> bool {
        self.peers.attempts.push((target_id, spell_id, skill));
        true
    }

    fn announce_cast_success(
        &mut self,
        target_id: u32,
        spell_id: u32,
        skill: i32,
        duration_seconds: f64,
    ) -> bool {
        self.peers
            .successes
            .push((target_id, spell_id, skill, duration_seconds));
        true
    }

    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        self.peers.capture_requests.push(after_sequence);
        let mut casts: Vec<PeerCast> = self
            .peers
            .casts
            .iter()
            .filter(|cast| cast.sequence > after_sequence)
            .cloned()
            .collect();
        casts.sort_by_key(|cast| cast.sequence);
        casts
    }

    fn log_info(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }

    fn log_warn(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }
}

/// A skill.
pub fn skill(skill_id: u32, name: &str, training: Training, current: u32) -> Skill {
    Skill {
        skill_id,
        name: name.to_owned(),
        training,
        current,
        base: current,
        ..Skill::default()
    }
}

/// A monster (`Target`): its weenie class is its id plus 1000.
pub fn target(id: u32, name: &str, distance: f32, angle: f32) -> CombatTarget {
    target_with_health(id, name, distance, angle, 1.0)
}

/// A monster with a health fraction.
pub fn target_with_health(
    id: u32,
    name: &str,
    distance: f32,
    angle: f32,
    health: f32,
) -> CombatTarget {
    CombatTarget {
        object_id: id,
        name: name.to_owned(),
        weenie_class_id: id + 1000,
        distance,
        relative_angle_degrees: angle,
        is_health_known: true,
        health_fraction: health,
        seconds_since_health_update: f64::INFINITY,
        ..CombatTarget::default()
    }
}

/// A war spell (`Spell`).
pub fn spell(id: u32, name: &str) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: 1,
        tier: 8,
        difficulty: 350,
        mana_cost: 30,
        duration_seconds: 0.0,
        school: 34,
        base_range_constant: 80.0,
        saying: "testwords".to_owned(),
        ..SpellInfo::default()
    }
}

/// The slot a quiver goes in.
pub const AMMUNITION_SLOT: u32 = 0x0080_0000;

/// One piece of equipment (`Equipment`), with the C# helper's defaults.
#[derive(Debug, Clone)]
pub struct Gear {
    pub id: u32,
    pub name: &'static str,
    pub damage_type: i32,
    pub damage: i32,
    pub equipped_location: u32,
    pub item_type: u32,
    pub combat_use: u8,
    pub ammo_type: u32,
    pub stack_size: i32,
    pub valid_locations: u32,
}

impl Gear {
    /// The helper's defaults for an id, a name and a damage type.
    pub fn new(id: u32, name: &'static str, damage_type: i32) -> Self {
        Self {
            id,
            name,
            damage_type,
            damage: 20,
            equipped_location: 0,
            item_type: 1,
            combat_use: 1,
            ammo_type: 0,
            stack_size: 1,
            valid_locations: 0x0010_0000,
        }
    }

    pub fn equipped(mut self, location: u32) -> Self {
        self.equipped_location = location;
        self
    }

    pub fn item_type(mut self, item_type: u32) -> Self {
        self.item_type = item_type;
        self
    }

    pub fn valid(mut self, locations: u32) -> Self {
        self.valid_locations = locations;
        self
    }

    pub fn damage(mut self, damage: i32) -> Self {
        self.damage = damage;
        self
    }

    pub fn combat_use(mut self, combat_use: u8) -> Self {
        self.combat_use = combat_use;
        self
    }

    pub fn ammo(mut self, ammo_type: u32, stack_size: i32) -> Self {
        self.ammo_type = ammo_type;
        self.stack_size = stack_size;
        self
    }

    pub fn build(self) -> EquipmentItem {
        EquipmentItem {
            object_id: self.id,
            name: self.name.to_owned(),
            item_type: self.item_type,
            valid_locations: self.valid_locations,
            equipped_location: self.equipped_location,
            container_id: 1,
            wielder_id: 0,
            combat_use: self.combat_use,
            damage_type: self.damage_type,
            weapon_skill: 44,
            damage: self.damage,
            damage_variance: 0.25,
            ammo_type: self.ammo_type,
            stack_size: self.stack_size,
            object_class: if self.item_type & 0x8000 != 0 {
                ObjectClass::WandStaffOrb
            } else if self.item_type & 0x100 != 0 {
                ObjectClass::MissileWeapon
            } else {
                ObjectClass::MeleeWeapon
            },
            ..EquipmentItem::default()
        }
    }
}

/// A wand.
pub fn wand(id: u32, name: &'static str) -> Gear {
    Gear::new(id, name, 0).item_type(0x0000_8000)
}

/// The fixture's wielded caster.
pub fn wielded_caster(id: u32) -> EquipmentItem {
    wand(id, "Fixture Wand").equipped(0x0010_0000).build()
}

/// The fixture's wielded weapon (every element).
pub fn wielded_planned_weapon(id: u32) -> EquipmentItem {
    Gear::new(id, "Fixture Weapon", 0x007F)
        .equipped(0x0010_0000)
        .build()
}

/// A combat snapshot in melee (`Physical`).
pub fn physical() -> CombatSnapshot {
    physical_with(false, false, 0.0)
}

/// A melee snapshot with an attack's progress.
pub fn physical_with(request: bool, build: bool, bar: f32) -> CombatSnapshot {
    CombatSnapshot {
        selected_id: 0,
        mode: CombatMode::Melee,
        attack_height: AttackHeight::Medium,
        desired_power: 0.5,
        power_bar_level: bar,
        build_in_progress: build,
        request_in_progress: request,
        server_response_pending: false,
        repeat_attack_in_progress: false,
        ..CombatSnapshot::default()
    }
}

/// A snapshot in peace (`Peaceful`).
pub fn peaceful() -> CombatSnapshot {
    CombatSnapshot {
        mode: CombatMode::Peace,
        ..physical()
    }
}

/// A snapshot in a mode.
pub fn in_mode(mode: CombatMode) -> CombatSnapshot {
    CombatSnapshot { mode, ..physical() }
}

/// The navigation snapshot at a heading.
pub fn navigation_at(heading: f32) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: 1,
        position: Position {
            cell_id: 0x7F7F_0001,
            east_west: 0.0,
            north_south: 0.0,
            elevation: 0.0,
            heading_degrees: heading,
            is_outdoor: true,
        },
        is_moving: false,
        is_airborne: false,
        ..NavigationSnapshot::default()
    }
}

/// The lines a recording log kept.
pub type LogLines = Rc<RefCell<Vec<(MacroLogChannel, String)>>>;

/// A log that keeps its lines.
pub fn recording_log() -> (MacroLog, LogLines) {
    let lines: LogLines = Rc::default();
    let sink = lines.clone();
    let log: MacroLog = Rc::new(move |channel, text: &str| {
        sink.borrow_mut().push((channel, text.to_owned()));
    });
    (log, lines)
}
