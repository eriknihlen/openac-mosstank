//! Whether a tier can be cast now, read from the client (`BuffCastability`):
//! not blacklisted, its scarabs in the pack, its components carried. What a
//! walk reports goes to a [`CastabilityReport`].

use std::collections::HashMap;
use std::rc::Rc;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use mosstank_automation::Automation;
use mosstank_automation::components::uses_blacklisted_component;
use mosstank_dotnet::collections::fold_ignore_case;

use crate::plan::{BuffCastability, BuffTierRejection};
use crate::profile::BuffLine;

/// Where a walk's warnings and picks go.
pub trait CastabilityReport {
    /// A warning (a missing scarab, a line with nothing castable).
    fn warn(&mut self, text: &str);

    /// The tier a line resolved to, with every tier passed over.
    fn note_tier_pick(
        &mut self,
        line: &BuffLine,
        pick: Option<&SpellInfo>,
        rejections: &[BuffTierRejection],
    );
}

/// The castability of tiers for one walk, over the client.
#[allow(missing_debug_implementations)]
pub struct AutomationCastability<'a> {
    automation: &'a mut dyn Automation,
    owned: Vec<InventoryItem>,
    blacklisted_components: &'a str,
    report: &'a mut dyn CastabilityReport,
    counts: HashMap<String, i32>,
    host_components: HashMap<u32, bool>,
    /// The self buffs the character knows, once read (or handed over by
    /// a caller that read them for the same call).
    known: Option<Rc<[SpellInfo]>>,
}

impl<'a> AutomationCastability<'a> {
    /// A walk over `owned` (the items the scarabs are counted in).
    pub fn new(
        automation: &'a mut dyn Automation,
        owned: Vec<InventoryItem>,
        blacklisted_components: &'a str,
        report: &'a mut dyn CastabilityReport,
    ) -> Self {
        Self {
            automation,
            owned,
            blacklisted_components,
            report,
            counts: HashMap::new(),
            host_components: HashMap::new(),
            known: None,
        }
    }

    /// The self buffs, if this walk has read them.
    #[must_use]
    pub fn known_if_read(&self) -> Option<Rc<[SpellInfo]>> {
        self.known.clone()
    }

    /// The same, with the self buffs the caller read for this call.
    #[must_use]
    pub fn with_known(mut self, known: Rc<[SpellInfo]>) -> Self {
        self.known = Some(known);
        self
    }

    fn has_host_components(&mut self, spell_id: u32) -> bool {
        if let Some(&cached) = self.host_components.get(&spell_id) {
            return cached;
        }
        let answer = self.automation.has_components(spell_id);
        self.host_components.insert(spell_id, answer);
        answer
    }

    /// The scarabs the formula needs, each in the pack as often as it is
    /// named; the reason names the first one short.
    fn has_scarabs_in_inventory(&mut self, spell: &SpellInfo) -> Result<(), String> {
        if spell.formula_component_ids.is_empty() {
            return Ok(());
        }
        // In the order each scarab is first named, as the C#'s dictionary
        // enumerates.
        let mut needed: Vec<(String, i32)> = Vec::new();
        for &component_id in &spell.formula_component_ids {
            let Some(component) = self.automation.spells().try_get_component(component_id) else {
                continue;
            };
            if component.name.is_empty() || !component.name.contains("Scarab") {
                continue;
            }
            match needed.iter_mut().find(|(name, _)| *name == component.name) {
                Some((_, count)) => *count += 1,
                None => needed.push((component.name, 1)),
            }
        }
        for (name, count) in needed {
            if self.count_owned(&name) >= count {
                continue;
            }
            self.report.warn(&format!(
                "Out of component \"{name}\" \u{2014} spells that need it are off."
            ));
            return Err(format!("no {name}"));
        }
        Ok(())
    }

    /// Every owned item of that name, summing stack counts; an item with no
    /// stack count of its own counts as one.
    fn count_owned(&mut self, name: &str) -> i32 {
        let key = fold_ignore_case(name);
        if let Some(&cached) = self.counts.get(&key) {
            return cached;
        }
        let total = self
            .owned
            .iter()
            .filter(|item| item.name == name)
            .map(|item| {
                if item.stack_size > 0 {
                    item.stack_size
                } else {
                    1
                }
            })
            .fold(0i32, i32::wrapping_add);
        self.counts.insert(key, total);
        total
    }
}

impl BuffCastability for AutomationCastability<'_> {
    fn spell_catalog(&mut self) -> Option<&mut dyn mosstank_automation::SpellCatalog> {
        Some(self.automation.spells())
    }

    fn is_castable(&mut self, tier: &SpellInfo) -> (bool, Option<String>) {
        if uses_blacklisted_component(self.automation.spells(), tier, self.blacklisted_components) {
            return (false, Some("blacklisted".to_owned()));
        }
        if let Err(reason) = self.has_scarabs_in_inventory(tier) {
            return (false, Some(reason));
        }
        if !self.has_host_components(tier.spell_id) {
            return (false, Some("no components".to_owned()));
        }
        (true, None)
    }

    fn note_no_castable_tier(&mut self, line: &BuffLine) {
        let Some(first) = line.tiers.first() else {
            return;
        };
        self.report.warn(&format!(
            "Buff skipped: no known spell in the class of {}.",
            first.name
        ));
    }

    fn note_tier_pick(
        &mut self,
        line: &BuffLine,
        pick: Option<&SpellInfo>,
        rejections: &[BuffTierRejection],
    ) {
        self.report.note_tier_pick(line, pick, rejections);
    }
}

impl AutomationCastability<'_> {
    /// One spell of the table, through the walk's client.
    pub fn automation_spell(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.automation.spells().try_get(spell_id)
    }

    /// The self buffs the character knows, through the walk's client (read
    /// once for the walk).
    pub fn known_self_buffs(&mut self) -> Rc<[SpellInfo]> {
        if let Some(known) = &self.known {
            return Rc::clone(known);
        }
        let known = self.automation.spells().known_self_buffs_shared();
        self.known = Some(Rc::clone(&known));
        known
    }

    /// The character's skills, through the walk's client.
    pub fn skills(&mut self) -> Vec<ac_plugin_abi::character::Skill> {
        self.automation.skills()
    }
}
