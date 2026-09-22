using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The three files behind the catalogue, and the order a profile file is
/// looked for in.
/// </summary>
public sealed class UbSettingStoreTests
{
    private sealed class MemoryStorage : IPluginStorage
    {
        internal Dictionary<string, string> Files { get; } = new(StringComparer.OrdinalIgnoreCase);

        /// <summary>Keys whose read fails outright rather than returning bad content.</summary>
        internal HashSet<string> Unreadable { get; } = new(StringComparer.OrdinalIgnoreCase);

        public bool IsAvailable => true;

        public string? RootPath => "C:\\memory";

        public string? ReadText(string key) =>
            Unreadable.Contains(key)
                ? throw new IOException($"{key} is in use by another process.")
                : Files.TryGetValue(key, out string? text) ? text : null;

        public IReadOnlyList<string> List(string prefix) =>
            Files.Keys
                .Where(key => key.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                .OrderBy(static key => key, StringComparer.OrdinalIgnoreCase)
                .ToArray();

        public void WriteText(string key, string content) => Files[key] = content;

        public bool Delete(string key) => Files.Remove(key);
    }

    private sealed class UnavailableStorage : IPluginStorage
    {
    }

    private static UbSettingStore Store(
        IPluginStorage storage,
        string server = "Coldeve",
        string character = "Acdream") =>
        new UbSettingStore(storage, NullUbSettingLog.Instance).Bind(server, character);

    [Fact]
    public void EachTierLandsInTheFolderItsLayoutSays()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);

        store.Write(UbSettingScope.Global, "Nametags.Enabled", UbSettingValue.FromBool(false));
        store.Write(UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(9));
        store.Write(
            UbSettingScope.Character,
            "Networking.Tags",
            UbSettingValue.FromCollection(["tank"]));

        Assert.Contains("mosstank/ub/settings.json", storage.Files.Keys);
        Assert.Contains("mosstank/ub/profiles/default.settings.json", storage.Files.Keys);
        Assert.Contains("mosstank/ub/Coldeve/Acdream/settings.json", storage.Files.Keys);
    }

    [Fact]
    public void AValueWrittenInOneSessionComesBackInTheNext()
    {
        var storage = new MemoryStorage();
        Store(storage).Write(
            UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(9));

        UbSettingValue? again = Store(storage).Read(UbSettingScope.Profile, "AutoVendor.Tries");

        Assert.NotNull(again);
        Assert.Equal(9, again.Value.AsInt32());
    }

    /// <summary>
    /// Reading walks the narrowest file first: the character's own value
    /// wins over the profile's, and the profile's over the installation's.
    /// </summary>
    [Fact]
    public void TheNarrowestFileThatHasAValueWins()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        var catalog = new UbSettingCatalog(store);

        Assert.Equal(4, catalog.Require("AutoVendor.Tries").Get().AsInt32());

        store.Write(UbSettingScope.Global, "AutoVendor.Tries", UbSettingValue.FromInt(1));
        Assert.Equal(1, catalog.Require("AutoVendor.Tries").Get().AsInt32());

        store.Write(UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(2));
        Assert.Equal(2, catalog.Require("AutoVendor.Tries").Get().AsInt32());

        store.Write(UbSettingScope.Character, "AutoVendor.Tries", UbSettingValue.FromInt(3));
        Assert.Equal(3, catalog.Require("AutoVendor.Tries").Get().AsInt32());

        Assert.True(store.Clear(UbSettingScope.Character, "AutoVendor.Tries"));
        Assert.Equal(2, catalog.Require("AutoVendor.Tries").Get().AsInt32());
    }

    [Fact]
    public void AnotherCharacterOnTheSameAccountKeepsItsOwnFile()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        store.Write(
            UbSettingScope.Character, "Networking.Tags", UbSettingValue.FromCollection(["tank"]));

        store.Bind("Coldeve", "Horan");

        Assert.Null(store.Read(UbSettingScope.Character, "Networking.Tags"));
        store.Write(
            UbSettingScope.Character, "Networking.Tags", UbSettingValue.FromCollection(["healer"]));
        Assert.Contains("mosstank/ub/Coldeve/Horan/settings.json", storage.Files.Keys);

        store.Bind("Coldeve", "Acdream");
        Assert.Equal(
            ["tank"],
            store.Read(UbSettingScope.Character, "Networking.Tags")!.Value.Items);
    }

    [Fact]
    public void ANamedProfileIsItsOwnFileAndTheChoiceIsRememberedPerCharacter()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);

        store.SelectProfile("raid");
        store.Write(UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(9));
        Assert.Contains("mosstank/ub/profiles/raid.settings.json", storage.Files.Keys);

        store.SelectProfile("default");
        Assert.Null(store.Read(UbSettingScope.Profile, "AutoVendor.Tries"));

        // The next session on this character opens the same profile.
        store.SelectProfile("raid");
        UbSettingStore next = Store(storage);
        Assert.Equal("raid", next.ProfileName);
        Assert.Equal(9, next.Read(UbSettingScope.Profile, "AutoVendor.Tries")!.Value.AsInt32());
    }

    /// <summary>
    /// A profile has to exist as a file before anything can list it, so
    /// making one writes it even while it is empty. Selecting a name that
    /// was never created would leave the menu unable to offer it again.
    /// </summary>
    [Fact]
    public void AProfileMadeHereIsAFileAndIsListedFromThenOn()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);

        Assert.True(store.CreateProfile("raid", copyCurrent: false, out string notice));
        Assert.Contains("raid", notice, StringComparison.Ordinal);
        Assert.Equal("raid", store.ProfileName);
        Assert.Contains("mosstank/ub/profiles/raid.settings.json", storage.Files.Keys);
        Assert.Equal(["default", "raid"], store.AvailableProfiles());
        Assert.Equal(["default", "raid"], Store(storage).AvailableProfiles());
    }

    [Fact]
    public void AProfileCopiedHereCarriesTheOpenOnesValuesAndLeavesItAlone()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        store.Write(UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(9));

        Assert.True(store.CreateProfile("raid", copyCurrent: true, out _));

        Assert.Equal(9, store.Read(UbSettingScope.Profile, "AutoVendor.Tries")!.Value.AsInt32());
        store.Write(UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(11));
        store.SelectProfile("default");
        Assert.Equal(9, store.Read(UbSettingScope.Profile, "AutoVendor.Tries")!.Value.AsInt32());
    }

    [Fact]
    public void AProfileNeedsANameAndCannotTakeOneAlreadyInUse()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);

        Assert.False(store.CreateProfile("  ", copyCurrent: false, out string unnamed));
        Assert.Contains("name", unnamed, StringComparison.OrdinalIgnoreCase);

        Assert.True(store.CreateProfile("raid", copyCurrent: false, out _));
        Assert.False(store.CreateProfile("RAID", copyCurrent: false, out string taken));
        Assert.Contains("already", taken, StringComparison.Ordinal);
        Assert.False(store.CreateProfile("default", copyCurrent: false, out _));
        Assert.Equal("raid", store.ProfileName);
    }

    [Fact]
    public void TheProfileChoiceIsNotAnOrdinarySettingAndIsNeverListedAsOne()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        store.SelectProfile("raid");

        string document = storage.Files["mosstank/ub/Coldeve/Acdream/settings.json"];
        Assert.Contains("raid", document, StringComparison.Ordinal);
        Assert.DoesNotContain(
            UbSettingDefinitions.All,
            static definition => definition.Name.StartsWith("$", StringComparison.Ordinal));
        Assert.Null(store.Read(UbSettingScope.Character, "$profile"));
    }

    /// <summary>
    /// A file written by a later version carries settings this one has never
    /// heard of. Writing the file back must not throw them away, or opening
    /// the page once would silently downgrade someone's configuration.
    /// </summary>
    [Fact]
    public void ASettingThisVersionDoesNotKnowSurvivesAWrite()
    {
        var storage = new MemoryStorage();
        storage.Files["mosstank/ub/settings.json"] =
            "{\n  \"FromTheFuture.Something\": \"42\"\n}";

        UbSettingStore store = Store(storage);
        store.Write(UbSettingScope.Global, "Nametags.Enabled", UbSettingValue.FromBool(false));

        Assert.Contains(
            "FromTheFuture.Something",
            storage.Files["mosstank/ub/settings.json"],
            StringComparison.Ordinal);
    }

    [Fact]
    public void ADamagedFileIsReportedAndLeavesTheDefaultsStanding()
    {
        var storage = new MemoryStorage();
        storage.Files["mosstank/ub/settings.json"] = "{ this is not json";
        var log = new RecordingUbSettingLog();

        var store = new UbSettingStore(storage, log).Bind("Coldeve", "Acdream");

        Assert.Null(store.Read(UbSettingScope.Global, "Nametags.Enabled"));
        Assert.Contains(log.Warnings, warning => warning.Contains("mosstank/ub/settings.json", StringComparison.Ordinal));
    }

    /// <summary>
    /// A file whose contents did not parse is left alone, and so is one
    /// that could not be read at all. The second is the dangerous half:
    /// nothing is known about what is in it, so writing would replace a
    /// file that was never read, and the tier says nothing about how it
    /// came to be empty.
    /// </summary>
    [Fact]
    public void AFileThatWouldNotReadAtAllIsNotWrittenOverEither()
    {
        var storage = new MemoryStorage();
        storage.Files["mosstank/ub/settings.json"] = "{\n  \"Nametags.Enabled\": \"False\"\n}";
        storage.Unreadable.Add("mosstank/ub/settings.json");
        var log = new RecordingUbSettingLog();

        var store = new UbSettingStore(storage, log).Bind("Coldeve", "Acdream");
        store.Write(UbSettingScope.Global, "Nametags.MaxRange", UbSettingValue.FromSingle(20f));

        Assert.Equal("{\n  \"Nametags.Enabled\": \"False\"\n}", storage.Files["mosstank/ub/settings.json"]);
        Assert.False(store.IsWritable(UbSettingScope.Global));
        Assert.Contains(
            log.Warnings,
            warning => warning.Contains("mosstank/ub/settings.json", StringComparison.Ordinal));
    }

    /// <summary>
    /// A tier that is standing on its defaults because its file did not
    /// read is not a tier anything can be saved into, and the page has to
    /// be able to say so on every edit rather than once at load.
    /// </summary>
    [Fact]
    public void ATierSaysWhetherAWriteToItReachesAFile()
    {
        var storage = new MemoryStorage();
        storage.Files["mosstank/ub/profiles/default.settings.json"] = "{ this is not json";
        UbSettingStore store = Store(storage);

        Assert.True(store.IsWritable(UbSettingScope.Global));
        Assert.True(store.IsWritable(UbSettingScope.Character));
        Assert.False(store.IsWritable(UbSettingScope.Profile));

        // Nowhere to write at all, and no character yet, are the same
        // answer for the same reason: the write reaches no file.
        var unavailable = new UbSettingStore(new UnavailableStorage(), NullUbSettingLog.Instance);
        Assert.False(unavailable.IsWritable(UbSettingScope.Global));
        Assert.False(
            new UbSettingStore(new MemoryStorage(), NullUbSettingLog.Instance)
                .IsWritable(UbSettingScope.Character));
    }

    /// <summary>
    /// The three settings files are this plugin's own flat map of name to
    /// text. The original writes a nested typed document under the same
    /// name, so one of those copied in is exactly the damaged-file case:
    /// it is reported, the defaults stand, and it is left where it is.
    /// </summary>
    [Fact]
    public void ASettingsFileCopiedFromTheOriginalIsTheDamagedFileCase()
    {
        const string copiedIn = """
            {
              "$type": "UtilityBelt.Settings, UtilityBelt",
              "Plugin": { "SettingsProfile": "[character]" },
              "DungeonMaps": { "Display": { "Walls": { "Color": -16777089 } } }
            }
            """;
        var storage = new MemoryStorage();
        storage.Files["mosstank/ub/Coldeve/Acdream/settings.json"] = copiedIn;
        var log = new RecordingUbSettingLog();

        var store = new UbSettingStore(storage, log).Bind("Coldeve", "Acdream");
        store.Write(
            UbSettingScope.Character, "NetworkUI.SelectedTag", UbSettingValue.FromText("raid"));

        Assert.False(store.IsWritable(UbSettingScope.Character));
        Assert.Equal(copiedIn, storage.Files["mosstank/ub/Coldeve/Acdream/settings.json"]);
        Assert.Contains(
            log.Warnings,
            warning => warning.Contains(
                "mosstank/ub/Coldeve/Acdream/settings.json", StringComparison.Ordinal));
    }

    [Fact]
    public void WithNowhereToWriteEverythingStillWorksForTheSession()
    {
        var store = new UbSettingStore(new UnavailableStorage(), NullUbSettingLog.Instance)
            .Bind("Coldeve", "Acdream");
        var catalog = new UbSettingCatalog(store);

        catalog.Require("AutoVendor.Tries").Set(UbSettingValue.FromInt(9));

        Assert.Equal(9, catalog.Require("AutoVendor.Tries").Get().AsInt32());
    }

    [Fact]
    public void BeforeLoginThereIsNoCharacterFolderAndTheWiderTiersStillRead()
    {
        var storage = new MemoryStorage();
        var store = new UbSettingStore(storage, NullUbSettingLog.Instance);
        store.Write(UbSettingScope.Global, "Nametags.Enabled", UbSettingValue.FromBool(false));
        store.Write(UbSettingScope.Character, "Networking.Tags", UbSettingValue.FromCollection(["x"]));

        Assert.Contains("mosstank/ub/settings.json", storage.Files.Keys);
        Assert.DoesNotContain(
            storage.Files.Keys,
            static key => key.StartsWith("mosstank/ub/Coldeve/", StringComparison.Ordinal));
        Assert.False(store.Read(UbSettingScope.Global, "Nametags.Enabled")!.Value.Boolean);
        Assert.Equal(["x"], store.Read(UbSettingScope.Character, "Networking.Tags")!.Value.Items);
    }

    /// <summary>
    /// A profile file is looked for the way the tools that read one look for
    /// it: this character, then this server, then the installation, and then
    /// the same three again for a file called default.
    /// </summary>
    [Fact]
    public void AProfileFileIsLookedForFromTheNarrowestFolderOutwards()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);

        // Nothing written anywhere: the answer is where a new one would go.
        Assert.Equal("mosstank/ub/autovendor/shops.utl", store.ResolveProfileKey("autovendor", "shops.utl"));

        storage.Files["mosstank/ub/autovendor/default.utl"] = "";
        Assert.Equal("mosstank/ub/autovendor/default.utl", store.ResolveProfileKey("autovendor", "shops.utl"));

        storage.Files["mosstank/ub/Coldeve/autovendor/default.utl"] = "";
        Assert.Equal(
            "mosstank/ub/Coldeve/autovendor/default.utl",
            store.ResolveProfileKey("autovendor", "shops.utl"));

        storage.Files["mosstank/ub/Coldeve/Acdream/autovendor/default.utl"] = "";
        Assert.Equal(
            "mosstank/ub/Coldeve/Acdream/autovendor/default.utl",
            store.ResolveProfileKey("autovendor", "shops.utl"));

        // A file under its own name beats every default, from the widest
        // folder up.
        storage.Files["mosstank/ub/autovendor/shops.utl"] = "";
        Assert.Equal("mosstank/ub/autovendor/shops.utl", store.ResolveProfileKey("autovendor", "shops.utl"));

        storage.Files["mosstank/ub/Coldeve/autovendor/shops.utl"] = "";
        Assert.Equal(
            "mosstank/ub/Coldeve/autovendor/shops.utl",
            store.ResolveProfileKey("autovendor", "shops.utl"));

        storage.Files["mosstank/ub/Coldeve/Acdream/autovendor/shops.utl"] = "";
        Assert.Equal(
            "mosstank/ub/Coldeve/Acdream/autovendor/shops.utl",
            store.ResolveProfileKey("autovendor", "shops.utl"));
    }

    [Fact]
    public void ANameWithACharacterNoFolderCanHoldIsMadeSafe()
    {
        var storage = new MemoryStorage();
        var store = new UbSettingStore(storage, NullUbSettingLog.Instance)
            .Bind("Coldeve", "Bad:Name");

        store.Write(UbSettingScope.Character, "Networking.Tags", UbSettingValue.FromCollection(["x"]));

        Assert.Contains("mosstank/ub/Coldeve/Bad_Name/settings.json", storage.Files.Keys);
    }

    /// <summary>
    /// The plugin's own folder already has a layout and a migration. The new
    /// tree sits under its own prefix so the two can never collide and the
    /// migration stays as simple as it is.
    /// </summary>
    [Fact]
    public void TheNewTreeNeverTouchesTheLayoutAlreadyThere()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        store.Write(UbSettingScope.Global, "Nametags.Enabled", UbSettingValue.FromBool(false));
        store.Write(UbSettingScope.Profile, "AutoVendor.Tries", UbSettingValue.FromInt(9));
        store.Write(UbSettingScope.Character, "Networking.Tags", UbSettingValue.FromCollection(["x"]));

        Assert.All(
            storage.Files.Keys,
            static key => Assert.StartsWith("mosstank/ub/", key, StringComparison.Ordinal));
    }

    private sealed class RecordingUbSettingLog : IUbSettingLog
    {
        internal List<string> Warnings { get; } = [];

        public void Warn(string message) => Warnings.Add(message);
    }
}

/// <summary>
/// The alias list lives beside the settings files rather than in them, in
/// the file the alias profile setting names: this character's own, or a
/// named one under the profiles folder that any character can point at.
/// </summary>
public sealed class UbSettingStoreAliasFileTests
{
    private sealed class MemoryStorage : IPluginStorage
    {
        internal Dictionary<string, string> Files { get; } = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? RootPath => "C:\\memory";
        public string? ReadText(string key) => Files.TryGetValue(key, out string? text) ? text : null;
        public IReadOnlyList<string> List(string prefix) => Files.Keys
            .Where(key => key.StartsWith(prefix, StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();
        public void WriteText(string key, string content) => Files[key] = content;
        public bool Delete(string key) => Files.Remove(key);
    }

    private static UbSettingStore Store(IPluginStorage storage) =>
        new UbSettingStore(storage, NullUbSettingLog.Instance).Bind("Coldeve", "Acdream");

    [Theory]
    [InlineData("[character]", "mosstank/ub/Coldeve/Acdream/aliases.txt")]
    [InlineData("[CHARACTER]", "mosstank/ub/Coldeve/Acdream/aliases.txt")]
    [InlineData("  [character]  ", "mosstank/ub/Coldeve/Acdream/aliases.txt")]
    [InlineData("shared", "mosstank/ub/profiles/shared.aliases.txt")]
    [InlineData("my:aliases", "mosstank/ub/profiles/my_aliases.aliases.txt")]
    public void TheAliasProfileNamesTheFile(string profile, string key)
    {
        Assert.Equal(key, Store(new MemoryStorage()).AliasFileKey(profile));
    }

    /// <summary>
    /// A blank profile falls back to the character's own file rather than
    /// to a file with no name.
    /// </summary>
    [Fact]
    public void ABlankProfileIsTheCharactersOwnFile()
    {
        Assert.Equal("mosstank/ub/Coldeve/Acdream/aliases.txt", Store(new MemoryStorage()).AliasFileKey(""));
    }

    /// <summary>Before a character is known there is no character file to name.</summary>
    [Fact]
    public void TheCharacterFileNeedsACharacter()
    {
        var store = new UbSettingStore(new MemoryStorage(), NullUbSettingLog.Instance);
        Assert.Null(store.AliasFileKey("[character]"));
        Assert.Equal("mosstank/ub/profiles/shared.aliases.txt", store.AliasFileKey("shared"));
    }

    [Fact]
    public void LinesRoundTripThroughTheFile()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        string key = store.AliasFileKey("[character]")!;

        Assert.Empty(store.ReadLines(key));
        Assert.True(store.WriteLines(key, ["^/a$ = /b", "[eat] ^/c$ -> 1"]));

        Assert.Equal("^/a$ = /b\n[eat] ^/c$ -> 1\n", storage.Files[key]);
        Assert.Equal(["^/a$ = /b", "[eat] ^/c$ -> 1"], store.ReadLines(key));
        Assert.Equal(["^/a$ = /b", "[eat] ^/c$ -> 1"], Store(storage).ReadLines(key));
    }

    /// <summary>A file written by hand, with Windows line ends and blank lines, reads the same.</summary>
    [Fact]
    public void AHandWrittenFileReads()
    {
        var storage = new MemoryStorage();
        storage.Files["mosstank/ub/profiles/shared.aliases.txt"] = "^/a$ = /b\r\n\r\n  ^/c$ = /d  \r\n";
        Assert.Equal(["^/a$ = /b", "^/c$ = /d"], Store(storage).ReadLines("mosstank/ub/profiles/shared.aliases.txt"));
    }

    /// <summary>
    /// A write with nowhere to go says so, so the page can tell the
    /// player the edit is for this session only.
    /// </summary>
    [Fact]
    public void AWriteWithNoFileIsRefused()
    {
        var store = new UbSettingStore(new MemoryStorage(), NullUbSettingLog.Instance);
        Assert.False(store.WriteLines(null, ["^/a$ = /b"]));
        Assert.Empty(store.ReadLines(null));
        Assert.False(new UbSettingStore(NoOpPluginStorage.Instance, NullUbSettingLog.Instance)
            .Bind("Coldeve", "Acdream")
            .WriteLines("mosstank/ub/Coldeve/Acdream/aliases.txt", ["^/a$ = /b"]));
    }

    /// <summary>Rebinding to another character reads that character's file, not a cached one.</summary>
    [Fact]
    public void RebindingForgetsWhatWasRead()
    {
        var storage = new MemoryStorage();
        UbSettingStore store = Store(storage);
        string key = store.AliasFileKey("[character]")!;
        store.WriteLines(key, ["^/a$ = /b"]);
        storage.Files[key] = "^/x$ = /y\n";
        Assert.Equal(["^/a$ = /b"], store.ReadLines(key));

        store.Bind("Coldeve", "Horan");
        store.Bind("Coldeve", "Acdream");
        Assert.Equal(["^/x$ = /y"], store.ReadLines(key));
    }
}
