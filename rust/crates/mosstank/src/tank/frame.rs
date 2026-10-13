//! The frame (`MossTankPanel.OnTick` and the loop around it): the session's
//! edges, the receipts that wake the pass, the cast tracker, the hold on
//! the pass while a transaction is in flight, the movers and the swing
//! stepped on the host's frame, and the macro's start, stop and death.
//!
//! The session's own records (the action history, the kill statistics, the
//! game database check, the published meta state) are in `session`, the
//! peer features in `peers`. The parts of the C#'s frame that belong to the
//! panel's windows are not here (MT-11): the nav lines, the maps and the
//! item admission.

use mosstank_automation::cast_tracker::SpellCastTracker;
use mosstank_buffs::buff_rule::BuffTurn;
use mosstank_buffs::mana::ManaTurn;
use mosstank_buffs::recharge_controller::VitalTurn;

use mosstank_combat::mover::CreepSettings;
use mosstank_expr::ExpressionValue;
use mosstank_loot::corpse_approach::ApproachTurn;
use mosstank_meta::scheduler::MacroLogChannel;
use mosstank_nav::RouteTurn;
use mosstank_ub::tools::PrologueOwners;

use super::rules::{LentLoot, buff_owner, combat_ctx, combat_turn, lent_loot, route_turn};
use super::{Core, ONBOARDING_KEY, Tank, TankCx};
use crate::client::TankClient;

/// What a death tells the player (`DeathNotice`).
pub(crate) const DEATH_NOTICE: &str = "You died! Buffing, Combat, Nav and Loot have been disabled. Use /vt deathrestore to restore them.";

/// What `/vt deathrestore` tells the player (`DeathRestoredNotice`).
pub(crate) const DEATH_RESTORED_NOTICE: &str =
    "Buffing, Combat, Nav and Loot are back to their earlier values.";

/// How long the pass is held for a transaction before the watchdog lets it
/// go: the cast tracker's worst case.
const TRANSACTION_SUSPENSION_WATCHDOG_SECONDS: f64 = SpellCastTracker::WORST_CASE_BUSY_SECONDS;

/// The chat kind of local speech (`SpellCastTracker.LocalSpeechChatKind`).
const LOCAL_SPEECH_CHAT_KIND: u32 = 0;

impl Tank {
    /// One frame (`OnTick`).
    #[allow(clippy::too_many_lines)]
    pub fn on_tick(&mut self, client: &mut dyn TankClient, elapsed_seconds: f64) {
        self.begin_call(client);
        self.frame(client, elapsed_seconds);
        self.end_call(client);
    }

    /// The frame itself.
    #[allow(clippy::too_many_lines)]
    fn frame(&mut self, client: &mut dyn TankClient, elapsed_seconds: f64) {
        let clock = client.clock();
        self.core.clock.set(&clock);
        self.core
            .buff_rule
            .item_ledger_mut()
            .set_utc_now(clock.utc_unix_ms);
        let (mut cx, scheduler) = self.cx(client);
        cx.observe_macro_activity(scheduler, elapsed_seconds);
        cx.core.tick_kill_statistics(cx.client);

        let available = cx.client.rules().is_available();
        if !available {
            if cx.core.automation_was_available {
                cx.handle_session_ended();
            }
            cx.core.automation_was_available = false;
            cx.refresh_display_bindings(elapsed_seconds);
            cx.core.write_out(cx.client);
            return;
        }
        if !cx.core.automation_was_available {
            cx.core.automation_was_available = true;
            cx.handle_session_started();
        }
        cx.update_peer_subscriptions();
        // The name tags, first in the frame as the panel runs them.
        cx.with_ub(|commands, call| {
            commands.tools_mut().tick_nametags(call, elapsed_seconds);
        });
        cx.tick_display_tools(elapsed_seconds);
        cx.tick_dungeon_map(elapsed_seconds);
        cx.paint_display_tools();
        let meta_state = cx.meta.current_state().to_owned();
        cx.core.publish_status(cx.client, &meta_state);

        cx.core.sync_route_after_kill();
        cx.core.locks.borrow_mut().advance(elapsed_seconds);
        // Beside the slot clock rather than inside a rule pass: the slots
        // an open takes include the one the loot rule is gated on.
        {
            let core = &mut *cx.core;
            let lent = lent_loot!(core);
            lent.loot.observe_mana_donors(cx.client.looter());
            if lent.loot.observe_corpse_opened(cx.client.looter()) {
                scheduler.poke();
            }
            if core.combat.enabled() {
                lent.loot
                    .tick_identification(cx.client.looter(), elapsed_seconds);
            }
        }
        cx.observe_fast_cast_movement(elapsed_seconds);
        {
            let core = &mut *cx.core;
            let mut owner = buff_owner!(core);
            core.buff_rule
                .advance(cx.client.rules(), &mut owner, elapsed_seconds);
        }
        cx.ensure_character_profile();
        cx.ensure_timer_persistence();
        cx.tick_configured_item_assessment(elapsed_seconds);
        cx.tick_profile_item_addition(elapsed_seconds);
        cx.core.flush_queued_announcements(cx.client);
        cx.core.tick_game_info_update(cx.client);
        cx.show_first_run_guidance();
        cx.observe_command_portal_state();
        // The UB commands' delayed lines and the dialog watch.
        cx.with_ub(|commands, call| commands.tick_early(call, elapsed_seconds));
        let macro_running = cx.core.combat.enabled();
        let character = cx.client.rules().character();
        let dead = character.health.1 > 0 && character.health.0 == 0;
        if dead && !cx.core.was_dead_for_macro {
            cx.handle_death(macro_running);
        }
        cx.core.was_dead_for_macro = dead;
        cx.meta
            .current_state()
            .clone_into(&mut cx.core.settings.combat.meta_state);
        // The tinker run, just before the command jump (`TickTinkering`).
        cx.with_ub(|commands, call| {
            commands.tools_mut().tick_tinkering(call, elapsed_seconds);
        });
        cx.refresh_display_bindings(elapsed_seconds);
        // The command jump, which owns the character while it lasts.
        let command_jump_owns_action =
            cx.with_ub(|commands, call| commands.tick_command_jump(call, elapsed_seconds));
        // The UB tools that own the character while they run, in the
        // panel's order, the vendor run among them.
        let can_act = !cx.core.buff_rule.is_bursting() && !command_jump_owns_action;
        let owners = cx.with_ub(|commands, call| {
            commands.tools_mut().tick_prologue(
                call,
                elapsed_seconds,
                can_act,
                &mut |call, can_act| {
                    call.services
                        .vendor_tick(call.host, elapsed_seconds, can_act)
                },
            )
        });
        cx.core.prologue_owns_action = command_jump_owns_action || owners.any();
        // Not owners: the experience spender and /ub clearbugged.
        cx.with_ub(|commands, call| {
            commands
                .tools_mut()
                .tick_after_prologue(call, elapsed_seconds);
        });
        cx.core.write_out(cx.client);

        cx.observe_scheduler_pokes(scheduler);

        // The macro's own switch, and nothing else: keeping gear charged
        // while the macro is off is the stopped-macro list's job.
        let scheduler_active = cx.core.combat.enabled();
        if scheduler_active && !scheduler.is_running() {
            cx.release_transaction_suspension(scheduler);
            scheduler.start(&mut cx);
            cx.core.scheduler_running = scheduler.is_running();
            cx.core.locks.borrow_mut().clear_all();
            cx.core.reset_once_per_run_warnings();
            cx.core.vendor_trade.on_macro_started();
        } else if !scheduler_active && scheduler.is_running() {
            cx.release_transaction_suspension(scheduler);
            scheduler.stop(&mut cx);
            cx.core.scheduler_running = scheduler.is_running();
        }
        cx.observe_cast_result(elapsed_seconds);
        cx.observe_pending_transactions(elapsed_seconds);
        cx.observe_cast_suspension(scheduler, elapsed_seconds);
        // A turn in flight freezes the pass, so it needs a driver beside the
        // pass rather than inside it.
        cx.core
            .combat
            .advance_held_turn(cx.client.rules(), elapsed_seconds);
        cx.core.write_out(cx.client);
        // What the other clients cast is read on the frame too.
        {
            let core = &mut *cx.core;
            let mut ctx = combat_ctx!(cx.client, core);
            core.combat.observe_remote_casts(&mut ctx);
        }
        cx.core.write_out(cx.client);
        scheduler.external_suspension = cx.core.prologue_owns_action;
        cx.report_scheduler_suspension(scheduler, command_jump_owns_action, owners);
        // The movers run on the host's frame, before the pass, so the pass
        // sees the frame's work done. Only one of them is ever armed.
        {
            let core = &mut *cx.core;
            let clear = core.navigation_slots_are_clear();
            let mut turn: RouteTurn<'_> = route_turn!(cx.client, core);
            core.navigation
                .step_armed_mover(&mut turn, elapsed_seconds, clear);
        }
        cx.core.write_out(cx.client);
        {
            let core = &mut *cx.core;
            let lent = LentLoot::new(&mut core.loot, &mut core.settings.inventory.loot);
            let mut turn = ApproachTurn {
                host: cx.client.looter(),
                loot: &mut *lent.loot,
                creep: Some(CreepSettings {
                    combat: &core.settings.combat,
                    vitals: &core.settings.vitals,
                }),
            };
            core.corpse_approach
                .step_armed_mover(&mut turn, elapsed_seconds);
        }
        cx.core.write_out(cx.client);
        {
            let core = &mut *cx.core;
            let mut ctx = combat_ctx!(cx.client, core);
            core.combat
                .step_armed_approach_mover(&mut ctx, elapsed_seconds);
        }
        cx.core.write_out(cx.client);
        // The swing runs on the host's frame, not on the pass that armed it.
        cx.core
            .combat
            .drive_swing_executor(cx.client.rules(), elapsed_seconds);
        cx.core.write_out(cx.client);
        scheduler.advance(&mut cx, elapsed_seconds);
        // After the pass: a held item's cast is driven by the frame on every
        // frame the attack's own turn did not drive it.
        {
            let core = &mut *cx.core;
            let mut ctx = combat_ctx!(cx.client, core);
            core.combat
                .observe_held_item_cast(&mut ctx, elapsed_seconds);
        }
        if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            cx.core.walk_clock += elapsed_seconds;
        }
        if scheduler
            .last_executed_index()
            .is_some_and(|index| scheduler.main_rules()[index].name() == "Attack")
        {
            cx.core.last_attack_seconds = cx.core.walk_clock;
        }
        cx.core
            .gate
            .borrow_mut()
            .advance_pass(cx.client.rules(), elapsed_seconds);
        // The idle-peace owner's status on the pass it wins.
        {
            let idle = cx.core.idle_peace.borrow();
            if mosstank_meta::scheduler::MacroRule::<super::TankFamily>::running(&*idle)
                && let Some(status) = idle.status.clone()
            {
                drop(idle);
                cx.core.status = status;
            }
        }
        if cx.core.panel.active_tab == super::panel::TankTab::Route {
            cx.refresh_route_editor();
        }
        cx.core.write_out(cx.client);
        cx.flush_timer_persistence();
    }

    /// Starts or stops the macro (`SetMacroRunning`).
    pub fn set_macro_running(&mut self, client: &mut dyn TankClient, running: bool) {
        self.begin_call(client);
        let (cx, _) = self.cx(client);
        cx.core.set_macro_running_with(cx.client, running);
        cx.core.write_out(cx.client);
        self.end_call(client);
    }
}

impl TankCx<'_> {
    /// `HandleSessionEnded`.
    fn handle_session_ended(&mut self) {
        self.core.release_peer_subscriptions(self.client);
        self.core.peers.forget_expression_requirements();
        self.core.end_session_records();
        self.with_ub(|commands, call| commands.tools_mut().nametags.reset(call.host));
        self.reset_display_tools();
        if self.core.combat.enabled() {
            let core = &mut *self.core;
            let mut turn = combat_turn!(self.client, core);
            core.combat.on_tick(&mut turn, 0.0);
            core.forward_cast_outcomes();
        }
        self.core.clear_fast_cast_movement(self.client);
        self.core.buff_rule.reset();
        self.core.selection_before_pass = None;
        "Lost the session.".clone_into(&mut self.core.status);
        self.core.gate.borrow_mut().reset();
        self.reset_session_scoped_controllers();
    }

    /// `HandleSessionStarted`.
    fn handle_session_started(&mut self) {
        let core = &mut *self.core;
        self.meta.reset_session(self.client.meta());
        // A new session clears where the meta had got to, not whether the
        // profile asked for one.
        self.meta.set_enabled(core.settings.meta.enabled);
        self.meta.expressions().clear_session();
        self.meta
            .expressions()
            .destroy_auxiliary_views(self.client.meta());
        self.meta.views().destroy_all(self.client.meta());
        self.reset_command_session();
        self.core.buff_rule.reset();
        "Idle.".clone_into(&mut self.core.status);
    }

    /// `ResetSessionScopedControllers`.
    fn reset_session_scoped_controllers(&mut self) {
        let core = &mut *self.core;
        core.random_helper.reset();
        core.summon_pet.reset();
        core.normal_pet_refill.reset();
        core.idle_pet_refill.reset();
        core.crafting.reset();
        core.inventory_maintenance.reset();
        core.vital_recharge.reset();
        core.vital_helper_recharge.reset();
        core.dispel.reset();
        core.item_mana.reset();
        {
            let lent = lent_loot!(core);
            lent.loot.reset(self.client.looter());
        }
        {
            let mut turn: RouteTurn<'_> = route_turn!(self.client, core);
            core.navigation.reset(&mut turn);
        }
        core.vendor_trade.reset();
        core.fellowship.reset();
        self.meta.reset_session(self.client.meta());
        self.meta.views().destroy_all(self.client.meta());
        self.meta
            .expressions()
            .destroy_auxiliary_views(self.client.meta());
        self.meta.expressions().clear_session();
        self.with_ub(|commands, call| commands.tools_mut().reset_session(call));
        self.reset_command_session();
        self.core.buff_rule.reset();
        mosstank_meta::engine::DEFAULT_STATE_NAME
            .clone_into(&mut self.core.settings.combat.meta_state);
    }

    /// `ResetCommandSession`: the UB commands' jump, delayed lines, dialog
    /// watch and held keys, and the portal count.
    fn reset_command_session(&mut self) {
        self.with_ub(|commands, call| commands.reset_session(call));
        self.core.command_portal_state = self.client.rules().navigation_snapshot().is_portal_space;
        self.core.command_portal_count = 0;
    }

    /// `ObserveCommandPortalState`.
    fn observe_command_portal_state(&mut self) {
        let current = self.client.rules().navigation_snapshot().is_portal_space;
        if current != self.core.command_portal_state {
            self.core.command_portal_state = current;
            self.core.command_portal_count += 1;
        }
    }

    /// `ShowFirstRunGuidance`.
    fn show_first_run_guidance(&mut self) {
        if !self.core.first_run_guidance_pending || !self.client.rules().is_available() {
            return;
        }
        self.core.first_run_guidance_pending = false;
        const GUIDANCE: &str = "First run: choose profiles, configure the Options tab, then press Run Macro. Minimize with \u{2013}; MossTank keeps running from the right-side plugin shelf. Put VTank .nav/.utl/.met files in imports and use /vt nav, /vt loot, or /vt meta import <name>.";
        Core::announce(self.client, GUIDANCE);
        let written = self
            .client
            .stores()
            .storage(mosstank_stores::storage::Root::Plugin)
            .write_text(ONBOARDING_KEY, "shown");
        if let Err(error) = written {
            self.client.rules().log_warn(&format!(
                "MossTank could not persist first-run guidance state: {error}"
            ));
        }
    }

    /// The buff rule binds the item timers to whoever is in the world
    /// (`EnsureTimerPersistence`); a change waiting for the old character is
    /// written before the bind.
    fn ensure_timer_persistence(&mut self) {
        let core = &mut *self.core;
        let mut owner = buff_owner!(core);
        let character = self.client.rules().character();
        let available = self.client.rules().is_available();
        let storage = self
            .client
            .stores()
            .storage(mosstank_stores::storage::Root::Plugin);
        core.buff_rule
            .ensure_timer_persistence_with(available, &character, storage, &mut owner);
    }

    /// Writes the item timers that changed this frame (register STORE-6).
    fn flush_timer_persistence(&mut self) {
        let storage = self
            .client
            .stores()
            .storage(mosstank_stores::storage::Root::Plugin);
        self.core.buff_rule.item_ledger_mut().flush(storage);
    }

    /// `ObserveFastCastMovement`.
    fn observe_fast_cast_movement(&mut self, elapsed_seconds: f64) {
        let fast_cast = &mut self.core.fast_cast;
        if !fast_cast.active {
            return;
        }
        fast_cast.elapsed += elapsed_seconds.max(0.0);
        let automation = self.client.rules();
        let receipt_arrived =
            automation.last_cast_completion().revision != fast_cast.start_completion_revision;
        let cast_ended = fast_cast.elapsed >= 0.2 && !automation.is_casting();
        if receipt_arrived || cast_ended || fast_cast.elapsed >= 10.0 {
            self.core.clear_fast_cast_movement(self.client);
        }
    }

    /// `HandleDeath`: with the death setting on and the macro running, the
    /// four switches are saved and turned off, and a restore is offered.
    pub(crate) fn handle_death(&mut self, macro_running: bool) {
        self.core
            .buff_rule
            .invalidate_item_timers(self.client.rules());
        if !macro_running || !self.core.settings.combat.stop_macro_on_death {
            return;
        }
        let saved = super::DeathSaved {
            nav: self.get_meta_option("EnableNav").is_truthy(),
            loot: self.get_meta_option("EnableLooting").is_truthy(),
            buff: self.get_meta_option("EnableBuffing").is_truthy(),
            combat: self.get_meta_option("EnableCombat").is_truthy(),
        };
        self.core.death_saved = saved;
        for name in [
            "EnableNav",
            "EnableLooting",
            "EnableBuffing",
            "EnableCombat",
        ] {
            let _ = self.set_meta_option(name, &ExpressionValue::Boolean(false));
        }
        Core::announce(self.client, DEATH_NOTICE);
    }

    /// `RestoreAfterDeath`.
    pub(crate) fn restore_after_death(&mut self) {
        let saved = self.core.death_saved;
        let _ = self.set_meta_option("EnableNav", &ExpressionValue::Boolean(saved.nav));
        let _ = self.set_meta_option("EnableLooting", &ExpressionValue::Boolean(saved.loot));
        let _ = self.set_meta_option("EnableBuffing", &ExpressionValue::Boolean(saved.buff));
        let _ = self.set_meta_option("EnableCombat", &ExpressionValue::Boolean(saved.combat));
        Core::announce(self.client, DEATH_RESTORED_NOTICE);
    }

    /// Runs the next pass on the frame an answer the macro waits for
    /// arrives, rather than on the next heartbeat (`ObserveSchedulerPokes`).
    fn observe_scheduler_pokes(
        &mut self,
        scheduler: &mut mosstank_meta::scheduler::MacroScheduler<super::TankFamily>,
    ) {
        let automation = self.client.rules();
        let snapshot = super::PokeSnapshot {
            magic: automation.last_cast_completion().revision,
            items: automation.last_item_completion().revision,
            combat_mode: automation.combat_mode().tag(),
            equipping: automation.equipment_busy(),
            items_busy: automation.items_busy(),
            inventory: self.client.looter().last_inventory_completion().revision,
            appraisal: self.client.looter().appraisal().revision,
            container: self.client.looter().current_container_id(),
            contents_ready: self.client.looter().current_contents_ready(),
        };
        if snapshot == self.core.poke {
            return;
        }
        self.core.poke = snapshot;
        scheduler.poke();
    }

    /// The cast tracker hears chat and moves on every frame, and what it
    /// raised is heard at once (`ObserveCastResult`; register COMBAT-3).
    fn observe_cast_result(&mut self, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        let armed = self.core.tracker.borrow().is_busy();
        let own_id = self.client.rules().character().id;
        let messages = self
            .client
            .rules()
            .chat_messages(self.core.cast_tracker_chat_sequence);
        for message in messages {
            self.core.cast_tracker_chat_sequence =
                self.core.cast_tracker_chat_sequence.max(message.sequence);
            if armed {
                self.core.tracker.borrow_mut().observe_chat(
                    message.sequence,
                    &message.text,
                    message.kind == LOCAL_SPEECH_CHAT_KIND
                        && message.sender_id != 0
                        && message.sender_id == own_id,
                    message.log_text_type,
                );
            }
        }
        self.core.tracker.borrow_mut().advance(elapsed);
        let core = &mut *self.core;
        let mut ctx = combat_ctx!(self.client, core);
        core.combat.drain_cast_events(&mut ctx);
        core.forward_cast_outcomes();
    }

    /// Each transaction owner is watched on the host frame, so the hold
    /// ends when the answer lands (`ObservePendingTransactions`).
    fn observe_pending_transactions(&mut self, elapsed_seconds: f64) {
        let core = &mut *self.core;
        for helpers in [false, true] {
            let mut turn = VitalTurn {
                automation: self.client.rules(),
                settings: &core.settings.vitals,
                combat: &core.settings.combat,
            };
            let controller = if helpers {
                &mut core.vital_helper_recharge
            } else {
                &mut core.vital_recharge
            };
            controller.observe_pending_receipt(&mut turn, elapsed_seconds);
        }
        core.dispel
            .observe_pending_receipt(self.client.rules(), elapsed_seconds);
        core.crafting.observe_pending_receipt(
            self.client.rules(),
            &core.settings.inventory,
            elapsed_seconds,
        );
        {
            let mut turn = ManaTurn {
                automation: self.client.rules(),
                settings: &core.settings.inventory,
                profiles: &core.settings.combat,
            };
            core.item_mana
                .observe_pending_receipt(&mut turn, elapsed_seconds);
        }
        core.combat
            .observe_learned_debuff_receipt(self.client.rules(), elapsed_seconds);
        core.write_out(self.client);
    }

    /// The reference's global busy count: a cast, a wand cast, a kit use
    /// and a jump waypoint hold the pass (`ObserveCastSuspension`).
    fn observe_cast_suspension(
        &mut self,
        scheduler: &mut mosstank_meta::scheduler::MacroScheduler<super::TankFamily>,
        elapsed_seconds: f64,
    ) {
        let core = &*self.core;
        let in_flight = core.tracker.borrow().is_busy()
            || core.combat.held_item_cast_in_flight()
            || core.combat.learned_debuff_cast_in_flight()
            || core.dispel.cast_in_flight()
            || core.vital_recharge.holds_pass()
            || core.vital_helper_recharge.holds_pass()
            || core.crafting.use_in_flight()
            || core.navigation.holds_pass();
        if self.core.transaction_suspension_held {
            self.core.transaction_suspension_elapsed += elapsed_seconds.max(0.0);
            if in_flight
                && self.core.transaction_suspension_elapsed
                    < TRANSACTION_SUSPENSION_WATCHDOG_SECONDS
            {
                return;
            }
            self.release_transaction_suspension(scheduler);
            return;
        }
        if !in_flight {
            return;
        }
        self.core.hold_pass();
    }

    /// Gives back the one hold the shell raises, if it holds one
    /// (`ReleaseTransactionSuspension`).
    fn release_transaction_suspension(
        &mut self,
        scheduler: &mut mosstank_meta::scheduler::MacroScheduler<super::TankFamily>,
    ) {
        self.core.transaction_suspension_elapsed = 0.0;
        if !self.core.transaction_suspension_held {
            return;
        }
        self.core.transaction_suspension_held = false;
        scheduler.resume();
    }

    /// Says once when the pass is held, and why, and when it resumes
    /// (`ReportSchedulerSuspension`).
    fn report_scheduler_suspension(
        &mut self,
        scheduler: &mosstank_meta::scheduler::MacroScheduler<super::TankFamily>,
        command_jump_owns_action: bool,
        owners: PrologueOwners,
    ) {
        let cause = if !scheduler.is_running() {
            None
        } else if command_jump_owns_action {
            Some("a command jump owns the character")
        } else if owners.give {
            Some("a profile give owns the character")
        } else if owners.count {
            Some("a profile count owns the character")
        } else if owners.vendor {
            Some("a vendor run owns the character")
        } else if self.core.transaction_suspension_held {
            Some("a cast, a wand cast or a kit is in flight")
        } else if self.core.combat.turn_holds_pass() {
            Some("the combat turn holds the pass")
        } else if scheduler.is_suspended() {
            Some("held by a rule")
        } else {
            None
        };
        if cause == self.core.reported_suspension.as_deref() {
            return;
        }
        self.core.reported_suspension = cause.map(str::to_owned);
        self.core
            .observe_activity("Pause", cause.unwrap_or("Resumed."));
        let line = match cause {
            None => "(scheduler) resumed".to_owned(),
            Some(cause) => format!("(scheduler) suspended: {cause}"),
        };
        self.core
            .emit_macro_log(self.client, MacroLogChannel::RuleInfo, &line, true);
    }
}

impl Core {
    /// `HoldPass`.
    pub(crate) fn hold_pass(&mut self) {
        self.transaction_suspension_held = true;
        self.transaction_suspension_elapsed = 0.0;
        self.hold.suspend();
    }

    /// `SetMacroRunning` for a caller that writes out itself.
    pub(crate) fn set_macro_running(&mut self, client: &mut dyn TankClient, running: bool) {
        self.set_macro_running_with(client, running);
    }

    /// `SetMacroRunning`: starting anchors the round; stopping puts down
    /// what every rule had in flight.
    pub(crate) fn set_macro_running_with(&mut self, client: &mut dyn TankClient, running: bool) {
        if running && !self.profiles.has_active_profile() {
            self.status = self
                .profiles
                .load_failure_notice()
                .unwrap_or("Cannot start without a complete settings profile.")
                .to_owned();
            let status = self.status.clone();
            Core::announce(client, &status);
            return;
        }
        if self.combat.enabled() == running {
            return;
        }
        {
            let mut turn = combat_turn!(client, self);
            self.combat.toggle(&mut turn);
        }
        self.forward_cast_outcomes();
        if running {
            // Starting decides where the round begins: the nearest point of
            // the route the character could walk to. Only if the start took.
            if self.combat.enabled() {
                let mut turn: RouteTurn<'_> = route_turn!(client, self);
                if let Some(line) = self.navigation.anchor_round_to_start(&mut turn) {
                    self.queue_log(MacroLogChannel::RuleInfo, &line);
                }
            }
            return;
        }
        if self.combat.enabled() {
            return;
        }
        self.after_macro_stopped(client);
    }

    /// The teardown of a stop (the second half of `SetMacroRunning`).
    pub(crate) fn after_macro_stopped(&mut self, client: &mut dyn TankClient) {
        if self.buff_rule.is_bursting() {
            self.clear_fast_cast_movement(client);
            self.buff_rule.stop_burst_only();
            self.restore_selection(client);
            "Stopped.".clone_into(&mut self.status);
        }
        self.gate.borrow_mut().reset();
        self.summon_pet.reset();
        self.normal_pet_refill.reset();
        self.idle_pet_refill.reset();
        self.vital_recharge.reset();
        self.vital_helper_recharge.reset();
        self.dispel.reset();
        self.crafting.reset();
        {
            let lent = lent_loot!(self);
            lent.loot.reset(client.looter());
        }
        // Not Reset: stopping puts down what the route had in flight and
        // keeps where the round had got to.
        {
            let mut turn: RouteTurn<'_> = route_turn!(client, self);
            self.navigation.stop_for_macro_stop(&mut turn);
        }
        // The walk to a corpse holds nothing but the keys.
        self.corpse_approach.stop_for_lost_turn(client.rules());
    }

    /// `ClearMossTankActionLocks`.
    pub(crate) fn clear_action_locks(&mut self, client: &mut dyn TankClient) {
        self.clear_fast_cast_movement(client);
        self.buff_rule.clear_cast_attempt();
        self.buff_rule.clear_recast_lock();
        self.combat.clear_action_locks(client.rules());
        self.vital_recharge.reset();
        self.vital_helper_recharge.reset();
        self.dispel.reset();
        self.crafting.reset();
        self.item_mana.reset();
        {
            let lent = lent_loot!(self);
            lent.loot.reset(client.looter());
        }
        let mut turn: RouteTurn<'_> = route_turn!(client, self);
        self.navigation.clear_action_locks(&mut turn);
    }

    /// The buff rule's Force Buff (`ForceBuff`).
    pub(crate) fn force_buff(&mut self, client: &mut dyn TankClient) {
        let mut owner = buff_owner!(self);
        let mut turn = BuffTurn {
            automation: client.rules(),
            settings: &self.settings.buffs,
            combat: &self.settings.combat,
            vitals: &self.settings.vitals,
            host: &mut owner,
        };
        self.buff_rule.start_force(&mut turn);
    }

    /// `CancelForceBuff`.
    pub(crate) fn cancel_force_buff(&mut self, client: &mut dyn TankClient) {
        let mut owner = buff_owner!(self);
        self.buff_rule.cancel_force(client.rules(), &mut owner);
    }
}
