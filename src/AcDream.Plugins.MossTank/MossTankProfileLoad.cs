namespace AcDream.Plugins.MossTank;

/// <summary>
/// What happened when a profile store was asked for the selected profile.
/// The three answers need three different responses, and collapsing them to
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
