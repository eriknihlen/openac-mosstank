using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class ManaStoneSupplyTests
{
    private static PluginInventoryItem Stone => new() { ObjectId = 10, Name = "Stone",
        ObjectClass = PluginObjectClass.ManaStone, ItemType = 0x80000 };
    private static PluginInventoryItem Donor => new() { ObjectId = 20, Name = "Donor",
        Workmanship = 5, ItemCurrentMana = 2000, Effects = 1 };

    [Fact]
    public void DonorMustRemainUntinkeredUnwornAndExplicitlyClassified()
    {
        var classes = new Dictionary<uint, LootAction> { [20] = LootAction.ManaTank };
        ManaStoneTransferPlan? Plan(PluginInventoryItem donor) =>
            ManaStoneTransferPlanner.Plan([Stone, donor], classes, 1000, ProfiledStone);
        Assert.NotNull(Plan(Donor)); // Value zero is allowed; workmanship is the criterion.
        Assert.Null(Plan(Donor with { Workmanship = 0, Value = 100 }));
        Assert.Null(Plan(Donor with { NumTimesTinkered = 1 }));
        Assert.Null(Plan(Donor with { EquippedLocation = 1 }));
        Assert.Null(Plan(Donor with { WielderObjectId = 1 }));
        Assert.Null(Plan(Donor with { Effects = 0 }));
        // The floor is exact: one point short is short.
        Assert.Null(Plan(Donor with { ItemCurrentMana = 999 }));
        Assert.NotNull(Plan(Donor with { ItemCurrentMana = 1000 }));
        Assert.NotNull(Plan(Donor with { ItemCurrentMana = 1001 }));
        classes[20] = LootAction.Keep;
        Assert.Null(Plan(Donor));
    }

    /// <summary>
    /// Nobody's writing gets emptied into a stone. Either half of an
    /// inscription -- the text or the name of whoever wrote it -- says the
    /// item was meant to be kept.
    ///
    /// Mutation: drop either key from <c>IsUninscribed</c> and its case here
    /// becomes a drainable donor.
    /// </summary>
    [Theory]
    [InlineData(7u)]
    [InlineData(8u)]
    public void AnInscribedDonorIsNeverDrained(uint written)
    {
        var classes = new Dictionary<uint, LootAction> { [20] = LootAction.ManaTank };
        PluginItemProperties? Written(uint objectId) => Text(
            objectId == 20 ? new Dictionary<uint, string> { [written] = "Erik" }
                : new Dictionary<uint, string>());

        Assert.NotNull(ManaStoneTransferPlanner.Plan(
            [Stone, Donor], classes, 1000, ProfiledStone, null, null,
            static _ => Text(new Dictionary<uint, string>())));
        Assert.Null(ManaStoneTransferPlanner.Plan(
            [Stone, Donor], classes, 1000, ProfiledStone, null, null, Written));
        // Properties the client cannot produce are not a licence to drain.
        Assert.Null(ManaStoneTransferPlanner.Plan(
            [Stone, Donor], classes, 1000, ProfiledStone, null, null,
            static _ => null));
    }

    private static PluginItemProperties Text(
        IReadOnlyDictionary<uint, string> strings) => new(
            new Dictionary<uint, int>(),
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            strings,
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());

    [Fact]
    public void ChargedUnconfiguredOrHeldSourcesCannotDrainDonors()
    {
        var classes = new Dictionary<uint, LootAction> { [20] = LootAction.ManaTank };
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone with { Effects = 1 }, Donor], classes, 1000, ProfiledStone));
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone, Donor], classes, 1000, static _ => false));
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone, Donor], classes, 1000, ProfiledStone, id => id != 10));
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone, Donor], classes, 1000, ProfiledStone, null, id => id != 20));
    }

    /// <summary>
    /// A stone the profile's helper list names as one: the client's own class
    /// AND the profile's kind, which is what the macro reads before it treats
    /// an item as a fillable stone.
    /// </summary>
    private static bool ProfiledStone(PluginInventoryItem item) =>
        item.ObjectClass == PluginObjectClass.ManaStone
        && item.Name == "Stone";

    [Fact]
    public void SuccessfulReplyWaitsForInventoryAndChargeAndMatchesBothIds()
    {
        var plan = new ManaStoneTransferPlan(10, 20, "Stone", "Donor");
        var reply = new PluginItemUseCompletion(2, 10, 20, 0);
        ManaFillOutcome Observe(PluginInventoryItem[] items, PluginItemUseCompletion completion, bool timeout = false) =>
            ManaStoneFillConfirmation.Observe(plan, items, completion, 1, timeout);
        Assert.Equal(ManaFillOutcome.Waiting, Observe([Stone, Donor], reply));
        Assert.Equal(ManaFillOutcome.Waiting, Observe([Stone], reply));
        var charged = Stone with { Effects = 1 };
        Assert.Equal(ManaFillOutcome.Waiting, Observe([charged], reply with { TargetObjectId = 99 }));
        Assert.Equal(ManaFillOutcome.Waiting, Observe([charged], reply with { SourceObjectId = 99 }));
        Assert.Equal(ManaFillOutcome.Waiting, Observe([charged], reply with { Revision = 1 }));
        Assert.Equal(ManaFillOutcome.Confirmed, Observe([charged], reply));
        Assert.Equal(ManaFillOutcome.Confirmed, Observe([], reply));
        Assert.Equal(ManaFillOutcome.Uncertain, Observe([Stone, Donor], reply with { WeenieError = 1 }));
        Assert.Equal(ManaFillOutcome.Uncertain, Observe([Stone, Donor], default, true));
    }
}
