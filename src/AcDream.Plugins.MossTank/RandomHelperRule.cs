using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Occasionally casts one random beneficial spell on one nearby player.
/// The rule deliberately remembers no target enchantments or earlier picks.
/// </summary>
internal sealed class RandomHelperRule : IMacroRule
{
    private const double MaximumPlayerDistanceMeters = 18d;
    private const int MaximumSpellDraws = 100;

    private static readonly string[] TierOneSpellNames =
    [
        "Endurance Other I",
        "Regeneration Other I",
        "Rejuvenation Other I",
        "Armor Other I",
        "Blade Protection Other I",
        "Bludgeoning Protection Other I",
        "Cold Protection Other I",
        "Fire Protection Other I",
        "Lightning Protection Other I",
        "Piercing Protection Other I",
        "Acid Protection Other I",
    ];

    private readonly IPluginHost _host;
    private readonly BuffSettings _settings;
    private readonly ActionLockTable _actionLocks;
    private readonly CombatModeGate _combatModeGate;
    private readonly Random _random;
    private readonly HashSet<string> _postedWarnings = new(StringComparer.Ordinal);

    private PluginSpellInfo _pendingSpell;
    private uint _pendingTargetObjectId;
    private string _pendingTargetName = string.Empty;
    private bool _hasPendingCast;
    private string? _declineReason;

    public RandomHelperRule(
        IPluginHost host,
        BuffSettings settings,
        ActionLockTable actionLocks,
        CombatModeGate combatModeGate,
        Random? random = null)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _actionLocks = actionLocks
            ?? throw new ArgumentNullException(nameof(actionLocks));
        _combatModeGate = combatModeGate
            ?? throw new ArgumentNullException(nameof(combatModeGate));
        _random = random ?? new Random();
    }

    public string Name => "RandomHelper";

    public string Status { get; private set; } = string.Empty;

    public string? DeclineReason => _declineReason;

    public Action<MacroLogChannel, string>? Log { get; set; }

    public bool ValidNow(in MacroPassContext context)
    {
        ClearPendingCast();
        _declineReason = null;

        if (!context.CanAct)
            return false;
        if (!_settings.RandomHelperBuffs)
            return Decline("disabled");
        if (_actionLocks.IsLocked(ActionLockKind.ItemUse))
            return Decline("item use is locked");
        if (_actionLocks.IsLocked(ActionLockKind.RandomHelperBuffLock))
            return Decline("the helper interval is still active");

        IAutomationSurface automation = _host.Automation;
        if (!automation.IsAvailable)
            return Decline("the automation surface is unavailable");
        if (automation.Magic.IsCasting)
            return Decline("another cast is in progress");

        PluginNavigationSnapshot navigation = automation.Navigation.Snapshot;
        if (!navigation.IsAvailable)
            return Decline("the local position is unavailable");

        List<PluginWorldObject> players = CaptureNearbyPlayers(
            automation,
            navigation.Position);
        if (players.Count == 0)
            return Decline("no nearby player is eligible");

        // One target is retained for the complete spell search.
        PluginWorldObject target = players[_random.Next(players.Count)];
        BuffCastability castability = BuildCastability(automation);
        for (int attempt = 0; attempt < MaximumSpellDraws; attempt++)
        {
            string tierOneName =
                TierOneSpellNames[_random.Next(TierOneSpellNames.Length)];
            if (!BuffSelfRule.ResolveBestKnown(
                    automation,
                    tierOneName,
                    _settings,
                    castability,
                    out PluginSpellInfo spell))
            {
                continue;
            }

            // The intended branch accepts the first castable result. Keeping
            // the opposite branch would make the rule structurally inert.
            _pendingSpell = spell;
            _pendingTargetObjectId = target.ObjectId;
            _pendingTargetName = target.Name;
            _hasPendingCast = true;
            Status = $"Preparing {spell.Name} for {target.Name}";
            return true;
        }

        return Decline("no helper spell is currently castable");
    }

    public bool Running
    {
        get => false;
        set
        {
            if (!value)
            {
                ClearPendingCast();
                return;
            }
            IssuePendingCast();
        }
    }

    /// <summary>Clears this rule's pending work and its interval lock.</summary>
    public void Reset()
    {
        ClearPendingCast();
        _actionLocks.Release(ActionLockKind.RandomHelperBuffLock);
        ResetOncePerRunWarnings();
        _declineReason = null;
        Status = string.Empty;
    }

    public void ResetOncePerRunWarnings() => _postedWarnings.Clear();

    private List<PluginWorldObject> CaptureNearbyPlayers(
        IAutomationSurface automation,
        in PluginNavigationPosition localPosition)
    {
        uint self = automation.Character.ObjectId;
        var players = new List<PluginWorldObject>();
        foreach (PluginWorldObject candidate in automation.Objects.CaptureObjects())
        {
            if (candidate.ObjectClass != PluginObjectClass.Player
                || candidate.ObjectId == self
                || !candidate.HasPosition
                || localPosition.HorizontalDistanceMeters(candidate.Position)
                    >= MaximumPlayerDistanceMeters)
            {
                continue;
            }
            players.Add(candidate);
        }
        return players;
    }

    private BuffCastability BuildCastability(IAutomationSurface automation) =>
        new(
            automation.Spells,
            automation.Magic,
            automation.Items.IsAvailable
                ? automation.Items.CaptureOwnedItems()
                : [],
            _settings.BlacklistedSpellComponents,
            WarnOnce,
            static (_, _, _) => { });

    private void IssuePendingCast()
    {
        if (!_hasPendingCast)
            return;

        // Consume the pass-local pick before calling shared services. A
        // re-entrant scheduler activation therefore cannot issue it twice.
        PluginSpellInfo spell = _pendingSpell;
        uint targetObjectId = _pendingTargetObjectId;
        string targetName = _pendingTargetName;
        ClearPendingCast();

        if (!_combatModeGate.TryPrepare(PluginCombatMode.Magic))
        {
            Status = _combatModeGate.Status;
            return;
        }

        IMagicCommands magic = _host.Automation.Magic;
        PluginCastGate gate = magic.EvaluateGate(spell.SpellId, targetObjectId);
        if (gate != PluginCastGate.Ready)
        {
            Status = $"Cannot cast {spell.Name}: {gate}";
            return;
        }

        PluginCastRequestResult request =
            magic.RequestCast(spell.SpellId, targetObjectId);
        if (request != PluginCastRequestResult.Sent)
        {
            Status = $"Cannot cast {spell.Name}: {request}";
            return;
        }

        // A preparation attempt or a refused request does not consume the
        // configured interval. Only a cast accepted for issue earns the lock.
        _actionLocks.Arm(
            ActionLockKind.RandomHelperBuffLock,
            _settings.RandomHelperIntervalSeconds);
        Status = $"Casting {spell.Name} on {targetName}";
        Log?.Invoke(
            MacroLogChannel.SpellCast,
            $"Casting: {spell.Name} on {targetObjectId} ({targetName})");
        _host.Log.Info(
            $"MossTank: random helper {spell.Name} -> {targetName}");
    }

    private bool Decline(string reason)
    {
        _declineReason = reason;
        Status = string.Empty;
        return false;
    }

    private void WarnOnce(string text)
    {
        if (!_postedWarnings.Add(text))
            return;
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + text);
        _host.Log.Warn("MossTank: " + text);
    }

    private void ClearPendingCast()
    {
        _pendingSpell = default;
        _pendingTargetObjectId = 0u;
        _pendingTargetName = string.Empty;
        _hasPendingCast = false;
    }
}
