//! The combat-mode gate and the equipment tracker beneath it.

mod common;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::items::{EquipmentItem, EquipmentObservation};
use ac_plugin_abi::social::BusyState;
use ac_plugin_abi::world::ObjectClass;
use common::{FakeAutomation, Gear, in_mode, physical, recording_log, wand};
use mosstank_automation::equipment::EquipmentTracker;
use mosstank_combat::CombatModeGate;
use mosstank_combat::gate::{GateTurn, NO_WAND_NOTICE, Prepare};
use mosstank_meta::scheduler::MacroLogChannel;
use mosstank_settings::model::{CombatSettings, VitalSettings};

/// The C# `Gate(...)` helper: a gate, its settings and the stops it asks
/// for.
struct Rig {
    surface: FakeAutomation,
    gate: CombatModeGate,
    combat: CombatSettings,
    vitals: VitalSettings,
    stops: Vec<String>,
}

impl Rig {
    fn new(surface: FakeAutomation) -> Self {
        Self::with(surface, CombatSettings::default(), VitalSettings::default())
    }

    fn with(surface: FakeAutomation, combat: CombatSettings, vitals: VitalSettings) -> Self {
        Self {
            surface,
            gate: CombatModeGate::new(),
            combat,
            vitals,
            stops: Vec::new(),
        }
    }

    fn drain(&mut self) {
        while let Some(stop) = self.gate.take_stop_request() {
            self.stops.push(stop);
        }
    }

    fn prepare_with(&mut self, request: Prepare<'_>) -> bool {
        let mut turn = GateTurn {
            automation: &mut self.surface,
            combat: &self.combat,
            vitals: &self.vitals,
        };
        let ready = self.gate.try_prepare(&mut turn, request, None);
        self.drain();
        ready
    }

    fn prepare(&mut self, mode: CombatMode) -> bool {
        self.prepare_with(Prepare::mode(mode))
    }

    fn advance(&mut self, seconds: f64) {
        self.gate.advance_pass(&mut self.surface, seconds);
    }

    fn drop_to_peace(&mut self, items: &[EquipmentItem], name: &str) -> bool {
        let ready =
            self.gate
                .try_drop_to_peace(&mut self.surface, &self.combat, &self.vitals, items, name);
        self.drain();
        ready
    }
}

fn recovery_wand_held() -> EquipmentItem {
    wand(800, "Recovery Wand").equipped(0x0010_0000).build()
}

fn profiled(names_and_ids: &[(&str, Option<u32>)]) -> CombatSettings {
    let mut settings = CombatSettings::default();
    for &(name, id) in names_and_ids {
        settings.combat_item_names.insert(name.to_owned());
        settings.combat_item_order.push(name.to_owned());
        if let Some(id) = id {
            settings.combat_item_order_ids.push(id);
        }
    }
    settings
}

fn calls(surface: &FakeAutomation) -> Vec<&str> {
    surface.call_log.iter().map(String::as_str).collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateWaitsForTheServersStanceAfterAModeChangeBeforeItIsReady
#[test]
fn gate_waits_for_the_servers_stance_after_a_mode_change_before_it_is_ready() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        withhold_mode_echo: true,
        equipment_items: vec![recovery_wand_held()],
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.mode_change_requests, 1);
    assert_eq!(rig.surface.combat_snapshot.mode, CombatMode::Magic);

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Entering Magic mode");
    assert_eq!(rig.surface.mode_change_requests, 1);

    rig.surface.confirm_pending_mode_change();
    rig.advance(0.1);
    assert!(rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Ready");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateIsReadyTheMomentTheServerAgreesWhenTheHostSaysWhatTheServerSaid
#[test]
fn gate_is_ready_the_moment_the_server_agrees_when_the_host_says_what_the_server_said() {
    let mut snapshot = in_mode(CombatMode::Peace);
    snapshot.server_mode = CombatMode::Peace;
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: snapshot,
        withhold_mode_echo: true,
        equipment_items: vec![recovery_wand_held()],
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.combat_snapshot.mode, CombatMode::Magic);
    assert_eq!(rig.surface.combat_snapshot.server_mode, CombatMode::Peace);

    // The window running out changes nothing while the server disagrees.
    rig.advance(0.7);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Entering Magic mode");
    assert_eq!(rig.surface.mode_change_requests, 1);

    rig.surface.confirm_pending_mode_change();
    assert_eq!(rig.surface.combat_snapshot.server_mode, CombatMode::Magic);
    rig.advance(0.01);
    assert!(rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Ready");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateGivesUpWaitingForTheServersStanceAfterTheConfirmationWindow
#[test]
fn gate_gives_up_waiting_for_the_servers_stance_after_the_confirmation_window() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        withhold_mode_echo: true,
        equipment_items: vec![recovery_wand_held()],
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    rig.advance(0.5);
    assert!(!rig.prepare(CombatMode::Magic));
    rig.advance(0.11);
    assert!(rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.mode_change_requests, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateWieldedCasterInPeaceReAsksForMagicUntilTheClientAgrees
#[test]
fn gate_wielded_caster_in_peace_re_asks_for_magic_until_the_client_agrees() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        defer_mode_confirmation: true,
        equipment_items: vec![recovery_wand_held()],
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.mode_change_requests, 1);
    assert_eq!(calls(&rig.surface), ["EnterMode:Magic"]);

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.mode_change_requests, 2);

    rig.surface.confirm_pending_mode_change();
    rig.advance(0.1);
    assert!(rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.mode_change_requests, 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateBlocksOnPendingInventoryButNotAppraisalBusyCount
#[test]
fn gate_blocks_on_pending_inventory_but_not_appraisal_busy_count() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        equipment_items: vec![wand(800, "Wand").equipped(0x0010_0000).build()],
        busy_state: BusyState {
            busy_count: 1,
            pending_inventory: false,
            awaiting_appraisal: 800,
            use_source: 0,
            use_target: 0,
            awaiting_use_completion: false,
        },
        ..FakeAutomation::default()
    });

    assert!(rig.prepare(CombatMode::Magic));
    rig.surface.busy_state.pending_inventory = true;
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Busy");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateProfiledCasterNotWieldedWieldsThenEntersMagicInOrder
#[test]
fn gate_profiled_caster_not_wielded_wields_then_enters_magic_in_order() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: physical(),
            defer_mode_confirmation: true,
            simulate_async_equip: true,
            equipment_items: vec![wand(800, "Recovery Wand").build()],
            ..FakeAutomation::default()
        },
        profiled(&[("Recovery Wand", Some(800))]),
        VitalSettings::default(),
    );

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(calls(&rig.surface), ["EnterMode:Peace"]);

    rig.surface.confirm_pending_mode_change();
    // The ack restarts the window, so the gate still reports the old mode
    // for one more pass and re-asks for peace.
    rig.advance(1.0);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(calls(&rig.surface), ["EnterMode:Peace", "EnterMode:Peace"]);

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(
        calls(&rig.surface),
        ["EnterMode:Peace", "EnterMode:Peace", "Equip:00000320"]
    );

    rig.surface.confirm_pending_equip();
    rig.advance(1.0);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(
        calls(&rig.surface),
        [
            "EnterMode:Peace",
            "EnterMode:Peace",
            "Equip:00000320",
            "EnterMode:Magic"
        ]
    );

    rig.surface.confirm_pending_mode_change();
    rig.advance(1.0);
    assert!(rig.prepare(CombatMode::Magic));
    assert_eq!(
        calls(&rig.surface),
        [
            "EnterMode:Peace",
            "EnterMode:Peace",
            "Equip:00000320",
            "EnterMode:Magic"
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateLogsRequestingPeaceThenEquipThenRequestingMagicAndNeverReDropsAfterEquip
#[test]
fn gate_logs_requesting_peace_then_equip_then_requesting_magic_and_never_re_drops_after_equip() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: physical(),
            equipment_items: vec![wand(800, "Recovery Wand").build()],
            ..FakeAutomation::default()
        },
        profiled(&[("Recovery Wand", Some(800))]),
        VitalSettings::default(),
    );
    let (log, lines) = recording_log();
    rig.gate.set_log(Some(log));

    let mut ready = false;
    for _ in 0..8 {
        if ready {
            break;
        }
        rig.advance(1.0);
        ready = rig.prepare(CombatMode::Magic);
    }
    assert!(ready, "gate never converged");

    let lines = lines.borrow();
    let expected = [
        "(FCM) requesting Peace",
        "(FCM) requesting Peace",
        "(FCM) equip Recovery Wand",
        "(FCM) requesting Magic",
    ];
    assert_eq!(lines.len(), expected.len());
    for ((channel, text), want) in lines.iter().zip(expected) {
        assert_eq!(*channel, MacroLogChannel::BusyState);
        assert_eq!(text, want);
    }
    let equip_index = lines
        .iter()
        .position(|(_, text)| text.contains("equip"))
        .unwrap();
    assert!(
        !lines[equip_index + 1..]
            .iter()
            .any(|(_, text)| text.contains("requesting Peace"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateNoWandAnywherePostsTheNoticeOnceAndStopsTheMacro
#[test]
fn gate_no_wand_anywhere_posts_the_notice_once_and_stops_the_macro() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: physical(),
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.posted_system_messages.len(), 1);
    assert_eq!(
        rig.surface.posted_system_messages[0].replace("[MossTank] ", ""),
        "Add at least one wand to your profile first."
    );
    assert_eq!(rig.stops, ["Add at least one wand to your profile first."]);

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.posted_system_messages.len(), 1);
    assert_eq!(rig.stops.len(), 2);

    rig.gate.reset();
    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.posted_system_messages.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateDoesNotTreatSidecarNamesAsItemIdentity
#[test]
fn gate_does_not_treat_sidecar_names_as_item_identity() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: common::peaceful(),
            equipment_items: vec![wand(801, "Same Wand").build()],
            ..FakeAutomation::default()
        },
        profiled(&[("Same Wand", None)]),
        VitalSettings::default(),
    );

    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), NO_WAND_NOTICE);
    assert_eq!(rig.stops, [NO_WAND_NOTICE]);
    assert!(rig.surface.call_log.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateUsesOrderedIdWithDuplicateNamesAndSkipsMissingOrForeignObjects
#[test]
fn gate_uses_ordered_id_with_duplicate_names_and_skips_missing_or_foreign_objects() {
    let mut surface = FakeAutomation {
        combat_snapshot: common::peaceful(),
        equipment_items: vec![
            wand(801, "Same Wand").build(),
            wand(802, "Same Wand").build(),
            wand(800, "Same Wand").build(),
        ],
        ..FakeAutomation::default()
    };
    surface.unowned_equipment_ids.insert(801);
    let mut settings = profiled(&[("Same Wand", None)]);
    settings.combat_item_order_ids.extend([803, 801, 802, 800]);
    let mut rig = Rig::with(surface, settings, VitalSettings::default());

    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.last_equip_object_id, 802);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateModeRequestNeverConfirmedKeepsAskingWithoutGivingUp
#[test]
fn gate_mode_request_never_confirmed_keeps_asking_without_giving_up() {
    let vitals = VitalSettings {
        drop_to_peace_mode_retry_count: 2,
        ..VitalSettings::default()
    };
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: in_mode(CombatMode::Peace),
            defer_mode_confirmation: true,
            equipment_items: vec![recovery_wand_held()],
            ..FakeAutomation::default()
        },
        CombatSettings::default(),
        vitals,
    );

    for _ in 0..6 {
        rig.advance(1.0);
        assert!(!rig.prepare(CombatMode::Magic));
    }

    assert_eq!(rig.surface.mode_change_requests, 6);
    assert!(rig.stops.is_empty());
    assert!(rig.surface.posted_system_messages.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateDropToPeaceBudgetIsCountedInPassesAndEndsInWandUseRecovery
#[test]
fn gate_drop_to_peace_budget_is_counted_in_passes_and_ends_in_wand_use_recovery() {
    let vitals = VitalSettings {
        drop_to_peace_mode_retry_count: 3,
        ..VitalSettings::default()
    };
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: physical(),
            defer_mode_confirmation: true,
            equipment_items: vec![wand(800, "Recovery Wand").build()],
            ..FakeAutomation::default()
        },
        profiled(&[("Recovery Wand", Some(800))]),
        vitals,
    );

    rig.advance(1.0);
    assert!(!rig.prepare(CombatMode::Magic));
    rig.advance(1.0);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.mode_change_requests, 2);
    assert_eq!(rig.surface.last_used_item, 0);

    rig.advance(1.0);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.last_used_item, 800);
    assert_eq!(rig.surface.mode_change_requests, 2);
    assert!(rig.stops.is_empty());
    assert_eq!(rig.surface.posted_system_messages.len(), 1);
    assert!(
        rig.surface.posted_system_messages[0]
            .to_lowercase()
            .contains("bugged combat state")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateFallbackWandIsItemsPageInsertionOrderNotNameOrder
#[test]
fn gate_fallback_wand_is_items_page_insertion_order_not_name_order() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: in_mode(CombatMode::Peace),
            equipment_items: vec![
                wand(801, "Zephyr Wand").build(),
                wand(802, "Adamant Wand").build(),
            ],
            ..FakeAutomation::default()
        },
        profiled(&[("Zephyr Wand", Some(801)), ("Adamant Wand", Some(802))]),
        VitalSettings::default(),
    );

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(calls(&rig.surface), ["Equip:00000321"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateKeepsAnAlreadyWieldedCasterRatherThanReEquipping
#[test]
fn gate_keeps_an_already_wielded_caster_rather_than_re_equipping() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: in_mode(CombatMode::Magic),
            equipment_items: vec![
                wand(801, "Zephyr Wand").equipped(0x0010_0000).build(),
                wand(802, "Adamant Wand").build(),
            ],
            ..FakeAutomation::default()
        },
        profiled(&[("Adamant Wand", None)]),
        VitalSettings::default(),
    );

    rig.advance(0.1);
    assert!(rig.prepare(CombatMode::Magic));
    assert!(rig.surface.call_log.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateIsHandsOffWhileTheCombatModeIsUnknown
#[test]
fn gate_is_hands_off_while_the_combat_mode_is_unknown() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: ac_plugin_abi::combat::CombatSnapshot {
                mode: CombatMode::Unknown,
                ..Default::default()
            },
            equipment_items: vec![wand(800, "Recovery Wand").build()],
            ..FakeAutomation::default()
        },
        profiled(&[("Recovery Wand", Some(800))]),
        VitalSettings::default(),
    );

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert!(rig.surface.call_log.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateWithNoEquipmentProjectionStillRequiresTheMode
#[test]
fn gate_with_no_equipment_projection_still_requires_the_mode() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: physical(),
        equipment_available: false,
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(calls(&rig.surface), ["EnterMode:Magic"]);

    rig.advance(1.0);
    assert!(rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Ready");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateDoesNotReportReadyWhenModeCommandIsUnavailable
#[test]
fn gate_does_not_report_ready_when_mode_command_is_unavailable() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: physical(),
            mode_command_unavailable: true,
            equipment_items: vec![wand(800, "War Wand").equipped(0x0010_0000).build()],
            ..FakeAutomation::default()
        },
        profiled(&[("War Wand", None)]),
        VitalSettings::default(),
    );

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.gate.status(), "Combat mode is unavailable");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: RawModeChangeWithoutMotionAckDoesNotRestartWindow
#[test]
fn raw_mode_change_without_motion_ack_does_not_restart_window() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: physical(),
        defer_mode_confirmation: true,
        ..FakeAutomation::default()
    });

    assert!(!rig.drop_to_peace(&[], "Sword"));
    rig.surface.combat_snapshot.mode = CombatMode::Peace;
    rig.advance(0.61);

    assert!(rig.drop_to_peace(&[], "Sword"));
    assert_eq!(calls(&rig.surface), ["EnterMode:Peace"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: EquipmentTrackerSeedsFromWorldOrderAndUsesExactSlots
#[test]
fn equipment_tracker_seeds_from_world_order_and_uses_exact_slots() {
    let mut surface = FakeAutomation {
        equipment_items: vec![
            Gear::new(10, "Zulu Sword", 1).equipped(0x0200_0000).build(),
            Gear::new(30, "Combined Mask", 1)
                .equipped(0x0030_0000)
                .build(),
            Gear::new(20, "Alpha Sword", 1)
                .equipped(0x0010_0000)
                .build(),
        ],
        ..FakeAutomation::default()
    };
    let mut tracker = EquipmentTracker::new(&mut surface);

    assert_eq!(tracker.weapon_id(&mut surface), 20);
    assert_eq!(tracker.shield_id(&mut surface), 0);
    assert_eq!(tracker.equipped_ids(&mut surface), [10, 30, 20]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: EquipmentTrackerAcceptsInitialEquippedObjectWithoutSettlement
#[test]
fn equipment_tracker_accepts_initial_equipped_object_without_settlement() {
    let mut surface = FakeAutomation::default();
    let mut tracker = EquipmentTracker::new(&mut surface);
    let mut pokes = 0;

    surface.equipment_items = vec![wand(10, "Held Wand").equipped(0x0100_0000).build()];
    surface.emit_placement(EquipmentObservation {
        object_id: 10,
        equipped_location: 0x0100_0000,
        is_removal: false,
        is_initial_placement: true,
    });

    assert_eq!(tracker.weapon_id(&mut surface), 10);
    assert_eq!(tracker.equipped_ids(&mut surface), [10]);
    tracker.advance(&mut surface, 0.1);
    if tracker.take_settled() {
        pokes += 1;
    }
    assert_eq!(pokes, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: EquipmentTrackerReceiptsSettleIndependentlyAndValidateIds
#[test]
fn equipment_tracker_receipts_settle_independently_and_validate_ids() {
    let mut surface = FakeAutomation {
        equipment_items: vec![Gear::new(10, "Sword", 1).equipped(0x0010_0000).build()],
        ..FakeAutomation::default()
    };
    let mut tracker = EquipmentTracker::new(&mut surface);
    let mut pokes = 0;
    let mut advance = |tracker: &mut EquipmentTracker, surface: &mut FakeAutomation, s: f64| {
        tracker.advance(surface, s);
        if tracker.take_settled() {
            pokes += 1;
        }
        pokes
    };
    assert!(tracker.try_arm_swap(10, true, CombatMode::Peace));
    assert!(tracker.recently_swapped());

    surface.emit_placement(EquipmentObservation {
        object_id: 99,
        equipped_location: 0x0080_0000,
        is_removal: false,
        is_initial_placement: false,
    });
    assert_eq!(tracker.ammo_id(&mut surface), 0); // receipt can predate the object
    assert_eq!(advance(&mut tracker, &mut surface, 0.099), 0);
    assert!(tracker.recently_swapped());
    assert_eq!(advance(&mut tracker, &mut surface, 0.001), 1);
    assert!(!tracker.recently_swapped());

    assert!(tracker.try_arm_swap(10, false, CombatMode::Melee));
    advance(&mut tracker, &mut surface, 0.8);
    assert!(tracker.recently_swapped()); // strict deadline comparison
    advance(&mut tracker, &mut surface, 0.001);
    assert!(!tracker.recently_swapped());

    surface.emit_placement(EquipmentObservation {
        object_id: 10,
        equipped_location: 0,
        is_removal: true,
        is_initial_placement: false,
    });
    assert_eq!(tracker.weapon_id(&mut surface), 0);
    assert!(!tracker.equipped_ids(&mut surface).contains(&10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateDefaultShieldUsesKindAfterWeaponClass
#[test]
fn gate_default_shield_uses_kind_after_weapon_class() {
    for masked_class in [
        ObjectClass::MeleeWeapon,
        ObjectClass::MissileWeapon,
        ObjectClass::WandStaffOrb,
    ] {
        let mut misleading = Gear::new(200, "Misleading Sword", 1)
            .valid(0x0020_0000)
            .build();
        misleading.object_class = masked_class;
        let mut shield = Gear::new(300, "Shield", 0).valid(0x0020_0000).build();
        shield.object_class = ObjectClass::Armor;
        let mut settings = CombatSettings::default();
        settings.combat_item_order_ids.extend([200, 300]);
        let mut rig = Rig::with(
            FakeAutomation {
                combat_snapshot: in_mode(CombatMode::Peace),
                equipment_items: vec![
                    Gear::new(100, "Sword", 1).equipped(0x0010_0000).build(),
                    misleading,
                    shield,
                ],
                ..FakeAutomation::default()
            },
            settings,
            VitalSettings::default(),
        );

        assert!(!rig.prepare_with(Prepare {
            override_item_id: 100,
            auto_select: false,
            ..Prepare::mode(CombatMode::Melee)
        }));
        assert!(calls(&rig.surface).contains(&"EquipSecondary:0000012C"));
        assert!(!calls(&rig.surface).contains(&"EquipSecondary:000000C8"));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: GateThrownSecondaryPrecedesPrimaryAfterRemoval
#[test]
fn gate_thrown_secondary_precedes_primary_after_removal() {
    let mut shield = Gear::new(300, "Shield", 0).valid(0x0020_0000).build();
    shield.object_class = ObjectClass::Armor;
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Peace),
        equipment_items: vec![
            Gear::new(991, "Old Sword", 1).equipped(0x0010_0000).build(),
            Gear::new(100, "Thrown", 1).item_type(0x100).build(),
            shield,
        ],
        ..FakeAutomation::default()
    });
    let request = Prepare {
        override_item_id: 100,
        auto_select: false,
        secondary_item_id: Some(300),
        ..Prepare::mode(CombatMode::Missile)
    };

    assert!(!rig.prepare_with(request));
    rig.advance(0.1);
    assert!(!rig.prepare_with(request));
    rig.advance(0.1);
    assert!(!rig.prepare_with(request));

    assert_eq!(
        calls(&rig.surface),
        ["Move:000003DF", "EquipSecondary:0000012C", "Equip:00000064"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: OncePerRunWarningLatchClearsAtTheMacroStartEdge
#[test]
fn once_per_run_warning_latch_clears_at_the_macro_start_edge() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: physical(),
        ..FakeAutomation::default()
    });

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.posted_system_messages.len(), 1);

    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.posted_system_messages.len(), 1);

    rig.gate.reset_once_per_run_warnings();
    rig.advance(0.1);
    assert!(!rig.prepare(CombatMode::Magic));
    assert_eq!(rig.surface.posted_system_messages.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: FcmOwnedObjectWithNoWeaponMaskIsAccepted
#[test]
fn fcm_owned_object_with_no_weapon_mask_is_accepted() {
    let mut rig = Rig::new(FakeAutomation {
        combat_snapshot: physical(),
        equipment_items: vec![
            Gear::new(900, "Bread", 0).item_type(1).valid(0).build(),
            wand(800, "War Wand").equipped(0x0010_0000).build(),
        ],
        ..FakeAutomation::default()
    });
    let request = Prepare {
        override_item_id: 900,
        ..Prepare::mode(CombatMode::Magic)
    };

    rig.advance(0.1);
    rig.prepare_with(request);
    assert!(rig.surface.posted_system_messages.is_empty());

    rig.advance(0.1);
    rig.prepare_with(request);
    assert!(rig.surface.posted_system_messages.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheModeWindowIsTimeBasedAndTheAckRestartsIt
#[test]
fn the_mode_window_is_time_based_and_the_ack_restarts_it() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: physical(),
            defer_mode_confirmation: true,
            equipment_items: vec![wand(800, "Recovery Wand").build()],
            ..FakeAutomation::default()
        },
        profiled(&[("Recovery Wand", Some(800))]),
        VitalSettings::default(),
    );

    rig.advance(0.1);
    let items = rig.surface.equipment_items.clone();
    assert!(!rig.drop_to_peace(&items, "Recovery Wand"));
    assert_eq!(calls(&rig.surface), ["EnterMode:Peace"]);

    rig.surface.confirm_pending_mode_change();
    rig.advance(0.1);
    assert!(!rig.drop_to_peace(&items, "Recovery Wand"));
    assert_eq!(calls(&rig.surface), ["EnterMode:Peace", "EnterMode:Peace"]);

    rig.advance(0.1);
    assert!(rig.drop_to_peace(&items, "Recovery Wand"));
    assert_eq!(calls(&rig.surface), ["EnterMode:Peace", "EnterMode:Peace"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ThreeGateCallsWithinOnePassAdvanceTheClockExactlyOnce
#[test]
fn three_gate_calls_within_one_pass_advance_the_clock_exactly_once() {
    let mut rig = Rig::with(
        FakeAutomation {
            combat_snapshot: in_mode(CombatMode::Magic),
            equipment_items: vec![recovery_wand_held()],
            ..FakeAutomation::default()
        },
        profiled(&[("Recovery Wand", Some(800))]),
        VitalSettings::default(),
    );

    assert!(rig.prepare(CombatMode::Magic));
    assert!(rig.surface.call_log.is_empty());

    rig.advance(0.25);
    let after_one_advance = rig.gate.since_mode_request_seconds();

    assert!(rig.prepare(CombatMode::Magic));
    assert!(rig.prepare(CombatMode::Magic));
    assert!(rig.prepare(CombatMode::Magic));
    assert!(rig.surface.call_log.is_empty());

    assert!((rig.gate.since_mode_request_seconds() - after_one_advance).abs() == 0.0);
}
