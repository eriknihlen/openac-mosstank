using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Fills the plugin's own folder from the flat layout it used to write,
/// once, on the first run that finds the folder empty of a file it needs.
/// </summary>
/// <remarks>
/// It only ever COPIES. The shared profile directory is not this plugin's
/// to tidy: another tool may be reading the same files, and a user who
/// dropped their own files in there expects to still find them. A file
/// already present at its new name is left exactly as it is, which is what
/// makes a second run a no-op.
/// </remarks>
internal static class MossTankFileLayoutMigration
{
    /// <summary>
    /// The outcome of one migration pass: how many files were copied into
    /// each folder, and the twins that had to be set aside.
    /// </summary>
    internal readonly record struct Result(
        int Settings,
        int Loot,
        int Navs,
        int Metas,
        int TwinsKept)
    {
        internal int Total => Settings + Loot + Navs + Metas;

        /// <summary>
        /// The single line a user reads, or null when there was nothing to
        /// copy and so nothing to say.
        /// </summary>
        internal string? Describe(string? rootPath)
        {
            if (Total == 0)
                return null;
            var parts = new List<string>();
            if (Settings > 0)
                parts.Add($"{Settings} settings");
            if (Loot > 0)
                parts.Add($"{Loot} loot");
            if (Navs > 0)
                parts.Add($"{Navs} nav");
            if (Metas > 0)
                parts.Add($"{Metas} meta");
            string where = rootPath is { Length: > 0 }
                ? Path.Combine(rootPath, VtankProfileDirectory.Root)
                : VtankProfileDirectory.Root;
            string twins = TwinsKept > 0
                ? $" {TwinsKept} duplicate character file(s) were kept alongside."
                : string.Empty;
            return $"MossTank copied {string.Join(", ", parts)} file(s) into "
                + $"{where} (the originals were left where they were).{twins}";
        }
    }

    internal static Result Run(IPluginStorage storage, DateTimeOffset now)
    {
        ArgumentNullException.ThrowIfNull(storage);
        if (!storage.IsAvailable)
            return default;

        int settings = 0;
        int loot = 0;
        int navs = 0;
        int metas = 0;
        int twins = 0;
        // A file whose name carries the display marker is copied last, so
        // that when both spellings of one character exist the marked one --
        // the one the session went on writing once the character's own
        // object had arrived, and so the live one -- lands on top and the
        // other is set aside whole rather than skipped.
        foreach (string key in storage.List(string.Empty)
            .OrderBy(static key => HasDisplayMarker(key) ? 1 : 0)
            .ThenBy(static key => key, StringComparer.OrdinalIgnoreCase))
        {
            if (Destination(key) is not { } destination)
                continue;
            if (!TryCopy(storage, key, destination, now, out bool keptTwin))
                continue;
            if (keptTwin)
                twins++;
            if (destination.StartsWith(
                VtankProfileDirectory.SettingsFolder, StringComparison.Ordinal))
            {
                settings++;
            }
            else if (destination.StartsWith(
                VtankProfileDirectory.LootFolder, StringComparison.Ordinal))
            {
                loot++;
            }
            else if (destination.StartsWith(
                VtankProfileDirectory.NavFolder, StringComparison.Ordinal))
            {
                navs++;
            }
            else
            {
                metas++;
            }
        }
        return new Result(settings, loot, navs, metas, twins);
    }

    /// <summary>
    /// Where one key in the old layout belongs, or null when the key is not
    /// this plugin's to move: anything already inside the plugin's folder,
    /// its own side-car and import folders, and any other tool's subfolder.
    /// </summary>
    private static string? Destination(string key)
    {
        if (key.StartsWith(
                VtankProfileDirectory.Root + "/", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }
        string bareName = key[(key.LastIndexOf('/') + 1)..];
        if (bareName.Length == 0 || VtankProfileDirectory.IsSideFile(bareName))
            return null;
        string folder = key.Length == bareName.Length
            ? string.Empty
            : key[..(key.Length - bareName.Length - 1)];

        if (folder.Equals(
                VtankProfileDirectory.LegacyNavFolder, StringComparison.OrdinalIgnoreCase))
        {
            return $"{VtankProfileDirectory.NavFolder}/{Canonical(bareName)}";
        }
        if (folder.Equals(
                VtankProfileDirectory.LegacyMetaFolder, StringComparison.OrdinalIgnoreCase))
        {
            return $"{VtankProfileDirectory.MetaFolder}/{Canonical(bareName)}";
        }
        if (folder.Length > 0)
            return null; // someone else's subfolder.

        if (bareName.EndsWith(".usd", StringComparison.OrdinalIgnoreCase)
            || bareName.EndsWith(".cdf", StringComparison.OrdinalIgnoreCase))
        {
            return $"{VtankProfileDirectory.SettingsFolder}/{Canonical(bareName)}";
        }
        if (bareName.EndsWith(".utl", StringComparison.OrdinalIgnoreCase))
            return $"{VtankProfileDirectory.LootFolder}/{Canonical(bareName)}";
        if (bareName.EndsWith(".af", StringComparison.OrdinalIgnoreCase))
        {
            // The flat layout told routes and metas apart by a name marker
            // rather than by a folder.
            return VtankProfileDirectory.IsLegacyFlatRouteFileName(bareName)
                ? $"{VtankProfileDirectory.NavFolder}/"
                    + Canonical(VtankProfileDirectory.StripLegacyNavMarker(bareName))
                : $"{VtankProfileDirectory.MetaFolder}/{Canonical(bareName)}";
        }
        return null;
    }

    private static bool HasDisplayMarker(string key)
    {
        string bareName = key[(key.LastIndexOf('/') + 1)..];
        return !Canonical(bareName).Equals(bareName, StringComparison.Ordinal);
    }

    /// <summary>
    /// The name this file gets in the new layout: the display marker the
    /// world puts in front of some characters' names is not part of the
    /// name, so a file that picked one up is filed under the plain name the
    /// same character is keyed by from now on.
    /// </summary>
    private static string Canonical(string bareName)
    {
        if (bareName.StartsWith(
            VtankProfileDirectory.HiddenPrefix + "+", StringComparison.Ordinal))
        {
            return VtankProfileDirectory.HiddenPrefix
                + bareName[(VtankProfileDirectory.HiddenPrefix.Length + 1)..];
        }
        // A binding file is named <server>_<character>, so the marker sits
        // after the separator rather than at the front.
        if (bareName.EndsWith(".cdf", StringComparison.OrdinalIgnoreCase))
        {
            int marker = bareName.IndexOf("_+", StringComparison.Ordinal);
            if (marker >= 0)
                return bareName.Remove(marker + 1, 1);
        }
        return bareName;
    }

    /// <summary>
    /// Copies one file unless its destination is already taken. A taken
    /// destination that this key would have become a twin of is kept under
    /// a dated name rather than dropped.
    /// </summary>
    private static bool TryCopy(
        IPluginStorage storage,
        string sourceKey,
        string destinationKey,
        DateTimeOffset now,
        out bool keptTwin)
    {
        keptTwin = false;
        string? content = storage.ReadText(sourceKey);
        if (content is null)
            return false;

        string? existing = storage.ReadText(destinationKey);
        if (existing is not null)
        {
            string sourceBareName = sourceKey[(sourceKey.LastIndexOf('/') + 1)..];
            string destinationBareName =
                destinationKey[(destinationKey.LastIndexOf('/') + 1)..];
            bool isTwin = !sourceBareName.Equals(
                destinationBareName, StringComparison.OrdinalIgnoreCase);
            if (!isTwin)
                return false; // already copied on an earlier run.

            // Two files for one character, written in the same session under
            // the two spellings of its name. The marked one is the one the
            // session went on writing once the character's own object had
            // arrived, so it is the live one; the other is set aside whole.
            string keptKey = destinationKey
                + VtankProfileDirectory.TwinSuffix
                + now.ToString("yyyy-MM-dd", System.Globalization.CultureInfo.InvariantCulture);
            if (storage.ReadText(keptKey) is not null)
                return false; // an earlier run already resolved this pair.
            storage.WriteText(keptKey, existing);
            storage.WriteText(destinationKey, content);
            keptTwin = true;
            return true;
        }

        storage.WriteText(destinationKey, content);
        return true;
    }
}
