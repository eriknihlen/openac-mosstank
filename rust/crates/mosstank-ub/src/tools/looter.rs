//! The looter (UtilityBelt's `Looter` tool, MIT; see `rust/NOTICE.md`): it
//! takes what the macro's active loot profile keeps out of a chest or the
//! character's own corpse. Three ways in, one engine:
//!
//! - by itself, while `Looter.Enabled` is on: a chest (with
//!   `Looter.EnableChests`) or the character's own corpse (with
//!   `Looter.EnableMyCorpse`) that opens, by hand or otherwise, is looted
//!   and left open (`EchoFilter_ServerDispatch`, `Item_OnViewContents`);
//! - `/ub use` on a chest or a corpse (`OpenContainer`): it is opened, with
//!   retries, and, when the same switches allow (a monster's corpse needs
//!   only `Looter.Enabled`), looted and closed;
//! - the key looter (`StartUI`): the UB page's key-and-chest row, or
//!   `/ub keyloot <key>` (MossTank's own command; UtilityBelt has only the
//!   button). A key of that name unlocks the chest, it is opened, looted and
//!   closed, and the round starts again while keys of that name are left.
//!
//! The verdict on an item is the macro's own rule engine's
//! ([`UbHost::loot_decide`], the one the corpse looter uses), over the rules
//! of the active loot profile: Keep, Keep # (up to its count, counting what
//! is owned and what is on its way) and Salvage take the item (a salvage
//! verdict loots the item; nothing is salvaged here), anything else leaves
//! it. An item whose verdict turns on an appraisal is appraised first, one
//! at a time. Everything in the character's own corpse is taken, as the
//! reference takes it.
//!
//! What the reference reads off the server's messages is read off the
//! client here: a chest's Locked property (the server's property update or
//! an appraisal) says whether it is locked, and the end of the key's use
//! says whether the key turned it (no error, or "already unlocked").
//!
//! A monster corpse is looted only through `/ub use`: the macro's own
//! corpse looter loots the rest, and a container it is working is left to
//! it.

use std::collections::HashSet;

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_automation::locks::ActionLockKind;
use mosstank_dotnet::text::ordinal;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_settings::ub::UbSettingValue;

use crate::chat::{self, tools};
use crate::commands::UbCall;
use crate::host::UbHost;
use crate::tools::give::friendly_time;

/// The looter's master switch.
pub const ENABLED: &str = "Looter.Enabled";
/// Loot chests that open.
pub const ENABLE_CHESTS: &str = "Looter.EnableChests";
/// Loot the character's own corpse when it opens.
pub const ENABLE_MY_CORPSE: &str = "Looter.EnableMyCorpse";
/// Hold the macro's melee and missile attacks while looting the own corpse.
pub const BLOCK_VTANK_MELEE: &str = "Looter.BlockVtankMelee";
/// How many times a container is asked to open.
pub const MAX_OPEN_ATTEMPTS: &str = "Looter.MaxOpenAttempts";
/// How many times a key is tried on a chest.
pub const MAX_UNLOCK_ATTEMPTS: &str = "Looter.MaxUnlockAttempts";
/// How many steps one item is tried before it is given up on.
pub const ATTEMPTS_BEFORE_BLACKLISTING: &str = "Looter.AttemptsBeforeBlacklisting";
/// The pause between tries at unlocking, opening and closing, milliseconds.
pub const DELAY_SPEED: &str = "Looter.DelaySpeed";
/// How often the looter steps, milliseconds.
pub const OVERALL_SPEED: &str = "Looter.OverallSpeed";

/// The shortest step the reference allows, milliseconds ("OverallSpeed
/// under 100 is not allowed.").
pub const MINIMUM_OVERALL_SPEED_MS: i32 = 100;
/// The fewest item slots a container has to have to be looted
/// (`IsValidContainer`): a chest or a corpse, never a pack.
pub const MINIMUM_ITEM_SLOTS: u8 = 48;
/// The integer property a container's item slots are kept under.
pub const ITEMS_CAPACITY_PROPERTY: u32 = 6;
/// The true/false property that says a chest is locked.
pub const LOCKED_PROPERTY: u32 = 3;
/// The server's answer to a key used on a chest that is not locked.
pub const LOCK_ALREADY_UNLOCKED: u32 = 0x04AF;
/// The server's answer to a key used on a chest that stands open
/// (`WERROR_LOCK_ALREADY_OPEN`, 1201).
pub const LOCK_ALREADY_OPEN: u32 = 0x04B1;
/// How long the macro's slots are held while the own corpse is looted, and
/// held again after (`LockVtankSettings`).
pub const MACRO_HOLD_SECONDS: f64 = 30.0;
/// How long an appraisal is waited for before it is asked again.
pub const IDENTIFY_TIMEOUT_SECONDS: f64 = 10.0;
/// How many times one item is asked about before it is judged on what is
/// known.
pub const MAXIMUM_IDENTIFY_ATTEMPTS: i32 = 3;

/// The words that call a key-loot run off.
const STOP_WORDS: [&str; 4] = ["stop", "cancel", "quit", "abort"];

/// The macro's slots held while the own corpse is looted, and whether the
/// melee switch is what holds it (`LockVtankSettings`).
const MACRO_HOLDS: [(ActionLockKind, bool); 5] = [
    (ActionLockKind::Navigation, false),
    (ActionLockKind::VoidSpellLockedOut, false),
    (ActionLockKind::WarSpellLockedOut, false),
    (ActionLockKind::MeleeAttackShot, true),
    (ActionLockKind::ItemUse, false),
];

/// What kind of container is being looted (`containertype`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// A chest.
    Chest,
    /// The character's own corpse.
    MyCorpse,
    /// Anyone else's corpse.
    MonsterCorpse,
}

/// Where the looter stands (`looterstate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LooterState {
    /// Nothing under way; open containers are watched.
    Done,
    /// The container closed.
    Closed,
    /// Trying the key.
    Unlocking,
    /// The chest is locked.
    Locked,
    /// The key turned.
    Unlocked,
    /// Asking the container to open.
    Opening,
    /// The container is open; its contents are awaited.
    Open,
    /// Judging and taking the items.
    Looting,
    /// Asking the container to close.
    Closing,
}

/// How the run under way began (`runtype`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunType {
    /// A container the player opened.
    None,
    /// `/ub use`.
    UbUse,
    /// The key looter.
    Ui,
}

/// One item's place in the run (`itemstate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Not judged yet.
    InContainer,
    /// Its appraisal is awaited.
    RequestingInfo,
    /// To be taken.
    NeedsToBeLooted,
    /// To be left.
    Ignore,
    /// Taken.
    Looted,
    /// Given up on.
    Blacklisted,
}

impl ItemState {
    /// Whether the item is finished with.
    #[must_use]
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Ignore | Self::Looted | Self::Blacklisted)
    }
}

#[derive(Debug, Clone)]
struct ContainerItem {
    item: InventoryItem,
    state: ItemState,
    loot_attempts: i32,
    identify_attempts: i32,
}

/// The settings one step reads.
#[derive(Debug, Clone, Copy)]
struct Pacing {
    delay_seconds: f64,
    max_open_attempts: i32,
    max_unlock_attempts: i32,
    attempts_before_blacklisting: i32,
    block_melee: bool,
}

impl Pacing {
    fn read(call: &UbCall<'_>) -> Self {
        let whole = |name: &str| call.settings.value(name).as_int32();
        Self {
            delay_seconds: f64::from(whole(DELAY_SPEED).max(0)) / 1000.0,
            max_open_attempts: whole(MAX_OPEN_ATTEMPTS),
            max_unlock_attempts: whole(MAX_UNLOCK_ATTEMPTS),
            attempts_before_blacklisting: whole(ATTEMPTS_BEFORE_BLACKLISTING),
            block_melee: call.flag(BLOCK_VTANK_MELEE),
        }
    }
}

/// The looter.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ChestLooter {
    state: LooterState,
    last_state: LooterState,
    first: bool,
    run: RunType,
    owned_container: bool,
    container_id: u32,
    kind: ContainerKind,
    key_name: String,
    key_id: u32,
    items: Vec<ContainerItem>,
    rules: Vec<LootRule>,
    open_attempts: i32,
    unlock_attempts: i32,
    since_attempt: f64,
    since_step: f64,
    step_seconds: f64,
    run_seconds: f64,
    clock_running: bool,
    awaiting_identify: u32,
    awaiting_seconds: f64,
    /// The appraisal slot's revision just after the request awaited.
    awaiting_revision: i64,
    /// The next appraisal is held up by the slot's other users.
    appraisal_blocked: bool,
    blocked_seconds: f64,
    unlock_baseline: i64,
    locked_at_round_start: bool,
    unlocked_this_round: bool,
    asked_about_container: bool,
    passive_seen: u32,
    holds: Vec<ActionLockKind>,
    page_key: String,
    page_chest_id: u32,
    page_chest_name: String,
}

impl Default for ChestLooter {
    fn default() -> Self {
        Self::new()
    }
}

impl ChestLooter {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: LooterState::Done,
            last_state: LooterState::Done,
            first: true,
            run: RunType::None,
            owned_container: false,
            container_id: 0,
            kind: ContainerKind::Chest,
            key_name: String::new(),
            key_id: 0,
            items: Vec::new(),
            rules: Vec::new(),
            open_attempts: 0,
            unlock_attempts: 0,
            since_attempt: 0.0,
            since_step: 0.0,
            step_seconds: f64::from(MINIMUM_OVERALL_SPEED_MS) / 1000.0,
            run_seconds: 0.0,
            clock_running: false,
            awaiting_identify: 0,
            awaiting_seconds: 0.0,
            awaiting_revision: 0,
            appraisal_blocked: false,
            blocked_seconds: 0.0,
            unlock_baseline: 0,
            locked_at_round_start: false,
            unlocked_this_round: false,
            asked_about_container: false,
            passive_seen: 0,
            holds: Vec::new(),
            page_key: String::new(),
            page_chest_id: 0,
            page_chest_name: String::new(),
        }
    }

    /// Where it stands.
    #[must_use]
    pub fn state(&self) -> LooterState {
        self.state
    }

    /// How the run under way began.
    #[must_use]
    pub fn run_type(&self) -> RunType {
        self.run
    }

    /// The container being worked, 0 for none.
    #[must_use]
    pub fn container_id(&self) -> u32 {
        self.container_id
    }

    /// Whether anything is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state != LooterState::Done
    }

    /// Whether a key-loot run is under way.
    #[must_use]
    pub fn is_key_looting(&self) -> bool {
        self.run == RunType::Ui
    }

    /// One item's place in the run, `None` for an item the run does not
    /// hold.
    #[must_use]
    pub fn item_state(&self, object_id: u32) -> Option<ItemState> {
        self.items
            .iter()
            .find(|entry| entry.item.object_id == object_id)
            .map(|entry| entry.state)
    }

    // ---- the frame ------------------------------------------------------

    /// One frame: the looter steps once every `Looter.OverallSpeed`
    /// milliseconds (`BaseTimer_Timeout`). Never owns the character, as the
    /// reference's never does.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        if self.state != LooterState::Done {
            self.since_attempt += elapsed;
            if self.clock_running {
                self.run_seconds += elapsed;
            }
            if self.awaiting_identify != 0 {
                self.awaiting_seconds += elapsed;
            }
            if self.appraisal_blocked {
                self.blocked_seconds += elapsed;
            }
        }
        self.since_step += elapsed;
        if self.since_step + 1e-9 < self.step_seconds {
            return;
        }
        self.since_step = 0.0;
        self.step(call);
    }

    fn step(&mut self, call: &mut UbCall<'_>) {
        if self.state == LooterState::Done {
            self.watch_open_container(call);
            return;
        }
        let mut speed = call.settings.value(OVERALL_SPEED).as_int32();
        if speed < MINIMUM_OVERALL_SPEED_MS {
            // The reference says so and puts the setting back
            // (`Looter_Changed`).
            call.write_ub("OverallSpeed under 100 is not allowed.");
            let row = call.settings.require(OVERALL_SPEED);
            call.settings
                .set(&row, UbSettingValue::from_int(MINIMUM_OVERALL_SPEED_MS));
            speed = MINIMUM_OVERALL_SPEED_MS;
        }
        self.step_seconds = f64::from(speed) / 1000.0;
        if self.last_state != self.state {
            self.first = true;
            self.last_state = self.state;
        }
        let pacing = Pacing::read(call);
        match self.state {
            LooterState::Done => {}
            LooterState::Closed => {
                if self.owned_container {
                    self.finish(call, true, false);
                } else {
                    self.state = LooterState::Done;
                }
            }
            LooterState::Locked => {
                if self.run == RunType::Ui {
                    self.state = LooterState::Unlocking;
                } else {
                    call.post_ub(&chat::tool(tools::LOOTER, "Chest is locked... bailing"));
                    self.finish(call, false, true);
                }
            }
            LooterState::Unlocked => {
                self.state = LooterState::Opening;
                self.since_attempt = 0.0;
            }
            LooterState::Unlocking => self.unlocking(call, pacing),
            LooterState::Opening => self.opening(call, pacing),
            LooterState::Open => self.open(call, pacing),
            LooterState::Looting => self.looting(call, pacing),
            LooterState::Closing => self.closing(call, pacing),
        }
    }

    /// Nothing under way: a chest or the own corpse that opens is looted
    /// while the switches say so (`Item_OnViewContents`).
    fn watch_open_container(&mut self, call: &mut UbCall<'_>) {
        let open = call.host.open_container();
        if open == 0 {
            self.passive_seen = 0;
            return;
        }
        if open == self.passive_seen || !call.flag(ENABLED) || !call.host.loot_contents_ready() {
            return;
        }
        self.passive_seen = open;
        if call.services.macro_loot_container() == open {
            return;
        }
        let Some(container) = call.host.object(open) else {
            return;
        };
        let Some(kind) = loot_container_kind(call.host, &container) else {
            return;
        };
        let wanted = match kind {
            ContainerKind::Chest => call.flag(ENABLE_CHESTS),
            ContainerKind::MyCorpse => call.flag(ENABLE_MY_CORPSE),
            ContainerKind::MonsterCorpse => false,
        };
        if !wanted {
            return;
        }
        let contents = call.host.loot_contents();
        if contents.is_empty() {
            return;
        }
        self.reset_run();
        self.container_id = open;
        self.kind = kind;
        self.run = RunType::None;
        self.owned_container = false;
        self.begin_contents(call, contents);
        self.state = LooterState::Looting;
    }

    // ---- the ways in ----------------------------------------------------

    /// `/ub use` on a chest or a corpse (`OpenContainer`): it is opened, with
    /// retries. With `Looter.Enabled` on it is then looted and closed: a
    /// chest with `Looter.EnableChests`, the own corpse with
    /// `Looter.EnableMyCorpse`, and a monster's corpse always (the command
    /// named it). Otherwise it is only opened. A run under way is put down
    /// without a word.
    pub fn open_container(&mut self, call: &mut UbCall<'_>, container: &WorldObject) {
        let kind = loot_container_kind(call.host, container).unwrap_or(ContainerKind::Chest);
        self.release_holds(call);
        self.reset_run();
        self.container_id = container.object_id;
        self.kind = kind;
        self.run = RunType::UbUse;
        self.owned_container = true;
        self.state = LooterState::Opening;
    }

    /// Starts a key-loot run on a chest with keys of a name (`StartUI`);
    /// false, having said why, when none of them is carried.
    pub fn start_key_run(&mut self, call: &mut UbCall<'_>, chest_id: u32, key_name: &str) -> bool {
        self.release_holds(call);
        self.reset_run();
        self.container_id = chest_id;
        key_name.clone_into(&mut self.key_name);
        self.start_round(call)
    }

    /// One round of a key-loot run: the key, then the chest (`StartUI`).
    fn start_round(&mut self, call: &mut UbCall<'_>) -> bool {
        let Some(key) = find_key(call.host, &self.key_name) else {
            call.write_ub("Looter: Out of keys to use");
            self.end_run();
            return false;
        };
        let chest = self.container_id;
        let name = std::mem::take(&mut self.key_name);
        self.reset_run();
        self.container_id = chest;
        self.key_name = name;
        self.key_id = key;
        self.kind = ContainerKind::Chest;
        self.run = RunType::Ui;
        self.owned_container = true;
        self.locked_at_round_start = chest_locked(call.host, chest) == Some(true);
        self.state = LooterState::Unlocking;
        true
    }

    /// Stops a run without a word (`StopLooter`); false when nothing was
    /// running. What it took stays taken; an open container stays open.
    pub fn stop(&mut self, call: &mut UbCall<'_>) -> bool {
        if !self.is_running() {
            return false;
        }
        self.release_holds(call);
        self.passive_seen = self.container_id;
        self.end_run();
        true
    }

    /// `/ub keyloot <key name>`, and `/ub keyloot stop` (or cancel, quit,
    /// abort): MossTank's own command, the UB page's key looter by name. The
    /// chest is the selected object.
    pub fn keyloot_command(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let text = arguments.trim();
        if text.is_empty() {
            call.write_ub_bad_syntax("keyloot");
            return;
        }
        if STOP_WORDS
            .iter()
            .any(|word| ordinal::equals_ignore_case(text, word))
        {
            if self.is_key_looting() {
                call.write_ub("Looter stopped");
                self.stop(call);
            } else {
                call.write_ub_error("Looter is not running.");
            }
            return;
        }
        if self.is_key_looting() {
            call.write_ub_error(
                "Looter: Already running.  Please wait until it completes or use /ub keyloot stop to quit previous session",
            );
            return;
        }
        let Some(chest) = selected_chest(call) else {
            return;
        };
        if find_key(call.host, text).is_none() {
            call.write_ub_error(&format!("Looter: no {text} in your inventory"));
            return;
        }
        self.start_key_run(call, chest.object_id, text);
    }

    // ---- the UB page's key looter (`ItemGiverView`, its Looter group) ----

    /// The key the page uses.
    #[must_use]
    pub fn page_key(&self) -> &str {
        &self.page_key
    }

    /// The chest the page uses, by name.
    #[must_use]
    pub fn page_chest(&self) -> &str {
        &self.page_chest_name
    }

    /// The page's Start button's caption.
    #[must_use]
    pub fn page_button_text(&self) -> &'static str {
        if self.is_key_looting() {
            "Stop"
        } else {
            "Start"
        }
    }

    /// "Set key": the selected object's name, when it is a key
    /// (`Looter_SetKeyButton_Hit`).
    pub fn set_page_key(&mut self, call: &mut UbCall<'_>) {
        let Some(selected) = selected_object(call) else {
            return;
        };
        if selected.object_class != ObjectClass::Key {
            call.write_ub_error("Looter: target is not a key");
            return;
        }
        self.page_key = selected.name;
    }

    /// "Set chest": the selected object, when it is a chest
    /// (`Looter_SetTargetChestButton_Hit`).
    pub fn set_page_chest(&mut self, call: &mut UbCall<'_>) {
        let Some(selected) = selected_object(call) else {
            return;
        };
        if selected.object_class != ObjectClass::Container {
            call.write_ub_error("Looter: target is not a chest");
            return;
        }
        self.page_chest_id = selected.object_id;
        self.page_chest_name = selected.name;
    }

    /// The page's Start/Stop button (`Looter_StartButton_Hit`).
    pub fn press_page_button(&mut self, call: &mut UbCall<'_>) {
        if self.is_key_looting() {
            call.write_ub("Looter stopped");
            self.stop(call);
            return;
        }
        if self.page_chest_id == 0 {
            call.write_ub_error("Looter: no chest set");
            return;
        }
        let key = self.page_key.clone();
        self.start_key_run(call, self.page_chest_id, &key);
    }

    // ---- the states -----------------------------------------------------

    /// Tries the key on the chest until it turns (`looterstate.Unlocking`).
    fn unlocking(&mut self, call: &mut UbCall<'_>, pacing: Pacing) {
        // A chest that already stands open is looted as it stands: the
        // server answers a key used on it with "already open"
        // ([`LOCK_ALREADY_OPEN`]), and the reference then goes on to Open
        // when the open container is its chest (`Item_UseDone`, 1201).
        if call.host.open_container() == self.container_id {
            self.unlocked_this_round = true;
            self.since_attempt = 0.0;
            self.state = LooterState::Open;
            return;
        }
        if self.unlock_attempts > 0 {
            let completion = call.host.item_state().last_completion;
            if completion.revision > self.unlock_baseline
                && completion.source_id == self.key_id
                && completion.target_id == self.container_id
            {
                if completion.weenie_error == 0 || completion.weenie_error == LOCK_ALREADY_UNLOCKED
                {
                    self.unlocked_this_round = true;
                    self.state = LooterState::Unlocked;
                    return;
                }
                // The key did not turn: the next try answers again.
                self.unlock_baseline = completion.revision;
            }
            if self.locked_at_round_start
                && chest_locked(call.host, self.container_id) == Some(false)
            {
                self.unlocked_this_round = true;
                self.state = LooterState::Unlocked;
                return;
            }
        }
        if !self.first && self.since_attempt < pacing.delay_seconds {
            return;
        }
        if self.unlock_attempts >= pacing.max_unlock_attempts {
            call.write_ub("Reached max number of unlock attempts");
            self.finish(call, false, true);
            return;
        }
        // A key used up is gone: the next of the name is tried.
        let Some(key) = find_key(call.host, &self.key_name) else {
            call.write_ub("Looter: Out of keys to use");
            self.finish(call, false, true);
            return;
        };
        self.key_id = key;
        self.unlock_baseline = call.host.item_state().last_completion.revision;
        // A use the client holds back for another is no try at the lock.
        if call.host.apply_item(key, self.container_id).status == ItemStatus::Busy {
            return;
        }
        self.since_attempt = 0.0;
        self.first = false;
        self.unlock_attempts += 1;
    }

    /// Whether `/ub use` loots what it opened: the looter on, and the
    /// container's own switch on; a monster's corpse needs none.
    fn ub_use_loots(&self, call: &UbCall<'_>) -> bool {
        call.flag(ENABLED)
            && match self.kind {
                ContainerKind::Chest => call.flag(ENABLE_CHESTS),
                ContainerKind::MyCorpse => call.flag(ENABLE_MY_CORPSE),
                ContainerKind::MonsterCorpse => true,
            }
    }

    /// Asks the container to open until it does (`looterstate.Opening`).
    fn opening(&mut self, call: &mut UbCall<'_>, pacing: Pacing) {
        if call.host.open_container() == self.container_id {
            // Only opened, and left open, when the switches say not to loot
            // it (`looterstate.Opening`, `Item_OnViewContents`).
            if self.run == RunType::UbUse && !self.ub_use_loots(call) {
                self.finish(call, false, true);
                return;
            }
            self.since_attempt = 0.0;
            self.state = LooterState::Open;
            return;
        }
        // A key that just turned is newer news than a Locked property the
        // update has not yet replaced.
        if !self.unlocked_this_round && chest_locked(call.host, self.container_id) == Some(true) {
            self.state = LooterState::Locked;
            return;
        }
        if !self.first && self.since_attempt < pacing.delay_seconds {
            return;
        }
        if self.open_attempts >= pacing.max_open_attempts {
            call.write_ub("Reached max number of opening attempts");
            self.finish(call, false, true);
            return;
        }
        self.since_attempt = 0.0;
        self.first = false;
        // The reference appraises the container, which is how it comes to
        // know whether it is locked.
        if !self.asked_about_container {
            self.asked_about_container = true;
            if call
                .host
                .object(self.container_id)
                .is_some_and(|known| !known.has_appraisal_data)
            {
                call.host.identify(self.container_id);
            }
        }
        call.host.open_loot(self.container_id);
        self.open_attempts += 1;
    }

    /// The container is open: its contents are read once they arrive
    /// (`looterstate.Open`), for no longer than the opening attempts would
    /// have taken (at least a second).
    fn open(&mut self, call: &mut UbCall<'_>, pacing: Pacing) {
        if call.host.open_container() != self.container_id {
            self.state = LooterState::Opening;
            return;
        }
        if !call.host.loot_contents_ready() {
            let ceiling =
                (f64::from(pacing.max_open_attempts.max(1)) * pacing.delay_seconds).max(1.0);
            if self.since_attempt >= ceiling {
                call.write_ub("Reached max number of opening attempts");
                self.finish(call, false, true);
            }
            return;
        }
        let contents = call.host.loot_contents();
        self.begin_contents(call, contents);
        self.state = LooterState::Looting;
    }

    /// Judges the items one at a time and takes what is to be taken
    /// (`looterstate.Looting`).
    fn looting(&mut self, call: &mut UbCall<'_>, pacing: Pacing) {
        if call.host.open_container() != self.container_id {
            // Closed under the run: what was left, was left.
            if self.owned_container {
                self.state = LooterState::Closed;
            } else {
                self.finish(call, true, false);
            }
            return;
        }
        // What the container holds now; nothing is taken while the client
        // reads it again.
        let ready = call.host.loot_contents_ready();
        let present: HashSet<u32> = if ready {
            call.host
                .loot_contents()
                .iter()
                .map(|item| item.object_id)
                .collect()
        } else {
            HashSet::new()
        };
        if ready {
            for entry in &mut self.items {
                if !entry.state.is_settled() && !present.contains(&entry.item.object_id) {
                    entry.state = ItemState::Looted;
                }
            }
        }
        self.judge_next(call);

        if let Some(index) = self.items.iter().position(|entry| {
            entry.state == ItemState::NeedsToBeLooted && present.contains(&entry.item.object_id)
        }) {
            if self.kind == ContainerKind::MyCorpse {
                self.hold_macro(call, pacing.block_melee);
            }
            let object_id = self.items[index].item.object_id;
            // Every pickup asked for is an attempt, as the reference counts
            // the moves it sends, refused ones too: an item the client keeps
            // refusing is given up on in the end. Only a client too busy to
            // take the ask has not been asked.
            let sent = !call.host.item_state().is_busy
                && call.host.loot_pickup(object_id).status != ItemStatus::Busy;
            let entry = &mut self.items[index];
            if sent {
                entry.loot_attempts += 1;
            }
            if sent && entry.loot_attempts >= pacing.attempts_before_blacklisting {
                entry.state = ItemState::Blacklisted;
                let name = entry.item.name.clone();
                call.write_ub(&format!("Looter: Failed to loot {name}"));
            }
        }

        if self.items.iter().all(|entry| entry.state.is_settled()) {
            if self.owned_container {
                self.since_attempt = 0.0;
                self.state = LooterState::Closing;
            } else {
                self.finish(call, true, false);
            }
        }
    }

    /// Asks the container to close until it has (`looterstate.Closing`).
    fn closing(&mut self, call: &mut UbCall<'_>, pacing: Pacing) {
        if call.host.open_container() != self.container_id {
            self.state = LooterState::Closed;
            return;
        }
        if self.first || self.since_attempt >= pacing.delay_seconds {
            self.since_attempt = 0.0;
            self.first = false;
            call.host.close_loot(self.container_id);
        }
    }

    // ---- judging --------------------------------------------------------

    /// The contents as the run's items: the own corpse's are all taken,
    /// anything else's judged; the profile is read now, and the clock
    /// starts.
    fn begin_contents(&mut self, call: &mut UbCall<'_>, contents: Vec<InventoryItem>) {
        self.rules = call.services.active_loot_rules().unwrap_or_default();
        let state = if self.kind == ContainerKind::MyCorpse {
            ItemState::NeedsToBeLooted
        } else {
            ItemState::InContainer
        };
        self.items = contents
            .into_iter()
            .map(|item| ContainerItem {
                item,
                state,
                loot_attempts: 0,
                identify_attempts: 0,
            })
            .collect();
        self.run_seconds = 0.0;
        self.clock_running = true;
    }

    /// One verdict a step (`GetLootDecision`): the appraisal awaited first,
    /// then the next item not judged yet. An item whose verdict turns on an
    /// appraisal is appraised first, one at a time, through the appraisal
    /// slot the corpse looter shares.
    fn judge_next(&mut self, call: &mut UbCall<'_>) {
        if self.awaiting_identify != 0 {
            let awaited = self.awaiting_identify;
            let arrived = call
                .host
                .object(awaited)
                .is_some_and(|known| known.has_appraisal_data);
            // A give-up counts only when it came after this request: the slot
            // goes on naming an item given up on before.
            let appraisal = call.host.appraisal();
            let given_up = (appraisal.last_abandoned_id == awaited
                && appraisal.revision > self.awaiting_revision)
                || self.awaiting_seconds >= IDENTIFY_TIMEOUT_SECONDS;
            if arrived || given_up {
                self.awaiting_identify = 0;
                self.awaiting_seconds = 0.0;
                // An item that left the container meanwhile stays settled.
                if let Some(index) = self
                    .index_of(awaited)
                    .filter(|&index| self.items[index].state == ItemState::RequestingInfo)
                {
                    if arrived {
                        self.decide(call, index);
                    } else {
                        self.items[index].identify_attempts += 1;
                        self.items[index].state = ItemState::InContainer;
                    }
                }
            }
        }

        let Some(index) = self
            .items
            .iter()
            .position(|entry| entry.state == ItemState::InContainer)
        else {
            return;
        };
        let item = self.items[index].item.clone();
        let appraised = call
            .host
            .object(item.object_id)
            .is_some_and(|known| known.has_appraisal_data);
        if !appraised && self.items[index].identify_attempts < MAXIMUM_IDENTIFY_ATTEMPTS {
            let properties = call.host.properties(item.object_id).unwrap_or_default();
            if call
                .host
                .loot_needs_identify(&item, &properties, &self.rules)
            {
                self.request_appraisal(call, index);
                return;
            }
        }
        self.decide(call, index);
    }

    /// Asks for one item's appraisal when the slot is free.
    fn request_appraisal(&mut self, call: &mut UbCall<'_>, index: usize) {
        if self.awaiting_identify != 0 {
            return;
        }
        if call.services.corpse_identifier_busy() || call.host.appraisal().awaiting_id != 0 {
            // Held up by the slot's other users: the wait counts against the
            // item as an unanswered request would, so one item cannot hold
            // the rest up for ever.
            self.appraisal_blocked = true;
            if self.blocked_seconds >= IDENTIFY_TIMEOUT_SECONDS {
                self.blocked_seconds = 0.0;
                self.items[index].identify_attempts += 1;
            }
            return;
        }
        self.appraisal_blocked = false;
        self.blocked_seconds = 0.0;
        let object_id = self.items[index].item.object_id;
        let result = call.host.identify(object_id);
        if result.accepted() {
            self.awaiting_identify = object_id;
            self.awaiting_seconds = 0.0;
            self.awaiting_revision = call.host.appraisal().revision;
            self.items[index].state = ItemState::RequestingInfo;
        } else if result.status != ItemStatus::Busy {
            self.items[index].identify_attempts += 1;
        }
    }

    /// The rules' verdict on one item: Keep, Keep # and Salvage take it,
    /// anything else leaves it. A keep-up-to rule counts what is owned and
    /// what this run is already taking.
    fn decide(&mut self, call: &mut UbCall<'_>, index: usize) {
        let item = self.items[index].item.clone();
        let properties = call.host.properties(item.object_id).unwrap_or_default();
        let mut owned: Vec<InventoryItem> = call.host.owned_items_shared().to_vec();
        owned.extend(
            self.items
                .iter()
                .filter(|entry| entry.state == ItemState::NeedsToBeLooted)
                .map(|entry| entry.item.clone()),
        );
        let decision = call
            .host
            .loot_decide(&item, &properties, &self.rules, &owned);
        self.items[index].state = match decision.map(|decision| decision.action) {
            Some(LootAction::Keep | LootAction::KeepUpTo | LootAction::Salvage) => {
                ItemState::NeedsToBeLooted
            }
            _ => ItemState::Ignore,
        };
    }

    fn index_of(&self, object_id: u32) -> Option<usize> {
        self.items
            .iter()
            .position(|entry| entry.item.object_id == object_id)
    }

    // ---- the macro's slots ----------------------------------------------

    /// Holds the macro's route, its attack spells, its item use and (with
    /// `Looter.BlockVtankMelee`) its melee and missile attacks while the own
    /// corpse is looted; a slot already held is left as it is
    /// (`LockVtankSettings`).
    fn hold_macro(&mut self, call: &mut UbCall<'_>, block_melee: bool) {
        let mut locks = call.locks.borrow_mut();
        for (kind, needs_switch) in MACRO_HOLDS {
            if needs_switch && !block_melee {
                continue;
            }
            if locks.is_locked(kind) {
                continue;
            }
            locks.arm(kind, MACRO_HOLD_SECONDS);
            if !self.holds.contains(&kind) {
                self.holds.push(kind);
            }
        }
    }

    /// Lets go of the slots this looter held (`UnLockVtankSettings`).
    fn release_holds(&mut self, call: &mut UbCall<'_>) {
        if self.holds.is_empty() {
            return;
        }
        let mut locks = call.locks.borrow_mut();
        for kind in self.holds.drain(..) {
            locks.release(kind);
        }
    }

    // ---- the end --------------------------------------------------------

    /// Ends a round (`DoneLooting`): the line saying how it went, the slots
    /// let go, and a key-loot run's next round while keys are left.
    /// `force` ends a key-loot run however many keys are left.
    fn finish(&mut self, call: &mut UbCall<'_>, write_chat: bool, force: bool) {
        if write_chat {
            let line = self.completion_line();
            call.write_ub(&line);
        }
        self.release_holds(call);
        self.passive_seen = self.container_id;
        let looted_nothing = self.count(ItemState::Looted) == 0;
        if self.run == RunType::Ui && !force {
            // A round that took nothing ends the run: a chest that stays
            // unlocked keeps what was left in it until it resets, and would
            // be opened and closed for as long as the keys last.
            if looted_nothing {
                self.end_run();
            } else {
                self.start_round(call);
            }
            return;
        }
        self.end_run();
    }

    /// "Looter completed in 3s Scanned 5 items and looted 2 items", with
    /// the items it could not take when there were any.
    fn completion_line(&self) -> String {
        let scanned = self.items.len();
        let looted = self.count(ItemState::Looted);
        let failed = self.count(ItemState::NeedsToBeLooted) + self.count(ItemState::Blacklisted);
        let took = friendly_time(self.run_seconds);
        if failed == 0 {
            format!("Looter completed in {took} Scanned {scanned} items and looted {looted} items")
        } else {
            format!(
                "Looter completed in {took} --- Scanned {scanned} items and looted {looted} items --- Failed to loot {failed} items"
            )
        }
    }

    fn count(&self, state: ItemState) -> usize {
        self.items
            .iter()
            .filter(|entry| entry.state == state)
            .count()
    }

    /// Clears the round, keeping what the run as a whole knows.
    fn reset_run(&mut self) {
        self.state = LooterState::Done;
        self.last_state = LooterState::Done;
        self.first = true;
        self.items.clear();
        self.rules.clear();
        self.open_attempts = 0;
        self.unlock_attempts = 0;
        self.since_attempt = 0.0;
        self.run_seconds = 0.0;
        self.clock_running = false;
        self.awaiting_identify = 0;
        self.awaiting_seconds = 0.0;
        self.awaiting_revision = 0;
        self.appraisal_blocked = false;
        self.blocked_seconds = 0.0;
        self.unlock_baseline = 0;
        self.locked_at_round_start = false;
        self.unlocked_this_round = false;
        self.asked_about_container = false;
        self.key_id = 0;
    }

    /// Ends the run.
    fn end_run(&mut self) {
        self.reset_run();
        self.run = RunType::None;
        self.owned_container = false;
        self.container_id = 0;
        self.key_name.clear();
    }

    /// Puts down a run without a word, as a new session does.
    pub fn reset(&mut self, call: &mut UbCall<'_>) {
        self.release_holds(call);
        self.end_run();
        self.passive_seen = 0;
    }
}

/// What kind of loot container an object is, `None` for one the looter
/// leaves alone: a chest or a corpse with room for at least
/// [`MINIMUM_ITEM_SLOTS`] items (`IsValidContainer`, `GetContainerType`),
/// never a pack. The server sends the slot count as one byte, and a chest's
/// is mostly "-1" (255, no limit): the byte is what is judged.
pub fn loot_container_kind(host: &mut dyn UbHost, object: &WorldObject) -> Option<ContainerKind> {
    if !matches!(
        object.object_class,
        ObjectClass::Container | ObjectClass::Corpse
    ) || object.is_owned
    {
        return None;
    }
    let mut slots = object.items_capacity;
    if slots == 0 {
        slots = host
            .int_property(object.object_id, ITEMS_CAPACITY_PROPERTY)
            .unwrap_or(0);
    }
    if slot_byte(slots) < MINIMUM_ITEM_SLOTS {
        return None;
    }
    if object.object_class == ObjectClass::Container {
        return Some(ContainerKind::Chest);
    }
    let own = host
        .character()
        .is_some_and(|character| object.name == format!("Corpse of {}", character.name));
    Some(if own {
        ContainerKind::MyCorpse
    } else {
        ContainerKind::MonsterCorpse
    })
}

/// The slot count as the server's byte carries it.
#[must_use]
pub fn slot_byte(slots: i32) -> u8 {
    slots.to_le_bytes()[0]
}

/// Whether the client holds a chest as locked, `None` while it does not
/// know.
fn chest_locked(host: &mut dyn UbHost, chest_id: u32) -> Option<bool> {
    host.properties(chest_id)?
        .bools
        .iter()
        .find(|(property, _)| *property == LOCKED_PROPERTY)
        .map(|(_, locked)| *locked)
}

/// The first carried key of a name, any case, nothing worn
/// (`FindInventoryObjectByName`).
fn find_key(host: &mut dyn UbHost, name: &str) -> Option<u32> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    host.owned_items_shared()
        .iter()
        .find(|item| item.equipped_location == 0 && ordinal::equals_ignore_case(&item.name, name))
        .map(|item| item.object_id)
}

/// The selected object, or the reference's error when nothing is.
fn selected_object(call: &mut UbCall<'_>) -> Option<WorldObject> {
    let selected = call
        .host
        .selection()
        .selected
        .and_then(|id| call.host.object(id));
    if selected.is_none() {
        call.write_ub_error("Looter: nothing selected");
    }
    selected
}

/// The selected object when it is a chest the looter can loot.
fn selected_chest(call: &mut UbCall<'_>) -> Option<WorldObject> {
    let selected = selected_object(call)?;
    if loot_container_kind(call.host, &selected) != Some(ContainerKind::Chest) {
        call.write_ub_error("Looter: target is not a chest");
        return None;
    }
    Some(selected)
}
