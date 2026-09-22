using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The dungeon map and its small window of controls. The map reads its
/// rows from the same catalogue the UB page edits, polled a few times a
/// second, and writes the zoom back to its row so the page shows it.
/// </summary>
internal sealed partial class MossTankPanel
{
    private DungeonMapController _dungeonMap = null!;

    private void InitializeDungeonMap(IPluginHost host)
    {
        _dungeonMap = new DungeonMapController(
            host.Ui,
            host.VtankProfiles,
            ReadDungeonMapSettings,
            zoom => _ubCatalog.Require("DungeonMaps.MapZoom").Set(UbSettingValue.FromSingle(zoom)),
            () => host.Automation.Navigation.Snapshot is { IsAvailable: true } snapshot ? snapshot : null,
            host.Automation.DungeonMap,
            () => host.Automation.Objects.CaptureObjects(),
            () => (host.Automation.Character.ObjectId, host.Automation.Character.Name));
    }

    private DungeonMapSettings ReadDungeonMapSettings() => new(
        _ubCatalog.Require("DungeonMaps.Enabled").Get().Boolean,
        _ubCatalog.Require("DungeonMaps.DrawWhenClosed").Get().Boolean,
        _ubCatalog.Require("DungeonMaps.ShowVisitedTiles").Get().Boolean,
        _ubCatalog.Require("DungeonMaps.VisitedTilesColor").Get().AsColor(),
        _ubCatalog.Require("DungeonMaps.ShowCompass").Get().Boolean,
        _ubCatalog.Require("DungeonMaps.Opacity").Get().AsInt32(),
        _ubCatalog.Require("DungeonMaps.MapZoom").Get().AsSingle(),
        _ubCatalog.Require("DungeonMaps.Debug").Get().Boolean,
        ReadDungeonMapPart("DungeonName"),
        ReadDungeonMapPart("Walls"),
        ReadDungeonMapPart("Floors"),
        new DungeonMarkerStyles(
            ReadDungeonMarker("You"),
            ReadDungeonMarker("Others"),
            ReadDungeonMarker("Items"),
            ReadDungeonMarker("Monsters"),
            ReadDungeonMarker("NPCs"),
            ReadDungeonMarker("MyCorpse"),
            ReadDungeonMarker("OtherCorpses"),
            ReadDungeonMarker("Portals"),
            ReadDungeonMarker("Containers"),
            ReadDungeonMarker("Doors"),
            ReadDungeonMarker("EverythingElse")));

    private DungeonMapPart ReadDungeonMapPart(string group)
    {
        string prefix = $"DungeonMaps.Display.{group}.";
        return new DungeonMapPart(
            _ubCatalog.Require(prefix + "Enabled").Get().Boolean,
            _ubCatalog.Require(prefix + "Color").Get().AsColor());
    }

    private DungeonMarkerStyle ReadDungeonMarker(string group)
    {
        string prefix = $"DungeonMaps.Display.Markers.{group}.";
        return new DungeonMarkerStyle(
            _ubCatalog.Require(prefix + "Enabled").Get().Boolean,
            _ubCatalog.Require(prefix + "UseIcon").Get().Boolean,
            _ubCatalog.Require(prefix + "ShowLabel").Get().Boolean,
            _ubCatalog.Require(prefix + "Color").Get().AsColor(),
            _ubCatalog.Require(prefix + "Size").Get().AsInt32());
    }

    private void TickDungeonMap(double elapsedSeconds) => _dungeonMap.OnTick(elapsedSeconds);

    private void ResetDungeonMap() => _dungeonMap.Reset();

    private void DisposeDungeonMap() => _dungeonMap.Dispose();

    // ---- The dungeon window ---------------------------------------------
    //
    // The map itself is a canvas the host paints under every window; this
    // window is the strip of controls that drives it. The canvas takes the
    // pointer too: a drag pans, a shift-drag turns and the wheel zooms at
    // the cursor; the buttons make the same calls in fixed steps about the
    // middle, and the storey is the slider's.

    public bool UbDungeonVisible => _dungeonMap.Shown;

    public Action ShowUbDungeon => () => _dungeonMap.Shown = true;

    public Action HideUbDungeon => () => _dungeonMap.Shown = false;

    public string UbDungeonFollowText => _dungeonMap.FollowText;

    public Action ToggleUbDungeonFollow => _dungeonMap.ToggleFollow;

    public string UbDungeonLayersText => _dungeonMap.LayersText;

    public Action ToggleUbDungeonLayers => () => _dungeonMap.ShowAllLayers = !_dungeonMap.ShowAllLayers;

    public Action UbDungeonZoomIn => _dungeonMap.ZoomIn;

    public Action UbDungeonZoomOut => _dungeonMap.ZoomOut;

    public Action UbDungeonTurnLeft => () => _dungeonMap.Rotate(-DungeonMapController.RotateStepDegrees);

    public Action UbDungeonTurnRight => () => _dungeonMap.Rotate(DungeonMapController.RotateStepDegrees);

    public Action UbDungeonPanLeft => () => _dungeonMap.Pan(DungeonMapController.PanStepPixels, 0d);

    public Action UbDungeonPanRight => () => _dungeonMap.Pan(-DungeonMapController.PanStepPixels, 0d);

    public Action UbDungeonPanUp => () => _dungeonMap.Pan(0d, DungeonMapController.PanStepPixels);

    public Action UbDungeonPanDown => () => _dungeonMap.Pan(0d, -DungeonMapController.PanStepPixels);

    public float UbDungeonHeight => _dungeonMap.HeightPercent;

    public Action<float> SetUbDungeonHeight => value =>
        _dungeonMap.SetHeightPercent((int)Math.Round(value));

    public string UbDungeonStatusText => _dungeonMap.Status;
}
