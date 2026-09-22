using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Where a command that names an object is allowed to look. The reference's
/// command flags spell the same three: no flag searches everything,
/// <c>i</c> the packs, <c>l</c> the landscape.
/// </summary>
internal enum UbSearchScope
{
    /// <summary>Packs first, then the open container, then the landscape.</summary>
    All,

    /// <summary>Only what the character is carrying.</summary>
    Inventory,

    /// <summary>Only what is standing in the world.</summary>
    Landscape,
}

/// <summary>
/// Finding the one object a typed name stands for, in the order the reference
/// searches: a number or "selected" first, then the packs, the open container
/// and finally the nearest matching thing on the ground.
/// </summary>
internal static class UbObjectSearch
{
    /// <summary>
    /// The object a name stands for, or false when nothing answers to it.
    /// </summary>
    /// <param name="host">The client to ask.</param>
    /// <param name="name">
    /// The whole name, part of one when <paramref name="partial"/>, a decimal
    /// or hexadecimal object id, or the word "selected".
    /// </param>
    /// <param name="scope">Which objects may answer.</param>
    /// <param name="partial">True when part of a name is enough.</param>
    /// <param name="excludeObjectId">
    /// An object the search must skip: the item already picked as the first
    /// half of "use A on B", so that "use taper on taper" is two tapers.
    /// </param>
    /// <param name="found">The object, when one answered.</param>
    internal static bool TryFind(
        IPluginHost host,
        string? name,
        UbSearchScope scope,
        bool partial,
        uint excludeObjectId,
        out PluginWorldObject found)
    {
        ArgumentNullException.ThrowIfNull(host);
        found = default;
        string text = name?.Trim() ?? string.Empty;
        if (text.Length == 0)
            return false;

        IReadOnlyList<PluginWorldObject> objects =
            host.Automation.Objects.CaptureObjects();
        if (TryFindBySpecialName(host, objects, text, out found))
            return true;

        uint openContainer = host.Automation.Objects.OpenContainerObjectId;
        switch (scope)
        {
            case UbSearchScope.Inventory:
                return TryFirst(
                    objects.Where(static obj => obj.IsOwned),
                    text,
                    partial,
                    excludeObjectId,
                    out found);
            case UbSearchScope.Landscape:
                return TryNearest(
                    host,
                    objects.Where(static obj => obj.IsLandscape),
                    text,
                    partial,
                    excludeObjectId,
                    out found);
            default:
                return TryFirst(
                        objects.Where(static obj => obj.IsOwned),
                        text,
                        partial,
                        excludeObjectId,
                        out found)
                    || (openContainer != 0u && TryFirst(
                        objects.Where(obj => obj.ContainerObjectId == openContainer),
                        text,
                        partial,
                        excludeObjectId,
                        out found))
                    || TryNearest(
                        host,
                        objects.Where(static obj => obj.IsLandscape),
                        text,
                        partial,
                        excludeObjectId,
                        out found);
        }
    }

    /// <summary>
    /// The nearest object of one of <paramref name="classes"/> whose name
    /// answers to <paramref name="name"/>. A blank name matches every object
    /// of those classes, which is how "the closest portal" is asked for.
    /// </summary>
    internal static bool TryFindNearest(
        IPluginHost host,
        string? name,
        bool partial,
        IReadOnlyList<PluginObjectClass> classes,
        out PluginWorldObject found)
    {
        ArgumentNullException.ThrowIfNull(host);
        ArgumentNullException.ThrowIfNull(classes);
        found = default;
        string text = name?.Trim() ?? string.Empty;
        IReadOnlyList<PluginWorldObject> objects =
            host.Automation.Objects.CaptureObjects();
        if (text.Length != 0
            && TryFindBySpecialName(host, objects, text, out found)
            && classes.Contains(found.ObjectClass))
        {
            return true;
        }

        uint self = host.Automation.Character.ObjectId;
        PluginNavigationSnapshot player = host.Automation.Navigation.Snapshot;
        PluginWorldObject? best = null;
        double bestDistance = double.MaxValue;
        foreach (PluginWorldObject candidate in objects)
        {
            if (candidate.ObjectId == self || !classes.Contains(candidate.ObjectClass))
                continue;
            if (!Matches(candidate.Name, text, partial))
                continue;
            double distance = Distance(player, candidate);
            if (best is not null && distance >= bestDistance)
                continue;
            best = candidate;
            bestDistance = distance;
        }
        if (best is not { } value)
            return false;
        found = value;
        return true;
    }

    /// <summary>
    /// Flat ground distance from the character, in meters. An object with no
    /// position of its own is on the character, so it is zero away.
    /// </summary>
    internal static double Distance(
        in PluginNavigationSnapshot player,
        in PluginWorldObject candidate) =>
        !player.IsAvailable || !candidate.HasPosition
            ? 0d
            : player.Position.HorizontalDistanceMeters(candidate.Position);

    private static bool TryFindBySpecialName(
        IPluginHost host,
        IReadOnlyList<PluginWorldObject> objects,
        string text,
        out PluginWorldObject found)
    {
        if (uint.TryParse(
                text,
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out uint decimalId)
            && TryById(objects, decimalId, out found))
        {
            return true;
        }
        string hex = text.StartsWith("0x", StringComparison.OrdinalIgnoreCase)
            ? text[2..]
            : text;
        if (uint.TryParse(
                hex,
                NumberStyles.HexNumber,
                CultureInfo.InvariantCulture,
                out uint hexId)
            && TryById(objects, hexId, out found))
        {
            return true;
        }
        if (text.Equals("selected", StringComparison.OrdinalIgnoreCase)
            && host.Selection.SelectedObjectId is { } selected
            && selected != 0u
            && TryById(objects, selected, out found))
        {
            return true;
        }
        found = default;
        return false;
    }

    private static bool TryById(
        IReadOnlyList<PluginWorldObject> objects,
        uint objectId,
        out PluginWorldObject found)
    {
        foreach (PluginWorldObject candidate in objects)
        {
            if (candidate.ObjectId != objectId)
                continue;
            found = candidate;
            return true;
        }
        found = default;
        return false;
    }

    private static bool TryFirst(
        IEnumerable<PluginWorldObject> candidates,
        string text,
        bool partial,
        uint excludeObjectId,
        out PluginWorldObject found)
    {
        foreach (PluginWorldObject candidate in candidates
            .OrderBy(static obj => obj.ObjectId))
        {
            if (candidate.ObjectId == excludeObjectId)
                continue;
            if (!Matches(candidate.Name, text, partial))
                continue;
            found = candidate;
            return true;
        }
        found = default;
        return false;
    }

    private static bool TryNearest(
        IPluginHost host,
        IEnumerable<PluginWorldObject> candidates,
        string text,
        bool partial,
        uint excludeObjectId,
        out PluginWorldObject found)
    {
        PluginNavigationSnapshot player = host.Automation.Navigation.Snapshot;
        PluginWorldObject? best = null;
        double bestDistance = double.MaxValue;
        foreach (PluginWorldObject candidate in candidates)
        {
            if (candidate.ObjectId == excludeObjectId)
                continue;
            if (!Matches(candidate.Name, text, partial))
                continue;
            double distance = Distance(player, candidate);
            if (best is not null && distance >= bestDistance)
                continue;
            best = candidate;
            bestDistance = distance;
        }
        if (best is not { } value)
        {
            found = default;
            return false;
        }
        found = value;
        return true;
    }

    private static bool Matches(string name, string text, bool partial) =>
        text.Length == 0
        || (partial
            ? name.Contains(text, StringComparison.OrdinalIgnoreCase)
            : name.Equals(text, StringComparison.OrdinalIgnoreCase));
}
