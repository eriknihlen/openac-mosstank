//! The navigation tests' client (`NavigationTests.FakeAutomation`) and the
//! rig that hands it, the route and the controller to each call.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::magic::{CastCompletion, CastGate, CastResult, SpellInfo};
use ac_plugin_abi::navigation::{
    GoToReport, GoToState, MoveChannel, MoveDirection, MovePace, MoveUnit, MovementIntent,
    NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, Position};
use mosstank_automation::locks::SharedActionLocks;
use mosstank_automation::{Automation, CombatModePreparation, SpellCatalog};
use mosstank_formats::route::{
    ClientPathing, NavigationSettings, RouteMode, RouteWaypoint, RouteWaypointType,
};
use mosstank_nav::door_rule::DoorRuleHost;
use mosstank_nav::{NavigationAutomation, NavigationController, RouteTurn};

/// `NavigationTests.FakeMagic`: what a recall waypoint asks of the magic
/// side.
#[derive(Debug, Clone)]
pub struct FakeMagic {
    pub cast_spell_ids: Vec<u32>,
    pub requested_spell_ids: Vec<u32>,
    /// What the client answers a cast request with.
    pub request_answer: CastResult,
    /// Whether a cast is in the air right now.
    pub is_casting: bool,
    /// The latched outcome of the last cast that finished.
    pub last_completion: CastCompletion,
}

impl Default for FakeMagic {
    fn default() -> Self {
        Self {
            cast_spell_ids: Vec::new(),
            requested_spell_ids: Vec::new(),
            request_answer: CastResult::Sent,
            is_casting: false,
            last_completion: CastCompletion::default(),
        }
    }
}

/// `NavigationTests.FakeAutomation`.
#[derive(Debug)]
pub struct FakeAutomation {
    pub world_appraisal_available: bool,
    pub identified_world_objects: Vec<u32>,
    pub combat_mode: CombatMode,
    pub mode_requests: Vec<String>,
    pub posted_system_messages: Vec<String>,
    pub chat_input_active: bool,
    pub magic: Option<FakeMagic>,
    pub navigation_snapshot: NavigationSnapshot,
    pub objects: HashMap<u32, NavigationObject>,
    pub world_objects: Vec<NavigationObject>,
    pub intents: Vec<MovementIntent>,
    pub submitted_chat: Vec<String>,
    pub chat_messages: Vec<ChatLine>,
    pub used_objects: Vec<u32>,
    pub clear_count: u32,
    pub item_completion: ItemUseCompletion,
    pub active_vendor_object_id: u32,
    pub found_object: Option<NavigationObject>,
    pub find_name: Option<String>,
    pub go_to_requests: Vec<(Position, f32)>,
    pub go_to_answer: NavigationStatus,
    pub go_to_report: GoToReport,
    pub stop_go_to_calls: u32,
    pub faced_headings: Vec<f32>,
    /// The moves the client was asked to carry out, in order.
    pub moves: Vec<(MoveDirection, MovePace, f32, MoveUnit)>,
    /// The jumps it was asked for, by power.
    pub jumps: Vec<f32>,
    /// The channels it was asked to stop.
    pub stopped_moves: Vec<Option<MoveChannel>>,
}

impl Default for FakeAutomation {
    fn default() -> Self {
        Self {
            world_appraisal_available: false,
            identified_world_objects: Vec::new(),
            combat_mode: CombatMode::Unknown,
            mode_requests: Vec::new(),
            posted_system_messages: Vec::new(),
            chat_input_active: false,
            magic: None,
            navigation_snapshot: NavigationSnapshot::default(),
            objects: HashMap::new(),
            world_objects: Vec::new(),
            intents: Vec::new(),
            submitted_chat: Vec::new(),
            chat_messages: Vec::new(),
            used_objects: Vec::new(),
            clear_count: 0,
            item_completion: ItemUseCompletion::default(),
            active_vendor_object_id: 0,
            found_object: None,
            find_name: None,
            go_to_requests: Vec::new(),
            go_to_answer: NavigationStatus::Accepted,
            go_to_report: GoToReport::default(),
            stop_go_to_calls: 0,
            faced_headings: Vec::new(),
            moves: Vec::new(),
            jumps: Vec::new(),
            stopped_moves: Vec::new(),
        }
    }
}

impl FakeAutomation {
    /// A client standing at `snapshot`.
    pub fn at(snapshot: NavigationSnapshot) -> Self {
        Self {
            navigation_snapshot: snapshot,
            ..Self::default()
        }
    }

    /// The magic side, set up on first use.
    pub fn magic(&mut self) -> &mut FakeMagic {
        self.magic.get_or_insert_with(FakeMagic::default)
    }

    /// Ends the client's walk in `state`.
    pub fn end_go_to(&mut self, state: GoToState, reason: &str) {
        self.go_to_report.state = state;
        self.go_to_report.reason = Some(reason.to_owned());
    }
}

impl SpellCatalog for FakeAutomation {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, _spell_id: u32) -> Option<SpellInfo> {
        None
    }
}

impl Automation for FakeAutomation {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo::default()
    }

    fn skills(&mut self) -> Vec<Skill> {
        Vec::new()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::Refused
    }

    fn cast(&mut self, _spell_id: u32, _target: Option<u32>) -> bool {
        false
    }

    fn last_cast_completion(&mut self) -> CastCompletion {
        self.magic
            .as_ref()
            .map(|magic| magic.last_completion.clone())
            .unwrap_or_default()
    }

    fn combat_mode(&mut self) -> CombatMode {
        self.combat_mode
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.mode_requests.push(format!("EnterMode:{mode:?}"));
        CombatResult {
            status: CombatStatus::Started,
            notice: None,
        }
    }

    fn items_available(&mut self) -> bool {
        true
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        Vec::new()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.item_completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.used_objects.push(object_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }

    /// The world's appraisal answers only when the test says it is there,
    /// as on the live client.
    fn identify(&mut self, object_id: u32) -> ItemResult {
        if !self.world_appraisal_available {
            return ItemResult::of(ItemStatus::Unavailable);
        }
        self.identified_world_objects.push(object_id);
        ItemResult::of(ItemStatus::Started)
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

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}

impl NavigationAutomation for FakeAutomation {
    fn navigation(&mut self) -> NavigationSnapshot {
        self.navigation_snapshot.clone()
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        self.objects.get(&object_id).cloned()
    }

    fn find_object(
        &mut self,
        name: &str,
        _near: Position,
        _maximum_distance_meters: f64,
    ) -> Option<NavigationObject> {
        self.find_name = Some(name.to_owned());
        self.found_object.clone()
    }

    fn navigation_objects(&mut self) -> Vec<NavigationObject> {
        self.world_objects.clone()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.intents.push(intent);
        NavigationStatus::Accepted
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.clear_count += 1;
        NavigationStatus::Accepted
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.faced_headings.push(heading_degrees);
        NavigationStatus::Accepted
    }

    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        self.moves.push((direction, pace, amount, unit));
        NavigationStatus::Accepted
    }

    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        self.stopped_moves.push(Some(channel));
        NavigationStatus::Accepted
    }

    fn jump(&mut self, power: f32) -> NavigationStatus {
        self.jumps.push(power);
        NavigationStatus::Accepted
    }

    fn go_to_place(&mut self, position: Position, arrival_meters: f32) -> NavigationStatus {
        self.go_to_requests.push((position, arrival_meters));
        if self.go_to_answer != NavigationStatus::Accepted {
            return self.go_to_answer;
        }
        self.go_to_report = GoToReport {
            sequence: self.go_to_report.sequence + 1,
            state: GoToState::Walking,
            reason: Some("walking".to_owned()),
            ..GoToReport::default()
        };
        NavigationStatus::Accepted
    }

    fn stop_go_to(&mut self) -> NavigationStatus {
        self.stop_go_to_calls += 1;
        self.go_to_report.state = GoToState::Stopped;
        self.go_to_report.reason = Some("stopped".to_owned());
        NavigationStatus::Accepted
    }

    fn go_to_report(&mut self) -> GoToReport {
        self.go_to_report.clone()
    }

    fn chat_input_active(&mut self) -> bool {
        self.chat_input_active
    }

    fn submit_chat(&mut self, text: &str) -> bool {
        self.submitted_chat.push(text.to_owned());
        true
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.active_vendor_object_id
    }

    fn is_casting(&mut self) -> bool {
        self.magic.as_ref().is_some_and(|magic| magic.is_casting)
    }

    fn request_cast(&mut self, spell_id: u32) -> CastResult {
        let Some(magic) = self.magic.as_mut() else {
            return CastResult::Unavailable;
        };
        magic.requested_spell_ids.push(spell_id);
        if magic.request_answer != CastResult::Sent {
            return magic.request_answer;
        }
        magic.cast_spell_ids.push(spell_id);
        CastResult::Sent
    }
}

/// The macro's combat-mode gate as these tests need it: it asks for the
/// wanted stance until the character stands in it. The gate itself (caster
/// choice, wielding) belongs to the combat slice; the route only asks it
/// and reads the answer.
#[derive(Debug, Default)]
pub struct StanceGate;

impl CombatModePreparation for StanceGate {
    fn try_prepare(
        &mut self,
        automation: &mut dyn Automation,
        wanted: CombatMode,
        _override_item_id: u32,
        _auto_select: bool,
    ) -> bool {
        if automation.combat_mode() == wanted {
            return true;
        }
        let _ = automation.enter_combat_mode(wanted);
        false
    }

    fn status(&self) -> String {
        String::new()
    }
}

/// The client, the route and the controller, handed to each call together.
#[derive(Debug)]
pub struct Rig {
    pub automation: FakeAutomation,
    pub settings: NavigationSettings,
    pub controller: NavigationController,
    pub idle_peace_mode: bool,
    /// The door rule's own gate.
    pub door_enabled: bool,
    /// Whether another rule holds a lock the door rule waits on.
    pub door_locked: bool,
    /// How often the door rule took its locks.
    pub door_armed: u32,
}

impl Rig {
    /// A rig over a route.
    pub fn new(automation: FakeAutomation, settings: NavigationSettings) -> Self {
        Self {
            automation,
            settings,
            controller: NavigationController::new(),
            idle_peace_mode: false,
            door_enabled: true,
            door_locked: false,
            door_armed: 0,
        }
    }

    /// The call's turn.
    pub fn turn(&mut self) -> RouteTurn<'_> {
        RouteTurn {
            automation: &mut self.automation,
            settings: &mut self.settings,
            idle_peace_mode: self.idle_peace_mode,
        }
    }

    /// Splits the rig into the controller and its turn.
    pub fn parts(&mut self) -> (&mut NavigationController, RouteTurn<'_>) {
        (
            &mut self.controller,
            RouteTurn {
                automation: &mut self.automation,
                settings: &mut self.settings,
                idle_peace_mode: self.idle_peace_mode,
            },
        )
    }

    pub fn tick(&mut self, seconds: f64, can_act: bool) -> bool {
        let (controller, mut turn) = self.parts();
        controller.tick(&mut turn, seconds, can_act)
    }

    /// One host frame: the clock moves, then the controller takes its turn
    /// (the production order).
    pub fn frame(&mut self, seconds: f64) -> bool {
        self.controller.advance_clock(seconds);
        self.tick(seconds, true)
    }

    pub fn claim(&mut self, can_act: bool) -> bool {
        let (controller, mut turn) = self.parts();
        controller.claim_from_rule_pass(&mut turn, can_act)
    }

    pub fn step(&mut self, seconds: f64, navigation_slots_are_clear: bool) {
        let (controller, mut turn) = self.parts();
        controller.step_armed_mover(&mut turn, seconds, navigation_slots_are_clear);
    }

    /// Steps the armed mover through a stretch of host frames, each long
    /// enough to be a steering frame (the C# `StepFrames`, its loop kept).
    pub fn step_frames(&mut self, seconds: f64) {
        let mut elapsed = 0.0;
        while elapsed < seconds {
            self.step(0.05, true);
            elapsed += 0.05;
        }
    }

    pub fn door(&mut self, seconds: f64, can_act: bool) -> bool {
        let (controller, mut turn) = self.parts();
        controller.tick_door_rule(&mut turn, seconds, can_act)
    }

    pub fn stop_for_lost_turn(&mut self) {
        let (controller, mut turn) = self.parts();
        controller.stop_for_lost_turn(&mut turn);
    }

    pub fn stop_for_macro_stop(&mut self) {
        let (controller, mut turn) = self.parts();
        controller.stop_for_macro_stop(&mut turn);
    }

    pub fn reset(&mut self) {
        let (controller, mut turn) = self.parts();
        controller.reset(&mut turn);
    }

    pub fn anchor(&mut self) -> Option<String> {
        let (controller, mut turn) = self.parts();
        controller.anchor_round_to_start(&mut turn)
    }

    pub fn skip(&mut self, count: i32) -> i32 {
        let (controller, mut turn) = self.parts();
        controller.skip_waypoints(&mut turn, count)
    }

    pub fn step_back(&mut self, count: i32) -> i32 {
        let (controller, mut turn) = self.parts();
        controller.step_back_waypoints(&mut turn, count)
    }

    pub fn status(&self) -> String {
        self.controller.status().to_owned()
    }

    pub fn index(&self) -> i32 {
        self.controller.current_waypoint_index()
    }

    pub fn bind_locks(&mut self, locks: &SharedActionLocks) {
        self.controller.bind_action_locks(Rc::clone(locks));
    }

    pub fn bind_gate(&mut self) {
        let gate: Rc<RefCell<dyn CombatModePreparation>> = Rc::new(RefCell::new(StanceGate));
        self.controller.bind_combat_mode_gate(gate);
    }
}

impl DoorRuleHost for Rig {
    fn door_rule_enabled(&mut self) -> bool {
        self.door_enabled
    }

    fn door_rule_locked(&mut self) -> bool {
        self.door_locked
    }

    fn door_rule_arm(&mut self) {
        self.door_armed += 1;
    }

    fn tick_door_rule(&mut self, elapsed_seconds: f64, can_act: bool) -> bool {
        self.door(elapsed_seconds, can_act)
    }

    fn door_status(&mut self) -> String {
        self.controller.door_status().to_owned()
    }
}

/// A position on the test landblock, at ground level.
pub fn position(east_west: f64, north_south: f64) -> Position {
    position_facing(east_west, north_south, 0.0)
}

/// A position on the test landblock, facing `heading`.
pub fn position_facing(east_west: f64, north_south: f64, heading: f32) -> Position {
    Position {
        cell_id: 0x7F7F_0001,
        east_west,
        north_south,
        elevation: 0.0,
        heading_degrees: heading,
        is_outdoor: true,
    }
}

/// The character in the world, standing still at `position`.
pub fn snapshot(position: Position) -> NavigationSnapshot {
    snapshot_airborne(position, false)
}

pub fn snapshot_airborne(position: Position, airborne: bool) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: 1,
        position,
        is_moving: false,
        is_airborne: airborne,
        ..NavigationSnapshot::default()
    }
}

pub fn waypoint(kind: RouteWaypointType, position: Position) -> RouteWaypoint {
    RouteWaypoint {
        kind,
        position,
        ..RouteWaypoint::default()
    }
}

/// An object as navigation sees it.
pub fn object(object_id: u32, name: &str, position: Position) -> NavigationObject {
    NavigationObject {
        object_id,
        name: name.to_owned(),
        position,
        ..NavigationObject::default()
    }
}

/// A closed door with a known (or unknown) lock state.
pub fn door(
    object_id: u32,
    name: &str,
    position: Position,
    has_lock_state: bool,
) -> NavigationObject {
    NavigationObject {
        is_door: true,
        is_open: false,
        has_lock_state,
        ..object(object_id, name, position)
    }
}

/// An enabled route.
pub fn route(
    mode: RouteMode,
    minimum_distance_meters: f64,
    client_pathing: ClientPathing,
    waypoints: Vec<RouteWaypoint>,
) -> NavigationSettings {
    NavigationSettings {
        enabled: true,
        mode,
        minimum_distance_meters,
        client_pathing,
        waypoints,
        ..NavigationSettings::default()
    }
}

/// The C# `Controller(automation, mode, [minimum], waypoints)`: an enabled
/// route, 2 m arrival unless given, client pathing when stuck.
pub fn rig(automation: FakeAutomation, mode: RouteMode, waypoints: Vec<RouteWaypoint>) -> Rig {
    rig_min(automation, mode, 2.0, waypoints)
}

pub fn rig_min(
    automation: FakeAutomation,
    mode: RouteMode,
    minimum_distance_meters: f64,
    waypoints: Vec<RouteWaypoint>,
) -> Rig {
    Rig::new(
        automation,
        route(
            mode,
            minimum_distance_meters,
            ClientPathing::WhenStuck,
            waypoints,
        ),
    )
}
