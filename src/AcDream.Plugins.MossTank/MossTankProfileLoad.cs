namespace AcDream.Plugins.MossTank;

/// <summary>
/// What happened when a profile store was asked for the selected profile.
/// Each answer needs a different response, and collapsing them to
/// a bool is how an unreadable profile came to be silently replaced by an
/// empty one: "not written yet" wants the current settings saved out as the
/// new profile, while "there but unreadable" must keep the author's file and
/// say so.
/// </summary>
internal enum MossTankProfileLoad
{
    /// <summary>The selected profile was read and applied.</summary>
    Loaded,

    /// <summary>
    /// Every complete rule before an incomplete tail was applied. The source
    /// remains write-protected until it can be read completely.
    /// </summary>
    Partial,

    /// <summary>
    /// The selected profile has never been written. Writing the current
    /// settings out under that name is the intended first-run behaviour.
    /// </summary>
    Missing,

    /// <summary>
    /// The selected profile exists but could not be read. Nothing was
    /// applied; the file must be left exactly as the author wrote it.
    /// </summary>
    Failed,
}
