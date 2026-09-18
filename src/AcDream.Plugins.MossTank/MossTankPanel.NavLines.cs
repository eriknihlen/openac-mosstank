using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private IPluginWorldLineLayer? _navLineLayer;
    private int? _navLineSignature;
    private IPluginWorldLineLayer? _walkableLayer;
    private double _walkableRefresh;
    private bool _navdataWasRunning;
    public string NavdataGenerationText { get; private set; } = "Generate navigation data for this area and its neighbours.";
    public Action GenerateNearbyNavdata => () =>
    {
        if (_host.Automation.Navigation.GenerateNearbyNavdata())
        {
            _navdataWasRunning = true;
            NavdataGenerationText = "Generating nearby navigation data…";
        }
        else NavdataGenerationText = "Generation unavailable or busy; enter the world and try again.";
    };
    public Action CancelNavdataGeneration => () => _host.Automation.Navigation.CancelNavdataGeneration();

    private void UpdateNavdataGeneration()
    {
        var status = _host.Automation.Navigation.NavdataGenerationStatus;
        if (status.Running)
        {
            _navdataWasRunning = true;
            NavdataGenerationText = $"Generating navdata: {status.Completed}/{status.Total} landblocks";
        }
        else if (_navdataWasRunning)
        {
            _navdataWasRunning = false;
            NavdataGenerationText = status.Message;
            _navigation.RefreshNavigationGeometry();
            _walkableRefresh = 0d;
            _host.Log.Info(status.Message);
        }
    }
    public bool ShowWalkableAreasEnabled => _navigationSettings.ShowWalkableAreas;
    public Action ToggleShowWalkableAreas => () =>
    {
        _navigationSettings.ShowWalkableAreas = !_navigationSettings.ShowWalkableAreas;
        _walkableRefresh = 0d;
        UpdateWalkableAreas(0d);
        SaveProfile();
    };

    private void ClearWalkableAreas()
    {
        _walkableLayer?.Dispose();
        _walkableLayer = null;
        _walkableRefresh = 0d;
        _host.Automation.Navigation.CaptureWalkableMesh(false);
    }

    private void UpdateWalkableAreas(double elapsed)
    {
        var snapshot = _host.Automation.Navigation.Snapshot;
        if (!_navigationSettings.ShowWalkableAreas || !snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            if (_walkableLayer is not null) ClearWalkableAreas();
            return;
        }
        _walkableRefresh -= Math.Max(0d, elapsed);
        if (_walkableRefresh > 0d) return;
        _walkableRefresh = 1d;
        _walkableLayer ??= _host.WorldLines.CreateLayer();
        if (_walkableLayer is not null)
            _walkableLayer.SetLines(_host.Automation.Navigation.CaptureWalkableMesh());
    }
    public bool ShowNavLinesEnabled => _navigationSettings.ShowNavLines;
    public Action ToggleShowNavLines => () =>
    {
        _navigationSettings.ShowNavLines = !_navigationSettings.ShowNavLines;
        UpdateNavLines();
        SaveProfile();
    };

    private void ClearNavLines()
    {
        ClearWalkableAreas();
        _navLineLayer?.SetLines(Array.Empty<PluginWorldLine>());
        _navLineSignature = null;
    }

    private void UpdateNavLines()
    {
        PluginNavigationSnapshot snapshot = _host.Automation.Navigation.Snapshot;
        if (!_navigationSettings.ShowNavLines || !snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            if (_navLineSignature is not null) ClearNavLines();
            return;
        }
        _navLineLayer ??= _host.WorldLines.CreateLayer();
        if (_navLineLayer is null) return;
        var hash = new HashCode();
        hash.Add(_navigationSettings.Mode);
        hash.Add(_navigationSettings.MinimumDistanceMeters);
        hash.Add(snapshot.Position.IsOutdoor);
        hash.Add(_navigation.CurrentWaypointIndex);
        foreach (RouteWaypoint point in _navigationSettings.Waypoints)
        {
            hash.Add(point.Type);
            hash.Add(point.Position);
        }
        int signature = hash.ToHashCode();
        if (_navLineSignature == signature) return;
        _navLineSignature = signature;
        var result = new List<PluginWorldLine>();
        var points = _navigationSettings.Waypoints;
        int count = Math.Min(points.Count, 2048);
        for (int i = 0; i < count; i++)
        {
            PluginNavigationPosition point = Lift(points[i].Position);
            bool current = i == _navigation.CurrentWaypointIndex;
            uint color = current ? 0x80FF40u : 0xFF40FFu;
            double radius = Math.Clamp(_navigationSettings.MinimumDistanceMeters, 0.5d, 50d) / 240d;
            int segments = 24;
            for (int j = 0; j < segments; j++)
            {
                double a = j * Math.Tau / segments;
                double b = (j + 1) * Math.Tau / segments;
                result.Add(new PluginWorldLine(
                    point with { EastWest = point.EastWest + Math.Cos(a) * radius,
                        NorthSouth = point.NorthSouth + Math.Sin(a) * radius },
                    point with { EastWest = point.EastWest + Math.Cos(b) * radius,
                        NorthSouth = point.NorthSouth + Math.Sin(b) * radius }, color));
            }
            int next = i + 1;
            if (next == count && _navigationSettings.Mode == RouteMode.Circular) next = 0;
            if (next >= count || next == i || points[i].Type is
                RouteWaypointType.Portal or RouteWaypointType.PortalByName
                or RouteWaypointType.Recall or RouteWaypointType.Jump)
                continue;
            result.Add(new PluginWorldLine(point, Lift(points[next].Position), 0xFF40FFu));
        }
        _navLineLayer.SetLines(result.Select(line => line with
        {
            FollowTerrain = snapshot.Position.IsOutdoor,
            WidthMeters = 0.25f,
        }).ToArray());
    }

    private static PluginNavigationPosition Lift(PluginNavigationPosition point) =>
        point with { Elevation = point.Elevation + 0.05d / 240d };
}
