using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class MossTankMetaProfileStore
{
    public const string ByCharacter = "By char";

    private const string LegacyRosterKey = "profiles/meta/index.json";

    private readonly IPluginHost _host;
    private string _character = string.Empty;
    private string _selected = ByCharacter;
    private string? _pendingLegacyBareName;
    private bool _rosterSwept;

    public MossTankMetaProfileStore(IPluginHost host)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
    }

    public string Selected => NameOf(_selected);
    public string? RecoveryNotice { get; private set; }

    /// <summary>The file the last load read, or tried to read.</summary>
    public string? LastLoadKey { get; private set; }

    /// <summary>Whether the last load found and read that file.</summary>
    public bool LastLoadSucceeded { get; private set; }

    /// <summary>
    /// Why the last load could not read its file, or null when it read it or
    /// found no file there.
    /// </summary>
    public string? LastLoadError { get; private set; }
    public string? SaveNotice { get; private set; }

    private string Server => _host.Automation.Character.WorldName;
    private IPluginStorage VtankStorage => _host.VtankProfiles;
    private bool CanBindFiles => _character.Length > 0 && Server.Length > 0;

    private const string FolderPrefix = VtankProfileDirectory.MetaFolder + "/";

    /// <summary>
    /// A meta named in the plugin's own format, unless the name says
    /// otherwise: a file dropped into the folder as the older ".met" form is
    /// addressed exactly as it sits there, so picking it loads it.
    /// </summary>
    private static string ToFileName(string name)
    {
        string bareName = VtankProfileDirectory.StripFolder(
            name, VtankProfileDirectory.MetaFolder);
        if (!bareName.EndsWith(".af", StringComparison.OrdinalIgnoreCase)
            && !bareName.EndsWith(".met", StringComparison.OrdinalIgnoreCase))
        {
            bareName += ".af";
        }
        return $"{VtankProfileDirectory.MetaFolder}/{bareName}";
    }

    /// <summary>
    /// The file a meta written under a new name goes to: the older ".met"
    /// form for a bare name, or exactly the file a name with its extension
    /// says.
    /// </summary>
    private static string NewFileName(string name) =>
        BareName(name).Equals(name, StringComparison.Ordinal)
            ? ToFileName(name + ".met")
            : ToFileName(name);

    /// <summary>
    /// The file a name stands for, if one is there. A name that carries its
    /// extension asks for exactly that file and gets it whenever it is there,
    /// even with a meta of the other form under the same name beside it. A
    /// bare name, or a full name whose file is not there, means the dropped
    /// ".met" first and the plugin's own ".af" second, the way the reference
    /// reads a meta name. Where both sit side by side the ".af" goes by its
    /// full name (see <see cref="NameOf(string)"/>), so the selection and
    /// the pickers still reach it. With <paramref name="exactOnly"/> a name
    /// that carries its extension reaches that file or nothing.
    /// </summary>
    private string? ResolveExisting(string name, bool exactOnly = false)
    {
        if (!VtankStorage.IsAvailable)
            return null;
        string bareName = BareName(name);
        if (!bareName.Equals(name, StringComparison.Ordinal))
        {
            string exact = ToFileName(name);
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

    /// <summary>A meta's name without the extension it may carry.</summary>
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

    private static readonly string[] NameExtensions = [".met", ".af"];

    private static bool IsDroppedForeignFormat(string key) =>
        key.EndsWith(".met", StringComparison.OrdinalIgnoreCase);

    /// <summary>
    /// The name a file goes by, which is also the name that loads it again.
    /// A bare name loads the ".met" first, so a ".met" goes by its bare name;
    /// so does the plugin's own ".af", except beside a ".met" of the same
    /// name, where it keeps its extension so picking it by that name reaches
    /// it rather than the file beside it.
    /// </summary>
    private string NameOf(string fileName) => NameOf(fileName, DroppedMetaExists);

    private static string NameOf(string fileName, Func<string, bool> droppedExists)
    {
        if (fileName.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
            return fileName;
        string value = fileName.StartsWith(FolderPrefix, StringComparison.Ordinal)
            ? fileName[FolderPrefix.Length..]
            : fileName;
        if (value.EndsWith(".met", StringComparison.OrdinalIgnoreCase))
            return value[..^".met".Length];
        if (!value.EndsWith(".af", StringComparison.OrdinalIgnoreCase))
            return value;
        string bareName = value[..^3];
        return droppedExists(bareName) ? value : bareName;
    }

    private bool DroppedMetaExists(string bareName) =>
        VtankStorage.IsAvailable && VtankStorage.ReadText(DroppedFileName(bareName)) is not null;

    /// <summary>Where a meta dropped in as the older ".met" form sits.</summary>
    private static string DroppedFileName(string bareName) =>
        $"{VtankProfileDirectory.MetaFolder}/{bareName}.met";

    public IReadOnlyList<string> AvailableNames
    {
        get
        {
            IReadOnlyList<VtankProfileDirectory.ProfileEntry> entries =
                VtankProfileDirectory.ListMetaProfiles(VtankStorage);
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
        if (normalized.Equals(_character, StringComparison.OrdinalIgnoreCase))
            return false;
        _character = normalized;
        _pendingLegacyBareName = null;
        VtankProfileDirectory.VtankCharacterBinding? binding = CanBindFiles
            ? VtankProfileDirectory.TryReadCharacterBinding(VtankStorage, _character, Server)
            : null;
        _selected = binding is { MetaFileName.Length: > 0 } bound
            ? bound.MetaFileName
            : ByCharacter;
        return true;
    }

    public MetaProfile LoadCurrent()
    {
        SweepLegacyRosterIfNeeded();
        MigrateLegacyIfNeeded();
        string fileName = CurrentFileName();
        LastLoadKey = fileName;
        LastLoadSucceeded = false;
        LastLoadError = null;
        string? text = VtankStorage.IsAvailable ? VtankStorage.ReadText(fileName) : null;
        if (text is null)
            return new MetaProfile();
        if (IsDroppedForeignFormat(fileName))
        {
            if (VtankMetaProfileSerializer.TryLoad(
                    text, _host.Automation.Spells, out MetaProfile dropped, out string metError))
            {
                LastLoadSucceeded = true;
                return dropped;
            }
            LastLoadError = metError;
            RecoveryNotice = MossTankProfileRecovery.Preserve(
                _host, "meta", fileName, text, new FormatException(metError));
            _host.Log.Warn(RecoveryNotice);
            return new MetaProfile();
        }
        if (!MetafSerializer.TryLoadMeta(text, _host.Automation.Spells, out MetaProfile profile, out string error))
        {
            LastLoadError = error;
            RecoveryNotice = MossTankProfileRecovery.Preserve(
                _host, "meta", fileName, text, new FormatException(error));
            _host.Log.Warn(RecoveryNotice);
            return new MetaProfile();
        }
        LastLoadSucceeded = true;
        return profile;
    }

    /// <summary>
    /// The automatic, per-character file of a character nobody has named yet.
    /// There is no such file: a name arrives some way into a login and goes
    /// away again during a relog, and a file filed under the gap between
    /// them is one the character, once named, never reads.
    /// </summary>
    private bool FilesUnderNobody =>
        _character.Length == 0
        && _selected.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase);

    /// <summary>
    /// Writes the meta back to the file it was loaded from, in that file's
    /// own form: a ".met" stays a ".met". False, with <see cref="SaveNotice"/>
    /// saying why, when nothing was written.
    /// </summary>
    public bool SaveCurrent(MetaProfile profile)
    {
        if (FilesUnderNobody)
            return false;
        return SaveTo(CurrentFileName(), profile, out _);
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
            && _host.Storage.ReadText(LegacyNamedKey(bareName)) is not null)
        {
            _selected = ToFileName(bareName);
            _pendingLegacyBareName = bareName;
            WriteBinding();
            return true;
        }
        return false;
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
            && _host.Storage.ReadText(LegacyNamedKey(BareName(normalized))) is not null;
    }

    /// <summary>
    /// Writes a meta under a new name and selects it. A bare name is written
    /// in the older ".met" form, as the reference writes a meta; a name that
    /// carries ".met" or ".af" is written exactly as it says.
    /// </summary>
    public bool Create(
        string? name,
        bool copyCurrent,
        MetaProfile current,
        out string notice)
    {
        string normalized = Normalize(name);
        if (normalized.Length is < 1 or > 64
            || normalized.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
        {
            notice = "Enter a unique Meta profile name (1-64 characters).";
            return false;
        }
        string fileName = NewFileName(normalized);
        MetaProfile document = copyCurrent ? Clone(current) : new MetaProfile();
        if (!SaveTo(fileName, document, out notice))
            return false;
        _selected = fileName;
        _pendingLegacyBareName = null;
        WriteBinding();
        notice = copyCurrent
            ? $"Copied Meta profile to {NameOf(fileName)}."
            : $"Created Meta profile {NameOf(fileName)}.";
        return true;
    }

    /// <summary>Whether a VTank .met of this name waits in imports or exports.</summary>
    public bool LegacyImportExists(string? name)
    {
        string normalized = Normalize(name);
        return _host.Storage.IsAvailable
            && normalized.Length != 0
            && FindLegacyImport(normalized) is not null;
    }

    private string? FindLegacyImport(string normalized) =>
        _host.Storage.List("imports")
            .Concat(_host.Storage.List("exports"))
            .FirstOrDefault(candidate =>
                candidate.EndsWith(".met", StringComparison.OrdinalIgnoreCase)
                && Path.GetFileNameWithoutExtension(candidate).Equals(
                    normalized,
                    StringComparison.OrdinalIgnoreCase));

    public bool TryImportLegacy(
        string? name,
        out MetaProfile profile,
        out string notice)
    {
        string normalized = Normalize(name);
        if (!_host.Storage.IsAvailable || normalized.Length == 0)
        {
            profile = new MetaProfile();
            notice = "Legacy Meta storage is unavailable.";
            return false;
        }
        string? key = FindLegacyImport(normalized);
        string? source = key is null ? null : _host.Storage.ReadText(key);
        if (string.IsNullOrWhiteSpace(source))
        {
            profile = new MetaProfile();
            notice = $"VTank Meta file '{normalized}.met' was not found in imports.";
            return false;
        }
        if (!VtankMetaProfileSerializer.TryLoad(
                source, _host.Automation.Spells, out profile, out string error))
        {
            notice = $"Could not import {Path.GetFileName(key)}: {error}";
            return false;
        }
        string fileName = ToFileName(normalized);
        if (!SaveTo(fileName, profile, out string saveNotice))
        {
            notice = saveNotice;
            return false;
        }
        _selected = fileName;
        _pendingLegacyBareName = null;
        WriteBinding();
        notice = $"Imported VTank Meta profile {NameOf(fileName)}.";
        return true;
    }

    public bool Delete(out string notice)
    {
        if (_selected.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase))
        {
            notice = "'By char' is the built-in Meta profile and cannot be deleted.";
            return false;
        }
        string fileName = _selected;
        if (VtankStorage.IsAvailable)
            VtankStorage.Delete(fileName);
        _selected = ByCharacter;
        _pendingLegacyBareName = null;
        WriteBinding();
        notice = $"Deleted Meta profile {NameOf(fileName)}.";
        return true;
    }

    public MetaProfile ClearCurrent()
    {
        var empty = new MetaProfile();
        SaveCurrent(empty);
        return empty;
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

            string legacyKey = LegacyNamedKey(name);
            MetaProfile? legacy = ReadLegacyJson(legacyKey);
            if (legacy is null)
                continue;

            string bare = name.EndsWith(".af", StringComparison.OrdinalIgnoreCase)
                ? name
                : name + ".af";
            string fileName = $"{VtankProfileDirectory.MetaFolder}/{bare}";
            if (VtankStorage.IsAvailable && VtankStorage.ReadText(fileName) is null)
            {
                if (!SaveTo(fileName, legacy, out string notice))
                {
                    // Representational loss (a disabled rule) — same gate as
                    // MigrateLegacyIfNeeded: keep the row so this can be
                    // retried once the user resolves it.
                    _host.Log.Warn($"MossTank could not sweep legacy Meta profile '{name}': {notice}");
                    remaining.Add(name);
                    continue;
                }
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
        _host.Log.Warn($"Migrated {migrated} legacy MossTank named Meta profile(s) from the old roster.");
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
            RecoveryNotice = MossTankProfileRecovery.Preserve(
                _host, "meta", LegacyRosterKey, json, error);
            _host.Log.Warn(RecoveryNotice);
            return null;
        }
    }

    private void MigrateLegacyIfNeeded()
    {
        string fileName = CurrentFileName();
        if (!VtankStorage.IsAvailable || VtankStorage.ReadText(fileName) is not null)
        {
            _pendingLegacyBareName = null;
            return;
        }
        string legacyKey = _pendingLegacyBareName is { Length: > 0 } bareName
            ? LegacyNamedKey(bareName)
            : LegacyByCharacterKey();
        MetaProfile? legacy = ReadLegacyJson(legacyKey);
        if (legacy is null)
        {
            _pendingLegacyBareName = null;
            return;
        }
        if (!SaveTo(fileName, legacy, out string notice))
        {
            // Representational loss (a disabled rule) blocks the migration
            // outright rather than silently dropping it: the legacy JSON
            // stays put (still fully readable) until the user resolves it.
            _host.Log.Warn($"MossTank could not migrate legacy Meta profile: {notice}");
            _pendingLegacyBareName = null;
            return;
        }
        if (_host.Storage.IsAvailable)
            _host.Storage.Delete(legacyKey);
        _pendingLegacyBareName = null;
        _host.Log.Warn($"Migrated legacy MossTank Meta profile '{legacyKey}' to '{fileName}'.");
    }

    private MetaProfile? ReadLegacyJson(string key)
    {
        if (!_host.Storage.IsAvailable)
            return null;
        string? json = null;
        try
        {
            json = _host.Storage.ReadText(key);
            return string.IsNullOrWhiteSpace(json)
                ? null
                : JsonSerializer.Deserialize<MetaProfile>(json, JsonOptions);
        }
        catch (Exception error)
        {
            RecoveryNotice = MossTankProfileRecovery.Preserve(_host, "meta", key, json, error);
            _host.Log.Error(RecoveryNotice, error);
            return null;
        }
    }

    /// <summary>
    /// Writes a meta to a file in that file's own form: the older ".met"
    /// form for a ".met", the plugin's own for anything else, with the line
    /// breaks the file already has. A meta the form cannot hold is not
    /// written at all, and the notice says why.
    /// </summary>
    private bool SaveTo(string fileName, MetaProfile profile, out string notice)
    {
        string text;
        try
        {
            text = IsDroppedForeignFormat(fileName)
                ? VtankMetaProfileSerializer.Save(profile)
                : MetafSerializer.SaveMeta(profile);
        }
        catch (InvalidOperationException error)
        {
            notice = $"Meta profile '{fileName}' was NOT saved: {error.Message}";
            SaveNotice = notice;
            _host.Log.Warn(notice);
            return false;
        }
        if (!VtankStorage.IsAvailable)
        {
            notice = "VTank Meta storage is unavailable.";
            return false;
        }
        try
        {
            VtankStorage.WriteText(
                fileName,
                ProfileLineEndings.Match(text, VtankStorage.ReadText(fileName)));
        }
        catch (Exception error)
        {
            notice = $"Meta profile '{fileName}' could not be saved: {error.Message}";
            SaveNotice = notice;
            _host.Log.Warn(notice);
            return false;
        }
        SaveNotice = null;
        notice = string.Empty;
        return true;
    }

    // ------------------------------------------------------------------
    // File naming, storage plumbing.
    // ------------------------------------------------------------------

    private string CurrentFileName() => _selected.Equals(ByCharacter, StringComparison.OrdinalIgnoreCase)
        ? $"{VtankProfileDirectory.MetaFolder}/{VtankProfileDirectory.AutoCharacterFileName(_character, Server, "af")}"
        : _selected;

    private void WriteBinding()
    {
        if (!CanBindFiles || !VtankStorage.IsAvailable)
            return;
        VtankProfileDirectory.VtankCharacterBinding existing =
            VtankProfileDirectory.TryReadCharacterBinding(VtankStorage, _character, Server)
            ?? new VtankProfileDirectory.VtankCharacterBinding(
                string.Empty, string.Empty, string.Empty, CurrentFileName());
        VtankProfileDirectory.WriteCharacterBinding(
            VtankStorage,
            _character,
            Server,
            existing with { MetaFileName = CurrentFileName() });
    }

    private static MetaProfile Clone(MetaProfile profile) =>
        JsonSerializer.Deserialize<MetaProfile>(
            JsonSerializer.Serialize(profile, JsonOptions),
            JsonOptions) ?? new MetaProfile();

    private static string Normalize(string? name) => name?.Trim() ?? string.Empty;

    private string LegacyByCharacterKey() =>
        $"profiles/meta/by-character/{Hash(_character)}.json";

    private static string LegacyNamedKey(string name) =>
        $"profiles/meta/named/{Hash(name)}.json";

    private static string Hash(string value)
    {
        byte[] hash = SHA256.HashData(Encoding.UTF8.GetBytes(value.ToLowerInvariant()));
        return Convert.ToHexString(hash.AsSpan(0, 12)).ToLowerInvariant();
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
}
