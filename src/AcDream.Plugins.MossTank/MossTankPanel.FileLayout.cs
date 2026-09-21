namespace AcDream.Plugins.MossTank;

/// <summary>
/// Where the plugin's files live, and the lines that tell the user about
/// them: the one-off copy out of the old flat layout, and a line per
/// profile load naming the file and the path it was read from.
/// </summary>
internal sealed partial class MossTankPanel
{
    /// <summary>
    /// Lines that were produced before there was anywhere to post them.
    /// The stores load during construction, before a chat surface exists,
    /// and a load nobody was told about is a load the user cannot check.
    /// </summary>
    private readonly List<string> _queuedAnnouncements = [];

    private bool _fileLayoutMigrated;

    /// <summary>
    /// Fills the plugin's folder from the old flat layout, once. Runs
    /// before the stores read anything, because everything they look for
    /// is addressed inside that folder from now on.
    /// </summary>
    private void MigrateFileLayout()
    {
        if (_fileLayoutMigrated)
            return;
        _fileLayoutMigrated = true;
        try
        {
            MossTankFileLayoutMigration.Result result =
                MossTankFileLayoutMigration.Run(_host.VtankProfiles, DateTimeOffset.Now);
            if (result.Describe(_host.VtankProfiles.RootPath) is { } line)
            {
                _host.Log.Info(line);
                QueueAnnouncement(line);
            }
        }
        catch (Exception error)
        {
            // A failed copy leaves the old files exactly where they were,
            // so the run can go on with an empty folder rather than no run.
            _host.Log.Warn(
                "MossTank could not copy its files into their own folder: "
                + error.Message);
        }
    }

    /// <summary>
    /// The one line a load produces: what kind of profile, which file, and
    /// the full path it came off. A user who cannot see which file the
    /// client actually read cannot tell a stale profile from a live one.
    /// Nothing is said for a profile that does not exist yet -- there was no
    /// load to report.
    /// </summary>
    private void AnnounceLoadOutcome(
        string kind,
        string? storageKey,
        MossTankProfileLoad outcome)
    {
        if (storageKey is not { Length: > 0 } key)
            return;
        switch (outcome)
        {
            case MossTankProfileLoad.Loaded:
                QueueAnnouncement(
                    $"Loaded {kind} profile {FileNameOf(key)} from {FullPathOf(key)}");
                break;
            case MossTankProfileLoad.Partial:
                QueueAnnouncement(
                    $"Loaded {kind} profile {FileNameOf(key)} from {FullPathOf(key)} "
                    + "up to an incomplete tail");
                break;
            case MossTankProfileLoad.Failed:
                QueueAnnouncement(
                    $"Could not load {kind} profile {FileNameOf(key)} from "
                    + FullPathOf(key));
                break;
            default:
                break;
        }
    }

    private static string FileNameOf(string storageKey) =>
        storageKey[(storageKey.LastIndexOf('/') + 1)..];

    /// <summary>
    /// The path on disk behind a storage key. The client reports the
    /// directory its keys are written beneath; without one there is nothing
    /// to resolve against, so the key is named as the key it is.
    /// </summary>
    private string FullPathOf(string storageKey) =>
        _host.VtankProfiles.RootPath is { Length: > 0 } root
            ? Path.Combine(root, storageKey.Replace('/', Path.DirectorySeparatorChar))
            : $"{storageKey} (the client does not report where its profile "
                + "directory is)";

    private void QueueAnnouncement(string line)
    {
        if (_host.Automation.IsAvailable)
        {
            Announce(line);
            return;
        }
        _queuedAnnouncements.Add(line);
    }

    private void FlushQueuedAnnouncements()
    {
        if (_queuedAnnouncements.Count == 0 || !_host.Automation.IsAvailable)
            return;
        foreach (string line in _queuedAnnouncements)
            Announce(line);
        _queuedAnnouncements.Clear();
    }
}
