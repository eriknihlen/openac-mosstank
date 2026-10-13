//! The drain planner, the fill confirmation and the donor journal
//! (`ManaStoneSupplyTests`, `ManaDonorJournalTests`).

use std::collections::HashMap;

use ac_plugin_abi::items::{InventoryItem, ItemUseCompletion};
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_formats::utl::LootAction;
use mosstank_loot::donors::{DonorStore, ManaDonorJournal};
use mosstank_loot::mana::{self, ManaFillOutcome, ManaStoneTransferPlan, TransferChecks};

fn stone() -> InventoryItem {
    InventoryItem {
        object_id: 10,
        name: "Stone".to_owned(),
        object_class: ObjectClass::ManaStone,
        item_type: 0x80000,
        ..InventoryItem::default()
    }
}

fn donor() -> InventoryItem {
    InventoryItem {
        object_id: 20,
        name: "Donor".to_owned(),
        workmanship: 5.0,
        item_current_mana: 2000,
        effects: 1,
        ..InventoryItem::default()
    }
}

/// A stone the profile's helper list names as one.
fn profiled_stone(item: &InventoryItem) -> bool {
    item.object_class == ObjectClass::ManaStone && item.name == "Stone"
}

/// The planner's questions, answered as each test's lambdas answer them.
struct Checks {
    profiled: fn(&InventoryItem) -> bool,
    can_use_stone: Option<fn(u32) -> bool>,
    can_drain_tank: Option<fn(u32) -> bool>,
    capture: Option<Box<dyn Fn(u32) -> Option<ItemProperties>>>,
}

impl Checks {
    fn profiled(profiled: fn(&InventoryItem) -> bool) -> Self {
        Self {
            profiled,
            can_use_stone: None,
            can_drain_tank: None,
            capture: None,
        }
    }
}

impl TransferChecks for Checks {
    fn is_profiled_mana_stone(&mut self, item: &InventoryItem) -> bool {
        (self.profiled)(item)
    }

    fn can_use_stone(&mut self, object_id: u32) -> bool {
        self.can_use_stone.is_none_or(|check| check(object_id))
    }

    fn can_drain_tank(&mut self, object_id: u32) -> bool {
        self.can_drain_tank.is_none_or(|check| check(object_id))
    }

    fn reads_writing(&self) -> bool {
        self.capture.is_some()
    }

    fn capture(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.capture.as_ref().and_then(|capture| capture(object_id))
    }
}

fn plan(
    owned: &[InventoryItem],
    classes: &HashMap<u32, LootAction>,
    checks: &mut Checks,
) -> Option<ManaStoneTransferPlan> {
    mana::plan(owned, &|id| classes.get(&id).copied(), 1000, checks)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaStoneSupplyTests.cs :: DonorMustRemainUntinkeredUnwornAndExplicitlyClassified
#[test]
fn donor_must_remain_untinkered_unworn_and_explicitly_classified() {
    let mut classes = HashMap::from([(20, LootAction::ManaTank)]);
    let try_plan = |donor: InventoryItem, classes: &HashMap<u32, LootAction>| {
        plan(
            &[stone(), donor],
            classes,
            &mut Checks::profiled(profiled_stone),
        )
    };
    // Value zero is allowed; workmanship is the criterion.
    assert!(try_plan(donor(), &classes).is_some());
    assert!(
        try_plan(
            InventoryItem {
                workmanship: 0.0,
                value: 100,
                ..donor()
            },
            &classes
        )
        .is_none()
    );
    assert!(
        try_plan(
            InventoryItem {
                num_times_tinkered: 1,
                ..donor()
            },
            &classes
        )
        .is_none()
    );
    assert!(
        try_plan(
            InventoryItem {
                equipped_location: 1,
                ..donor()
            },
            &classes
        )
        .is_none()
    );
    assert!(
        try_plan(
            InventoryItem {
                wielder_id: 1,
                ..donor()
            },
            &classes
        )
        .is_none()
    );
    assert!(
        try_plan(
            InventoryItem {
                effects: 0,
                ..donor()
            },
            &classes
        )
        .is_none()
    );
    // The floor is exact: one point short is short.
    for (mana, expected) in [(999, false), (1000, true), (1001, true)] {
        let found = try_plan(
            InventoryItem {
                item_current_mana: mana,
                ..donor()
            },
            &classes,
        );
        assert_eq!(found.is_some(), expected, "{mana}");
    }
    classes.insert(20, LootAction::Keep);
    assert!(try_plan(donor(), &classes).is_none());
}

fn text(strings: &[(u32, &str)]) -> ItemProperties {
    ItemProperties {
        strings: strings.iter().map(|(k, v)| (*k, (*v).to_owned())).collect(),
        ..ItemProperties::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaStoneSupplyTests.cs :: AnInscribedDonorIsNeverDrained
#[test]
fn an_inscribed_donor_is_never_drained() {
    for written in [7u32, 8] {
        let classes = HashMap::from([(20, LootAction::ManaTank)]);
        let with_capture = |capture: Box<dyn Fn(u32) -> Option<ItemProperties>>| Checks {
            capture: Some(capture),
            ..Checks::profiled(profiled_stone)
        };

        let mut blank = with_capture(Box::new(|_| Some(text(&[]))));
        assert!(plan(&[stone(), donor()], &classes, &mut blank).is_some());

        let mut inscribed = with_capture(Box::new(move |id| {
            Some(if id == 20 {
                text(&[(written, "Erik")])
            } else {
                text(&[])
            })
        }));
        assert!(plan(&[stone(), donor()], &classes, &mut inscribed).is_none());

        // Properties the client cannot produce are not a licence to drain.
        let mut unreadable = with_capture(Box::new(|_| None));
        assert!(plan(&[stone(), donor()], &classes, &mut unreadable).is_none());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaStoneSupplyTests.cs :: ChargedUnconfiguredOrHeldSourcesCannotDrainDonors
#[test]
fn charged_unconfigured_or_held_sources_cannot_drain_donors() {
    let classes = HashMap::from([(20, LootAction::ManaTank)]);
    let charged = InventoryItem {
        effects: 1,
        ..stone()
    };
    assert!(
        plan(
            &[charged, donor()],
            &classes,
            &mut Checks::profiled(profiled_stone)
        )
        .is_none()
    );
    assert!(
        plan(
            &[stone(), donor()],
            &classes,
            &mut Checks::profiled(|_| false)
        )
        .is_none()
    );
    let mut stone_held = Checks {
        can_use_stone: Some(|id| id != 10),
        ..Checks::profiled(profiled_stone)
    };
    assert!(plan(&[stone(), donor()], &classes, &mut stone_held).is_none());
    let mut donor_held = Checks {
        can_drain_tank: Some(|id| id != 20),
        ..Checks::profiled(profiled_stone)
    };
    assert!(plan(&[stone(), donor()], &classes, &mut donor_held).is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaStoneSupplyTests.cs :: SuccessfulReplyWaitsForInventoryAndChargeAndMatchesBothIds
#[test]
fn successful_reply_waits_for_inventory_and_charge_and_matches_both_ids() {
    let plan = ManaStoneTransferPlan {
        stone_object_id: 10,
        tank_object_id: 20,
        stone_name: "Stone".to_owned(),
        tank_name: "Donor".to_owned(),
    };
    let reply = ItemUseCompletion {
        revision: 2,
        source_id: 10,
        target_id: 20,
        weenie_error: 0,
    };
    let observe = |items: &[InventoryItem], completion: &ItemUseCompletion, timeout: bool| {
        mana::observe_fill(&plan, items, completion, 1, timeout)
    };
    let charged = InventoryItem {
        effects: 1,
        ..stone()
    };
    assert_eq!(
        observe(&[stone(), donor()], &reply, false),
        ManaFillOutcome::Waiting
    );
    assert_eq!(observe(&[stone()], &reply, false), ManaFillOutcome::Waiting);
    assert_eq!(
        observe(
            std::slice::from_ref(&charged),
            &ItemUseCompletion {
                target_id: 99,
                ..reply.clone()
            },
            false
        ),
        ManaFillOutcome::Waiting
    );
    assert_eq!(
        observe(
            std::slice::from_ref(&charged),
            &ItemUseCompletion {
                source_id: 99,
                ..reply.clone()
            },
            false
        ),
        ManaFillOutcome::Waiting
    );
    assert_eq!(
        observe(
            std::slice::from_ref(&charged),
            &ItemUseCompletion {
                revision: 1,
                ..reply.clone()
            },
            false
        ),
        ManaFillOutcome::Waiting
    );
    assert_eq!(
        observe(std::slice::from_ref(&charged), &reply, false),
        ManaFillOutcome::Confirmed
    );
    assert_eq!(observe(&[], &reply, false), ManaFillOutcome::Confirmed);
    assert_eq!(
        observe(
            &[stone(), donor()],
            &ItemUseCompletion {
                weenie_error: 1,
                ..reply.clone()
            },
            false
        ),
        ManaFillOutcome::Uncertain
    );
    assert_eq!(
        observe(&[stone(), donor()], &ItemUseCompletion::default(), true),
        ManaFillOutcome::Uncertain
    );
}

/// The journal tests' storage (`ManaDonorJournalTests.Storage`).
#[derive(Debug, Default)]
struct Storage {
    values: HashMap<String, String>,
    fail_writes: bool,
}

impl DonorStore for Storage {
    fn is_available(&mut self) -> bool {
        true
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, String> {
        Ok(self.values.get(key).cloned())
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), String> {
        if self.fail_writes {
            return Err("Test write failure".to_owned());
        }
        self.values.insert(key.to_owned(), content.to_owned());
        Ok(())
    }

    fn info(&mut self, _message: &str) {}

    fn error(&mut self, _message: &str) {}
}

fn journal_donor() -> InventoryItem {
    InventoryItem {
        object_id: 20,
        weenie_class_id: 48,
        name: "Wand".to_owned(),
        container_id: 1,
        ..InventoryItem::default()
    }
}

fn journal_stone() -> InventoryItem {
    InventoryItem {
        object_id: 10,
        weenie_class_id: 47,
        name: "Stone".to_owned(),
        container_id: 1,
        ..InventoryItem::default()
    }
}

fn journal_plan() -> ManaStoneTransferPlan {
    ManaStoneTransferPlan {
        stone_object_id: 10,
        tank_object_id: 20,
        stone_name: "Stone".to_owned(),
        tank_name: "Wand".to_owned(),
    }
}

fn open(storage: &mut Storage) -> ManaDonorJournal {
    let mut journal = ManaDonorJournal::new();
    journal.bind(storage, "Coldeve", "Tester");
    journal
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: RelogRetainsPermissionButAnIncompleteInventoryCannotRemoveIt
#[test]
fn relog_retains_permission_but_an_incomplete_inventory_cannot_remove_it() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    assert!(journal.remember(&mut storage, &journal_donor()));
    let mut journal = open(&mut storage);
    journal.reconcile(&mut storage, &[], false);
    assert!(journal.donor_ready(&mut storage, 20));
    journal.reconcile(&mut storage, &[journal_donor()], true);
    assert!(journal.donor_ready(&mut storage, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: ConfirmedOwnershipLossRevokesPermissionPermanently
#[test]
fn confirmed_ownership_loss_revokes_permission_permanently() {
    for operation in ["drop", "sell", "give", "consume"] {
        assert!(!operation.is_empty());
        let mut storage = Storage::default();
        let mut journal = open(&mut storage);
        journal.remember(&mut storage, &journal_donor());
        journal.reconcile(&mut storage, &[], true);
        journal.reconcile(&mut storage, &[journal_donor()], true);
        assert!(!journal.donor_ready(&mut storage, 20));
        assert_eq!(open(&mut storage).entries().count(), 0);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: MissingDonorAfterRelogIsRemovedWhenInventoryCompletes
#[test]
fn missing_donor_after_relog_is_removed_when_inventory_completes() {
    let mut storage = Storage::default();
    open(&mut storage).remember(&mut storage, &journal_donor());
    let mut restored = open(&mut storage);
    restored.reconcile(&mut storage, &[], true);
    assert_eq!(restored.entries().count(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: EachCharacterAndServerHasSeparatePermission
#[test]
fn each_character_and_server_has_separate_permission() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    journal.bind(&mut storage, "Other server", "Tester");
    assert_eq!(journal.entries().count(), 0);
    journal.bind(&mut storage, "Coldeve", "Other character");
    assert_eq!(journal.entries().count(), 0);
    journal.bind(&mut storage, "Coldeve", "Tester");
    assert!(journal.donor_ready(&mut storage, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: InterruptedSendStaysHeldAcrossRelogAndLateInventorySettlesIt
#[test]
fn interrupted_send_stays_held_across_relog_and_late_inventory_settles_it() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    assert!(journal.begin(&mut storage, &journal_plan()));
    let mut journal = open(&mut storage);
    journal.reconcile(&mut storage, &[journal_stone(), journal_donor()], true);
    assert!(!journal.donor_ready(&mut storage, 20));
    assert!(journal.stone_held(10));
    let charged = InventoryItem {
        effects: 1,
        ..journal_stone()
    };
    journal.reconcile(&mut storage, &[charged], true);
    assert!(!journal.stone_held(10));
    assert_eq!(journal.entries().count(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: DroppingDonorWhileRequestIsUncertainDoesNotFreeTheStoneToRepeatIt
#[test]
fn dropping_donor_while_request_is_uncertain_does_not_free_the_stone_to_repeat_it() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    journal.begin(&mut storage, &journal_plan());
    journal.reconcile(&mut storage, &[journal_stone()], true);
    assert!(!journal.donor_ready(&mut storage, 20));
    assert!(journal.stone_held(10));
    journal.reconcile(&mut storage, &[journal_stone(), journal_donor()], true);
    assert!(!journal.donor_ready(&mut storage, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: RecycledObjectIdentityIsNotPermissionToDrainAnotherItem
#[test]
fn recycled_object_identity_is_not_permission_to_drain_another_item() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    let recycled = InventoryItem {
        weenie_class_id: 99,
        ..journal_donor()
    };
    journal.reconcile(&mut storage, &[recycled], true);
    assert!(!journal.donor_ready(&mut storage, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: PlaceholderDuringLoginDoesNotRevokeTheSavedIdentity
#[test]
fn placeholder_during_login_does_not_revoke_the_saved_identity() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    let placeholder = InventoryItem {
        weenie_class_id: 0,
        name: String::new(),
        ..journal_donor()
    };
    journal.reconcile(&mut storage, &[placeholder], false);
    assert!(journal.donor_ready(&mut storage, 20));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ManaDonorJournalTests.cs :: FailureToPersistPreventsSendingAndCorruptHistoryIsNotOverwritten
#[test]
fn failure_to_persist_prevents_sending_and_corrupt_history_is_not_overwritten() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    storage.fail_writes = true;
    assert!(!journal.begin(&mut storage, &journal_plan()));
    assert!(!journal.can_operate(&mut storage));
    storage.fail_writes = false;
    assert_eq!(storage.values.len(), 1);
    let key = storage.values.keys().next().expect("one file").clone();
    storage.values.insert(key.clone(), "broken".to_owned());
    let mut journal = open(&mut storage);
    assert!(!journal.can_operate(&mut storage));
    assert!(!journal.remember(&mut storage, &journal_donor()));
    assert_eq!(storage.values[&key], "broken");
}

/// The file's name and shape, as `SHA256(scope)` and `JsonSerializer`
/// write them.
#[test]
fn the_journal_file_is_keyed_by_the_scope_hash_and_written_compact() {
    let mut storage = Storage::default();
    let mut journal = open(&mut storage);
    journal.remember(&mut storage, &journal_donor());
    let (key, content) = storage.values.iter().next().expect("one file");
    assert!(key.starts_with("mana-donors/"));
    assert!(key.ends_with(".json"));
    let hex = &key["mana-donors/".len()..key.len() - ".json".len()];
    assert_eq!(hex.len(), 64);
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
    );
    assert_eq!(
        content,
        r#"[{"ObjectId":20,"ClassId":48,"Name":"Wand","StoneId":0,"Removed":false,"BlockedReason":null}]"#
    );
}
