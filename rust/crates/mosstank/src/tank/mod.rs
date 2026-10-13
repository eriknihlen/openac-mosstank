//! The macro shell (the C#'s `MossTankPanel` without its windows): the
//! settings and the profile stores, every rule's controller, the macro
//! scheduler and the frame that drives them, the `/vt` commands and the
//! autostart.
//!
//! The shell is three owners, so a pass can lend the client and the parts
//! to the rules without lending the scheduler to itself: the scheduler,
//! the meta engine, and [`Core`] (everything else). The client comes in per
//! call as a [`TankClient`].
//!
//! The rules' log lines, the looter's warnings and the gate's stops are
//! raised inside calls that hold the client, so they queue
//! ([`QueuedLine`]) and the shell writes them out right after each rule and
//! frame step (register LOG-1, COMBAT-1).

mod actions;
mod autostart;
mod commands;
mod diagnostics;
mod display;
mod frame;
mod meta_windows;
pub(crate) mod options;
pub(crate) mod panel;
mod peers;
pub(crate) mod profiles;
mod route_commands;
pub(crate) mod rules;
mod services;
mod session;
pub mod ub;
mod vendor;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mosstank_automation::Automation;
use mosstank_automation::cast_tracker::{SharedCastTracker, SpellCastTracker};
use mosstank_automation::locks::{ActionLockTable, SharedActionLocks};
use mosstank_buffs::buff_rule::{BuffRuleHost, BuffSelfRule};
use mosstank_buffs::mana::ItemManaRechargeController;
use mosstank_buffs::recharge_controller::VitalRechargeController;
use mosstank_combat::controller::CombatController;
use mosstank_combat::dispel::DispelController;
use mosstank_combat::gate::{CombatModeGate, SharedCombatModeGate};
use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_expr::ExpressionClock;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;
use mosstank_loot::controller::LootController;
use mosstank_loot::corpse_approach::CorpseApproachController;
use mosstank_loot::scroll::ReadScrollController;
use mosstank_meta::engine::MetaEngine;
use mosstank_meta::runtime::ExpressionRuntime;
use mosstank_meta::scheduler::{IdlePeaceRule, MacroLogChannel, MacroScheduler, PassHold};
use mosstank_nav::NavigationController;
use mosstank_settings::model::AllSettings;
use mosstank_stores::layout::StorageFolderPreparer;
use mosstank_stores::loot_store::LootProfileStore;
use mosstank_stores::meta_store::MetaProfileStore;
use mosstank_stores::profile_store::ProfileStore;
use mosstank_stores::route_store::RouteProfileStore;
use mosstank_vendor::FellowshipManager;

use crate::client::TankClient;

pub(crate) use commands::{UB_VERB, VTANK_VERB};
pub use rules::{TankCx, TankFamily};

/// The macro's own chat prefix (`MossTankChat`).
pub(crate) const CHAT_PREFIX: &str = "[MossTank]";

/// A chat line with the macro's prefix, unless it carries it already
/// (`MossTankChat.Post`).
pub(crate) fn post_chat(client: &mut dyn Automation, text: &str) {
    if text.starts_with(CHAT_PREFIX) {
        client.post_system_message(text);
    } else {
        client.post_system_message(&format!("{CHAT_PREFIX} {text}"));
    }
}

/// A line raised inside a call that holds the client, written out by the
/// shell right after it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QueuedLine {
    /// A macro log line (`EmitMacroLog`); `chat` false for the lines that
    /// only go to the session log.
    Macro {
        channel: MacroLogChannel,
        message: String,
        chat: bool,
    },
    /// A line for the player in chat (`WriteVtank`).
    Chat(String),
    /// A warning for the session log.
    Warn(String),
}

/// The shared queue of [`QueuedLine`]s.
pub(crate) type LineQueue = Rc<RefCell<Vec<QueuedLine>>>;

/// The host's clocks, read at the start of each call, for the expression
/// runtime (register EXPR-2).
#[derive(Debug, Default)]
pub(crate) struct HostClock {
    utc_ms: Cell<i64>,
    utc_100ns: Cell<i64>,
    offset_minutes: Cell<i32>,
    offset_seconds: Cell<i32>,
    monotonic: Cell<u64>,
}

impl HostClock {
    /// Takes the client's reading. The finer readings are taken when they
    /// agree with the millisecond ones (a host that leaves them unset has
    /// only those).
    pub(crate) fn set(&self, clock: &ac_plugin_abi::host::Clock) {
        self.utc_ms.set(clock.utc_unix_ms);
        self.utc_100ns.set(
            if clock.utc_unix_100ns.div_euclid(10_000) == clock.utc_unix_ms {
                clock.utc_unix_100ns
            } else {
                clock.utc_unix_ms.saturating_mul(10_000)
            },
        );
        self.offset_minutes.set(clock.utc_offset_minutes);
        self.offset_seconds.set(
            if clock.utc_offset_seconds.div_euclid(60) == clock.utc_offset_minutes {
                clock.utc_offset_seconds
            } else {
                clock.utc_offset_minutes.saturating_mul(60)
            },
        );
        self.monotonic.set(clock.monotonic_ms);
    }
}

impl mosstank_session::SessionClock for HostClock {
    fn utc_now_100ns(&self) -> i64 {
        self.utc_100ns.get()
    }

    fn local_offset_seconds(&self) -> i32 {
        self.offset_seconds.get()
    }

    fn monotonic_ms(&self) -> u64 {
        self.monotonic.get()
    }
}

impl ExpressionClock for HostClock {
    fn utc_now_ms(&self) -> i64 {
        self.utc_ms.get()
    }

    fn local_offset_minutes(&self) -> i32 {
        self.offset_minutes.get()
    }

    fn monotonic_ms(&self) -> u64 {
        self.monotonic.get()
    }
}

/// FastCastBuffs' forward hold (`BeginFastCastMovement`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FastCast {
    active: bool,
    start_completion_revision: i64,
    elapsed: f64,
}

/// What the stopped-and-saved switches were when the character died.
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct DeathSaved {
    nav: bool,
    loot: bool,
    buff: bool,
    combat: bool,
}

/// The client's state the frame compares with the last frame's to poke the
/// pass on a receipt (`ObserveSchedulerPokes`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PokeSnapshot {
    magic: i64,
    items: i64,
    combat_mode: u8,
    equipping: bool,
    items_busy: bool,
    inventory: i64,
    appraisal: i64,
    container: u32,
    contents_ready: bool,
}

/// Everything the shell owns but the scheduler and the meta engine.
#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
pub struct Core {
    pub(crate) settings: AllSettings,
    pub(crate) no_buff_item_names: OrdinalStringSet,
    /// The profile's log channels (`_commandLogTypes`).
    pub(crate) command_log_types: IgnoreCaseStringSet,
    /// The channels the session asked for from outside (autostart).
    pub(crate) session_log_types: IgnoreCaseStringSet,

    pub(crate) profiles: ProfileStore,
    pub(crate) loot_profiles: LootProfileStore,
    pub(crate) route_profiles: RouteProfileStore,
    pub(crate) meta_profiles: MetaProfileStore,
    pub(crate) embedded_route_label: Option<String>,
    pub(crate) folders: StorageFolderPreparer,
    pub(crate) queued_announcements: Vec<String>,
    pub(crate) game_info: VtankGameInfoDatabase,

    pub(crate) locks: SharedActionLocks,
    pub(crate) gate: SharedCombatModeGate,
    pub(crate) tracker: SharedCastTracker,
    pub(crate) hold: PassHold,
    pub(crate) lines: LineQueue,
    pub(crate) clock: Rc<HostClock>,

    pub(crate) combat: CombatController,
    pub(crate) buff_rule: BuffSelfRule,
    pub(crate) vital_recharge: VitalRechargeController,
    pub(crate) vital_helper_recharge: VitalRechargeController,
    pub(crate) dispel: DispelController,
    pub(crate) item_mana: ItemManaRechargeController,
    pub(crate) loot: LootController,
    pub(crate) corpse_approach: CorpseApproachController,
    pub(crate) read_scroll: ReadScrollController,
    /// The macro's own stack-and-cram owner (`InventoryMaintenanceController`).
    pub(crate) inventory_maintenance:
        mosstank_ub::tools::stack_cram::InventoryMaintenanceController,
    /// Pea splits, kit and food restocks, ammunition crafts (`CraftingController`).
    pub(crate) crafting: mosstank_loot::crafting::CraftingController,
    /// The helper band's pet refill, on the normal threshold.
    pub(crate) normal_pet_refill: mosstank_combat::pets::PetRefillRule,
    /// The idle band's pet refill, on the idle threshold.
    pub(crate) idle_pet_refill: mosstank_combat::pets::PetRefillRule,
    /// The pet summoner beside the main list.
    pub(crate) summon_pet: mosstank_combat::pets::SummonPetRule,
    /// The random helper buffs.
    pub(crate) random_helper: mosstank_buffs::random_helper::RandomHelperRule,
    pub(crate) navigation: NavigationController,
    pub(crate) fellowship: FellowshipManager,
    /// The vendor run and the `/ub vendor` commands (`VendorTradeController`).
    pub(crate) vendor_trade: mosstank_vendor::VendorTradeController,
    /// Whether the scheduler runs, as of the start of the call and each
    /// start or stop since (`_scheduler.IsRunning`, which the vendor run
    /// reads to stand down from opening vendors while the route owns that).
    pub(crate) scheduler_running: bool,
    pub(crate) idle_peace: rules::SharedIdlePeace,

    pub(crate) status: String,
    pub(crate) profile_lifecycle_notice: String,
    pub(crate) loot_editor_notice: String,
    pub(crate) route_notice: String,
    pub(crate) meta_notice: String,
    pub(crate) corpse_id_wait_decided: bool,
    pub(crate) navigation_waits_on_corpse_id: bool,
    pub(crate) route_held_for_rare: bool,
    pub(crate) death_saved: DeathSaved,
    pub(crate) was_dead_for_macro: bool,
    pub(crate) automation_was_available: bool,
    pub(crate) prologue_owns_action: bool,
    pub(crate) transaction_suspension_held: bool,
    pub(crate) transaction_suspension_elapsed: f64,
    pub(crate) poke: PokeSnapshot,
    pub(crate) cast_tracker_chat_sequence: u64,
    pub(crate) reported_suspension: Option<String>,
    pub(crate) walk_clock: f64,
    pub(crate) last_attack_seconds: f64,
    pub(crate) selection_before_pass: Option<u32>,
    pub(crate) fast_cast: FastCast,
    pub(crate) applying_profile_options: bool,
    pub(crate) initialized: bool,
    pub(crate) autostart_applied_for_current_session: bool,
    pub(crate) command_portal_state: bool,
    pub(crate) command_portal_count: i32,
    pub(crate) first_run_guidance_pending: bool,
    /// A meta interval set during a call, for the scheduler once the call
    /// returns.
    pub(crate) meta_interval_pending: Option<f64>,
    /// The looting switch the fight's kill hold reads.
    pub(crate) route_after_kill: Rc<Cell<bool>>,
    /// What only the windows keep.
    pub(crate) panel: panel::PanelState,
    /// The peer features asked of the other clients.
    pub(crate) peers: peers::PeerState,
    /// The session's own records: the action history and debug log, the
    /// kill statistics, the game database check, the published meta state.
    pub(crate) session: session::SessionParts,
}

/// The macro shell.
#[allow(missing_debug_implementations)]
pub struct Tank {
    pub(crate) scheduler: MacroScheduler<TankFamily>,
    pub(crate) meta: MetaEngine,
    pub(crate) core: Core,
    pub(crate) ub: ub::UbParts,
}

/// Where the first-run guidance is remembered.
const ONBOARDING_KEY: &str = "onboarding/v1.txt";

impl Core {
    /// A macro log line (`EmitMacroLog`).
    pub(crate) fn emit_macro_log(
        &mut self,
        client: &mut dyn TankClient,
        channel: MacroLogChannel,
        message: &str,
        chat: bool,
    ) {
        if !self.is_logging(channel) {
            return;
        }
        if chat {
            post_chat(client.rules(), &format!("{CHAT_PREFIX} {message}"));
        }
        client
            .rules()
            .log_info(&format!("[vt log {}] {message}", channel_name(channel)));
    }

    /// Whether a channel is logged (`IsLogging`).
    pub(crate) fn is_logging(&self, channel: MacroLogChannel) -> bool {
        let name = channel_name(channel);
        self.command_log_types.contains(name) || self.session_log_types.contains(name)
    }

    /// Queues a macro log line for the next write-out.
    pub(crate) fn queue_log(&self, channel: MacroLogChannel, message: &str) {
        self.lines.borrow_mut().push(QueuedLine::Macro {
            channel,
            message: message.to_owned(),
            chat: true,
        });
    }

    /// Writes out every queued line, in order, and runs the stops the
    /// shared gate asked for.
    pub(crate) fn write_out(&mut self, client: &mut dyn TankClient) {
        loop {
            let lines: Vec<QueuedLine> = std::mem::take(&mut *self.lines.borrow_mut());
            if lines.is_empty() {
                break;
            }
            for line in lines {
                match line {
                    QueuedLine::Macro {
                        channel,
                        message,
                        chat,
                    } => self.emit_macro_log(client, channel, &message, chat),
                    QueuedLine::Chat(text) => post_chat(client.rules(), &text),
                    QueuedLine::Warn(text) => client.rules().log_warn(&text),
                }
            }
        }
        // Settled gear wakes the pass (`EquipmentSettled = Poke`).
        if self.gate.borrow_mut().take_settled() {
            self.hold.poke();
        }
        loop {
            let stop = self.gate.borrow_mut().take_stop_request();
            let Some(notice) = stop else {
                break;
            };
            self.stop_macro_from_gate(client, &notice);
        }
    }

    /// A line for the player (`WriteVtank`).
    pub(crate) fn write_vtank(client: &mut dyn TankClient, text: &str) {
        post_chat(client.rules(), text);
    }

    /// `Announce`.
    pub(crate) fn announce(client: &mut dyn TankClient, text: &str) {
        post_chat(client.rules(), &format!("{CHAT_PREFIX} {text}"));
    }

    /// A line to tell the player, now when there is a character to tell,
    /// else once there is (`QueueAnnouncement`).
    pub(crate) fn queue_announcement(&mut self, client: &mut dyn TankClient, line: &str) {
        if client.rules().is_available() {
            Self::announce(client, line);
            return;
        }
        self.queued_announcements.push(line.to_owned());
    }

    /// `FlushQueuedAnnouncements`.
    pub(crate) fn flush_queued_announcements(&mut self, client: &mut dyn TankClient) {
        if self.queued_announcements.is_empty() || !client.rules().is_available() {
            return;
        }
        for line in std::mem::take(&mut self.queued_announcements) {
            Self::announce(client, &line);
        }
    }

    /// The shared gate asked for the macro to stop (`StopMacroFromGate`).
    pub(crate) fn stop_macro_from_gate(&mut self, client: &mut dyn TankClient, notice: &str) {
        if self.buff_rule.is_bursting() {
            self.stop(client, notice);
        }
        notice.clone_into(&mut self.status);
        self.set_macro_running(client, false);
    }

    /// `Stop`: the buff burst and what it borrowed are put down.
    pub(crate) fn stop(&mut self, client: &mut dyn TankClient, status: &str) {
        self.clear_fast_cast_movement(client);
        self.buff_rule.stop();
        status.clone_into(&mut self.status);
        self.restore_selection(client);
        self.gate.borrow_mut().reset();
    }

    /// `RestoreSelection`.
    pub(crate) fn restore_selection(&mut self, client: &mut dyn TankClient) {
        restore_selection(client.rules(), &mut self.selection_before_pass);
    }

    /// `ClearFastCastMovement`.
    pub(crate) fn clear_fast_cast_movement(&mut self, client: &mut dyn TankClient) {
        clear_fast_cast(client.rules(), &mut self.fast_cast);
    }

    /// `ResetOncePerRunWarnings`.
    pub(crate) fn reset_once_per_run_warnings(&mut self) {
        self.random_helper.reset_once_per_run_warnings();
        self.gate.borrow_mut().reset_once_per_run_warnings();
        self.navigation.reset_once_per_run_warnings();
        self.buff_rule.reset_once_per_run_warnings();
        self.item_mana.reset_once_per_run_warnings();
    }

    /// Hands the cast outcomes the fight heard to the buff rule
    /// (`OnBuffCastOutcome`).
    pub(crate) fn forward_cast_outcomes(&mut self) {
        for outcome in self.combat.take_cast_outcomes() {
            self.buff_rule.observe_cast_outcome(&outcome);
        }
    }
}

/// The name a channel prints as (`MacroLogChannel.ToString()`).
pub(crate) fn channel_name(channel: MacroLogChannel) -> &'static str {
    match channel {
        MacroLogChannel::ActiveRule => "ActiveRule",
        MacroLogChannel::RuleInfo => "RuleInfo",
        MacroLogChannel::BusyState => "BusyState",
        MacroLogChannel::SpellCast => "SpellCast",
        MacroLogChannel::DebuffChoice => "DebuffChoice",
        MacroLogChannel::CastInfo => "CastInfo",
        MacroLogChannel::Loot => "Loot",
        MacroLogChannel::Misc => "Misc",
        MacroLogChannel::Timers => "Timers",
    }
}

/// Every channel, in the enum's order.
pub(crate) const CHANNELS: [MacroLogChannel; 9] = [
    MacroLogChannel::ActiveRule,
    MacroLogChannel::RuleInfo,
    MacroLogChannel::BusyState,
    MacroLogChannel::SpellCast,
    MacroLogChannel::DebuffChoice,
    MacroLogChannel::CastInfo,
    MacroLogChannel::Loot,
    MacroLogChannel::Misc,
    MacroLogChannel::Timers,
];

fn restore_selection(automation: &mut dyn Automation, before: &mut Option<u32>) {
    match before.take() {
        Some(previous) if previous != 0 => {
            automation.select(previous);
        }
        _ => {
            automation.clear_selection();
        }
    }
}

fn clear_fast_cast(automation: &mut dyn Automation, fast_cast: &mut FastCast) {
    if !fast_cast.active {
        return;
    }
    automation.clear_movement_intent();
    *fast_cast = FastCast::default();
}

/// The buff rule's owner (`IBuffRuleHost`, the panel's side of it). A stop
/// the rule asks for is run by the shell right after the rule's call: the
/// rule returns at once after asking.
pub(crate) struct BuffOwner<'a> {
    macro_enabled: bool,
    fast_cast_buffs: bool,
    status: &'a mut String,
    lines: &'a LineQueue,
    selection_before_pass: &'a mut Option<u32>,
    fast_cast: &'a mut FastCast,
    /// The stop the rule asked for (`StopFromBuffRule`).
    pub(crate) stop_request: Option<String>,
}

impl<'a> BuffOwner<'a> {
    pub(crate) fn new(
        macro_enabled: bool,
        fast_cast_buffs: bool,
        status: &'a mut String,
        lines: &'a LineQueue,
        selection_before_pass: &'a mut Option<u32>,
        fast_cast: &'a mut FastCast,
    ) -> Self {
        Self {
            macro_enabled,
            fast_cast_buffs,
            status,
            lines,
            selection_before_pass,
            fast_cast,
            stop_request: None,
        }
    }
}

impl BuffRuleHost for BuffOwner<'_> {
    fn macro_enabled(&self) -> bool {
        self.macro_enabled
    }

    fn set_status(&mut self, status: &str) {
        status.clone_into(self.status);
    }

    fn announce(&mut self, automation: &mut dyn Automation, text: &str) {
        post_chat(automation, &format!("{CHAT_PREFIX} {text}"));
    }

    fn log(&mut self, channel: MacroLogChannel, message: &str) {
        self.lines.borrow_mut().push(QueuedLine::Macro {
            channel,
            message: message.to_owned(),
            chat: true,
        });
    }

    fn mirror_to_log(&mut self, channel: MacroLogChannel, message: &str) {
        self.lines.borrow_mut().push(QueuedLine::Macro {
            channel,
            message: message.to_owned(),
            chat: false,
        });
    }

    fn capture_selection(&mut self, automation: &mut dyn Automation) {
        if self.selection_before_pass.is_none() {
            *self.selection_before_pass = automation.selected_object();
        }
    }

    fn restore_selection(&mut self, automation: &mut dyn Automation) {
        restore_selection(automation, self.selection_before_pass);
    }

    fn stop_from_buff_rule(&mut self, _automation: &mut dyn Automation, status: &str) {
        self.stop_request = Some(status.to_owned());
    }

    /// `BeginFastCastMovement`.
    fn begin_fast_cast(
        &mut self,
        automation: &mut dyn Automation,
        spell: &ac_plugin_abi::magic::SpellInfo,
    ) {
        if !self.fast_cast_buffs || !is_vtank_instant_cast(spell) {
            return;
        }
        if spell.school == 34 || spell.school == 43 {
            return;
        }
        let result = automation.set_movement_intent(ac_plugin_abi::navigation::MovementIntent {
            forward: true,
            ..ac_plugin_abi::navigation::MovementIntent::default()
        });
        if result != ac_plugin_abi::navigation::NavigationStatus::Accepted {
            return;
        }
        *self.fast_cast = FastCast {
            active: true,
            start_completion_revision: automation.last_cast_completion().revision,
            elapsed: 0.0,
        };
    }
}

/// `IsVtankInstantCast`.
fn is_vtank_instant_cast(spell: &ac_plugin_abi::magic::SpellInfo) -> bool {
    if spell.difficulty < 50 {
        return true;
    }
    if spell.is_untargeted
        && !spell.is_fellowship
        && spell.duration_seconds >= 60.0
        && (spell.school == 31 || spell.school == 33)
    {
        return true;
    }
    (243..=249).contains(&spell.family) || spell.family == 639
}

impl Tank {
    /// The shell for a session, its profiles read for whoever is in the
    /// world (the C#'s panel constructor). `seed` seeds the worn-gear
    /// appraisal spread (register MANA-1).
    pub fn new(client: &mut dyn TankClient, seed: i32) -> Self {
        let clock = Rc::new(HostClock::default());
        clock.set(&client.clock());
        let lines: LineQueue = Rc::new(RefCell::new(Vec::new()));
        let locks = ActionLockTable::shared();
        let gate: SharedCombatModeGate = Rc::new(RefCell::new(CombatModeGate::new()));
        let tracker: SharedCastTracker = Rc::new(RefCell::new(SpellCastTracker::new()));
        let first_run_guidance_pending = needs_first_run_guidance(client);
        let session = session::SessionParts::new(client, clock.clone());

        let folder_lines = lines.clone();
        let announce_lines = lines.clone();
        let folders = StorageFolderPreparer::new(
            move |line| {
                folder_lines
                    .borrow_mut()
                    .push(QueuedLine::Warn(line.to_owned()));
            },
            move |line| {
                announce_lines
                    .borrow_mut()
                    .push(QueuedLine::Chat(format!("{CHAT_PREFIX} {line}")));
            },
        );

        let game_info = profiles::load_game_info(client);
        let crafting = mosstank_loot::crafting::CraftingController::new(game_info.clone());
        // The random helper's own draws, seeded apart from the worn-gear
        // spread (register MANA-1).
        let helper_seed = i32::try_from(client.clock().random_seed & 0x7FFF_FFFF).unwrap_or(0);
        let profiles = ProfileStore::new(client.stores());

        let mut combat = CombatController::with(game_info.clone(), tracker.clone());
        combat.bind_combat_mode_gate(gate.clone());
        let mut buff_rule = BuffSelfRule::new();
        buff_rule.bind_combat_mode_gate(gate.clone());
        buff_rule.bind_cast_tracker(tracker.clone());
        buff_rule.bind_consumables(locks.clone());
        let mut vital_recharge = VitalRechargeController::new();
        let mut vital_helper_recharge = VitalRechargeController::new();
        for recharge in [&mut vital_recharge, &mut vital_helper_recharge] {
            recharge.bind_action_locks(locks.clone());
            recharge.bind_combat_mode_gate(gate.clone());
            recharge.bind_cast_tracker(tracker.clone());
        }
        let mut dispel = DispelController::new();
        dispel.bind_combat_mode_gate(gate.clone());
        dispel.bind_action_locks(locks.clone());
        gate.borrow_mut().bind_action_locks(locks.clone());
        let mut item_mana = ItemManaRechargeController::new(seed);
        item_mana.bind_action_locks(locks.clone());
        let mut loot = LootController::new(mosstank_settings::model::LootSettings::default());
        loot.bind_action_locks(locks.clone());
        let mut corpse_approach = CorpseApproachController::new();
        corpse_approach.bind_combat_mode_gate(gate.clone());
        let random_helper = mosstank_buffs::random_helper::RandomHelperRule::new(
            locks.clone(),
            gate.clone(),
            helper_seed,
        );
        let mut navigation = NavigationController::new();
        navigation.bind_action_locks(locks.clone());
        navigation.bind_combat_mode_gate(gate.clone());
        let mut vendor_trade = mosstank_vendor::VendorTradeController::new();
        vendor_trade.bind_action_locks(locks.clone());

        let meta_profiles = MetaProfileStore::new();
        let expressions = ExpressionRuntime::new(client.meta(), Some(seed), clock.clone());
        let mut meta = MetaEngine::new(
            client.meta(),
            expressions,
            mosstank_formats::meta::MetaProfile::default(),
        );

        let mut core = Core {
            settings: AllSettings::default(),
            no_buff_item_names: OrdinalStringSet::new(),
            command_log_types: IgnoreCaseStringSet::new(),
            session_log_types: IgnoreCaseStringSet::new(),
            profiles,
            loot_profiles: LootProfileStore::new(),
            route_profiles: RouteProfileStore::new(),
            meta_profiles,
            embedded_route_label: None,
            folders,
            queued_announcements: Vec::new(),
            game_info,
            locks,
            gate,
            tracker,
            hold: PassHold::default(),
            lines,
            clock,
            combat,
            buff_rule,
            vital_recharge,
            vital_helper_recharge,
            dispel,
            item_mana,
            loot,
            corpse_approach,
            read_scroll: ReadScrollController::new(),
            inventory_maintenance:
                mosstank_ub::tools::stack_cram::InventoryMaintenanceController::new(),
            crafting,
            normal_pet_refill: mosstank_combat::pets::PetRefillRule::new(),
            idle_pet_refill: mosstank_combat::pets::PetRefillRule::new(),
            summon_pet: mosstank_combat::pets::SummonPetRule::new(),
            random_helper,
            navigation,
            fellowship: FellowshipManager::new(),
            vendor_trade,
            scheduler_running: false,
            idle_peace: Rc::new(RefCell::new(IdlePeaceRule::default())),
            status: "Idle.".to_owned(),
            profile_lifecycle_notice: "Macro settings are stored by character.".to_owned(),
            loot_editor_notice: "Add a rule or select one to edit.".to_owned(),
            route_notice: "Add the current position or a selected object.".to_owned(),
            meta_notice: "Add a rule or select one to edit.".to_owned(),
            corpse_id_wait_decided: false,
            navigation_waits_on_corpse_id: false,
            route_held_for_rare: false,
            death_saved: DeathSaved::default(),
            was_dead_for_macro: false,
            automation_was_available: false,
            prologue_owns_action: false,
            transaction_suspension_held: false,
            transaction_suspension_elapsed: 0.0,
            poke: PokeSnapshot::default(),
            cast_tracker_chat_sequence: 0,
            reported_suspension: None,
            walk_clock: 0.0,
            last_attack_seconds: f64::NEG_INFINITY,
            selection_before_pass: None,
            fast_cast: FastCast::default(),
            applying_profile_options: false,
            initialized: false,
            autostart_applied_for_current_session: false,
            command_portal_state: false,
            command_portal_count: 0,
            first_run_guidance_pending,
            meta_interval_pending: None,
            route_after_kill: Rc::new(Cell::new(false)),
            panel: panel::PanelState::default(),
            peers: peers::PeerState::default(),
            session,
        };
        core.bind_controller_hooks();
        // The history hears every new status line of the fight, and starts
        // with the one it has (`RecordActionHistory`).
        let history = core.session.history.clone();
        core.combat
            .set_status_changed(Some(Box::new(move |status: &str| {
                history.borrow_mut().record_combat(status);
            })));
        let status = core.combat.status().to_owned();
        core.session.history.borrow_mut().record_combat(&status);

        // Before any store reads anything: every key they use is addressed
        // inside the plugin's own folder.
        core.migrate_file_layout(client);
        core.folders.prepare_start_on(client.stores());

        let mut ub = ub::UbParts::new(client);
        // The UB store's binding lays out the character's own folders
        // (`InitializeUbSettings`).
        {
            let character = client.rules().character();
            core.folders.prepare_character(
                client
                    .stores()
                    .storage(mosstank_stores::storage::Root::Profiles),
                Some(&character.world),
                Some(&character.name),
            );
        }
        {
            // The sharing switch and the peer tag are read from the UB settings
            // on every look, so an edit applies at once.
            let enabled = ub.reader.clone();
            let tag = ub.reader.clone();
            core.combat.bind_cast_sharing(
                Box::new(move || {
                    mosstank_ub::catalog::UbCatalog::flag(&*enabled, "Sharing.Vitals")
                }),
                Box::new(move || {
                    Some(mosstank_ub::catalog::UbCatalog::value(&*tag, "Sharing.CastTag").text)
                }),
            );
        }
        let mut scheduler = rules::build_scheduler(&core.idle_peace);
        core.hold = scheduler.pass_hold();
        {
            let suspend = core.hold.clone();
            let resume = core.hold.clone();
            core.combat.bind_pass_suspension(
                Box::new(move || suspend.suspend()),
                Box::new(move || resume.resume()),
            );
        }
        services::register_service_functions(&mut meta);
        {
            let mut cx = TankCx {
                client: &mut *client,
                core: &mut core,
                meta: &mut meta,
                ub: &mut ub,
            };
            cx.load_initial_profiles();
        }
        scheduler.meta_interval_seconds =
            f64::from(core.profiles.meta_interval_milliseconds()) / 1000.0;
        meta.set_interval_milliseconds(core.profiles.meta_interval_milliseconds());
        meta.set_enabled(core.settings.meta.enabled);
        core.initialized = true;
        {
            let mut cx = TankCx {
                client: &mut *client,
                core: &mut core,
                meta: &mut meta,
                ub: &mut ub,
            };
            cx.apply_persisted_option_overrides();
            cx.refresh_editors();
            cx.refresh_meta_editor();
        }
        core.automation_was_available = client.rules().is_available();
        core.write_out(client);
        Self {
            scheduler,
            meta,
            core,
            ub,
        }
    }

    /// The call's state over this shell.
    pub(crate) fn cx<'a>(
        &'a mut self,
        client: &'a mut dyn TankClient,
    ) -> (TankCx<'a>, &'a mut MacroScheduler<TankFamily>) {
        (
            TankCx {
                client,
                core: &mut self.core,
                meta: &mut self.meta,
                ub: &mut self.ub,
            },
            &mut self.scheduler,
        )
    }

    /// The status line.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.core.status
    }

    /// Whether the macro runs (`CombatMacroRunning`).
    #[must_use]
    pub fn macro_running(&self) -> bool {
        self.core.combat.enabled()
    }

    /// The live loot rules, for the classifier.
    #[must_use]
    pub fn live_loot_rules(&self) -> &[mosstank_formats::utl::LootRule] {
        &self.core.settings.inventory.loot.rules
    }

    /// The rules of the running list, by name, in order.
    #[must_use]
    pub fn rule_names(&self) -> Vec<String> {
        self.scheduler
            .main_rules()
            .iter()
            .map(|rule| rule.name())
            .collect()
    }

    /// The rules of the stopped-macro list, by name.
    #[must_use]
    pub fn macro_disabled_rule_names(&self) -> Vec<String> {
        self.scheduler
            .macro_disabled_rules()
            .iter()
            .map(|rule| rule.name())
            .collect()
    }

    /// How many rule passes ran.
    #[must_use]
    pub fn pass_count(&self) -> i64 {
        self.scheduler.pass_count()
    }

    /// The meta's state.
    #[must_use]
    pub fn meta_state(&self) -> &str {
        self.meta.current_state()
    }

    /// Whether the meta runs.
    #[must_use]
    pub fn meta_enabled(&self) -> bool {
        self.meta.enabled()
    }

    /// The settings in force.
    #[must_use]
    pub fn settings(&self) -> &AllSettings {
        &self.core.settings
    }

    /// The route's waypoint being walked to.
    #[must_use]
    pub fn route_waypoint_index(&self) -> i32 {
        self.core.navigation.current_waypoint_index()
    }

    /// Whether one of the shared action slots is held now
    /// (`ActionLocks.IsLocked`).
    #[must_use]
    pub fn is_action_locked(&self, kind: mosstank_automation::locks::ActionLockKind) -> bool {
        self.core.locks.borrow_mut().is_locked(kind)
    }
}

fn needs_first_run_guidance(client: &mut dyn TankClient) -> bool {
    let storage = client
        .stores()
        .storage(mosstank_stores::storage::Root::Plugin);
    if !storage.is_available() {
        return false;
    }
    match storage.read_text(ONBOARDING_KEY) {
        Ok(text) => text.is_none_or(|text| text.trim().is_empty()),
        Err(error) => {
            client.rules().log_warn(&format!(
                "MossTank could not read first-run guidance state: {error}"
            ));
            false
        }
    }
}

impl Tank {
    /// A meta interval set during the call reaches the scheduler.
    pub(crate) fn apply_pending_meta_interval(&mut self) {
        if let Some(seconds) = self.core.meta_interval_pending.take() {
            self.scheduler.meta_interval_seconds = seconds;
        }
    }

    /// One line typed under `/vt` (`ExecuteVtankCommand`).
    pub fn execute_vtank_command(&mut self, client: &mut dyn TankClient, arguments: &str) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        commands::execute_vtank(&mut cx, arguments);
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// One line typed under `/ub` (`ExecuteUbCommand`).
    pub fn execute_ub_command(&mut self, client: &mut dyn TankClient, arguments: &str) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        commands::execute_ub(&mut cx, arguments);
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// What the macro needs the character for, so a walk the client plans
    /// waits for it (`WalkPauseReason`).
    #[must_use]
    pub fn walk_pause_reason(&self) -> Option<String> {
        let last_rule = self
            .scheduler
            .last_executed_index()
            .map(|index| self.scheduler.main_rules()[index].name());
        mosstank_nav::route_view::walk_pause_reason_for(
            self.scheduler.is_running(),
            last_rule.as_deref(),
            self.core.buff_rule.is_bursting(),
            mosstank_nav::route_view::route_holds_client_walks(
                self.core.settings.navigation.enabled,
                self.core.navigation.is_client_walking(),
            ),
            self.core.settings.inventory.loot.enabled,
            self.core.walk_clock - self.core.last_attack_seconds,
        )
    }

    /// A placement the client announced: the equipment profile's run reads
    /// its receipts (`PlacementObserved`).
    pub fn on_equipment_placement(
        &mut self,
        observation: &ac_plugin_abi::items::EquipmentObservation,
    ) {
        self.ub
            .commands
            .tools_mut()
            .equip
            .on_equipment_placement(observation.clone());
    }

    /// The client's notifications the meta engine follows (register
    /// META-3).
    pub fn on_event(&mut self, client: &mut dyn TankClient, event: &ac_plugin_abi::Event) {
        use ac_plugin_abi::Event;
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        // The kill statistics hear the client before the meta does: the C#
        // recorder subscribes first.
        cx.core.kill_statistics_event(cx.client, event);
        match event {
            Event::Net(ac_plugin_abi::net::NetEvent::HttpCompleted(reply)) => {
                cx.core.on_http_reply(cx.client, reply);
            }
            Event::Ui(ac_plugin_abi::ui::UiEvent::Pointer(pointer)) => {
                cx.on_canvas_pointer(pointer);
            }
            Event::LoginComplete => cx.meta.on_login_complete(),
            Event::Logoff => {
                cx.with_engine(|engine, host, services| engine.on_logoff(host, services));
            }
            Event::LocalPlayerDied(message) => cx.meta.on_local_player_died(message),
            Event::PortalTransition(transition) => cx.meta.on_portal_transition(transition),
            Event::ItemUseCompleted(completion) => cx.meta.on_item_use_completed(completion),
            Event::ContainerOpened(container) => cx.meta.on_container_opened(*container),
            Event::ContainerClosed(container) => cx.meta.on_container_closed(*container),
            // The vendor run hears the counter open, close and answer
            // (register VEND-2).
            Event::Vendor(vendor) => match vendor {
                ac_plugin_abi::trade::VendorEvent::Opened(id) => {
                    cx.core.vendor_trade.on_vendor_opened(*id);
                }
                ac_plugin_abi::trade::VendorEvent::Closed => {
                    cx.core.vendor_trade.on_vendor_closed();
                }
                ac_plugin_abi::trade::VendorEvent::TransactionCompleted(transaction) => {
                    cx.core
                        .vendor_trade
                        .on_transaction_completed(transaction.clone());
                }
            },
            // The name tags follow objects arriving, appraised and leaving.
            Event::ObjectChanged(change) => {
                cx.with_ub(|commands, call| {
                    commands
                        .tools_mut()
                        .nametags
                        .on_object_changed(call.host, change);
                });
            }
            Event::ConfirmationRequested(confirmation) => {
                cx.meta.on_confirmation_requested(confirmation);
                // The dialog watch answers the question it was told to.
                cx.with_ub(|commands, call| commands.on_confirmation(call, confirmation));
            }
            _ => {}
        }
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// The plugin is stopping (`MossTankPanel.Disable` and `Dispose`, the
    /// parts the shell has).
    pub fn disable(&mut self, client: &mut dyn TankClient) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.core.release_peer_subscriptions(cx.client);
        cx.core.dispose_session_records(cx.client);
        // The UB tools' runs end with the plugin, and the name tags give
        // their labels back.
        cx.with_ub(|commands, call| {
            commands.tools_mut().reset_session(call);
            commands.tools_mut().nametags.dispose(call.host);
        });
        cx.dispose_display_tools();
        if cx.core.combat.enabled() {
            cx.core.set_macro_running(cx.client, false);
        }
        if cx.core.buff_rule.is_bursting() {
            cx.core.stop(cx.client, "Stopped.");
        }
        let core = &mut *cx.core;
        core.vital_recharge.reset();
        core.vital_helper_recharge.reset();
        core.inventory_maintenance.reset();
        core.crafting.reset();
        core.item_mana.reset();
        {
            let lent = rules::lent_loot!(core);
            lent.loot.reset(cx.client.looter());
        }
        {
            let mut turn = rules::route_turn!(cx.client, core);
            core.navigation.reset(&mut turn);
        }
        core.vendor_trade.reset();
        core.fellowship.reset();
        cx.meta.set_enabled(false);
        cx.meta.views().destroy_all(cx.client.meta());
        cx.meta
            .expressions()
            .destroy_auxiliary_views(cx.client.meta());
        cx.meta.expressions().clear_session();
        // A key the macro held must not outlive the plugin that pressed it.
        cx.meta
            .expressions()
            .held_motions()
            .release_if_held(cx.client.meta());
        cx.core.write_out(cx.client);
        let storage = cx
            .client
            .stores()
            .storage(mosstank_stores::storage::Root::Plugin);
        cx.core.buff_rule.item_ledger_mut().flush(storage);
    }
}

impl Core {
    /// The controllers' log lines and warnings go to the shell's queue, the
    /// fight's kill hold reads the looting switch (`BindActionLocks`).
    pub(crate) fn bind_controller_hooks(&mut self) {
        let lines = self.lines.clone();
        let log: mosstank_combat::MacroLog = Rc::new(move |channel, message: &str| {
            lines.borrow_mut().push(QueuedLine::Macro {
                channel,
                message: message.to_owned(),
                chat: true,
            });
        });
        self.combat.set_log(Some(log.clone()));
        self.gate.borrow_mut().set_log(Some(log));
        let route_after_kill = self.route_after_kill.clone();
        self.combat
            .bind_action_locks(self.locks.clone(), Box::new(move || route_after_kill.get()));
        let lines = self.lines.clone();
        self.loot.log = Some(Box::new(move |message: &str| {
            lines.borrow_mut().push(QueuedLine::Macro {
                channel: MacroLogChannel::Loot,
                message: message.to_owned(),
                chat: true,
            });
        }));
        let lines = self.lines.clone();
        self.loot.warning = Some(Box::new(move |text: &str| {
            lines
                .borrow_mut()
                .push(QueuedLine::Chat(format!("{CHAT_PREFIX} {text}")));
        }));
        let lines = self.lines.clone();
        self.crafting.warning = Some(Box::new(move |text: &str| {
            lines
                .borrow_mut()
                .push(QueuedLine::Chat(format!("{CHAT_PREFIX} {text}")));
        }));
        // The panel's peace gate: an item is crafted from peace mode.
        let gate = self.gate.clone();
        self.crafting.ready_to_craft_in_peace = Some(Box::new(move |turn| {
            let equipment = if turn.automation.equipment_available() {
                turn.automation.owned_equipment()
            } else {
                Vec::new()
            };
            gate.borrow_mut().try_drop_to_peace(
                turn.automation,
                turn.profiles,
                turn.vitals,
                &equipment,
                "an item",
            )
        }));
        let lines = self.lines.clone();
        self.random_helper.log = Some(Box::new(move |channel, message: &str| {
            lines.borrow_mut().push(QueuedLine::Macro {
                channel,
                message: message.to_owned(),
                chat: true,
            });
        }));
        let lines = self.lines.clone();
        self.read_scroll.log = Some(Box::new(move |message: &str| {
            lines.borrow_mut().push(QueuedLine::Macro {
                channel: MacroLogChannel::Loot,
                message: message.to_owned(),
                chat: true,
            });
        }));
    }

    /// The fight reads whether looting holds the route after a kill
    /// (`_inventorySettings.Loot.HoldsRouteAfterKill`, read live in the
    /// C#): kept current before every call that can kill.
    pub(crate) fn sync_route_after_kill(&self) {
        self.route_after_kill
            .set(self.settings.inventory.loot.holds_route_after_kill());
    }
}

impl Tank {
    /// What kind of rule fills each slot of the running list, in order.
    #[must_use]
    pub fn rule_shapes(&self) -> Vec<(String, mosstank_meta::scheduler::RuleShape)> {
        self.scheduler
            .main_rules()
            .iter()
            .map(|rule| (rule.name(), rule.shape()))
            .collect()
    }
}

impl Tank {
    /// The selected settings profile (`SelectedMacroProfile`).
    #[must_use]
    pub fn selected_settings_profile(&self) -> String {
        self.core.profiles.selected()
    }

    /// The selected loot profile (`LootProfileName`).
    #[must_use]
    pub fn selected_loot_profile(&self) -> String {
        self.core.loot_profiles.selected()
    }

    /// The route shown as selected (`SelectedRouteProfile`): a meta's own
    /// route by its label, else the selected file.
    pub fn selected_route_profile(&self, client: &mut dyn TankClient) -> String {
        self.core.embedded_route_label.clone().unwrap_or_else(|| {
            self.core
                .route_profiles
                .selected(client.stores())
                .unwrap_or_default()
        })
    }

    /// The selected meta profile (`SelectedMetaProfile`).
    pub fn selected_meta_profile(&self, client: &mut dyn TankClient) -> String {
        self.core
            .meta_profiles
            .selected(client.stores())
            .unwrap_or_default()
    }

    /// An option's value (`GetMetaOption`).
    #[must_use]
    pub fn option(&self, name: &str) -> mosstank_expr::ExpressionValue {
        self.core.get_meta_option(self.meta.enabled(), name)
    }

    /// How many rules the meta has.
    #[must_use]
    pub fn meta_rule_count(&self) -> usize {
        self.meta.profile().rules.len()
    }

    /// The route's notice (`RouteNotice`).
    #[must_use]
    pub fn route_notice(&self) -> &str {
        &self.core.route_notice
    }

    /// The profile lifecycle notice (`ProfileLifecycleNotice`).
    #[must_use]
    pub fn profile_notice(&self) -> &str {
        &self.core.profile_lifecycle_notice
    }
}

impl Tank {
    /// The start of every call: the client shows UB lines as the UB
    /// settings say.
    pub(crate) fn begin_call(&mut self, client: &mut dyn TankClient) {
        self.core.clock.set(&client.clock());
        self.core.scheduler_running = self.scheduler.is_running();
        client.set_ub_displays(self.ub.displays());
    }

    /// The end of every call: the UB settings changed in it are written
    /// (register STORE-1), and a new meta interval reaches the scheduler.
    pub(crate) fn end_call(&mut self, client: &mut dyn TankClient) {
        if self.core.panel.items_dirty {
            let (mut cx, _) = self.cx(client);
            cx.refresh_item_editors();
        }
        self.ub.flush(client);
        // The item timers a call changed are written before it returns
        // (register STORE-6), whatever kind of call it was.
        let storage = client
            .stores()
            .storage(mosstank_stores::storage::Root::Plugin);
        self.core.buff_rule.item_ledger_mut().flush(storage);
        self.apply_pending_meta_interval();
    }
}

impl Tank {
    /// Runs an expression with the meta's functions and the plugin's
    /// services (`EvaluateExpression`).
    ///
    /// # Errors
    /// The text is not an expression, or the run failed: the message.
    pub fn evaluate_expression(
        &mut self,
        client: &mut dyn TankClient,
        source: &str,
    ) -> Result<mosstank_expr::ExpressionValue, String> {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        let result =
            cx.with_engine(|engine, host, services| engine.evaluate(host, services, source));
        cx.core.write_out(cx.client);
        self.end_call(client);
        result.map_err(|error| error.message())
    }
}
