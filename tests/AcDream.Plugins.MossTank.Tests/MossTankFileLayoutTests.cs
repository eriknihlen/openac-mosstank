using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The one folder the plugin keeps its files in, the copy that fills it from
/// the old flat layout, and the one canonical name a character's files are
/// filed under.
/// </summary>
public sealed class MossTankFileLayoutTests
{
    [Fact]
    public void CanonicalCharacterKeyDropsTheServersDisplayMarker()
    {
        Assert.Equal("Acdream", VtankProfileDirectory.CanonicalCharacterKey("+Acdream"));
        Assert.Equal("Acdream", VtankProfileDirectory.CanonicalCharacterKey("Acdream"));
        Assert.Equal("Acdream", VtankProfileDirectory.CanonicalCharacterKey("  +Acdream  "));
        Assert.Equal(string.Empty, VtankProfileDirectory.CanonicalCharacterKey(null));
        Assert.Equal(string.Empty, VtankProfileDirectory.CanonicalCharacterKey("+"));
    }

    [Fact]
    public void EveryKindHasItsOwnFolderUnderOneRoot()
    {
        Assert.Equal("mosstank", VtankProfileDirectory.Root);
        Assert.Equal("mosstank/profiles", VtankProfileDirectory.SettingsFolder);
        Assert.Equal("mosstank/navs", VtankProfileDirectory.NavFolder);
        Assert.Equal("mosstank/metas", VtankProfileDirectory.MetaFolder);
        Assert.Equal("mosstank/loot", VtankProfileDirectory.LootFolder);
    }

    [Fact]
    public void SettingsPickerListsOnlyUsdFromTheProfilesFolder()
    {
        var storage = new MemoryStorage();
        storage.WriteText("mosstank/profiles/Shared.usd", "1\r\n");
        storage.WriteText("mosstank/profiles/--Barris_Coldeve.usd", "1\r\n");
        storage.WriteText("mosstank/profiles/Barris_Coldeve.cdf", "1\r\n");
        storage.WriteText("mosstank/loot/Shared.utl", "1\r\n");
        storage.WriteText("mosstank/navs/Shared.af", "1\r\n");
        storage.WriteText("Elsewhere.usd", "1\r\n");

        IReadOnlyList<VtankProfileDirectory.ProfileEntry> entries =
            VtankProfileDirectory.ListSettingsProfiles(
                storage, "Barris", "Coldeve", mineOnly: false);

        Assert.Equal(
            ["Shared.usd"],
            entries.Where(static e => e.FileName.Length > 0)
                .Select(static e => e.DisplayName));
        Assert.Equal(
            "mosstank/profiles/Shared.usd",
            entries.First(static e => e.FileName.Length > 0).FileName);
    }

    [Fact]
    public void NavigationPickerListsAfAndNavFromTheNavsFolder()
    {
        var storage = new MemoryStorage();
        storage.WriteText("mosstank/navs/Beta.af", "1\r\n");
        storage.WriteText("mosstank/navs/alpha.nav", "1\r\n");
        storage.WriteText("mosstank/navs/Beta.af.bak", "1\r\n");
        storage.WriteText("mosstank/metas/Hunt.af", "1\r\n");
        storage.WriteText("mosstank/loot/Keep.utl", "1\r\n");

        Assert.Equal(
            ["alpha.nav", "Beta.af"],
            VtankProfileDirectory.ListNavigationProfiles(storage)
                .Where(static e => e.FileName.Length > 0)
                .Select(static e => e.DisplayName));
    }

    [Fact]
    public void MetaPickerListsAfAndMetFromTheMetasFolder()
    {
        var storage = new MemoryStorage();
        storage.WriteText("mosstank/metas/Hunt.af", "1\r\n");
        storage.WriteText("mosstank/metas/dropped.met", "1\r\n");
        storage.WriteText("mosstank/metas/Hunt.af.tmp", "1\r\n");
        storage.WriteText("mosstank/navs/Route.af", "1\r\n");

        Assert.Equal(
            ["dropped.met", "Hunt.af"],
            VtankProfileDirectory.ListMetaProfiles(storage)
                .Where(static e => e.FileName.Length > 0)
                .Select(static e => e.DisplayName));
    }

    [Fact]
    public void LootPickerListsOnlyUtlFromTheLootFolder()
    {
        var storage = new MemoryStorage();
        storage.WriteText("mosstank/loot/Keep.utl", "1\r\n");
        storage.WriteText("mosstank/loot/Keep.utl.twin-2026-01-01", "1\r\n");
        storage.WriteText("mosstank/profiles/Shared.usd", "1\r\n");
        storage.WriteText("Elsewhere.utl", "1\r\n");

        Assert.Equal(
            ["Keep.utl"],
            VtankProfileDirectory.ListLootProfiles(storage)
                .Where(static e => e.FileName.Length > 0)
                .Select(static e => e.DisplayName));
    }

    private static readonly DateTimeOffset Now =
        new(2026, 9, 21, 12, 0, 0, TimeSpan.Zero);

    [Fact]
    public void MigrationCopiesTheOldFlatLayoutIntoItsFolders()
    {
        var storage = new MemoryStorage();
        storage.WriteText("--Barris_Coldeve.usd", "settings\r\n");
        storage.WriteText("Coldeve_Barris.cdf", "binding\r\n");
        storage.WriteText("Keep.utl", "loot\r\n");
        storage.WriteText("navs/Hunt.af", "route\r\n");
        storage.WriteText("metas/Buffing.af", "meta\r\n");
        storage.WriteText("nav_Legacy.af", "flat route\r\n");
        storage.WriteText("Flat.af", "flat meta\r\n");

        MossTankFileLayoutMigration.Result result =
            MossTankFileLayoutMigration.Run(storage, Now);

        Assert.Equal("settings", storage.ReadText("mosstank/profiles/--Barris_Coldeve.usd")?.Trim());
        Assert.Equal("binding", storage.ReadText("mosstank/profiles/Coldeve_Barris.cdf")?.Trim());
        Assert.Equal("loot", storage.ReadText("mosstank/loot/Keep.utl")?.Trim());
        Assert.Equal("route", storage.ReadText("mosstank/navs/Hunt.af")?.Trim());
        Assert.Equal("meta", storage.ReadText("mosstank/metas/Buffing.af")?.Trim());
        Assert.Equal("flat route", storage.ReadText("mosstank/navs/Legacy.af")?.Trim());
        Assert.Equal("flat meta", storage.ReadText("mosstank/metas/Flat.af")?.Trim());
        Assert.Equal(7, result.Total);
    }

    [Fact]
    public void MigrationTellsFlatRoutesFromFlatMetasAndDropsTheRouteMarker()
    {
        // The flat layout had no folders, so it told the two kinds apart by
        // a marker in the name. The marker is what the folder now says.
        var storage = new MemoryStorage();
        storage.WriteText("nav_Hunt.af", "named route\r\n");
        storage.WriteText("--nav_Barris_Coldeve.af", "character route\r\n");
        storage.WriteText("--Barris_Coldeve.af", "character meta\r\n");

        _ = MossTankFileLayoutMigration.Run(storage, Now);

        Assert.Equal("named route", storage.ReadText("mosstank/navs/Hunt.af")?.Trim());
        Assert.Equal(
            "character route",
            storage.ReadText("mosstank/navs/--Barris_Coldeve.af")?.Trim());
        Assert.Equal(
            "character meta",
            storage.ReadText("mosstank/metas/--Barris_Coldeve.af")?.Trim());
    }

    [Fact]
    public void MigrationNeverRemovesOrRewritesWhatItCopiedFrom()
    {
        var storage = new MemoryStorage();
        storage.WriteText("--Barris_Coldeve.usd", "settings\r\n");
        storage.WriteText("navs/Hunt.af", "route\r\n");

        _ = MossTankFileLayoutMigration.Run(storage, Now);

        Assert.Equal("settings", storage.ReadText("--Barris_Coldeve.usd")?.Trim());
        Assert.Equal("route", storage.ReadText("navs/Hunt.af")?.Trim());
    }

    [Fact]
    public void MigrationLeavesAnotherToolsFilesAlone()
    {
        var storage = new MemoryStorage();
        storage.WriteText("GameInfo.ugd", "database\r\n");
        storage.WriteText("someone-else/Their.usd", "theirs\r\n");
        storage.WriteText("Backup.usd.bak", "backup\r\n");

        MossTankFileLayoutMigration.Result result =
            MossTankFileLayoutMigration.Run(storage, Now);

        Assert.Equal(0, result.Total);
        Assert.DoesNotContain(
            storage.Keys,
            static key => key.StartsWith("mosstank/", StringComparison.Ordinal));
    }

    [Fact]
    public void MigrationRunsOnceAndLeavesAlreadyCopiedFilesUntouched()
    {
        var storage = new MemoryStorage();
        storage.WriteText("Keep.utl", "old\r\n");
        storage.WriteText("mosstank/loot/Keep.utl", "already here\r\n");

        MossTankFileLayoutMigration.Result first =
            MossTankFileLayoutMigration.Run(storage, Now);
        MossTankFileLayoutMigration.Result second =
            MossTankFileLayoutMigration.Run(storage, Now);

        Assert.Equal(0, first.Total);
        Assert.Equal(0, second.Total);
        Assert.Equal("already here", storage.ReadText("mosstank/loot/Keep.utl")?.Trim());
    }

    [Fact]
    public void MigrationKeepsTheMarkedTwinAndSetsTheOtherAside()
    {
        // One session wrote both spellings of one character's name. The
        // marked file is the live one; the other must not be thrown away.
        var storage = new MemoryStorage();
        storage.WriteText("--Acdream_sawato.usd", "written before the object arrived\r\n");
        storage.WriteText("--+Acdream_sawato.usd", "written after it arrived\r\n");
        storage.WriteText("sawato_Acdream.cdf", "early binding\r\n");
        storage.WriteText("sawato_+Acdream.cdf", "later binding\r\n");

        MossTankFileLayoutMigration.Result result =
            MossTankFileLayoutMigration.Run(storage, Now);

        Assert.Equal(
            "written after it arrived",
            storage.ReadText("mosstank/profiles/--Acdream_sawato.usd")?.Trim());
        Assert.Equal(
            "written before the object arrived",
            storage.ReadText("mosstank/profiles/--Acdream_sawato.usd.twin-2026-09-21")?.Trim());
        Assert.Equal(
            "later binding",
            storage.ReadText("mosstank/profiles/sawato_Acdream.cdf")?.Trim());
        Assert.Equal(2, result.TwinsKept);
        Assert.DoesNotContain(
            storage.Keys,
            static key => key.StartsWith("mosstank/", StringComparison.Ordinal)
                && key.Contains('+', StringComparison.Ordinal));
    }

    [Fact]
    public void MigrationSaysWhatItCopiedAndWhere()
    {
        var result = new MossTankFileLayoutMigration.Result(2, 1, 0, 0, TwinsKept: 1);

        string line = Assert.IsType<string>(result.Describe("/data/vtank"));

        Assert.Contains("2 settings", line, StringComparison.Ordinal);
        Assert.Contains("1 loot", line, StringComparison.Ordinal);
        Assert.Contains("mosstank", line, StringComparison.Ordinal);
        Assert.Contains("duplicate", line, StringComparison.Ordinal);
        Assert.Null(new MossTankFileLayoutMigration.Result(0, 0, 0, 0, 0).Describe(null));
    }

    internal sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text = new(StringComparer.Ordinal);

        public bool IsAvailable => true;

        public string? RootPath { get; set; }

        public IReadOnlyCollection<string> Keys => _text.Keys;

        public string? ReadText(string key) =>
            _text.TryGetValue(key, out string? value) ? value : null;

        public IReadOnlyList<string> List(string prefix) => _text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();

        public void WriteText(string key, string content) => _text[key] = content;

        public bool Delete(string key) => _text.Remove(key);
    }
}
