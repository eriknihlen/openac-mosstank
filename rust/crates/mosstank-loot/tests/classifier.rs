//! MossTank's loot classifier (`MossTankLootClassifierTests`).

use std::collections::HashMap;

use ac_plugin_abi::items::{InventoryItem, LootAction as PublicLootAction, LootContext};
use ac_plugin_abi::world::ItemProperties;
use mosstank_formats::fixtures;
use mosstank_formats::utl::{LootAction, LootRule, VtankLootRequirement};
use mosstank_loot::classifier::MossTankLootClassifier;
use mosstank_stores::storage::{PluginStorage, StorageError};

const LONG_DESC_PROPERTY_ID: u32 = 16;

/// The VTank profile folder, counting its reads (`Host.VtankProfiles`).
#[derive(Debug, Default)]
struct Profiles {
    files: HashMap<String, String>,
    read_count: usize,
}

impl Profiles {
    fn with(name: &str, text: String) -> Self {
        Self {
            files: HashMap::from([(name.to_owned(), text)]),
            read_count: 0,
        }
    }
}

impl PluginStorage for Profiles {
    fn is_available(&mut self) -> bool {
        true
    }

    fn root_path(&mut self) -> Option<String> {
        None
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        self.read_count += 1;
        Ok(self.files.get(key).cloned())
    }

    fn list(&mut self, _prefix: &str) -> Result<Vec<String>, StorageError> {
        Ok(Vec::new())
    }

    fn write_text(&mut self, _key: &str, _content: &str) -> Result<(), StorageError> {
        Ok(())
    }

    fn ensure_directory(&mut self, _prefix: &str) -> Result<bool, StorageError> {
        Ok(true)
    }

    fn delete(&mut self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }
}

fn context(has_long_desc: bool) -> LootContext {
    LootContext {
        item: InventoryItem {
            name: "Test Item".to_owned(),
            stack_size: 1,
            ..InventoryItem::default()
        },
        properties: ItemProperties {
            strings: if has_long_desc {
                vec![(LONG_DESC_PROPERTY_ID, "A description.".to_owned())]
            } else {
                Vec::new()
            },
            ..ItemProperties::default()
        },
        owned_items: Vec::new(),
    }
}

fn keep_all(name: &str, action: LootAction) -> Vec<LootRule> {
    let mut rule = LootRule::named(name, "*");
    rule.action = action;
    vec![rule]
}

fn appraisal_dependent_rule() -> LootRule {
    let mut rule = LootRule::named("Spell", "*");
    rule.vtank_requirements = vec![VtankLootRequirement {
        kind: 9,
        payload: String::new(),
    }];
    rule
}

fn loot_a() -> String {
    fixtures::read_vtank("loot-a.utl")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: ClassifyUsesTheLiveRulesDelegate
#[test]
fn classify_uses_the_live_rules_delegate() {
    let classifier = MossTankLootClassifier::new();
    let result = classifier.classify(&keep_all("All", LootAction::Keep), &context(false), None);
    assert!(result.matched);
    assert_eq!(result.action, PublicLootAction::Keep);
    assert_eq!(result.rule_name, "All");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: ClassifyMapsManaStoneOntoItsPublicEquivalent
#[test]
fn classify_maps_mana_stone_onto_its_public_equivalent() {
    let classifier = MossTankLootClassifier::new();
    let result = classifier.classify(
        &keep_all("Mana", LootAction::ManaStone),
        &context(false),
        None,
    );
    assert!(result.matched);
    assert_eq!(result.action, PublicLootAction::ManaStone);
    assert_eq!(result.rule_name, "Mana");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: ClassifyMapsManaTankOntoItsPublicEquivalent
#[test]
fn classify_maps_mana_tank_onto_its_public_equivalent() {
    let classifier = MossTankLootClassifier::new();
    let result = classifier.classify(
        &keep_all("Mana", LootAction::ManaTank),
        &context(false),
        None,
    );
    assert!(result.matched);
    assert_eq!(result.action, PublicLootAction::ManaTank);
    assert_eq!(result.rule_name, "Mana");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: NeedsIdentificationIsFalseWhenTheItemAlreadyHasALongDescription
#[test]
fn needs_identification_is_false_when_the_item_already_has_a_long_description() {
    let classifier = MossTankLootClassifier::new();
    assert!(!classifier.needs_identification(&[appraisal_dependent_rule()], &context(true)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: NeedsIdentificationIsTrueWhenUnidentifiedAndARuleReadsAnAppraisalDependentProperty
#[test]
fn needs_identification_is_true_when_unidentified_and_a_rule_reads_an_appraisal_dependent_property()
{
    let classifier = MossTankLootClassifier::new();
    assert!(classifier.needs_identification(&[appraisal_dependent_rule()], &context(false)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: NeedsIdentificationIsFalseWhenNoRuleReadsAnAppraisalDependentProperty
#[test]
fn needs_identification_is_false_when_no_rule_reads_an_appraisal_dependent_property() {
    let classifier = MossTankLootClassifier::new();
    let mut rule = LootRule::named("ObjectClass", "*");
    rule.vtank_requirements = vec![VtankLootRequirement {
        kind: 7,
        payload: "0".to_owned(),
    }];
    assert!(!classifier.needs_identification(&[rule], &context(false)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: TryClassifyWithProfileReturnsFalseWhenTheNamedProfileDoesNotExist
#[test]
fn try_classify_with_profile_returns_false_when_the_named_profile_does_not_exist() {
    let mut classifier = MossTankLootClassifier::new();
    let mut profiles = Profiles::default();
    let found = classifier.try_classify_with_profile(
        &mut profiles,
        "does-not-exist",
        0.0,
        &context(false),
        None,
    );
    assert!(found.is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: TryClassifyWithProfileLoadsTheNamedUtlFileFromVtankProfiles
#[test]
fn try_classify_with_profile_loads_the_named_utl_file_from_vtank_profiles() {
    let mut classifier = MossTankLootClassifier::new();
    let mut profiles = Profiles::with("loot-a.utl", loot_a());
    // loot-a.utl's one rule carries a retired Type=6 requirement, so it
    // never matches; the profile is still found.
    let classification = classifier
        .try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None)
        .expect("found");
    assert!(!classification.matched);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: TryClassifyWithProfileAcceptsTheNameWithOrWithoutTheUtlExtension
#[test]
fn try_classify_with_profile_accepts_the_name_with_or_without_the_utl_extension() {
    let mut classifier = MossTankLootClassifier::new();
    let mut profiles = Profiles::with("loot-a.utl", loot_a());
    assert!(
        classifier
            .try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None)
            .is_some()
    );
    assert!(
        classifier
            .try_classify_with_profile(&mut profiles, "loot-a.utl", 0.0, &context(false), None)
            .is_some()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: TryClassifyWithProfileCachesTheParsedProfileAcrossCalls
#[test]
fn try_classify_with_profile_caches_the_parsed_profile_across_calls() {
    let mut classifier = MossTankLootClassifier::new();
    let mut profiles = Profiles::with("loot-a.utl", loot_a());
    classifier.try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None);
    classifier.try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None);
    assert_eq!(profiles.read_count, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: TryClassifyWithProfileDoesNotReReadWithinTheRecheckInterval
#[test]
fn try_classify_with_profile_does_not_re_read_within_the_recheck_interval() {
    let mut classifier = MossTankLootClassifier::new();
    let mut profiles = Profiles::with("loot-a.utl", loot_a());
    classifier.try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None);
    classifier.try_classify_with_profile(&mut profiles, "loot-a", 4.0, &context(false), None);
    assert_eq!(profiles.read_count, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankLootClassifierTests.cs :: TryClassifyWithProfileFindsAProfileDroppedInAfterTheRecheckInterval
#[test]
fn try_classify_with_profile_finds_a_profile_dropped_in_after_the_recheck_interval() {
    let mut classifier = MossTankLootClassifier::new();
    let mut profiles = Profiles::default();
    assert!(
        classifier
            .try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None)
            .is_none()
    );

    profiles.files.insert("loot-a.utl".to_owned(), loot_a());

    // Still within the interval: the stale "not found" is reused.
    assert!(
        classifier
            .try_classify_with_profile(&mut profiles, "loot-a", 0.0, &context(false), None)
            .is_none()
    );
    assert_eq!(profiles.read_count, 1);

    assert!(
        classifier
            .try_classify_with_profile(&mut profiles, "loot-a", 5.0, &context(false), None)
            .is_some()
    );
    assert_eq!(profiles.read_count, 2);
}
