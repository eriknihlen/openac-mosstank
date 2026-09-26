// Adapted from metaf by Eskarina of Morningthaw and Coldeve,
// https://github.com/JJEII/metaf (GPLv3). See NOTICE.md.

using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal static class VtankNavRouteSerializer
{
    private const string Header = "uTank2 NAV 1.2";

    public static bool TryLoad(
        string source,
        NavigationSettings target,
        ISpellCatalog spells,
        out string error)
    {
        ArgumentNullException.ThrowIfNull(target);
        ArgumentNullException.ThrowIfNull(spells);
        try
        {
            string nav = UnwrapEmbedded(source);
            using var reader = new StringReader(nav);
            if (!ReadLine(reader).Equals(Header, StringComparison.Ordinal))
                throw new FormatException("Nav file version does not match uTank2 NAV 1.2.");

            var parsed = new NavigationSettings
            {
                Enabled = target.Enabled,
                Priority = target.Priority,
                MinimumDistanceMeters = target.MinimumDistanceMeters,
                FollowAroundCorners = target.FollowAroundCorners,
                OpenDoors = target.OpenDoors,
                Mode = ReadInt(reader) switch
                {
                    1 => RouteMode.Circular,
                    2 => RouteMode.Linear,
                    3 => RouteMode.Target,
                    4 => RouteMode.Once,
                    _ => throw new FormatException("Unknown VTank navigation type."),
                },
            };

            if (parsed.Mode == RouteMode.Target)
            {
                parsed.FollowTargetName = ReadLine(reader);
                parsed.FollowTargetObjectId = unchecked((uint)ReadInt(reader));
            }
            else
            {
                int count = ReadInt(reader);
                if (count is < 0 or > 100_000)
                    throw new FormatException("Invalid VTank waypoint count.");
                for (int index = 0; index < count; index++)
                    parsed.Waypoints.Add(ReadWaypoint(reader, spells));
            }

            Apply(parsed, target);
            error = string.Empty;
            return true;
        }
        catch (Exception exception) when (exception is FormatException
            or OverflowException or EndOfStreamException)
        {
            error = exception.Message;
            return false;
        }
    }

    /// <summary>
    /// The route in the older ".nav" form, line for line as that form's own
    /// writer lays it out, so a route loaded from a ".nav" saves back to the
    /// same bytes and the tool that owns the form reads what is written.
    /// </summary>
    /// <exception cref="InvalidOperationException">
    /// The route holds something the form has no way to say. Nothing is
    /// written in that case rather than a file that quietly lost it.
    /// </exception>
    public static string Save(NavigationSettings source)
    {
        ArgumentNullException.ThrowIfNull(source);
        var lines = new List<string>();
        WriteRoute(lines, source);
        return string.Join("\r\n", lines) + "\r\n";
    }

    /// <summary>
    /// The route's lines in the ".nav" form, from the header on, for a file
    /// of its own or for a route embedded in a meta.
    /// </summary>
    internal static void WriteRoute(List<string> lines, NavigationSettings source)
    {
        ArgumentNullException.ThrowIfNull(lines);
        ArgumentNullException.ThrowIfNull(source);
        lines.Add(Header);
        lines.Add(source.Mode switch
        {
            RouteMode.Circular => "1",
            RouteMode.Linear => "2",
            RouteMode.Target => "3",
            RouteMode.Once => "4",
            _ => throw new InvalidOperationException(
                $"A .nav file has no route mode {source.Mode}."),
        });
        if (source.Mode == RouteMode.Target)
        {
            // A follow route is its target and nothing else.
            if (source.Waypoints.Count != 0)
            {
                throw new InvalidOperationException(
                    "A .nav follow route cannot also hold waypoints.");
            }
            lines.Add(source.FollowTargetName);
            lines.Add(FormatInt(unchecked((int)source.FollowTargetObjectId)));
            return;
        }
        lines.Add(FormatInt(source.Waypoints.Count));
        foreach (RouteWaypoint waypoint in source.Waypoints)
            WriteWaypoint(lines, waypoint);
    }

    private static void WriteWaypoint(List<string> lines, RouteWaypoint waypoint)
    {
        lines.Add(waypoint.Type switch
        {
            RouteWaypointType.Point => "0",
            RouteWaypointType.Portal => "1",
            RouteWaypointType.Recall => "2",
            RouteWaypointType.Pause => "3",
            RouteWaypointType.ChatCommand => "4",
            RouteWaypointType.OpenVendor => "5",
            RouteWaypointType.PortalByName => "6",
            RouteWaypointType.UseNpc => "7",
            RouteWaypointType.Checkpoint => "8",
            RouteWaypointType.Jump => "9",
            _ => throw new InvalidOperationException(
                $"A .nav file has no waypoint type {waypoint.Type}."),
        });
        lines.Add(FormatDouble(waypoint.Position.EastWest));
        lines.Add(FormatDouble(waypoint.Position.NorthSouth));
        lines.Add(FormatDouble(waypoint.Position.Elevation));
        lines.Add("0");
        switch (waypoint.Type)
        {
            case RouteWaypointType.Portal:
                lines.Add(FormatInt(unchecked((int)waypoint.ObjectId)));
                break;
            case RouteWaypointType.Recall:
                lines.Add(FormatInt(checked((int)RecallSpellId(waypoint))));
                break;
            case RouteWaypointType.Pause:
                lines.Add(FormatInt(waypoint.DurationMilliseconds));
                break;
            case RouteWaypointType.ChatCommand:
                lines.Add(SingleLine(waypoint.Text, "chat command"));
                break;
            case RouteWaypointType.OpenVendor:
                lines.Add(FormatInt(unchecked((int)waypoint.ObjectId)));
                lines.Add(SingleLine(waypoint.ObjectName, "vendor name"));
                break;
            case RouteWaypointType.PortalByName:
            case RouteWaypointType.UseNpc:
                lines.Add(SingleLine(waypoint.ObjectName, "object name"));
                lines.Add(FormatInt(waypoint.LegacyObjectClass));
                lines.Add(waypoint.LegacyReferenceValid ? "True" : "False");
                lines.Add(FormatDouble(waypoint.ReferencePosition.EastWest));
                lines.Add(FormatDouble(waypoint.ReferencePosition.NorthSouth));
                lines.Add(FormatDouble(waypoint.ReferencePosition.Elevation));
                break;
            case RouteWaypointType.Jump:
                if (waypoint.JumpDirection == RouteJumpDirection.Backward)
                {
                    throw new InvalidOperationException(
                        "A .nav file has no backward jump.");
                }
                lines.Add(FormatDouble(waypoint.JumpHeadingDegrees));
                lines.Add(waypoint.JumpHoldShift ? "True" : "False");
                lines.Add(FormatJumpCharge(waypoint));
                break;
        }
    }

    /// <summary>
    /// The recall's spell: the one it was read with, or the one its kind
    /// stands for when it was made here.
    /// </summary>
    private static uint RecallSpellId(RouteWaypoint waypoint) =>
        waypoint.RecallSpellId != 0u
            ? waypoint.RecallSpellId
            : RouteWaypoint.SpellIdForRecall(waypoint.Recall);

    private static string SingleLine(string value, string what)
    {
        string text = value ?? string.Empty;
        if (text.Contains('\n', StringComparison.Ordinal)
            || text.Contains('\r', StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                $"A .nav file cannot hold a {what} that runs over more than one line.");
        }
        return text;
    }

    private static string FormatInt(int value) =>
        value.ToString(CultureInfo.InvariantCulture);

    /// <summary>
    /// A number as the shortest text that reads back as exactly it, and zero
    /// without a sign. The form's own writer prints fifteen significant
    /// digits, and for every number in its files the shortest such text is
    /// those same digits, so a file it wrote saves back unchanged; a number
    /// fifteen digits cannot hold, from another tool or measured here, keeps
    /// every digit it needs, so a save never moves a point.
    /// </summary>
    internal static string FormatDouble(double value)
    {
        if (!double.IsFinite(value))
            throw new InvalidOperationException("A .nav file cannot hold a number that is not finite.");
        return value == 0d
            ? "0"
            : value.ToString("R", CultureInfo.InvariantCulture);
    }

    private static RouteWaypoint ReadWaypoint(
        TextReader reader,
        ISpellCatalog spells)
    {
        int type = ReadInt(reader);
        double eastWest = ReadDouble(reader);
        double northSouth = ReadDouble(reader);
        double elevation = ReadDouble(reader);
        _ = ReadLine(reader);
        var waypoint = new RouteWaypoint
        {
            Type = type switch
            {
                0 => RouteWaypointType.Point,
                1 => RouteWaypointType.Portal,
                2 => RouteWaypointType.Recall,
                3 => RouteWaypointType.Pause,
                4 => RouteWaypointType.ChatCommand,
                5 => RouteWaypointType.OpenVendor,
                6 => RouteWaypointType.PortalByName,
                7 => RouteWaypointType.UseNpc,
                8 => RouteWaypointType.Checkpoint,
                9 => RouteWaypointType.Jump,
                _ => throw new FormatException($"Unknown VTank waypoint type {type}."),
            },
            Position = Position(eastWest, northSouth, elevation),
        };

        switch (type)
        {
            case 1:
                waypoint.ObjectId = unchecked((uint)ReadInt(reader));
                break;
            case 2:
                waypoint.RecallSpellId = checked((uint)ReadInt(reader));
                if (spells.TryGet(waypoint.RecallSpellId, out PluginSpellInfo spell))
                    waypoint.RecallSpellName = spell.Name;
                // Without a loaded spell table the name would stay empty and a
                // metaf export would write an unresolvable `rcl {}`; the recall
                // table names the waypoint whether the catalogue answers or not.
                if (RouteWaypoint.TryResolveRecallKind(
                    waypoint.RecallSpellId,
                    out RouteRecallKind recall))
                {
                    waypoint.Recall = recall;
                    if (string.IsNullOrWhiteSpace(waypoint.RecallSpellName))
                        waypoint.RecallSpellName = RouteWaypoint.RecallDisplayName(recall);
                }
                break;
            case 3:
                waypoint.DurationMilliseconds = ReadInt(reader);
                break;
            case 4:
                waypoint.Text = ReadLine(reader);
                break;
            case 5:
                waypoint.ObjectId = unchecked((uint)ReadInt(reader));
                waypoint.ObjectName = ReadLine(reader);
                break;
            case 6:
            case 7:
                waypoint.ObjectName = ReadLine(reader);
                waypoint.LegacyObjectClass = ReadInt(reader);
                waypoint.LegacyReferenceValid = ReadBoolean(reader);
                double referenceEastWest = ReadDouble(reader);
                double referenceNorthSouth = ReadDouble(reader);
                double referenceElevation = ReadDouble(reader);
                waypoint.ReferencePosition = Position(
                    referenceEastWest,
                    referenceNorthSouth,
                    referenceElevation);
                break;
            case 9:
                waypoint.JumpHeadingDegrees = ReadDouble(reader);
                waypoint.JumpHoldShift = ReadBoolean(reader);
                ParseJump(ReadLine(reader), waypoint);
                break;
        }
        return waypoint;
    }

    /// <summary>
    /// A jump's charge time with its direction riding in the fifth decimal,
    /// 3 forward, 4 strafe left, 5 strafe right, printed as the form prints
    /// any number: 50 milliseconds forward is "50.00003".
    /// </summary>
    private static string FormatJumpCharge(RouteWaypoint waypoint)
    {
        int direction = waypoint.JumpDirection switch
        {
            RouteJumpDirection.StrafeLeft => 4,
            RouteJumpDirection.StrafeRight => 5,
            _ => 3,
        };
        return FormatDouble(waypoint.JumpChargeMilliseconds + (direction / 100_000d));
    }

    private static void ParseJump(string source, RouteWaypoint target)
    {
        string value = source.Trim();
        // A charge of nothing is its direction code alone, which the form
        // prints in exponent notation: "4E-05" is a strafe left on the spot.
        double whole = double.Parse(value, NumberStyles.Float, CultureInfo.InvariantCulture);
        if (whole is > 0d and < 0.0001d)
        {
            target.JumpChargeMilliseconds = 0;
            target.JumpDirection = Math.Round(whole * 100_000d) switch
            {
                4d => RouteJumpDirection.StrafeLeft,
                5d => RouteJumpDirection.StrafeRight,
                _ => RouteJumpDirection.Forward,
            };
            return;
        }
        char suffix = value.Length == 0 ? '\0' : value[^1];
        bool encoded = suffix is '3' or '4' or '5'
            && value.Length >= 6
            && value[^6] == '.';
        string milliseconds = encoded ? value[..^1] : value;
        target.JumpChargeMilliseconds = checked((int)Math.Round(
            double.Parse(milliseconds, NumberStyles.Float, CultureInfo.InvariantCulture),
            MidpointRounding.AwayFromZero));
        target.JumpDirection = suffix switch
        {
            '4' when encoded => RouteJumpDirection.StrafeLeft,
            '5' when encoded => RouteJumpDirection.StrafeRight,
            _ => RouteJumpDirection.Forward,
        };
    }

    private static string UnwrapEmbedded(string source)
    {
        string normalized = source?.Replace("\r\n", "\n", StringComparison.Ordinal)
            ?? string.Empty;
        if (normalized.StartsWith(Header, StringComparison.Ordinal))
            return normalized;
        using var reader = new StringReader(normalized);
        _ = ReadLine(reader); // embedded route display name
        _ = ReadInt(reader);
        return reader.ReadToEnd();
    }

    private static PluginNavigationPosition Position(
        double eastWest,
        double northSouth,
        double elevation) => new(
            0u,
            eastWest,
            northSouth,
            elevation,
            0f,
            IsOutdoor: true);

    internal static void Apply(NavigationSettings source, NavigationSettings target)
    {
        target.Mode = source.Mode;
        target.FollowTargetObjectId = source.FollowTargetObjectId;
        target.FollowTargetName = source.FollowTargetName;
        target.Waypoints.Clear();
        target.Waypoints.AddRange(source.Waypoints.Select(static value => value.Clone()));
    }

    private static string ReadLine(TextReader reader) =>
        reader.ReadLine() ?? throw new EndOfStreamException("Unexpected end of VTank nav data.");

    private static int ReadInt(TextReader reader) => int.Parse(
        ReadLine(reader),
        NumberStyles.Integer,
        CultureInfo.InvariantCulture);

    private static double ReadDouble(TextReader reader) => double.Parse(
        ReadLine(reader),
        NumberStyles.Float,
        CultureInfo.InvariantCulture);

    private static bool ReadBoolean(TextReader reader) => bool.Parse(ReadLine(reader));
}
