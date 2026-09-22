using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum ManaFillOutcome { Waiting, Confirmed, Uncertain }

internal static class ManaStoneFillConfirmation
{
    public static ManaFillOutcome Observe(
        ManaStoneTransferPlan plan,
        IReadOnlyList<PluginInventoryItem> owned,
        PluginItemUseCompletion completion,
        long previousRevision,
        bool timedOut)
    {
        bool matches = completion.Revision > previousRevision
            && completion.SourceObjectId == plan.StoneObjectId
            && completion.TargetObjectId == plan.TankObjectId;
        if (matches && !completion.IsSuccess)
            return ManaFillOutcome.Uncertain;
        bool donorGone = !owned.Any(item => item.ObjectId == plan.TankObjectId);
        PluginInventoryItem stone = owned.FirstOrDefault(item => item.ObjectId == plan.StoneObjectId);
        bool chargeChanged = stone.ObjectId == 0u || (stone.Effects & 1u) != 0u;
        if (matches && donorGone && chargeChanged)
            return ManaFillOutcome.Confirmed;
        return timedOut ? ManaFillOutcome.Uncertain : ManaFillOutcome.Waiting;
    }
}
