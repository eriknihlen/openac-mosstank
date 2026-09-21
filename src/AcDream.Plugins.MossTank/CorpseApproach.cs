using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Walking to a corpse the looter cannot reach. It is a navigation goal like a
/// waypoint is: the same close-in mover, aimed at wherever the chosen corpse
/// currently is, re-asked every mover frame. The rule sits one position above
/// the open, so the character arrives first and the open then finds the corpse
/// inside its own arm's reach.
/// <para>
/// Two radii bound it, and both come from the profile. Outside the outer one
/// there is no goal at all — a corpse further away than the profile's reach is
/// simply not walked to — and inside the inner one there is no goal either,
/// which is what ends the walk: the rule stops being valid, loses the turn, and
/// the mover drops the keys.
/// </para>
/// </summary>
internal sealed class CorpseApproachController
{
    private readonly IPluginHost _host;
    private readonly LootSettings _settings;
    private readonly LootController _loot;
    private readonly NavigationMover _mover;

    private uint _goalCorpse;
    private double _goalDistance;
    private PluginNavigationPosition _goalPosition;
    private string _status = "No corpse to walk to.";

    internal CorpseApproachController(
        IPluginHost host,
        LootSettings settings,
        LootController loot)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _loot = loot ?? throw new ArgumentNullException(nameof(loot));
        _mover = new NavigationMover(host) { Status = value => _status = value };
    }

    internal void BindCombatModeGate(CombatModeGate gate, CombatSettings settings) =>
        _mover.BindCombatModeGate(gate, settings);

    internal string Status => _status;

    /// <summary>
    /// What the rule appends to its "Running" line: the range to the corpse and
    /// where it is, the same two facts the reference's navigate rule logs.
    /// </summary>
    internal string RunningDetail => _goalCorpse == 0u
        ? string.Empty
        : string.Create(
            CultureInfo.InvariantCulture,
            $"[targ range {_goalDistance:0.###}, "
                + $"targ loc {_goalPosition.EastWest:0.######}, "
                + $"{_goalPosition.NorthSouth:0.######}, "
                + $"{_goalPosition.Elevation:0.######} ]");

    internal bool IsOutsideCreepDistance()
    {
        return _loot.TrySelectApproachCorpse(
                _settings.CorpseApproachRange,
                out PluginLootContainer corpse)
            && corpse.Distance >= NavigationMover.CreepDistanceMeters;
    }

    /// <summary>The walk's own turn: it answers the pass and arms the mover.</summary>
    internal bool ClaimFromRulePass(bool canAct)
    {
        // The pass consumes whatever the mover was owed, so a rule turn and a
        // mover frame never both spend the same time.
        _ = _mover.TakePendingSeconds();
        bool claimed = Tick(canAct);
        _mover.Arm(claimed);
        return claimed;
    }

    /// <summary>
    /// One host frame of the armed mover. The corpse moves nowhere, but the
    /// character does, so the bearing and the range are re-asked on the mover's
    /// own interval rather than once per rule pass.
    /// </summary>
    internal void StepArmedMover(double elapsedSeconds)
    {
        if (_mover.TryTakeMoverFrame(elapsedSeconds, out _))
            _ = Tick(canAct: true);
    }

    /// <summary>
    /// Losing the turn drops the keys, exactly as the reference's navigate rule
    /// does when the scheduler hands the pass to somebody else.
    /// </summary>
    internal void StopForLostTurn()
    {
        _mover.StopForLostTurn();
        _goalCorpse = 0u;
    }

    private bool Tick(bool canAct)
    {
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot snapshot = navigation.Snapshot;
        if (!canAct)
            return Decline("Corpse walk paused.");
        if (!_settings.Enabled)
            return Decline("Looting disabled.");
        if (!snapshot.IsAvailable || snapshot.IsPortalSpace)
            return Decline("Waiting for the world.");
        if (!TryResolveGoal(out PluginLootContainer corpse))
            return Decline("No corpse to walk to.");

        // The outer radius is the profile's reach and the inner one is where
        // the walk ends. Both are exclusions, not a band the mover clamps to:
        // outside the reach there is nothing to walk to, and inside the inner
        // stop the walk is over and the open takes it from here.
        double distance = corpse.Distance;
        if (distance > _settings.CorpseApproachRange)
        {
            return Decline(string.Create(
                CultureInfo.InvariantCulture,
                $"{corpse.Name} is outside the corpse approach range "
                    + $"({distance:0.0}m)."));
        }
        if (distance <= _settings.CorpseMinimumApproachRange)
            return Decline($"Standing at {corpse.Name}.");

        _goalCorpse = corpse.ObjectId;
        _goalDistance = distance;
        _goalPosition = corpse.Position;
        _status = string.Create(
            CultureInfo.InvariantCulture,
            $"Walking to {corpse.Name} ({distance:0.0}m).");
        return _mover.Steer(
            navigation,
            snapshot.Position,
            corpse.Position,
            distance);
    }

    /// <summary>
    /// No goal this pass: say why, drop whatever the mover was holding, and
    /// forget the corpse so the running line does not describe a walk that is
    /// no longer happening.
    /// </summary>
    private bool Decline(string status)
    {
        _mover.StopMovement();
        _goalCorpse = 0u;
        _status = status;
        return false;
    }

    /// <summary>
    /// Which corpse, and where it is now. The pick is the looter's, so a corpse
    /// the looter would refuse — already emptied, refused by the server,
    /// unopenable, somebody else's kill, not yet described — is never walked
    /// to. A corpse the client cannot place is skipped: there is nowhere to
    /// walk.
    /// </summary>
    private bool TryResolveGoal(out PluginLootContainer corpse)
    {
        if (!_loot.TrySelectApproachCorpse(
                _settings.CorpseApproachRange,
                out corpse))
        {
            return false;
        }
        return corpse.HasPosition;
    }
}
