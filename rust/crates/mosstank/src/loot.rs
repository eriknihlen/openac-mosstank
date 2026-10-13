//! The looter's view of the client ([`LootHost`]) over the SDK's host for
//! the length of one plugin call, and MossTank's loot classifier as the
//! client's other plugins ask it (registered as `moss-tank`, answered
//! through the plugin's callback export).

use ac_plugin_abi::callback::{Callback, CallbackAnswer, LootQuestion};
use ac_plugin_abi::items::{
    AppraisalState, InventoryCompletion, InventoryItem, ItemResult, LootClassification,
    LootContext, LootedItem,
};
use ac_plugin_abi::world::{ItemProperties, WorldObject};
use mosstank_formats::utl::LootRule;
use mosstank_loot::classifier::{CLASSIFIER_DISPLAY_NAME, CLASSIFIER_ID, MossTankLootClassifier};
use mosstank_loot::host::{LootCorpse, LootHost};
use mosstank_stores::storage::PluginStorage;
use openac_plugin::Host;

use crate::automation::HostAutomation;
use crate::storage::HostStorage;

/// The looter's view of the client: the one client of the call.
pub type HostLoot<'h, 'a> = HostAutomation<'h, 'a>;

impl LootHost for HostAutomation<'_, '_> {
    fn loot_available(&mut self) -> bool {
        self.host.loot_state().is_available
    }

    fn requested_container_id(&mut self) -> u32 {
        self.host.loot_state().requested_container_id
    }

    fn current_container_id(&mut self) -> u32 {
        self.host.loot_state().current_container_id
    }

    fn current_contents_ready(&mut self) -> bool {
        self.host.loot_state().current_contents_ready
    }

    fn appraisal(&mut self) -> AppraisalState {
        self.host.loot_state().appraisal
    }

    fn last_inventory_completion(&mut self) -> InventoryCompletion {
        self.host.loot_state().last_inventory_completion
    }

    fn loot_corpses(&mut self, maximum_distance: f32) -> Vec<LootCorpse> {
        self.host
            .corpses(maximum_distance)
            .into_iter()
            .map(LootCorpse::from)
            .collect()
    }

    fn loot_contents(&mut self) -> Vec<InventoryItem> {
        self.host.loot_contents()
    }

    fn loot_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.host.loot_properties(object_id)
    }

    fn loot_open(&mut self, container_id: u32) -> ItemResult {
        self.host.open_loot(container_id)
    }

    fn loot_close(&mut self, container_id: u32) -> ItemResult {
        self.host.close_loot(container_id)
    }

    fn loot_identify(&mut self, object_id: u32) -> ItemResult {
        self.host.identify_loot(object_id)
    }

    fn loot_pickup(&mut self, object_id: u32) -> ItemResult {
        self.host.pick_up(object_id, false)
    }

    fn salvage(&mut self, tool_id: u32, item_ids: &[u32]) -> ItemResult {
        self.host.salvage(tool_id, item_ids.to_vec())
    }

    fn sell(&mut self, object_id: u32) -> ItemResult {
        self.host.sell_item(object_id, 0)
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.host.item_state().active_vendor_id
    }

    fn is_spell_known(&mut self, spell_id: u32) -> bool {
        self.host.is_spell_known(spell_id)
    }

    fn objects(&mut self) -> Vec<WorldObject> {
        self.host.objects()
    }

    fn classify(
        &mut self,
        classifier_id: &str,
        context: LootContext,
    ) -> Option<LootClassification> {
        self.host.classify(classifier_id, "", context)
    }

    fn notify_looted(&mut self, classifier_id: &str, item: LootedItem) -> bool {
        self.host.notify_looted(classifier_id, item)
    }

    fn notify_item_removed(&mut self, classifier_id: &str, object_id: u32) -> bool {
        self.host.notify_item_removed(classifier_id, object_id)
    }

    fn storage_available(&mut self) -> bool {
        HostStorage::plugin_data(self.host).is_available()
    }

    fn storage_read_text(&mut self, key: &str) -> Result<Option<String>, String> {
        HostStorage::plugin_data(self.host).read_text(key)
    }

    fn storage_write_text(&mut self, key: &str, content: &str) -> Result<(), String> {
        HostStorage::plugin_data(self.host).write_text(key, content)
    }

    fn log_error(&mut self, text: &str) {
        self.host.error(text);
    }
}

/// MossTank's loot classifier as the client's other plugins see it: its
/// registration, and its answers to the client's questions.
#[derive(Debug, Default)]
pub struct LootClassifierService {
    handle: Option<u32>,
    classifier: MossTankLootClassifier,
}

impl LootClassifierService {
    /// A classifier not yet registered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the classifier with the client (`moss-tank`, scoped by the
    /// client as `<plugin id>/moss-tank`).
    ///
    /// # Errors
    /// The client refused the registration.
    pub fn register(&mut self, host: &mut Host<'_>) -> Result<(), String> {
        let handle = host.register_loot_classifier(CLASSIFIER_ID, CLASSIFIER_DISPLAY_NAME)?;
        self.handle = Some(handle);
        Ok(())
    }

    /// Removes the registration.
    pub fn unregister(&mut self, host: &mut Host<'_>) {
        if let Some(handle) = self.handle.take() {
            host.unregister_loot_classifier(handle);
        }
    }

    /// The handle the client gave the classifier.
    #[must_use]
    pub fn handle(&self) -> Option<u32> {
        self.handle
    }

    /// Answers a question the client asks the classifier, against the live
    /// rules (or the stored profile the question names); `None` for a
    /// question that is not this classifier's.
    pub fn answer(
        &mut self,
        host: &mut Host<'_>,
        live_rules: &[LootRule],
        callback: &Callback,
    ) -> Option<CallbackAnswer> {
        let handle = self.handle?;
        match callback {
            Callback::LootClassify(question) if question.handle == handle => Some(
                CallbackAnswer::Classification(self.classify(host, live_rules, question)),
            ),
            Callback::LootNeedsIdentification(question) if question.handle == handle => {
                Some(CallbackAnswer::Bool(
                    self.classifier
                        .needs_identification(live_rules, &question.context),
                ))
            }
            Callback::LootLooted((owner, _)) | Callback::LootRemoved((owner, _))
                if *owner == handle =>
            {
                Some(CallbackAnswer::Nothing)
            }
            _ => None,
        }
    }

    fn classify(
        &mut self,
        host: &mut Host<'_>,
        live_rules: &[LootRule],
        question: &LootQuestion,
    ) -> Option<LootClassification> {
        if question.profile.is_empty() {
            let mut automation = HostAutomation::new(host);
            return Some(self.classifier.classify(
                live_rules,
                &question.context,
                Some(&mut automation),
            ));
        }
        let now = monotonic_seconds(host);
        let rules = {
            let mut profiles = HostStorage::profiles(host);
            self.classifier
                .load_profile_rules(&mut profiles, &question.profile, now)?
        };
        let mut automation = HostAutomation::new(host);
        Some(
            self.classifier
                .classify(&rules, &question.context, Some(&mut automation)),
        )
    }
}

/// The client's monotonic clock, seconds.
#[allow(clippy::cast_precision_loss)]
fn monotonic_seconds(host: &mut Host<'_>) -> f64 {
    host.clock().monotonic_ms as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::items::LootAction as PublicLootAction;
    use mosstank_formats::utl::LootAction;
    use openac_plugin::testing::FakeHost;
    use openac_plugin::{Plugin, StartInfo};

    use super::*;

    /// A plugin carrying only the classifier, over live rules that keep
    /// everything.
    #[derive(Debug, Default)]
    struct Classifier {
        service: LootClassifierService,
        rules: Vec<LootRule>,
    }

    impl Plugin for Classifier {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            self.service.register(host)
        }

        fn callback(&mut self, host: &mut Host<'_>, callback: Callback) -> CallbackAnswer {
            self.service
                .answer(host, &self.rules, &callback)
                .unwrap_or_default()
        }
    }

    fn keep_all() -> Vec<LootRule> {
        let mut rule = LootRule::named("All", "*");
        rule.action = LootAction::Keep;
        vec![rule]
    }

    fn question(handle: u32, profile: &str) -> LootQuestion {
        LootQuestion {
            handle,
            profile: profile.to_owned(),
            context: LootContext {
                item: InventoryItem {
                    name: "Test Item".to_owned(),
                    stack_size: 1,
                    ..InventoryItem::default()
                },
                ..LootContext::default()
            },
        }
    }

    #[test]
    fn enabling_registers_moss_tank_and_the_live_rules_answer() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::LootClassifierRegister>(&Ok(7));
        let mut plugin = Classifier {
            rules: keep_all(),
            ..Classifier::default()
        };
        fake.enable(&mut plugin).unwrap();

        let registrations = fake.requests::<calls::LootClassifierRegister>();
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].id, "moss-tank");
        assert_eq!(registrations[0].display_name, "MossTank");

        let answer = fake.ask(&mut plugin, &Callback::LootClassify(question(7, "")));
        let CallbackAnswer::Classification(Some(found)) = answer else {
            panic!("a classification: {answer:?}");
        };
        assert!(found.matched);
        assert_eq!(found.action, PublicLootAction::Keep);
        assert_eq!(found.rule_name, "All");
    }

    #[test]
    fn a_question_for_another_handle_is_not_answered() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::LootClassifierRegister>(&Ok(7));
        let mut plugin = Classifier {
            rules: keep_all(),
            ..Classifier::default()
        };
        fake.enable(&mut plugin).unwrap();

        let answer = fake.ask(&mut plugin, &Callback::LootClassify(question(8, "")));
        assert_eq!(answer, CallbackAnswer::Nothing);
    }

    #[test]
    fn a_stored_profile_that_is_not_there_is_no_classification() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::LootClassifierRegister>(&Ok(7));
        let mut plugin = Classifier {
            rules: keep_all(),
            ..Classifier::default()
        };
        fake.enable(&mut plugin).unwrap();

        let answer = fake.ask(&mut plugin, &Callback::LootClassify(question(7, "missing")));
        assert_eq!(answer, CallbackAnswer::Classification(None));
    }
}
