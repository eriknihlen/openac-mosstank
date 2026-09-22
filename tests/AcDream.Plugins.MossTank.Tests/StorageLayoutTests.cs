using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The folder tree the plugin lays out: which folders exist after a start,
/// which appear on a login, and what happens when one cannot be made.
/// </summary>
public sealed class StorageLayoutTests
{
    /// <summary>A storage that records every folder it was asked to make.</summary>
    private sealed class FakePluginStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text = new(StringComparer.OrdinalIgnoreCase);

        /// <summary>Every prefix asked for, in order, including repeats.</summary>
        internal List<string> Directories { get; } = [];

        /// <summary>Prefixes that answer no rather than making a folder.</summary>
        internal HashSet<string> Refuse { get; } = new(StringComparer.OrdinalIgnoreCase);

        /// <summary>Prefixes whose creation fails outright.</summary>
        internal HashSet<string> Throw { get; } = new(StringComparer.OrdinalIgnoreCase);

        public bool IsAvailable => true;

        public string? RootPath => "C:\\memory";

        public string? ReadText(string key) => _text.TryGetValue(key, out string? value) ? value : null;

        public IReadOnlyList<string> List(string prefix) =>
            _text.Keys.Where(key => key.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)).ToList();

        public void WriteText(string key, string content) => _text[key] = content;

        public bool EnsureDirectory(string prefix)
        {
            Directories.Add(prefix);
            if (Throw.Contains(prefix))
                throw new IOException($"{prefix} is not writable.");
            return !Refuse.Contains(prefix);
        }

        public bool Delete(string key) => _text.Remove(key);
    }

    private static (StorageFolderPreparer Preparer, List<string> Log, List<string> Chat) Build()
    {
        List<string> log = [];
        List<string> chat = [];
        return (new StorageFolderPreparer(log.Add, chat.Add), log, chat);
    }

    [Fact]
    public void A_start_makes_the_shared_profile_folders()
    {
        var shared = new FakePluginStorage();
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareStart(shared, storage);

        Assert.Equal(
            [
                "mosstank/profiles",
                "mosstank/metas",
                "mosstank/navs",
                "mosstank/loot",
                "mosstank/ub/",
                "mosstank/ub/profiles",
                "mosstank/ub/autovendor",
                "mosstank/ub/equip",
                "mosstank/ub/itemgiver",
                "mosstank/ub/maps",
                "mosstank/ub/dungeonmaps",
                "mosstank/ub/dungeonmaps/visited",
            ],
            shared.Directories);
    }

    [Fact]
    public void A_start_makes_the_plugin_storage_folders()
    {
        var shared = new FakePluginStorage();
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareStart(shared, storage);

        Assert.Equal(
            [
                "expressions/global",
                "expressions/persistent",
                "profiles/macro",
                "profiles/macro/sidecar",
            ],
            storage.Directories);
    }

    [Fact]
    public void A_start_runs_once()
    {
        var shared = new FakePluginStorage();
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareStart(shared, storage);
        int made = storage.Directories.Count;
        preparer.PrepareStart(shared, storage);

        Assert.Equal(made, storage.Directories.Count);
    }

    [Fact]
    public void A_login_makes_the_server_and_character_tiers()
    {
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareCharacter(storage, "Frostfell", "Acdream");

        Assert.Equal(
            [
                "mosstank/ub/Frostfell/",
                "mosstank/ub/Frostfell/autovendor",
                "mosstank/ub/Frostfell/equip",
                "mosstank/ub/Frostfell/Acdream/",
                "mosstank/ub/Frostfell/Acdream/autovendor",
                "mosstank/ub/Frostfell/Acdream/equip",
            ],
            storage.Directories);
    }

    [Fact]
    public void A_name_a_folder_cannot_hold_is_filed_under_the_safe_spelling()
    {
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareCharacter(storage, "Dark:Side", "Ac/dream?");

        Assert.Equal(
            [
                "mosstank/ub/Dark_Side/",
                "mosstank/ub/Dark_Side/autovendor",
                "mosstank/ub/Dark_Side/equip",
                "mosstank/ub/Dark_Side/Ac_dream_/",
                "mosstank/ub/Dark_Side/Ac_dream_/autovendor",
                "mosstank/ub/Dark_Side/Ac_dream_/equip",
            ],
            storage.Directories);
    }

    [Fact]
    public void The_same_login_twice_makes_nothing_again()
    {
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareCharacter(storage, "Frostfell", "Acdream");
        storage.Directories.Clear();
        preparer.PrepareCharacter(storage, "Frostfell", "Acdream");

        Assert.Empty(storage.Directories);
    }

    [Fact]
    public void Another_character_on_the_same_server_makes_only_its_own_tier()
    {
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareCharacter(storage, "Frostfell", "Acdream");
        storage.Directories.Clear();
        preparer.PrepareCharacter(storage, "Frostfell", "Horan");

        Assert.Equal(
            [
                "mosstank/ub/Frostfell/Horan/",
                "mosstank/ub/Frostfell/Horan/autovendor",
                "mosstank/ub/Frostfell/Horan/equip",
            ],
            storage.Directories);
    }

    [Fact]
    public void A_login_without_a_world_names_nothing()
    {
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareCharacter(storage, string.Empty, "Acdream");

        Assert.Empty(storage.Directories);
    }

    [Fact]
    public void A_world_without_a_character_stops_at_the_server_tier()
    {
        var storage = new FakePluginStorage();
        (StorageFolderPreparer preparer, _, _) = Build();

        preparer.PrepareCharacter(storage, "Frostfell", null);

        Assert.Equal(
            ["mosstank/ub/Frostfell/", "mosstank/ub/Frostfell/autovendor", "mosstank/ub/Frostfell/equip"],
            storage.Directories);
    }

    [Fact]
    public void A_refused_folder_is_logged_with_its_path_and_said_once()
    {
        var storage = new FakePluginStorage();
        storage.Refuse.Add("mosstank/ub/maps");
        (StorageFolderPreparer preparer, List<string> log, List<string> chat) = Build();

        preparer.PrepareStart(storage, new FakePluginStorage());
        preparer.PrepareCharacter(storage, "Frostfell", "Acdream");

        string line = Assert.Single(log);
        Assert.Contains(Path.Combine("C:\\memory", "mosstank", "ub", "maps"), line);
        Assert.Single(chat);
    }

    [Fact]
    public void A_refused_folder_does_not_stop_the_folders_after_it()
    {
        var storage = new FakePluginStorage();
        storage.Throw.Add("mosstank/ub/profiles");
        (StorageFolderPreparer preparer, List<string> log, _) = Build();

        preparer.PrepareStart(storage, new FakePluginStorage());

        Assert.Contains("mosstank/ub/dungeonmaps/visited", storage.Directories);
        Assert.Contains("is not writable", Assert.Single(log));
    }

    [Fact]
    public void A_refused_folder_is_tried_again_on_the_next_login()
    {
        var storage = new FakePluginStorage();
        storage.Refuse.Add("mosstank/ub/Frostfell/equip");
        (StorageFolderPreparer preparer, _, List<string> chat) = Build();

        preparer.PrepareCharacter(storage, "Frostfell", "Acdream");
        storage.Directories.Clear();
        preparer.PrepareCharacter(storage, "Frostfell", "Horan");

        Assert.Contains("mosstank/ub/Frostfell/equip", storage.Directories);
        Assert.Single(chat);
    }

    [Fact]
    public void A_session_with_no_storage_is_not_a_failure()
    {
        (StorageFolderPreparer preparer, List<string> log, List<string> chat) = Build();

        preparer.PrepareStart(NoOpPluginStorage.Instance, NoOpPluginStorage.Instance);
        preparer.PrepareCharacter(NoOpPluginStorage.Instance, "Frostfell", "Acdream");

        Assert.Empty(log);
        Assert.Empty(chat);
    }

    [Fact]
    public void The_layout_is_the_one_the_settings_store_reads_from()
    {
        Assert.Contains(UbSettingStore.Root + "profiles", StorageLayout.VtankProfileFolders);
        Assert.Contains(
            UbSettingStore.Root + "Frostfell/Acdream/",
            StorageLayout.CharacterFolders("Frostfell", "Acdream"));
    }

    /// <summary>
    /// The plugin's own storage holds state the plugin keeps for itself.
    /// Every folder a person would go looking for -- the UB tree included --
    /// belongs in the shared profile directory instead.
    /// </summary>
    [Fact]
    public void No_UB_folder_is_laid_out_in_the_plugins_own_storage()
    {
        Assert.DoesNotContain(
            StorageLayout.PluginStorageFolders,
            folder => folder.Contains("ub/", StringComparison.OrdinalIgnoreCase));
        Assert.All(
            StorageLayout.CharacterFolders("Frostfell", "Acdream"),
            folder => Assert.StartsWith(
                VtankProfileDirectory.Root + "/", folder, StringComparison.Ordinal));
    }
}
