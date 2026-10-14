//! MossTank as an openAC plugin.
//!
//! The conversion from the C# plugin (`../src/AcDream.Plugins.MossTank`)
//! goes slice by slice: the pure parts live in `mosstank-formats`,
//! `mosstank-expr`, `mosstank-data`, `mosstank-settings` and
//! `mosstank-stores`, the rules in their own crates; this crate is the
//! plugin shell that ties them to the client (`MossTankPlugin`): its
//! lifecycle, its two command words, the macro shell ([`tank`]) driven on
//! every tick, the hold on the walks the client plans, and MossTank's loot
//! classifier for the other plugins ([`loot`]).
//!
//! The windows (the main panel and its eleven companions, [`panel`]) show
//! the shell's values and send their handlers to it.

pub mod automation;
pub mod canvases;
pub mod client;
pub mod loot;
pub mod panel;
pub mod storage;
pub mod tank;
pub mod ub_host;

pub mod meta_host;
pub mod meta_windows;
pub mod nav_host;
pub mod vendor_host;

use ac_plugin_abi::callback::{Callback, CallbackAnswer};
use ac_plugin_abi::items::EquipmentObservation;
use ac_plugin_abi::ui::UiEvent;
use ac_plugin_abi::{Event, TICK_SECONDS};
use openac_plugin::{Host, Plugin, StartInfo};

use crate::automation::HostAutomation;
use crate::nav_host::GoToPause;
use crate::tank::Tank;

/// The plugin's version, shown to the player.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The plugin.
#[derive(Default)]
#[allow(missing_debug_implementations)]
pub struct MossTank {
    /// MossTank's loot classifier, as the client's other plugins ask it.
    loot_classifier: loot::LootClassifierService,
    /// The placements the client announced (`Event::EquipmentPlacement`),
    /// kept until a rule reads them: every client view of a call reads this
    /// one buffer (register COMBAT-2).
    observations: Vec<EquipmentObservation>,
    /// The macro shell, from enable to disable.
    tank: Option<Tank>,
    /// The hold on the walks the client plans.
    go_to_pause: GoToPause,
    /// The windows, once registered.
    windows: Option<panel::Windows>,
    /// The windows the meta draws, from the call that makes one to the
    /// call that takes it down.
    meta_windows: meta_windows::MetaWindows,
    /// What the client's views keep between calls (the spell table).
    kept: automation::KeptReads,
    /// An event the client told changed what the windows may show: the next
    /// tick pushes their values.
    windows_stale: bool,
    /// The last push due was left to this tick (a push waits a tick at most).
    windows_deferred: bool,
}

impl MossTank {
    /// The equipment placements kept for the rules and the classifier to
    /// read.
    #[must_use]
    pub fn observed_placements(&self) -> usize {
        self.observations.len()
    }

    /// The macro shell, once enabled.
    #[must_use]
    pub fn tank(&self) -> Option<&Tank> {
        self.tank.as_ref()
    }

    /// The windows the meta draws.
    #[must_use]
    pub fn meta_windows(&self) -> &meta_windows::MetaWindows {
        &self.meta_windows
    }

    /// Pushes every window's values as the shell has them now.
    fn push_windows(&mut self, host: &mut Host<'_>) {
        let Some(windows) = self.windows.as_mut() else {
            return;
        };
        let names = windows.value_names();
        let Some(tank) = self.tank.as_mut() else {
            return;
        };
        let values: std::collections::HashMap<&str, ac_plugin_abi::ui::UiValue> = {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            names
                .iter()
                .filter_map(|name| {
                    let value = tank.ui_value(&mut client, name)?;
                    Some((name.as_str(), value))
                })
                .collect()
        };
        windows.push(host, values);
        // The meta's windows read the views and the status list as they
        // stand.
        self.meta_windows.push(host, &mut |window_id, name| {
            tank.meta_window_value(window_id, name)
        });
    }

    /// The macro shell, once enabled, with the client view a call
    /// hands it.
    pub fn with_tank<R>(
        &mut self,
        host: &mut Host<'_>,
        f: impl FnOnce(&mut Tank, &mut HostAutomation<'_, '_>) -> R,
    ) -> Option<R> {
        let tank = self.tank.as_mut()?;
        // What the client said before this call is not kept into it, nor
        // what it says now past it.
        self.kept.end_call();
        let answer = {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            f(tank, &mut client)
        };
        self.kept.end_call();
        Some(answer)
    }
}

/// Whether the player did this (typed a verb, used a window, clicked a
/// link, picked a theme): its effect is shown at once.
fn is_player_action(event: &Event) -> bool {
    matches!(
        event,
        Event::Command(_) | Event::Ui(_) | Event::ChatLinkClicked(_) | Event::ThemeChanged(_)
    )
}

/// The seed of the plugin's own generator, from the client's (register
/// MANA-1).
fn seed(host: &mut Host<'_>) -> i32 {
    let random = host.clock().random_seed;
    i32::try_from(random & 0x7FFF_FFFF).unwrap_or(0)
}

impl Plugin for MossTank {
    // Each call begins and ends with nothing of the client kept
    // (`KeptReads::end_call`): the client changes between calls.
    fn enable(&mut self, host: &mut Host<'_>, info: &StartInfo) -> Result<(), String> {
        self.kept.end_call();
        let result = self.enable_call(host, info);
        self.kept.end_call();
        result
    }

    fn disable(&mut self, host: &mut Host<'_>) {
        self.kept.end_call();
        self.disable_call(host);
        self.kept.end_call();
    }

    fn tick(&mut self, host: &mut Host<'_>) {
        self.kept.end_call();
        self.tick_call(host);
        self.kept.end_call();
    }

    fn on_event(&mut self, host: &mut Host<'_>, event: Event) {
        self.kept.end_call();
        self.event_call(host, event);
        self.kept.end_call();
    }

    fn callback(&mut self, host: &mut Host<'_>, callback: Callback) -> CallbackAnswer {
        self.kept.end_call();
        let answer = self.callback_call(host, callback);
        self.kept.end_call();
        answer
    }
}

impl MossTank {
    fn enable_call(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
        let seed = seed(host);
        {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            self.tank = Some(Tank::new(&mut client, seed));
        }
        host.info("MossTank initialized");
        self.windows = Some(panel::Windows::register(host));
        // Two words, two command sets: the macro's commands answer on /vt,
        // the UtilityBelt ones on /ub, and neither answers the other's.
        host.register_command(tank::VTANK_VERB, "MossTank's macro commands")?;
        host.register_command(tank::UB_VERB, "MossTank's UtilityBelt commands")?;
        if let Err(error) = self.loot_classifier.register(host) {
            host.warn(&format!(
                "MossTank's loot classifier is not registered: {error}"
            ));
        }
        self.go_to_pause = GoToPause::take(host);
        // The host was used here, past the shell's view.
        self.kept.end_call();
        self.push_windows(host);
        let available = host.character().is_some();
        host.info(if available {
            "MossTank enabled"
        } else {
            "MossTank enabled (no live session yet; the Buff button will report 'Not in world' until one is up)"
        });
        Ok(())
    }

    fn disable_call(&mut self, host: &mut Host<'_>) {
        if let Some(mut tank) = self.tank.take() {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            tank.disable(&mut client);
        }
        // Both words are given up with the plugin.
        host.unregister_command(tank::VTANK_VERB);
        host.unregister_command(tank::UB_VERB);
        if let Some(windows) = self.windows.take() {
            windows.remove(host);
        }
        self.go_to_pause.release(host);
        self.loot_classifier.unregister(host);
        self.observations.clear();
        host.info("MossTank disabled");
    }

    fn tick_call(&mut self, host: &mut Host<'_>) {
        let Some(tank) = self.tank.as_mut() else {
            return;
        };
        let had_spell_table = self.kept.has_spell_table();
        let passes = tank.pass_count();
        {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            tank.on_tick(&mut client, TICK_SECONDS);
            tank.tick_autostart(&mut client);
        }
        // The chat-line patterns are built one a tick ahead of their first
        // use: built all at once, on the first cast's result line, they
        // cost more than one call may run. A tick that read the spell table
        // (the character's first in the world), or whose name tags looked at
        // a crowded world, has work enough: the next one builds.
        let read_spell_table = had_spell_table != self.kept.has_spell_table();
        let heavy = tank.frame_was_heavy() || tank.pass_count() != passes || read_spell_table;
        // A light tick builds ahead what a large spellbook makes costly for
        // the first pass that wants it (the self buffs' lines, the attack
        // catalog), one a tick.
        let warmed = !heavy && {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            tank.warm_caches(&mut client)
        };
        if !read_spell_table && !tank.frame_was_heavy() && !warmed {
            mosstank_automation::combat_text::warm_one();
        }
        let reason = tank.walk_pause_reason();
        if self.go_to_pause.need(host, reason.as_deref()) {
            // The walk's hold changed through the host, past the view.
            self.kept.end_call();
        }
        // The values follow on their own every tenth of a second, and on the
        // first tick after an event the client told; a tick that ran the
        // macro's pass or did a tick's worth of once-only work (the spell
        // table read, the name tags' first look at a crowded world) leaves
        // them to the next, once (register UI-1).
        let due = self
            .windows
            .as_mut()
            .is_some_and(|windows| windows.push_due(TICK_SECONDS));
        let stale = std::mem::take(&mut self.windows_stale);
        if due || stale {
            if heavy && !self.windows_deferred {
                self.windows_stale = true;
                self.windows_deferred = true;
            } else {
                self.windows_deferred = false;
                self.push_windows(host);
            }
        }
    }

    fn event_call(&mut self, host: &mut Host<'_>, event: Event) {
        if let Event::EquipmentPlacement(observation) = event {
            // Placements are observed from enable to disable only, as the
            // C# observers are attached.
            if let Some(tank) = self.tank.as_mut() {
                tank.on_equipment_placement(&observation);
                self.observations.push(observation);
            }
            return;
        }
        let Some(tank) = self.tank.as_mut() else {
            return;
        };
        // A click on one of the meta's windows: which window it is.
        let meta_window = match &event {
            Event::Ui(UiEvent::Action(action)) => {
                self.meta_windows.window_id(action.panel).map(str::to_owned)
            }
            _ => None,
        };
        {
            let mut client = HostAutomation::for_call(
                host,
                &mut self.observations,
                &mut self.meta_windows,
                &mut self.kept,
            );
            match &event {
                Event::Command(call) if call.verb.eq_ignore_ascii_case(tank::VTANK_VERB) => {
                    tank.execute_vtank_command(&mut client, &call.arguments);
                }
                Event::Command(call) if call.verb.eq_ignore_ascii_case(tank::UB_VERB) => {
                    tank.execute_ub_command(&mut client, &call.arguments);
                }
                Event::Ui(UiEvent::Action(action)) if meta_window.is_some() => {
                    let window_id = meta_window.as_deref().unwrap_or_default();
                    tank.meta_window_act(&mut client, window_id, &action.handler);
                }
                Event::Ui(UiEvent::Action(action))
                    if self
                        .windows
                        .as_ref()
                        .is_some_and(|windows| windows.owns(action.panel)) =>
                {
                    tank.ui_act(&mut client, &action.handler, action.value.clone());
                }
                _ => tank.on_event(&mut client, &event),
            }
        }
        // What the player did is shown at once; what the client told is shown
        // on the next tick, so a burst of the client's events (a login brings
        // thousands) works the values out once a tick, not once an event
        // (register UI-1).
        if is_player_action(&event) {
            self.push_windows(host);
        } else {
            self.windows_stale = true;
        }
    }

    fn callback_call(&mut self, host: &mut Host<'_>, callback: Callback) -> CallbackAnswer {
        if let Callback::ChatIntercept((handle, line)) = &callback
            && let Some(Some(decision)) = self.with_tank(host, |tank, client| {
                tank.intercept_chat(client, *handle, line)
            })
        {
            self.push_windows(host);
            return CallbackAnswer::Intercept(decision);
        }
        let rules = self
            .tank
            .as_ref()
            .map(|tank| tank.live_loot_rules().to_vec())
            .unwrap_or_default();
        self.loot_classifier
            .answer(host, &mut self.observations, &rules, &callback)
            .unwrap_or_default()
    }
}

openac_plugin::export_plugin!(MossTank);

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use openac_plugin::testing::FakeHost;

    use super::*;

    #[test]
    fn enabling_says_so_in_the_log() {
        let mut host = FakeHost::new("acdream.mosstank");
        let mut plugin = MossTank::default();
        host.enable(&mut plugin).unwrap();
        assert!(
            host.logs
                .iter()
                .any(|(_, line)| line == "MossTank initialized")
        );
        assert!(
            host.logs
                .iter()
                .any(|(_, line)| line.starts_with("MossTank enabled"))
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPluginTests.cs :: EnableRegistersTheLootClassifierAndDisableRevokesIt
    #[test]
    fn enable_registers_the_loot_classifier_and_disable_revokes_it() {
        let mut host = FakeHost::new("acdream.mosstank");
        host.set_reply::<calls::LootClassifierRegister>(&Ok(3));
        let mut plugin = MossTank::default();
        assert!(host.requests::<calls::LootClassifierRegister>().is_empty());

        host.enable(&mut plugin).unwrap();

        let registered = host.requests::<calls::LootClassifierRegister>();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].id, "moss-tank");
        assert_eq!(registered[0].display_name, "MossTank");
        assert!(
            host.requests::<calls::LootClassifierUnregister>()
                .is_empty()
        );

        host.disable(&mut plugin);

        assert_eq!(host.requests::<calls::LootClassifierUnregister>(), [3]);
    }
}
