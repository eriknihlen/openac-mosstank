//! The route follower against the C# plugin, black-box: the scenarios in
//! `golden/routes.txt` are run through the C# `NavigationController` by
//! `RustPortNavGolden.cs` (in the C# tests), which writes every call the
//! controller made and every line it said, step by step, to
//! `golden/routes.out`; this test runs the same scenarios through the Rust
//! controller and must write the same. Regenerate the expected file with
//!
//!     MOSSTANK_RUST_GOLDEN=1 dotnet test --filter RustPortNavGolden
//!
//! The scenario language and the transcript format are shared with the C#
//! runner; change both together.

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
use mosstank_automation::locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_formats::route::{
    ClientPathing, NavigationSettings, RouteJumpDirection, RouteMode, RouteRecallKind,
    RouteWaypoint, RouteWaypointType,
};
use mosstank_nav::{NavigationAutomation, NavigationController, RouteTurn};
use openac_plugin_compat::number::{to_string_f32, to_string_f64};

fn b(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

/// Space-separated words; double quotes anywhere in a word keep the spaces
/// between them and are dropped (`name="Town Portal"`).
fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        if c == '"' {
            quoted = !quoted;
            started = true;
        } else if c == ' ' && !quoted {
            if started {
                words.push(std::mem::take(&mut word));
            }
            started = false;
        } else {
            word.push(c);
            started = true;
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// A number; with an `m` suffix, metres turned into map units.
fn number(text: &str) -> f64 {
    match text.strip_suffix('m') {
        Some(metres) => metres.parse::<f64>().expect("a number") / 240.0,
        None => text.parse::<f64>().expect("a number"),
    }
}

fn at(text: &str, heading: f32) -> Position {
    let parts: Vec<&str> = text.split(',').collect();
    Position {
        cell_id: 0x7F7F_0001,
        east_west: number(parts[0]),
        north_south: number(parts[1]),
        elevation: parts.get(2).map_or(0.0, |part| number(part)),
        heading_degrees: heading,
        is_outdoor: true,
    }
}

fn keys(words: &[String], from: usize) -> HashMap<String, String> {
    words[from..]
        .iter()
        .map(|word| match word.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (word.clone(), String::new()),
        })
        .collect()
}

fn route_mode(name: &str) -> RouteMode {
    match name {
        "Circular" => RouteMode::Circular,
        "Linear" => RouteMode::Linear,
        "Target" => RouteMode::Target,
        "Once" => RouteMode::Once,
        _ => panic!("route mode {name}"),
    }
}

fn client_pathing(name: &str) -> ClientPathing {
    match name {
        "Never" => ClientPathing::Never,
        "WhenStuck" => ClientPathing::WhenStuck,
        "Always" => ClientPathing::Always,
        _ => panic!("client pathing {name}"),
    }
}

fn waypoint_type(name: &str) -> RouteWaypointType {
    (0..=9)
        .filter_map(RouteWaypointType::from_i32)
        .find(|kind| kind.name() == name)
        .unwrap_or_else(|| panic!("waypoint type {name}"))
}

fn recall_kind(name: &str) -> RouteRecallKind {
    RouteRecallKind::ALL
        .into_iter()
        .find(|kind| kind.name() == name)
        .unwrap_or_else(|| panic!("recall {name}"))
}

fn jump_direction(name: &str) -> RouteJumpDirection {
    match name {
        "Forward" => RouteJumpDirection::Forward,
        "StrafeLeft" => RouteJumpDirection::StrafeLeft,
        "StrafeRight" => RouteJumpDirection::StrafeRight,
        "Backward" => RouteJumpDirection::Backward,
        _ => panic!("jump direction {name}"),
    }
}

fn cast_result(name: &str) -> CastResult {
    match name {
        "Sent" => CastResult::Sent,
        "UnknownSpell" => CastResult::UnknownSpell,
        "NoTarget" => CastResult::NoTarget,
        "IncompatibleTarget" => CastResult::IncompatibleTarget,
        "MissingComponents" => CastResult::MissingComponents,
        "Unavailable" => CastResult::Unavailable,
        _ => panic!("cast result {name}"),
    }
}

fn navigation_status(name: &str) -> NavigationStatus {
    match name {
        "Unavailable" => NavigationStatus::Unavailable,
        "Accepted" => NavigationStatus::Accepted,
        "Rejected" => NavigationStatus::Rejected,
        "Held" => NavigationStatus::Held,
        _ => panic!("navigation status {name}"),
    }
}

fn go_to_state(name: &str) -> GoToState {
    (0_u8..=10)
        .filter_map(GoToState::from_tag)
        .find(|state| format!("{state:?}") == name)
        .unwrap_or_else(|| panic!("go-to state {name}"))
}

#[derive(Debug)]
struct GoldenMagic {
    request_answer: CastResult,
    is_casting: bool,
    last_completion: CastCompletion,
}

/// The recording client: what `NavigationTests.FakeAutomation` answers,
/// every call written to the transcript as the C# runner writes it.
#[derive(Debug, Default)]
struct GoldenAutomation {
    transcript: Vec<String>,
    world_appraisal_available: bool,
    magic: Option<GoldenMagic>,
    chat_input_active: bool,
    snapshot: NavigationSnapshot,
    objects: HashMap<u32, NavigationObject>,
    world_objects: Vec<NavigationObject>,
    chat_messages: Vec<ChatLine>,
    item_completion: ItemUseCompletion,
    active_vendor_object_id: u32,
    found_object: Option<NavigationObject>,
    go_to_answer: Option<NavigationStatus>,
    go_to_report: GoToReport,
}

impl SpellCatalog for GoldenAutomation {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, _spell_id: u32) -> Option<SpellInfo> {
        None
    }
}

impl Automation for GoldenAutomation {
    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        self.snapshot.clone()
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        self.objects.get(&object_id).cloned()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.transcript.push(format!(
            "intent F={} B={} SL={} SR={} TL={} TR={} run={} jump={}",
            b(intent.forward),
            b(intent.backward),
            b(intent.strafe_left),
            b(intent.strafe_right),
            b(intent.turn_left),
            b(intent.turn_right),
            b(intent.run),
            b(intent.jump)
        ));
        NavigationStatus::Accepted
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.transcript.push("clear".to_owned());
        NavigationStatus::Accepted
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.transcript
            .push(format!("face {}", to_string_f32(heading_degrees)));
        NavigationStatus::Accepted
    }

    fn chat_input_active(&mut self) -> bool {
        self.chat_input_active
    }

    fn is_casting(&mut self) -> bool {
        self.magic.as_ref().is_some_and(|magic| magic.is_casting)
    }

    fn request_cast(&mut self, spell_id: u32, _target: Option<u32>) -> CastResult {
        let Some(magic) = self.magic.as_ref() else {
            return CastResult::Unavailable;
        };
        let answer = magic.request_answer;
        self.transcript
            .push(format!("cast {spell_id} -> {answer:?}"));
        answer
    }

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
        CombatMode::Unknown
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
        Vec::new()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.item_completion.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.transcript.push(format!("use {object_id}"));
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        if !self.world_appraisal_available {
            return ItemResult::of(ItemStatus::Unavailable);
        }
        self.transcript.push(format!("identify {object_id}"));
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
        self.transcript.push(format!("post {text}"));
    }

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}

impl NavigationAutomation for GoldenAutomation {
    fn find_object(
        &mut self,
        name: &str,
        near: Position,
        maximum_distance_meters: f64,
    ) -> Option<NavigationObject> {
        self.transcript.push(format!(
            "find {name} near {},{} within {}",
            to_string_f64(near.east_west),
            to_string_f64(near.north_south),
            to_string_f64(maximum_distance_meters)
        ));
        self.found_object.clone()
    }

    fn navigation_objects(&mut self) -> Vec<NavigationObject> {
        self.world_objects.clone()
    }

    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        self.transcript.push(format!(
            "move {direction:?} {pace:?} {} {unit:?}",
            to_string_f32(amount)
        ));
        NavigationStatus::Accepted
    }

    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        self.transcript.push(format!("stopmove {channel:?}"));
        NavigationStatus::Accepted
    }

    fn jump(&mut self, power: f32) -> NavigationStatus {
        self.transcript
            .push(format!("jump {}", to_string_f32(power)));
        NavigationStatus::Accepted
    }

    fn go_to_place(&mut self, position: Position, arrival_meters: f32) -> NavigationStatus {
        let answer = self.go_to_answer.unwrap_or(NavigationStatus::Accepted);
        self.transcript.push(format!(
            "goto {},{} within {} -> {answer:?}",
            to_string_f64(position.east_west),
            to_string_f64(position.north_south),
            to_string_f32(arrival_meters)
        ));
        if answer != NavigationStatus::Accepted {
            return answer;
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
        self.transcript.push("stopgoto".to_owned());
        self.go_to_report.state = GoToState::Stopped;
        self.go_to_report.reason = Some("stopped".to_owned());
        NavigationStatus::Accepted
    }

    fn go_to_report(&mut self) -> GoToReport {
        self.go_to_report.clone()
    }

    fn submit_chat(&mut self, text: &str) -> bool {
        self.transcript.push(format!("submit {text}"));
        true
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.active_vendor_object_id
    }
}

/// One scenario: the client, the route, the controller, the lock table.
struct Scenario {
    automation: GoldenAutomation,
    settings: NavigationSettings,
    controller: NavigationController,
    locks: Option<SharedActionLocks>,
}

impl Scenario {
    fn new() -> Self {
        Self {
            automation: GoldenAutomation::default(),
            settings: NavigationSettings {
                enabled: true,
                ..NavigationSettings::default()
            },
            controller: NavigationController::new(),
            locks: None,
        }
    }

    fn turn(&mut self) -> (&mut NavigationController, RouteTurn<'_>) {
        (
            &mut self.controller,
            RouteTurn::new(&mut self.automation, &mut self.settings),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn apply(&mut self, words: &[String], line: &str, output: &mut String) {
        match words[0].as_str() {
            "route" => {
                self.settings.mode = route_mode(&words[1]);
                self.settings.minimum_distance_meters = number(&words[2]);
                let keys = keys(words, 3);
                if let Some(max) = keys.get("max") {
                    self.settings.maximum_distance_meters = number(max);
                }
                if let Some(portal) = keys.get("portal") {
                    self.settings.portal_use_distance_meters = number(portal);
                }
                if let Some(pathing) = keys.get("pathing") {
                    self.settings.client_pathing = client_pathing(pathing);
                }
                if let Some(doors) = keys.get("doors") {
                    self.settings.open_doors = doors == "1";
                }
                if let Some(identify) = keys.get("identify") {
                    self.settings.door_identify_range_meters = number(identify);
                }
                if let Some(open) = keys.get("open") {
                    self.settings.door_open_range_meters = number(open);
                }
                if let Some(follow) = keys.get("follow") {
                    self.settings.follow_target_object_id = follow.parse().unwrap();
                }
                if let Some(corners) = keys.get("corners") {
                    self.settings.follow_around_corners = corners == "1";
                }
                if let Some(enabled) = keys.get("enabled") {
                    self.settings.enabled = enabled == "1";
                }
                return;
            }
            "wp" => {
                self.settings.waypoints.push(waypoint(words));
                return;
            }
            "me" => {
                self.automation.snapshot = me(words);
                return;
            }
            "obj" => {
                self.put_object(words);
                return;
            }
            "unobj" => {
                let id: u32 = words[1].parse().unwrap();
                self.automation.objects.remove(&id);
                self.automation
                    .world_objects
                    .retain(|value| value.object_id != id);
                return;
            }
            "find" => {
                self.automation.found_object = Some(NavigationObject {
                    object_id: words[1].parse().unwrap(),
                    name: words[2].clone(),
                    position: at(&words[3], 0.0),
                    ..NavigationObject::default()
                });
                return;
            }
            "nofind" => {
                self.automation.found_object = None;
                return;
            }
            "typing" => {
                self.automation.chat_input_active = words[1] == "1";
                return;
            }
            "vendor" => {
                self.automation.active_vendor_object_id = words[1].parse().unwrap();
                return;
            }
            "appraisal" => {
                self.automation.world_appraisal_available = words[1] == "1";
                return;
            }
            "magic" => {
                let keys = keys(words, 1);
                let magic = self.automation.magic.get_or_insert_with(|| GoldenMagic {
                    request_answer: CastResult::Sent,
                    is_casting: false,
                    last_completion: CastCompletion::default(),
                });
                if let Some(answer) = keys.get("answer") {
                    magic.request_answer = cast_result(answer);
                }
                if let Some(casting) = keys.get("casting") {
                    magic.is_casting = casting == "1";
                }
                if let Some(completion) = keys.get("completion") {
                    let parts: Vec<&str> = completion.split(',').collect();
                    magic.last_completion = CastCompletion {
                        revision: parts[0].parse().unwrap(),
                        spell_id: parts[1].parse().unwrap(),
                        target_id: 0,
                        weenie_error: parts[2].parse().unwrap(),
                    };
                }
                return;
            }
            "usecompletion" => {
                self.automation.item_completion = ItemUseCompletion {
                    revision: words[1].parse().unwrap(),
                    source_id: words[2].parse().unwrap(),
                    target_id: 0,
                    weenie_error: words[3].parse().unwrap(),
                };
                return;
            }
            "goanswer" => {
                self.automation.go_to_answer = Some(navigation_status(&words[1]));
                return;
            }
            "goend" => {
                self.automation.go_to_report.state = go_to_state(&words[1]);
                self.automation.go_to_report.reason = Some(words[2].clone());
                return;
            }
            "chat" => {
                self.automation.chat_messages.push(ChatLine {
                    sequence: words[1].parse().unwrap(),
                    log_text_type: words[2].parse().unwrap(),
                    sender_id: words[3].parse().unwrap(),
                    kind: words[4].parse().unwrap(),
                    sender: words[5].clone(),
                    text: words[6].clone(),
                    display_text: words[7].clone(),
                    ..ChatLine::default()
                });
                return;
            }
            "locks" => {
                let locks = ActionLockTable::shared();
                self.controller.bind_action_locks(Rc::clone(&locks));
                self.locks = Some(locks);
                return;
            }
            "lockadvance" => {
                self.locks
                    .as_ref()
                    .expect("locks bound")
                    .borrow_mut()
                    .advance(number(&words[1]));
                return;
            }
            _ => {}
        }

        output.push_str("> ");
        output.push_str(line);
        output.push('\n');
        let mut result = "-".to_owned();
        let mut logged = Vec::new();
        match words[0].as_str() {
            "tick" => {
                let (controller, mut turn) = self.turn();
                result =
                    b(controller.tick(&mut turn, number(&words[1]), words.len() < 3)).to_owned();
            }
            "frame" => {
                let seconds = number(&words[1]);
                let (controller, mut turn) = self.turn();
                controller.advance_clock(seconds);
                result = b(controller.tick(&mut turn, seconds, true)).to_owned();
            }
            "claim" => {
                let (controller, mut turn) = self.turn();
                result = b(controller.claim_from_rule_pass(&mut turn, words.len() < 2)).to_owned();
            }
            "step" => {
                let (controller, mut turn) = self.turn();
                controller.step_armed_mover(&mut turn, number(&words[1]), words.len() < 3);
            }
            "steps" => {
                let seconds = number(&words[1]);
                let (controller, mut turn) = self.turn();
                let mut elapsed = 0.0;
                while elapsed < seconds {
                    controller.step_armed_mover(&mut turn, 0.05, true);
                    elapsed += 0.05;
                }
            }
            "door" => {
                let (controller, mut turn) = self.turn();
                result =
                    b(controller.tick_door_rule(&mut turn, number(&words[1]), words.len() < 3))
                        .to_owned();
            }
            "macrostop" => {
                let (controller, mut turn) = self.turn();
                controller.stop_for_macro_stop(&mut turn);
            }
            "lostturn" => {
                let (controller, mut turn) = self.turn();
                controller.stop_for_lost_turn(&mut turn);
            }
            "reset" => {
                let (controller, mut turn) = self.turn();
                controller.reset(&mut turn);
            }
            "anchor" => {
                let (controller, mut turn) = self.turn();
                logged.extend(controller.anchor_round_to_start(&mut turn));
            }
            "skip" => {
                let (controller, mut turn) = self.turn();
                result = controller
                    .skip_waypoints(&mut turn, words[1].parse().unwrap())
                    .to_string();
            }
            "back" => {
                let (controller, mut turn) = self.turn();
                result = controller
                    .step_back_waypoints(&mut turn, words[1].parse().unwrap())
                    .to_string();
            }
            "reverse" => self.controller.toggle_reverse(),
            "clearlocks" => {
                let (controller, mut turn) = self.turn();
                controller.clear_action_locks(&mut turn);
            }
            "pathing" => {
                self.settings.client_pathing = client_pathing(&words[1]);
                let (controller, mut turn) = self.turn();
                controller.client_pathing_changed(&mut turn);
            }
            "warnreset" => self.controller.reset_once_per_run_warnings(),
            "query" => {
                let (controller, mut turn) = self.turn();
                let detail = controller.running_detail(&mut turn);
                let normal = controller.allows_normal_movement(&mut turn);
                let settings = &*turn.settings;
                result = format!(
                    "detail={detail} holds={} normal={} nothing={} block={:?} rev={} walking={} action={} consumed0={}",
                    b(controller.holds_pass()),
                    b(normal),
                    b(controller.has_nothing_left_to_walk(settings)),
                    controller.step_back_block(settings),
                    b(controller.reversing()),
                    b(controller.is_client_walking()),
                    b(controller.has_active_action()),
                    b(controller.is_waypoint_consumed(settings, 0)),
                );
            }
            other => panic!("unknown line: {other}"),
        }

        for call in self.automation.transcript.drain(..) {
            output.push_str("  ");
            output.push_str(&call);
            output.push('\n');
        }
        for line in logged {
            output.push_str("  log ");
            output.push_str(&line);
            output.push('\n');
        }
        output.push_str(&format!(
            "  = {result} | {} | door: {} | idx {}",
            self.controller.status(),
            self.controller.door_status(),
            self.controller.current_waypoint_index()
        ));
        if let Some(locks) = &self.locks {
            let mut locks = locks.borrow_mut();
            output.push_str(&format!(
                " | locks {}{}{}",
                b(locks.is_locked(ActionLockKind::Navigation)),
                b(locks.is_locked(ActionLockKind::ItemUse)),
                b(locks.is_locked(ActionLockKind::DoorOpening))
            ));
        }
        output.push('\n');
    }

    fn put_object(&mut self, words: &[String]) {
        let id: u32 = words[1].parse().unwrap();
        let keys = keys(words, 4);
        let value = NavigationObject {
            object_id: id,
            name: words[2].clone(),
            position: at(&words[3], 0.0),
            is_door: keys.contains_key("door"),
            is_open: keys.contains_key("open"),
            is_locked: keys.contains_key("locked"),
            has_lock_state: keys.contains_key("lockstate"),
            lock_difficulty: keys
                .get("diff")
                .map_or(0, |difficulty| difficulty.parse().unwrap()),
        };
        self.automation.objects.insert(id, value.clone());
        match self
            .automation
            .world_objects
            .iter_mut()
            .find(|existing| existing.object_id == id)
        {
            Some(existing) => *existing = value,
            None => self.automation.world_objects.push(value),
        }
    }
}

fn waypoint(words: &[String]) -> RouteWaypoint {
    let mut waypoint = RouteWaypoint {
        kind: waypoint_type(&words[1]),
        ..RouteWaypoint::default()
    };
    let keys = keys(words, 2);
    if let Some(position) = keys.get("at") {
        waypoint.position = at(position, 0.0);
    }
    if let Some(reference) = keys.get("ref") {
        waypoint.reference_position = at(reference, 0.0);
    }
    if let Some(id) = keys.get("id") {
        waypoint.object_id = id.parse().unwrap();
    }
    if let Some(name) = keys.get("name") {
        waypoint.object_name.clone_from(name);
    }
    if let Some(text) = keys.get("text") {
        waypoint.text.clone_from(text);
    }
    if let Some(duration) = keys.get("dur") {
        waypoint.duration_milliseconds = duration.parse().unwrap();
    }
    if let Some(recall) = keys.get("recall") {
        waypoint.recall = recall_kind(recall);
    }
    if let Some(spell) = keys.get("spell") {
        waypoint.recall_spell_id = spell.parse().unwrap();
    }
    if let Some(spell_name) = keys.get("spellname") {
        waypoint.recall_spell_name.clone_from(spell_name);
    }
    if let Some(heading) = keys.get("heading") {
        waypoint.jump_heading_degrees = number(heading);
    }
    if let Some(shift) = keys.get("shift") {
        waypoint.jump_hold_shift = shift == "1";
    }
    if let Some(charge) = keys.get("charge") {
        waypoint.jump_charge_milliseconds = charge.parse().unwrap();
    }
    if let Some(direction) = keys.get("dir") {
        waypoint.jump_direction = jump_direction(direction);
    }
    waypoint
}

#[allow(clippy::cast_possible_truncation)]
fn me(words: &[String]) -> NavigationSnapshot {
    let heading = number(&words[3]) as f32;
    let position = at(&format!("{},{}", words[1], words[2]), heading);
    let mut snapshot = NavigationSnapshot {
        is_available: !words.iter().any(|word| word == "gone"),
        is_portal_space: words.iter().any(|word| word == "portal"),
        local_object_id: 1,
        position,
        is_moving: false,
        is_airborne: words.iter().any(|word| word == "air"),
        ..NavigationSnapshot::default()
    };
    if let Some(confirmed) = words
        .iter()
        .find_map(|word| word.strip_prefix("confirmed="))
    {
        let parts: Vec<&str> = confirmed.split(',').collect();
        snapshot.confirmed_position = at(&format!("{},{}", parts[0], parts[1]), 0.0);
        snapshot.confirmed_position_revision = parts[2].parse().unwrap();
    }
    snapshot
}

fn run(text: &str) -> String {
    let mut output = String::new();
    let mut scenario: Option<Scenario> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words = words(line);
        if words[0] == "case" {
            scenario = Some(Scenario::new());
            output.push_str("##### ");
            output.push_str(&words[1]);
            output.push('\n');
            continue;
        }
        scenario
            .as_mut()
            .unwrap_or_else(|| panic!("a line outside a case: {line}"))
            .apply(&words, line, &mut output);
    }
    output
}

// Source: tests/AcDream.Plugins.MossTank.Tests/RustPortNavGolden.cs :: WriteNavGoldenData (the transcript it writes is the expected one)
#[test]
fn routes_match_the_csharp_controller() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let scenarios = std::fs::read_to_string(folder.join("routes.txt")).unwrap();
    let expected = std::fs::read_to_string(folder.join("routes.out")).unwrap();
    let actual = run(&scenarios);
    if actual != expected {
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/golden/routes.actual.out");
        let _ = std::fs::create_dir_all(target.parent().unwrap());
        let _ = std::fs::write(&target, &actual);
        let first = actual
            .lines()
            .zip(expected.lines())
            .position(|(a, e)| a != e)
            .unwrap_or_else(|| actual.lines().count().min(expected.lines().count()));
        let context: Vec<String> = expected
            .lines()
            .zip(actual.lines())
            .enumerate()
            .skip(first.saturating_sub(4))
            .take(8)
            .map(|(number, (e, a))| {
                format!("{}: expected {e}\n{}:   actual {a}", number + 1, number + 1)
            })
            .collect();
        panic!(
            "the route transcript differs from the C# one at line {} (written to {}):\n{}",
            first + 1,
            target.display(),
            context.join("\n")
        );
    }
}
