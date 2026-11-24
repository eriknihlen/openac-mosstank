//! What the route reads from and asks of the client, beyond the rules'
//! common view of it ([`Automation`]): the C#'s `INavigationAutomation`,
//! the chat bar, the vendor windows, the appraisal queue and the cast
//! requests a route waypoint makes.
//!
//! A method the C# interface gives a default body has the same default
//! here.

use ac_plugin_abi::magic::{SpellInfo, SpellList};
use ac_plugin_abi::navigation::{
    GoToReport, MoveChannel, MoveDirection, MovePace, MoveUnit, NavigationObject, NavigationStatus,
};
use ac_plugin_abi::world::Position;
use mosstank_automation::Automation;

/// The plugin's UB tools as a route's chat waypoint waits on them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UbActivity {
    /// The UB tool running now that holds the route, named by the command
    /// that starts it (`give`, `equip`, `looter`, ...).
    pub busy_tool: Option<&'static str>,
    /// The number of the last `/ub` line the plugin ran: where a line the
    /// route submits is looked for from.
    pub last_line: u64,
}

/// A chat line the plugin's UB commands run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbLine {
    /// A command.
    Command,
    /// A command that only stops a tool (`/ub give stop`).
    Stop,
}

/// What became of a `/ub` line the route submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbLineState {
    /// The plugin has not run it yet.
    NotRun,
    /// Run, but the line it put off (`/ub delay`, `/ub bc`) has not run yet.
    PutOff,
    /// Run, with whatever it put off.
    Run,
}

/// The client as the route sees it.
pub trait NavigationAutomation: Automation {
    /// The object nearest `near` named exactly `name` (any case), within
    /// `maximum_distance_meters` across the ground (`TryFindObject`).
    fn find_object(
        &mut self,
        name: &str,
        near: Position,
        maximum_distance_meters: f64,
    ) -> Option<NavigationObject> {
        let _ = (name, near, maximum_distance_meters);
        None
    }

    /// Every object whose position is known (`CaptureObjects`).
    fn navigation_objects(&mut self) -> Vec<NavigationObject> {
        Vec::new()
    }

    /// Begins a move the client carries out and ends by itself (`Move`).
    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        let _ = (direction, pace, amount, unit);
        NavigationStatus::Unavailable
    }

    /// Ends the move on one channel (`StopMoving(channel)`).
    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        let _ = channel;
        NavigationStatus::Unavailable
    }

    /// Jumps with `power` of a full charge.
    fn jump(&mut self, power: f32) -> NavigationStatus {
        let _ = power;
        NavigationStatus::Unavailable
    }

    /// Walks to a place along a route the client plans
    /// (`GoTo(position, arrivalMeters)`).
    fn go_to_place(&mut self, position: Position, arrival_meters: f32) -> NavigationStatus {
        let _ = (position, arrival_meters);
        NavigationStatus::Unavailable
    }

    /// Ends the walk this plugin started.
    fn stop_go_to(&mut self) -> NavigationStatus {
        NavigationStatus::Unavailable
    }

    /// The latest walk the client planned.
    fn go_to_report(&mut self) -> GoToReport {
        GoToReport::default()
    }

    /// Submits a line as if typed into the chat bar (`Submit`); false when
    /// the client took no line.
    fn submit_chat(&mut self, text: &str) -> bool {
        let _ = text;
        false
    }

    /// What the plugin's UB tools are doing now; nothing for a client
    /// without them.
    fn ub_activity(&mut self) -> UbActivity {
        UbActivity::default()
    }

    /// Whether `text` is a line the plugin's UB commands run once the
    /// client hands it back, after the call that submitted it, and of what
    /// kind; `None` for any other line.
    fn ub_line(&mut self, text: &str) -> Option<UbLine> {
        let _ = text;
        None
    }

    /// What became of the `/ub` line `text`, submitted when the last line
    /// the plugin had run was `since` ([`UbActivity::last_line`]).
    fn ub_line_state(&mut self, since: u64, text: &str) -> UbLineState {
        let _ = (since, text);
        UbLineState::Run
    }

    /// The vendor the client has open (`Items.ActiveVendorObjectId`), 0
    /// for none.
    fn active_vendor_id(&mut self) -> u32 {
        0
    }

    /// The vendor the trade side is working (`Vendor.VendorObjectId` while
    /// `Vendor.IsOpen`), 0 for none.
    fn open_vendor_id(&mut self) -> u32 {
        0
    }

    /// The object an appraisal is awaited for
    /// (`Loot.Appraisal.AwaitingObjectId`), 0 for none.
    fn appraisal_awaiting_id(&mut self) -> u32 {
        0
    }

    /// One of the character's known-spell lists (`KnownSelfBuffs`,
    /// `KnownAttackSpells`, `KnownCombatSpells`).
    fn known_spell_list(&mut self, list: SpellList) -> Vec<SpellInfo> {
        let _ = list;
        Vec::new()
    }

    /// The ids and names of one of the known-spell lists.
    fn known_spell_names(&mut self, list: SpellList) -> Vec<(u32, String)> {
        self.known_spell_list(list)
            .into_iter()
            .map(|spell| (spell.spell_id, spell.name))
            .collect()
    }
}

/// MossTank's tag on the lines it posts in chat.
pub const CHAT_TAG: &str = "[MossTank]";

/// Posts a line in the character's chat window under MossTank's tag
/// (`MossTankChat.Post`): the tag is added unless the line already has it.
pub fn post(automation: &mut dyn Automation, text: &str) {
    if text.starts_with(CHAT_TAG) {
        automation.post_system_message(text);
    } else {
        automation.post_system_message(&format!("{CHAT_TAG} {text}"));
    }
}
