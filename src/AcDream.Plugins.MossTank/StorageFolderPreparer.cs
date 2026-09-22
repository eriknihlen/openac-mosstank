using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Creates the folder tree in <see cref="StorageLayout"/>: the fixed part
/// once when the plugin starts, and the per-character part again whenever a
/// different world or character logs in.
/// </summary>
/// <remarks>
/// Every folder already made is remembered, so the same login asked for
/// twice makes no second round of calls. A folder that cannot be made is not
/// fatal: the path is named in the log once, one line says so in chat once,
/// and the run goes on with whatever the storage did manage.
/// </remarks>
internal sealed class StorageFolderPreparer
{
    private readonly Action<string> _log;
    private readonly Action<string> _announce;
    private readonly HashSet<string> _made = new(StringComparer.OrdinalIgnoreCase);
    private readonly HashSet<string> _refused = new(StringComparer.OrdinalIgnoreCase);
    private bool _announcedRefusal;
    private bool _startDone;

    internal StorageFolderPreparer(Action<string> log, Action<string> announce)
    {
        _log = log ?? throw new ArgumentNullException(nameof(log));
        _announce = announce ?? throw new ArgumentNullException(nameof(announce));
    }

    /// <summary>
    /// The folders that do not depend on a login, across both roots. Runs
    /// once for the life of the plugin.
    /// </summary>
    internal void PrepareStart(IPluginStorage vtankProfiles, IPluginStorage pluginStorage)
    {
        if (_startDone)
            return;
        _startDone = true;
        Prepare("vtank", vtankProfiles, StorageLayout.VtankProfileFolders);
        Prepare("storage", pluginStorage, StorageLayout.PluginStorageFolders);
    }

    /// <summary>
    /// The server and character tiers of the UB tree, inside the shared
    /// profile directory. Called on every login; only a login that names a
    /// world or a character not seen before does any work.
    /// </summary>
    internal void PrepareCharacter(IPluginStorage vtankProfiles, string? server, string? character) =>
        Prepare("vtank", vtankProfiles, StorageLayout.CharacterFolders(server, character));

    private void Prepare(string root, IPluginStorage storage, IReadOnlyList<string> folders)
    {
        // No storage is not a failed folder: there is nowhere for the tree to
        // go and nothing for a person to be told about.
        if (!storage.IsAvailable)
            return;
        foreach (string folder in folders)
        {
            string remembered = root + "|" + folder;
            if (_made.Contains(remembered))
                continue;
            if (Create(storage, folder))
                _made.Add(remembered);
        }
    }

    private bool Create(IPluginStorage storage, string folder)
    {
        string path = PathOf(storage, folder);
        try
        {
            if (storage.EnsureDirectory(folder))
                return true;
        }
        catch (Exception error)
        {
            Refused($"MossTank could not create the folder {path}: {error.Message}");
            return false;
        }

        Refused($"MossTank could not create the folder {path}.");
        return false;
    }

    /// <summary>
    /// One log line per path that would not be made, and one chat line for
    /// the whole run: a tree that is half made is worth saying once, not
    /// once a folder.
    /// </summary>
    private void Refused(string line)
    {
        if (!_refused.Add(line))
            return;
        _log(line);
        if (_announcedRefusal)
            return;
        _announcedRefusal = true;
        _announce(
            "MossTank could not create part of its folder structure; "
            + "see the log for the paths.");
    }

    private static string PathOf(IPluginStorage storage, string folder)
    {
        string relative = folder.TrimEnd('/');
        return storage.RootPath is { Length: > 0 } root
            ? Path.Combine(root, relative.Replace('/', Path.DirectorySeparatorChar))
            : relative;
    }
}
