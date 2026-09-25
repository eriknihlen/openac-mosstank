namespace AcDream.Plugins.MossTank;

/// <summary>
/// How a jump's hold time becomes the power it is asked of the client with.
/// A full charge takes one second, so every millisecond held is a thousandth
/// of it; the client leaves with exactly that power rather than with whatever
/// the frames it saw the key held for happen to add up to.
/// </summary>
internal static class JumpPower
{
    /// <summary>
    /// The least power the client takes. A hold of nothing is a tap, and the
    /// client takes no jump of no power at all, so a tap asks for the
    /// smallest power above it: a hop too small to lift the character.
    /// </summary>
    internal const float Least = float.Epsilon;

    /// <summary>
    /// The power a hold of <paramref name="milliseconds"/> asks for: a
    /// thousandth of a full charge per millisecond, at least
    /// <see cref="Least"/> and at most a full charge.
    /// </summary>
    internal static float FromMilliseconds(int milliseconds) =>
        Math.Clamp(milliseconds / 1000f, Least, 1f);

    /// <summary>
    /// The seconds the client holds the charge for when asked for a hold of
    /// <paramref name="milliseconds"/>: the hold itself, up to the one second
    /// a full charge takes.
    /// </summary>
    internal static double ChargeSeconds(int milliseconds) =>
        Math.Clamp(milliseconds, 0, 1000) / 1000d;
}
