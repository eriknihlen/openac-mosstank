using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class MossTankRouteProfileStore
{
    public const string ByCharacter = "By char";
    private const string FolderPrefix = VtankProfileDirectory.NavFolder + "/";
    private const string LegacyRosterKey = "profiles/route/index.json";

    private readonly IPluginHost _host;
    private string _characterName = string.Empty;
    private string _selected = ByCharacter;
    private string? _pendingLegacyBareName;
    private bool _rosterSwept;

    public MossTankRouteProfileStore(IPluginHost host)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
    }

    public string Selected => NameOf(_selected);
    public string? RecoveryNotice { get; private set; }

    /// <summary>The file the last load read, or tried to read.</summary>
    public string? LastLoadKey { get; private set; }

    private string Server => _host.Automation.Character.WorldName;
    private IPluginStorage VtankStorage => _host.VtankProfiles;
    private bool CanBindFiles => _characterName.Length > 0 && Server.Length > 0;

    /// <summary>
    /// The name a file goes by, which is also the name that loads it again.
    /// A bare name loads the ".nav" first, so a ".nav" goes by its bare name;
    /// so does the plugin's own ".af", except beside a ".nav" of the same
    /// name, where it keeps its extension so picking it by that name reaches
    /// it rather than the file beside it.
    /// </summary>
    private string NameOf(string fileName) => NameOf(fileName, DroppedRouteExists);

    private static string NameOf(string fileName, Func<string, bool> droppedExists)
    {
        if (fileName.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
            return fileName;
        string value = fileName.StartsWith(FolderPrefix, StringComparison.Ordinal)
            ? fileName[FolderPrefix.Length..]
            : fileName;
        if (value.EndsWith(".nav", StringComparison.OrdinalIgnoreCase))
            return value[..^".nav".Length];
        if (!value.EndsWith(".af", StringComparison.OrdinalIgnoreCase))
            return value;
        string bareName = value[..^3];
        return droppedExists(bareName) ? value : bareName;
    }

    private bool DroppedRouteExists(string bareName) =>
        VtankStorage.IsAvailable && VtankStorage.ReadText(DroppedFileName(bareName)) is not null;

    /// <summary>Where a route dropped in as the older ".nav" form sits.</summary>
    private static string DroppedFileName(string bareName) =>
        $"{VtankProfileDirectory.NavFolder}/{bareName}.nav";

    public IReadOnlyList<string> AvailableNames
    {
        get
        {
            IReadOnlyList<VtankProfileDirectory.ProfileEntry> entries =
                VtankProfileDirectory.ListNavigationProfiles(VtankStorage);
            var listed = new HashSet<string>(
                entries.Select(static entry => entry.FileName),
                StringComparer.OrdinalIgnoreCase);
            var names = new List<string> { ByCharacter };
            foreach (VtankProfileDirectory.ProfileEntry entry in entries)
            {
                if (entry.FileName.Length == 0)
                    continue;
                names.Add(NameOf(
                    entry.FileName,
                    bareName => listed.Contains(DroppedFileName(bareName))));
            }
            return names;
        }
    }

    public bool BindCharacter(string? characterName)
    {
        string normalized = VtankProfileDirectory.CanonicalCharacterKey(characterName);
        if (string.Equals(normalized, _characterName, StringComparison.OrdinalIgnoreCase))
            return false;
        _characterName = normalized;
        _pendingLegacyBareName = null;
        VtankProfileDirectory.VtankCharacterBinding? binding = CanBindFiles
            ? VtankProfileDirectory.TryReadCharacterBinding(VtankStorage, _characterName, Server)
            : null;
        _selected = binding is { NavFileName.Length: > 0 } bound
            ? bound.NavFileName
            : ByCharacter;
        return true;
    }

    public bool Select(string? name, bool exactOnly = false)
    {
        string normalized = Normalize(name);
        if (normalized.Length == 0)
            return false;
        if (normalized.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
        {
            _selected = ByCharacter;
            _pendingLegacyBareName = null;
            WriteBinding();
            return true;
        }

        if (ResolveExisting(normalized, exactOnly) is { } found)
        {
            _selected = found;
            _pendingLegacyBareName = null;
            WriteBinding();
            return true;
        }
        string bareName = BareName(normalized);
        if (_host.Storage.IsAvailable
            && _host.Storage.ReadText(LegacyProfileKey(bareName, byCharacter: false)) is not null)
        {
            _selected = ToFileName(bareName);
            _pendingLegacyBareName = bareName;
            WriteBinding();
            return true;
        }
        return false;
    }

    /// <summary>
    /// The route a follow walks. Following someone is not an edit of the
    /// loaded route: a follow route carries no waypoints, so writing one
    /// over a route file wipes that route. A follow switches to this route
    /// instead and aims it, and the file that was loaded stays as it was.
    /// </summary>
    public const string FollowRouteName = "UBFollow";

    /// <summary>Whether the follow route is the selected route.</summary>
    public bool FollowRouteSelected => _selected.Equals(
        ToFileName(FollowRouteName), StringComparison.OrdinalIgnoreCase);

    /// <summary>
    /// Selects the follow route, writing it empty first if it is not there
    /// yet. Nothing else is written: the route that was selected keeps its
    /// file exactly as it is.
    /// </summary>
    public void SelectFollowRoute()
    {
        string fileName = ToFileName(FollowRouteName);
        if (VtankStorage.IsAvailable && VtankStorage.ReadText(fileName) is null)
            WriteRoute(fileName, new NavigationSettings());
        _selected = fileName;
        _pendingLegacyBareName = null;
        WriteBinding();
    }

    public bool Exists(string? name)
    {
        string normalized = Normalize(name);
        if (normalized.Length == 0)
            return false;
        if (normalized.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
            return true;

        if (ResolveExisting(normalized) is not null)
            return true;

        return _host.Storage.IsAvailable
            && _host.Storage.ReadText(
                LegacyProfileKey(BareName(normalized), byCharacter: false)) is not null;
    }

    /// <summary>
    /// Writes a route under a new name and selects it. A bare name is written
    /// in the older ".nav" form, as the reference writes a route; a name that
    /// carries ".nav" or ".af" is written exactly as it says.
    /// </summary>
    public bool Create(
        string? name,
        bool copyCurrent,
        NavigationSettings current,
        out string notice)
    {
        string normalized = Normalize(name);
        if (normalized.Length is < 1 or > 64)
        {
            notice = "Enter a route profile name (1-64 characters).";
            return false;
        }
        if (normalized.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
        {
            notice = "'By char' is the built-in route profile.";
            return false;
        }
        string fileName = NewFileName(normalized);
        NavigationSettings source = copyCurrent ? current : new NavigationSettings();
        if (!WriteRoute(fileName, source))
        {
            notice = SaveNotice ?? "Route storage is unavailable.";
            return false;
        }
        _selected = fileName;
        _pendingLegacyBareName = null;
        WriteBinding();
        notice = copyCurrent
            ? $"Copied route to {NameOf(fileName)}."
            : $"Created route profile {NameOf(fileName)}.";
        return true;
    }

    public MossTankProfileLoad LoadCurrent(
        NavigationSettings target,
        ISpellCatalog spells)
    {
        ArgumentNullException.ThrowIfNull(target);
        ArgumentNullException.ThrowIfNull(spells);
        SweepLegacyRosterIfNeeded();
        MigrateLegacyIfNeeded(target, spells);
        string fileName = CurrentFileName();
        LastLoadKey = fileName;
        string? text = VtankStorage.IsAvailable ? VtankStorage.ReadText(fileName) : null;
        if (text is null)
            return MossTankProfileLoad.Missing;
        if (IsDroppedForeignFormat(fileName))
        {
            if (VtankNavRouteSerializer.TryLoad(text, target, spells, out string navError))
                return MossTankProfileLoad.Loaded;
            RecoveryNotice = MossTankProfileRecovery.Preserve(
                _host, "route", fileName, text, new FormatException(navError));
            _host.Log.Warn(RecoveryNotice);
            return MossTankProfileLoad.Failed;
        }
        if (!MetafSerializer.TryLoadNav(text, target, spells, out string error))
        {
            RecoveryNotice = MossTankProfileRecovery.Preserve(
                _host, "route", fileName, text, new FormatException(error));
            _host.Log.Warn(RecoveryNotice);
            return MossTankProfileLoad.Failed;
        }
        return MossTankProfileLoad.Loaded;
    }

    /// <summary>
    /// The automatic, per-character file of a character nobody has named yet.
    /// There is no such file: a name arrives some way into a login and goes
    /// away again during a relog, and a file filed under the gap between
    /// them is one the character, once named, never reads.
    /// </summary>
    private bool FilesUnderNobody =>
        _characterName.Length == 0
        && _selected.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase);

    /// <summary>Why the last save wrote nothing, or null when it wrote.</summary>
    public string? SaveNotice { get; private set; }

    /// <summary>
    /// Writes the route back to the file it was loaded from, in that file's
    /// own form: a ".nav" stays a ".nav". False, with <see cref="SaveNotice"/>
    /// saying why, when nothing was written.
    /// </summary>
    public bool SaveCurrent(NavigationSettings settings)
    {
        SaveNotice = null;
        if (FilesUnderNobody)
            return false;
        return WriteRoute(CurrentFileName(), settings);
    }

    public bool TryImportLegacy(
        string? name,
        NavigationSettings target,
        ISpellCatalog spells,
        out string notice)
    {
        string normalized = Normalize(name);
        if (!_host.Storage.IsAvailable || normalized.Length == 0)
        {
            notice = "Legacy navigation storage is unavailable.";
            return false;
        }
        string? key = _host.Storage.List("imports")
            .Concat(_host.Storage.List("exports"))
            .FirstOrDefault(candidate =>
                candidate.EndsWith(".nav", StringComparison.OrdinalIgnoreCase)
                && Path.GetFileNameWithoutExtension(candidate).Equals(
                    normalized,
                    StringComparison.OrdinalIgnoreCase));
        string? source = key is null ? null : _host.Storage.ReadText(key);
        if (string.IsNullOrWhiteSpace(source))
        {
            notice = $"VTank navigation file '{normalized}.nav' was not found in imports.";
            return false;
        }
        if (!VtankNavRouteSerializer.TryLoad(source, target, spells, out string error))
        {
            notice = $"Could not import {Path.GetFileName(key)}: {error}";
            return false;
        }
        string fileName = ToFileName(normalized);
        if (!WriteRoute(fileName, target))
        {
            notice = SaveNotice ?? "Route storage is unavailable.";
            return false;
        }
        _selected = fileName;
        _pendingLegacyBareName = null;
        WriteBinding();
        notice = $"Imported VTank navigation profile {NameOf(fileName)}.";
        return true;
    }

    /// <summary>
    /// Copies one saved route onto another landblock, shifting every point by
    /// the given offsets. The selection is untouched: translating a route is
    /// a file operation, not a decision about which route to walk.
    /// </summary>
    /// <param name="sourceName">The route to read: a bare name reads the .nav first, a name with its extension that file.</param>
    /// <param name="targetName">The name to write it under: a bare name writes a .nav, a name with its extension that file.</param>
    /// <param name="eastWestOffset">Coordinates to add to every point's east/west.</param>
    /// <param name="northSouthOffset">Coordinates to add to every point's north/south.</param>
    /// <param name="force">True to overwrite a route that is already there.</param>
    /// <param name="spells">Used to name a recall waypoint's spell.</param>
    /// <param name="notice">
    /// Why nothing was written, or, when the route was written, the name it
    /// was saved under.
    /// </param>
    /// <param name="records">How many waypoints the written route holds.</param>
    /// <returns>True when the translated route was written.</returns>
    public bool TryTranslate(
        string? sourceName,
        string? targetName,
        double eastWestOffset,
        double northSouthOffset,
        bool force,
        ISpellCatalog spells,
        out string notice,
        out int records)
    {
        ArgumentNullException.ThrowIfNull(spells);
        records = 0;
        string source = Normalize(sourceName);
        string target = Normalize(targetName);
        if (source.Length == 0 || target.Length == 0)
        {
            notice = "Name both the route to translate and the route to save it as.";
            return false;
        }
        if (!VtankStorage.IsAvailable)
        {
            notice = "Route storage is unavailable.";
            return false;
        }

        string? sourceKey = ResolveExisting(source);
        string? text = sourceKey is null ? null : VtankStorage.ReadText(sourceKey);
        if (text is null)
        {
            notice = $"Could not find route to load: {source}";
            return false;
        }
        string targetKey = NewFileName(target);
        if (!force && VtankStorage.ReadText(targetKey) is not null)
        {
            notice =
                "Output path already exists! Run with force flag to overwrite: "
                + NameOf(targetKey);
            return false;
        }

        var scratch = new NavigationSettings();
        bool parsed = IsDroppedForeignFormat(sourceKey!)
            ? VtankNavRouteSerializer.TryLoad(text, scratch, spells, out _)
            : MetafSerializer.TryLoadNav(text, scratch, spells, out _);
        if (!parsed)
        {
            notice = "Unable to parse route";
            return false;
        }
        if (!scratch.Waypoints.Any(static point =>
            point.Type == RouteWaypointType.Point))
        {
            notice = "Unable to translate route, no nav points found!";
            return false;
        }

        foreach (RouteWaypoint point in scratch.Waypoints)
        {
            point.Position = Shift(point.Position, eastWestOffset, northSouthOffset);
            point.ReferencePosition = Shift(
                point.ReferencePosition, eastWestOffset, northSouthOffset);
        }
        if (!WriteRoute(targetKey, scratch))
        {
            notice = SaveNotice ?? "Route storage is unavailable.";
            return false;
        }
        records = scratch.Waypoints.Count;
        notice = NameOf(targetKey);
        return true;
    }

    private static PluginNavigationPosition Shift(
        in PluginNavigationPosition position,
        double eastWest,
        double northSouth) => position with
        {
            EastWest = position.EastWest + eastWest,
            NorthSouth = position.NorthSouth + northSouth,
        };

    public bool Delete(out string notice)
    {
        if (_selected.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
        {
            notice = "'By char' is the built-in route profile and cannot be deleted.";
            return false;
        }
        string fileName = _selected;
        if (VtankStorage.IsAvailable)
            VtankStorage.Delete(fileName);
        _selected = ByCharacter;
        _pendingLegacyBareName = null;
        WriteBinding();
        notice = $"Deleted route profile {NameOf(fileName)}.";
        return true;
    }

    /// <summary>
    /// Resets only the fields this store owns (Mode/Waypoints/FollowTarget)
    /// — Enabled/Priority/MinimumDistanceMeters/FollowAroundCorners/
    /// OpenDoors/Door* belong to the Settings profile and are left alone.
    /// </summary>
    public void ClearCurrent(NavigationSettings target)
    {
        target.Mode = RouteMode.Circular;
        target.FollowTargetObjectId = 0u;
        target.FollowTargetName = string.Empty;
        target.Waypoints.Clear();
        SaveCurrent(target);
    }


    // ------------------------------------------------------------------
    // Legacy JSON -> .af migration.
    // ------------------------------------------------------------------

    private void SweepLegacyRosterIfNeeded()
    {
        if (_rosterSwept)
            return;
        _rosterSwept = true;
        if (!_host.Storage.IsAvailable)
            return;
        List<string>? names = ReadRosterNames();
        if (names is not { Count: > 0 })
            return;

        var remaining = new List<string>();
        int migrated = 0;
        foreach (string rawName in names)
        {
            string name = (rawName ?? string.Empty).Trim();
            if (name.Length == 0 || name.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
                continue; // stale/invalid row; drop it rather than loop on it forever.

            string legacyKey = LegacyProfileKey(name, byCharacter: false);
            LegacyRouteDocument? legacy = ReadLegacyJson(legacyKey);
            if (legacy is null)
                continue;

            string fileName = ToFileName(name);
            if (VtankStorage.IsAvailable && VtankStorage.ReadText(fileName) is null)
            {
                var scratch = new NavigationSettings();
                legacy.ApplyRouteOnly(scratch);
                WriteRoute(fileName, scratch);
            }
            _host.Storage.Delete(legacyKey);
            migrated++;
        }

        if (migrated == 0)
            return;

        if (remaining.Count > 0)
        {
            _host.Storage.WriteText(
                LegacyRosterKey,
                JsonSerializer.Serialize(new LegacyRosterDocument { Names = remaining }, JsonOptions));
        }
        else
        {
            _host.Storage.Delete(LegacyRosterKey);
        }
        _host.Log.Warn($"Migrated {migrated} legacy MossTank named route profile(s) from the old roster.");
    }

    private List<string>? ReadRosterNames()
    {
        string? json = null;
        try
        {
            json = _host.Storage.ReadText(LegacyRosterKey);
            return string.IsNullOrWhiteSpace(json)
                ? null
                : JsonSerializer.Deserialize<LegacyRosterDocument>(json, JsonOptions)?.Names;
        }
        catch (Exception error)
        {
            RecoveryNotice = MossTankProfileRecovery.Preserve(_host, "route", LegacyRosterKey, json, error);
            _host.Log.Warn(RecoveryNotice);
            return null;
        }
    }

    private void MigrateLegacyIfNeeded(NavigationSettings target, ISpellCatalog spells)
    {
        string fileName = CurrentFileName();
        if (!VtankStorage.IsAvailable || VtankStorage.ReadText(fileName) is not null)
        {
            _pendingLegacyBareName = null;
            return;
        }
        string legacyKey = _pendingLegacyBareName is { Length: > 0 } bareName
            ? LegacyProfileKey(bareName, byCharacter: false)
            : LegacyProfileKey(_characterName, byCharacter: true);
        LegacyRouteDocument? legacy = ReadLegacyJson(legacyKey);
        if (legacy is null)
        {
            _pendingLegacyBareName = null;
            return;
        }
        legacy.ApplyRouteOnly(target);
        WriteRoute(fileName, target);
        if (_host.Storage.IsAvailable)
            _host.Storage.Delete(legacyKey);
        _pendingLegacyBareName = null;
        _host.Log.Warn($"Migrated legacy MossTank route profile '{legacyKey}' to '{fileName}'.");
    }

    private LegacyRouteDocument? ReadLegacyJson(string key)
    {
        if (!_host.Storage.IsAvailable)
            return null;
        string? json = null;
        try
        {
            json = _host.Storage.ReadText(key);
            return string.IsNullOrWhiteSpace(json)
                ? null
                : JsonSerializer.Deserialize<LegacyRouteDocument>(json, JsonOptions);
        }
        catch (Exception error)
        {
            RecoveryNotice = MossTankProfileRecovery.Preserve(_host, "route", key, json, error);
            _host.Log.Warn(RecoveryNotice);
            return null;
        }
    }

    // ------------------------------------------------------------------
    // File naming, storage plumbing.
    // ------------------------------------------------------------------

    private string CurrentFileName() => _selected.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase)
        ? $"{VtankProfileDirectory.NavFolder}/{VtankProfileDirectory.AutoCharacterFileName(_characterName, Server, "af")}"
        : _selected;

    /// <summary>
    /// A route named in the plugin's own format, unless the name says
    /// otherwise: a file dropped into the folder as the older ".nav" form is
    /// addressed exactly as it sits there, so picking it loads it.
    /// </summary>
    private static string ToFileName(string bareName) =>
        bareName.EndsWith(".nav", StringComparison.OrdinalIgnoreCase)
        || bareName.EndsWith(".af", StringComparison.OrdinalIgnoreCase)
            ? $"{VtankProfileDirectory.NavFolder}/{bareName}"
            : $"{VtankProfileDirectory.NavFolder}/{bareName}.af";

    /// <summary>
    /// The file a route written under a new name goes to: the older ".nav"
    /// form for a bare name, or exactly the file a name with its extension
    /// says.
    /// </summary>
    private static string NewFileName(string name) =>
        BareName(name).Equals(name, StringComparison.Ordinal)
            ? ToFileName(name + ".nav")
            : ToFileName(name);

    /// <summary>
    /// The file a name stands for, if one is there. A name that carries its
    /// extension asks for exactly that file and gets it whenever it is there,
    /// even with a route of the other form under the same name beside it. A
    /// bare name, or a full name whose file is not there, means the dropped
    /// ".nav" first and the plugin's own ".af" second, the way the reference
    /// reads a route name. A route saves back to the file it came from, in
    /// that file's form, so an edit is what loads again by the same name.
    /// Where both forms sit side by side the ".af" goes by its full name
    /// (see <see cref="NameOf(string)"/>), so the selection and the pickers
    /// still reach it. With <paramref name="exactOnly"/> a name that carries
    /// its extension reaches that file or nothing.
    /// </summary>
    private string? ResolveExisting(string name, bool exactOnly = false)
    {
        if (!VtankStorage.IsAvailable)
            return null;
        string bareName = BareName(name);
        if (!bareName.Equals(name, StringComparison.Ordinal))
        {
            string exact = $"{VtankProfileDirectory.NavFolder}/{name}";
            if (VtankStorage.ReadText(exact) is not null)
                return exact;
            if (exactOnly)
                return null;
        }
        string dropped = DroppedFileName(bareName);
        if (VtankStorage.ReadText(dropped) is not null)
            return dropped;
        string own = ToFileName(bareName);
        return VtankStorage.ReadText(own) is not null ? own : null;
    }

    /// <summary>A route's name without the extension it may carry.</summary>
    internal static string BareName(string name)
    {
        foreach (string extension in NameExtensions)
        {
            if (name.Length > extension.Length
                && name.EndsWith(extension, StringComparison.OrdinalIgnoreCase))
            {
                return name[..^extension.Length];
            }
        }
        return name;
    }

    private static readonly string[] NameExtensions = [".nav", ".af"];

    private static bool IsDroppedForeignFormat(string key) =>
        key.EndsWith(".nav", StringComparison.OrdinalIgnoreCase);

    private void WriteBinding()
    {
        if (!CanBindFiles || !VtankStorage.IsAvailable)
            return;
        VtankProfileDirectory.VtankCharacterBinding existing =
            VtankProfileDirectory.TryReadCharacterBinding(VtankStorage, _characterName, Server)
            ?? new VtankProfileDirectory.VtankCharacterBinding(
                string.Empty, string.Empty, CurrentFileName(), null);
        VtankProfileDirectory.WriteCharacterBinding(
            VtankStorage,
            _characterName,
            Server,
            existing with { NavFileName = CurrentFileName() });
    }

    /// <summary>
    /// Writes a route to its file in that file's own form: the older ".nav"
    /// form for a ".nav", the plugin's own for anything else. A route the
    /// form cannot hold is not written at all, and <see cref="SaveNotice"/>
    /// says why.
    /// </summary>
    private bool WriteRoute(string fileName, NavigationSettings settings)
    {
        string text;
        try
        {
            text = IsDroppedForeignFormat(fileName)
                ? VtankNavRouteSerializer.Save(settings)
                : MetafSerializer.SaveNav(settings);
        }
        catch (InvalidOperationException error)
        {
            SaveNotice = $"Route profile {NameOf(fileName)} was NOT saved: {error.Message}";
            _host.Log.Warn(SaveNotice);
            return false;
        }
        if (!VtankStorage.IsAvailable)
            return false;
        try
        {
            VtankStorage.WriteText(
                fileName,
                ProfileLineEndings.Match(text, VtankStorage.ReadText(fileName)));
        }
        catch (Exception error)
        {
            SaveNotice = $"Route profile {NameOf(fileName)} could not be saved: {error.Message}";
            _host.Log.Warn(SaveNotice);
            return false;
        }
        SaveNotice = null;
        return true;
    }

    private static string Normalize(string? name) => name?.Trim() ?? string.Empty;

    private static string LegacyProfileKey(string value, bool byCharacter)
    {
        string identity = (byCharacter ? "char:" : "named:") + value.Trim().ToUpperInvariant();
        string hash = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(identity)));
        return $"profiles/route/{hash}.json";
    }

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNameCaseInsensitive = true,
    };

    private sealed class LegacyRosterDocument
    {
        public List<string> Names { get; set; } = [];
    }

    private sealed class LegacyRouteDocument
    {
        public int Version { get; set; } = 1;
        public bool Enabled { get; set; }
        public bool Priority { get; set; }
        public RouteMode Mode { get; set; } = RouteMode.Circular;
        public double MinimumDistanceMeters { get; set; } = 2d;
        public uint FollowTargetObjectId { get; set; }
        public string FollowTargetName { get; set; } = string.Empty;
        public bool FollowAroundCorners { get; set; } = true;
        public bool OpenDoors { get; set; }
        public double DoorIdentifyRangeMeters { get; set; } = 20d;
        public double DoorOpenRangeMeters { get; set; } = 4d;
        public int DoorLockpickExcessThreshold { get; set; } = -50;
        public LegacyWaypointDocument[] Waypoints { get; set; } = [];

        public void ApplyRouteOnly(NavigationSettings value)
        {
            value.Mode = Enum.IsDefined(Mode) ? Mode : RouteMode.Circular;
            value.FollowTargetObjectId = FollowTargetObjectId;
            value.FollowTargetName = FollowTargetName ?? string.Empty;
            value.Waypoints.Clear();
            foreach (LegacyWaypointDocument waypoint in Waypoints ?? [])
                value.Waypoints.Add(waypoint.ToWaypoint());
        }
    }

    private sealed class LegacyWaypointDocument
    {
        public RouteWaypointType Type { get; set; }
        public uint CellId { get; set; }
        public double EastWest { get; set; }
        public double NorthSouth { get; set; }
        public double Elevation { get; set; }
        public float HeadingDegrees { get; set; }
        public bool IsOutdoor { get; set; }
        public uint ReferenceCellId { get; set; }
        public double ReferenceEastWest { get; set; }
        public double ReferenceNorthSouth { get; set; }
        public double ReferenceElevation { get; set; }
        public float ReferenceHeadingDegrees { get; set; }
        public bool ReferenceIsOutdoor { get; set; }
        public uint ObjectId { get; set; }
        public string ObjectName { get; set; } = string.Empty;
        public int LegacyObjectClass { get; set; }
        public bool LegacyReferenceValid { get; set; } = true;
        public string Text { get; set; } = string.Empty;
        public int DurationMilliseconds { get; set; } = 5000;
        public int Recall { get; set; }
        public uint RecallSpellId { get; set; }
        public string RecallSpellName { get; set; } = string.Empty;
        public float JumpHeadingDegrees { get; set; }
        /// <summary>
        /// The jump's shift flag, under the name these older documents
        /// stored it by; carried over bit for bit, as the route files do.
        /// </summary>
        public bool JumpRun { get; set; }
        public int JumpChargeMilliseconds { get; set; } = 1000;
        public RouteJumpDirection JumpDirection { get; set; }

        public RouteWaypoint ToWaypoint()
        {
            RouteRecallKind recall = MapLegacyRecall(Recall);
            return new RouteWaypoint
            {
                Type = Enum.IsDefined(Type) ? Type : RouteWaypointType.Point,
                Position = new PluginNavigationPosition(
                    CellId, EastWest, NorthSouth, Elevation, HeadingDegrees, IsOutdoor),
                ReferencePosition = new PluginNavigationPosition(
                    ReferenceCellId,
                    ReferenceEastWest,
                    ReferenceNorthSouth,
                    ReferenceElevation,
                    ReferenceHeadingDegrees,
                    ReferenceIsOutdoor),
                ObjectId = ObjectId,
                ObjectName = ObjectName ?? string.Empty,
                LegacyObjectClass = LegacyObjectClass,
                LegacyReferenceValid = LegacyReferenceValid,
                Text = Text ?? string.Empty,
                DurationMilliseconds = Math.Clamp(DurationMilliseconds, 0, 3_600_000),
                Recall = recall,
                RecallSpellId = RouteWaypoint.SpellIdForRecall(recall),
                RecallSpellName = RouteWaypoint.RecallDisplayName(recall),
                JumpHeadingDegrees = float.IsFinite(JumpHeadingDegrees) ? JumpHeadingDegrees : 0f,
                JumpHoldShift = JumpRun,
                JumpChargeMilliseconds = JumpChargeMilliseconds,
                JumpDirection = Enum.IsDefined(JumpDirection) ? JumpDirection : RouteJumpDirection.Forward,
            };
        }

        private static RouteRecallKind MapLegacyRecall(int legacyOrdinal) => legacyOrdinal switch
        {
            0 => RouteRecallKind.LifestoneRecall,        // old Lifestone
            1 => RouteRecallKind.Marketplace,             // old Marketplace
            2 => RouteRecallKind.PrimaryPortalRecall,     // old PrimaryPortal
            3 => RouteRecallKind.SecondaryPortalRecall,   // old SecondaryPortal
            _ => RouteRecallKind.PrimaryPortalRecall,
        };
    }
}
