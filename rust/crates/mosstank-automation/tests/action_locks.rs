//! The shared cooldown slots.

use mosstank_automation::locks::{ActionLockKind, ActionLockTable};

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionLockTableTests.cs :: ArmKeepsTheLaterDeadline
#[test]
fn arm_keeps_the_later_deadline() {
    let mut locks = ActionLockTable::new();
    locks.arm(ActionLockKind::Navigation, 3.0);
    locks.arm(ActionLockKind::Navigation, 0.5);
    locks.advance(1.0);
    assert!(locks.is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionLockTableTests.cs :: ArmExtendsAShorterLock
#[test]
fn arm_extends_a_shorter_lock() {
    let mut locks = ActionLockTable::new();
    locks.arm(ActionLockKind::ItemUse, 0.75);
    locks.arm(ActionLockKind::ItemUse, 3.0);
    locks.advance(2.5);
    assert!(locks.is_locked(ActionLockKind::ItemUse));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionLockTableTests.cs :: ADeadlineOnThisExactInstantIsStillLocked
#[test]
fn a_deadline_on_this_exact_instant_is_still_locked() {
    let mut locks = ActionLockTable::new();
    locks.arm(ActionLockKind::DoorOpening, 1.0);
    locks.advance(1.0);
    assert!(locks.is_locked(ActionLockKind::DoorOpening));
    locks.advance(0.001);
    assert!(!locks.is_locked(ActionLockKind::DoorOpening));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionLockTableTests.cs :: AnExpiredLockIsPrunedSoALaterShortArmIsNotExtended
#[test]
fn an_expired_lock_is_pruned_so_a_later_short_arm_is_not_extended() {
    let mut locks = ActionLockTable::new();
    locks.arm(ActionLockKind::Salvage, 10.0);
    locks.advance(11.0);
    assert!(!locks.is_locked(ActionLockKind::Salvage));
    locks.arm(ActionLockKind::Salvage, 1.0);
    locks.advance(2.0);
    assert!(!locks.is_locked(ActionLockKind::Salvage));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionLockTableTests.cs :: ReleaseDropsALiveLockAndClearAllDropsEveryOne
#[test]
fn release_drops_a_live_lock_and_clear_all_drops_every_one() {
    let mut locks = ActionLockTable::new();
    locks.arm(ActionLockKind::Navigation, 5.0);
    locks.arm(ActionLockKind::ItemUse, 5.0);
    locks.release(ActionLockKind::Navigation);
    assert!(!locks.is_locked(ActionLockKind::Navigation));
    assert!(locks.is_locked(ActionLockKind::ItemUse));
    locks.clear_all();
    assert!(!locks.is_locked(ActionLockKind::ItemUse));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionLockTableTests.cs :: AnUnarmedSlotIsNeverLocked
#[test]
fn an_unarmed_slot_is_never_locked() {
    let mut locks = ActionLockTable::new();
    locks.advance(100.0);
    assert!(!locks.is_locked(ActionLockKind::WarSpellLockedOut));
}
