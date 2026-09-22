using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Where the store says a file would not read.</summary>
internal interface IUbSettingLog
{
    /// <summary>One line a person should see.</summary>
    void Warn(string message);
}

/// <summary>A log that says nothing, for a session with nobody to tell.</summary>
internal sealed class NullUbSettingLog : IUbSettingLog
{
    /// <summary>The shared instance; this type holds no state.</summary>
    internal static NullUbSettingLog Instance { get; } = new();

    private NullUbSettingLog() { }

    /// <inheritdoc />
    public void Warn(string message)
    {
    }
}

/// <summary>
/// The three files behind the catalogue, written into the shared profile
/// directory under one prefix of their own.
/// </summary>
/// <remarks>
/// <para>
/// The layout mirrors where the tools being brought over keep their files,
/// so a profile file copied from an existing setup is found where it is
/// looked for:
/// </para>
/// <code>
/// mosstank/ub/settings.json                        the installation
/// mosstank/ub/profiles/&lt;name&gt;.settings.json   a named, shareable set
/// mosstank/ub/&lt;Server&gt;/&lt;Character&gt;/settings.json  this character
/// mosstank/ub/&lt;folder&gt;/&lt;file&gt;            a profile file, at three depths
/// </code>
/// <para>
/// The three settings files are the exception, and copying one in does
/// not work. They sit where the original keeps its own, but they are this
/// plugin's flat map of name to text, not the nested typed document the
/// original writes under that name. One of those copied in here is
/// exactly the damaged-file case below: it is reported, the defaults
/// stand, and it is left where it is rather than replaced.
/// </para>
/// <para>
/// Everything sits under <c>mosstank/ub/</c>, beside the profiles, metas,
/// navs and loot files: a person edits, copies and shares these, so they
/// belong in the directory they can find rather than in the plugin's own
/// storage, which holds only state the plugin keeps for itself. An earlier
/// version wrote the same tree at <c>ub/</c> in that storage, and a
/// one-time copy brings it across.
/// </para>
/// <para>
/// A value is stored as the text the value shape reads back, not as typed
/// JSON, because the shape is the catalogue's to know and a string
/// round-trips every shape exactly. Keys the running version does not
/// recognise are kept and written back untouched, so a file from a later
/// version is not quietly stripped by opening the page once.
/// </para>
/// </remarks>
internal sealed class UbSettingStore : IUbSettingValues
{
    /// <summary>
    /// Everything this store writes lives under this prefix, inside the
    /// shared profile directory rather than the plugin's own storage: these
    /// are files a person edits, copies between installations and shares,
    /// so they belong beside the profiles, metas, navs and loot files that
    /// already sit there.
    /// </summary>
    internal const string Root = VtankProfileDirectory.Root + "/ub/";

    /// <summary>The profile opened when nobody has chosen one.</summary>
    internal const string DefaultProfileName = "default";

    /// <summary>
    /// The key the chosen profile name is kept under. It starts with a
    /// character no dotted setting name can start with, so it can share the
    /// character's file without ever appearing as a setting.
    /// </summary>
    private const string ProfileNameKey = "$profile";

    private const string DefaultProfileFileName = "default.utl";

    private static readonly JsonSerializerOptions Options = new()
    {
        WriteIndented = true,
    };

    private readonly IPluginStorage _storage;
    private readonly IUbSettingLog _log;
    private readonly Dictionary<UbSettingScope, Tier> _tiers = new()
    {
        [UbSettingScope.Global] = new Tier(),
        [UbSettingScope.Profile] = new Tier(),
        [UbSettingScope.Character] = new Tier(),
    };

    /// <summary>
    /// Files of plain lines kept beside the settings files, by key, read
    /// once and then served from here until the character changes.
    /// </summary>
    private readonly Dictionary<string, List<string>> _lineFiles = new(StringComparer.Ordinal);

    private string _server = string.Empty;
    private string _character = string.Empty;
    private string _profile = DefaultProfileName;

    internal UbSettingStore(IPluginStorage storage, IUbSettingLog log)
    {
        _storage = storage ?? throw new ArgumentNullException(nameof(storage));
        _log = log ?? throw new ArgumentNullException(nameof(log));
    }

    /// <summary>The named profile in use.</summary>
    internal string ProfileName => _profile;

    /// <summary>
    /// The server and character the character tier writes for. Called again
    /// on every login, because the same session can carry more than one.
    /// </summary>
    internal UbSettingStore Bind(string? server, string? character)
    {
        string newServer = Safe(server);
        string newCharacter = Safe(character);
        if (newServer == _server && newCharacter == _character)
            return this;

        _server = newServer;
        _character = newCharacter;
        _tiers[UbSettingScope.Character].Forget();
        _lineFiles.Clear();
        // Which profile this character opens is that character's own choice,
        // so it is only known once the character is.
        _profile = ReadRaw(UbSettingScope.Character, ProfileNameKey) is { Length: > 0 } chosen
            ? chosen
            : DefaultProfileName;
        _tiers[UbSettingScope.Profile].Forget();
        return this;
    }

    /// <summary>Opens a named profile, and remembers the choice for this character.</summary>
    internal void SelectProfile(string name)
    {
        string chosen = Safe(name);
        if (chosen.Length == 0)
            chosen = DefaultProfileName;
        if (chosen == _profile)
            return;
        _profile = chosen;
        _tiers[UbSettingScope.Profile].Forget();
        WriteRaw(UbSettingScope.Character, ProfileNameKey, chosen);
    }

    /// <summary>
    /// Makes a named profile and opens it. The file is written even when it
    /// is empty, because a profile nothing has written is a profile nothing
    /// can list, and a menu can only ever offer what it can list.
    /// </summary>
    /// <param name="name">What to call it.</param>
    /// <param name="copyCurrent">
    /// Whether the open profile's values go into it. Copying is what makes
    /// the tier shareable in practice: set a client up, then hand a copy on
    /// under its own name without disturbing the one in use.
    /// </param>
    /// <param name="notice">One line for whoever pressed the button.</param>
    /// <returns>Whether it was made.</returns>
    internal bool CreateProfile(string name, bool copyCurrent, out string notice)
    {
        string chosen = Safe(name);
        if (chosen.Length == 0)
        {
            notice = "Type a name for the new profile first.";
            return false;
        }
        if (AvailableProfiles().Contains(chosen, StringComparer.OrdinalIgnoreCase))
        {
            notice = $"A profile called '{chosen}' already exists.";
            return false;
        }

        // Read what is being copied before the tier is pointed elsewhere.
        var carried = new Dictionary<string, string>(
            copyCurrent
                ? Load(UbSettingScope.Profile).Values
                : [],
            StringComparer.OrdinalIgnoreCase);

        _profile = chosen;
        Tier tier = _tiers[UbSettingScope.Profile];
        tier.Forget();
        tier.Loaded = true;
        foreach ((string key, string text) in carried)
            tier.Values[key] = text;
        Save(UbSettingScope.Profile, tier);
        WriteRaw(UbSettingScope.Character, ProfileNameKey, chosen);

        notice = copyCurrent
            ? $"Profile '{chosen}' is open, holding a copy of what the last one held."
            : $"Profile '{chosen}' is open.";
        return true;
    }

    /// <summary>The named profiles already written, in name order.</summary>
    internal IReadOnlyList<string> AvailableProfiles()
    {
        const string suffix = ".settings.json";
        var names = new List<string> { DefaultProfileName };
        if (_storage.IsAvailable)
        {
            foreach (string key in _storage.List(Root + "profiles"))
            {
                string file = key[(key.LastIndexOf('/') + 1)..];
                if (!file.EndsWith(suffix, StringComparison.OrdinalIgnoreCase))
                    continue;
                string name = file[..^suffix.Length];
                if (name.Length != 0 && !names.Contains(name, StringComparer.OrdinalIgnoreCase))
                    names.Add(name);
            }
        }
        names.Sort(StringComparer.OrdinalIgnoreCase);
        return names;
    }

    /// <inheritdoc />
    public UbSettingValue? Read(UbSettingScope scope, string name)
    {
        if (name.StartsWith('$'))
            return null;
        return ReadRaw(scope, name) is { } text
            && UbSettingValue.TryParse(UbSettingDefinitions.KindOf(name), text, out UbSettingValue value)
                ? value
                : null;
    }

    /// <inheritdoc />
    public void Write(UbSettingScope scope, string name, UbSettingValue value) =>
        WriteRaw(scope, name, value.ToStorageString());

    /// <inheritdoc />
    public bool IsWritable(UbSettingScope scope) =>
        KeyOf(scope) is not null && _storage.IsAvailable && !Load(scope).ReadOnly;

    /// <inheritdoc />
    public bool Clear(UbSettingScope scope, string name)
    {
        Tier tier = Load(scope);
        if (!tier.Values.Remove(name))
            return false;
        Save(scope, tier);
        return true;
    }

    /// <summary>
    /// Where a named profile file is, looked for the way the tools that read
    /// one look for it: this character's folder, then this server's, then
    /// the installation's, and then the same three again for a file called
    /// <c>default.utl</c>. When none of the six exists the answer is where a
    /// new one would be written.
    /// </summary>
    internal string ResolveProfileKey(string folder, string fileName)
    {
        string safeFolder = Safe(folder);
        string safeFile = Safe(fileName);
        string character = CharacterFolder() is { Length: > 0 } own
            ? $"{own}{safeFolder}/"
            : string.Empty;
        string server = _server.Length > 0 ? $"{Root}{_server}/{safeFolder}/" : string.Empty;
        string plugin = $"{Root}{safeFolder}/";

        foreach (string? candidate in new[]
        {
            character.Length > 0 ? character + safeFile : null,
            server.Length > 0 ? server + safeFile : null,
            plugin + safeFile,
            character.Length > 0 ? character + DefaultProfileFileName : null,
            server.Length > 0 ? server + DefaultProfileFileName : null,
            plugin + DefaultProfileFileName,
        })
        {
            if (candidate is not null && Exists(candidate))
                return candidate;
        }
        return plugin + safeFile;
    }

    /// <summary>
    /// Where a profile file of this character's own goes: the character's
    /// folder when one is bound, otherwise the installation's.
    /// </summary>
    internal string CharacterProfileKey(string folder, string fileName)
    {
        string safeFolder = Safe(folder);
        string safeFile = Safe(fileName);
        string character = CharacterFolder();
        return (character.Length > 0 ? $"{character}{safeFolder}/" : $"{Root}{safeFolder}/") + safeFile;
    }

    /// <summary>
    /// The folders a profile file is looked for in, in lookup order: the
    /// character's, the server's and the installation's, each with a
    /// trailing slash; only the ones that can be named are listed.
    /// </summary>
    internal IReadOnlyList<string> ProfileFolderPrefixes(string folder)
    {
        string safeFolder = Safe(folder);
        var prefixes = new List<string>(3);
        if (CharacterFolder() is { Length: > 0 } character)
            prefixes.Add($"{character}{safeFolder}/");
        if (_server.Length > 0)
            prefixes.Add($"{Root}{_server}/{safeFolder}/");
        prefixes.Add($"{Root}{safeFolder}/");
        return prefixes;
    }

    // ---- The alias file ---------------------------------------------------
    //
    // The alias list is not a settings value: it is its own file, the way
    // the tools being brought over keep it, so a character can point at a
    // named set that other characters share while its settings stay its
    // own. The alias profile setting names the file; this is the map from
    // that name to the key.

    /// <summary>The alias profile value that means the character's own file.</summary>
    internal const string CharacterAliasProfile = "[character]";

    private const string AliasFileName = "aliases.txt";

    /// <summary>
    /// Where the alias list of one alias profile is: the character's own
    /// file for <see cref="CharacterAliasProfile"/> (or a blank name), and
    /// <c>ub/profiles/&lt;name&gt;.aliases.txt</c> for a named one. Null when
    /// the character's file is asked for before a character is known.
    /// </summary>
    internal string? AliasFileKey(string? profile)
    {
        string name = Safe(profile);
        if (name.Length == 0 || name.Equals(CharacterAliasProfile, StringComparison.OrdinalIgnoreCase))
            return CharacterFolder() is { Length: > 0 } folder ? folder + AliasFileName : null;
        return $"{Root}profiles/{name}.{AliasFileName}";
    }

    /// <summary>
    /// The lines of one file, trimmed, blank ones left out. Empty when
    /// there is no such file, no key, or nowhere to read from.
    /// </summary>
    internal IReadOnlyList<string> ReadLines(string? key)
    {
        if (key is null)
            return [];
        if (_lineFiles.TryGetValue(key, out List<string>? cached))
            return cached;
        var lines = new List<string>();
        if (_storage.IsAvailable)
        {
            try
            {
                string? text = _storage.ReadText(key);
                if (text is not null)
                    foreach (string line in text.Split('\n'))
                    {
                        string trimmed = line.Trim();
                        if (trimmed.Length != 0)
                            lines.Add(trimmed);
                    }
            }
            catch (Exception error)
            {
                _log.Warn($"MossTank could not read {key} ({error.Message}); it is treated as empty.");
            }
        }
        _lineFiles[key] = lines;
        return lines;
    }

    /// <summary>
    /// Writes one file of lines, one a line. False when there is no key or
    /// nowhere to write, in which case the lines are kept for this session
    /// so the page still shows what was typed.
    /// </summary>
    internal bool WriteLines(string? key, IEnumerable<string> lines)
    {
        ArgumentNullException.ThrowIfNull(lines);
        if (key is null)
            return false;
        var kept = lines
            .Select(static line => line.Trim())
            .Where(static line => line.Length != 0)
            .ToList();
        _lineFiles[key] = kept;
        if (!_storage.IsAvailable)
            return false;
        try
        {
            _storage.WriteText(key, string.Concat(kept.Select(static line => line + '\n')));
            return true;
        }
        catch (Exception error)
        {
            _log.Warn($"MossTank could not save {key}: {error.Message}");
            return false;
        }
    }

    private bool Exists(string key) =>
        _storage.IsAvailable && _storage.ReadText(key) is not null;

    private string? ReadRaw(UbSettingScope scope, string name) =>
        Load(scope).Values.TryGetValue(name, out string? text) ? text : null;

    private void WriteRaw(UbSettingScope scope, string name, string text)
    {
        Tier tier = Load(scope);
        tier.Values[name] = text;
        Save(scope, tier);
    }

    private Tier Load(UbSettingScope scope)
    {
        Tier tier = _tiers[scope];
        if (tier.Loaded)
            return tier;
        tier.Loaded = true;
        tier.Values.Clear();

        string? key = KeyOf(scope);
        if (key is null || !_storage.IsAvailable)
            return tier;

        string? json;
        try
        {
            json = _storage.ReadText(key);
        }
        catch (Exception error)
        {
            // Nothing is known about what is in the file, which is a
            // stronger reason not to write it than bad content is: writing
            // would replace a file that was never read.
            _log.Warn(
                $"MossTank could not read {key} ({error.Message}); its "
                + "settings are at their defaults and it will not be "
                + "written until it can be read.");
            tier.ReadOnly = true;
            return tier;
        }
        if (string.IsNullOrWhiteSpace(json))
            return tier;

        try
        {
            Dictionary<string, string>? document =
                JsonSerializer.Deserialize<Dictionary<string, string>>(json, Options);
            if (document is not null)
                foreach ((string name, string text) in document)
                    tier.Values[name] = text;
        }
        catch (JsonException error)
        {
            // The file stays where it is: a person can repair it, and the
            // defaults hold until they do. Overwriting it would throw away
            // the only copy of what they configured.
            _log.Warn(
                $"MossTank could not read {key} ({error.Message}); its "
                + "settings are at their defaults until the file is repaired.");
            tier.ReadOnly = true;
        }
        return tier;
    }

    private void Save(UbSettingScope scope, Tier tier)
    {
        string? key = KeyOf(scope);
        if (key is null || tier.ReadOnly || !_storage.IsAvailable)
            return;
        try
        {
            _storage.WriteText(
                key,
                JsonSerializer.Serialize(
                    new SortedDictionary<string, string>(tier.Values, StringComparer.Ordinal),
                    Options));
        }
        catch (Exception error)
        {
            _log.Warn($"MossTank could not save {key}: {error.Message}");
        }
    }

    /// <summary>
    /// The file one tier lives in, or null when there is nowhere yet to put
    /// it -- which is what the character tier is before login.
    /// </summary>
    private string? KeyOf(UbSettingScope scope) => scope switch
    {
        UbSettingScope.Global => Root + "settings.json",
        UbSettingScope.Profile => $"{Root}profiles/{_profile}.settings.json",
        UbSettingScope.Character => CharacterFolder() is { Length: > 0 } folder
            ? folder + "settings.json"
            : null,
        _ => null,
    };

    private string CharacterFolder() =>
        _server.Length > 0 && _character.Length > 0
            ? $"{Root}{_server}/{_character}/"
            : string.Empty;

    /// <summary>
    /// A name that came from the world goes into a real folder name, so
    /// anything a folder cannot hold becomes an underscore.
    /// </summary>
    private static string Safe(string? value) => StorageLayout.Safe(value);

    private sealed class Tier
    {
        internal Dictionary<string, string> Values { get; } = new(StringComparer.OrdinalIgnoreCase);

        internal bool Loaded { get; set; }

        /// <summary>Set when the file on disk did not read, so it is not written over.</summary>
        internal bool ReadOnly { get; set; }

        internal void Forget()
        {
            Loaded = false;
            ReadOnly = false;
            Values.Clear();
        }
    }
}
