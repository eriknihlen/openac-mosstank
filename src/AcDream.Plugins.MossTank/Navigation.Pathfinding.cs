using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class NavigationController
{
    private const double PathCornerArrivalMeters = 0.75d;
    private const double PathDeviationMeters = 6d;
    private const double PathDiscontinuityMeters = 12d;
    private const double PathLoadingRetrySeconds = 0.5d;
    private const double PathVerticalToleranceMeters = 1.5d;

    private NavigationPathLeg? _pathLeg;
    private PluginNavigationPosition[] _pathCorners = [];
    private int _pathCornerIndex;
    private PluginNavigationPathStatus _pathStatus =
        PluginNavigationPathStatus.Unavailable;
    private bool _pathHasPlanResult;
    private long _pathMeshRevision;
    private double _pathRetryAt;
    private PluginNavigationPosition _pathSegmentStart;
    private PluginNavigationPosition _pathObservedPosition;
    private PluginNavigationPosition _pathObservedConfirmedPosition;
    private ulong _pathObservedConfirmedRevision;
    private bool _hasPathObservedPosition;
    internal void RefreshNavigationGeometry() => ClearNavigationPath();

    private bool SteerAlongNavigationPath(
        INavigationAutomation navigation,
        in PluginNavigationSnapshot snapshot,
        in PluginNavigationPosition destination,
        RouteWaypointType waypointType,
        uint objectId,
        double arrivalMeters,
        string statusLabel,
        out bool pathEnded)
    {
        ObservePathPosition(snapshot);
        var leg = new NavigationPathLeg(
            _settings.Mode,
            _index,
            _reverse,
            waypointType,
            objectId,
            destination.CellId,
            destination.EastWest,
            destination.NorthSouth,
            destination.Elevation,
            destination.IsOutdoor);
        if (_pathLeg != leg)
        {
            ClearNavigationPath();
            _pathLeg = leg;
            _pathSegmentStart = snapshot.Position;
        }

        bool needsPlan = !_pathHasPlanResult
            || ((_pathStatus is PluginNavigationPathStatus.Loading
                    or PluginNavigationPathStatus.Unavailable)
                && _mover.Now >= _pathRetryAt);
        if (needsPlan)
        {
            PlanNavigationPath(
                navigation,
                snapshot.Position,
                destination,
                arrivalMeters);
        }

        if (_pathStatus != PluginNavigationPathStatus.Complete)
        {
            StopMovement();
            pathEnded = false;
            _status = PathFailureStatus(statusLabel, _pathStatus);
            return false;
        }

        if (_pathCornerIndex < _pathCorners.Length
            && SpatialDistanceMeters(
                snapshot.Position,
                _pathCorners[_pathCornerIndex]) <= PathCornerArrivalMeters)
        {
            _pathSegmentStart = _pathCorners[_pathCornerIndex];
            _pathCornerIndex++;
        }

        if (_pathCornerIndex >= _pathCorners.Length)
        {
            StopMovement();
            pathEnded = true;
            return false;
        }

        PluginNavigationPosition corner = _pathCorners[_pathCornerIndex];
        if (DistanceToSpatialSegmentMeters(
                snapshot.Position,
                _pathSegmentStart,
                corner) > PathDeviationMeters)
        {
            ClearNavigationPath();
            _pathLeg = leg;
            _pathSegmentStart = snapshot.Position;
            PlanNavigationPath(
                navigation,
                snapshot.Position,
                destination,
                arrivalMeters);
            if (_pathStatus != PluginNavigationPathStatus.Complete
                || _pathCorners.Length == 0)
            {
                StopMovement();
                pathEnded = false;
                _status = PathFailureStatus(statusLabel, _pathStatus);
                return false;
            }
            corner = _pathCorners[0];
        }

        double cornerDistance = SpatialDistanceMeters(snapshot.Position, corner);
        _status =
            $"{statusLabel}: path corner {_pathCornerIndex + 1}/{_pathCorners.Length} "
            + $"({cornerDistance:0.0}m).";
        pathEnded = false;
        return Steer(
            navigation,
            snapshot.Position,
            corner,
            cornerDistance);
    }

    private void PlanNavigationPath(
        INavigationAutomation navigation,
        in PluginNavigationPosition current,
        in PluginNavigationPosition destination,
        double arrivalMeters)
    {
        double horizontalTolerance = Math.Clamp(arrivalMeters, 0.5d, 4d);
        PluginNavigationPathResult result = navigation.FindPath(
            destination,
            horizontalTolerance,
            PathVerticalToleranceMeters);
        _pathHasPlanResult = true;
        _pathStatus = result.Status;
        _pathMeshRevision = result.MeshRevision;
        _pathCornerIndex = 0;
        _pathCorners = [];

        if (result.Status is PluginNavigationPathStatus.Loading
            or PluginNavigationPathStatus.Unavailable)
        {
            _pathRetryAt = _mover.Now + PathLoadingRetrySeconds;
            return;
        }
        if (result.Status != PluginNavigationPathStatus.Complete)
            return;
        if (result.Corners is null || result.Corners.Count == 0)
        {
            _pathStatus = PluginNavigationPathStatus.Unreachable;
            return;
        }

        var detached = new PluginNavigationPosition[result.Corners.Count];
        for (int index = 0; index < detached.Length; index++)
        {
            PluginNavigationPosition corner = result.Corners[index];
            if (!Finite(corner))
            {
                _pathStatus = PluginNavigationPathStatus.InvalidPosition;
                return;
            }
            detached[index] = corner;
        }

        _pathCorners = detached;
        _pathSegmentStart = current;
    }

    private void ObservePathPosition(in PluginNavigationSnapshot snapshot)
    {
        if (!_hasPathObservedPosition)
        {
            CaptureObservedPathPosition(snapshot);
            return;
        }

        bool discontinuity = SpatialDistanceMeters(
                snapshot.Position,
                _pathObservedPosition) > PathDiscontinuityMeters;
        if (!discontinuity
            && snapshot.ConfirmedPositionRevision != 0UL
            && _pathObservedConfirmedRevision != 0UL
            && snapshot.ConfirmedPositionRevision
                != _pathObservedConfirmedRevision)
        {
            discontinuity = SpatialDistanceMeters(
                    snapshot.ConfirmedPosition,
                    _pathObservedConfirmedPosition)
                > PathDiscontinuityMeters;
        }

        if (discontinuity)
            ClearNavigationPath();
        CaptureObservedPathPosition(snapshot);
    }

    private void CaptureObservedPathPosition(
        in PluginNavigationSnapshot snapshot)
    {
        _pathObservedPosition = snapshot.Position;
        _pathObservedConfirmedPosition = snapshot.ConfirmedPosition;
        _pathObservedConfirmedRevision = snapshot.ConfirmedPositionRevision;
        _hasPathObservedPosition = true;
    }

    private void ClearNavigationPath()
    {
        _pathLeg = null;
        _pathCorners = [];
        _pathCornerIndex = 0;
        _pathStatus = PluginNavigationPathStatus.Unavailable;
        _pathHasPlanResult = false;
        _pathMeshRevision = 0;
        _pathRetryAt = 0d;
        _pathSegmentStart = default;
        _pathObservedPosition = default;
        _pathObservedConfirmedPosition = default;
        _pathObservedConfirmedRevision = 0UL;
        _hasPathObservedPosition = false;
    }

    private static string PathFailureStatus(
        string label,
        PluginNavigationPathStatus status) => status switch
        {
            PluginNavigationPathStatus.Loading =>
                $"{label}: loading navigation geometry.",
            PluginNavigationPathStatus.MissingTiles =>
                $"{label}: navigation geometry is missing.",
            PluginNavigationPathStatus.CorruptTiles =>
                $"{label}: navigation geometry is corrupt.",
            PluginNavigationPathStatus.InvalidPosition =>
                $"{label}: destination is invalid.",
            PluginNavigationPathStatus.StartOutsideMesh =>
                $"{label}: current position is outside traversable geometry.",
            PluginNavigationPathStatus.GoalOutsideMesh =>
                $"{label}: destination is outside traversable geometry.",
            PluginNavigationPathStatus.Unreachable =>
                $"{label}: destination is unreachable.",
            PluginNavigationPathStatus.CapacityExceeded =>
                $"{label}: route exceeds the navigation range.",
            PluginNavigationPathStatus.SearchLimitReached =>
                $"{label}: no route found within the navigation search area.",
            _ => $"{label}: navigation path is unavailable.",
        };

    internal long PathMeshRevision => _pathMeshRevision;

    private static bool Finite(in PluginNavigationPosition position) =>
        double.IsFinite(position.EastWest)
        && double.IsFinite(position.NorthSouth)
        && double.IsFinite(position.Elevation)
        && float.IsFinite(position.HeadingDegrees);

    private static double DistanceToSpatialSegmentMeters(
        in PluginNavigationPosition point,
        in PluginNavigationPosition start,
        in PluginNavigationPosition end)
    {
        double abX = end.EastWest - start.EastWest;
        double abY = end.NorthSouth - start.NorthSouth;
        double abZ = end.Elevation - start.Elevation;
        double lengthSquared = abX * abX + abY * abY + abZ * abZ;
        if (lengthSquared <= double.Epsilon)
            return SpatialDistanceMeters(point, start);

        double apX = point.EastWest - start.EastWest;
        double apY = point.NorthSouth - start.NorthSouth;
        double apZ = point.Elevation - start.Elevation;
        double t = Math.Clamp(
            (apX * abX + apY * abY + apZ * abZ) / lengthSquared,
            0d,
            1d);
        double dx = point.EastWest - (start.EastWest + t * abX);
        double dy = point.NorthSouth - (start.NorthSouth + t * abY);
        double dz = point.Elevation - (start.Elevation + t * abZ);
        return Math.Sqrt(dx * dx + dy * dy + dz * dz) * 240d;
    }

    private readonly record struct NavigationPathLeg(
        RouteMode Mode,
        int Index,
        bool Reverse,
        RouteWaypointType WaypointType,
        uint ObjectId,
        uint CellId,
        double EastWest,
        double NorthSouth,
        double Elevation,
        bool IsOutdoor);
}
