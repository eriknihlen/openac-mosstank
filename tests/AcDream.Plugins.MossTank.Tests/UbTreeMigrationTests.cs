using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The one-time copy of the UB tree out of the plugin's own storage, where
/// an earlier version wrote it, into the shared profile directory.
/// </summary>
public sealed class UbTreeMigrationTests
{
    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = new(StringComparer.Ordinal);

        public bool IsAvailable { get; init; } = true;

        public string? RootPath { get; init; }

        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;

        public IReadOnlyList<string> List(string prefix) => Text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();

        public void WriteText(string key, string content) => Text[key] = content;

        public bool Delete(string key) => Text.Remove(key);
    }

    /// <summary>
    /// Every file under the old prefix arrives at the same relative path
    /// under the new root, whatever depth it sat at.
    /// Mutation: address the destination by the bare file name instead of
    /// the relative path and the nested files collide into one.
    /// </summary>
    [Fact]
    public void EveryFileArrivesAtTheSameRelativePathUnderTheNewRoot()
    {
        var plugin = new MemoryStorage
        {
            Text =
            {
                ["ub/settings.json"] = "installation",
                ["ub/maps/markers.csv"] = "Town, Holtburg, 42.1, 33.6",
                ["ub/Coldeve/Acdream/settings.json"] = "character",
                ["ub/Coldeve/Acdream/equip/raid.utl"] = "rules",
                ["expressions/global/keep.json"] = "not UB",
            },
        };
        var vtank = new MemoryStorage();

        UbTreeMigration.Result result = UbTreeMigration.Run(plugin, vtank);

        Assert.Equal(4, result.Copied);
        Assert.Equal(0, result.AlreadyPresent);
        Assert.Equal("installation", vtank.ReadText("mosstank/ub/settings.json"));
        Assert.Equal(
            "Town, Holtburg, 42.1, 33.6", vtank.ReadText("mosstank/ub/maps/markers.csv"));
        Assert.Equal("character", vtank.ReadText("mosstank/ub/Coldeve/Acdream/settings.json"));
        Assert.Equal("rules", vtank.ReadText("mosstank/ub/Coldeve/Acdream/equip/raid.utl"));
        // Nothing outside the old prefix is this migration's to touch.
        Assert.DoesNotContain("mosstank/ub/expressions/global/keep.json", vtank.Text.Keys);
        Assert.Equal(4, vtank.Text.Count);
    }

    /// <summary>
    /// The originals stay exactly where they were: a copy, never a move, so
    /// a person who goes back to the older build still has their files.
    /// Mutation: delete the source after the write and the count drops.
    /// </summary>
    [Fact]
    public void TheOriginalsAreLeftWhereTheyWere()
    {
        var plugin = new MemoryStorage { Text = { ["ub/settings.json"] = "installation" } };

        UbTreeMigration.Run(plugin, new MemoryStorage());

        Assert.Equal("installation", plugin.ReadText("ub/settings.json"));
    }

    /// <summary>
    /// A file already at its new name is the live one and is left exactly as
    /// it is, which is what makes the second run a no-op.
    /// Mutation: drop the destination check and the older copy overwrites
    /// the edits made since.
    /// </summary>
    [Fact]
    public void AFileAlreadyAtItsNewNameIsLeftAlone()
    {
        var plugin = new MemoryStorage
        {
            Text = { ["ub/settings.json"] = "old", ["ub/maps/markers.csv"] = "markers" },
        };
        var vtank = new MemoryStorage
        {
            Text = { ["mosstank/ub/settings.json"] = "edited since" },
        };

        UbTreeMigration.Result first = UbTreeMigration.Run(plugin, vtank);

        Assert.Equal(1, first.Copied);
        Assert.Equal(1, first.AlreadyPresent);
        Assert.Equal("edited since", vtank.ReadText("mosstank/ub/settings.json"));

        UbTreeMigration.Result second = UbTreeMigration.Run(plugin, vtank);

        Assert.Equal(0, second.Copied);
        Assert.Equal(2, second.AlreadyPresent);
        Assert.Null(second.Describe("C:\\memory"));
    }

    /// <summary>
    /// The one line a person reads: how many files, where they went, and
    /// that the originals are still where they were.
    /// Mutation: leave the root out of the sentence and there is nothing
    /// naming the folder to go and look in.
    /// </summary>
    [Fact]
    public void TheCopyIsAnnouncedInOneLineNamingTheFolder()
    {
        var plugin = new MemoryStorage
        {
            Text = { ["ub/settings.json"] = "a", ["ub/maps/markers.csv"] = "b" },
        };

        UbTreeMigration.Result result = UbTreeMigration.Run(plugin, new MemoryStorage());
        string line = Assert.IsType<string>(result.Describe("C:\\memory"));

        Assert.Equal(
            "MossTank copied 2 UtilityBelt file(s) into C:\\memory\\mosstank\\ub "
            + "(the originals were left where they were).",
            line);
    }

    /// <summary>
    /// Nothing to copy says nothing, and neither does a session with no
    /// storage behind either root.
    /// </summary>
    [Fact]
    public void NothingToCopySaysNothing()
    {
        var empty = new MemoryStorage();

        Assert.Null(UbTreeMigration.Run(empty, new MemoryStorage()).Describe("C:\\memory"));

        var unavailable = new MemoryStorage { IsAvailable = false };
        var plugin = new MemoryStorage { Text = { ["ub/settings.json"] = "a" } };

        Assert.Equal(default, UbTreeMigration.Run(plugin, unavailable));
        Assert.Equal(default, UbTreeMigration.Run(unavailable, new MemoryStorage()));
    }
}
