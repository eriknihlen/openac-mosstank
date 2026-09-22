using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal static class VtankProfileDirectory
{
    /// <summary>The real VTank "--" reserved-prefix marker (section 3).</summary>
    internal const string HiddenPrefix = "--";

    /// <summary>
    /// The nav-profile-only "~~" reserved prefix (section 3) — VTank's own
    /// producer/purpose was not determined by the KB research pass either;
    /// this only reproduces the filter.
    /// </summary>
    internal const string NavHiddenPrefix = "~~";

    internal const string ByCharacterLabel = "[By char]";
    internal const string DefaultLabel = "[Default]";
    internal const string NoneLabel = "[None]";

    /// <summary>
    /// A marker the world puts in front of some characters' display names.
    /// It is not part of the character's name.
    /// </summary>
    private const char DisplayMarker = '+';

    /// <summary>
    /// The one name a character's files are filed under, whatever spelling
    /// the world happens to report at the moment it is asked.
    /// </summary>
    /// <remarks>
    /// The reported name arrives in two spellings during a single login: the
    /// plain name while only the character list has it, and the same name
    /// behind a display marker once the character's own object has streamed
    /// in. Keying files by the reported string therefore files one session
    /// under two names — settings are written to one file and read back from
    /// the other. The marker is display only, and a character's name never
    /// starts with one, so dropping it gives a key that is the same before
    /// and after the object arrives and the same one a character without a
    /// marker already has.
    /// </remarks>
    public static string CanonicalCharacterKey(string? reportedName) =>
        string.IsNullOrWhiteSpace(reportedName)
            ? string.Empty
            : reportedName.Trim().TrimStart(DisplayMarker).Trim();

    /// <summary>
    /// The one folder the plugin keeps every file it owns in, beneath the
    /// shared profile directory the host hands out. Nothing outside it is
    /// written once the first-run copy has filled it.
    /// </summary>
    internal const string Root = "mosstank";

    /// <summary>Settings documents and the per-character binding.</summary>
    internal const string SettingsFolder = Root + "/profiles";

    internal const string MetaFolder = Root + "/metas";

    internal const string NavFolder = Root + "/navs";

    internal const string LootFolder = Root + "/loot";

    /// <summary>The old flat layout the first-run copy reads from.</summary>
    internal const string LegacyNavFolder = "navs";

    internal const string LegacyMetaFolder = "metas";

    /// <summary>
    /// The suffix a settings file keeps when it loses a name collision, so
    /// nothing is thrown away.
    /// </summary>
    internal const string TwinSuffix = ".twin-";

    private const string LegacyNavMarker = "nav_";

    /// <summary>
    /// A file a picker never offers: something another tool or this plugin
    /// left beside a real profile rather than a profile of its own.
    /// </summary>
    internal static bool IsSideFile(string bareFileName) =>
        bareFileName.Contains(".bak", StringComparison.OrdinalIgnoreCase)
        || bareFileName.Contains(TwinSuffix, StringComparison.OrdinalIgnoreCase)
        || bareFileName.EndsWith(".tmp", StringComparison.OrdinalIgnoreCase)
        || bareFileName.StartsWith('.');

    /// <summary>The bare file name inside <paramref name="folder"/>.</summary>
    internal static string StripFolder(string key, string folder)
    {
        string prefix = folder + "/";
        return key.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)
            ? key[prefix.Length..]
            : key;
    }

    internal static bool IsLegacyFlatRouteFileName(string bareFileName) =>
        bareFileName.StartsWith(LegacyNavMarker, StringComparison.Ordinal)
        || (bareFileName.StartsWith(HiddenPrefix, StringComparison.Ordinal)
            && bareFileName[HiddenPrefix.Length..].StartsWith(
                LegacyNavMarker, StringComparison.Ordinal));

    internal static string StripLegacyNavMarker(string bareFileName)
    {
        bool isHidden = bareFileName.StartsWith(HiddenPrefix, StringComparison.Ordinal);
        string rest = isHidden ? bareFileName[HiddenPrefix.Length..] : bareFileName;
        rest = rest[LegacyNavMarker.Length..];
        return isHidden ? HiddenPrefix + rest : rest;
    }

    /// <summary>
    /// Every settings document in the profiles folder, whoever owns it. The
    /// pickers hide another character's reserved family; an operation that
    /// spans every profile on disk must not.
    /// </summary>
    internal static IReadOnlyList<string> ListEverySettingsKey(IPluginStorage storage) =>
        [.. EnumerateFolderFileNames(storage, SettingsFolder, ".usd")
            .Select(bareName => $"{SettingsFolder}/{bareName}")];

    public static string AutoCharacterFileName(
        string characterName,
        string server,
        string extension) =>
        $"{HiddenPrefix}{characterName}_{server}.{extension.TrimStart('.')}";

    public static string SubProfilePrefix(string characterName, string server) =>
        $"{HiddenPrefix}{characterName}_{server}_";

    public static bool IsHiddenFromOtherCharacters(
        string fileName,
        string characterName,
        string server) =>
        fileName.StartsWith(HiddenPrefix, StringComparison.Ordinal)
        && !fileName.StartsWith(
            SubProfilePrefix(characterName, server),
            StringComparison.Ordinal);

    public static string? TryDisplayName(
        string fileName,
        string characterName,
        string server)
    {
        string prefix = SubProfilePrefix(characterName, server);
        if (!fileName.StartsWith(prefix, StringComparison.Ordinal))
            return null;
        string withoutPrefix = fileName[prefix.Length..];
        int dot = withoutPrefix.LastIndexOf('.');
        string suffix = dot >= 0 ? withoutPrefix[..dot] : withoutPrefix;
        return suffix.Length == 0 ? null : $"[Char] {suffix}";
    }

    /// <summary>
    /// One entry in a profile picker: the real on-disk file name, and the
    /// label VTank would show for it.
    /// </summary>
    public readonly record struct ProfileEntry(string FileName, string DisplayName);

    public static IReadOnlyList<ProfileEntry> ListSettingsProfiles(
        IPluginStorage storage,
        string characterName,
        string server,
        bool mineOnly,
        string? currentFileName = null)
    {
        var entries = new List<ProfileEntry>
        {
            new(string.Empty, DefaultLabel),
            new(string.Empty, ByCharacterLabel),
        };
        string? currentBareName = currentFileName is null
            ? null
            : StripFolder(currentFileName, SettingsFolder);
        foreach (string fileName in EnumerateFolderFileNames(storage, SettingsFolder, ".usd"))
        {
            string key = $"{SettingsFolder}/{fileName}";
            string? subProfileDisplay = TryDisplayName(fileName, characterName, server);
            if (subProfileDisplay is not null)
            {
                entries.Add(new ProfileEntry(key, subProfileDisplay));
                continue;
            }
            if (fileName.StartsWith(HiddenPrefix, StringComparison.Ordinal))
                continue; // someone else's --Name_Server(.usd|_*) family.
            if (mineOnly
                && !fileName.Equals(currentBareName, StringComparison.Ordinal))
            {
                continue;
            }
            entries.Add(new ProfileEntry(key, fileName));
        }
        return entries;
    }

    public static IReadOnlyList<ProfileEntry> ListNavigationProfiles(IPluginStorage storage)
    {
        var entries = new List<ProfileEntry>
        {
            new(string.Empty, NoneLabel),
            new(string.Empty, ByCharacterLabel),
        };
        // A route dropped in as the older ".nav" form is offered as it is:
        // picking it loads it, and a later save writes this plugin's own
        // ".af" beside it rather than over it.
        foreach (string bareName in EnumerateFolderFileNames(
            storage, NavFolder, ".af", ".nav"))
        {
            if (bareName.StartsWith(HiddenPrefix, StringComparison.Ordinal)
                || bareName.StartsWith(NavHiddenPrefix, StringComparison.Ordinal))
            {
                continue;
            }
            entries.Add(new ProfileEntry($"{NavFolder}/{bareName}", bareName));
        }
        return entries;
    }

    public static IReadOnlyList<ProfileEntry> ListMetaProfiles(IPluginStorage storage)
    {
        var entries = new List<ProfileEntry>
        {
            new(string.Empty, NoneLabel),
            new(string.Empty, ByCharacterLabel),
        };
        // As with routes: a meta dropped in as the older ".met" form is
        // offered as it is and loaded in place.
        foreach (string bareName in EnumerateFolderFileNames(
            storage, MetaFolder, ".af", ".met"))
        {
            if (bareName.StartsWith(HiddenPrefix, StringComparison.Ordinal))
                continue;
            entries.Add(new ProfileEntry($"{MetaFolder}/{bareName}", bareName));
        }
        return entries;
    }

    public static IReadOnlyList<ProfileEntry> ListLootProfiles(IPluginStorage storage)
    {
        var entries = new List<ProfileEntry>
        {
            new(string.Empty, NoneLabel),
            new(string.Empty, ByCharacterLabel),
        };
        foreach (string bareName in EnumerateFolderFileNames(storage, LootFolder, ".utl"))
        {
            if (bareName.StartsWith(HiddenPrefix, StringComparison.Ordinal))
                continue;
            entries.Add(new ProfileEntry($"{LootFolder}/{bareName}", bareName));
        }
        return entries;
    }

    public static string AstFileName(string characterName, string server) =>
        $"{characterName}_{server}.ast";

    public static string CdfFileName(string characterName, string server) =>
        $"{SettingsFolder}/{server}_{characterName}.cdf";

    /// <summary>
    /// Re-roots one name out of a binding file into the folder its kind
    /// lives in. A binding copied in from the old flat layout, or written by
    /// hand, names a file without saying which folder holds it; the kind
    /// does, so an unrooted name is read as the bare file name it is.
    /// </summary>
    private static string ReRoot(string storedName, string folder)
    {
        if (storedName.Length == 0)
            return storedName;
        string bareName = storedName[(storedName.LastIndexOf('/') + 1)..];
        return $"{folder}/{bareName}";
    }

    /// <summary>The literal version header a valid <c>.cdf</c> starts with.</summary>
    internal const string CdfHeader = "uTank2 CDF 1.0";

    public readonly record struct VtankCharacterBinding(
        string SettingsFileName,
        string LootFileName,
        string NavFileName,
        string? MetaFileName);

    public static VtankCharacterBinding? TryReadCharacterBinding(
        IPluginStorage storage,
        string characterName,
        string server)
    {
        ArgumentNullException.ThrowIfNull(storage);
        ArgumentException.ThrowIfNullOrWhiteSpace(characterName);
        ArgumentException.ThrowIfNullOrWhiteSpace(server);
        if (!storage.IsAvailable)
            return null;
        string? text = storage.ReadText(CdfFileName(characterName, server));
        if (text is null)
            return null;
        string[] lines = text
            .Replace("\r\n", "\n", StringComparison.Ordinal)
            .Split('\n');
        if (lines.Length < 4 || !string.Equals(lines[0], CdfHeader, StringComparison.Ordinal))
            return null;
        string settings = lines[1].EndsWith(".uts", StringComparison.OrdinalIgnoreCase)
            ? string.Concat(lines[1].AsSpan(0, lines[1].Length - 4), ".usd")
            : lines[1];
        string? meta = lines.Length >= 5 && lines[4].Length > 0 ? lines[4] : null;
        return new VtankCharacterBinding(
            ReRoot(settings, SettingsFolder),
            ReRoot(lines[2], LootFolder),
            ReRoot(lines[3], NavFolder),
            meta is null ? null : ReRoot(meta, MetaFolder));
    }

    public static void WriteCharacterBinding(
        IPluginStorage storage,
        string characterName,
        string server,
        VtankCharacterBinding binding)
    {
        ArgumentNullException.ThrowIfNull(storage);
        ArgumentException.ThrowIfNullOrWhiteSpace(characterName);
        ArgumentException.ThrowIfNullOrWhiteSpace(server);
        if (!storage.IsAvailable)
            return;
        var lines = new List<string>
        {
            CdfHeader,
            binding.SettingsFileName,
            binding.LootFileName,
            binding.NavFileName,
        };
        if (!string.IsNullOrEmpty(binding.MetaFileName))
            lines.Add(binding.MetaFileName);
        storage.WriteText(
            CdfFileName(characterName, server),
            string.Join("\r\n", lines) + "\r\n");
    }

    private static IEnumerable<string> EnumerateFolderFileNames(
        IPluginStorage storage,
        string folder,
        params string[] extensions)
    {
        if (!storage.IsAvailable)
            yield break;
        string folderPrefix = folder + "/";
        foreach (string bareName in storage.List(folder)
            .Where(key => key.StartsWith(folderPrefix, StringComparison.Ordinal))
            .Select(key => key[folderPrefix.Length..])
            .Where(bareName => !bareName.Contains('/', StringComparison.Ordinal))
            .Where(bareName => extensions.Any(extension =>
                bareName.EndsWith(extension, StringComparison.OrdinalIgnoreCase)))
            .Where(static bareName => !IsSideFile(bareName))
            .OrderBy(static bareName => bareName, StringComparer.OrdinalIgnoreCase))
        {
            yield return bareName;
        }
    }
}
