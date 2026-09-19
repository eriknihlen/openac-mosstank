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
        Assert.Null(Plan(Donor with { PublicFlags = 0x01000000 }));
        Assert.Null(Plan(Donor with { ItemCurrentMana = 999 }));
        Assert.Null(Plan(Donor with { Effects = 0 }));
        Assert.Null(Plan(Donor with { ObjectClass = PluginObjectClass.ManaStone }));
        classes[20] = LootAction.Keep;
        Assert.Null(Plan(Donor));
    }

    [Fact]
    public void ChargedUnconfiguredOrHeldSourcesCannotDrainDonors()
    {
        var classes = new Dictionary<uint, LootAction> { [20] = LootAction.ManaTank };
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone with { Effects = 1 }, Donor], classes, 1000, ProfiledStone));
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone, Donor], classes, 1000, static _ => false));
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone, Donor], classes, 1000, ProfiledStone, id => id != 10));
        Assert.Null(ManaStoneTransferPlanner.Plan([Stone, Donor], classes, 1000, ProfiledStone, id => id != 20));
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
