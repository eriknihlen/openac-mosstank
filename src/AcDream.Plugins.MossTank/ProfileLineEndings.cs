namespace AcDream.Plugins.MossTank;

/// <summary>
/// The line breaks a profile file is written with. The writers break lines
/// the way the files' own tools do on Windows; a file some other tool wrote
/// with bare line feeds keeps them when it is saved back, so saving a file
/// that was not edited leaves it exactly as it was.
/// </summary>
internal static class ProfileLineEndings
{
    /// <summary>
    /// <paramref name="written"/> with bare line feeds when the file it
    /// replaces, <paramref name="existing"/>, has line feeds and no
    /// carriage return before any of them; otherwise as it is.
    /// </summary>
    internal static string Match(string written, string? existing)
    {
        ArgumentNullException.ThrowIfNull(written);
        if (existing is null
            || !existing.Contains('\n', StringComparison.Ordinal)
            || existing.Contains("\r\n", StringComparison.Ordinal))
        {
            return written;
        }
        return written.Replace("\r\n", "\n", StringComparison.Ordinal);
    }
}
