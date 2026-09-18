using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal static class ConfiguredSupplyReadiness
{
    /// <summary>
    /// A zero projected property is meaningful only after the host has
    /// completed identification. An absent object-information surface cannot
    /// establish readiness.
    /// </summary>
    public static bool IsAssessed(
        IAutomationSurface automation,
        uint objectId)
    {
        ArgumentNullException.ThrowIfNull(automation);
        IWorldObjectAutomation objects = automation.Objects;
        return objects.IsAvailable
            && objects.TryGet(objectId, out PluginWorldObject item)
            && item.LastIdTime != 0;
    }

    public static bool TryCaptureProperties(
        IAutomationSurface automation,
        uint objectId,
        out PluginItemProperties properties)
    {
        ArgumentNullException.ThrowIfNull(automation);
        if (IsAssessed(automation, objectId)
            && automation.Items.TryCaptureProperties(objectId, out properties))
        {
            return true;
        }

        properties = default;
        return false;
    }
}
