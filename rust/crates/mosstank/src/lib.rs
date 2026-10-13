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

    /// Pushes every window's values as the shell has them now.
    fn push_windows(&mut self, host: &mut Host<'_>) {
        let Some(windows) = self.windows.as_mut() else {
            return;
        };
        let names = windows.value_names();
        let Some(tank) = self.tank.as_mut() else {
            return;
        };
        let values: std::collections::HashMap<String, ac_plugin_abi::ui::UiValue> = {
            let mut client =
                HostAutomation::with_equipment_observations(host, &mut self.observations);
            names
                .into_iter()
                .filter_map(|name| {
                    let value = tank.ui_value(&mut client, &name)?;
                    Some((name, value))
                })
                .collect()
        };
        windows.push(host, &values);
    }

    /// The macro shell, once enabled, with the client view a call
    /// hands it.
    pub fn with_tank<R>(
        &mut self,
        host: &mut Host<'_>,
        f: impl FnOnce(&mut Tank, &mut HostAutomation<'_, '_>) -> R,
    ) -> Option<R> {
        let tank = self.tank.as_mut()?;
        let mut client = HostAutomation::with_equipment_observations(host, &mut self.observations);
        Some(f(tank, &mut client))
    }
}

/// The seed of the plugin's own generator, from the client's (register
/// MANA-1).
fn seed(host: &mut Host<'_>) -> i32 {
    let random = host.clock().random_seed;
    i32::try_from(random & 0x7FFF_FFFF).unwrap_or(0)
}

impl Plugin for MossTank {
    fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
        let seed = seed(host);
        {
            let mut client =
                HostAutomation::with_equipment_observations(host, &mut self.observations);
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
        self.push_windows(host);
        let available = host.character().is_some();
        host.info(if available {
            "MossTank enabled"
        } else {
            "MossTank enabled (no live session yet; the Buff button will report 'Not in world' until one is up)"
        });
        Ok(())
    }

    fn disable(&mut self, host: &mut Host<'_>) {
        if let Some(mut tank) = self.tank.take() {
            let mut client =
                HostAutomation::with_equipment_observations(host, &mut self.observations);
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

    fn tick(&mut self, host: &mut Host<'_>) {
        let Some(tank) = self.tank.as_mut() else {
            return;
        };
        {
            let mut client =
                HostAutomation::with_equipment_observations(host, &mut self.observations);
            tank.on_tick(&mut client, TICK_SECONDS);
            tank.tick_autostart(&mut client);
        }
        let reason = tank.walk_pause_reason();
        self.go_to_pause.need(host, reason.as_deref());
        if self
            .windows
            .as_mut()
            .is_some_and(|windows| windows.push_due(TICK_SECONDS))
        {
            self.push_windows(host);
        }
    }

    fn on_event(&mut self, host: &mut Host<'_>, event: Event) {
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
        {
            let mut client =
                HostAutomation::with_equipment_observations(host, &mut self.observations);
            match &event {
                Event::Command(call) if call.verb.eq_ignore_ascii_case(tank::VTANK_VERB) => {
                    tank.execute_vtank_command(&mut client, &call.arguments);
                }
                Event::Command(call) if call.verb.eq_ignore_ascii_case(tank::UB_VERB) => {
                    tank.execute_ub_command(&mut client, &call.arguments);
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
        // Whatever the event changed is shown at once.
        self.push_windows(host);
    }

    fn callback(&mut self, host: &mut Host<'_>, callback: Callback) -> CallbackAnswer {
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
