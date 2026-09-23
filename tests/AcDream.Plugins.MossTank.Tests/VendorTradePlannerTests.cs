using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The money and slot planner behind a vendor run: what to buy with the coin
/// and slots on hand, what the vendor pays for what the rules say to sell,
/// and when a stack has to be split because the coin it fetches would not
/// fit. Every number here is worked by hand from the vendor's terms.
/// </summary>
public sealed class VendorTradePlannerTests
{
    /// <summary>
    /// Seen live: 1409 Prismatic Tapers listed at 34 were planned for 47,906
    /// with 47,916 coin, and the server refused. A taper is worth 22 at this
    /// vendor's markup of about 1.55: 34.1 a taper, listed as 34, but the
    /// server prices each stack of up to 1000 as a whole (34,100 for 1000).
    /// The count bought must fit the coin at the dearest price the listing
    /// allows. Mutation: coin / listed price buys 1409, which costs 48,047.
    /// </summary>
    [Fact]
    public void ABuyFitsTheCoinAtTheServersStackPrice()
    {
        const long coin = 47_916;
        int count = VendorTradePlanner.AffordableCount(coin, unitPrice: 34, maxStack: 1000);

        Assert.True(ServerCost(count, perItem: 34.1d, maxStack: 1000) <= coin);
        Assert.True(count >= 1390, $"bought only {count}; the margin is too wide");
        Assert.True(VendorTradePlanner.WorstCaseCost(34, count, 1000) <= coin);
    }

    // The server's charge: each stack priced whole, rounded up from a tenth over.
    private static long ServerCost(int count, double perItem, int maxStack)
    {
        long total = 0;
        for (int left = count; left > 0; left -= maxStack)
        {
            int stack = Math.Min(left, maxStack);
            total += (long)Math.Ceiling(perItem * stack - 0.1d);
        }
        return total;
    }

    private static readonly PluginVendorProfile ThreeQuarters = new(
        0.75f,
        DealsInItemTypes: 0x0000_0001u | 0x0000_0020u,
        MinimumValue: PluginVendorProfile.NoValueLimit,
        MaximumValue: 1000u,
        DealsInMagicalItems: true,
        AlternateCurrencyWeenieClassId: 0u,
        AlternateCurrencyAmount: 0u,
        AlternateCurrencyName: null!);

    /// <summary>
    /// Buying is cheapest first with trade notes last, and each line is
    /// capped three ways: by the coin left after the lines before it, by
    /// the count the rule still wants, and by the stacks the free slots can
    /// hold with two kept spare. Mutation: dropping the note-last rule puts
    /// the 3-coin note ahead of the 5-coin food and buys three of them.
    /// </summary>
    [Fact]
    public void BuysCheapestFirstNotesLastWithinCoinAndSlots()
    {
        VendorBuyCandidate[] wanted =
        [
            new(Listing(1u, "Note", PluginObjectClass.TradeNote, 3, maxStack: 1), 10, "notes"),
            new(Listing(2u, "Ration", PluginObjectClass.Food, 5, maxStack: 25), 40, "food"),
            new(Listing(3u, "Taper", PluginObjectClass.SpellComponent, 2, maxStack: 100), 100, "comps"),
        ];

        VendorTradePlan plan = VendorTradePlanner.Plan(
            VendorTradePlanner.OrderBuys(wanted),
            [],
            ThreeQuarters,
            new VendorPurse(CoinOnHand: 500, CoinStackSize: 25000, FreeSlots: 6),
            []);

        // Tapers first (2 coin): 100 wanted, 100 affordable, 1 slot.
        // Rations next (5 coin): 300 coin left buys 60, capped to 40 wanted,
        // 2 stacks of 25 -> 2 slots. Notes last although cheaper (3 coin):
        // 100 coin left buys 33, capped to 10 wanted, but notes do not
        // stack and 10 slots is more than the 6 free, so the count is cut
        // to what fits with two spare: 1 * ((6 - 2) - 3) = 1.
        Assert.Equal(
            [(3u, 100), (2u, 40), (1u, 1)],
            plan.Buys.Select(line => (line.TemplateObjectId, line.Count)).ToArray());
        Assert.Equal(200 + 200 + 3, plan.BuyCost);
        Assert.Empty(plan.Sells);
        Assert.Null(plan.Fatal);
    }

    /// <summary>
    /// A buy that runs out of slots is cut to the stacks that fit with two
    /// slots kept spare, and the plan stops there. Mutation: keeping one
    /// slot spare instead of two lets a third stack in.
    /// </summary>
    [Fact]
    public void BuyIsCutToTheStacksTheSlotsCanHold()
    {
        VendorBuyCandidate[] wanted =
        [
            new(Listing(2u, "Ration", PluginObjectClass.Food, 1, maxStack: 10), int.MaxValue, "food"),
            new(Listing(3u, "Taper", PluginObjectClass.SpellComponent, 1, maxStack: 10), int.MaxValue, "comps"),
        ];

        VendorTradePlan plan = VendorTradePlanner.Plan(
            VendorTradePlanner.OrderBuys(wanted),
            [],
            ThreeQuarters,
            new VendorPurse(CoinOnHand: 1000, CoinStackSize: 25000, FreeSlots: 4),
            []);

        // 1000 coin buys 1000 rations = 100 stacks; slots allow
        // 10 * ((4 - 2) - 0) = 20. The count was cut, so nothing follows.
        Assert.Equal([(2u, 20)], plan.Buys.Select(l => (l.TemplateObjectId, l.Count)).ToArray());
    }

    /// <summary>
    /// A listing without a price cannot be planned against, and the run says
    /// so instead of buying nothing quietly.
    /// </summary>
    [Fact]
    public void AListingWithoutAPriceIsFatal()
    {
        VendorTradePlan plan = VendorTradePlanner.Plan(
            [new(Listing(2u, "Ration", PluginObjectClass.Food, 0, maxStack: 10), 5, "food")],
            [],
            ThreeQuarters,
            new VendorPurse(100, 25000, 10),
            []);

        Assert.NotNull(plan.Fatal);
        Assert.Contains("Ration", plan.Fatal, StringComparison.Ordinal);
    }

    /// <summary>
    /// The vendor pays its rate on an item's per-unit value, rounded down and
    /// never below one coin; a trade note is paid at face value whatever the
    /// rate. Mutation: rounding to nearest pays 8 for a 10-coin item at
    /// 0.75.
    /// </summary>
    [Fact]
    public void PayoutFollowsTheVendorsRateNotesAtFace()
    {
        Assert.Equal(7, VendorTradePlanner.PayoutPerUnit(Owned(10u, "Dagger", 10, 1), ThreeQuarters));
        Assert.Equal(1, VendorTradePlanner.PayoutPerUnit(Owned(11u, "Pebble", 1, 1), ThreeQuarters));
        Assert.Equal(
            100,
            VendorTradePlanner.PayoutPerUnit(
                Owned(12u, "Note", 100, 1, PluginObjectClass.TradeNote),
                ThreeQuarters));
        // Value is for the whole stack: 50 coin over 5 units is 10 a unit.
        Assert.Equal(7, VendorTradePlanner.PayoutPerUnit(Owned(13u, "Arrow", 50, 5), ThreeQuarters));
    }

    /// <summary>
    /// What the vendor refuses: a category outside what it deals in, a unit
    /// value past its ceiling, a worthless item. A note passes every gate,
    /// including the ceiling. Mutation: applying the ceiling to notes drops
    /// the one item the vendor would have paid most for.
    /// </summary>
    [Fact]
    public void VendorRefusesOutsideItsCategoriesAndLimitsButAlwaysTakesNotes()
    {
        Assert.True(VendorTradePlanner.VendorBuys(Owned(1u, "Dagger", 10, 1, itemType: 0x1u), ThreeQuarters));
        Assert.False(VendorTradePlanner.VendorBuys(Owned(2u, "Robe", 10, 1, itemType: 0x4u), ThreeQuarters));
        Assert.False(VendorTradePlanner.VendorBuys(Owned(3u, "Crown", 5000, 1, itemType: 0x1u), ThreeQuarters));
        Assert.False(VendorTradePlanner.VendorBuys(Owned(4u, "Rock", 0, 1, itemType: 0x1u), ThreeQuarters));
        Assert.True(VendorTradePlanner.VendorBuys(
            Owned(5u, "Note", 5000, 1, PluginObjectClass.TradeNote, itemType: 0x40000u),
            ThreeQuarters));

        PluginVendorProfile floored = ThreeQuarters with { MinimumValue = 20u };
        Assert.False(VendorTradePlanner.VendorBuys(Owned(6u, "Dagger", 10, 1, itemType: 0x1u), floored));
        Assert.True(VendorTradePlanner.VendorBuys(Owned(7u, "Sword", 20, 1, itemType: 0x1u), floored));
    }

    /// <summary>
    /// Selling is cheapest proceeds first with notes last, and is capped at
    /// the transaction limit. With nothing to buy, no note is ever sold.
    /// </summary>
    [Fact]
    public void SellsCheapestFirstAndNeverSellsANoteWithNothingToBuy()
    {
        VendorSellCandidate[] sells =
        [
            new(Owned(20u, "Note", 100, 1, PluginObjectClass.TradeNote), "notes"),
            new(Owned(21u, "Sword", 40, 1), "junk"),
            new(Owned(22u, "Arrow", 20, 10), "junk"),
        ];

        VendorTradePlan plan = VendorTradePlanner.Plan(
            [],
            VendorTradePlanner.OrderSells(sells, ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(0, 25000, 10),
            [.. sells.Select(s => s.Item)]);

        // Arrows fetch 1 a unit (20/10 * 0.75 = 1.5 -> 1) times 10 = 10;
        // the sword fetches 30; the note is not sold.
        Assert.Equal(
            [(22u, 10L), (21u, 30L)],
            plan.Sells.Select(line => (line.ObjectId, line.Proceeds)).ToArray());
        Assert.Equal(40, plan.SellProceeds);
        Assert.Null(plan.Split);
    }

    /// <summary>
    /// When a buy is wanted but unaffordable, exactly one note is sold to
    /// fund it: a lone note in the packs is staged as it is, otherwise one
    /// is split off the stack first. Mutation: staging the whole stack sells
    /// every note the character has.
    /// </summary>
    [Fact]
    public void SellsExactlyOneNoteWhenCoinIsNeededForABuy()
    {
        VendorBuyCandidate[] buys =
        [
            new(Listing(1u, "Ration", PluginObjectClass.Food, 50, maxStack: 25), 10, "food"),
        ];
        PluginInventoryItem noteStack = Owned(20u, "Note", 5000, 5, PluginObjectClass.TradeNote);

        VendorTradePlan split = VendorTradePlanner.Plan(
            VendorTradePlanner.OrderBuys(buys),
            VendorTradePlanner.OrderSells([new(noteStack, "notes")], ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(CoinOnHand: 10, CoinStackSize: 25000, FreeSlots: 10),
            [noteStack]);
        Assert.Empty(split.Buys);
        Assert.Empty(split.Sells);
        Assert.Equal(new VendorSplitRequest(20u, "Note", 1), split.Split);

        PluginInventoryItem single = Owned(21u, "Note", 1000, 1, PluginObjectClass.TradeNote);
        VendorTradePlan staged = VendorTradePlanner.Plan(
            VendorTradePlanner.OrderBuys(buys),
            VendorTradePlanner.OrderSells(
                [new(noteStack, "notes"), new(single, "notes")],
                ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(10, 25000, 10),
            [noteStack, single]);
        Assert.Equal([(21u, 1000L)], staged.Sells.Select(l => (l.ObjectId, l.Proceeds)).ToArray());
        Assert.Null(staged.Split);
    }

    /// <summary>
    /// A note is never sold to fund another note, and no note is sold ahead
    /// of ordinary goods that can go first.
    /// </summary>
    [Fact]
    public void ANoteIsNotSoldToBuyANoteNorAheadOfOtherGoods()
    {
        PluginInventoryItem note = Owned(20u, "Note", 1000, 1, PluginObjectClass.TradeNote);
        PluginInventoryItem sword = Owned(21u, "Sword", 40, 1);

        VendorTradePlan noteForNote = VendorTradePlanner.Plan(
            [new(Listing(1u, "Big Note", PluginObjectClass.TradeNote, 5000, 1), 1, "notes")],
            VendorTradePlanner.OrderSells([new(note, "n")], ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(0, 25000, 10),
            [note]);
        Assert.Empty(noteForNote.Sells);
        Assert.Null(noteForNote.Split);

        VendorTradePlan goodsFirst = VendorTradePlanner.Plan(
            [new(Listing(1u, "Ration", PluginObjectClass.Food, 50, 25), 10, "food")],
            VendorTradePlanner.OrderSells([new(note, "n"), new(sword, "s")], ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(0, 25000, 10),
            [note, sword]);
        Assert.Equal([21u], goodsFirst.Sells.Select(l => l.ObjectId).ToArray());
    }

    /// <summary>
    /// Coin takes pack slots too. A stack whose proceeds would overflow the
    /// free slots is split down to what fits with two slots spare; a later
    /// stack that would not fit simply waits for the next round. Mutation:
    /// forgetting the two spare slots asks for 100 units, which fills the
    /// pack to the last slot.
    /// </summary>
    [Fact]
    public void SplitsAStackWhoseProceedsWouldNotFit()
    {
        // 1000 arrows at 100 coin each = 100,000 coin = 4 coin stacks.
        PluginInventoryItem arrows = Owned(30u, "Arrow", 100_000, 1000, itemType: 0x1u);
        var purse = new VendorPurse(CoinOnHand: 0, CoinStackSize: 25000, FreeSlots: 3);

        VendorTradePlan plan = VendorTradePlanner.Plan(
            [],
            VendorTradePlanner.OrderSells([new(arrows, "junk")], ThreeQuarters),
            ThreeQuarters,
            purse,
            [arrows]);

        // Payout is 75 a unit. (3 - 2) * 25000 / 75 = 333 units.
        Assert.Empty(plan.Sells);
        Assert.Equal(new VendorSplitRequest(30u, "Arrow", 333), plan.Split);

        PluginInventoryItem pebble = Owned(31u, "Pebble", 10, 10, itemType: 0x1u);
        VendorTradePlan second = VendorTradePlanner.Plan(
            [],
            VendorTradePlanner.OrderSells([new(pebble, "j"), new(arrows, "j")], ThreeQuarters),
            ThreeQuarters,
            purse,
            [pebble, arrows]);
        Assert.Equal([31u], second.Sells.Select(l => l.ObjectId).ToArray());
        Assert.Null(second.Split);

        VendorTradePlan noRoom = VendorTradePlanner.Plan(
            [],
            VendorTradePlanner.OrderSells([new(arrows, "j")], ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(0, 25000, 1),
            [arrows]);
        Assert.NotNull(noRoom.Fatal);
    }

    /// <summary>
    /// The transaction limit: at most 99 sell lines in one round.
    /// </summary>
    [Fact]
    public void SellIsCappedAtTheTransactionLimit()
    {
        VendorSellCandidate[] many = [.. Enumerable.Range(1, 150)
            .Select(i => new VendorSellCandidate(Owned((uint)i, "Pebble", 4, 1), "j"))];

        VendorTradePlan plan = VendorTradePlanner.Plan(
            [],
            VendorTradePlanner.OrderSells(many, ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(0, 25000, 10),
            [.. many.Select(m => m.Item)]);

        Assert.Equal(VendorTradePlanner.MaximumSellLines, plan.Sells.Count);
    }

    /// <summary>
    /// A round that has anything to buy buys and does not sell; selling is
    /// the round after. Mutation: planning both in one round sells the goods
    /// the buy needed the slots for.
    /// </summary>
    [Fact]
    public void ARoundWithABuyDoesNotAlsoSell()
    {
        VendorTradePlan plan = VendorTradePlanner.Plan(
            [new(Listing(1u, "Ration", PluginObjectClass.Food, 5, 25), 10, "food")],
            VendorTradePlanner.OrderSells([new(Owned(21u, "Sword", 40, 1), "j")], ThreeQuarters),
            ThreeQuarters,
            new VendorPurse(100, 25000, 10),
            []);

        Assert.Single(plan.Buys);
        Assert.Empty(plan.Sells);
    }

    internal static PluginVendorItem Listing(
        uint id,
        string name,
        PluginObjectClass objectClass,
        int unitPrice,
        int maxStack) =>
        new(id, id + 1000u, name, objectClass, unitPrice, 1)
        {
            MaxStackSize = maxStack,
            ItemType = 0x1u,
        };

    internal static PluginInventoryItem Owned(
        uint id,
        string name,
        int value,
        int stack,
        PluginObjectClass objectClass = PluginObjectClass.MeleeWeapon,
        uint itemType = 0x1u) =>
        new(id, id + 2000u, name, itemType, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            stack, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = value,
            MaximumStackSize = Math.Max(1, stack),
            ObjectClass = objectClass,
        };
}
