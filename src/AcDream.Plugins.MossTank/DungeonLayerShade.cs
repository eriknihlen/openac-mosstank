namespace AcDream.Plugins.MossTank;

/// <summary>
/// Whether one storey of a dungeon is drawn from a given height, and how
/// dim: the storey the height falls in is drawn as it is, the one above is
/// washed grey and half transparent so it reads as a ceiling, and the ones
/// below darken with distance until they are black. Storeys more than ten
/// metres above or twenty-four below are not drawn at all.
/// </summary>
/// <param name="Visible">Whether the storey is drawn.</param>
/// <param name="Brightness">What its colours are multiplied by, 0 to 1.</param>
/// <param name="Opacity">What its opacity is multiplied by, 0 to 1.</param>
internal readonly record struct DungeonLayerShade(bool Visible, float Brightness, float Opacity)
{
    /// <summary>A storey drawn exactly as it is.</summary>
    public static DungeonLayerShade Plain { get; } = new(true, 1f, 1f);

    /// <summary>A storey that is not drawn.</summary>
    public static DungeonLayerShade Hidden { get; } = new(false, 0f, 0f);

    /// <summary>Storeys are six metres tall; this is the height of the one a point is in.</summary>
    public const float BandHeight = 6f;

    /// <summary>
    /// The band a height falls in, in metres: bands are six metres tall and
    /// start three metres under their nominal height, so a height of five is
    /// in the band at six and a height of two in the one at nought.
    /// </summary>
    public static float BandOf(float z) => MathF.Floor((z + BandHeight / 2f) / BandHeight) * BandHeight;

    /// <summary>The shade of the storey at <paramref name="layerZ"/> seen from <paramref name="drawZ"/>.</summary>
    /// <param name="drawZ">The height the map is drawn from, in metres: the player's, or the slider's.</param>
    /// <param name="layerZ">The storey's band height, in metres.</param>
    /// <param name="showAllLayers">Draw every storey plainly, whatever the height.</param>
    public static DungeonLayerShade For(double drawZ, float layerZ, bool showAllLayers)
    {
        if (showAllLayers)
            return Plain;

        double above = drawZ - layerZ;
        if (above < -10d || above > 24d)
            return Hidden;

        // The storey above the player's own: a grey wash, half transparent.
        if (above < -3d)
            return new DungeonLayerShade(true, 151f / 255f, 121f / 255f);

        // The player's own storey.
        if (Math.Abs(above) < 3d)
            return Plain;

        // Below: dim by four tenths per storey, to black at two and a half
        // storeys down.
        float dim = (float)Math.Min(1d, Math.Abs(above) / BandHeight * 0.4d);
        return new DungeonLayerShade(true, 1f - dim, 1f);
    }

    /// <summary>
    /// The height a slider position picks: nought is the lowest storey and a
    /// hundred the highest.
    /// </summary>
    public static double DrawZFromSlider(float minZ, float maxZ, int percent) =>
        minZ + (maxZ - minZ) * (Math.Clamp(percent, 0, 100) / 100d);

    /// <summary>The slider position that shows a height; nought for a dungeon of one storey.</summary>
    public static int SliderFromDrawZ(float minZ, float maxZ, double drawZ)
    {
        double depth = maxZ - minZ;
        if (depth <= 0d)
            return 0;
        return (int)Math.Round(Math.Clamp((drawZ - minZ) / depth, 0d, 1d) * 100d);
    }
}
