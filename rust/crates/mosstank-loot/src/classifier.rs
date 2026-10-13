//! MossTank's loot-rule engine as a loot classifier other plugins can ask
//! (`MossTankLootClassifier`): an item against MossTank's live rules, or
//! against a named stored `.utl` profile, without MossTank's panel.

use std::collections::HashMap;

use ac_plugin_abi::items::{LootAction as PublicLootAction, LootClassification, LootContext};
use mosstank_automation::Automation;
use mosstank_dotnet::collections::fold_ignore_case;
use mosstank_formats::utl::{self, LootRule, VtankLootProfile};
use mosstank_stores::storage::PluginStorage;
use openac_plugin_compat::text::{casing, ordinal};

use crate::rules;

/// The client's long-description string property. It is written only once
/// an item has been identified, so its absence says the item is still
/// unappraised.
const LONG_DESC_PROPERTY_ID: u32 = 16;

/// Requirement kinds that can only be evaluated with appraisal data:
/// spell-name and appraised-spell-count checks, and every buffed or
/// tinkered rating.
const APPRAISAL_DEPENDENT_REQUIREMENT_TYPES: [i32; 10] =
    [0, 8, 9, 10, 2000, 2001, 2003, 2005, 2006, 2007];

/// How long a resolved profile (found or not) is trusted before storage is
/// read again, in seconds: a stored profile rarely changes mid-session, but
/// one dropped in after the plugin started must still be found.
pub const RECHECK_INTERVAL_SECONDS: f64 = 5.0;

/// The id MossTank registers its classifier under, and its name.
pub const CLASSIFIER_ID: &str = "moss-tank";

/// The classifier's name for people.
pub const CLASSIFIER_DISPLAY_NAME: &str = "MossTank";

#[derive(Debug, Clone)]
struct ProfileCacheEntry {
    profile: Option<VtankLootProfile>,
    source_text: Option<String>,
    checked_at: f64,
}

/// The classifier, with its cache of stored profiles.
#[derive(Debug, Default)]
pub struct MossTankLootClassifier {
    profile_cache: HashMap<String, ProfileCacheEntry>,
}

impl MossTankLootClassifier {
    /// A classifier with an empty profile cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The item against the live rules; `matched` is false when no rule
    /// chose it.
    pub fn classify(
        &self,
        live_rules: &[LootRule],
        context: &LootContext,
        host: Option<&mut (dyn Automation + '_)>,
    ) -> LootClassification {
        decide_against(live_rules, context, host)
    }

    /// Whether the item must be appraised before the live rules can judge
    /// it: it has no long description yet and some rule reads a property
    /// that only an appraisal brings.
    #[must_use]
    pub fn needs_identification(&self, live_rules: &[LootRule], context: &LootContext) -> bool {
        if context
            .properties
            .strings
            .iter()
            .any(|(key, _)| *key == LONG_DESC_PROPERTY_ID)
        {
            return false;
        }
        live_rules.iter().any(|rule| {
            rule.vtank_requirements.iter().any(|requirement| {
                APPRAISAL_DEPENDENT_REQUIREMENT_TYPES.contains(&requirement.kind)
            })
        })
    }

    /// The rules of a stored profile (`name` or `name.utl` in the VTank
    /// profile folder), from the cache while it is fresh; `None` when there
    /// is no such profile or it does not read. `now` is the host's clock,
    /// seconds.
    pub fn load_profile_rules(
        &mut self,
        profiles: &mut dyn PluginStorage,
        profile_name: &str,
        now: f64,
    ) -> Option<Vec<LootRule>> {
        let mut normalized = profile_name.trim_matches(casing::is_white_space);
        if ordinal::ends_with_ignore_case(normalized, ".utl") {
            normalized = &normalized[..normalized.len() - 4];
        }
        if normalized.is_empty() {
            return None;
        }
        let key = fold_ignore_case(normalized);

        if let Some(cached) = self.profile_cache.get(&key)
            && now - cached.checked_at < RECHECK_INTERVAL_SECONDS
        {
            return cached.profile.as_ref().map(|profile| profile.rules.clone());
        }

        // A storage that cannot read the file finds nothing there.
        let text = if profiles.is_available() {
            profiles
                .read_text(&format!("{normalized}.utl"))
                .ok()
                .flatten()
        } else {
            None
        };

        // The text has not changed since the last check: keep the parsed
        // result (found or not) and refresh the stamp.
        if let Some(cached) = self.profile_cache.get_mut(&key)
            && text == cached.source_text
        {
            cached.checked_at = now;
            return cached.profile.as_ref().map(|profile| profile.rules.clone());
        }

        let parsed = text
            .as_deref()
            .and_then(|source| utl::try_read(Some(source)).ok());
        let rules = parsed.as_ref().map(|profile| profile.rules.clone());
        self.profile_cache.insert(
            key,
            ProfileCacheEntry {
                profile: parsed,
                source_text: text,
                checked_at: now,
            },
        );
        rules
    }

    /// The item against a stored profile; `None` when the profile is not
    /// there.
    pub fn try_classify_with_profile(
        &mut self,
        profiles: &mut dyn PluginStorage,
        profile_name: &str,
        now: f64,
        context: &LootContext,
        host: Option<&mut (dyn Automation + '_)>,
    ) -> Option<LootClassification> {
        let rules = self.load_profile_rules(profiles, profile_name, now)?;
        Some(decide_against(&rules, context, host))
    }
}

fn decide_against(
    rules: &[LootRule],
    context: &LootContext,
    host: Option<&mut (dyn Automation + '_)>,
) -> LootClassification {
    let (decision, _) = rules::decide(
        &context.item,
        &context.properties,
        rules,
        &context.owned_items,
        None,
        host,
    );
    let Some(found) = decision else {
        return LootClassification::default();
    };
    // Both of MossTank's mana-transfer actions map one to one onto the
    // public enum; an action without a public member reads as no loot.
    let action = u8::try_from(found.action as i32)
        .ok()
        .and_then(PublicLootAction::from_tag)
        .unwrap_or(PublicLootAction::NoLoot);
    let keep_count = usize::try_from(found.rule_index)
        .ok()
        .and_then(|index| rules.get(index))
        .map_or(0, |rule| rule.keep_count);
    LootClassification {
        matched: true,
        action,
        rule_name: found.rule_name,
        priority: found.priority,
        keep_count,
    }
}
