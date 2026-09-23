using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The distance a meta's "distance to route" condition reads. Seen live:
/// bore.met's hunt state sends the character back to its start state when
/// the route is 1000 m away or more, and every route loaded from a file
/// read as infinitely far, so the circuit it had just loaded was replaced at
/// once by the start route again.
/// </summary>
public sealed class RouteDistanceTests
{
    /// <summary>
    /// A route read from a file has coordinates and no cell, and its points
    /// count. Mutation: skip points without a cell and the distance is
    /// infinite.
    /// </summary>
    [Fact]
    public void ARouteReadFromAFileIsMeasuredOnItsCoordinates()
    {
        const string af = """
            NAV: nav0 circular ~~ {
            	pnt -70.5461029529572 -66.0203276952108 0.366362762451172
            	pnt -70.2843772888184 -66.0715707778931 0.358354187011719
            """;
        var route = new NavigationSettings();
        Assert.True(MetafSerializer.TryLoadNav(af, route, NoOpAutomationSurface.Instance, out string error), error);
        // Standing a hundredth of a map unit (2.4 m) east of the first point.
        var player = new PluginNavigationPosition(0x7D640013u, -70.5361029529572, -66.0203276952108, 0.3, 0f, IsOutdoor: true);

        double meters = MossTankPanel.NearestRouteDistanceMeters(player, route.Waypoints);

        Assert.InRange(meters, 2.3, 2.5);
    }

    [Fact]
    public void AnEmptyRouteIsInfinitelyFar()
    {
        var player = new PluginNavigationPosition(0x7D640013u, -70.5, -66.0, 0.3, 0f, IsOutdoor: true);
        Assert.True(double.IsPositiveInfinity(MossTankPanel.NearestRouteDistanceMeters(player, [])));
    }
}
