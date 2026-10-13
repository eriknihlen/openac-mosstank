//! What came of the fight: the chat lines (kills, misses, hits, a wand's
//! confirmation), the shared cast tracker's events, the server's receipts,
//! the selection nudge after a cast, the other clients' casts, and the
//! monsters that have become ghosts.

use std::collections::HashSet;

use ac_plugin_abi::combat::CombatSnapshot;
use ac_plugin_abi::magic::{CastCompletion, SpellInfo};
use ac_plugin_abi::social::SelectionAction;
use ac_plugin_abi::types::ChatLine;
use mosstank_automation::Automation;
use mosstank_automation::cast_tracker::{
    CastEvent, SpellCastOutcome, SpellCastOutcomeInfo, SpellCastTracker,
};
use mosstank_automation::combat_text::{self, log_text_type};
use mosstank_dotnet::text::ordinal;
use openac_plugin_compat::number::format_f64;

use super::magic::is_vtank_instant_cast;
use super::{CombatController, Ctx, PHYSICAL_RESULT_TEXT_TAIL_SECONDS, is_known_dead};
use crate::item_debuff::CombatDebuffSourceKind;
use crate::{MacroLogChannel, debuff, post};

/// The chat kind of local speech.
const LOCAL_SPEECH_CHAT_KIND: u32 = 0;

fn is_matching_cast_line(text: &str, spell_name: &str) -> bool {
    text.starts_with(&format!("You cast {spell_name} on "))
}

impl CombatController {
    pub(super) fn arm_physical_result_text(&mut self, target_object_id: u32, target_name: &str) {
        self.physical_result_armed = true;
        self.physical_result_target_id = target_object_id;
        target_name.clone_into(&mut self.physical_result_target_name);
        self.physical_result_incarnation = self.find_target(target_object_id).incarnation;
    }

    pub(super) fn disarm_physical_result_text(&mut self) {
        if !self.physical_result_armed {
            return;
        }
        self.physical_result_armed = false;
        self.physical_completed_at = self.now;
    }

    /// The physical attack's results, read from chat.
    fn observe_physical_result_text(&mut self, cx: &mut Ctx<'_>, message: &ChatLine) {
        if !self.physical_result_armed
            && self.now - self.physical_completed_at > PHYSICAL_RESULT_TEXT_TAIL_SECONDS
        {
            return;
        }
        if self.physical_result_target_id == 0 {
            return;
        }
        let current = self.find_target(self.physical_result_target_id);
        if current.object_id != 0 && current.incarnation != self.physical_result_incarnation {
            return;
        }

        let text = message.text.as_str();
        if message.log_text_type == log_text_type::DEFAULT
            && text.trim() == combat_text::MISSILE_HIT_ENVIRONMENT
        {
            let id = self.physical_result_target_id;
            let tripped = self.failures.record_miss(id, self.now, cx.settings);
            let name = self.physical_result_target_name.clone();
            self.announce_blacklist(cx, tripped, id, &name);
        } else if message.log_text_type == log_text_type::OWN_COMBAT
            && combat_text::is_damage_report(text)
        {
            self.failures.reset_attempts(self.physical_result_target_id);
        }

        if message.log_text_type != log_text_type::DEFAULT {
            return;
        }
        let Some(slain) = combat_text::is_killing_blow(text) else {
            return;
        };

        // The looting hold goes up on the killing blow itself.
        self.arm_post_kill_navigation_lock();

        let mut slain_object_id = self.physical_result_target_id;
        let mut slain_name = self.physical_result_target_name.clone();
        if !slain.is_empty() && self.wielding_cleaving_weapon(cx.automation) {
            // A cleave can fell a bystander: the sentence says who died, and
            // it has to say so unambiguously.
            let Some(cleaved) = self.find_live_target_named(&slain) else {
                return;
            };
            slain_object_id = cleaved;
            slain_name = slain;
        } else if !slain.is_empty() && slain != self.physical_result_target_name {
            // The sentence has to name our monster, letter for letter.
            return;
        }

        self.log_line(
            MacroLogChannel::CastInfo,
            &format!("AttackExecutor: Kill blow ({text})"),
        );
        self.failures.reset_attempts(slain_object_id);
        cx.automation.log_info(&format!(
            "Target death attribution: physical target=0x{slain_object_id:08X}, name={slain_name}, message={text}"
        ));
        if slain_object_id == self.physical_result_target_id {
            self.disarm_physical_result_text();
        }
        self.end_killed_target(cx.automation, slain_object_id);
    }

    /// The one live creature carrying the name, or none when none or
    /// several do.
    fn find_live_target_named(&self, name: &str) -> Option<u32> {
        let mut found = 0;
        for target in &self.targets {
            if target.object_id == 0 || target.name != name || is_known_dead(target) {
                continue;
            }
            if found != 0 {
                return None;
            }
            found = target.object_id;
        }
        (found != 0).then_some(found)
    }

    /// Whether the wielded weapon cleaves.
    fn wielding_cleaving_weapon(&mut self, automation: &mut dyn Automation) -> bool {
        if !automation.equipment_available() {
            return false;
        }
        let items = self.pass_equipment(automation);
        let (weapon, _) = super::equip::wielded_pair(&items);
        if weapon == 0 {
            return false;
        }
        items
            .iter()
            .find(|item| item.object_id == weapon)
            .is_some_and(|item| item.cleaving > 1)
    }

    pub(super) fn observe_item_debuff_receipts(&mut self, cx: &mut Ctx<'_>) {
        let messages = cx.automation.chat_messages(self.observed_chat_sequence);
        let character = cx.automation.character().id;
        for message in messages {
            self.observed_chat_sequence = self.observed_chat_sequence.max(message.sequence);
            self.observe_physical_result_text(cx, &message);
            let own_speech = message.kind == LOCAL_SPEECH_CHAT_KIND
                && message.sender_id != 0
                && message.sender_id == character;
            self.cast_tracker.borrow_mut().observe_chat(
                message.sequence,
                &message.text,
                own_speech,
                message.log_text_type,
            );
            self.drain_cast_events(cx);
        }
    }

    /// The in-flight item transaction's own watcher.
    pub(super) fn observe_item_transaction(&mut self, cx: &mut Ctx<'_>) {
        for message in cx
            .automation
            .chat_messages(self.item_transaction_chat_sequence)
        {
            self.item_transaction_chat_sequence =
                self.item_transaction_chat_sequence.max(message.sequence);
            let Some(pending) = self.pending_item_debuff.clone() else {
                continue;
            };
            if message.log_text_type != log_text_type::MAGIC
                || !is_matching_cast_line(&message.text, &pending.source.spell.name)
            {
                continue;
            }
            self.debuffs.record_applied(
                pending.target_object_id,
                pending.source.identity,
                &pending.source.spell,
                self.now,
            );
            self.announce_cast_success(
                cx.automation,
                pending.target_object_id,
                &pending.source.spell,
            );
            self.failures.reset_attempts(pending.target_object_id);
            self.set_status(format!(
                "{} applied to {}",
                pending.source.spell.name, pending.target_name
            ));
            if !cx.settings.jump_out_wand_casting {
                self.start_selection_jiggle(cx, &pending.source.spell);
            }
            self.clear_pending_item_debuff();
        }

        let completion = cx.automation.last_item_completion();
        if completion.revision <= self.observed_item_completion {
            return;
        }
        self.observed_item_completion = completion.revision;
        let Some(pending) = self.pending_item_debuff.as_ref() else {
            return;
        };
        let success = completion.revision != 0 && completion.weenie_error == 0;
        if pending.source.kind != CombatDebuffSourceKind::CasterItem
            || completion.source_id != pending.source.item_object_id
            || completion.target_id != pending.target_object_id
            || success
        {
            return;
        }
        let item_name = pending.item_name.clone();
        self.set_status(format!(
            "{item_name} failed (0x{:X})",
            completion.weenie_error
        ));
        self.clear_pending_item_debuff();
    }

    /// Handles everything the shared cast tracker queued: a re-issue, a
    /// cast at a silent monster, an answer, and an ended cast (kept for the
    /// other owners of casts too).
    pub fn drain_cast_events(&mut self, cx: &mut Ctx<'_>) {
        loop {
            let events = self.cast_tracker.borrow_mut().take_events();
            if events.is_empty() {
                return;
            }
            for event in events {
                match event {
                    CastEvent::Reissue {
                        spell_id,
                        target_object_id,
                    } => {
                        // A request the server never answered is sent again.
                        if target_object_id == 0 {
                            cx.automation.cast(spell_id, None);
                        } else {
                            cx.automation.cast(spell_id, Some(target_object_id));
                        }
                    }
                    CastEvent::Attempted(object_id) => {
                        if self.failures.record_spell_attempt(object_id, cx.settings) {
                            let allowed = cx.settings.delete_ghost_monsters;
                            self.delete_ghost_monster(cx.automation, object_id, allowed, "");
                        }
                    }
                    CastEvent::Answered(object_id) => self.failures.reset_spell_attempts(object_id),
                    CastEvent::Completed(info) => {
                        self.on_cast_tracker_outcome(cx, &info);
                        self.cast_outcomes.push(info);
                    }
                }
            }
        }
    }

    fn on_cast_tracker_outcome(&mut self, cx: &mut Ctx<'_>, info: &SpellCastOutcomeInfo) {
        let object_id = info.target_object_id;
        let current = self.find_target(object_id);
        if object_id != 0
            && current.object_id != 0
            && current.incarnation != info.target_incarnation
        {
            return;
        }
        match info.outcome {
            SpellCastOutcome::Kill => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    &format!("SpellCaster: Spell kill reset ({})", info.text),
                );
                self.arm_post_kill_navigation_lock();
                if object_id == 0 {
                    return;
                }
                if !info.hits_multiple_targets && object_id == self.health.target_object_id() {
                    self.health.clear(self.now);
                }
                self.failures.reset_attempts(object_id);
                // A spell that strikes several cannot say which one died.
                if !info.hits_multiple_targets {
                    cx.automation.log_info(&format!(
                        "Target death attribution: spell target=0x{object_id:08X}, message={}",
                        info.text
                    ));
                    self.end_killed_target(cx.automation, object_id);
                }
            }
            SpellCastOutcome::PermanentFail => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    &format!("SpellCaster: Spell permanent fail reset ({})", info.text),
                );
                if object_id != 0 {
                    self.failures
                        .force_blacklist(object_id, self.now, cx.settings);
                }
            }
            SpellCastOutcome::Fail => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    &format!("SpellCaster: Spell fail reset ({})", info.text),
                );
            }
            SpellCastOutcome::Success => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    &format!("SpellCaster: Spell success reset ({})", info.text),
                );
                if object_id == 0 {
                    return;
                }
                if !info.hits_multiple_targets
                    && let Some(points) = combat_text::try_read_spell_damage(Some(&info.text))
                {
                    self.health.record_damage(object_id, points);
                }
                self.failures.reset_attempts(object_id);
            }
            SpellCastOutcome::ResultTimeout => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    "SpellCaster: gave up waiting for the cast result",
                );
                if object_id != 0 && !info.hits_multiple_targets {
                    let tripped = self.failures.record_miss(object_id, self.now, cx.settings);
                    self.announce_blacklist(cx, tripped, object_id, &info.target_name);
                }
            }
            SpellCastOutcome::LaunchTimeout => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    "SpellCaster: gave up waiting for the cast to start",
                );
            }
            SpellCastOutcome::Rejected => {
                self.log_line(
                    MacroLogChannel::CastInfo,
                    &format!("SpellCaster: Cast refused (0x{:04X})", info.weenie_error),
                );
            }
            SpellCastOutcome::None => {}
        }
    }

    pub(super) fn end_killed_target(&mut self, automation: &mut dyn Automation, object_id: u32) {
        let observed = self.find_target(object_id);
        automation.log_info(&format!(
            "Target marked dead: 0x{object_id:08X}, name={}, healthKnown={}, health={}, healthRevision={}, incarnation={}",
            observed.name,
            if observed.is_health_known { "True" } else { "False" },
            openac_plugin_compat::number::to_string_f32(observed.health_fraction),
            observed.health_revision,
            observed.incarnation
        ));
        self.failures
            .report_death(object_id, self.now, observed.health_revision);
        if self.pending_attack_target == object_id {
            self.pending_attack_spell = 0;
            self.pending_attack_target = 0;
        }
        if self.target_id != object_id {
            return;
        }
        // Dropping the target cancels its swing.
        self.clear_target(automation);
        self.until_scan = 0.0;
        self.set_status("Waiting for a target");
    }

    // --- The selection nudge after a cast.

    pub(super) fn observe_selection_jiggle(
        &mut self,
        automation: &mut dyn Automation,
        do_jiggle: bool,
        completion: &CastCompletion,
    ) {
        if automation.is_casting() {
            self.stop_selection_jiggle();
            return;
        }
        if completion.revision <= self.observed_jiggle_cast_completion {
            return;
        }
        self.observed_jiggle_cast_completion = completion.revision;
        if completion.is_success()
            && let Some(spell) = automation.spells().try_get(completion.spell_id)
        {
            self.start_jiggle_for(automation, do_jiggle, &spell);
        }
    }

    fn start_selection_jiggle(&mut self, cx: &mut Ctx<'_>, spell: &SpellInfo) {
        self.start_jiggle_for(cx.automation, cx.settings.do_jiggle, spell);
    }

    fn start_jiggle_for(
        &mut self,
        automation: &mut dyn Automation,
        do_jiggle: bool,
        spell: &SpellInfo,
    ) {
        if !do_jiggle || (is_vtank_instant_cast(spell) && matches!(spell.school, 34 | 43)) {
            return;
        }
        if !automation.selection_step(SelectionAction::PreviousSelection) {
            return;
        }
        self.selection_jiggle_active = true;
        self.selection_jiggle_previous_player = false;
        self.next_selection_jiggle_at = self.now;
        self.selection_jiggle_until = self.now + SpellCastTracker::RESULT_TICK_SECONDS;
    }

    pub(super) fn tick_selection_jiggle(&mut self, automation: &mut dyn Automation) {
        if !self.selection_jiggle_active {
            return;
        }
        if self.now >= self.selection_jiggle_until {
            self.stop_selection_jiggle();
            return;
        }
        if self.now < self.next_selection_jiggle_at {
            return;
        }
        let mut pulses = 0;
        loop {
            let action = if self.selection_jiggle_previous_player {
                SelectionAction::PreviousPlayer
            } else {
                SelectionAction::NextPlayer
            };
            if !automation.selection_step(action) {
                self.stop_selection_jiggle();
                return;
            }
            self.selection_jiggle_previous_player = !self.selection_jiggle_previous_player;
            self.next_selection_jiggle_at += 0.131;
            pulses += 1;
            if !(self.now >= self.next_selection_jiggle_at && pulses < 8) {
                break;
            }
        }
    }

    pub(super) fn stop_selection_jiggle(&mut self) {
        self.selection_jiggle_active = false;
        self.selection_jiggle_previous_player = false;
        self.next_selection_jiggle_at = 0.0;
        self.selection_jiggle_until = 0.0;
    }

    // --- Receipts.

    pub(super) fn observe_attack_receipts(
        &mut self,
        combat: &CombatSnapshot,
        cast: &CastCompletion,
    ) {
        if combat.completion_revision > self.observed_physical_completion {
            self.observed_physical_completion = combat.completion_revision;
            // Result text is read for two seconds past this point.
            self.physical_completed_at = self.now;
            self.pending_physical_target = 0;
        }
        if cast.revision <= self.observed_attack_cast_completion {
            return;
        }
        self.observed_attack_cast_completion = cast.revision;
        if self.pending_attack_spell == cast.spell_id {
            self.pending_attack_spell = 0;
            self.pending_attack_target = 0;
        }
    }

    /// The reference's "Cannot hit" line when a monster is given up on.
    fn announce_blacklist(&mut self, cx: &mut Ctx<'_>, tripped: bool, object_id: u32, name: &str) {
        if !tripped {
            return;
        }
        let mut shown = if name.trim().is_empty() {
            self.find_target(object_id).name
        } else {
            name.to_owned()
        };
        if shown.trim().is_empty() {
            "???".clone_into(&mut shown);
        }
        #[allow(clippy::cast_possible_truncation)]
        let seconds = cx.settings.blacklist_monster_timeout_seconds as i32;
        post(
            cx.automation,
            &format!(
                "Cannot hit {shown} ({object_id}) \u{2014} blacklisted for {seconds} seconds."
            ),
        );
    }

    /// A monster engaged a while whose health never moved is a ghost.
    pub(super) fn check_stalled_health_ghost(&mut self, cx: &mut Ctx<'_>) {
        let name = self.health.target_name().to_owned();
        if !self.enabled
            || !cx.settings.delete_ghost_monsters_by_health_tracker
            || self.health.target_object_id() == 0
            || !cx.settings.monster_facts.is_listed(&name)
            || cx.settings.monster_facts.maximum_health(&name) <= 0
        {
            return;
        }
        let stale = cx.settings.ghost_delete_health_tracker_seconds.max(0.0);
        let since_change = self
            .health
            .last_health_change_at()
            .map_or(f64::INFINITY, |changed| self.now - changed);
        if self.now - self.health.acquired_at() < stale || since_change < stale {
            return;
        }
        let id = self.health.target_object_id();
        self.delete_ghost_monster(cx.automation, id, true, "due to HP tracker notification");
    }

    /// Asks the client to forget an object it still draws.
    fn delete_ghost_monster(
        &mut self,
        automation: &mut dyn Automation,
        object_id: u32,
        allowed: bool,
        reason: &str,
    ) {
        if object_id == 0 || !allowed {
            return;
        }
        let scanned = self.find_target(object_id);
        let name = if scanned.object_id == object_id && !scanned.name.is_empty() {
            scanned.name
        } else if self.health.target_object_id() == object_id {
            self.health.target_name().to_owned()
        } else {
            String::new()
        };
        if name.is_empty() {
            return;
        }
        let result = automation.dismiss_ghost(object_id);
        if !result.accepted() {
            return;
        }
        post(
            automation,
            &if reason.is_empty() {
                format!("Deleting ghost monster {name} ({object_id})")
            } else {
                format!("Deleting ghost monster {name} ({object_id}) {reason}.")
            },
        );
        self.cast_tracker.borrow_mut().reset_for_target(object_id);
        if self.target_id == object_id {
            automation.abort_attack();
            self.clear_target(automation);
        }
    }

    // --- The other clients' casts.

    /// Takes in what the other clients on this computer cast since the last
    /// look (on the host frame; nothing is issued).
    pub fn observe_remote_casts(&mut self, cx: &mut Ctx<'_>) {
        if !cx.automation.is_available() || !(self.cast_sharing_enabled)() {
            return;
        }
        let casts = cx.automation.peer_casts(self.remote_cast_cursor);
        if casts.is_empty() {
            return;
        }
        let me = cx.automation.character().id;
        let mut trusted: Option<Option<HashSet<u32>>> = None;
        let table_live = self.enabled && !self.combat_policy_suspended;
        for cast in casts {
            if cast.caster_id == me
                || !cast.landed
                || cast.seconds_remaining.is_nan()
                || cast.seconds_remaining <= 0.0
            {
                self.remote_cast_cursor = self.remote_cast_cursor.max(cast.sequence);
                continue;
            }
            // One trust snapshot, taken only when it can be used.
            if trusted.is_none() {
                trusted = Some(self.trusted_peer_clients(cx.automation));
            }
            self.remote_cast_cursor = self.remote_cast_cursor.max(cast.sequence);
            if trusted
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|clients| !clients.contains(&cast.client_id))
            {
                continue;
            }
            let Some(spell) = cx.automation.spells().try_get(cast.spell_id) else {
                continue;
            };
            cx.automation
                .report_cast(cast.target_id, cast.spell_id, cast.seconds_remaining);
            if !table_live {
                continue;
            }
            let Some((identity, _)) = debuff::classify(&spell) else {
                continue;
            };
            if !self.debuffs.record_remote(
                cast.target_id,
                identity,
                &spell,
                self.now,
                cast.seconds_remaining,
            ) {
                continue;
            }
            self.log_line(
                MacroLogChannel::SpellCast,
                &format!(
                    "Peer cast: {} on {} by 0x{:08X}, {} s left",
                    spell.name,
                    cast.target_id,
                    cast.caster_id,
                    format_f64(cast.seconds_remaining, "0").unwrap_or_default()
                ),
            );
        }
    }

    /// The peers whose casts are taken in (`None`: every peer).
    fn trusted_peer_clients(&mut self, automation: &mut dyn Automation) -> Option<HashSet<u32>> {
        let tag = (self.cast_sharing_peer_tag)();
        let tag = tag.as_deref().map(str::trim).unwrap_or_default();
        if tag.is_empty() {
            return None;
        }
        let mut trusted = HashSet::new();
        for client in automation.network_clients() {
            if client
                .tags
                .iter()
                .any(|candidate| ordinal::equals_ignore_case(candidate, tag))
            {
                trusted.insert(client.client_id);
            }
        }
        Some(trusted)
    }

    /// A held wand's cast watched to its end on every frame the attack's
    /// own turn did not drive it.
    pub fn observe_held_item_cast(&mut self, cx: &mut Ctx<'_>, elapsed_seconds: f64) {
        if self.held_item_driven_by_turn {
            self.held_item_driven_by_turn = false;
            return;
        }
        if !self
            .pending_item_debuff
            .as_ref()
            .is_some_and(|pending| pending.source.kind == CombatDebuffSourceKind::CasterItem)
        {
            return;
        }
        self.advance_clock_from_frame(elapsed_seconds);
        self.observe_item_transaction(cx);
        let combat = cx.automation.combat_snapshot();
        self.tick_pending_item_debuff(cx.automation, cx.settings, &combat);
    }
}
