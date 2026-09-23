using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>One of the vendor's listings a rule wants, and how many.</summary>
/// <param name="Item">The listing.</param>
/// <param name="Wanted">
/// How many more the rule wants, or <see cref="int.MaxValue"/> for a rule
/// that keeps every one.
/// </param>
/// <param name="RuleName">The rule that wanted it, for the report.</param>
internal readonly record struct VendorBuyCandidate(
    PluginVendorItem Item,
    int Wanted,
    string RuleName);

/// <summary>One owned item a rule says to sell.</summary>
internal readonly record struct VendorSellCandidate(
    PluginInventoryItem Item,
    string RuleName);

/// <summary>One line of a buy: a listing, a count, and what it costs.</summary>
internal readonly record struct VendorBuyLine(
    uint TemplateObjectId,
    string Name,
    int Count,
    long Cost);

/// <summary>One line of a sell: an owned stack and what the vendor pays for it.</summary>
internal readonly record struct VendorSellLine(
    uint ObjectId,
    string Name,
    int StackSize,
    long Proceeds);

/// <summary>
/// A stack to split before the next round, because only part of it can be
/// sold this visit.
/// </summary>
internal readonly record struct VendorSplitRequest(
    uint ObjectId,
    string Name,
    int Amount);

/// <summary>
/// What the character has to shop with: coin, how much of it one pack slot
/// holds, and the slots free to take coin or goods.
/// </summary>
internal readonly record struct VendorPurse(
    long CoinOnHand,
    int CoinStackSize,
    int FreeSlots);

/// <summary>
/// One round of a vendor visit. A round either buys or sells, never both:
/// buying first keeps the goods the rules said to sell available as the
/// coin that buys with, and the sell round after it sees the slots the buy
/// took.
/// </summary>
internal sealed record VendorTradePlan(
    IReadOnlyList<VendorBuyLine> Buys,
    long BuyCost,
    IReadOnlyList<VendorSellLine> Sells,
    long SellProceeds,
    VendorSplitRequest? Split,
    string? Fatal)
{
    /// <summary>
    /// How much more than <see cref="BuyCost"/> the server may charge: it
    /// prices each stack as a whole and rounds once, where the listing rounds
    /// the single item.
    /// </summary>
    public long BuySlack { get; init; }

    public static VendorTradePlan Empty { get; } = new([], 0L, [], 0L, null, null);

    public bool IsEmpty =>
        Buys.Count == 0 && Sells.Count == 0 && Split is null && Fatal is null;
}

/// <summary>
/// The money and slot planner. It reads the vendor's terms -- what it pays,
/// what it deals in, its value limits -- against the coin and slots on hand
/// and decides what one round can buy or sell without overfilling the pack
/// or overspending the purse.
/// </summary>
internal static class VendorTradePlanner
{
    /// <summary>The most of one listing bought in a single round.</summary>
    public const int MaximumBuyCount = 5000;

    /// <summary>The most items one sell transaction carries.</summary>
    public const int MaximumSellLines = 99;

    /// <summary>
    /// How much coin one pack slot holds when the character carries none to
    /// read it from.
    /// </summary>
    public const int DefaultCoinStackSize = 25000;

    /// <summary>
    /// Pack slots kept free on top of what a buy or a sell needs, so the
    /// coin change and the odd extra stack always have somewhere to go.
    /// </summary>
    private const int SpareSlots = 2;

    /// <summary>
    /// The most <paramref name="count"/> items listed at <paramref name="unitPrice"/>
    /// can cost. The listing rounds one item's price to the nearest pyreal
    /// (up, from a tenth over); the server prices each stack it creates as a
    /// whole and rounds once, so a stack can cost up to a tenth of a pyreal
    /// more per item, plus one for the rounding of each stack.
    /// </summary>
    internal static long WorstCaseCost(long unitPrice, int count, int maxStack) =>
        unitPrice * count + (count + 9) / 10 + StacksFor(count, Math.Max(1, maxStack));

    /// <summary>The most items that can be bought with <paramref name="coin"/> in the worst case.</summary>
    internal static int AffordableCount(long coin, long unitPrice, int maxStack)
    {
        if (coin <= 0L || unitPrice <= 0L)
            return 0;
        long guess = Math.Min(int.MaxValue, coin * 10L / (unitPrice * 10L + 1L));
        int count = (int)guess;
        while (count > 0 && WorstCaseCost(unitPrice, count, maxStack) > coin)
            count--;
        while (count < int.MaxValue && WorstCaseCost(unitPrice, count + 1, maxStack) <= coin)
            count++;
        return count;
    }

    /// <summary>The value of one unit: the stack's value over its size.</summary>
    public static long UnitValue(in PluginInventoryItem item) =>
        item.Value / Math.Max(1, item.StackSize);

    /// <summary>
    /// What the vendor pays for one unit: its rate on the unit value, rounded
    /// down and never below one coin, or the face value for a trade note.
    /// </summary>
    public static long PayoutPerUnit(
        in PluginInventoryItem item,
        in PluginVendorProfile profile)
    {
        long unit = UnitValue(item);
        if (item.ObjectClass == PluginObjectClass.TradeNote)
            return unit;
        if (unit <= 0L)
            return 0L;
        long paid = (long)Math.Floor(unit * (double)profile.BuyRate);
        return Math.Max(1L, paid);
    }

    /// <summary>
    /// Whether this vendor takes the item at all: a trade note always, and
    /// anything else only in a category it deals in, worth something, and
    /// within its per-unit value limits.
    /// </summary>
    public static bool VendorBuys(
        in PluginInventoryItem item,
        in PluginVendorProfile profile)
    {
        if (item.ObjectClass == PluginObjectClass.TradeNote)
            return true;
        if ((profile.DealsInItemTypes & item.ItemType) == 0u)
            return false;
        long unit = UnitValue(item);
        if (unit <= 0L)
            return false;
        if (profile.MinimumValue != PluginVendorProfile.NoValueLimit
            && unit < profile.MinimumValue)
        {
            return false;
        }
        return profile.MaximumValue == PluginVendorProfile.NoValueLimit
            || unit <= profile.MaximumValue;
    }

    /// <summary>Cheapest first, trade notes last.</summary>
    public static IReadOnlyList<VendorBuyCandidate> OrderBuys(
        IEnumerable<VendorBuyCandidate> candidates) => candidates
            .OrderBy(static c => c.Item.ObjectClass == PluginObjectClass.TradeNote ? 1 : 0)
            .ThenBy(static c => c.Item.UnitPrice)
            .ToArray();

    /// <summary>Lowest proceeds first, trade notes last.</summary>
    public static IReadOnlyList<VendorSellCandidate> OrderSells(
        IEnumerable<VendorSellCandidate> candidates,
        PluginVendorProfile profile) => candidates
            .OrderBy(static c => c.Item.ObjectClass == PluginObjectClass.TradeNote ? 1 : 0)
            .ThenBy(c => PayoutPerUnit(c.Item, profile) * Math.Max(1, c.Item.StackSize))
            .ToArray();

    /// <summary>
    /// Plans one round. Buying wins the round when anything can be bought;
    /// otherwise the round sells, and sells a single trade note only when a
    /// buy is waiting on the coin.
    /// </summary>
    /// <param name="buys">The listings the rules want, already ordered.</param>
    /// <param name="sells">The owned items the rules say to sell, already ordered.</param>
    /// <param name="profile">The vendor's terms.</param>
    /// <param name="purse">Coin and slots on hand.</param>
    /// <param name="owned">
    /// Everything the character carries, searched for a lone trade note when
    /// one has to be sold.
    /// </param>
    public static VendorTradePlan Plan(
        IReadOnlyList<VendorBuyCandidate> buys,
        IReadOnlyList<VendorSellCandidate> sells,
        in PluginVendorProfile profile,
        in VendorPurse purse,
        IReadOnlyList<PluginInventoryItem> owned)
    {
        ArgumentNullException.ThrowIfNull(buys);
        ArgumentNullException.ThrowIfNull(sells);
        ArgumentNullException.ThrowIfNull(owned);

        var buyLines = new List<VendorBuyLine>();
        long buyCost = 0L;
        long buySlack = 0L;
        int buySlots = 0;
        foreach (VendorBuyCandidate candidate in buys)
        {
            PluginVendorItem item = candidate.Item;
            long price = item.UnitPrice;
            if (price <= 0L)
            {
                return new VendorTradePlan(
                    [], 0L, [], 0L, null,
                    $"No vendor price found for {item.Name}; nothing bought.");
            }
            int maxStack = Math.Max(1, item.MaxStackSize);
            bool note = item.ObjectClass == PluginObjectClass.TradeNote;
            if (WorstCaseCost(price, 1, maxStack) <= purse.CoinOnHand - buyCost - buySlack
                && purse.FreeSlots - buySlots > (note ? 0 : 1))
            {
                long affordable = AffordableCount(
                    purse.CoinOnHand - buyCost - buySlack, price, maxStack);
                int count = (int)Math.Min(affordable, candidate.Wanted);
                count = Math.Min(count, MaximumBuyCount);
                int stacks = StacksFor(count, maxStack);
                if (purse.FreeSlots < stacks + buySlots)
                    count = maxStack * ((purse.FreeSlots - SpareSlots) - buySlots);
                if (count <= 0)
                    break;

                buyLines.Add(new VendorBuyLine(
                    item.TemplateObjectId, item.Name, count, price * count));
                buySlots += StacksFor(count, maxStack);
                buyCost += price * count;
                buySlack += WorstCaseCost(price, count, maxStack) - price * count;
                if (candidate.Wanted > count)
                    break;
            }
            else if (buyLines.Count > 0)
            {
                break;
            }
        }
        if (buyLines.Count > 0)
            return new VendorTradePlan(buyLines, buyCost, [], 0L, null, null) { BuySlack = buySlack };

        PluginVendorItem? nextBuy = buys.Count > 0 ? buys[0].Item : null;
        bool nextBuyIsNote = nextBuy?.ObjectClass == PluginObjectClass.TradeNote;
        var sellLines = new List<VendorSellLine>();
        long proceeds = 0L;
        foreach (VendorSellCandidate candidate in sells)
        {
            if (sellLines.Count >= MaximumSellLines)
                break;
            PluginInventoryItem item = candidate.Item;
            long value = PayoutPerUnit(item, profile);
            int stack = Math.Max(1, item.StackSize);

            if (item.ObjectClass == PluginObjectClass.TradeNote)
            {
                // A note is coin in another shape: it is sold only to fund a
                // buy of something that is not itself a note, one at a time,
                // and only once the ordinary goods have gone first.
                if (nextBuy is null || nextBuyIsNote || sellLines.Count > 0)
                    break;
                if (!CoinFits(value, purse))
                {
                    return new VendorTradePlan(
                        [], 0L, [], 0L, null,
                        $"No inventory room to sell {item.Name}.");
                }
                PluginInventoryItem lone = owned.FirstOrDefault(other =>
                    other.ObjectClass == PluginObjectClass.TradeNote
                    && Math.Max(1, other.StackSize) == 1
                    && string.Equals(other.Name, item.Name, StringComparison.OrdinalIgnoreCase));
                if (lone.ObjectId != 0u)
                {
                    sellLines.Add(new VendorSellLine(
                        lone.ObjectId, lone.Name, 1, PayoutPerUnit(lone, profile)));
                    proceeds += PayoutPerUnit(lone, profile);
                    break;
                }
                return new VendorTradePlan(
                    [], 0L, [], 0L, new VendorSplitRequest(item.ObjectId, item.Name, 1), null);
            }

            if (!CoinFits(proceeds + value * stack, purse))
            {
                if (sellLines.Count > 0)
                    break;
                long amount = value > 0L
                    ? (long)(purse.FreeSlots - SpareSlots) * purse.CoinStackSize / value
                    : 0L;
                amount = Math.Min(amount, stack - 1);
                if (amount > 0L)
                {
                    return new VendorTradePlan(
                        [], 0L, [], 0L,
                        new VendorSplitRequest(item.ObjectId, item.Name, (int)amount),
                        null);
                }
                return new VendorTradePlan(
                    [], 0L, [], 0L, null,
                    $"No inventory room to sell {item.Name}.");
            }

            sellLines.Add(new VendorSellLine(item.ObjectId, item.Name, stack, value * stack));
            proceeds += value * stack;
        }

        return sellLines.Count > 0
            ? new VendorTradePlan([], 0L, sellLines, proceeds, null, null)
            : VendorTradePlan.Empty;
    }

    private static int StacksFor(int count, int maxStack) =>
        (int)Math.Ceiling(count / (double)maxStack);

    /// <summary>
    /// Whether that much coin fits in the pack with one slot still free:
    /// coin arrives as stacks of its own and needs slots like anything else.
    /// </summary>
    private static bool CoinFits(long coin, in VendorPurse purse)
    {
        int stackSize = Math.Max(1, purse.CoinStackSize);
        long stacks = (coin + stackSize - 1) / stackSize;
        return purse.FreeSlots - stacks > 0L;
    }
}
