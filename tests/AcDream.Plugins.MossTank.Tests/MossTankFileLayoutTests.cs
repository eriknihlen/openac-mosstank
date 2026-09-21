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
