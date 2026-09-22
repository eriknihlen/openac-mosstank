using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The display tools that came over with the UB page: the countryside map
/// and its small window of controls. They read their rows from the same
/// catalogue the page edits, polled a few times a second, so the page needs
/// no change event to reach them.
/// </summary>
internal sealed partial class MossTankPanel
{
    private LandscapeMapController _landscapeMap = null!;
    private NetworkHudController _networkHud = null!;
    private readonly LandscapeMarkerCatalog _landscapeMarkers = new();
    private IReadOnlyList<string> _ubClientNames = [];
    private int _selectedUbClient;
    private string _ubClientsNotice = string.Empty;

    /// <summary>
    /// The labels last handed to the client, so the row is pushed when it
    /// changes and not on every frame.
    /// </summary>
    private IReadOnlyList<string>? _publishedNetworkTags;

    private void InitializeDisplayTools(IPluginHost host)
    {
        Func<PluginNavigationPosition?> selfPosition =
            () => host.Automation.Navigation.Snapshot is { IsAvailable: true } snapshot
                ? snapshot.Position
                : null;
        _landscapeMarkers.Load(host.VtankProfiles);
        _landscapeMap = new LandscapeMapController(
            host.Ui,
            ReadLandscapeMapSettings,
            selfPosition,
            _landscapeMarkers);
        _networkHud = new NetworkHudController(
            host.Ui,
            host.Automation.Network,
            host.Automation.Objects,
            host.Selection,
            () => ReadNetworkHudSelf(host.Automation.Character),
            selfPosition,
            ReadNetworkHudSettings);
        PublishNetworkTags(onLoad: true);
    }

    /// <summary>
    /// Hands the client the labels this character answers to, so the other
    /// clients see it under them and a tagged broadcast reaches it.
    /// </summary>
    /// <param name="onLoad">
    /// True for the push made while the page is being built. An empty row is
    /// not pushed then: the client may have been started with labels of its
    /// own, and a character that has never named any must not clear them.
    /// Every later push is made because the row changed, so an emptied row
    /// does reach the client.
    /// </param>
    private void PublishNetworkTags(bool onLoad)
    {
        IReadOnlyList<string> tags = _ubCatalog.Require("Networking.Tags").Get().Items;
        if (_publishedNetworkTags is { } published
            && published.SequenceEqual(tags, StringComparer.Ordinal))
            return;
        string[] pushed = [.. tags];
        _publishedNetworkTags = pushed;
        if (onLoad && pushed.Length == 0)
            return;
        _host.Automation.Network.SetTags(pushed);
    }

    private LandscapeMapSettings ReadLandscapeMapSettings() => new(
        _ubCatalog.Require("LandscapeMaps.Enabled").Get().Boolean,
        _ubCatalog.Require("LandscapeMaps.Opacity").Get().AsInt32());

    private static NetworkHudSelf ReadNetworkHudSelf(ICharacterInfo character) => new(
        character.ObjectId,
        character.Name,
        character.CurrentHealth,
        character.MaxHealth,
        character.CurrentStamina,
        character.MaxStamina,
        character.CurrentMana,
        character.MaxMana);

    private NetworkHudSettings ReadNetworkHudSettings() => new(
        _ubCatalog.Require("NetworkUI.Enabled").Get().Boolean,
        _ubCatalog.Require("NetworkUI.ShowHudWhenClosed").Get().Boolean,
        _ubCatalog.Require("NetworkUI.SelectedTag").Get().Text,
        string.Join('\n', _ubCatalog.Require("NetworkUI.TrackedItems").Get().Items));

    private void TickDisplayTools(double elapsedSeconds)
    {
        PublishNetworkTags(onLoad: false);
        _landscapeMap.OnTick(elapsedSeconds);
        _networkHud.OnTick(elapsedSeconds);
        IReadOnlyList<NetworkHudRow> rows = _networkHud.Rows;
        if (rows.Count != _ubClientNames.Count
            || !rows.Select(static row => row.Name).SequenceEqual(_ubClientNames))
        {
            _ubClientNames = rows.Select(static row => row.Name).ToArray();
            _selectedUbClient = ClampRow(_selectedUbClient, _ubClientNames.Count);
        }
    }

    private void ResetDisplayTools()
    {
        _landscapeMap.Reset();
        _networkHud.Reset();
    }

    private void DisposeDisplayTools()
    {
        _landscapeMap.Dispose();
        _networkHud.Dispose();
    }

    // ---- The clients window ---------------------------------------------
    //
    // The readout is a canvas the host paints under every window; this
    // window lists the same characters, this one first, so one can be
    // selected without the pointer.

    public bool UbClientsVisible => _networkHud.Shown;

    public Action ShowUbClients => () => _networkHud.Shown = true;

    public Action HideUbClients => () => _networkHud.Shown = false;

    public string UbClientsTagText => _networkHud.TagText;

    public IReadOnlyList<string> UbClientNames => _ubClientNames;

    public int SelectedUbClientIndex => _selectedUbClient;

    public Action<int> SelectUbClient => index =>
    {
        _selectedUbClient = ClampRow(index, _ubClientNames.Count);
        _ubClientsNotice = _networkHud.SelectRow(index) is { } name
            ? $"Selected {name}."
            : string.Empty;
    };

    public string UbClientsNotice => _ubClientsNotice;

    // ---- The map window -------------------------------------------------
    //
    // The map itself is a canvas the host paints under every window; this
    // window is the strip of controls that drives it. The canvas takes the
    // pointer too: a drag slides the map and the wheel zooms at the cursor;
    // the buttons make the same calls in fixed steps about the middle.

    public bool UbMapVisible => _landscapeMap.Shown;

    public Action ShowUbMap => () =>
    {
        _landscapeMarkers.Load(_host.VtankProfiles);
        _landscapeMap.Shown = true;
    };

    public Action HideUbMap => () => _landscapeMap.Shown = false;

    public string UbMapFollowText => _landscapeMap.FollowText;

    public Action ToggleUbMapFollow => _landscapeMap.ToggleFollow;

    public Action UbMapZoomIn => _landscapeMap.ZoomIn;

    public Action UbMapZoomOut => _landscapeMap.ZoomOut;

    public Action UbMapPanLeft => () => _landscapeMap.Pan(LandscapeMapController.PanStepPixels, 0d);

    public Action UbMapPanRight => () => _landscapeMap.Pan(-LandscapeMapController.PanStepPixels, 0d);

    public Action UbMapPanUp => () => _landscapeMap.Pan(0d, LandscapeMapController.PanStepPixels);

    public Action UbMapPanDown => () => _landscapeMap.Pan(0d, -LandscapeMapController.PanStepPixels);

    public string UbMapReadout => _landscapeMap.Readout;

    public string UbMapMarkerCountText =>
        $"{_landscapeMarkers.Markers.Count} markers from {LandscapeMarkerCatalog.Folder}";
}
