using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class CombatModeGate
{
    public const string NoWandNotice =
        "You must add at least one wand to your profile.";

    public const string BuggedCombatStateWarning =
        "Warning: Macro detected bugged combat state. Attempting to wield an "
        + "item to clear it.";

    private const double ModeConfirmationSeconds = 0.6d;

    private const uint WeaponReadyMask = 0x03500000u;

    private readonly IPluginHost _host;
    private readonly CombatSettings _settings;
    private readonly VitalSettings _vitalSettings;
    private readonly Action<string> _stopMacro;

    private int _dropToPeaceRetries;

    private double _sinceModeRequest = ModeConfirmationSeconds;
    private PluginCombatMode _modeBeforeRequest = PluginCombatMode.Unknown;
    private bool _modeRequestInFlight;
    private bool _noWandNoticePosted;
    private double _diagnosticTime;
    private double _nextBusyDiagnostic;

    internal double SinceModeRequestSecondsForTests => _sinceModeRequest;

    /// <summary>Warnings already posted this session, said once each.</summary>
    private readonly HashSet<string> _postedWarnings = new(StringComparer.Ordinal);

    public CombatModeGate(
        IPluginHost host,
        CombatSettings settings,
        VitalSettings vitalSettings,
        Action<string> stopMacro)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _vitalSettings = vitalSettings
            ?? throw new ArgumentNullException(nameof(vitalSettings));
        _stopMacro = stopMacro ?? throw new ArgumentNullException(nameof(stopMacro));
    }

    public string Status { get; private set; } = string.Empty;

    private ActionLockTable _actionLocks = new();

    /// <summary>
    /// Shares the macro's cooldown table, so the item this gate uses to clear
    /// a stuck combat state holds every other rule off the way any other item
    /// use does.
    /// </summary>
    internal void BindActionLocks(ActionLockTable locks) =>
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));

    public Action<MacroLogChannel, string>? Log { get; set; }

    public Func<uint, MonsterDamageType, bool>? AmmunitionStale { get; set; }

    public Func<MonsterDamageType, bool>? WieldAmmunition { get; set; }

    public static bool IsCaster(in PluginEquipmentItem item) =>
        item.ObjectClass == PluginObjectClass.WandStaffOrb;

    /// <summary>
    /// Which stance a weapon puts the character in. This is the item's CLASS,
    /// not a guess from its numbers: a thrown weapon is a missile weapon even
    /// though it takes no ammunition, and a weapon that lists no damage is
    /// still a melee weapon.
    /// </summary>
    public static PluginCombatMode ModeFor(in PluginEquipmentItem item)
    {
        if (item.ObjectClass == PluginObjectClass.MeleeWeapon)
            return PluginCombatMode.Melee;
        if (item.ObjectClass == PluginObjectClass.MissileWeapon)
            return PluginCombatMode.Missile;
        return PluginCombatMode.Magic;
    }

    public void ResetOncePerRunWarnings()
    {
        _noWandNoticePosted = false;
        _postedWarnings.Clear();
    }

    private void PostWarningOnce(string text)
    {
        if (!_postedWarnings.Add(text))
            return;
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + text);
    }

    public void Reset()
    {
        _dropToPeaceRetries = 0;
        _sinceModeRequest = ModeConfirmationSeconds;
        _modeBeforeRequest = PluginCombatMode.Unknown;
        _modeRequestInFlight = false;
        _noWandNoticePosted = false;
        _postedWarnings.Clear();
        Status = string.Empty;
    }

    public void AdvancePass(double elapsedSeconds)
    {
        _sinceModeRequest += Math.Max(0d, elapsedSeconds);
        _diagnosticTime += Math.Max(0d, elapsedSeconds);
    }

    /// <param name="captured">
    /// The caller's own equipment projection, when it already has one for
    /// this pass. Building it walks and sorts every object the client knows,
    /// so a caller that asks many times in one pass hands its copy in rather
    /// than paying for it again. Omit it and the gate reads the host itself.
    /// </param>
    public bool TryPrepare(
        PluginCombatMode wanted,
        uint overrideItemId = 0u,
        bool autoSelect = true,
        MonsterDamageType element = MonsterDamageType.None,
        IReadOnlyList<PluginEquipmentItem>? captured = null)
    {
        IAutomationSurface automation = _host.Automation;
        IEquipmentAutomation equipment = automation.Equipment;

        // A dispatched inventory operation blocks mode preparation. BusyCount
        // also covers appraisals and use requests, so it cannot stand in for
        // the outstanding inventory operation.
        PluginBusyState busy = automation.Recovery.CaptureBusyState();
        if (busy.PendingInventory)
        {
            Status = "Busy";
            if (_diagnosticTime >= _nextBusyDiagnostic)
            {
                _nextBusyDiagnostic = _diagnosticTime + 5d;
                _host.Log.Info($"Macro busy: count={busy.BusyCount}, inventory={busy.PendingInventory}, " +
                    $"appraisal=0x{busy.AwaitingAppraisal:X8}, source=0x{busy.UseSource:X8}, " +
                    $"target=0x{busy.UseTarget:X8}, awaitingUse={busy.AwaitingUseCompletion}, " +
                    $"equipment={equipment.IsBusy}, magic={automation.Magic.IsCasting}");
            }
            return false;
        }

        if (!equipment.IsAvailable)
            return TryPrepareMode(wielded: null, wanted);

        IReadOnlyList<PluginEquipmentItem> items =
            captured ?? equipment.CaptureOwnedEquipment();
        PluginEquipmentItem? wielded = FindWieldedFor(items, wanted);

        uint primary = overrideItemId;
        if (primary != 0u)
        {
            PluginEquipmentItem? requested = FindById(items, primary);
            if (!IsOwnedKnownObject(primary, requested))
            {
                if (requested is { } named)
                {
                    Status = $"Warning: FCM ignoring item {named.Name} because "
                        + "it cannot currently be wielded.";
                    PostWarningOnce(Status);
                }
                primary = 0u;
            }
        }

        if ((autoSelect || overrideItemId == 0u)
            && wielded is { } worn
            && ModeFor(in worn) == wanted)
        {
            primary = worn.ObjectId;
        }

        // Otherwise the first wand on the Items page.
        if (primary == 0u)
        {
            if (FindFirstProfiledWand(items) is not { } fallback)
            {
                PostNoWandNoticeAndStop();
                return false;
            }
            primary = fallback.ObjectId;
        }

        bool flag2 = AmmunitionStale?.Invoke(primary, element) == true;
        if (flag2 || wielded?.ObjectId != primary)
        {
            if (wielded?.ObjectId != primary)
            {
                PluginEquipmentItem? target = FindById(items, primary);
                string name = target?.Name
                    ?? (_host.Automation.Objects.TryGet(primary, out PluginWorldObject world)
                        ? world.Name : "caster");

                // The drop-to-peace branch, with the retry budget and the
                // stuck-state recovery.
                if (!TryDropToPeace(items, name))
                    return false;

                Log?.Invoke(MacroLogChannel.BusyState, $"(FCM) equip {name}");
                PluginEquipmentCommandResult equip = equipment.Equip(primary);
                // Only a started switch is progress. Anything else will say
                // the same thing on the next pass and the one after, so it
                // is reported as the standstill it is rather than as an
                // equip that is under way.
                if (equip.Status != PluginEquipmentCommandStatus.Started)
                {
                    Status = equip.Notice
                        ?? $"Cannot equip {name} ({equip.Status}).";
                    PostWarningOnce(Status);
                    return false;
                }
                Status = $"Equipping {name}";
                return false;
            }

            // Reached only once the weapon already matches.
            if (flag2 && WieldAmmunition?.Invoke(element) == true)
                return false;
        }

        return TryPrepareMode(wielded, wanted);
    }

    /// <summary>
    /// Recompute the mode the wielded item implies and ask for it if it
    /// differs. Returns true only once they agree.
    /// </summary>
    private bool TryPrepareMode(
        PluginEquipmentItem? wielded,
        PluginCombatMode wanted)
    {
        PluginCombatMode implied = wielded is { } equipped
            ? ModeFor(in equipped)
            : wanted;

        if (_host.Automation.Combat.Snapshot.Mode != implied
            && !RequestMode(implied))
        {
            return false;
        }

        Status = "Ready";
        return true;
    }

    public bool TryDropToPeace(
        IReadOnlyList<PluginEquipmentItem> items,
        string forItemName)
    {
        ArgumentNullException.ThrowIfNull(items);

        PluginCombatMode mode = EffectiveMode();

        if (mode == PluginCombatMode.Unknown)
        {
            Status = "Waiting for the combat mode";
            return false;
        }

        if (mode == PluginCombatMode.Peace)
        {
            _dropToPeaceRetries = 0; // already there: the budget resets
            return true;
        }

        _dropToPeaceRetries++;
        if (_dropToPeaceRetries >= _vitalSettings.DropToPeaceModeRetryCount)
        {
            _dropToPeaceRetries = 0;
            if (FindFirstProfiledWand(items) is not { } recovery)
            {
                PostNoWandNoticeAndStop();
                return false;
            }

            _actionLocks.Arm(
                ActionLockKind.ItemUse,
                ItemUseLock.ImmediateSeconds);
            PluginItemCommandResult use =
                _host.Automation.Items.Use(recovery.ObjectId);
            Status = use.Status == PluginItemCommandStatus.Started
                ? BuggedCombatStateWarning
                : $"Combat-state recovery with {recovery.Name}: {use.Status}";
            if (use.Status == PluginItemCommandStatus.Started)
                _host.Automation.Chat.PostSystemMessage("[MossTank] " + BuggedCombatStateWarning);
            return false;
        }

        if (RequestMode(PluginCombatMode.Peace))
        {
            _dropToPeaceRetries = 0;
            return true;
        }
        if (Status.Length == 0 || Status.StartsWith("Entering", StringComparison.Ordinal))
            Status = $"Entering peace mode to equip {forItemName}";
        return false;
    }

    private readonly record struct ProfiledCaster(uint ObjectId, string Name);

    private bool IsOwnedKnownObject(
        uint objectId, PluginEquipmentItem? projected)
    {
        IWorldObjectAutomation objects = _host.Automation.Objects;
        if (objects.IsAvailable)
            return objects.TryGet(objectId, out PluginWorldObject value)
                && value.IsOwned;
        return projected is not null;
    }

    private ProfiledCaster? FindFirstProfiledWand(
        IReadOnlyList<PluginEquipmentItem> items)
    {
        IWorldObjectAutomation objects = _host.Automation.Objects;
        uint playerId = _host.Automation.Character.ObjectId;
        if (objects.IsAvailable)
        {
            foreach (uint id in _settings.CombatItemOrderIds)
            {
                if (id is 0u or uint.MaxValue || id == playerId
                    || !objects.TryGet(id, out PluginWorldObject item)
                    || !item.IsOwned
                    || item.ObjectClass != PluginObjectClass.WandStaffOrb)
                    continue;
                return new(item.ObjectId, item.Name);
            }
            return null;
        }
        foreach (uint id in _settings.CombatItemOrderIds)
        {
            if (id is 0u or uint.MaxValue || id == playerId)
                continue;
            foreach (PluginEquipmentItem item in items)
            {
                if (item.ObjectId == id && IsCaster(in item))
                    return new(item.ObjectId, item.Name);
            }
        }
        return null;
    }

    private static PluginEquipmentItem? FindById(
        IReadOnlyList<PluginEquipmentItem> items,
        uint objectId)
    {
        foreach (PluginEquipmentItem item in items)
        {
            if (item.ObjectId == objectId)
                return item;
        }
        return null;
    }

    /// <summary>
    /// The weapon the character is holding: the item sitting in one of the
    /// four slots a weapon occupies, which is the slot it is IN and not
    /// merely one it could go in -- the reference reads the same four
    /// wielded-location values.
    /// </summary>
    internal static PluginEquipmentItem? FindWielded(
        IReadOnlyList<PluginEquipmentItem> items)
    {
        foreach (PluginEquipmentItem item in items)
        {
            if (IsWeaponSlot(item.EquippedLocation))
                return item;
        }
        return null;
    }

    internal static bool IsWeaponSlot(uint location) =>
        location is 0x01000000u or 0x00100000u
            or 0x00400000u or 0x02000000u;

    private static PluginEquipmentItem? FindWieldedFor(
        IReadOnlyList<PluginEquipmentItem> items,
        PluginCombatMode wanted) => FindWielded(items);

    private PluginCombatMode EffectiveMode()
    {
        PluginCombatMode live = _host.Automation.Combat.Snapshot.Mode;

        if (_modeRequestInFlight && live != _modeBeforeRequest)
        {
            _modeRequestInFlight = false;
            _sinceModeRequest = 0d;
        }

        // The confirmation window is purely time-based — the reference
        // client's two arms are the same expression, so the flag it also
        // carries never affects the answer. Inside the window the saved mode
        // is the answer; outside it, the live one.
        return _sinceModeRequest < ModeConfirmationSeconds
            ? _modeBeforeRequest
            : live;
    }

    private bool RequestMode(PluginCombatMode mode)
    {
        _modeBeforeRequest = _host.Automation.Combat.Snapshot.Mode;
        _sinceModeRequest = 0d;
        _modeRequestInFlight = true;
        Log?.Invoke(MacroLogChannel.BusyState, $"(FCM) requesting {mode}");
        PluginCombatCommandResult result =
            _host.Automation.Combat.EnterMode(mode);
        if (result.Status == PluginCombatCommandStatus.Unavailable)
        {
            _modeRequestInFlight = false;
            return true;
        }
        Status = result.Status == PluginCombatCommandStatus.Refused
            ? result.Notice ?? $"Cannot enter {mode} mode"
            : $"Entering {mode} mode";
        return false;
    }

    private void PostNoWandNoticeAndStop()
    {
        Status = NoWandNotice;
        if (!_noWandNoticePosted)
        {
            _host.Automation.Chat.PostSystemMessage("[MossTank] " + NoWandNotice);
            _noWandNoticePosted = true;
        }
        _stopMacro(NoWandNotice);
    }
}
