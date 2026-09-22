using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum RouteMode
{
    Circular,
    Linear,
    Target,
    Once,
}

/// <summary>When the route hands a leg to the client's own pathing.</summary>
internal enum ClientPathing
{
    /// <summary>The reference's straight walk at every point; a stall is only said in chat.</summary>
    Never,

    /// <summary>
    /// The straight walk, and once per visit to a waypoint, when it has not
    /// covered ground for three seconds, the client walks that leg; the route
    /// takes the keys back however the client's walk ends.
    /// </summary>
    WhenStuck,

    /// <summary>Every leg goes to the client; a leg it cannot walk pauses the route.</summary>
    Always,
}

internal enum RouteWaypointType
{
    Point = 0,
    Portal = 1,
    Recall = 2,
    Pause = 3,
    ChatCommand = 4,
    OpenVendor = 5,
    PortalByName = 6,
    UseNpc = 7,
    Checkpoint = 8,
    Jump = 9,
}

internal enum RouteRecallKind
{
    PrimaryPortalRecall,
    SecondaryPortalRecall,
    LifestoneRecall,
    LifestoneSending,
    PortalRecall,
    RecallAphusLassel,
    RecallTheSanctuary,
    RecallToTheSingularityCaul,
    GlendenWoodRecall,
    AerlintheRecall,
    MountLetheRecall,
    UlgrimsRecall,
    BurRecall,
    ParadoxTouchedOlthoiInfestedAreaRecall,
    CallOfTheMhoireForge,
    ColosseumRecall,
    FacilityHubRecall,
    GearKnightInvasionAreaCampRecall,
    LostCityOfNeftetRecall,
    ReturnToTheKeep,
    RynthidRecall,
    ViridianRiseRecall,
    ViridianRiseGreatTreeRecall,
    CelestialHandStrongholdRecall,
    RadiantBloodStrongholdRecall,
    EldrytchWebStrongholdRecall,
    Marketplace,
}

internal enum RouteJumpDirection
{
    Forward,
    StrafeLeft,
    StrafeRight,

    /// <summary>
    /// Appended rather than placed beside <see cref="Forward"/>: saved routes
    /// carry this enum by number, so an existing profile's strafes must keep
    /// the numbers they were written with.
    /// </summary>
    Backward,
}

internal enum RouteInsertMode
{
    AddToEnd,
    InsertAbove,
    InsertBelow,
}

internal sealed class RouteWaypoint
{
    public RouteWaypointType Type { get; set; }
    public PluginNavigationPosition Position { get; set; }
    public PluginNavigationPosition ReferencePosition { get; set; }
    public uint ObjectId { get; set; }
    public string ObjectName { get; set; } = string.Empty;
    /// <summary>
    /// The object-class number the VTank NAV format carries, kept exactly as
    /// written for interchange.
    /// </summary>
    public int LegacyObjectClass { get; set; }
    public bool LegacyReferenceValid { get; set; } = true;
    public string Text { get; set; } = string.Empty;
    public int DurationMilliseconds { get; set; } = 5000;
    public RouteRecallKind Recall { get; set; }
    public uint RecallSpellId { get; set; }
    public string RecallSpellName { get; set; } = string.Empty;
    public float JumpHeadingDegrees { get; set; }
    public bool JumpRun { get; set; }
    public int JumpChargeMilliseconds { get; set; } = 1000;
    public RouteJumpDirection JumpDirection { get; set; }

    public RouteWaypoint Clone() => new()
    {
        Type = Type,
        Position = Position,
        ReferencePosition = ReferencePosition,
        ObjectId = ObjectId,
        ObjectName = ObjectName,
        LegacyObjectClass = LegacyObjectClass,
        LegacyReferenceValid = LegacyReferenceValid,
        Text = Text,
        DurationMilliseconds = DurationMilliseconds,
        Recall = Recall,
        RecallSpellId = RecallSpellId,
        RecallSpellName = RecallSpellName,
        JumpHeadingDegrees = JumpHeadingDegrees,
        JumpRun = JumpRun,
        JumpChargeMilliseconds = JumpChargeMilliseconds,
        JumpDirection = JumpDirection,
    };

    public string DisplayText => Type switch
    {
        RouteWaypointType.Point => $"Point: {FormatPosition(Position)}",
        RouteWaypointType.Portal => $"Portal: {ObjectLabel}",
        RouteWaypointType.Recall => $"Recall: {RecallLabel}",
        RouteWaypointType.Pause => string.Create(
            CultureInfo.InvariantCulture,
            $"Pause: {DurationMilliseconds / 1000d:0.###} seconds"),
        RouteWaypointType.ChatCommand => $"Chat command: {Text}",
        RouteWaypointType.OpenVendor => ObjectId == 0u
            ? "Close Vendor"
            : $"Open Vendor: {ObjectLabel}",
        RouteWaypointType.PortalByName => $"Portal: {ObjectLabel}",
        RouteWaypointType.UseNpc => $"Use NPC: {ObjectLabel}",
        RouteWaypointType.Checkpoint => $"Checkpoint: {FormatPosition(Position)}",
        RouteWaypointType.Jump =>
            $"Jump: {JumpHeadingDegrees.ToString("0.0", CultureInfo.InvariantCulture)}d, "
            + $"{JumpChargeMilliseconds.ToString(CultureInfo.InvariantCulture)}ms"
            + (JumpRun ? ", Shift" : string.Empty)
            + $", {JumpDirectionDisplayName(JumpDirection)}",
        _ => Type.ToString(),
    };

    private string ObjectLabel => string.IsNullOrWhiteSpace(ObjectName)
        ? $"0x{ObjectId:X8}"
        : ObjectName;

    internal string RecallLabel => !string.IsNullOrWhiteSpace(RecallSpellName)
        ? RecallSpellName
        : RecallSpellId != 0u
            ? RecallSpellId.ToString(CultureInfo.InvariantCulture)
            : RecallDisplayName(Recall);

    internal static string FormatPosition(in PluginNavigationPosition value)
    {
        string northSouth = value.NorthSouth >= 0d ? "N" : "S";
        string eastWest = value.EastWest >= 0d ? "E" : "W";
        return "("
            + Math.Abs(value.NorthSouth).ToString("0.###", CultureInfo.InvariantCulture)
            + northSouth
            + ", "
            + Math.Abs(value.EastWest).ToString("0.###", CultureInfo.InvariantCulture)
            + eastWest
            + ")";
    }

    internal static string RecallDisplayName(RouteRecallKind value) => value switch
    {
        RouteRecallKind.PrimaryPortalRecall => "Primary Portal Recall",
        RouteRecallKind.SecondaryPortalRecall => "Secondary Portal Recall",
        RouteRecallKind.LifestoneRecall => "Lifestone Recall",
        RouteRecallKind.LifestoneSending => "Lifestone Sending",
        RouteRecallKind.PortalRecall => "Portal Recall",
        RouteRecallKind.RecallAphusLassel => "Recall Aphus Lassel",
        RouteRecallKind.RecallTheSanctuary => "Recall the Sanctuary",
        RouteRecallKind.RecallToTheSingularityCaul => "Recall to the Singularity Caul",
        RouteRecallKind.GlendenWoodRecall => "Glenden Wood Recall",
        RouteRecallKind.AerlintheRecall => "Aerlinthe Recall",
        RouteRecallKind.MountLetheRecall => "Mount Lethe Recall",
        RouteRecallKind.UlgrimsRecall => "Ulgrim's Recall",
        RouteRecallKind.BurRecall => "Bur Recall",
        RouteRecallKind.ParadoxTouchedOlthoiInfestedAreaRecall =>
            "Paradox-touched Olthoi Infested Area Recall",
        RouteRecallKind.CallOfTheMhoireForge => "Call of the Mhoire Forge",
        RouteRecallKind.ColosseumRecall => "Colosseum Recall",
        RouteRecallKind.FacilityHubRecall => "Facility Hub Recall",
        RouteRecallKind.GearKnightInvasionAreaCampRecall =>
            "Gear Knight Invasion Area Camp Recall",
        RouteRecallKind.LostCityOfNeftetRecall => "Lost City of Neftet Recall",
        RouteRecallKind.ReturnToTheKeep => "Return to the Keep",
        RouteRecallKind.RynthidRecall => "Rynthid Recall",
        RouteRecallKind.ViridianRiseRecall => "Viridian Rise Recall",
        RouteRecallKind.ViridianRiseGreatTreeRecall => "Viridian Rise Great Tree Recall",
        RouteRecallKind.CelestialHandStrongholdRecall => "Celestial Hand Stronghold Recall",
        RouteRecallKind.RadiantBloodStrongholdRecall => "Radiant Blood Stronghold Recall",
        RouteRecallKind.EldrytchWebStrongholdRecall => "Eldrytch Web Stronghold Recall",
        RouteRecallKind.Marketplace => "Marketplace Recall",
        _ => value.ToString(),
    };

    /// <summary>
    /// Matches a written recall name — the display name this table prints, or
    /// the bare enum name — against the recall list, ignoring case. A recall is
    /// not a buff and not a combat spell, so the spell catalogue's known-spell
    /// lists never carry one; this table is where a recall name resolves.
    /// </summary>
    internal static bool TryResolveRecallKind(string? name, out RouteRecallKind kind)
    {
        kind = default;
        string trimmed = name?.Trim() ?? string.Empty;
        if (trimmed.Length == 0)
            return false;
        foreach (RouteRecallKind candidate in Enum.GetValues<RouteRecallKind>())
        {
            if (RecallDisplayName(candidate).Equals(trimmed, StringComparison.OrdinalIgnoreCase)
                || candidate.ToString().Equals(trimmed, StringComparison.OrdinalIgnoreCase))
            {
                kind = candidate;
                return true;
            }
        }
        return false;
    }

    /// <summary>The same table read the other way: a spell id back to its recall.</summary>
    internal static bool TryResolveRecallKind(uint spellId, out RouteRecallKind kind)
    {
        kind = default;
        if (spellId == 0u)
            return false;
        foreach (RouteRecallKind candidate in Enum.GetValues<RouteRecallKind>())
        {
            if (SpellIdForRecall(candidate) == spellId)
            {
                kind = candidate;
                return true;
            }
        }
        return false;
    }

    /// <summary>
    /// Fills in a recall waypoint's spell from the name it was written with:
    /// the recall table first, then whatever the spell catalogue offers. It is
    /// called again when the waypoint runs, because a route can be read before
    /// the character's spell tables are available.
    /// </summary>
    internal static void ResolveRecallSpell(RouteWaypoint waypoint, ISpellCatalog? spells)
    {
        if (waypoint.RecallSpellId != 0u)
            return;
        string name = waypoint.RecallSpellName?.Trim() ?? string.Empty;
        if (name.Length == 0)
            return;
        if (TryResolveRecallKind(name, out RouteRecallKind kind))
        {
            waypoint.Recall = kind;
            // Marketplace has no spell: it stays at id 0 and is submitted as a
            // command, which is why the tick guard exempts that one kind.
            waypoint.RecallSpellId = SpellIdForRecall(kind);
            return;
        }
        waypoint.RecallSpellId = ResolveCatalogSpellIdByName(spells, name);
    }

    /// <summary>
    /// The catalogue fallback: the whole content table by exact name, then the
    /// three known-spell lists for a host that does not carry the full table.
    /// </summary>
    private static uint ResolveCatalogSpellIdByName(ISpellCatalog? spells, string name)
    {
        if (spells is null)
            return 0u;
        if (spells.TryFindByName(name, partialMatch: false, out PluginSpellInfo found)
            && found.SpellId != 0u)
        {
            return found.SpellId;
        }
        foreach (IReadOnlyList<PluginSpellInfo> list in
            (IReadOnlyList<PluginSpellInfo>[])
            [spells.KnownSelfBuffs, spells.KnownAttackSpells, spells.KnownCombatSpells])
        {
            foreach (PluginSpellInfo spell in list)
            {
                if (spell.Name.Equals(name, StringComparison.OrdinalIgnoreCase))
                    return spell.SpellId;
            }
        }
        return 0u;
    }

    internal static string RecallShortCaption(RouteRecallKind value) => value switch
    {
        RouteRecallKind.PrimaryPortalRecall => "Primary",
        RouteRecallKind.SecondaryPortalRecall => "Secondary",
        RouteRecallKind.LifestoneRecall => "LS",
        RouteRecallKind.LifestoneSending => "LS Sending",
        RouteRecallKind.PortalRecall => "Portal",
        RouteRecallKind.RecallAphusLassel => "Aphus",
        RouteRecallKind.RecallTheSanctuary => "Sanctuary",
        RouteRecallKind.RecallToTheSingularityCaul => "Caul",
        RouteRecallKind.GlendenWoodRecall => "GW",
        RouteRecallKind.AerlintheRecall => "Aerlinthe",
        RouteRecallKind.MountLetheRecall => "Mt. Lethe",
        RouteRecallKind.UlgrimsRecall => "Ulgrim's",
        RouteRecallKind.BurRecall => "Bur",
        RouteRecallKind.ParadoxTouchedOlthoiInfestedAreaRecall => "PtOIA",
        RouteRecallKind.CallOfTheMhoireForge => "Graveyard",
        RouteRecallKind.ColosseumRecall => "Colosseum",
        RouteRecallKind.FacilityHubRecall => "Fac. Hub",
        RouteRecallKind.GearKnightInvasionAreaCampRecall => "Gear K. Camp",
        RouteRecallKind.LostCityOfNeftetRecall => "Neftet",
        RouteRecallKind.ReturnToTheKeep => "Candeth",
        RouteRecallKind.RynthidRecall => "Rynthid",
        RouteRecallKind.ViridianRiseRecall => "VR Rocks",
        RouteRecallKind.ViridianRiseGreatTreeRecall => "VR Tree",
        RouteRecallKind.CelestialHandStrongholdRecall => "Soc. CH",
        RouteRecallKind.RadiantBloodStrongholdRecall => "Soc. RB",
        RouteRecallKind.EldrytchWebStrongholdRecall => "Soc. EW",
        RouteRecallKind.Marketplace => "Marketplace",
        _ => value.ToString(),
    };

    internal static uint SpellIdForRecall(RouteRecallKind value) => value switch
    {
        RouteRecallKind.PrimaryPortalRecall => 48u,
        RouteRecallKind.SecondaryPortalRecall => 2647u,
        RouteRecallKind.LifestoneRecall => 1635u,
        RouteRecallKind.LifestoneSending => 1636u,
        RouteRecallKind.PortalRecall => 2645u,
        RouteRecallKind.RecallAphusLassel => 2931u,
        RouteRecallKind.RecallTheSanctuary => 2023u,
        RouteRecallKind.RecallToTheSingularityCaul => 2943u,
        RouteRecallKind.GlendenWoodRecall => 3865u,
        RouteRecallKind.AerlintheRecall => 2041u,
        RouteRecallKind.MountLetheRecall => 2813u,
        RouteRecallKind.UlgrimsRecall => 2941u,
        RouteRecallKind.BurRecall => 4084u,
        RouteRecallKind.ParadoxTouchedOlthoiInfestedAreaRecall => 4198u,
        RouteRecallKind.CallOfTheMhoireForge => 4128u,
        RouteRecallKind.ColosseumRecall => 4213u,
        RouteRecallKind.FacilityHubRecall => 5175u,
        RouteRecallKind.GearKnightInvasionAreaCampRecall => 5330u,
        RouteRecallKind.LostCityOfNeftetRecall => 5541u,
        RouteRecallKind.ReturnToTheKeep => 4214u,
        RouteRecallKind.RynthidRecall => 6150u,
        RouteRecallKind.ViridianRiseRecall => 6321u,
        RouteRecallKind.ViridianRiseGreatTreeRecall => 6322u,
        RouteRecallKind.CelestialHandStrongholdRecall => 6325u,
        RouteRecallKind.RadiantBloodStrongholdRecall => 6327u,
        RouteRecallKind.EldrytchWebStrongholdRecall => 6326u,
        _ => 0u,
    };

    private static string JumpDirectionDisplayName(RouteJumpDirection value) =>
        value switch
        {
            RouteJumpDirection.StrafeLeft => "Strafe Left",
            RouteJumpDirection.StrafeRight => "Strafe Right",
            RouteJumpDirection.Backward => "Backward",
            _ => "Forward",
        };
}

internal sealed class NavigationSettings
{
    /// <summary>The reference client's "Show Nav Lines": draw the loaded route in the world.</summary>
    public bool ShowNavLines { get; set; }
    public bool Enabled { get; set; }
    public bool Priority { get; set; }
    public RouteMode Mode { get; set; } = RouteMode.Circular;
    public double MinimumDistanceMeters { get; set; } = 2d;
    /// <summary>
    /// VTank's far stop range is the outer validity bound for a navigation
    /// rule, not a second arrival threshold.
    /// </summary>
    public double MaximumDistanceMeters { get; set; } = 999999d * 240d;
    public double PortalUseDistanceMeters { get; set; } = 4d;
    public uint FollowTargetObjectId { get; set; }
    public string FollowTargetName { get; set; } = string.Empty;
    public bool FollowAroundCorners { get; set; } = true;
    public ClientPathing ClientPathing { get; set; } = ClientPathing.WhenStuck;
    public bool OpenDoors { get; set; }
    public double DoorIdentifyRangeMeters { get; set; } = 20d;
    public double DoorOpenRangeMeters { get; set; } = 4d;
    public int DoorLockpickExcessThreshold { get; set; } = -50;

    /// <summary>
    /// How many times a jump asked for by command charges before it gives up.
    /// A charge the client never turns into a jump is silent, so without a
    /// ceiling one refusal would hold the character for ever.
    /// </summary>
    public int JumpAttempts { get; set; } = 3;
    public List<RouteWaypoint> Waypoints { get; } = [];
}

internal sealed class NavigationController
{
    // The mover's own numbers, named here too because the route is not the
    // only reader: the combat approach and the tests reach for them through
    // this class.
    internal const float HeadingToleranceDegrees =
        NavigationMover.HeadingToleranceDegrees;

    internal const double MoverIntervalSeconds =
        NavigationMover.MoverIntervalSeconds;

    internal const double FaceHeadingReissueSeconds =
        NavigationMover.FaceHeadingReissueSeconds;

    internal const double NoFaceHeadingStamp = NavigationMover.NoFaceHeadingStamp;
    private const double ChatInitialDelaySeconds = 0.2d;
    private const double UseRetrySeconds = 2d;
    private const double PortalTimeoutSeconds = 30d;
    private const double ObjectReacquireRadiusMeters = 2.5d;
    private const double PortalExitDistanceMeters = 15d;
    private const double RecallExitDistanceMeters = 2.4d;

    /// <summary>
    /// How far the character may have drifted between ticks and still count as
    /// standing still for a recall.
    /// </summary>
    private const double RecallStationaryToleranceMeters = 2.4d;
    private const double JumpLaunchGraceSeconds = 0.25d;
    private const double JumpCompletionTimeoutSeconds = 3d;
    private const int JumpChargeCeilingMilliseconds = 2000;

    /// <summary>
    /// A jump is aimed near-exactly, not to the walking band: the alignment
    /// state holds until the heading error is under a hundredth of a degree.
    /// </summary>
    private const float JumpHeadingToleranceDegrees = 0.01f;

    /// <summary>The jump's own re-face interval, far longer than the walk's.</summary>
    private const double JumpFaceHeadingReissueSeconds = 2d;

    /// <summary>The log-text type an NPC's "tells you," answer arrives with.</summary>
    private const uint NpcTellLogTextType = 3u;

    /// <summary>The log-text type an NPC's "gives you" line arrives with.</summary>
    private const uint NpcGiveLogTextType = 0u;
    private const double CheckpointRetrySeconds = 15d;
    private const double FollowBreadcrumbSpacingMeters = 0.096d;
    private const double FollowPathCaptureRangeMeters = 240d;
    private const double FollowPathArrivalMeters = 2.4d;
    private const double DoorActionTimeoutSeconds = 5d;
    private const uint LockpickPublicFlag = 0x00020000u;
    private const uint LockpickSkillId = 23u;
    private const uint LockpickModifierProperty = 40u;

    private readonly IPluginHost _host;
    private readonly NavigationSettings _settings;
    private int _index;
    private bool _reverse;
    private bool _onceComplete;
    private RouteWaypoint? _activeAction;
    private double _actionElapsed;
    private double _retryElapsed;
    private long _useCompletionBaseline;
    private ulong _chatBaseline;
    private bool _actionSent;
    private bool _sawPortalSpace;
    private bool _jumpReleased;
    private bool _jumpAligned;
    private bool _jumpSawAirborne;
    private double _jumpChargeElapsed;
    private double _jumpReleaseElapsed;
    private double _checkpointElapsed;
    private readonly List<PluginNavigationPosition> _followPath = [];
    private uint _activeDoorObjectId;
    private ActionLockTable? _actionLocks;

    /// <summary>
    /// The macro log sink. The route is otherwise silent between the rule's
    /// own lines, and the one thing worth a line of its own is where a
    /// start put the round: a reader watching a restart cannot otherwise
    /// tell an anchor from an arrival.
    /// </summary>
    internal Action<MacroLogChannel, string>? Log { get; set; }
    private uint _activeLockpickObjectId;
    private double _doorElapsed;
    private double _doorRetryElapsed;
    private PluginNavigationPosition _portalOrigin;
    private bool _hasPortalOrigin;
    private bool _recallNeedsPositionCapture = true;
    private PluginNavigationPosition _recallLastPosition;

    /// <summary>
    /// Whether a recall cast has gone out and nothing has been made of its
    /// outcome yet, and the completion revision that was standing when it was
    /// sent. Together they say "a newer completion than the one before my
    /// cast has landed", which is how the waypoint learns that the cast is
    /// over without portal space having come — a fizzle or a server refusal.
    /// </summary>
    private bool _recallCastPending;
    /// <summary>The last recall cast completed without an error: the
    /// teleport is on its way and the spell must not be sent again while
    /// the route waits for portal space.</summary>
    private bool _recallCastLanded;
    private long _recallCastRevision;

    /// <summary>
    /// The route's own close-in mover. One per rule, as the reference builds
    /// them: the corpse walk has its own, and the two never share armed state.
    /// </summary>
    private readonly NavigationMover _mover;

    /// <summary>
    /// The leg the client is walking for the route and that walk's sequence;
    /// the waypoint index that already had its one hand-off this visit;
    /// whether the route said it stalled again there; and whether a leg the
    /// client could not walk has paused the route.
    /// </summary>
    private RouteWaypoint? _clientWalkGoal;
    private long _clientWalkSequence;
    private int _clientHandOffIndex = -1;
    private bool _clientStallPosted;
    private bool _clientLegPaused;

    /// <summary>
    /// The jump's own re-face stamp. It cannot share the mover's, which is
    /// cleared every time the mover stops - and the mover is stopped for the
    /// whole of a jump waypoint.
    /// </summary>
    private double _jumpFaceHeadingStamp = NoFaceHeadingStamp;
    private string _status = "Navigation disabled.";

    /// <summary>
    /// The shared action-lock table. Opening a door holds the item slot for
    /// the use, and the navigation and door slots while the door swings, and
    /// an unidentified door in reach holds navigation for the identify.
    /// </summary>
    internal void BindActionLocks(ActionLockTable locks) =>
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));

    public NavigationController(IPluginHost host, NavigationSettings settings)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _mover = new NavigationMover(host)
        {
            Status = value => _status = value,
            WarnLowStopDistance = WarnLowWaypointDistance,
        };
    }

    /// <summary>
    /// The route's warning about a waypoint tight enough to be walked at in
    /// peace mode. The mover says when the case arises; the once-per-run
    /// bookkeeping is the route's, because "the run" is the route's idea.
    /// </summary>
    internal Action LowStopDistanceWarning => WarnLowWaypointDistance;

    private void WarnLowWaypointDistance()
    {
        if (_lowWaypointWarningPosted)
            return;
        _lowWaypointWarningPosted = true;
        _host.Automation.Chat.PostSystemMessage(
            "[MossTank] " + LowWaypointDistanceWarning);
    }

    public string Status => _status;

    /// <summary>
    /// Whether the route has nothing left to walk: no points at all, or a
    /// once route that has run to its end. A once route keeps its points (so
    /// its file is never rewritten short), so the point count alone cannot
    /// answer this; a meta's "navigation route empty" asks this instead.
    /// </summary>
    public bool HasNothingLeftToWalk =>
        _settings.Waypoints.Count == 0 || _onceComplete;

    /// <summary>
    /// Why the door turn did what it did. It is deliberately NOT the route's
    /// status: that sentence is about a waypoint and carries a live distance,
    /// so a door reason borrowed from it differs on every pass and the
    /// scheduler can never suppress the repeat. Every sentence written here
    /// is stable while the situation is.
    /// </summary>
    internal string DoorStatus => _doorStatus;

    private string _doorStatus = "no door in reach";

    /// <summary>
    /// The goal this rule is steering at, in the oracle's own shape: the
    /// range to it and where it is. It rides the rule's "Running" line so a
    /// route that looks stuck can be told apart from a route steering at the
    /// wrong place.
    /// </summary>
    public string RunningDetail
    {
        get
        {
            if (_settings.Mode == RouteMode.Target
                || _onceComplete
                || _index < 0
                || _index >= _settings.Waypoints.Count)
            {
                return string.Empty;
            }
            PluginNavigationPosition goal = _settings.Waypoints[_index].Position;
            double distance = _host.Automation.Navigation.Snapshot.Position
                .HorizontalDistanceMeters(goal);
            return string.Create(
                CultureInfo.InvariantCulture,
                $"[targ range {distance:0.###}, targ loc {goal.EastWest:0.######}, "
                    + $"{goal.NorthSouth:0.######}, {goal.Elevation:0.######} ]");
        }
    }

    public int CurrentWaypointIndex => _index;

    /// <summary>
    /// Whether a once route has already run this waypoint. A once route
    /// consumes by moving its cursor, never by removing the point, so the
    /// waypoints behind the cursor are the spent ones; every other mode
    /// comes back around and spends nothing.
    /// </summary>
    public bool IsWaypointConsumed(int index) =>
        _settings.Mode == RouteMode.Once && index >= 0 && index < _index;
    public bool Reversing => _reverse;

    public bool HasActiveAction => _activeAction is not null;


    internal const string LowWaypointDistanceWarning =
        "Warning: Idle peace selected with low waypoint minimum distance. "
        + "Will switch to magic mode.";

    private bool _lowWaypointWarningPosted;

    /// <summary>Warnings this run has already said once.</summary>
    private readonly HashSet<string> _postedWarnings = new(StringComparer.Ordinal);

    /// <summary>
    /// The one combat-mode gate the macro owns. The mover uses it for the
    /// creep push; a recall waypoint uses the same instance to stand the
    /// character in magic mode before it casts. There is no second gate: this
    /// field is the same object the mover was handed.
    /// </summary>
    private CombatModeGate? _combatModeGate;

    internal void BindCombatModeGate(CombatModeGate gate, CombatSettings settings)
    {
        _combatModeGate = gate ?? throw new ArgumentNullException(nameof(gate));
        _mover.BindCombatModeGate(gate, settings);
    }

    public void ToggleReverse()
    {
        _reverse = !_reverse;
        _status = $"Nav backwards is {_reverse}.";
    }

    internal void ResetOncePerRunWarnings()
    {
        _lowWaypointWarningPosted = false;
        _postedWarnings.Clear();
    }

    /// <summary>
    /// Back to the top of the route. This is for a route that has changed
    /// under the controller — loaded, edited, cleared — and for the end of a
    /// session. Stopping the macro is NOT one of those; it uses
    /// <see cref="StopForMacroStop"/>, which keeps the round's position, and
    /// the next start re-anchors it.
    ///
    /// The reference re-anchors on a route change too rather than going to
    /// the head; that difference is a deliberate deviation and is recorded
    /// with the project's other known deviations.
    /// </summary>
    public void Reset()
    {
        StopForMacroStop();
        _index = 0;
        _reverse = false;
        _onceComplete = false;
    }

    /// <summary>
    /// Where the round begins when the macro starts. Not where it left off,
    /// and not the first point either: the character is wherever the last run
    /// ended or wherever it died, so the round is re-anchored to the nearest
    /// point it could actually walk to. A once-through route starts at its
    /// head, and a follow route has no round to anchor.
    ///
    /// Only three waypoint kinds are candidates — the plain point, the portal
    /// and the checkpoint. The rest (pause, chat, jump, recall, vendor, NPC,
    /// portal-by-name) are things to do rather than places to be, and their
    /// stored coordinate is not somewhere the character can be near.
    /// </summary>
    public void AnchorRoundToStart()
    {
        if (_settings.Mode == RouteMode.Target)
            return;

        if (_settings.Mode == RouteMode.Once)
        {
            // Starting the round is one of the moments that puts the route
            // back at its head, so the once cursor is rewound with it.
            _index = 0;
            _onceComplete = false;
        }
        else
        {
            PluginNavigationPosition here =
                _host.Automation.Navigation.Snapshot.Position;
            int nearest = 0;
            double best = double.MaxValue;
            for (int index = 0; index < _settings.Waypoints.Count; index++)
            {
                RouteWaypoint waypoint = _settings.Waypoints[index];
                if (!IsAnchorCandidate(waypoint.Type))
                    continue;
                double distance = SpatialDistanceMeters(here, waypoint.Position);
                if (distance < best)
                {
                    best = distance;
                    nearest = index;
                }
            }
            // A route of nothing but actions anchors at its head, which is
            // what the zero the search starts from already says.
            _index = nearest;
        }

        if (_settings.Waypoints.Count > 0)
        {
            ClearAction();
            Log?.Invoke(
                MacroLogChannel.RuleInfo,
                $"Route starts at Waypoint {_index + 1}/{_settings.Waypoints.Count}"
                    + (_settings.Mode == RouteMode.Once
                        ? " (a once-through route starts at its head)"
                        : " (the nearest point to the character)"));
        }
    }

    private static bool IsAnchorCandidate(RouteWaypointType type) =>
        type is RouteWaypointType.Point
            or RouteWaypointType.Portal
            or RouteWaypointType.Checkpoint;

    /// <summary>
    /// Straight-line distance including height, which is what picking the
    /// nearest point of a route asks for — a point directly below you on the
    /// floor of a dungeon is not the one you are standing at.
    /// </summary>
    private static double SpatialDistanceMeters(
        in PluginNavigationPosition from,
        in PluginNavigationPosition to)
    {
        double eastWest = from.EastWest - to.EastWest;
        double northSouth = from.NorthSouth - to.NorthSouth;
        double elevation = from.Elevation - to.Elevation;
        return Math.Sqrt(
            (eastWest * eastWest)
            + (northSouth * northSouth)
            + (elevation * elevation)) * 240d;
    }

    /// <summary>
    /// What stopping the macro does to the route: everything in flight is put
    /// down — the movement, the door being opened, the waypoint action being
    /// worked, the checkpoint clock — and the round's own position is kept.
    /// Which point of the loop the character had reached, and which way round
    /// it was going, are the player's, not the macro run's, and the
    /// reference's stop leaves them alone. Starting again re-anchors the round
    /// rather than resuming it blindly — see <see cref="AnchorRoundToStart"/>.
    /// Loading or editing a route puts the round back to its first point; see
    /// <see cref="Reset"/>.
    /// </summary>
    public void StopForMacroStop()
    {
        // Layered, not copied: losing one pass is the innermost of the three
        // teardowns, a macro stop adds the things a returning turn would have
        // wanted kept, and a reset adds the round's own position on top of
        // that. Three near-identical bodies are how the next in-flight field
        // gets forgotten in one of them.
        StopForLostTurn();
        // The re-face throttle is a stamp on the mover's clock; a stopped
        // macro starts the next run without it.
        _mover.ClearFaceHeadingStamp();
        _clientLegPaused = false;
        _clientHandOffIndex = -1;
        _clientStallPosted = false;
        ResetOncePerRunWarnings();
        _checkpointElapsed = 0d;
        _followPath.Clear();
        ClearDoor();
        ClearAction();
        _status = _settings.Enabled
            ? "Route ready."
            : "Navigation disabled.";
    }

    /// <summary>
    /// Deliberately narrower than the three above and not layered on them:
    /// this is the "let go of whatever you are holding and try again" command,
    /// which keeps the follow trail and the once-per-run warnings.
    /// </summary>
    public void ClearActionLocks()
    {
        _mover.StopForLostTurn();
        _mover.ClearFaceHeadingStamp();
        _clientLegPaused = false;
        _clientHandOffIndex = -1;
        _clientStallPosted = false;
        _checkpointElapsed = 0d;
        ClearDoor();
        ClearAction();
        _status = _settings.Enabled
            ? "Route action locks cleared."
            : "Navigation disabled.";
    }

    /// <summary>
    /// The route rule's own turn. It answers whether the rule claims the pass
    /// and, on the pass it claims, arms the mover; it does not carry the
    /// mover's clock, because the pass is not the mover's clock.
    /// </summary>
    /// <summary>
    /// The reference's gate on the route rule's idle-peace fallback: normal
    /// movement is allowed while the goal is further than the creep distance
    /// and the waypoint is not a recall. Inside the creep band, or at a
    /// recall, the walk itself runs and the mover pushes into magic mode.
    /// </summary>
    internal bool AllowsNormalMovement()
    {
        PluginNavigationSnapshot snapshot = _host.Automation.Navigation.Snapshot;
        if (!snapshot.IsAvailable
            || _settings.Mode == RouteMode.Target
            || _onceComplete
            || _settings.Waypoints.Count == 0)
        {
            return true;
        }
        RouteWaypoint waypoint =
            _settings.Waypoints[Math.Clamp(_index, 0, _settings.Waypoints.Count - 1)];
        if (waypoint.Type == RouteWaypointType.Recall)
            return false;
        return snapshot.Position.HorizontalDistanceMeters(waypoint.Position)
            >= NavigationMover.CreepDistanceMeters;
    }

    internal bool ClaimFromRulePass(bool canAct)
    {
        bool claimed = Tick(_mover.TakePendingSeconds(), canAct);
        _mover.Arm(claimed);
        return claimed;
    }

    /// <summary>
    /// One frame of the armed mover. The host calls this every frame; the
    /// mover steers no faster than its own interval, and the rule pass is
    /// only what arms and disarms it.
    /// </summary>
    /// <param name="navigationSlotsAreClear">
    /// Whether the cooldown slots that hold navigation off are down, read on
    /// THIS frame. The rule's own gate reads the same slots, but a rule pass
    /// that is suspended â which is what another owner of the character does
    /// to it â asks no rule anything and so takes nobody's turn away: the
    /// route's rule stays the running one and its mover stays armed. Without
    /// this the frame driver walked the route on through a hold that was
    /// armed precisely to stop it. A closed slot is the losing-the-turn edge,
    /// movement intent dropped and all, and the pass that finds the slots
    /// clear again arms the mover anew.
    /// </param>
    internal void StepArmedMover(double elapsedSeconds, bool navigationSlotsAreClear)
    {
        if (!navigationSlotsAreClear)
        {
            _mover.AdvanceClock(elapsedSeconds);
            if (_mover.IsArmed)
                StopForLostTurn();
            return;
        }
        if (_mover.TryTakeMoverFrame(elapsedSeconds, out double due))
            _ = Tick(due, canAct: true);
    }

    /// <summary>
    /// Moves the navigation clock on by one frame. This is the ONE place it
    /// moves: the clock stands for wall time, and a turn or a pass is not a
    /// unit of it. It used to be advanced from both the route's turn and the
    /// door's, so on the ordinary pass where both were consulted every
    /// interval measured against it — the re-face throttles above — ran at
    /// roughly double speed, and at an uneven rate besides.
    /// </summary>
    internal void AdvanceClock(double elapsedSeconds) =>
        _mover.AdvanceClock(elapsedSeconds);

    public bool Tick(double elapsedSeconds, bool canAct)
    {
        elapsedSeconds = double.IsFinite(elapsedSeconds)
            ? Math.Max(0d, elapsedSeconds)
            : 0d;
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot snapshot = navigation.Snapshot;
        if (!_settings.Enabled || !snapshot.IsAvailable)
        {
            StopMovement();
            _status = _settings.Enabled
                ? "Waiting for the world."
                : "Navigation disabled.";
            return false;
        }
        if (snapshot.IsPortalSpace)
        {
            StopMovement();
            if (_activeAction?.Type is RouteWaypointType.Portal
                or RouteWaypointType.PortalByName
                or RouteWaypointType.Recall)
            {
                _sawPortalSpace = true;
            }
            _status = "Waiting for portal space.";
            return _activeAction is not null;
        }
        if (!canAct)
        {
            StopMovement();
            _status = "Navigation paused.";
            return false;
        }

        if (_settings.Mode == RouteMode.Target)
            return TickFollow(navigation, snapshot);
        if (_onceComplete || _settings.Waypoints.Count == 0)
        {
            StopMovement();
            _status = _onceComplete ? "Once route complete." : "Route is empty.";
            return false;
        }

        if (_clientLegPaused)
        {
            StopMovement();
            return false;
        }

        _index = Math.Clamp(_index, 0, _settings.Waypoints.Count - 1);
        RouteWaypoint waypoint = _settings.Waypoints[_index];
        if (waypoint.Type == RouteWaypointType.Point)
        {
            double distance = snapshot.Position.HorizontalDistanceMeters(
                waypoint.Position);
            if (distance > BoundedMaximumDistance())
            {
                StopMovement();
                _status = $"Waypoint is outside NavFarStopRange ({distance:0.0}m).";
                return false;
            }
            if (distance <= BoundedMinimumDistance())
            {
                // Arrival is not a stop: the next point is taken on this same
                // frame and the mover decides, from the angle to it, whether
                // the character turns while running or halts to turn. Only a
                // point that ends the route, or one that is not a point, stops.
                AdvanceWaypoint();
                if (_onceComplete || _settings.Waypoints.Count == 0)
                {
                    // The arrival that ends the route still claims this pass;
                    // the next one declines.
                    StopMovement();
                    _status = "Once route complete.";
                    return true;
                }
                _index = Math.Clamp(_index, 0, _settings.Waypoints.Count - 1);
                waypoint = _settings.Waypoints[_index];
                if (waypoint.Type != RouteWaypointType.Point)
                {
                    StopMovement();
                    return true;
                }
                distance = snapshot.Position.HorizontalDistanceMeters(waypoint.Position);
            }
            _status = string.Create(
                CultureInfo.InvariantCulture,
                $"Waypoint {_index + 1}/{_settings.Waypoints.Count}: {distance:0.0}m");
            return SteerLeg(navigation, snapshot, waypoint, distance);
        }
        if (waypoint.Type == RouteWaypointType.Checkpoint)
            return TickCheckpoint(navigation, snapshot, waypoint, elapsedSeconds);

        StopMovement();
        return TickAction(waypoint, elapsedSeconds, snapshot);
    }

    private bool TickFollow(
        INavigationAutomation navigation,
        in PluginNavigationSnapshot snapshot)
    {
        if (_settings.FollowTargetObjectId == 0u
            || !navigation.TryGetObject(
                _settings.FollowTargetObjectId,
                out PluginNavigationObject target))
        {
            StopMovement();
            _status = "Follow target unavailable.";
            return false;
        }
        double distance = snapshot.Position.HorizontalDistanceMeters(
            target.Position);
        if (distance > BoundedMaximumDistance())
        {
            StopMovement();
            _status = $"Follow target is outside NavFarStopRange ({distance:0.0}m).";
            return false;
        }
        if (distance <= BoundedMinimumDistance())
        {
            StopMovement();
            _status = $"Following {target.Name}: holding {distance:0.0}m.";
            return false;
        }
        PluginNavigationPosition destination = CaptureFollowDestination(
            snapshot.Position,
            target.Position);
        double destinationDistance = snapshot.Position.HorizontalDistanceMeters(
            destination);
        _status = $"Following {target.Name}: {distance:0.0}m.";
        return Steer(navigation, snapshot.Position, destination, destinationDistance);
    }

    private PluginNavigationPosition CaptureFollowDestination(
        in PluginNavigationPosition current,
        in PluginNavigationPosition target)
    {
        if (!_settings.FollowAroundCorners)
        {
            _followPath.Clear();
            return target;
        }

        if (_followPath.Count == 0
            || _followPath[^1].HorizontalDistanceMeters(target)
                >= FollowBreadcrumbSpacingMeters)
        {
            _followPath.Add(target);
        }

        for (int index = _followPath.Count - 1; index >= 1; index--)
        {
            if (DistanceToSegmentMeters(
                    current,
                    _followPath[index - 1],
                    _followPath[index]) < FollowPathArrivalMeters
                && current.HorizontalDistanceMeters(_followPath[index])
                    < FollowPathCaptureRangeMeters)
            {
                _followPath.RemoveRange(0, index);
                break;
            }
        }
        return _followPath.Count == 0 ? target : _followPath[0];
    }

    /// <summary>
    /// Opening a door is its own turn, taken before anything that might want
    /// the same tick. It lives on the navigation controller because it shares
    /// the mover and the door settings, but it is driven by
    /// <see cref="OpenDoorRule"/> from the door's own place in the rule order,
    /// not from inside a navigate turn.
    /// </summary>
    internal bool TickDoorRule(double elapsedSeconds, bool canAct)
    {
        elapsedSeconds = double.IsFinite(elapsedSeconds)
            ? Math.Max(0d, elapsedSeconds)
            : 0d;
        if (!canAct)
        {
            // Losing the pass to a rule ahead of this one is a decline, not a
            // reset. The door being identified and the lockpick already chosen
            // for it have to still be there on the pass this rule wins back —
            // an open sequence that resets every time anything else takes a
            // turn can never finish. The pass a rule does not win is a pass a
            // rule is not asked about.
            _doorStatus = "another rule has the turn";
            return false;
        }
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot snapshot = navigation.Snapshot;
        if (!_settings.Enabled
            || !_settings.OpenDoors
            || !snapshot.IsAvailable
            || snapshot.IsPortalSpace)
        {
            _doorStatus = !_settings.Enabled || !_settings.OpenDoors
                ? "door opening is off"
                : "waiting for the world";
            ClearDoor();
            return false;
        }

        return TickDoor(navigation, snapshot, elapsedSeconds);
    }

    private bool TickDoor(
        INavigationAutomation navigation,
        in PluginNavigationSnapshot snapshot,
        double elapsedSeconds)
    {
        if (!_settings.OpenDoors)
        {
            _doorStatus = "door opening is off";
            ClearDoor();
            return false;
        }

        IReadOnlyList<PluginNavigationObject> objects = navigation.CaptureObjects();
        if (_activeDoorObjectId != 0u)
        {
            foreach (PluginNavigationObject tracked in objects)
            {
                if (tracked.ObjectId != _activeDoorObjectId || !tracked.IsOpen)
                    continue;
                // The door has swung: the use is over and the doorway is
                // free, so both slots go down early rather than running out
                // their windows.
                _actionLocks?.Release(ActionLockKind.ItemUse);
                _actionLocks?.Release(ActionLockKind.DoorOpening);
                break;
            }
        }
        PluginNavigationObject door = default;
        bool found = false;
        double nearest = _settings.DoorIdentifyRangeMeters;
        foreach (PluginNavigationObject candidate in objects)
        {
            if (!candidate.IsDoor || candidate.IsOpen)
                continue;
            double distance = snapshot.Position.HorizontalDistanceMeters(
                candidate.Position);
            if (_activeDoorObjectId != 0u
                && candidate.ObjectId == _activeDoorObjectId)
            {
                door = candidate;
                nearest = distance;
                found = true;
                break;
            }
            if (_activeDoorObjectId == 0u && distance <= nearest)
            {
                door = candidate;
                nearest = distance;
                found = true;
            }
        }

        if (!found)
        {
            _doorStatus = "no door in reach";
            ClearDoor();
            return false;
        }
        if (!door.HasLockState)
        {
            if (nearest <= _settings.DoorIdentifyRangeMeters
                && _host.Automation.Loot.Appraisal.AwaitingObjectId == 0u)
            {
                _ = _host.Automation.Loot.Identify(door.ObjectId);
            }
            if (nearest <= _settings.DoorOpenRangeMeters)
            {
                StopMovement();
                _doorStatus = $"identifying door: {door.Name}";
                if (_actionLocks is { } identifying)
                {
                    // While the door is being identified nobody walks: the
                    // navigation slot is held for half a second and the
                    // pass is declined, so the route rules stand still
                    // without this rule having to own the turn.
                    identifying.Arm(ActionLockKind.Navigation, 0.5d);
                    return false;
                }
                return true;
            }
            _doorStatus = $"waiting for the lock state of {door.Name}";
            return false;
        }
        if (nearest > _settings.DoorOpenRangeMeters)
        {
            _doorStatus = $"{door.Name} is not in open range";
            ClearDoor();
            return false;
        }

        if (_activeDoorObjectId == 0u)
        {
            _activeDoorObjectId = door.ObjectId;
            // A door just taken up is due an attempt straight away; the
            // interval only measures the gap between attempts.
            _doorRetryElapsed = UseRetrySeconds;
            _activeLockpickObjectId = door.IsLocked
                ? SelectLockpick(door.LockDifficulty)
                : 0u;
            if (door.IsLocked && _activeLockpickObjectId == 0u)
            {
                _doorStatus = $"locked door skipped: {door.Name}";
                ClearDoor();
                return false;
            }
        }

        StopMovement();
        _doorElapsed += elapsedSeconds;
        _doorRetryElapsed += elapsedSeconds;
        if (_doorElapsed >= DoorActionTimeoutSeconds)
        {
            _doorStatus = $"door timed out: {door.Name}";
            ClearDoor();
            return false;
        }
        // The old form of this also accepted "this is the first accumulation
        // since a clear", which could never be told apart from the tick right
        // after an attempt reset the counter to zero — so the interval never
        // held and the door was used again on every pass.
        if (_doorRetryElapsed >= UseRetrySeconds)
        {
            // Every door action holds the item slot for its use. A plain use
            // also holds navigation and the door slot for as long as a door
            // takes to swing; a lockpick holds navigation only briefly, since
            // the door is not moving yet.
            if (_actionLocks is { } locks)
            {
                locks.Arm(ActionLockKind.ItemUse, 0.5d);
                if (_activeLockpickObjectId == 0u)
                {
                    locks.Arm(ActionLockKind.Navigation, 5d);
                    locks.Arm(ActionLockKind.DoorOpening, 5d);
                }
                else
                {
                    locks.Arm(ActionLockKind.Navigation, 1d);
                }
            }
            PluginItemCommandResult result = _activeLockpickObjectId == 0u
                ? _host.Automation.Items.Use(door.ObjectId)
                : _host.Automation.Items.Apply(
                    _activeLockpickObjectId,
                    door.ObjectId);
            _doorRetryElapsed = 0d;
            _doorStatus = result.Accepted
                ? _activeLockpickObjectId == 0u
                    ? $"opening door: {door.Name}"
                    : $"picking lock: {door.Name}"
                : $"waiting for door: {door.Name}";
        }
        return true;
    }

    private uint SelectLockpick(int difficulty)
    {
        if (!_host.Automation.Character.TryGetSkill(
                LockpickSkillId,
                out PluginSkillInfo skill)
            || skill.Current < Math.Max(
                0,
                difficulty + _settings.DoorLockpickExcessThreshold))
        {
            return 0u;
        }

        uint selected = 0u;
        double bestModifier = double.MinValue;
        foreach (PluginInventoryItem item in _host.Automation.Items.CaptureOwnedItems())
        {
            if ((item.PublicFlags & LockpickPublicFlag) == 0u)
                continue;
            double modifier = 0d;
            if (_host.Automation.Items.TryCaptureProperties(
                    item.ObjectId,
                    out PluginItemProperties properties)
                && properties.Floats.TryGetValue(
                    LockpickModifierProperty,
                    out double current))
            {
                modifier = current;
            }
            if (modifier <= bestModifier)
                continue;
            bestModifier = modifier;
            selected = item.ObjectId;
        }
        return selected;
    }

    private void ClearDoor()
    {
        _activeDoorObjectId = 0u;
        _activeLockpickObjectId = 0u;
        _doorElapsed = 0d;
        _doorRetryElapsed = UseRetrySeconds;
    }

    private bool TickCheckpoint(
        INavigationAutomation navigation,
        in PluginNavigationSnapshot snapshot,
        RouteWaypoint waypoint,
        double elapsedSeconds)
    {
        double liveDistance = snapshot.Position.HorizontalDistanceMeters(
            waypoint.Position);
        if (liveDistance > BoundedMaximumDistance())
        {
            StopMovement();
            _checkpointElapsed = 0d;
            _status = $"Checkpoint is outside NavFarStopRange ({liveDistance:0.0}m).";
            return false;
        }
        if (liveDistance > BoundedMinimumDistance())
        {
            _checkpointElapsed = 0d;
            _status = string.Create(CultureInfo.InvariantCulture, $"Checkpoint {_index + 1}/{_settings.Waypoints.Count}: {liveDistance:0.0}m");
            return SteerLeg(navigation, snapshot, waypoint, liveDistance);
        }

        StopMovement();
        PluginNavigationPosition confirmed = snapshot.ConfirmedPositionRevision == 0UL
            ? snapshot.Position
            : snapshot.ConfirmedPosition;
        double confirmedDistance = confirmed.HorizontalDistanceMeters(
            waypoint.Position);
        if (confirmedDistance <= BoundedMinimumDistance())
        {
            _checkpointElapsed = 0d;
            AdvanceWaypoint();
            return true;
        }

        _checkpointElapsed += elapsedSeconds;
        _status = $"Checkpoint: waiting for server ({confirmedDistance:0.0}m).";
        if (_checkpointElapsed >= CheckpointRetrySeconds)
        {
            _checkpointElapsed = 0d;
            _mover.NoteMovementIntent(
                navigation.SetMovementIntent(
                    new PluginMovementIntent(Forward: true, Run: false))
                    == PluginNavigationCommandStatus.Accepted);
            _status = "Checkpoint: nudging for server confirmation.";
        }
        return true;
    }

    /// <summary>
    /// The mover's typing branch: it cannot hold a turn key while the player is
    /// typing, so it stops and re-faces the goal at most once per
    /// <see cref="FaceHeadingReissueSeconds"/> instead.
    /// </summary>
    internal static PluginNavigationCommandStatus SteerTowards(
        INavigationAutomation navigation,
        float signedHeadingDeltaDegrees,
        float desiredHeadingDegrees,
        double now,
        ref double faceHeadingStamp,
        bool run)
    {
        if (Math.Abs(signedHeadingDeltaDegrees) > HeadingToleranceDegrees)
        {
            PluginNavigationCommandStatus stopped =
                navigation.ClearMovementIntent();
            if (stopped != PluginNavigationCommandStatus.Accepted)
                return stopped;

            // Outside the band: re-issue the facing, but only every so often.
            if (now - faceHeadingStamp >= FaceHeadingReissueSeconds)
            {
                faceHeadingStamp = now;
                PluginNavigationCommandStatus faced =
                    navigation.FaceHeading(desiredHeadingDegrees);
                if (faced != PluginNavigationCommandStatus.Accepted)
                    return faced;
            }

            return PluginNavigationCommandStatus.Accepted;
        }

        // Inside the band: move, and reset the stamp so the next departure
        // re-issues immediately.
        faceHeadingStamp = NoFaceHeadingStamp;
        return navigation.SetMovementIntent(
            new PluginMovementIntent(Forward: true, Run: run));
    }

    /// <summary>
    /// Turning down from the current heading by the unsigned offset and landing
    /// on the bearing means the bearing is counter-clockwise, so the turn is
    /// left. Heading grows clockwise, so this agrees with the sign of the
    /// wrapped difference everywhere except at exactly half a turn, where the
    /// choice is arbitrary and this one matches the reference behaviour.
    /// </summary>
    internal static bool PrefersLeftTurn(float current, float desired)
    {
        float offset = UnsignedHeadingDelta(current, desired);
        return UnsignedHeadingDelta(NormalizeHeading(current - offset), desired)
            < 1f;
    }

    /// <summary>The smaller of the two arcs between two headings, never negative.</summary>
    internal static float UnsignedHeadingDelta(float left, float right)
    {
        float high = left >= right ? left : right;
        float low = left >= right ? right : left;
        float inner = high - low;
        float outer = low - high + 360f;
        return inner < outer ? inner : outer;
    }

    internal static float NormalizeHeading(float value)
    {
        float wrapped = value % 360f;
        return wrapped < 0f ? wrapped + 360f : wrapped;
    }

    /// <summary>Steers the route's mover at a goal.</summary>
    private bool Steer(
        INavigationAutomation navigation,
        in PluginNavigationPosition current,
        in PluginNavigationPosition target,
        double distanceMeters) =>
        _mover.Steer(navigation, current, target, distanceMeters);

    /// <summary>
    /// Steers one leg of the route: the reference way, straight at the point
    /// with held keys, unless the profile hands every leg to the client, or
    /// the straight walk is stuck and the profile allows one hand-off per
    /// visit to a waypoint. While the client walks, the rule keeps its turn
    /// and only watches the report; every way that walk ends returns the
    /// keys to the mover, and arriving advances the route.
    /// </summary>
    private bool SteerLeg(
        INavigationAutomation navigation,
        in PluginNavigationSnapshot snapshot,
        RouteWaypoint waypoint,
        double distance)
    {
        if (_clientWalkGoal is not null)
            return WatchClientWalk(navigation, distance);
        if (_settings.ClientPathing == ClientPathing.Always)
            return AskClientWalk(navigation, waypoint, distance);

        bool claimed = Steer(navigation, snapshot.Position, waypoint.Position, distance);
        if (!_mover.IsStuck)
            return claimed;
        _mover.ResetStuckClock();
        if (_settings.ClientPathing == ClientPathing.WhenStuck
            && _clientHandOffIndex != _index)
        {
            _clientHandOffIndex = _index;
            _mover.StopMovement();
            return AskClientWalk(navigation, waypoint, distance);
        }
        if (!_clientStallPosted)
        {
            _clientStallPosted = true;
            _host.Automation.Chat.PostSystemMessage(string.Create(
                CultureInfo.InvariantCulture,
                $"[MossTank] The route has not covered ground for {NavigationMover.StuckSeconds:0} seconds on the way to waypoint {_index + 1}."));
        }
        return claimed;
    }

    private bool AskClientWalk(
        INavigationAutomation navigation,
        RouteWaypoint waypoint,
        double distance)
    {
        PluginNavigationCommandStatus asked = navigation.GoTo(
            waypoint.Position,
            (float)BoundedMinimumDistance());
        if (asked != PluginNavigationCommandStatus.Accepted)
        {
            string why = asked switch
            {
                PluginNavigationCommandStatus.Unavailable => "this client cannot path",
                PluginNavigationCommandStatus.Held =>
                    "another plugin or the player is driving the character",
                _ => "the client refused the walk",
            };
            if (_settings.ClientPathing == ClientPathing.Always)
            {
                PauseRoute($"Waypoint {_index + 1} could not be walked ({why})");
                return false;
            }
            _status = string.Create(
                CultureInfo.InvariantCulture,
                $"Waypoint {_index + 1}/{_settings.Waypoints.Count}: {distance:0.0}m; {why}, walking on");
            return true;
        }
        _clientWalkGoal = waypoint;
        _clientWalkSequence = navigation.GoToReport.Sequence;
        _status = ClientWalkStatus(distance);
        return true;
    }

    private bool WatchClientWalk(INavigationAutomation navigation, double distance)
    {
        PluginGoToReport report = navigation.GoToReport;
        if (report.Sequence != _clientWalkSequence)
        {
            // A newer walk replaced ours: whoever asked for it has the
            // character, and the keys come back on the next steered frame.
            _clientWalkGoal = null;
            return true;
        }
        switch (report.State)
        {
            case PluginGoToState.Planning:
            case PluginGoToState.Walking:
            case PluginGoToState.Waiting:
                _status = ClientWalkStatus(distance);
                return true;
            case PluginGoToState.Arrived:
            case PluginGoToState.ArrivedWithoutSight:
                _clientWalkGoal = null;
                AdvanceWaypoint();
                return true;
            case PluginGoToState.Stopped:
                _clientWalkGoal = null;
                return true;
            default:
                _clientWalkGoal = null;
                string reason = string.IsNullOrWhiteSpace(report.Reason)
                    ? report.State.ToString()
                    : report.Reason;
                if (_settings.ClientPathing == ClientPathing.Always)
                {
                    PauseRoute($"Waypoint {_index + 1} could not be walked ({reason})");
                    return false;
                }
                _status = string.Create(
                    CultureInfo.InvariantCulture,
                    $"Waypoint {_index + 1}/{_settings.Waypoints.Count}: {distance:0.0}m; the client's walk ended ({reason}), walking on");
                return true;
        }
    }

    private string ClientWalkStatus(double distance) => string.Create(
        CultureInfo.InvariantCulture,
        $"Waypoint {_index + 1}/{_settings.Waypoints.Count}: {distance:0.0}m, walked by the client");

    /// <summary>
    /// With every leg on the client, a leg it cannot walk stops the route
    /// where it stands and says so once; the route goes on when it is reset
    /// or the setting changes.
    /// </summary>
    private void PauseRoute(string why)
    {
        _clientLegPaused = true;
        _status = why + "; the route is paused. Reset the route or change Client pathing.";
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + _status);
    }

    /// <summary>Ends the walk the route asked the client for, if it is still under way.</summary>
    private void StopClientWalk()
    {
        if (_clientWalkGoal is null)
            return;
        _clientWalkGoal = null;
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginGoToReport report = navigation.GoToReport;
        if (report.Sequence == _clientWalkSequence
            && report.State is PluginGoToState.Planning
                or PluginGoToState.Walking
                or PluginGoToState.Waiting)
        {
            _ = navigation.StopGoTo();
        }
    }

    /// <summary>The Client pathing setting changed: whatever walk it allowed is let go, and a paused route may try again.</summary>
    internal void ClientPathingChanged()
    {
        StopClientWalk();
        _clientLegPaused = false;
        _clientHandOffIndex = -1;
        _clientStallPosted = false;
    }

    private bool TickAction(
        RouteWaypoint waypoint,
        double elapsedSeconds,
        in PluginNavigationSnapshot navigation)
    {
        if (!ReferenceEquals(_activeAction, waypoint))
        {
            ClearAction();
            _activeAction = waypoint;
            _useCompletionBaseline = _host.Automation.Items.LastCompletion.Revision;
            _chatBaseline = _host.Automation.Chat.CaptureMessages(0)
                .Select(static message => message.Sequence)
                .DefaultIfEmpty()
                .Max();
        }
        _actionElapsed += elapsedSeconds;
        _retryElapsed += elapsedSeconds;

        switch (waypoint.Type)
        {
            case RouteWaypointType.Pause:
                _status = $"Pause: {Math.Max(0d, waypoint.DurationMilliseconds / 1000d - _actionElapsed):0.0}s";
                if (_actionElapsed * 1000d >= Math.Max(0, waypoint.DurationMilliseconds))
                    CompleteAction();
                return true;

            case RouteWaypointType.ChatCommand:
                _status = $"Chat command: {waypoint.Text}";
                if (_actionElapsed < ChatInitialDelaySeconds)
                    return true;
                if (!_actionSent)
                {
                    _actionSent = _host.Automation.Chat.Submit(waypoint.Text);
                    if (!_actionSent)
                    {
                        _status = "Chat command was refused.";
                        return true;
                    }
                }
                CompleteAction();
                return true;

            case RouteWaypointType.Recall:
                return TickRecall(waypoint, navigation);

            case RouteWaypointType.OpenVendor:
                return TickOpenVendor(waypoint);

            case RouteWaypointType.Portal:
            case RouteWaypointType.PortalByName:
            case RouteWaypointType.UseNpc:
                return TickUse(waypoint, navigation);

            case RouteWaypointType.Jump:
                return TickJump(waypoint, elapsedSeconds, navigation);

            default:
                CompleteAction();
                return true;
        }
    }

    /// <summary>
    /// A vendor waypoint opens the vendor and holds until its window is up.
    /// Using something out of reach only STARTS a walk to it, so a waypoint
    /// that completed on the tick it sent the use cancelled its own walk by
    /// moving the route on; the hold is what lets that walk finish. The use
    /// is re-sent every <see cref="UseRetrySeconds"/> until the window opens,
    /// and after <see cref="PortalTimeoutSeconds"/> the route says so once
    /// and carries on rather than parking an unattended bot forever. A window
    /// that is already open — by this waypoint or by anything else — finishes
    /// the waypoint at once; the pause while the shop is actually worked is
    /// the vendor run's navigation lock, not this waypoint's business.
    /// </summary>
    private bool TickOpenVendor(RouteWaypoint waypoint)
    {
        if (waypoint.ObjectId != 0u && VendorWindowIsOpenFor(waypoint.ObjectId))
        {
            _status = $"Vendor window is open: {waypoint.ObjectName}.";
            CompleteAction();
            return true;
        }

        if (waypoint.ObjectId == 0u
            || !_host.Automation.Navigation.TryGetObject(
                waypoint.ObjectId,
                out PluginNavigationObject vendor))
        {
            WarnOnce(
                $"OpenVendor waypoint action ignored, vendor {waypoint.ObjectId} "
                    + $"({waypoint.ObjectName}) not found.");
            _status = $"Vendor not found: {waypoint.ObjectName}.";
            return HoldVendorWaypoint();
        }

        if (_actionElapsed >= PortalTimeoutSeconds)
        {
            WarnOnce(
                $"Vendor '{waypoint.ObjectName}' did not open; continuing route.");
            _status = $"Vendor did not open: {waypoint.ObjectName}.";
            CompleteAction();
            return true;
        }

        if (!_actionSent || _retryElapsed >= UseRetrySeconds)
        {
            PluginItemCommandResult result =
                _host.Automation.Items.Use(waypoint.ObjectId);
            // Sent is sent whether or not the host took it: a refusal waits
            // the same two seconds as an accepted use that has not landed,
            // instead of hammering the surface every tick.
            _actionSent = true;
            _retryElapsed = 0d;
            _status = result.Accepted
                ? $"Opening vendor: {vendor.Name}."
                : $"Waiting to use vendor: {vendor.Name}.";
        }
        return true;
    }

    /// <summary>
    /// Whether the vendor window that is up belongs to this waypoint's
    /// object. The item surface reports the vendor open for the client at
    /// large; the vendor surface reports the one the trade side is working.
    /// Either answer counts.
    /// </summary>
    private bool VendorWindowIsOpenFor(uint objectId) =>
        _host.Automation.Items.ActiveVendorObjectId == objectId
        || (_host.Automation.Vendor.IsOpen
            && _host.Automation.Vendor.VendorObjectId == objectId);

    /// <summary>
    /// The vendor waypoint's hold, with our own ceiling on it: a hold that
    /// never ends would park an unattended route on one bad waypoint forever.
    /// </summary>
    private bool HoldVendorWaypoint()
    {
        if (_actionElapsed < PortalTimeoutSeconds)
            return true;
        CompleteAction();
        return true;
    }

    private void WarnOnce(string text)
    {
        if (!_postedWarnings.Add(text))
            return;
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + text);
    }

    private bool TickUse(
        RouteWaypoint waypoint,
        in PluginNavigationSnapshot navigation)
    {
        if (waypoint.Type is RouteWaypointType.PortalByName
            or RouteWaypointType.UseNpc)
        {
            bool currentStillExists = waypoint.ObjectId != 0u
                && _host.Automation.Navigation.TryGetObject(
                    waypoint.ObjectId,
                    out PluginNavigationObject current)
                && (string.IsNullOrWhiteSpace(waypoint.ObjectName)
                    || current.Name.Equals(
                        waypoint.ObjectName,
                        StringComparison.OrdinalIgnoreCase));
            if (!currentStillExists)
            {
                // Search near the object's own recorded position ("tgtxyz"
                // in metaf's ptl/tlk grammar), not where the bot was
                // standing when the waypoint was authored ("myxyz" —
                // waypoint.Position): the object can be meaningfully far
                // from the approach point (e.g. a portal at the far end of
                // a room).
                if (!_host.Automation.Navigation.TryFindObject(
                        waypoint.ObjectName,
                        waypoint.ReferencePosition,
                        ObjectReacquireRadiusMeters,
                        out PluginNavigationObject reacquired))
                {
                    _status = $"Finding {waypoint.ObjectName}.";
                    if (_actionElapsed >= PortalTimeoutSeconds)
                        CompleteAction();
                    return true;
                }
                waypoint.ObjectId = reacquired.ObjectId;
                _actionSent = false;
                _retryElapsed = UseRetrySeconds;
            }
        }
        if (waypoint.ObjectId == 0u)
        {
            _status = "Waypoint object is unavailable; continuing.";
            CompleteAction();
            return true;
        }

        if ((waypoint.Type is RouteWaypointType.Portal
                or RouteWaypointType.PortalByName)
            && _host.Automation.Navigation.TryGetObject(
                waypoint.ObjectId,
                out PluginNavigationObject portal))
        {
            double distance = navigation.Position.HorizontalDistanceMeters(
                portal.Position);
            if (distance > Math.Clamp(
                    _settings.PortalUseDistanceMeters,
                    0.5d,
                    50d))
            {
                _status = $"Approaching {waypoint.ObjectName} ({distance:0.0}m).";
                return Steer(
                    _host.Automation.Navigation,
                    navigation.Position,
                    portal.Position,
                    distance);
            }
        }

        if (waypoint.Type == RouteWaypointType.UseNpc
            && HasNpcResponse(waypoint.ObjectName))
        {
            CompleteAction();
            return true;
        }

        PluginItemUseCompletion completion =
            _host.Automation.Items.LastCompletion;
        if (_actionSent
            && completion.Revision > _useCompletionBaseline
            && completion.SourceObjectId == waypoint.ObjectId)
        {
            _useCompletionBaseline = completion.Revision;
            if (!completion.IsSuccess)
            {
                _actionSent = false;
                _retryElapsed = UseRetrySeconds;
            }
        }

        if (navigation.IsPortalSpace)
            _sawPortalSpace = true;
        if (_sawPortalSpace && !navigation.IsPortalSpace)
        {
            if (waypoint.Type == RouteWaypointType.PortalByName
                && _hasPortalOrigin
                && navigation.Position.HorizontalDistanceMeters(_portalOrigin)
                    <= PortalExitDistanceMeters)
            {
                _sawPortalSpace = false;
                _actionSent = false;
                _retryElapsed = UseRetrySeconds;
                _status = "Portal exit stayed near its origin; retrying.";
                return true;
            }
            CompleteAction();
            return true;
        }
        if (_actionElapsed >= PortalTimeoutSeconds)
        {
            _status = $"Use timed out: {waypoint.ObjectName}.";
            CompleteAction();
            return true;
        }
        if (!_actionSent || _retryElapsed >= UseRetrySeconds)
        {
            PluginItemCommandResult result =
                _host.Automation.Items.Use(waypoint.ObjectId);
            _actionSent |= result.Accepted;
            if (result.Accepted && !_hasPortalOrigin)
            {
                _portalOrigin = navigation.Position;
                _hasPortalOrigin = true;
            }
            _retryElapsed = 0d;
            _status = result.Accepted
                ? $"Using {waypoint.ObjectName}."
                : $"Waiting to use {waypoint.ObjectName}.";
        }
        return true;
    }

    /// <summary>
    /// The NPC answered. Only two lines count, and each only with its own
    /// log-text type: a tell that opens "&lt;name&gt; tells you, " and a plain
    /// line that opens "&lt;name&gt; gives you". Any other line, of any type, is
    /// somebody else's conversation.
    /// </summary>
    /// <remarks>
    /// The test is over the line the chat window shows, which is what the
    /// answer is written against. A tell reaches a plugin with the sender and
    /// the message apart, so the shown line is rebuilt here; a server line
    /// arrives whole and is used as it stands. The message's kind cannot
    /// stand in for the log-text type — it only says where the line came
    /// from, and the two value spaces share small numbers without sharing
    /// meanings.
    /// </remarks>
    private bool HasNpcResponse(string npcName)
    {
        IReadOnlyList<PluginChatMessage> messages =
            _host.Automation.Chat.CaptureMessages(_chatBaseline);
        foreach (PluginChatMessage message in messages)
        {
            _chatBaseline = Math.Max(_chatBaseline, message.Sequence);
            bool answered = (uint)message.LogTextType switch
            {
                NpcTellLogTextType => ComposeTellLine(message).StartsWith(
                    npcName + " tells you, ",
                    StringComparison.Ordinal),
                NpcGiveLogTextType => message.Text.StartsWith(
                    npcName + " gives you",
                    StringComparison.Ordinal),
                _ => false,
            };
            if (answered)
                return true;
        }
        return false;
    }

    /// <summary>
    /// The line the chat window shows for a tell, rebuilt from the sender and
    /// message the plugin surface hands over separately.
    /// </summary>
    private static string ComposeTellLine(in PluginChatMessage message) =>
        message.SenderObjectId != 0u
            ? $"{message.Sender} tells you, \"{message.Text}\""
            : $"You tell {message.Sender}, \"{message.Text}\"";

    private bool TickRecall(
        RouteWaypoint waypoint,
        in PluginNavigationSnapshot navigation)
    {
        if (waypoint.RecallSpellId == 0u && waypoint.Recall != RouteRecallKind.Marketplace)
        {
            // The route may have been read before the character's spell tables
            // were available, so the name gets one more pass against the live
            // catalogue before the waypoint is given up on.
            RouteWaypoint.ResolveRecallSpell(waypoint, _host.Automation.Spells);
        }
        if (waypoint.RecallSpellId == 0u && waypoint.Recall != RouteRecallKind.Marketplace)
        {
            _status = string.IsNullOrWhiteSpace(waypoint.RecallSpellName)
                ? "Recall waypoint has no spell; skipping."
                : $"Recall spell '{waypoint.RecallSpellName}' not found; skipping waypoint.";
            // The status line lives on a panel the player may not have open;
            // a skipped waypoint is silent without this.
            WarnOnce(_status);
            CompleteAction();
            return true;
        }

        if (!IsStandingStillForRecall(navigation))
        {
            _status = "Recall: waiting to come to a stop.";
            return true;
        }

        if (navigation.IsPortalSpace)
            _sawPortalSpace = true;
        if (_sawPortalSpace && !navigation.IsPortalSpace)
        {
            if (!_hasPortalOrigin
                || navigation.Position.HorizontalDistanceMeters(_portalOrigin)
                    > RecallExitDistanceMeters)
            {
                CompleteAction();
                return true;
            }
            _sawPortalSpace = false;
            _actionSent = false;
            _recallCastPending = false;
            _recallCastLanded = false;
            _retryElapsed = UseRetrySeconds;
        }
        if (_actionElapsed >= PortalTimeoutSeconds)
        {
            _status = "Recall timed out; continuing route.";
            CompleteAction();
            return true;
        }

        IMagicCommands magic = _host.Automation.Magic;
        if (magic.IsCasting)
        {
            // The cast is already doing the waypoint's job, and a second
            // request made on top of a pending one is simply turned away. So
            // the retry clock is wound back for as long as the cast is in the
            // air: it is meant to measure how long the waypoint has gone
            // WITHOUT a cast working for it. The overall timeout above keeps
            // running, so a cast that never resolves still gives up.
            _retryElapsed = 0d;
            _status = $"Recall: {waypoint.RecallLabel}.";
            return true;
        }
        if (_recallCastPending
            && magic.LastCompletion.Revision != _recallCastRevision)
        {
            // The cast finished. Without an error the teleport is on its way
            // and takes a moment to start, so the spell is not sent again
            // while the route waits for portal space; with an error (a fizzle
            // or a refusal) there is nothing to wait for and the next attempt
            // goes out on this tick rather than two seconds from now.
            _recallCastPending = false;
            if (magic.LastCompletion.IsSuccess)
                _recallCastLanded = true;
            else
                _retryElapsed = UseRetrySeconds;
        }

        // A recall is a spell, and the server drops any cast made outside
        // magic mode: it answers the request with nothing but a "done" and
        // the recall never happens. So the character is stood in magic mode
        // first, asked for on every tick until the mode is actually held,
        // and only then is the spell sent. The timeout above still applies,
        // so a mode that never arrives gives the waypoint up rather than
        // parking the route here.
        if (waypoint.RecallSpellId != 0u && !IsReadyToCastRecall())
        {
            _status = "Recall: switching to magic mode.";
            return true;
        }

        if (!_recallCastLanded && (!_actionSent || _retryElapsed >= UseRetrySeconds))
        {
            long issueRevision = magic.LastCompletion.Revision;
            bool accepted = SubmitRecall(waypoint);
            if (accepted && waypoint.RecallSpellId != 0u)
            {
                _recallCastPending = true;
                _recallCastRevision = issueRevision;
            }
            _actionSent |= accepted;
            if (accepted && !_hasPortalOrigin)
            {
                _portalOrigin = navigation.Position;
                _hasPortalOrigin = true;
            }
            _retryElapsed = 0d;
        }
        _status = $"Recall: {waypoint.RecallLabel}.";
        return true;
    }

    /// <summary>
    /// A recall is only cast from a standstill. The check is a position
    /// comparison against the last tick rather than a movement flag, because
    /// what matters is that the character has actually stopped drifting: the
    /// first tick captures, and every tick that has moved captures again and
    /// answers "still moving".
    /// </summary>
    private bool IsStandingStillForRecall(in PluginNavigationSnapshot navigation)
    {
        if (_recallNeedsPositionCapture)
        {
            _recallNeedsPositionCapture = false;
            _recallLastPosition = navigation.Position;
            return true;
        }

        if (navigation.Position.HorizontalDistanceMeters(_recallLastPosition)
            > RecallStationaryToleranceMeters)
        {
            _recallNeedsPositionCapture = true;
            return false;
        }

        _recallLastPosition = navigation.Position;
        return true;
    }

    /// <summary>
    /// Whether the character is standing in magic mode, asking the macro's
    /// own combat-mode gate for it on every tick until it is. The gate
    /// answers true once it has nothing left to do and the mode is held, and
    /// the mode the snapshot reports is what the cast would actually go out
    /// in; both are read, so a gate that ever answered ready mid-change could
    /// not let a cast slip through in the old mode. With no gate bound (a
    /// route running without the combat rules) there is nothing to ask and
    /// the cast goes out as before.
    /// </summary>
    private bool IsReadyToCastRecall()
    {
        if (_combatModeGate is null)
            return true;
        bool prepared = _combatModeGate.TryPrepare(PluginCombatMode.Magic);
        return prepared
            && _host.Automation.Combat.Snapshot.Mode == PluginCombatMode.Magic;
    }

    private bool SubmitRecall(RouteWaypoint waypoint)
    {
        if (waypoint.RecallSpellId == 0u)
            return _host.Automation.Chat.Submit("/marketplace");

        // Cast reports only "did it go out"; the request form says why it did
        // not, and a recall that is silently refused every retry until the
        // waypoint times out is the one failure this waypoint cannot explain
        // from its status line alone.
        PluginCastRequestResult request =
            _host.Automation.Magic.RequestCast(waypoint.RecallSpellId);
        if (request == PluginCastRequestResult.Sent)
            return true;

        // "Unavailable" is the one answer that is not news: the host gives it
        // both for a session that cannot send and for a cast asked for while
        // one is already in the air, and the second of those is the recall
        // this waypoint itself started. Saying so would be telling the player
        // that a working recall failed. Every other refusal still speaks.
        if (request == PluginCastRequestResult.Unavailable
            && _host.Automation.Magic.IsCasting)
            return false;

        WarnOnce(
            string.Create(
                CultureInfo.InvariantCulture,
                $"Recall '{waypoint.RecallLabel}' refused: {request}."));
        return false;
    }

    private bool TickJump(
        RouteWaypoint waypoint,
        double elapsedSeconds,
        in PluginNavigationSnapshot navigation)
    {
        if (!_jumpReleased)
        {
            if (!_jumpAligned)
            {
                float delta = SignedHeadingDelta(
                    navigation.Position.HeadingDegrees,
                    waypoint.JumpHeadingDegrees);
                // A jump is aimed far more tightly than a walk, and it waits
                // longer between attempts: a few degrees of error is nothing
                // when walking and is a missed ledge when jumping.
                if (Math.Abs(delta) >= JumpHeadingToleranceDegrees)
                {
                    INavigationAutomation nav = _host.Automation.Navigation;
                    _mover.NoteMovementIntent(
                        nav.ClearMovementIntent()
                            == PluginNavigationCommandStatus.Accepted
                            && _mover.HasMovementIntent);
                    if (_mover.Now - _jumpFaceHeadingStamp
                        > JumpFaceHeadingReissueSeconds)
                    {
                        _jumpFaceHeadingStamp = _mover.Now;
                        _ = nav.FaceHeading(waypoint.JumpHeadingDegrees);
                    }
                    _status = $"Aligning jump: {Math.Abs(delta):0.0}d.";
                    return true;
                }

                _jumpFaceHeadingStamp = NoFaceHeadingStamp;
                _jumpAligned = true;
            }

            _jumpChargeElapsed += elapsedSeconds;
            int effectiveChargeMilliseconds = Math.Clamp(
                waypoint.JumpChargeMilliseconds, 0, JumpChargeCeilingMilliseconds);
            bool hold = _jumpChargeElapsed * 1000d < effectiveChargeMilliseconds;
            if (hold)
            {
                PluginMovementIntent intent = JumpIntent(waypoint, jump: true);
                _mover.NoteMovementIntent(
                    _host.Automation.Navigation.SetMovementIntent(intent)
                        == PluginNavigationCommandStatus.Accepted);
                _status = $"Charging jump: {effectiveChargeMilliseconds}ms.";
                return true;
            }
            PluginMovementIntent release = JumpIntent(waypoint, jump: false);
            _ = _host.Automation.Navigation.SetMovementIntent(release);
            _jumpReleased = true;
            _jumpReleaseElapsed = 0d;
            _status = "Jump released.";
            return true;
        }
        _jumpReleaseElapsed += elapsedSeconds;
        _jumpSawAirborne |= navigation.IsAirborne;
        if (navigation.IsAirborne
            || (!_jumpSawAirborne
                && _jumpReleaseElapsed < JumpCompletionTimeoutSeconds)
            || (_jumpSawAirborne
                && _jumpReleaseElapsed < JumpLaunchGraceSeconds))
            return true;
        CompleteAction();
        return true;
    }

    private static PluginMovementIntent JumpIntent(
        RouteWaypoint waypoint,
        bool jump) => waypoint.JumpDirection switch
        {
            RouteJumpDirection.StrafeLeft => new PluginMovementIntent(
                StrafeLeft: true, Run: waypoint.JumpRun, Jump: jump),
            RouteJumpDirection.StrafeRight => new PluginMovementIntent(
                StrafeRight: true, Run: waypoint.JumpRun, Jump: jump),
            RouteJumpDirection.Backward => new PluginMovementIntent(
                Backward: true, Run: waypoint.JumpRun, Jump: jump),
            _ => new PluginMovementIntent(
                Forward: true, Run: waypoint.JumpRun, Jump: jump),
        };

    private void CompleteAction()
    {
        ClearAction();
        AdvanceWaypoint();
    }

    private void AdvanceWaypoint()
    {
        ClearAction();
        _clientHandOffIndex = -1;
        _clientStallPosted = false;
        _checkpointElapsed = 0d;
        int count = _settings.Waypoints.Count;
        if (count == 0)
            return;
        switch (_settings.Mode)
        {
            case RouteMode.Circular:
                _index = !_reverse
                    ? (_index + 1) % count
                    : (_index - 1 + count) % count;
                break;
            case RouteMode.Linear:
                if (!_reverse)
                {
                    _index++;
                    if (_index >= count)
                    {
                        _index = Math.Max(0, count - 1);
                        _reverse = true;
                    }
                }
                else
                {
                    _index--;
                    if (_index < 0)
                    {
                        _index = 0;
                        _reverse = false;
                    }
                }
                break;
            case RouteMode.Once:
                // A once-through route consumes its points in memory only: the
                // cursor walks forward and the list itself is left whole.
                // Taking the point out of the list instead emptied the very
                // list the profile is serialised from, so running a route to
                // the end wrote an empty route over the file on disk.
                _index++;
                _onceComplete = _index >= count;
                break;
        }
    }

    private void ClearAction()
    {
        _activeAction = null;
        _actionElapsed = 0d;
        _retryElapsed = 0d;
        _useCompletionBaseline = 0;
        _chatBaseline = 0;
        _actionSent = false;
        _sawPortalSpace = false;
        _jumpReleased = false;
        _jumpAligned = false;
        _jumpSawAirborne = false;
        _jumpChargeElapsed = 0d;
        _jumpReleaseElapsed = 0d;
        _portalOrigin = default;
        _hasPortalOrigin = false;
        _recallNeedsPositionCapture = true;
        _recallLastPosition = default;
        _recallCastPending = false;
        _recallCastLanded = false;
        _recallCastRevision = 0L;
        _jumpFaceHeadingStamp = NoFaceHeadingStamp;
    }

    /// <summary>
    /// The navigate rule's teardown on the losing-the-turn edge: the
    /// reference client releases the held movement keys, and ours is the same
    /// thing in acdream's terms — drop the movement intent. It is what the
    /// scheduler wires as <c>onLostTurn</c> for both navigate tiers.
    /// </summary>
    internal void StopForLostTurn()
    {
        StopClientWalk();
        _mover.StopForLostTurn();
    }

    private void StopMovement()
    {
        StopClientWalk();
        _mover.StopMovement();
    }

    private double BoundedMinimumDistance() => Math.Clamp(
        _settings.MinimumDistanceMeters,
        0.5d,
        50d);

    private double BoundedMaximumDistance() => Math.Max(
        BoundedMinimumDistance(),
        _settings.MaximumDistanceMeters);

    internal static float DesiredHeading(
        in PluginNavigationPosition from,
        in PluginNavigationPosition to)
    {
        double dx = to.EastWest - from.EastWest;
        double dy = to.NorthSouth - from.NorthSouth;
        double heading = Math.Atan2(dx, dy) * 180d / Math.PI;
        if (heading < 0d)
            heading += 360d;
        return (float)heading;
    }

    internal static float SignedHeadingDelta(float current, float desired)
    {
        float delta = (desired - current) % 360f;
        if (delta > 180f)
            delta -= 360f;
        else if (delta < -180f)
            delta += 360f;
        return delta;
    }

    internal static double DistanceToSegmentMeters(
        in PluginNavigationPosition point,
        in PluginNavigationPosition start,
        in PluginNavigationPosition end)
    {
        double dx = end.EastWest - start.EastWest;
        double dy = end.NorthSouth - start.NorthSouth;
        double lengthSquared = dx * dx + dy * dy;
        if (lengthSquared <= double.Epsilon)
            return point.HorizontalDistanceMeters(start);
        double projection = ((point.EastWest - start.EastWest) * dx
            + (point.NorthSouth - start.NorthSouth) * dy) / lengthSquared;
        projection = Math.Clamp(projection, 0d, 1d);
        double nearestX = start.EastWest + projection * dx;
        double nearestY = start.NorthSouth + projection * dy;
        double deltaX = point.EastWest - nearestX;
        double deltaY = point.NorthSouth - nearestY;
        return Math.Sqrt(deltaX * deltaX + deltaY * deltaY) * 240d;
    }
}
