//! The Mag tab and the Inventory window in the shell: the client's events
//! and the frame reach the Mag features here, and the loaded loot profile
//! answers the item-info line through [`ShellVerdict`].

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::Automation;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_loot::rules;
use mosstank_mag::host::{LootVerdict, MagHost, Verdict};
use mosstank_mag::identify::IdentifyQueue;
use mosstank_mag::printer::{IdentCall, PrintSettings, Printer};
use mosstank_mag::settings;
use mosstank_ub::catalog::UbCatalog;

use super::rules::TankCx;
use crate::client::TankClient;

/// The loot profile MossTank loots by, as the item-info line asks it: the
/// first matching rule decides, as the looter's own decision does.
pub(crate) struct ShellVerdict<'a> {
    /// The profile's rules.
    pub(crate) rules: &'a [LootRule],
    /// A profile is active (`profile_active`); without one there is no
    /// verdict.
    pub(crate) profile_loaded: bool,
    /// What the character owns, for the keep-up-to counts.
    pub(crate) owned: &'a [InventoryItem],
    /// The client, for the requirements that read the character.
    pub(crate) host: &'a mut dyn Automation,
}

impl LootVerdict for ShellVerdict<'_> {
    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict> {
        if !self.profile_loaded {
            return None;
        }
        let (decision, refusal) = rules::decide(
            item,
            properties,
            self.rules,
            self.owned,
            None,
            Some(&mut *self.host),
        );
        Some(match decision {
            Some(decision) => Verdict {
                keep: true,
                salvage: decision.action == LootAction::Salvage,
                rule_name: decision.rule_name,
            },
            // A declining rule, or one at its keep-up-to limit, still names
            // itself; no rule names nothing.
            None => Verdict {
                keep: false,
                salvage: false,
                rule_name: refusal.rule_name,
            },
        })
    }

    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool {
        self.profile_loaded
            && rules::needs_identify(item, properties, self.rules, Some(&mut *self.host))
    }
}

/// The client and the loaded loot profile for one Mag call: the printer
/// reaches the client, and the profile through a [`ShellVerdict`] made per
/// question (both borrow the same client). The owned items, for the
/// keep-up-to counts, are listed once a call, at the first verdict.
struct ShellIdentCall<'a> {
    client: &'a mut dyn TankClient,
    rules: &'a [LootRule],
    profile_loaded: bool,
    owned: Option<Vec<InventoryItem>>,
}

impl IdentCall for ShellIdentCall<'_> {
    fn host(&mut self) -> &mut dyn MagHost {
        self.client.mag()
    }

    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict> {
        if !self.profile_loaded {
            return None;
        }
        let client = &mut *self.client;
        let owned = self.owned.get_or_insert_with(|| client.mag().owned_items());
        ShellVerdict {
            rules: self.rules,
            profile_loaded: true,
            owned,
            host: client.rules(),
        }
        .verdict(item, properties)
    }

    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool {
        ShellVerdict {
            rules: self.rules,
            profile_loaded: self.profile_loaded,
            owned: &[],
            host: self.client.rules(),
        }
        .needs_identify(item, properties)
    }
}

impl TankCx<'_> {
    /// The `Mag.ItemInfoOnIdent` settings, from the UB settings catalogue.
    fn item_info_settings(&self) -> PrintSettings {
        let catalog = &self.ub.catalog;
        PrintSettings {
            enabled: UbCatalog::flag(catalog, settings::ITEM_INFO_ENABLED),
            show_buffed_values: UbCatalog::flag(catalog, settings::ITEM_INFO_SHOW_BUFFED),
            show_value_and_burden: UbCatalog::flag(catalog, settings::ITEM_INFO_VALUE_BURDEN),
        }
    }

    /// Runs `f` with the printer, the identify queue and the client with
    /// the loaded loot profile.
    fn with_printer(
        &mut self,
        f: impl FnOnce(&mut Printer, &mut IdentifyQueue, &mut dyn IdentCall),
    ) {
        let loot = &self.core.settings.inventory.loot;
        let mut call = ShellIdentCall {
            client: &mut *self.client,
            rules: &loot.rules,
            profile_loaded: loot.profile_active,
            owned: None,
        };
        let mag = &mut *self.mag;
        f(&mut mag.printer, &mut mag.identify, &mut call);
    }

    /// A client event, for the Mag features: the identify chat line's
    /// selections, appraisals and opened containers.
    pub(crate) fn mag_event(&mut self, event: &ac_plugin_abi::Event) {
        use ac_plugin_abi::Event;
        use ac_plugin_abi::world::ObjectChangeKind;
        match event {
            Event::SelectionChanged(change) if change.selected.is_some_and(|id| id != 0) => {
                let enabled = self.item_info_settings().enabled;
                let now = self.client.mag().now_ms();
                self.mag.printer.on_selection(change.selected, now, enabled);
            }
            Event::ObjectChanged(change) if change.kind == ObjectChangeKind::IdentReceived => {
                let Some(object) = &change.current else {
                    return;
                };
                let settings = self.item_info_settings();
                let now = self.client.mag().now_ms();
                self.with_printer(|printer, queue, call| {
                    printer.on_ident(call, queue, object, now, settings);
                });
            }
            Event::ContainerOpened(container) => {
                let enabled = self.item_info_settings().enabled;
                if !enabled {
                    return;
                }
                let host = self.client.mag();
                let now = host.now_ms();
                if let Some(opened) = host.object(*container) {
                    self.mag
                        .printer
                        .on_container_opened(&opened.name, now, enabled);
                }
            }
            _ => {}
        }
    }

    /// One frame of the Mag features, with the character in the world: the
    /// container scan, then the identify queue.
    pub(crate) fn mag_tick(&mut self) {
        if !self.mag.printer.is_idle() {
            let settings = self.item_info_settings();
            let now = self.client.mag().now_ms();
            self.with_printer(|printer, queue, call| printer.tick(call, queue, now, settings));
        }
        if !self.mag.identify.is_empty() {
            let looter_busy = self.core.loot.is_identifying();
            self.mag.identify.tick(self.client.mag(), looter_busy);
        }
    }

    /// The character left the world: the appraisals it waited on and the
    /// identify line's state are forgotten.
    pub(crate) fn mag_session_ended(&mut self) {
        self.mag.identify.clear();
        self.mag.printer.reset();
    }
}

#[cfg(test)]
mod tests {
    use mosstank_ub::testing::FakeUbHost;

    use super::*;

    fn rule(name: &str, action: LootAction) -> LootRule {
        let mut rule = LootRule::named(name, "*");
        rule.action = action;
        rule
    }

    fn verdict(rules: &[LootRule], profile_loaded: bool) -> Option<Verdict> {
        let mut host = FakeUbHost::new();
        let mut shell = ShellVerdict {
            rules,
            profile_loaded,
            owned: &[],
            host: &mut host,
        };
        let item = InventoryItem {
            object_id: 7,
            name: "Ring".to_owned(),
            ..InventoryItem::default()
        };
        shell.verdict(&item, &ItemProperties::default())
    }

    #[test]
    fn a_keeping_rule_keeps_and_names_itself() {
        assert_eq!(
            verdict(&[rule("Rings", LootAction::Keep)], true),
            Some(Verdict {
                keep: true,
                rule_name: "Rings".to_owned(),
                salvage: false,
            })
        );
    }

    #[test]
    fn a_salvage_rule_keeps_to_salvage() {
        let answer = verdict(&[rule("Salvage", LootAction::Salvage)], true).unwrap();
        assert!(answer.keep && answer.salvage);
    }

    #[test]
    fn a_declining_rule_keeps_nothing_but_still_names_itself() {
        assert_eq!(
            verdict(&[rule("Junk", LootAction::NoLoot)], true),
            Some(Verdict {
                keep: false,
                rule_name: "Junk".to_owned(),
                salvage: false,
            })
        );
    }

    #[test]
    fn no_matching_rule_keeps_nothing_with_no_name() {
        assert_eq!(
            verdict(&[], true),
            Some(Verdict {
                keep: false,
                rule_name: String::new(),
                salvage: false,
            })
        );
    }

    #[test]
    fn without_a_profile_there_is_no_verdict() {
        assert_eq!(verdict(&[rule("Rings", LootAction::Keep)], false), None);
    }
}
