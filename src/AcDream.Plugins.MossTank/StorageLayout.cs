namespace AcDream.Plugins.MossTank;

/// <summary>
/// The folder tree the plugin owns, in one list, so the folders that are
/// created and the folders that are tested are the same folders.
/// </summary>
/// <remarks>
/// A folder that only appears once something has been written into it is a
/// folder a person cannot put a file into beforehand: the profile they want
/// to drop in has nowhere to go, and the empty tree gives no hint of what
/// belongs where. So the whole tree is laid out when the plugin starts, and
/// the per-character part of it when a character logs in.
/// </remarks>
internal static class StorageLayout
{
    /// <summary>
    /// The folders inside the shared profile directory the host hands out:
    /// everything a person edits, copies between installations or shares.
    /// They hold the files other tools in the same directory expect to
    /// find, and the whole UB tree, whose plugin tier is the tier a profile
    /// shared between characters sits in.
    /// </summary>
    internal static IReadOnlyList<string> VtankProfileFolders { get; } =
    [
        VtankProfileDirectory.SettingsFolder,
        VtankProfileDirectory.MetaFolder,
        VtankProfileDirectory.NavFolder,
        VtankProfileDirectory.LootFolder,
        UbSettingStore.Root,
        UbSettingStore.Root + "profiles",
        UbSettingStore.Root + AutoVendorFolder,
        UbSettingStore.Root + EquipFolder,
        UbSettingStore.Root + ItemGiverFolder,
        UbSettingStore.Root + "maps",
        UbSettingStore.Root + "dungeonmaps",
        UbSettingStore.Root + "dungeonmaps/visited",
    ];

    /// <summary>
    /// The folders inside the plugin's own storage that do not depend on who
    /// is logged in. This storage holds only state the plugin keeps for
    /// itself; nothing a person would go looking for belongs here.
    /// </summary>
    internal static IReadOnlyList<string> PluginStorageFolders { get; } =
    [
        "expressions/global",
        "expressions/persistent",
        "profiles/macro",
        "profiles/macro/sidecar",
    ];

    /// <summary>The per-tier folders a profile file may sit in.</summary>
    private const string AutoVendorFolder = "autovendor";

    private const string EquipFolder = "equip";

    private const string ItemGiverFolder = "itemgiver";

    /// <summary>
    /// The storage key of an item giver profile by name, in the shared
    /// <c>itemgiver</c> folder: the one tier the reference keeps them in. A
    /// name may come with or without <c>.utl</c>.
    /// </summary>
    internal static string ItemGiverProfileKey(string name)
    {
        string bare = name.Trim();
        if (bare.EndsWith(".utl", StringComparison.OrdinalIgnoreCase))
            bare = bare[..^4];
        return UbSettingStore.Root + ItemGiverFolder + "/" + bare + ".utl";
    }

    /// <summary>
    /// The folders belonging to one logged-in character, inside the shared
    /// profile directory: the server tier, shared by every character on that
    /// world, and
    /// the character tier beneath it. Nothing is named while the world is
    /// unknown, and the character tier waits for the character.
    /// </summary>
    internal static IReadOnlyList<string> CharacterFolders(string? server, string? character)
    {
        string world = Safe(server);
        if (world.Length == 0)
            return [];
        string serverRoot = UbSettingStore.Root + world + "/";
        var folders = new List<string>
        {
            serverRoot,
            serverRoot + AutoVendorFolder,
            serverRoot + EquipFolder,
        };
        string name = Safe(character);
        if (name.Length == 0)
            return folders;
        string characterRoot = serverRoot + name + "/";
        folders.Add(characterRoot);
        folders.Add(characterRoot + AutoVendorFolder);
        folders.Add(characterRoot + EquipFolder);
        return folders;
    }

    /// <summary>
    /// A name that came from the world goes into a real folder name, so
    /// anything a folder cannot hold becomes an underscore.
    /// </summary>
    internal static string Safe(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
            return string.Empty;
        char[] invalid = Path.GetInvalidFileNameChars();
        var text = new System.Text.StringBuilder(value.Length);
        foreach (char character in value.Trim())
            text.Append(invalid.Contains(character) ? '_' : character);
        return text.ToString();
    }
}
