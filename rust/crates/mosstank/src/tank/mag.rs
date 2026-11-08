//! The Mag tab and the Inventory window in the shell: the client's events
//! and the frame reach the Mag features here, and the loaded loot profile
//! answers the item-info line through [`ShellVerdict`].

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::Automation;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_loot::rules;
use mosstank_mag::host::{LootVerdict, Verdict};

use super::rules::TankCx;

/// The loot profile MossTank loots by, as the item-info line asks it: the
/// first matching rule decides, as the looter's own decision does.
// The identify chat line builds one per line it prints.
#[cfg_attr(not(test), allow(dead_code))]
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

impl TankCx<'_> {
    /// A client event, for the Mag features.
    pub(crate) fn mag_event(&mut self, event: &ac_plugin_abi::Event) {
        let _ = event;
    }

    /// One frame of the Mag features, with the character in the world.
    pub(crate) fn mag_tick(&mut self) {}

    /// The character left the world: the appraisals it waited on are
    /// forgotten.
    pub(crate) fn mag_session_ended(&mut self) {
        self.mag.identify.clear();
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
