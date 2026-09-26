using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class LootingTests
{
    [Theory]
    [InlineData(PluginInventoryCommandKind.Merge, 1u)]
    [InlineData(PluginInventoryCommandKind.Merge, 0u)]
    [InlineData(PluginInventoryCommandKind.Pickup, 1u)]
    [InlineData(PluginInventoryCommandKind.Unknown, 0u)]
    public void ExhaustedStackPickupAdvancesToRemainingLoot(
        PluginInventoryCommandKind completionKind, uint error)
    {
        const uint corpse = 0x70001600u, pea = 0x70001601u, next = 0x70001602u;
        LootSettings settings = ChainSettings();
        settings.CorpseLootItemMaxAttempts = 2;
        PluginInventoryItem first = Loose(pea, "Gold Pea", 77u, 0) with
        {
            MaximumStackSize = 100,
            StackSize = 1,
        };
        PluginInventoryItem second = Loose(next, "Next item", 78u, 1);
        PullStage stage = StageOpenCorpse(corpse, [first, second], settings,
            new Automation { Corpses = [ChainCorpse(corpse)] });
        for (int attempt = 0; attempt < 2; attempt++)
        {
            Assert.True(stage.Controller.Tick(0.3d, canAct: true));
            Assert.Equal(pea, stage.Automation.Picked[^1]);
            stage.Locks.Advance(1d);
            stage.Automation.ItemsBusy = true;
            Assert.True(stage.Controller.Tick(0.3d, canAct: true));
            Assert.Equal(attempt + 1, stage.Automation.Picked.Count);
            stage.Automation.ItemsBusy = false;
            stage.Automation.InventoryCompletion = completionKind == PluginInventoryCommandKind.Unknown
                ? default
                : new(attempt + 1, completionKind, pea, error);
        }

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([pea, pea, next], stage.Automation.Picked);
        stage.Automation.Contents = [first];
        stage.Automation.Owned = [second];
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.DoesNotContain(pea, stage.Controller.ClassifiedOwnedItems.Keys);
        Assert.Contains(next, stage.Controller.ClassifiedOwnedItems.Keys);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void UnrelatedOrOldMergeFailureDoesNotInterruptPendingPickup(bool old)
    {
        const uint corpse = 0x70001610u, pea = 0x70001611u;
        PullStage stage = StageOpenCorpse(corpse, Item(pea, "Gold Pea", 77u), ChainSettings());
        stage.Automation.InventoryCompletion = new(1, PluginInventoryCommandKind.Merge, pea, 1u);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        stage.Automation.ItemsBusy = true;
        stage.Locks.Advance(1d);
        stage.Automation.InventoryCompletion = new(old ? 1 : 2,
            PluginInventoryCommandKind.Merge, old ? pea : pea + 1, 1u);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([pea], stage.Automation.Picked);
    }
}
