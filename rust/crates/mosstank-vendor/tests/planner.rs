//! The money and slot planner behind a vendor run
//! (`VendorTradePlannerTests`). Every number is worked by hand from the
//! vendor's terms.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::trade::{NO_VALUE_LIMIT, VendorItem, VendorProfile};
use ac_plugin_abi::world::ObjectClass;
use mosstank_vendor::planner::{
    self, MAXIMUM_SELL_LINES, VendorBuyCandidate, VendorPurse, VendorSellCandidate,
    VendorSplitRequest,
};

/// The server's charge: each stack priced whole, rounded up from a tenth
/// over.
#[allow(clippy::cast_possible_truncation)]
fn server_cost(count: i32, per_item: f64, max_stack: i32) -> i64 {
    let mut total = 0_i64;
    let mut left = count;
    while left > 0 {
        let stack = left.min(max_stack);
        total += (per_item * f64::from(stack) - 0.1).ceil() as i64;
        left -= max_stack;
    }
    total
}

fn three_quarters() -> VendorProfile {
    VendorProfile {
        buy_rate: 0.75,
        deals_in_item_types: 0x0000_0001 | 0x0000_0020,
        minimum_value: NO_VALUE_LIMIT,
        maximum_value: 1000,
        deals_in_magical_items: true,
        alternate_currency_weenie_class_id: 0,
        alternate_currency_amount: 0,
        alternate_currency_name: None,
    }
}

fn listing(
    id: u32,
    name: &str,
    object_class: ObjectClass,
    unit_price: i32,
    max_stack: i32,
) -> VendorItem {
    VendorItem {
        template_id: id,
        weenie_class_id: id + 1000,
        name: name.to_owned(),
        object_class,
        unit_price,
        stack_size: 1,
        max_stack_size: max_stack,
        item_type: 0x1,
    }
}

fn owned_as(
    id: u32,
    name: &str,
    value: i32,
    stack: i32,
    object_class: ObjectClass,
    item_type: u32,
) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: id + 2000,
        name: name.to_owned(),
        item_type,
        container_id: 1,
        stack_size: stack,
        value,
        maximum_stack_size: stack.max(1),
        object_class,
        ..InventoryItem::default()
    }
}

fn owned(id: u32, name: &str, value: i32, stack: i32) -> InventoryItem {
    owned_as(id, name, value, stack, ObjectClass::MeleeWeapon, 0x1)
}

fn note(id: u32, name: &str, value: i32, stack: i32) -> InventoryItem {
    owned_as(id, name, value, stack, ObjectClass::TradeNote, 0x1)
}

fn buy(item: VendorItem, wanted: i32, rule: &str) -> VendorBuyCandidate {
    VendorBuyCandidate {
        item,
        wanted,
        rule_name: rule.to_owned(),
    }
}

fn sell(item: InventoryItem, rule: &str) -> VendorSellCandidate {
    VendorSellCandidate {
        item,
        rule_name: rule.to_owned(),
    }
}

fn purse(coin_on_hand: i64, coin_stack_size: i32, free_slots: i32) -> VendorPurse {
    VendorPurse {
        coin_on_hand,
        coin_stack_size,
        free_slots,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: ABuyFitsTheCoinAtTheServersStackPrice
#[test]
fn a_buy_fits_the_coin_at_the_servers_stack_price() {
    let coin = 47_916;
    let count = planner::affordable_count(coin, 34, 1000);

    assert!(server_cost(count, 34.1, 1000) <= coin);
    assert!(count >= 1390, "bought only {count}; the margin is too wide");
    assert!(planner::worst_case_cost(34, count, 1000) <= coin);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: BuysCheapestFirstNotesLastWithinCoinAndSlots
#[test]
fn buys_cheapest_first_notes_last_within_coin_and_slots() {
    let wanted = vec![
        buy(
            listing(1, "Note", ObjectClass::TradeNote, 3, 1),
            10,
            "notes",
        ),
        buy(listing(2, "Ration", ObjectClass::Food, 5, 25), 40, "food"),
        buy(
            listing(3, "Taper", ObjectClass::SpellComponent, 2, 100),
            100,
            "comps",
        ),
    ];

    let plan = planner::plan(
        &planner::order_buys(wanted),
        &[],
        &three_quarters(),
        &purse(500, 25000, 6),
        &[],
    );

    // Tapers first (2 coin): 100 wanted, 100 affordable, 1 slot. Rations
    // next (5 coin): 300 coin left buys 60, capped to 40 wanted, 2 stacks.
    // Notes last although cheaper: notes do not stack and 10 slots is more
    // than the 6 free, so the count is cut to 1 * ((6 - 2) - 3) = 1.
    let lines: Vec<(u32, i32)> = plan
        .buys
        .iter()
        .map(|line| (line.template_object_id, line.count))
        .collect();
    assert_eq!(lines, [(3, 100), (2, 40), (1, 1)]);
    assert_eq!(plan.buy_cost, 200 + 200 + 3);
    assert!(plan.sells.is_empty());
    assert!(plan.fatal.is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: BuyIsCutToTheStacksTheSlotsCanHold
#[test]
fn buy_is_cut_to_the_stacks_the_slots_can_hold() {
    let wanted = vec![
        buy(
            listing(2, "Ration", ObjectClass::Food, 1, 10),
            i32::MAX,
            "food",
        ),
        buy(
            listing(3, "Taper", ObjectClass::SpellComponent, 1, 10),
            i32::MAX,
            "comps",
        ),
    ];

    let plan = planner::plan(
        &planner::order_buys(wanted),
        &[],
        &three_quarters(),
        &purse(1000, 25000, 4),
        &[],
    );

    // 1000 coin buys 1000 rations = 100 stacks; slots allow
    // 10 * ((4 - 2) - 0) = 20. The count was cut, so nothing follows.
    let lines: Vec<(u32, i32)> = plan
        .buys
        .iter()
        .map(|line| (line.template_object_id, line.count))
        .collect();
    assert_eq!(lines, [(2, 20)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: AListingWithoutAPriceIsFatal
#[test]
fn a_listing_without_a_price_is_fatal() {
    let plan = planner::plan(
        &[buy(
            listing(2, "Ration", ObjectClass::Food, 0, 10),
            5,
            "food",
        )],
        &[],
        &three_quarters(),
        &purse(100, 25000, 10),
        &[],
    );

    let fatal = plan.fatal.expect("a fatal reason");
    assert!(fatal.contains("Ration"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: PayoutFollowsTheVendorsRateNotesAtFace
#[test]
fn payout_follows_the_vendors_rate_notes_at_face() {
    let profile = three_quarters();
    assert_eq!(
        planner::payout_per_unit(&owned(10, "Dagger", 10, 1), &profile),
        7
    );
    assert_eq!(
        planner::payout_per_unit(&owned(11, "Pebble", 1, 1), &profile),
        1
    );
    assert_eq!(
        planner::payout_per_unit(&note(12, "Note", 100, 1), &profile),
        100
    );
    // Value is for the whole stack: 50 coin over 5 units is 10 a unit.
    assert_eq!(
        planner::payout_per_unit(&owned(13, "Arrow", 50, 5), &profile),
        7
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: VendorRefusesOutsideItsCategoriesAndLimitsButAlwaysTakesNotes
#[test]
fn vendor_refuses_outside_its_categories_and_limits_but_always_takes_notes() {
    let profile = three_quarters();
    let item = |id, name, value, item_type| {
        owned_as(id, name, value, 1, ObjectClass::MeleeWeapon, item_type)
    };
    assert!(planner::vendor_buys(&item(1, "Dagger", 10, 0x1), &profile));
    assert!(!planner::vendor_buys(&item(2, "Robe", 10, 0x4), &profile));
    assert!(!planner::vendor_buys(
        &item(3, "Crown", 5000, 0x1),
        &profile
    ));
    assert!(!planner::vendor_buys(&item(4, "Rock", 0, 0x1), &profile));
    assert!(planner::vendor_buys(
        &owned_as(5, "Note", 5000, 1, ObjectClass::TradeNote, 0x40000),
        &profile
    ));

    let floored = VendorProfile {
        minimum_value: 20,
        ..three_quarters()
    };
    assert!(!planner::vendor_buys(&item(6, "Dagger", 10, 0x1), &floored));
    assert!(planner::vendor_buys(&item(7, "Sword", 20, 0x1), &floored));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: SellsCheapestFirstAndNeverSellsANoteWithNothingToBuy
#[test]
fn sells_cheapest_first_and_never_sells_a_note_with_nothing_to_buy() {
    let profile = three_quarters();
    let sells = vec![
        sell(note(20, "Note", 100, 1), "notes"),
        sell(owned(21, "Sword", 40, 1), "junk"),
        sell(owned(22, "Arrow", 20, 10), "junk"),
    ];
    let items: Vec<InventoryItem> = sells
        .iter()
        .map(|candidate| candidate.item.clone())
        .collect();

    let plan = planner::plan(
        &[],
        &planner::order_sells(sells, &profile),
        &profile,
        &purse(0, 25000, 10),
        &items,
    );

    // Arrows fetch 1 a unit (20/10 * 0.75 = 1.5 -> 1) times 10 = 10; the
    // sword fetches 30; the note is not sold.
    let lines: Vec<(u32, i64)> = plan
        .sells
        .iter()
        .map(|line| (line.object_id, line.proceeds))
        .collect();
    assert_eq!(lines, [(22, 10), (21, 30)]);
    assert_eq!(plan.sell_proceeds, 40);
    assert!(plan.split.is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: SellsExactlyOneNoteWhenCoinIsNeededForABuy
#[test]
fn sells_exactly_one_note_when_coin_is_needed_for_a_buy() {
    let profile = three_quarters();
    let buys = || {
        planner::order_buys(vec![buy(
            listing(1, "Ration", ObjectClass::Food, 50, 25),
            10,
            "food",
        )])
    };
    let note_stack = note(20, "Note", 5000, 5);

    let split = planner::plan(
        &buys(),
        &planner::order_sells(vec![sell(note_stack.clone(), "notes")], &profile),
        &profile,
        &purse(10, 25000, 10),
        std::slice::from_ref(&note_stack),
    );
    assert!(split.buys.is_empty());
    assert!(split.sells.is_empty());
    assert_eq!(
        split.split,
        Some(VendorSplitRequest {
            object_id: 20,
            name: "Note".to_owned(),
            amount: 1
        })
    );

    let single = note(21, "Note", 1000, 1);
    let staged = planner::plan(
        &buys(),
        &planner::order_sells(
            vec![
                sell(note_stack.clone(), "notes"),
                sell(single.clone(), "notes"),
            ],
            &profile,
        ),
        &profile,
        &purse(10, 25000, 10),
        &[note_stack, single],
    );
    let lines: Vec<(u32, i64)> = staged
        .sells
        .iter()
        .map(|line| (line.object_id, line.proceeds))
        .collect();
    assert_eq!(lines, [(21, 1000)]);
    assert!(staged.split.is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: ANoteIsNotSoldToBuyANoteNorAheadOfOtherGoods
#[test]
fn a_note_is_not_sold_to_buy_a_note_nor_ahead_of_other_goods() {
    let profile = three_quarters();
    let note_item = note(20, "Note", 1000, 1);
    let sword = owned(21, "Sword", 40, 1);

    let note_for_note = planner::plan(
        &[buy(
            listing(1, "Big Note", ObjectClass::TradeNote, 5000, 1),
            1,
            "notes",
        )],
        &planner::order_sells(vec![sell(note_item.clone(), "n")], &profile),
        &profile,
        &purse(0, 25000, 10),
        std::slice::from_ref(&note_item),
    );
    assert!(note_for_note.sells.is_empty());
    assert!(note_for_note.split.is_none());

    let goods_first = planner::plan(
        &[buy(
            listing(1, "Ration", ObjectClass::Food, 50, 25),
            10,
            "food",
        )],
        &planner::order_sells(
            vec![sell(note_item.clone(), "n"), sell(sword.clone(), "s")],
            &profile,
        ),
        &profile,
        &purse(0, 25000, 10),
        &[note_item, sword],
    );
    let sold: Vec<u32> = goods_first
        .sells
        .iter()
        .map(|line| line.object_id)
        .collect();
    assert_eq!(sold, [21]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: SplitsAStackWhoseProceedsWouldNotFit
#[test]
fn splits_a_stack_whose_proceeds_would_not_fit() {
    let profile = three_quarters();
    // 1000 arrows at 100 coin each = 100,000 coin = 4 coin stacks.
    let arrows = owned(30, "Arrow", 100_000, 1000);
    let three_free = purse(0, 25000, 3);

    let plan = planner::plan(
        &[],
        &planner::order_sells(vec![sell(arrows.clone(), "junk")], &profile),
        &profile,
        &three_free,
        std::slice::from_ref(&arrows),
    );

    // Payout is 75 a unit. (3 - 2) * 25000 / 75 = 333 units.
    assert!(plan.sells.is_empty());
    assert_eq!(
        plan.split,
        Some(VendorSplitRequest {
            object_id: 30,
            name: "Arrow".to_owned(),
            amount: 333
        })
    );

    let pebble = owned(31, "Pebble", 10, 10);
    let second = planner::plan(
        &[],
        &planner::order_sells(
            vec![sell(pebble.clone(), "j"), sell(arrows.clone(), "j")],
            &profile,
        ),
        &profile,
        &three_free,
        &[pebble, arrows.clone()],
    );
    let sold: Vec<u32> = second.sells.iter().map(|line| line.object_id).collect();
    assert_eq!(sold, [31]);
    assert!(second.split.is_none());

    let no_room = planner::plan(
        &[],
        &planner::order_sells(vec![sell(arrows.clone(), "j")], &profile),
        &profile,
        &purse(0, 25000, 1),
        &[arrows],
    );
    assert!(no_room.fatal.is_some());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: SellIsCappedAtTheTransactionLimit
#[test]
fn sell_is_capped_at_the_transaction_limit() {
    let profile = three_quarters();
    let many: Vec<VendorSellCandidate> = (1..=150)
        .map(|i| sell(owned(i, "Pebble", 4, 1), "j"))
        .collect();
    let items: Vec<InventoryItem> = many
        .iter()
        .map(|candidate| candidate.item.clone())
        .collect();

    let plan = planner::plan(
        &[],
        &planner::order_sells(many, &profile),
        &profile,
        &purse(0, 25000, 10),
        &items,
    );

    assert_eq!(plan.sells.len(), MAXIMUM_SELL_LINES);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradePlannerTests.cs :: ARoundWithABuyDoesNotAlsoSell
#[test]
fn a_round_with_a_buy_does_not_also_sell() {
    let profile = three_quarters();
    let plan = planner::plan(
        &[buy(
            listing(1, "Ration", ObjectClass::Food, 5, 25),
            10,
            "food",
        )],
        &planner::order_sells(vec![sell(owned(21, "Sword", 40, 1), "j")], &profile),
        &profile,
        &purse(100, 25000, 10),
        &[],
    );

    assert_eq!(plan.buys.len(), 1);
    assert!(plan.sells.is_empty());
}
