using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Brings the UB tree across from the plugin's own storage, where an
/// earlier version wrote it at <c>ub/</c>, into the shared profile
/// directory at <see cref="UbSettingStore.Root"/>, once.
/// </summary>
/// <remarks>
/// It only ever COPIES, for the same reason the first-run copy into the
/// plugin's folder does: a file left where it was is a file a person can
/// still find, and an installation that is rolled back to the older build
/// still has its settings. A destination that already holds a file is left
/// exactly as it is -- that file is the live one -- which is what makes a
/// second run a no-op.
/// </remarks>
internal static class UbTreeMigration
{
    /// <summary>
    /// The prefix the tree used to sit under, inside the plugin's own
    /// storage. Nothing writes here any more; it is read once and left.
    /// </summary>
    internal const string LegacyRoot = "ub/";

    /// <summary>What one pass did.</summary>
    internal readonly record struct Result(int Copied, int AlreadyPresent)
    {
        /// <summary>
        /// The single line a user reads, or null when nothing was copied and
        /// so there is nothing to say.
        /// </summary>
        internal string? Describe(string? rootPath)
        {
            if (Copied == 0)
                return null;
            string where = rootPath is { Length: > 0 }
                ? Path.Combine(rootPath, UbSettingStore.Root.TrimEnd('/')
                    .Replace('/', Path.DirectorySeparatorChar))
                : UbSettingStore.Root.TrimEnd('/');
            string kept = AlreadyPresent > 0
                ? $" {AlreadyPresent} file(s) already there were left alone."
                : string.Empty;
            return $"MossTank copied {Copied} UtilityBelt file(s) into {where} "
                + $"(the originals were left where they were).{kept}";
        }
    }

    /// <summary>
    /// Copies every file under the old prefix to the same relative path
    /// under the new root.
    /// </summary>
    internal static Result Run(IPluginStorage pluginStorage, IPluginStorage vtankProfiles)
    {
        ArgumentNullException.ThrowIfNull(pluginStorage);
        ArgumentNullException.ThrowIfNull(vtankProfiles);
        if (!pluginStorage.IsAvailable || !vtankProfiles.IsAvailable)
            return default;

        int copied = 0;
        int alreadyPresent = 0;
        foreach (string key in pluginStorage.List(string.Empty)
            .Where(static key =>
                key.StartsWith(LegacyRoot, StringComparison.OrdinalIgnoreCase))
            .OrderBy(static key => key, StringComparer.Ordinal))
        {
            string relative = key[LegacyRoot.Length..];
            if (relative.Length == 0)
                continue;
            string destination = UbSettingStore.Root + relative;
            if (vtankProfiles.ReadText(destination) is not null)
            {
                alreadyPresent++;
                continue;
            }
            if (pluginStorage.ReadText(key) is not { } content)
                continue;
            vtankProfiles.WriteText(destination, content);
            copied++;
        }
        return new Result(copied, alreadyPresent);
    }
}
