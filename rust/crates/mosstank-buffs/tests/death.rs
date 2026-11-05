//! What a death does to the buff rule's item timers (divergence BUFF-2).

use mosstank_buffs::buff_rule::BuffSelfRule;

const CHARACTER: u32 = 0x5000_0001;
const IMPENETRABILITY_FAMILY: u32 = 159;

/// The game keeps an item's enchantments through a death, banes cast on
/// the character included: the rule keeps their timers, so it does not
/// cast them all again after every death.
#[test]
fn a_death_keeps_the_bane_and_item_timers() {
    let mut rule = BuffSelfRule::new();
    rule.item_ledger_mut().note_cast(
        CHARACTER,
        IMPENETRABILITY_FAMILY,
        2108,
        7,
        3600.0,
        0.0,
        "Impenetrability VII",
        &mut |_| {},
    );

    rule.on_death();

    assert_eq!(rule.item_ledger().count(), 1);
    let left = rule
        .item_ledger()
        .remaining_seconds(CHARACTER, IMPENETRABILITY_FAMILY, 7, 0.0);
    assert!((left - 3600.0).abs() < 1e-9, "{left}");
}

/// A force under way ends at a death, and the timers it zeroed come back.
#[test]
fn a_death_ends_a_force_with_the_real_times_back() {
    let mut rule = BuffSelfRule::new();
    rule.item_ledger_mut().note_cast(
        CHARACTER,
        IMPENETRABILITY_FAMILY,
        2108,
        7,
        3600.0,
        0.0,
        "Impenetrability VII",
        &mut |_| {},
    );
    rule.item_ledger_mut().force_all(0.0);
    assert!(rule.item_ledger().has_forced_entries());

    rule.on_death();

    assert!(!rule.item_ledger().has_forced_entries());
    let left = rule
        .item_ledger()
        .remaining_seconds(CHARACTER, IMPENETRABILITY_FAMILY, 7, 0.0);
    assert!((left - 3600.0).abs() < 1e-9, "{left}");
}
