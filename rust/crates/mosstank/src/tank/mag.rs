//! The Mag tab and the Inventory window in the shell: the client's events
//! and the frame reach the Mag features here, and the loaded loot profile
//! answers the item-info line through [`ShellVerdict`].

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_loot::controller;
use mosstank_loot::host::LootHost;
use mosstank_mag::host::{LootVerdict, MagHost, Verdict};
use mosstank_mag::identify::IdentifyQueue;
use mosstank_mag::inventory_search::InfoSettings;
use mosstank_mag::printer::{IdentCall, PrintSettings, Printer};
use mosstank_mag::settings;
use mosstank_settings::model::LootSettings;
use mosstank_ub::catalog::UbCatalog;

use super::rules::TankCx;
use crate::client::TankClient;

/// The Inventory window's id.
pub(crate) const INVENTORY_WINDOW: &str = "inventory";

/// The loot profile MossTank loots by, as the item-info line asks it: the
/// looter's own decision ([`controller::profile_decision`]): the first
/// matching rule, or the external classifier the looter is set to.
pub(crate) struct ShellVerdict<'a> {
    /// The profile's rules.
    pub(crate) rules: &'a [LootRule],
    /// The external classifier the looter asks instead, blank for none.
    pub(crate) external_classifier_id: &'a str,
    /// A profile is active (`profile_active`); without one there is no
    /// verdict.
    pub(crate) profile_loaded: bool,
    /// What the character owns, for the keep-up-to counts.
    pub(crate) owned: &'a [InventoryItem],
    /// The client, for the requirements that read the character and the
    /// external classifier.
    pub(crate) host: &'a mut dyn LootHost,
}

impl LootVerdict for ShellVerdict<'_> {
    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict> {
        if !self.profile_loaded {
            return None;
        }
        let (decision, refusal) = controller::profile_decision(
            &mut *self.host,
            item,
            properties,
            self.rules,
            self.external_classifier_id,
            self.owned,
            None,
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
            && controller::profile_needs_identify(
                &mut *self.host,
                item,
                properties,
                self.rules,
                self.external_classifier_id,
            )
    }
}

/// The client and the loaded loot profile for one Mag call: the printer
/// reaches the client, and the profile through a [`ShellVerdict`] made per
/// question (both borrow the same client). The owned items, for the
/// keep-up-to counts and the identified item's record, are listed once a
/// call.
struct ShellIdentCall<'a> {
    client: &'a mut dyn TankClient,
    loot: &'a LootSettings,
    owned: Option<Vec<InventoryItem>>,
}

impl IdentCall for ShellIdentCall<'_> {
    fn host(&mut self) -> &mut dyn MagHost {
        self.client.mag()
    }

    fn owned(&mut self) -> &[InventoryItem] {
        let client = &mut *self.client;
        self.owned.get_or_insert_with(|| client.mag().owned_items())
    }

    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict> {
        if !self.loot.profile_active {
            return None;
        }
        let client = &mut *self.client;
        let owned = self.owned.get_or_insert_with(|| client.mag().owned_items());
        ShellVerdict {
            rules: &self.loot.rules,
            external_classifier_id: &self.loot.external_classifier_id,
            profile_loaded: true,
            owned,
            host: client.looter(),
        }
        .verdict(item, properties)
    }

    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool {
        ShellVerdict {
            rules: &self.loot.rules,
            external_classifier_id: &self.loot.external_classifier_id,
            profile_loaded: self.loot.profile_active,
            owned: &[],
            host: self.client.looter(),
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
            loot,
            owned: None,
        };
        let mag = &mut *self.mag;
        f(&mut mag.printer, &mut mag.identify, &mut call);
    }

    /// The settings the Inventory window's item texts are made with (the
    /// item info line's).
    pub(crate) fn inventory_info_settings(&self) -> InfoSettings {
        let settings = self.item_info_settings();
        InfoSettings {
            show_buffed_values: settings.show_buffed_values,
            show_value_and_burden: settings.show_value_and_burden,
        }
    }

    /// The Inventory window is shown (a dock button toggles it, which raises
    /// no event: the client is asked).
    pub(crate) fn inventory_shown(&mut self) -> bool {
        self.client.mag().ui_view_visible(INVENTORY_WINDOW)
    }

    /// The `Mag.InventoryManagement.InventoryLogger` setting.
    fn inventory_logger_enabled(&self) -> bool {
        UbCatalog::flag(&self.ub.catalog, settings::INVENTORY_LOGGER)
    }

    /// A client event, for the Mag features: the inventory logger's login,
    /// changes and logoff; the mana list's login, logoff, changes and
    /// system lines; the identify chat line's selections, appraisals and
    /// opened containers.
    pub(crate) fn mag_event(&mut self, event: &ac_plugin_abi::Event) {
        use ac_plugin_abi::Event;
        use ac_plugin_abi::world::ObjectChangeKind;
        match event {
            Event::LoginComplete => {
                self.mag.inventory_log.on_login_complete();
                let mag = &mut *self.mag;
                mag.mana.on_login(self.client.mag(), &mut mag.identify);
            }
            Event::Logoff => {
                let enabled = self.inventory_logger_enabled();
                self.mag.inventory_log.on_logoff(self.client.mag(), enabled);
                self.mag.mana.reset();
            }
            Event::ObjectChanged(change) => {
                let enabled = self.inventory_logger_enabled();
                let mag = &mut *self.mag;
                mag.inventory_search.on_object_changed(change);
                mag.inventory_log
                    .on_object_changed(change, &mut mag.identify, enabled);
                mag.mana
                    .on_object_changed(self.client.mag(), change, &mut mag.identify);
            }
            Event::ChatReceived(line) => {
                let mag = &mut *self.mag;
                mag.mana.on_chat(line, &mut mag.identify);
            }
            _ => {}
        }
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
    /// container scan, the inventory logger's tick's worth, the items put
    /// on for the mana list, then the identify queue, and the Inventory window's search (on a tick the
    /// logger left light). True when either did a tick's worth of work.
    pub(crate) fn mag_tick(&mut self) -> bool {
        if !self.mag.printer.is_idle() {
            let settings = self.item_info_settings();
            let now = self.client.mag().now_ms();
            self.with_printer(|printer, queue, call| printer.tick(call, queue, now, settings));
        }
        let enabled = self.inventory_logger_enabled();
        let mag = &mut *self.mag;
        let logged = mag
            .inventory_log
            .tick(self.client.mag(), &mut mag.identify, enabled);
        if mag.mana.has_placements() {
            mag.mana.tick(self.client.mag(), &mut mag.identify);
        }
        if !self.mag.identify.is_empty() {
            let looter_busy = self.core.loot.is_identifying();
            self.mag.identify.tick(self.client.mag(), looter_busy);
        }
        if logged {
            return true;
        }
        let shown = self.inventory_shown();
        let settings = self.inventory_info_settings();
        self.mag
            .inventory_search
            .tick(self.client.mag(), shown, settings)
    }

    /// The character left the world: the appraisals it waited on, the
    /// identify line's state, the inventory logger's, the mana list's and
    /// the Inventory window's rows are forgotten.
    pub(crate) fn mag_session_ended(&mut self) {
        self.mag.identify.clear();
        self.mag.printer.reset();
        self.mag.inventory_log.reset();
        self.mag.inventory_search.reset();
        self.mag.mana.reset();
    }

    /// The Mana page's values, built at this moment (once a moment).
    pub(crate) fn mana_view(&mut self) -> &mosstank_mag::mana::ManaView {
        self.mag.mana.refresh(self.client.mag());
        self.mag.mana.view()
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::items::{LootAction as PublicLootAction, LootClassification};
    use openac_plugin::Host;
    use openac_plugin::testing::FakeHost;

    use super::*;
    use crate::automation::HostAutomation;

    fn rule(name: &str, action: LootAction) -> LootRule {
        let mut rule = LootRule::named(name, "*");
        rule.action = action;
        rule
    }

    fn ring() -> InventoryItem {
        InventoryItem {
            object_id: 7,
            name: "Ring".to_owned(),
            ..InventoryItem::default()
        }
    }

    /// The verdict with the rules, or the external classifier `classifier`
    /// answering `answer`.
    fn verdict_by(
        rules: &[LootRule],
        profile_loaded: bool,
        classifier: &str,
        answer: Option<LootClassification>,
    ) -> (Option<Verdict>, bool) {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::LootClassify>(&answer);
        let mut host = Host::with_transport(&mut fake);
        let mut client = HostAutomation::new(&mut host);
        let mut shell = ShellVerdict {
            rules,
            external_classifier_id: classifier,
            profile_loaded,
            owned: &[],
            host: &mut client,
        };
        let properties = ItemProperties::default();
        let verdict = shell.verdict(&ring(), &properties);
        let needs = shell.needs_identify(&ring(), &properties);
        (verdict, needs)
    }

    fn verdict(rules: &[LootRule], profile_loaded: bool) -> Option<Verdict> {
        verdict_by(rules, profile_loaded, "", None).0
    }

    #[test]
    fn an_external_classifier_decides_as_the_looter_asks_it() {
        // The looter's own path (`decide_item` with an external classifier
        // named): its classification, its rule name.
        let answer = LootClassification {
            matched: true,
            action: PublicLootAction::Salvage,
            rule_name: "Their Salvage".to_owned(),
            priority: 1,
            keep_count: 0,
        };
        let (verdict, needs) = verdict_by(
            &[rule("Junk", LootAction::NoLoot)],
            true,
            "other.plugin",
            Some(answer),
        );
        assert_eq!(
            verdict,
            Some(Verdict {
                keep: true,
                rule_name: "Their Salvage".to_owned(),
                salvage: true,
            })
        );
        // The looter always appraises for an external classifier.
        assert!(needs);
    }

    #[test]
    fn an_external_classifier_that_matches_nothing_names_no_rule() {
        let (verdict, _) = verdict_by(
            &[rule("Rings", LootAction::Keep)],
            true,
            "other.plugin",
            None,
        );
        assert_eq!(
            verdict,
            Some(Verdict {
                keep: false,
                rule_name: String::new(),
                salvage: false,
            })
        );
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
