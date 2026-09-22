using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>The switches an equipment run reads, live, from the settings page.</summary>
internal sealed class EquipProfileSettings
{
    public Func<bool> Think { get; init; } = static () => false;
}

/// <summary>
/// Puts on a set of gear named by a loot profile under <c>ub/equip/</c>:
/// everything not in the set comes off, then the set goes on with the
/// lore-gated pieces last, so a piece that raises lore is worn before the
/// piece that needs it. It also writes such a profile from what is worn
/// now, and lists and dry-runs them.
/// </summary>
/// <remarks>
/// <para>
/// The combat controller already owns what is wielded while a monster is
/// being fought, through its per-monster equipment step. Two owners for
/// equipment placement is a known failure class in this plugin, so this
/// controller is a prologue owner only: it refuses to start while the combat
/// controller has a target, says why, and holds the item-use and navigation
/// locks for its whole run so nothing else dresses the character meanwhile.
/// </para>
/// <para>
/// Each step waits for the client's own placement receipt rather than
/// re-sending every frame: an equip goes out once and the next goes out
/// when the piece is seen in its slot; a dequip is a move to the player's
/// own pack and the next waits for the piece to be seen leaving. A run
/// with no receipt for ten seconds gives up.
/// </para>
/// </remarks>
internal sealed class EquipProfileController : IDisposable
{
    /// <summary>A run with no progress for this long gives up.</summary>
    private const double BailSeconds = 10d;

    /// <summary>
    /// How long a step holds the locks. Re-armed every tick while running
    /// and let go when the run ends, so the figure only matters if the
    /// plugin stops ticking.
    /// </summary>
    private const double LockSeconds = 30d;

    /// <summary>One appraisal request in flight at a time, this far apart.</summary>
    private const double IdentifyIntervalSeconds = 0.5d;

    /// <summary>How long one appraisal is waited for before it counts as failed.</summary>
    private const double IdentifyTimeoutSeconds = 10d;

    private const int MaximumIdentifyAttemptsPerItem = 3;

    /// <summary>How often the wait for appraisals is mentioned in chat.</summary>
    private const double IdentifyNoticeSeconds = 15d;

    /// <summary>How often one piece is asked to go on before it is skipped.</summary>
    private const int MaximumEquipAttemptsPerItem = 15;

    /// <summary>How long a create keeps its "say it again to overwrite" offer.</summary>
    private const double OverwriteConfirmationSeconds = 30d;

    private const string ProfileFolder = "equip";
    private const string ProfileExtension = ".utl";

    /// <summary>The item property that carries the lore needed to use it.</summary>
    private const uint LoreRequirementProperty = 109u;

    /// <summary>The material property, in the created profile.</summary>
    private const uint MaterialProperty = 131u;

    /// <summary>The value property, in the created profile.</summary>
    private const uint ValueProperty = 19u;

    /// <summary>The loot-profile key space for an item's own fields.</summary>
    private const uint ProfileIntBase = 218_103_808u;

    /// <summary>The slots an item can occupy, in that key space.</summary>
    private const uint ProfileEquipableSlotsKey = ProfileIntBase + 14;

    /// <summary>The name string key in a loot profile requirement.</summary>
    private const uint ProfileNameKey = 1u;

    private const int StringMatchRequirement = 1;
    private const int IntEqualsRequirement = 12;

    private static readonly PluginItemProperties EmptyProperties = new(
        new Dictionary<uint, int>(),
        new Dictionary<uint, long>(),
        new Dictionary<uint, bool>(),
        new Dictionary<uint, double>(),
        new Dictionary<uint, string>(),
        new Dictionary<uint, uint>(),
        new Dictionary<uint, uint>());

    private enum Phase
    {
        Idle,
        Identifying,
        Dequipping,
        Equipping,
    }

    private enum Mode
    {
        Load,
        Test,
    }

    private readonly IPluginHost _host;
    private readonly UbSettingStore _store;
    private readonly Func<bool> _combatHasTarget;
    private readonly List<LootRule> _rules = [];
    private readonly List<PluginInventoryItem> _candidates = [];
    private readonly Queue<uint> _toEquip = new();
    private readonly HashSet<uint> _profileItems = [];
    private readonly Dictionary<uint, int> _identifyAttempts = [];
    private readonly Action<PluginEquipmentObservation> _onPlacementObserved;
    private readonly Queue<PluginEquipmentObservation> _observations = new();
    private EquipProfileSettings _settings = new();
    private ActionLockTable? _locks;
    private Phase _phase = Phase.Idle;
    private Mode _mode = Mode.Load;
    private string _profileKey = string.Empty;
    private double _now;
    private double _sinceProgress;
    private double _runSeconds;
    private double _sinceIdentifyRequest;
    private double _awaitingIdentifySeconds;
    private double _lastIdentifyNotice = double.NegativeInfinity;
    private uint _awaitingIdentify;
    private uint _dequipping;
    private uint _equipping;
    private int _equipAttempts;
    private string _pendingOverwriteKey = string.Empty;
    private double _pendingOverwriteAt = double.NegativeInfinity;
    private bool _disposed;

    public EquipProfileController(
        IPluginHost host,
        UbSettingStore store,
        Func<bool> combatHasTarget)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _store = store ?? throw new ArgumentNullException(nameof(store));
        _combatHasTarget = combatHasTarget ?? throw new ArgumentNullException(nameof(combatHasTarget));
        _onPlacementObserved = _observations.Enqueue;
        host.Automation.Equipment.PlacementObserved += _onPlacementObserved;
    }

    public bool IsRunning => _phase != Phase.Idle;

    public string Status { get; private set; } = "Equip profile idle.";

    public void BindSettings(EquipProfileSettings settings) =>
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));

    public void BindActionLocks(ActionLockTable locks) => _locks = locks;

    /// <summary>The <c>/vt equip</c> verbs: list, load, test and create.</summary>
    public IReadOnlyList<string> Command(string arguments)
    {
        const string usage = "Syntax: /vt equip list | load <profile> | test <profile> | create <profile>";
        string text = (arguments ?? string.Empty).Trim();
        int space = text.IndexOf(' ');
        string verb = (space < 0 ? text : text[..space]).ToLowerInvariant();
        string rest = space < 0 ? string.Empty : text[(space + 1)..].Trim();
        switch (verb)
        {
            case "list":
                return List(rest);
            case "load":
                TryStart(rest, Mode.Load);
                return [Status];
            case "test":
                TryStart(rest, Mode.Test);
                return [Status];
            case "create":
                return [Create(rest)];
            default:
                return [usage];
        }
    }

    /// <summary>
    /// Starts a load or a dry run. False, with the reason in
    /// <see cref="Status"/>, when the combat controller has a target, a run
    /// is already up, the session is not in the world, or the profile
    /// cannot be found or read.
    /// </summary>
    public bool TryStart(string? profileName, bool testOnly) =>
        TryStart(profileName, testOnly ? Mode.Test : Mode.Load);

    private bool TryStart(string? profileName, Mode mode)
    {
        if (!_host.Automation.IsAvailable || !_host.Automation.Character.IsInWorld)
        {
            Status = "Equip profile refused: not in the world.";
            return false;
        }
        if (IsRunning)
        {
            Status = "Equip profile refused: a run is already in progress.";
            return false;
        }
        if (_combatHasTarget())
        {
            Status = "Equip profile refused: the macro has a monster target and owns "
                + "what is wielded; stop the macro or wait for the fight to end.";
            return false;
        }
        string key = ResolveKey(profileName);
        string? text = _host.VtankProfiles.IsAvailable ? _host.VtankProfiles.ReadText(key) : null;
        if (text is null)
        {
            Status = $"No equip profile exists: {key}";
            return false;
        }
        _rules.Clear();
        if (!MossTankLootProfileStore.TryParseRules(text, _rules))
        {
            Status = $"Equip profile could not be read: {key}";
            return false;
        }

        _mode = mode;
        _profileKey = key;
        _candidates.Clear();
        _candidates.AddRange(EquippableItems());
        _toEquip.Clear();
        _profileItems.Clear();
        _identifyAttempts.Clear();
        _observations.Clear();
        _awaitingIdentify = 0u;
        _awaitingIdentifySeconds = 0d;
        _sinceIdentifyRequest = IdentifyIntervalSeconds;
        _lastIdentifyNotice = double.NegativeInfinity;
        _dequipping = 0u;
        _equipping = 0u;
        _equipAttempts = 0;
        _sinceProgress = 0d;
        _runSeconds = 0d;
        _phase = Phase.Identifying;
        Status = mode == Mode.Test
            ? $"Equip profile test started: {key} ({_candidates.Count} candidate(s))."
            : $"Equip profile started: {key} ({_candidates.Count} candidate(s)).";
        return true;
    }

    /// <summary>
    /// Advances the run. True while it owns the character. The locks are
    /// re-armed on every tick of a load so a slow appraisal round cannot
    /// let them lapse mid-run.
    /// </summary>
    public bool Tick(double elapsedSeconds, bool canAct)
    {
        double step = Math.Max(0d, elapsedSeconds);
        _now += step;
        if (!IsRunning)
        {
            _observations.Clear();
            return false;
        }
        if (!_host.Automation.IsAvailable)
        {
            Stop("Equip profile stopped: the session ended.");
            return false;
        }
        _runSeconds += step;
        _sinceProgress += step;
        if (_mode == Mode.Load)
        {
            _locks?.Arm(ActionLockKind.ItemUse, LockSeconds);
            _locks?.Arm(ActionLockKind.Navigation, LockSeconds);
        }
        DrainObservations();
        if (_sinceProgress > BailSeconds)
        {
            Stop("Equip profile bailed: timeout expired.");
            return false;
        }
        if (!canAct)
            return true;

        // The phases fall through within one tick: a phase that has nothing
        // left to wait for hands over at once rather than costing a frame.
        if (_phase == Phase.Identifying)
        {
            TickIdentify(step);
            if (_awaitingIdentify != 0u || NextToIdentify() is not null)
                return true;
            BuildQueue();
            if (_mode == Mode.Test)
            {
                ReportTest();
                Stop($"Equip profile test finished: {_toEquip.Count} item(s).");
                return false;
            }
            _phase = Phase.Dequipping;
            _sinceProgress = 0d;
        }
        if (_phase == Phase.Dequipping)
        {
            if (IsBusy())
                return true;
            if (_dequipping != 0u)
            {
                if (IsEquipped(_dequipping))
                    return true;
                _dequipping = 0u;
                _sinceProgress = 0d;
            }
            if (NextToDequip() is { } worn)
            {
                PluginItemCommandResult moved = _host.Automation.Items.MoveToContainer(
                    worn, _host.Automation.Character.ObjectId);
                if (moved.Status == PluginItemCommandStatus.Started)
                    _dequipping = worn;
                else
                    Stop($"Equip profile stopped: could not take off {Name(worn)} ({moved.Status}).");
                return IsRunning;
            }
            _phase = Phase.Equipping;
            _sinceProgress = 0d;
        }
        TickEquip();
        return IsRunning;
    }

    public void Reset()
    {
        if (IsRunning)
            Stop(null);
        _observations.Clear();
        _pendingOverwriteKey = string.Empty;
        _pendingOverwriteAt = double.NegativeInfinity;
    }

    public void Dispose()
    {
        if (_disposed)
            return;
        _disposed = true;
        Reset();
        _host.Automation.Equipment.PlacementObserved -= _onPlacementObserved;
    }

    // ---- the run ----------------------------------------------------------

    private void TickIdentify(double step)
    {
        _sinceIdentifyRequest += step;
        if (_awaitingIdentify != 0u)
        {
            _awaitingIdentifySeconds += step;
            if (HasAppraisalData(_awaitingIdentify))
            {
                _awaitingIdentify = 0u;
                _awaitingIdentifySeconds = 0d;
                _sinceProgress = 0d;
                return;
            }
            if (_awaitingIdentifySeconds < IdentifyTimeoutSeconds)
                return;
            _identifyAttempts[_awaitingIdentify] = Attempts(_awaitingIdentify) + 1;
            _awaitingIdentify = 0u;
            _awaitingIdentifySeconds = 0d;
        }
        if (_sinceIdentifyRequest < IdentifyIntervalSeconds)
            return;
        if (NextToIdentify() is not { } next)
            return;
        _sinceIdentifyRequest = 0d;
        if (_runSeconds - _lastIdentifyNotice >= IdentifyNoticeSeconds)
        {
            _lastIdentifyNotice = _runSeconds;
            int remaining = _candidates.Count(item => !HasAppraisalData(item.ObjectId)
                && Attempts(item.ObjectId) < MaximumIdentifyAttemptsPerItem
                && LootRuleEngine.NeedsIdentify(item, PropertiesOf(item.ObjectId), _rules, _host));
            Write($"Equip profile: waiting to identify {remaining} item(s), about {remaining} second(s).");
        }
        PluginItemCommandResult asked = _host.Automation.Objects.Identify(next);
        if (asked.Status == PluginItemCommandStatus.Started)
        {
            _awaitingIdentify = next;
            _awaitingIdentifySeconds = 0d;
        }
        else
        {
            _identifyAttempts[next] = Attempts(next) + 1;
        }
    }

    /// <summary>
    /// The first candidate the rules cannot decide without an appraisal
    /// that has not been answered and has attempts left.
    /// </summary>
    private uint? NextToIdentify()
    {
        foreach (PluginInventoryItem item in _candidates)
        {
            if (HasAppraisalData(item.ObjectId)
                || Attempts(item.ObjectId) >= MaximumIdentifyAttemptsPerItem)
            {
                continue;
            }
            PluginItemProperties properties = PropertiesOf(item.ObjectId);
            if (LootRuleEngine.NeedsIdentify(item, properties, _rules, _host))
                return item.ObjectId;
        }
        return null;
    }

    /// <summary>
    /// The pieces the profile keeps, in the order they go on: by the lore
    /// they require, lowest first, so a piece that raises lore is worn
    /// before a piece that needs it. The sort is stable, so pieces with the
    /// same requirement keep pack order.
    /// </summary>
    private void BuildQueue()
    {
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        var chosen = new List<(PluginInventoryItem Item, int Lore)>();
        foreach (PluginInventoryItem item in _candidates)
        {
            PluginItemProperties properties = PropertiesOf(item.ObjectId);
            LootDecision? decision = LootRuleEngine.Decide(item, properties, _rules, owned, host: _host);
            if (decision is null
                || decision.Value.Action is not (LootAction.Keep or LootAction.KeepUpTo))
            {
                continue;
            }
            int lore = properties.Ints is { } ints
                && ints.TryGetValue(LoreRequirementProperty, out int value)
                    ? value
                    : 0;
            chosen.Add((item, lore));
        }
        _toEquip.Clear();
        _profileItems.Clear();
        foreach ((PluginInventoryItem item, _) in chosen.OrderBy(static entry => entry.Lore))
        {
            _toEquip.Enqueue(item.ObjectId);
            _profileItems.Add(item.ObjectId);
        }
    }

    private void ReportTest()
    {
        Write("Will attempt to equip the following items in order:");
        foreach (uint objectId in _toEquip)
            Write($" * {Name(objectId)} <{objectId}>");
    }

    private uint? NextToDequip()
    {
        foreach (PluginEquipmentItem item in _host.Automation.Equipment.CaptureOwnedEquipment())
        {
            if (item.IsEquipped && !_profileItems.Contains(item.ObjectId))
                return item.ObjectId;
        }
        return null;
    }

    private void TickEquip()
    {
        if (IsBusy())
            return;
        while (_toEquip.Count > 0)
        {
            uint head = _toEquip.Peek();
            if (!TryFindOwned(head, out PluginInventoryItem item))
            {
                Write($"Equip profile: could not find item {head}; skipping.");
                _toEquip.Dequeue();
                _sinceProgress = 0d;
                continue;
            }
            if (item.IsEquipped)
            {
                _toEquip.Dequeue();
                _sinceProgress = 0d;
                if (_equipping == head)
                    _equipping = 0u;
                continue;
            }
            if (_equipping == head)
            {
                // Sent and not yet seen in its slot: wait for the receipt.
                return;
            }
            if (_equipping != head)
            {
                _equipping = head;
                _equipAttempts = 0;
            }
            if (++_equipAttempts > MaximumEquipAttemptsPerItem)
            {
                Write($"Equip profile: too many attempts on {item.Name}; skipping.");
                _toEquip.Dequeue();
                _equipping = 0u;
                _sinceProgress = 0d;
                continue;
            }
            // No slot request: the client picks the slot, as the reference
            // tool does. A mask is forwarded verbatim and the server widens
            // it only for armour and clothing, so an item that fits two
            // slots (a ring, a bracelet) would claim both and the second
            // piece of the pair would be refused.
            PluginEquipmentCommandResult sent =
                _host.Automation.Equipment.Equip(head, requestedLocation: 0u);
            switch (sent.Status)
            {
                case PluginEquipmentCommandStatus.Started:
                    return;
                case PluginEquipmentCommandStatus.AlreadyEquipped:
                    _toEquip.Dequeue();
                    _equipping = 0u;
                    _sinceProgress = 0d;
                    continue;
                case PluginEquipmentCommandStatus.Busy:
                    return;
                default:
                    // Refused, unknown to the client or unavailable: the
                    // piece is skipped rather than asked for again.
                    _equipping = 0u;
                    Write($"Equip profile: {item.Name} was not accepted ({sent.Status}"
                        + (sent.Notice is { Length: > 0 } notice ? $": {notice}" : string.Empty)
                        + "); skipping.");
                    _toEquip.Dequeue();
                    _sinceProgress = 0d;
                    continue;
            }
        }
        Stop($"Finished equipping items in {_runSeconds.ToString("0.0", CultureInfo.InvariantCulture)}s");
    }

    /// <summary>
    /// The client's placement receipts, read on the tick they arrive: a
    /// piece seen in its slot advances the equip queue, a piece seen leaving
    /// its slot finishes the dequip in flight.
    /// </summary>
    private void DrainObservations()
    {
        while (_observations.Count > 0)
        {
            PluginEquipmentObservation observation = _observations.Dequeue();
            if (observation.IsRemoval)
            {
                if (observation.ObjectId == _dequipping)
                {
                    _dequipping = 0u;
                    _sinceProgress = 0d;
                }
                continue;
            }
            if (_toEquip.Count > 0 && observation.ObjectId == _toEquip.Peek())
            {
                _toEquip.Dequeue();
                _equipping = 0u;
                _sinceProgress = 0d;
            }
        }
    }

    private void Stop(string? notice)
    {
        bool wasLoad = _mode == Mode.Load && _phase is Phase.Dequipping or Phase.Equipping;
        _phase = Phase.Idle;
        _toEquip.Clear();
        _profileItems.Clear();
        _candidates.Clear();
        _awaitingIdentify = 0u;
        _dequipping = 0u;
        _equipping = 0u;
        _locks?.Release(ActionLockKind.ItemUse);
        _locks?.Release(ActionLockKind.Navigation);
        if (notice is null)
        {
            Status = "Equip profile idle.";
            return;
        }
        Status = notice;
        if (wasLoad && notice.StartsWith("Finished", StringComparison.Ordinal))
            Think("Equipment Manager: " + notice);
        else
            Write(notice);
    }

    // ---- create and list ----------------------------------------------------

    /// <summary>
    /// Writes what is worn as a profile in the character's own folder: one
    /// keep rule per piece, matched by exact name, the slots it fits, its
    /// value and, when it has one, its material. An existing file is not
    /// written over until the same command is given again within half a
    /// minute; a live profile was overwritten once by a create that asked
    /// nothing.
    /// </summary>
    private string Create(string? profileName)
    {
        if (!_host.Automation.IsAvailable || !_host.Automation.Character.IsInWorld)
            return "Equip profile create refused: not in the world.";
        if (!_host.VtankProfiles.IsAvailable)
            return "Equip profile create refused: storage is unavailable.";
        string fileName = FileName(profileName);
        string key = _store.CharacterProfileKey(ProfileFolder, fileName);
        bool exists = _host.VtankProfiles.ReadText(key) is not null;
        bool confirmed = exists
            && key.Equals(_pendingOverwriteKey, StringComparison.Ordinal)
            && _now - _pendingOverwriteAt <= OverwriteConfirmationSeconds;
        if (exists && !confirmed)
        {
            _pendingOverwriteKey = key;
            _pendingOverwriteAt = _now;
            return $"Equip profile '{fileName}' already exists at {key}; give the same "
                + $"command again within {OverwriteConfirmationSeconds:0} seconds to overwrite it.";
        }
        _pendingOverwriteKey = string.Empty;
        _pendingOverwriteAt = double.NegativeInfinity;

        var rules = new List<LootRule>();
        foreach (PluginInventoryItem item in _host.Automation.Items.CaptureOwnedItems())
        {
            if (!item.IsEquipped || item.ValidLocations == 0u)
                continue;
            var requirements = new List<VtankLootRequirement>
            {
                Requirement(StringMatchRequirement, "^" + Regex.Escape(item.Name) + "$", ProfileNameKey),
                Requirement(IntEqualsRequirement, item.ValidLocations, ProfileEquipableSlotsKey),
                Requirement(IntEqualsRequirement, item.Value, ValueProperty),
            };
            if (item.MaterialType != 0u)
                requirements.Add(Requirement(IntEqualsRequirement, item.MaterialType, MaterialProperty));
            rules.Add(new LootRule
            {
                Name = item.Name,
                Action = LootAction.Keep,
                HasImportedRequirements = true,
                VtankRequirements = requirements,
            });
        }
        _host.VtankProfiles.WriteText(key, MossTankLootProfileStore.SerializeRules(rules));
        return $"Equip profile created at {key} with {rules.Count} item(s).";
    }

    private IReadOnlyList<string> List(string pattern)
    {
        var lines = new List<string> { "Equip profiles:" };
        if (!_host.VtankProfiles.IsAvailable)
            return lines;
        string wanted = (pattern ?? string.Empty).Trim();
        foreach (string prefix in _store.ProfileFolderPrefixes(ProfileFolder))
        {
            foreach (string key in _host.VtankProfiles.List(prefix))
            {
                string file = key[(key.LastIndexOf('/') + 1)..];
                if (!file.EndsWith(ProfileExtension, StringComparison.OrdinalIgnoreCase))
                    continue;
                if (wanted.Length > 0
                    && !file.Contains(wanted, StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }
                lines.Add($" * {file} ({key[..^(file.Length + 1)]})");
            }
        }
        return lines;
    }

    // ---- helpers ------------------------------------------------------------

    private string ResolveKey(string? profileName) =>
        _store.ResolveProfileKey(ProfileFolder, FileName(profileName));

    private string FileName(string? profileName)
    {
        string requested = (profileName ?? string.Empty).Trim();
        if (requested.EndsWith(ProfileExtension, StringComparison.OrdinalIgnoreCase))
            requested = requested[..^ProfileExtension.Length];
        if (requested.Length == 0)
            requested = _host.Automation.Character.Name;
        return requested + ProfileExtension;
    }

    /// <summary>
    /// What can be worn or wielded at all: the classes the reference tool
    /// dresses with, and only when the item names a slot it fits.
    /// </summary>
    private IEnumerable<PluginInventoryItem> EquippableItems() =>
        _host.Automation.Items.CaptureOwnedItems().Where(static item =>
            item.ValidLocations != 0u
            && item.ObjectClass is PluginObjectClass.Armor
                or PluginObjectClass.Clothing
                or PluginObjectClass.Gem
                or PluginObjectClass.Jewelry
                or PluginObjectClass.MeleeWeapon
                or PluginObjectClass.MissileWeapon
                or PluginObjectClass.WandStaffOrb);

    private bool IsBusy() =>
        _host.Automation.Items.IsBusy || _host.Automation.Equipment.IsBusy;

    private bool IsEquipped(uint objectId) =>
        TryFindOwned(objectId, out PluginInventoryItem item) && item.IsEquipped;

    private bool TryFindOwned(uint objectId, out PluginInventoryItem item)
    {
        foreach (PluginInventoryItem owned in _host.Automation.Items.CaptureOwnedItems())
        {
            if (owned.ObjectId == objectId)
            {
                item = owned;
                return true;
            }
        }
        item = default;
        return false;
    }

    private bool HasAppraisalData(uint objectId) =>
        _host.Automation.Objects.TryGet(objectId, out PluginWorldObject known)
        && known.HasAppraisalData;

    private PluginItemProperties PropertiesOf(uint objectId) =>
        _host.Automation.Items.TryCaptureProperties(objectId, out PluginItemProperties properties)
            ? properties
            : EmptyProperties;

    private int Attempts(uint objectId) =>
        _identifyAttempts.TryGetValue(objectId, out int attempts) ? attempts : 0;

    private string Name(uint objectId) =>
        TryFindOwned(objectId, out PluginInventoryItem item) ? item.Name : objectId.ToString(CultureInfo.InvariantCulture);

    private static VtankLootRequirement Requirement(int type, string value, uint key) => new()
    {
        Type = type,
        Payload = value + "\r\n" + key.ToString(CultureInfo.InvariantCulture) + "\r\n",
    };

    private static VtankLootRequirement Requirement(int type, long value, uint key) =>
        Requirement(type, value.ToString(CultureInfo.InvariantCulture), key);

    private void Think(string text) =>
        Write(_settings.Think() ? $"You think, \"{text}\"" : text);

    private void Write(string text) => _host.Automation.Chat.PostSystemMessage(text);
}
