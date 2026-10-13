//! VTank's fellowship manager (`FellowshipManagerTests`).

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::social::{FellowMember, FellowshipState, FellowshipStatus};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, Position};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_nav::NavigationAutomation;
use mosstank_vendor::{FellowshipAutomation, FellowshipManager};

fn member(id: u32, name: &str) -> FellowMember {
    FellowMember {
        object_id: id,
        name: name.to_owned(),
        current_health: 100,
        max_health: 100,
        current_stamina: 100,
        max_stamina: 100,
        current_mana: 100,
        max_mana: 100,
        distance: 0.0,
        ..FellowMember::default()
    }
}

fn position(east_west: f64) -> Position {
    Position {
        cell_id: 0x0001_0001,
        east_west,
        north_south: 0.0,
        elevation: 0.0,
        heading_degrees: 0.0,
        is_outdoor: true,
    }
}

/// `FellowshipManagerTests.FakeAutomation`: the character "Leader" (id 1)
/// leads an open fellowship; Alice (id 2) stands 4.8 m away.
#[derive(Debug)]
struct FakeFellows {
    sequence: u64,
    messages: Vec<ChatLine>,
    is_open: bool,
    roster: Vec<FellowMember>,
    recruited_object_id: u32,
    dismissed_object_id: u32,
    assigned_leader_object_id: u32,
    submitted: Vec<String>,
}

impl FakeFellows {
    fn new() -> Self {
        Self {
            sequence: 0,
            messages: Vec::new(),
            is_open: true,
            roster: vec![member(1, "Leader")],
            recruited_object_id: 0,
            dismissed_object_id: 0,
            assigned_leader_object_id: 0,
            submitted: Vec::new(),
        }
    }

    fn add_tell(&mut self, sender_id: u32, sender: &str, text: &str) {
        self.sequence += 1;
        self.messages.push(ChatLine {
            sequence: self.sequence,
            sender_id,
            kind: 3,
            sender: sender.to_owned(),
            text: text.to_owned(),
            ..ChatLine::default()
        });
    }
}

impl SpellCatalog for FakeFellows {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, _spell_id: u32) -> Option<SpellInfo> {
        None
    }
}

impl Automation for FakeFellows {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            name: "Leader".to_owned(),
            ..CharacterInfo::default()
        }
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
        false
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        Vec::new()
    }

    fn use_item(&mut self, _object_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }

    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.messages
            .iter()
            .filter(|message| message.sequence > after_sequence)
            .cloned()
            .collect()
    }

    fn post_system_message(&mut self, _text: &str) {}

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}

impl NavigationAutomation for FakeFellows {
    fn navigation(&mut self) -> NavigationSnapshot {
        NavigationSnapshot {
            is_available: true,
            is_portal_space: false,
            local_object_id: 1,
            position: position(0.0),
            ..NavigationSnapshot::default()
        }
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        (object_id == 2).then(|| NavigationObject {
            object_id: 2,
            name: "Alice".to_owned(),
            position: position(0.02),
            ..NavigationObject::default()
        })
    }

    fn set_movement_intent(&mut self, _intent: MovementIntent) -> NavigationStatus {
        NavigationStatus::Accepted
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        NavigationStatus::Accepted
    }

    fn submit_chat(&mut self, text: &str) -> bool {
        self.submitted.push(text.to_owned());
        true
    }
}

impl FellowshipAutomation for FakeFellows {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn fellowship_state(&mut self) -> FellowshipState {
        FellowshipState {
            is_in_fellowship: true,
            name: "Test Fellow".to_owned(),
            leader_id: 1,
            is_open: self.is_open,
            is_locked: false,
            member_count: self.roster.len() as i32,
        }
    }

    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        self.roster.clone()
    }

    fn fellowship_recruit(&mut self, player_id: u32) -> FellowshipStatus {
        self.recruited_object_id = player_id;
        FellowshipStatus::Accepted
    }

    fn fellowship_dismiss(&mut self, player_id: u32) -> FellowshipStatus {
        self.dismissed_object_id = player_id;
        FellowshipStatus::Accepted
    }

    fn fellowship_assign_leader(&mut self, player_id: u32) -> FellowshipStatus {
        self.assigned_leader_object_id = player_id;
        FellowshipStatus::Accepted
    }

    fn fellowship_set_open(&mut self, open: bool) -> FellowshipStatus {
        self.is_open = open;
        FellowshipStatus::Accepted
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/FellowshipManagerTests.cs :: XpTellQueuesAndRecruitsANearbyPlayerThroughTheHostCommand
#[test]
fn xp_tell_queues_and_recruits_a_nearby_player_through_the_host_command() {
    let mut automation = FakeFellows::new();
    automation.add_tell(2, "Alice", "xp");
    let mut manager = FellowshipManager::new();

    manager.tick(&mut automation, 0.1, true);

    assert_eq!(automation.recruited_object_id, 2);
    assert!(
        automation
            .submitted
            .iter()
            .any(|line| line.contains("I will recruit you in a moment"))
    );
    assert!(manager.waiting_names().iter().any(|name| name == "Alice"));

    automation.roster.push(member(2, "Alice"));
    manager.tick(&mut automation, 0.1, true);
    assert!(!manager.waiting_names().iter().any(|name| name == "Alice"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/FellowshipManagerTests.cs :: MemberVoteExecutesGiveLeaderAfterTheOfficialTwoMinuteWindow
#[test]
fn member_vote_executes_give_leader_after_the_official_two_minute_window() {
    let mut automation = FakeFellows::new();
    automation.roster.push(member(2, "Alice"));
    automation.roster.push(member(3, "Bob"));
    let mut manager = FellowshipManager::new();
    automation.add_tell(2, "Alice", "startvote giveleader Bob");

    manager.tick(&mut automation, 0.1, true);
    automation.add_tell(3, "Bob", "vote 1 yes");
    manager.tick(&mut automation, 0.1, true);
    manager.tick(&mut automation, 120.0, true);

    assert_eq!(automation.assigned_leader_object_id, 3);
    assert!(
        automation
            .submitted
            .iter()
            .any(|line| line.contains("passed (2/0)"))
    );
}
