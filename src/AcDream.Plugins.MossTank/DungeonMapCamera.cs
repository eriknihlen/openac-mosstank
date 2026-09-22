using System.Numerics;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Where the dungeon map looks from: the landblock-local point at the
/// middle of the canvas, how far it is zoomed, and how it is turned.
/// Following, it sits on the player and turns with their heading so that
/// what lies ahead is up; a pan, a turn or the height slider lets go of
/// the player until follow is asked for again.
/// </summary>
/// <remarks>
/// <para>
/// The picture is north-up, so turning it by the heading is the whole of
/// the rotation: a point d map pixels from the middle lands at d scaled
/// and turned anticlockwise on screen by the heading, which puts the
/// point ahead of the player above the middle.
/// </para>
/// <para>
/// <see cref="PanBy"/>, <see cref="RotateByDrag"/> and <see cref="ZoomAbout"/>
/// are what a drag, a shift-drag and a wheel over the canvas call with the
/// pointer's own deltas; the control strip drives the same camera with
/// fixed steps about the middle.
/// </para>
/// </remarks>
internal sealed class DungeonMapCamera
{
    /// <summary>As far out as the map zooms: screen pixels per map pixel.</summary>
    public const float MinScale = 0.3f;

    /// <summary>As far in as the map zooms.</summary>
    public const float MaxScale = 5f;

    /// <summary>How much one zoom step changes the scale.</summary>
    public const float ScaleStep = 0.2f;

    private float _scale = ScaleFromSetting(4.2f);
    private double _rotation;

    /// <summary>Screen pixels per map pixel, from 0.3 to 5.</summary>
    public float Scale
    {
        get => _scale;
        set => _scale = Math.Clamp(value, MinScale, MaxScale);
    }

    /// <summary>How far the picture is turned, in degrees clockwise from north-up, 0 to 360.</summary>
    public double RotationDegrees
    {
        get => _rotation;
        private set => _rotation = ((value % 360d) + 360d) % 360d;
    }

    /// <summary>The landblock-local point at the middle of the canvas, in metres.</summary>
    public Vector2 CenterMeters { get; private set; }

    /// <summary>Whether the camera sits on the player and turns with them.</summary>
    public bool Following { get; private set; } = true;

    /// <summary>
    /// The settings row keeps the zoom as five minus the scale, so the
    /// shipped 4.2 is a scale of 0.8: four screen pixels to the metre.
    /// </summary>
    public static float ScaleFromSetting(float mapZoom) => Math.Clamp(MaxScale - mapZoom, MinScale, MaxScale);

    /// <summary>The settings row's value for a scale.</summary>
    public static float SettingFromScale(float scale) => MaxScale - scale;

    /// <summary>Sits on the player, if following.</summary>
    public void Follow(Vector2 playerMeters, float headingDegrees)
    {
        if (!Following)
            return;
        CenterMeters = playerMeters;
        RotationDegrees = headingDegrees;
    }

    /// <summary>Goes back to sitting on the player from the next <see cref="Follow"/>.</summary>
    public void ResumeFollowing() => Following = true;

    /// <summary>Stays where it is while the player moves.</summary>
    public void StopFollowing() => Following = false;

    /// <summary>One step closer, to the nearest tenth so the steps land on round numbers.</summary>
    public void ZoomIn() => Scale = MathF.Round(Scale + ScaleStep, 1);

    /// <summary>One step further out.</summary>
    public void ZoomOut() => Scale = MathF.Round(Scale - ScaleStep, 1);

    /// <summary>
    /// Zooms by so many steps about a canvas point, as a wheel does: the
    /// spot under the point before is the spot under it after. Following,
    /// only the scale moves, since the middle is put back on the player
    /// anyway. Does not let go of the player.
    /// </summary>
    public void ZoomAbout(double x, double y, int steps, double canvasWidth, double canvasHeight)
    {
        float before = Scale;
        Scale = MathF.Round(Scale + (steps * ScaleStep), 1);
        if (Following || Scale == before)
            return;
        // The spot sits d pixels from the middle; scaled it would sit at
        // d times the ratio, so the picture slides by the difference.
        double ratio = Scale / before;
        ShiftBy((x - (canvasWidth / 2d)) * (1d - ratio), (y - (canvasHeight / 2d)) * (1d - ratio));
    }

    /// <summary>
    /// Moves the picture by a screen delta, as a drag does: the middle of
    /// the canvas moves the other way, turned back into the map's frame
    /// and scaled into metres. Lets go of the player.
    /// </summary>
    public void PanBy(double dxPixels, double dyPixels)
    {
        Following = false;
        ShiftBy(dxPixels, dyPixels);
    }

    private void ShiftBy(double dxPixels, double dyPixels)
    {
        double radians = RotationDegrees * Math.PI / 180d;
        double cos = Math.Cos(radians);
        double sin = Math.Sin(radians);
        // Turn the screen delta back into the picture's frame (undoing the
        // anticlockwise turn by the heading), then into metres, y up.
        double mapDx = (-dxPixels * cos) - (-dyPixels * sin);
        double mapDy = (-dxPixels * sin) + (-dyPixels * cos);
        CenterMeters += new Vector2(
            (float)(mapDx / Scale / DungeonMapPicture.PixelsPerMeter),
            (float)(-mapDy / Scale / DungeonMapPicture.PixelsPerMeter));
    }

    /// <summary>
    /// Turns the picture by a drag: a degree for every pixel dragged left
    /// and every pixel dragged up, as the reference does. Lets go of the player.
    /// </summary>
    public void RotateByDrag(double dxPixels, double dyPixels) => RotateBy(-(dxPixels + dyPixels));

    /// <summary>Turns the picture by so many degrees clockwise. Lets go of the player.</summary>
    public void RotateBy(double degrees)
    {
        Following = false;
        RotationDegrees += degrees;
    }

    /// <summary>Where a map pixel lands on a canvas of the given size.</summary>
    public Vector2 ScreenFromMap(Vector2 mapPixel, Vector2 centerMapPixel, double canvasWidth, double canvasHeight)
    {
        Vector2 d = (mapPixel - centerMapPixel) * Scale;
        double radians = -RotationDegrees * Math.PI / 180d;
        double cos = Math.Cos(radians);
        double sin = Math.Sin(radians);
        return new Vector2(
            (float)((d.X * cos) - (d.Y * sin) + (canvasWidth / 2d)),
            (float)((d.X * sin) + (d.Y * cos) + (canvasHeight / 2d)));
    }

    /// <summary>Where a landblock-local point lands on a canvas of the given size.</summary>
    public Vector2 ScreenFromLocal(DungeonMapPicture picture, Vector3 local, double canvasWidth, double canvasHeight) =>
        ScreenFromMap(
            picture.MapPixelFromLocal(local),
            picture.MapPixelFromLocal(CenterMeters),
            canvasWidth,
            canvasHeight);

    /// <summary>The direction north points on screen, as a unit vector, y down.</summary>
    public Vector2 NorthOnScreen()
    {
        double radians = -RotationDegrees * Math.PI / 180d;
        return new Vector2((float)Math.Sin(radians), (float)-Math.Cos(radians));
    }
}
