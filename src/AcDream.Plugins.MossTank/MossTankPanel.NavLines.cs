using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private IPluginWorldLineLayer? _navLineLayer;
    private int? _navLineSignature;

    /// <summary>The reference client's "Show Nav Lines" toggle.</summary>
    public bool ShowNavLinesEnabled => _navigationSettings.ShowNavLines;

    public Action ToggleShowNavLines => () =>
    {
        _navigationSettings.ShowNavLines = !_navigationSettings.ShowNavLines;
        UpdateNavLines();
        SaveProfile();
    };

    private void ClearNavLines()
    {
        _navLineLayer?.SetLines(Array.Empty<PluginWorldLine>());
        _navLineSignature = null;
    }

    /// <summary>
    /// Draws the loaded route as a line through its points, a ring of the
    /// arrival radius around each, and the current point in its own colour.
    /// The lines are rebuilt only when something they depend on changes.
    /// </summary>
    private void UpdateNavLines()
    {
        PluginNavigationSnapshot snapshot = _host.Automation.Navigation.Snapshot;
        if (!_navigationSettings.ShowNavLines || !snapshot.IsAvailable || snapshot.IsPortalSpace)
        {
            if (_navLineSignature is not null)
                ClearNavLines();
            return;
        }
        _navLineLayer ??= _host.WorldLines.CreateLayer();
        if (_navLineLayer is null)
            return;
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
        if (_navLineSignature == signature)
            return;
        _navLineSignature = signature;

        var result = new List<PluginWorldLine>();
        List<RouteWaypoint> points = _navigationSettings.Waypoints;
        int count = Math.Min(points.Count, 2048);
        for (int i = 0; i < count; i++)
        {
            PluginNavigationPosition point = Lift(points[i].Position);
            bool current = i == _navigation.CurrentWaypointIndex;
            uint color = current ? 0x80FF40u : 0xFF40FFu;
            // The ring is the arrival radius, in landblock units.
            double radius = Math.Clamp(_navigationSettings.MinimumDistanceMeters, 0.5d, 50d) / 240d;
            const int segments = 24;
            for (int j = 0; j < segments; j++)
            {
                double a = j * Math.Tau / segments;
                double b = (j + 1) * Math.Tau / segments;
                result.Add(new PluginWorldLine(
                    point with
                    {
                        EastWest = point.EastWest + Math.Cos(a) * radius,
                        NorthSouth = point.NorthSouth + Math.Sin(a) * radius,
                    },
                    point with
                    {
                        EastWest = point.EastWest + Math.Cos(b) * radius,
                        NorthSouth = point.NorthSouth + Math.Sin(b) * radius,
                    },
                    color));
            }
            int next = i + 1;
            if (next == count && _navigationSettings.Mode == RouteMode.Circular)
                next = 0;
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

    // A hair above the ground so the line is not buried in the terrain.
    private static PluginNavigationPosition Lift(PluginNavigationPosition point) =>
        point with { Elevation = point.Elevation + 0.05d / 240d };
}
