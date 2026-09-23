using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The switches a vendor run reads. Each is a live read, so an edit on the
/// settings page applies to the next tick rather than the next run.
/// </summary>
internal sealed class VendorTradeSettings
{
    public Func<bool> Enabled { get; init; } = static () => true;
    public Func<bool> EnableBuying { get; init; } = static () => true;
    public Func<bool> EnableSelling { get; init; } = static () => true;
    public Func<bool> TestMode { get; init; } = static () => false;
    public Func<bool> Think { get; init; } = static () => false;
    public Func<bool> ShowMerchantInfo { get; init; } = static () => true;
    public Func<bool> OnlyFromMainPack { get; init; } = static () => false;
    public Func<int> Tries { get; init; } = static () => 4;
    public Func<int> TriesTimeMilliseconds { get; init; } = static () => 5000;
}

/// <summary>
/// What is never sold whatever a rule says. Coin is not goods; what is worn
/// or wielded is in use; a pack or a foci is furniture; an inscribed,
/// tinkered, attuned or retained item is one somebody meant to keep; and
/// anything the macro profile itself names -- a wand, a weapon -- is what
/// the character fights with.
/// </summary>
internal static class VendorSellGuard
{
    private const uint InscriptionProperty = 7u;
    private const uint ScribeProperty = 8u;
    private const uint RetainedProperty = 88u;
    private const uint AttunedProperty = 114u;
    private const uint TinkerCountProperty = 171u;
    private const uint ImbuedEffectProperty = 179u;
    private const uint IconUnderlayProperty = 52u;
    private const uint RareUnderlay = 0x06005B0Cu;

    public static bool IsSafeToSell(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        uint playerObjectId,
        bool onlyFromMainPack,
        IReadOnlySet<uint> protectedObjectIds)
    {
        if (item.ObjectId == 0u || item.Value <= 0)
            return false;
        if (item.ObjectClass is PluginObjectClass.Money
            or PluginObjectClass.Container
            or PluginObjectClass.Foci)
        {
            return false;
        }
        if (item.IsEquipped || item.WielderObjectId != 0u)
            return false;
        if (protectedObjectIds.Contains(item.ObjectId))
            return false;
        if (item.NumTimesTinkered > 0)
            return false;
        if (onlyFromMainPack && item.ContainerObjectId != playerObjectId)
            return false;
        if (HasText(properties, InscriptionProperty) || HasText(properties, ScribeProperty))
            return false;
        if (properties.Ints is { } ints
            && ((ints.TryGetValue(AttunedProperty, out int attuned) && attuned > 0)
                || (ints.TryGetValue(TinkerCountProperty, out int tinkers) && tinkers > 0)
                || (ints.TryGetValue(ImbuedEffectProperty, out int imbued) && imbued != 0)))
        {
            return false;
        }
        if (properties.Bools is { } bools
            && bools.TryGetValue(RetainedProperty, out bool retained)
            && retained)
        {
            return false;
        }
        return properties.DataIds is not { } dataIds
            || !dataIds.TryGetValue(IconUnderlayProperty, out uint underlay)
            || underlay != RareUnderlay;
    }

    private static bool HasText(in PluginItemProperties properties, uint key) =>
        properties.Strings is { } strings
        && strings.TryGetValue(key, out string? text)
        && !string.IsNullOrWhiteSpace(text);
}

/// <summary>
/// Buys and sells at an open vendor by a loot profile. It owns the character
/// for the whole visit: a stack pass first, then rounds of buying and
/// selling planned by <see cref="VendorTradePlanner"/>, each committed
/// through the staged lists and waited on until the server answers, until a
/// round has nothing left to do. It also answers the <c>/vt vendor</c>
/// commands, which stage and commit by hand and open a vendor by name.
/// </summary>
internal sealed class VendorTradeController : IDisposable
{
    /// <summary>A run with no progress for this long gives up.</summary>
    private const double BailSeconds = 60d;

    /// <summary>
    /// How long each tick of a running visit holds navigation and item use.
    /// The window only has to outlive the gap between ticks; it is re-armed
    /// every tick and released the moment the run ends.
    /// </summary>
    private const double LockSeconds = 30d;

    /// <summary>
    /// How long a round waits for the coin and the goods to catch up with
    /// the server's answer before it plans on what it can see.
    /// </summary>
    private const double SettleTimeoutSeconds = 15d;

    /// <summary>How long a split is given to produce its stack.</summary>
    private const double SplitTimeoutSeconds = 10d;

    /// <summary>
    /// Coin the settle does not wait for: a vendor rounds its payouts, so
    /// the coin that arrives is never exactly the coin that was planned.
    /// </summary>
    private const long CoinTolerance = 50L;

    private const uint CoinWeenieClassId = 273u;
    private const string ProfileFolder = "autovendor";
    private const string ProfileExtension = ".utl";

    private static readonly PluginItemProperties EmptyProperties = new(
        new Dictionary<uint, int>(),
        new Dictionary<uint, long>(),
        new Dictionary<uint, bool>(),
        new Dictionary<uint, double>(),
        new Dictionary<uint, string>(),
        new Dictionary<uint, uint>(),
        new Dictionary<uint, uint>());

    private static readonly (uint Bit, string Name)[] ShopCategories =
    [
        (0x00000001u, "Weapons"), (0x00000002u, "Armor"), (0x00000004u, "Clothing"),
        (0x00000008u, "Jewelry"), (0x00000010u, "Miscellaneous"), (0x00000020u, "Food"),
        (0x00000080u, "Miscellaneous"), (0x00000100u, "Weapons"), (0x00000200u, "Containers"),
        (0x00000400u, "Miscellaneous"), (0x00000800u, "Gems"), (0x00001000u, "Spell Components"),
        (0x00002000u, "Books, Paper"), (0x00004000u, "Keys, Tools"), (0x00008000u, "Magic Items"),
        (0x00040000u, "Trade Notes"), (0x00080000u, "Mana Stones"), (0x00100000u, "Services"),
        (0x00400000u, "Cooking Items"), (0x00800000u, "Alchemical Items"), (0x01000000u, "Fletching Items"),
        (0x04000000u, "Alchemical Items"), (0x08000000u, "Fletching Items"), (0x20000000u, "Keys, Tools"),
    ];

    private enum Phase
    {
        StackCram,
        Plan,
        AwaitTransaction,
        Settle,
        AwaitSplit,
    }

    private readonly IPluginHost _host;
    private readonly UbSettingStore _store;
    private readonly List<LootRule> _rules = [];
    private readonly HashSet<uint> _pendingSell = [];
    private readonly Queue<PluginVendorTransaction> _completed = new();
    private VendorTradeSettings _settings = new();
    private Func<double, bool, bool> _stackCramPass = static (_, _) => false;
    private Func<bool> _routeOwnsVendorOpen = static () => false;
    private Func<IReadOnlySet<uint>> _protectedItems = static () => new HashSet<uint>();
    private ActionLockTable? _locks;

    private uint _pendingOpen;
    private bool _pendingClose;
    private Phase _phase;
    private bool _firstStackPass;
    private double _sinceProgress;
    private double _sinceCompletion;
    private long _expectedCoin;
    private long _coinBefore;
    private long _splitRevision;
    private uint _splitObjectId;
    private double _splitWaited;
    private string _vendorName = string.Empty;
    private uint _vendorObjectId;
    private uint _reportedVendor;

    // The open-by-name command's own little run.
    private uint _openTarget;
    private string _openName = string.Empty;
    private int _openTries;
    private double _sinceOpenTry;

    // A staged sell that had to split a stack first.
    private uint _stageSplitObjectId;
    private string _stageSplitName = string.Empty;
    private int _stageSplitNeeded;
    private int _stageSplitAsked;
    private long _stageSplitRevision;

    public VendorTradeController(IPluginHost host, UbSettingStore store)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _store = store ?? throw new ArgumentNullException(nameof(store));
        IVendorAutomation vendor = _host.Automation.Vendor;
        vendor.Opened += OnOpened;
        vendor.Closed += OnClosed;
        vendor.TransactionCompleted += OnTransactionCompleted;
    }

    public bool IsRunning { get; private set; }

    public string Status { get; private set; } = "Vendor run idle.";

    public void BindSettings(VendorTradeSettings settings) =>
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));

    /// <summary>
    /// The stack-and-cram pass a run asks for before it plans: called every
    /// tick with the elapsed time and whether this is the first call of the
    /// pass, answering true while it still has work in flight.
    /// </summary>
    public void BindStackCramPass(Func<double, bool, bool> pass) =>
        _stackCramPass = pass ?? throw new ArgumentNullException(nameof(pass));

    /// <summary>
    /// Whether the route is the owner of opening vendors right now, in which
    /// case the open-by-name command stands down rather than fight it.
    /// </summary>
    public void BindRouteOwnsVendorOpen(Func<bool> routeOwnsVendorOpen) =>
        _routeOwnsVendorOpen = routeOwnsVendorOpen
            ?? throw new ArgumentNullException(nameof(routeOwnsVendorOpen));

    /// <summary>
    /// The shared lock table a running visit holds navigation and item use
    /// on, so the route mover does not walk the character off mid-visit.
    /// </summary>
    public void BindActionLocks(ActionLockTable locks) =>
        _locks = locks ?? throw new ArgumentNullException(nameof(locks));

    /// <summary>The items the macro profile uses, which are never sold.</summary>
    public void BindProtectedItems(Func<IReadOnlySet<uint>> protectedItems) =>
        _protectedItems = protectedItems
            ?? throw new ArgumentNullException(nameof(protectedItems));

    /// <summary>
    /// Starts a run against the open vendor. The profile is the vendor's own
    /// name unless one is given, looked for the tiered way; in test mode the
    /// run only says what it would do.
    /// </summary>
    public bool TryStart(string? profileName)
    {
        if (!_host.Automation.IsAvailable)
        {
            Status = "Vendor run needs a live session.";
            return false;
        }
        IVendorAutomation vendor = _host.Automation.Vendor;
        if (!vendor.IsOpen || vendor.VendorObjectId == 0u)
        {
            Status = "Vendor run cannot start: no vendor open.";
            return false;
        }
        if (IsRunning)
            Stop(null);

        _vendorObjectId = vendor.VendorObjectId;
        _vendorName = vendor.VendorName;
        string requested = (profileName ?? string.Empty).Trim();
        if (requested.EndsWith(ProfileExtension, StringComparison.OrdinalIgnoreCase))
            requested = requested[..^ProfileExtension.Length];
        string fileName = (requested.Length > 0 ? requested : _vendorName) + ProfileExtension;
        string key = _store.ResolveProfileKey(ProfileFolder, fileName);
        string? text = _host.VtankProfiles.IsAvailable ? _host.VtankProfiles.ReadText(key) : null;
        if (text is null)
        {
            Status = $"No vendor profile exists: {key}";
            return false;
        }
        if (!MossTankLootProfileStore.TryParseRules(text, _rules))
        {
            Status = $"Vendor profile could not be read: {key}";
            return false;
        }

        if (_settings.TestMode())
        {
            ReportTestMode();
            Status = $"Vendor run test mode reported for {_vendorName}.";
            return true;
        }

        _phase = Phase.StackCram;
        _firstStackPass = true;
        _sinceProgress = 0d;
        _pendingSell.Clear();
        _completed.Clear();
        _expectedCoin = 0L;
        IsRunning = true;
        Status = $"Vendor run started at {_vendorName} with {_rules.Count} rule(s).";
        return true;
    }

    /// <summary>Stops a run on the player's say-so. False when none was running.</summary>
    public bool StopRequested()
    {
        if (!IsRunning)
        {
            Status = "Vendor run is not running.";
            return false;
        }
        Stop("Vendor run cancelled: " + _vendorName);
        return true;
    }

    /// <summary>
    /// Advances the run and the open-by-name command. True while either owns
    /// the character.
    /// </summary>
    public bool Tick(double elapsedSeconds, bool canAct)
    {
        double step = Math.Max(0d, elapsedSeconds);
        DrainEvents();
        ObserveStageSplit();
        bool opening = TickOpen(step, canAct);
        if (!IsRunning)
            return opening;

        if (!_host.Automation.IsAvailable || !_host.Automation.Vendor.IsOpen)
        {
            Stop("Vendor run stopped: the vendor closed.");
            return opening;
        }
        if (!_settings.Enabled())
        {
            Stop("Vendor run stopped: disabled.");
            return opening;
        }
        // Re-armed every tick, so a long visit is held for as long as it
        // lives and not just for the first window.
        _locks?.Arm(ActionLockKind.Navigation, LockSeconds);
        _locks?.Arm(ActionLockKind.ItemUse, LockSeconds);
        _sinceProgress += step;
        if (_sinceProgress > BailSeconds)
        {
            Stop("Vendor run bailed: timeout expired.");
            return opening;
        }
        if (!canAct)
            return true;

        switch (_phase)
        {
            case Phase.StackCram:
                bool busy = _stackCramPass(step, _firstStackPass);
                _firstStackPass = false;
                if (!busy)
                {
                    _phase = Phase.Plan;
                    _sinceProgress = 0d;
                }
                return true;
            case Phase.Plan:
                PlanRound();
                return IsRunning;
            case Phase.AwaitTransaction:
                return true;
            case Phase.Settle:
                TickSettle(step);
                return true;
            case Phase.AwaitSplit:
                TickSplit(step);
                return IsRunning;
            default:
                return true;
        }
    }

    /// <summary>
    /// The macro started: the route owns vendor opening from now on, so an
    /// open-by-name begun before it is dropped rather than sent beside it.
    /// </summary>
    public void OnMacroStarted()
    {
        if (_openTarget == 0u)
            return;
        _openTarget = 0u;
        _openTries = 0;
        Status = $"Vendor open dropped: the macro started and the route owns opening {_openName}.";
    }

    public void Reset()
    {
        if (IsRunning)
        {
            _locks?.Release(ActionLockKind.Navigation);
            _locks?.Release(ActionLockKind.ItemUse);
        }
        IsRunning = false;
        _phase = Phase.StackCram;
        _rules.Clear();
        _pendingSell.Clear();
        _completed.Clear();
        _pendingOpen = 0u;
        _pendingClose = false;
        _openTarget = 0u;
        _openTries = 0;
        _stageSplitObjectId = 0u;
        _reportedVendor = 0u;
        Status = "Vendor run idle.";
    }

    public void Dispose()
    {
        IVendorAutomation vendor = _host.Automation.Vendor;
        vendor.Opened -= OnOpened;
        vendor.Closed -= OnClosed;
        vendor.TransactionCompleted -= OnTransactionCompleted;
    }

    /// <summary>
    /// The <c>/vt vendor</c> family: <c>open[p] &lt;name&gt;</c>,
    /// <c>opencancel</c>, <c>buyall</c>, <c>sellall</c>, <c>clearbuy</c>,
    /// <c>clearsell</c>, <c>addbuy[p] [n] &lt;item&gt;</c> and
    /// <c>addsell[p] [n] &lt;item&gt;</c>. Returns the lines to say back.
    /// </summary>
    public IReadOnlyList<string> VendorCommand(string arguments)
    {
        const string usage =
            "Syntax: /vt vendor open[p] <name> | opencancel | buyall | sellall | "
            + "clearbuy | clearsell | addbuy[p] [count] <item> | addsell[p] [count] <item>";
        string text = (arguments ?? string.Empty).Trim();
        int space = text.IndexOf(' ');
        string verb = (space < 0 ? text : text[..space]).ToLowerInvariant();
        string rest = space < 0 ? string.Empty : text[(space + 1)..].Trim();
        bool partial = verb.EndsWith('p') && verb is "openp" or "addbuyp" or "addsellp";
        IVendorAutomation vendor = _host.Automation.Vendor;

        switch (verb)
        {
            case "buy":
            case "buyall":
                return [Describe("Buy", vendor.BuyAll())];
            case "sell":
            case "sellall":
                return [Describe("Sell", vendor.SellAll())];
            case "clearbuy":
                return [Describe("Clear buy list", vendor.ClearBuyList())];
            case "clearsell":
                return [Describe("Clear sell list", vendor.ClearSellList())];
            case "open":
            case "openp":
                return [BeginOpen(rest, partial)];
            case "opencancel":
                _openTarget = 0u;
                _openTries = 0;
                return ["Vendor open cancelled."];
            case "addbuy":
            case "addbuyp":
                return [AddBuyByName(rest, partial)];
            case "addsell":
            case "addsellp":
                return [AddSellByName(rest, partial)];
            default:
                return [usage];
        }
    }

    private void OnOpened(uint vendorObjectId)
    {
        _pendingOpen = vendorObjectId;
        _pendingClose = false;
    }

    private void OnClosed()
    {
        _pendingClose = true;
        _pendingOpen = 0u;
    }

    private void OnTransactionCompleted(PluginVendorTransaction transaction) =>
        _completed.Enqueue(transaction);

    private void DrainEvents()
    {
        if (_pendingClose)
        {
            _pendingClose = false;
            if (IsRunning)
                Stop("Vendor run stopped: the vendor closed.");
        }
        if (_pendingOpen != 0u)
        {
            uint opened = _pendingOpen;
            _pendingOpen = 0u;
            if (_openTarget != 0u && opened == _openTarget)
            {
                _openTarget = 0u;
                _openTries = 0;
                Status = $"Vendor opened: {_openName}.";
            }
            ReportMerchant(opened);
            if (!IsRunning && _settings.Enabled() && _host.Automation.IsAvailable)
                TryStart(null);
        }
        while (_completed.Count > 0)
        {
            PluginVendorTransaction transaction = _completed.Dequeue();
            if (!IsRunning || _phase != Phase.AwaitTransaction)
                continue;
            if (!transaction.Success)
            {
                Stop($"Vendor run stopped: {transaction.Kind} failed"
                    + (string.IsNullOrWhiteSpace(transaction.Notice) ? "." : $": {transaction.Notice}"));
                continue;
            }
            _phase = Phase.Settle;
            _sinceCompletion = 0d;
            _sinceProgress = 0d;
        }
    }

    private void ReportMerchant(uint vendorObjectId)
    {
        if (!_settings.ShowMerchantInfo() || _reportedVendor == vendorObjectId)
            return;
        _reportedVendor = vendorObjectId;
        IVendorAutomation vendor = _host.Automation.Vendor;
        PluginVendorProfile profile = vendor.Profile;
        string categories = string.Join(", ", ShopCategories
            .Where(c => (profile.DealsInItemTypes & c.Bit) != 0u)
            .Select(c => c.Name)
            .Distinct());
        string ceiling = profile.MaximumValue == PluginVendorProfile.NoValueLimit
            ? "no limit"
            : profile.MaximumValue.ToString("n0", CultureInfo.InvariantCulture);
        bool enabled = _settings.Enabled();
        Write(string.Create(
            CultureInfo.InvariantCulture,
            $"{vendor.VendorName}[0x{vendorObjectId:X8}]: pays {profile.BuyRate * 100f:0}% of value, "
            + $"deals in {(categories.Length > 0 ? categories : "nothing")}, max value {ceiling}, "
            + $"Buy: {(enabled && _settings.EnableBuying() ? "Enabled" : "Disabled")} "
            + $"Sell: {(enabled && _settings.EnableSelling() ? "Enabled" : "Disabled")}"));
    }

    private void PlanRound()
    {
        IVendorAutomation vendor = _host.Automation.Vendor;
        if (vendor.IsBusy || _host.Automation.Items.IsBusy)
            return;
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        PluginVendorProfile profile = vendor.Profile;
        VendorPurse purse = Purse(owned, profile);
        IReadOnlyList<VendorBuyCandidate> buys = BuyCandidates(vendor, owned, profile);
        IReadOnlyList<VendorSellCandidate> sells = SellCandidates(owned, profile);
        VendorTradePlan plan = VendorTradePlanner.Plan(buys, sells, profile, purse, owned);

        vendor.ClearBuyList();
        vendor.ClearSellList();
        _pendingSell.Clear();

        if (plan.Fatal is not null)
        {
            Stop("Vendor run stopped: " + plan.Fatal);
            return;
        }
        if (plan.Split is { } split)
        {
            BeginSplit(split);
            return;
        }
        if (plan.Buys.Count > 0)
        {
            foreach (VendorBuyLine line in plan.Buys)
                vendor.AddToBuyList(line.TemplateObjectId, line.Count);
            PluginVendorCommandResult result = vendor.BuyAll();
            if (!result.Accepted)
            {
                if (result.Status == PluginVendorCommandStatus.Busy)
                    return;
                Stop($"Vendor run stopped: buy refused ({result.Status}{Notice(result.Notice)}).");
                return;
            }
            _coinBefore = purse.CoinOnHand;
            _expectedCoin = -plan.BuyCost;
            _phase = Phase.AwaitTransaction;
            _sinceProgress = 0d;
            Status = string.Create(
                CultureInfo.InvariantCulture,
                $"Buying {string.Join(", ", plan.Buys.Select(l => $"{l.Count} {l.Name}"))} for {plan.BuyCost:n0}.");
            return;
        }
        if (plan.Sells.Count > 0)
        {
            foreach (VendorSellLine line in plan.Sells)
            {
                vendor.AddToSellList(line.ObjectId);
                _pendingSell.Add(line.ObjectId);
            }
            PluginVendorCommandResult result = vendor.SellAll();
            if (!result.Accepted)
            {
                _pendingSell.Clear();
                if (result.Status == PluginVendorCommandStatus.Busy)
                    return;
                Stop($"Vendor run stopped: sell refused ({result.Status}{Notice(result.Notice)}).");
                return;
            }
            _coinBefore = purse.CoinOnHand;
            _expectedCoin = plan.SellProceeds;
            _phase = Phase.AwaitTransaction;
            _sinceProgress = 0d;
            Status = string.Create(
                CultureInfo.InvariantCulture,
                $"Selling {plan.Sells.Count} item(s) for {plan.SellProceeds:n0}.");
            return;
        }
        Stop("Vendor run finished: " + _vendorName, think: true);
    }

    private void TickSettle(double step)
    {
        _sinceCompletion += step;
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        long coinNow = CoinOnHand(owned, _host.Automation.Vendor.Profile);
        long remaining = _expectedCoin - (coinNow - _coinBefore);
        bool soldGone = !owned.Any(item => _pendingSell.Contains(item.ObjectId));
        if ((Math.Abs(remaining) < CoinTolerance && soldGone)
            || _sinceCompletion >= SettleTimeoutSeconds)
        {
            _pendingSell.Clear();
            _expectedCoin = 0L;
            _phase = Phase.Plan;
            _sinceProgress = 0d;
        }
    }

    private void BeginSplit(in VendorSplitRequest split)
    {
        _splitRevision = _host.Automation.Items.LastInventoryCompletion.Revision;
        PluginItemCommandResult result = _host.Automation.Items.MoveToContainer(
            split.ObjectId,
            _host.Automation.Character.ObjectId,
            (uint)split.Amount,
            0);
        if (!result.Accepted)
        {
            if (result.Status == PluginItemCommandStatus.Busy)
                return;
            Stop($"Vendor run stopped: could not split {split.Name} ({result.Status}{Notice(result.Notice)}).");
            return;
        }
        _splitObjectId = split.ObjectId;
        _splitWaited = 0d;
        _phase = Phase.AwaitSplit;
        Status = $"Splitting {split.Amount} off {split.Name} to sell.";
    }

    private void TickSplit(double step)
    {
        _splitWaited += step;
        PluginInventoryCompletion completion = _host.Automation.Items.LastInventoryCompletion;
        if (completion.Revision > _splitRevision && completion.SourceObjectId == _splitObjectId)
        {
            if (!completion.IsSuccess)
            {
                Stop($"Vendor run stopped: the split failed (0x{completion.WeenieError:X}).");
                return;
            }
            _phase = Phase.Plan;
            _sinceProgress = 0d;
            return;
        }
        if (_splitWaited >= SplitTimeoutSeconds)
            Stop("Vendor run stopped: the split was not answered.");
    }

    private IReadOnlyList<VendorBuyCandidate> BuyCandidates(
        IVendorAutomation vendor,
        IReadOnlyList<PluginInventoryItem> owned,
        in PluginVendorProfile profile)
    {
        if (!_settings.EnableBuying())
            return [];
        var candidates = new List<VendorBuyCandidate>();
        bool buyingContainers = false;
        foreach (PluginVendorItem listing in vendor.Items)
        {
            PluginInventoryItem facade = Facade(vendor, listing, out PluginItemProperties properties);
            LootDecision? decision = LootRuleEngine.Decide(facade, properties, _rules, owned, host: _host);
            if (decision is not { } decided)
                continue;
            if (decided.Action == LootAction.KeepUpTo)
            {
                int limit = Math.Max(0, _rules[decided.RuleIndex].KeepCount);
                if (listing.ObjectClass == PluginObjectClass.Container)
                {
                    int packs = owned.Count(static item => item.ObjectClass == PluginObjectClass.Container);
                    if (buyingContainers || limit <= packs)
                        continue;
                    buyingContainers = true;
                    candidates.Add(new VendorBuyCandidate(listing, limit - packs, decided.RuleName));
                    continue;
                }
                int held = owned
                    .Where(item => string.Equals(item.Name, listing.Name, StringComparison.OrdinalIgnoreCase))
                    .Sum(static item => Math.Max(1, item.StackSize));
                if (limit > held)
                    candidates.Add(new VendorBuyCandidate(listing, limit - held, decided.RuleName));
            }
            else if (decided.Action == LootAction.Keep)
            {
                candidates.Add(new VendorBuyCandidate(listing, int.MaxValue, decided.RuleName));
            }
        }
        _ = profile;
        return VendorTradePlanner.OrderBuys(candidates);
    }

    private IReadOnlyList<VendorSellCandidate> SellCandidates(
        IReadOnlyList<PluginInventoryItem> owned,
        in PluginVendorProfile profile)
    {
        if (!_settings.EnableSelling() || profile.UsesAlternateCurrency)
            return [];
        uint player = _host.Automation.Character.ObjectId;
        bool mainPackOnly = _settings.OnlyFromMainPack();
        IReadOnlySet<uint> protectedItems = _protectedItems();
        var candidates = new List<VendorSellCandidate>();
        foreach (PluginInventoryItem item in owned)
        {
            PluginItemProperties properties = _host.Automation.Items
                .TryCaptureProperties(item.ObjectId, out PluginItemProperties value)
                    ? value
                    : EmptyProperties;
            if (!VendorSellGuard.IsSafeToSell(item, properties, player, mainPackOnly, protectedItems))
                continue;
            LootDecision? decision = LootRuleEngine.Decide(item, properties, _rules, owned, host: _host);
            if (decision is not { Action: LootAction.Sell } decided)
                continue;
            if (!VendorTradePlanner.VendorBuys(item, profile))
                continue;
            candidates.Add(new VendorSellCandidate(item, decided.RuleName));
        }
        return VendorTradePlanner.OrderSells(candidates, profile);
    }

    /// <summary>
    /// A vendor listing in the shape the rules read, so one rule engine
    /// decides both what to buy and what to sell.
    /// </summary>
    private static PluginInventoryItem Facade(
        IVendorAutomation vendor,
        in PluginVendorItem listing,
        out PluginItemProperties properties)
    {
        properties = vendor.TryCaptureProperties(listing.TemplateObjectId, out PluginItemProperties value)
            ? value
            : EmptyProperties;
        int stack = Math.Max(1, listing.StackSize);
        int unitValue = properties.Ints is { } ints && ints.TryGetValue(19u, out int listed)
            ? listed / stack
            : listing.UnitPrice;
        return new PluginInventoryItem(
            listing.TemplateObjectId, listing.WeenieClassId, listing.Name, listing.ItemType,
            0u, 0u, 0u, 0u, 0u, 0u, 0u, stack, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = unitValue * stack,
            MaximumStackSize = Math.Max(1, listing.MaxStackSize),
            ObjectClass = listing.ObjectClass,
        };
    }

    private VendorPurse Purse(IReadOnlyList<PluginInventoryItem> owned, in PluginVendorProfile profile)
    {
        int coinStack = owned
            .Where(static item => item.WeenieClassId == CoinWeenieClassId)
            .Select(static item => item.MaximumStackSize)
            .FirstOrDefault(static size => size > 1);
        return new VendorPurse(
            CoinOnHand(owned, profile),
            coinStack > 1 ? coinStack : VendorTradePlanner.DefaultCoinStackSize,
            Math.Max(0, _host.Automation.Character.MainPackFreeSlots));
    }

    private static long CoinOnHand(IReadOnlyList<PluginInventoryItem> owned, in PluginVendorProfile profile)
    {
        bool alternate = profile.UsesAlternateCurrency;
        uint currency = alternate ? profile.AlternateCurrencyWeenieClassId : CoinWeenieClassId;
        // Only the currency itself pays: pyreal coins, or the vendor's own
        // alternate currency. Peas and trade notes are money-class items the
        // server does not count as coin; counting their stacks planned buys
        // it refused outright.
        return owned
            .Where(item => item.WeenieClassId == currency)
            .Sum(static item => (long)Math.Max(1, item.StackSize));
    }

    private void ReportTestMode()
    {
        IVendorAutomation vendor = _host.Automation.Vendor;
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        PluginVendorProfile profile = vendor.Profile;
        var report = new StringBuilder("Vendor run TEST MODE\nBuy Items:\n");
        int lines = 0;
        foreach (VendorBuyCandidate buy in BuyCandidates(vendor, owned, profile))
        {
            string category = ShopCategories
                .FirstOrDefault(c => (buy.Item.ItemType & c.Bit) != 0u).Name
                ?? string.Create(CultureInfo.InvariantCulture, $"Unknown Category 0x{buy.Item.ItemType:X8}");
            string count = buy.Wanted == int.MaxValue ? "all" : buy.Wanted.ToString(CultureInfo.InvariantCulture);
            report.Append(string.Create(
                CultureInfo.InvariantCulture,
                $"  {category} -> {buy.Item.Name} * {count} at {buy.Item.UnitPrice:n0} - {buy.RuleName}\n"));
            lines++;
        }
        if (lines == 0)
            report.Append("  (Nothing)\n");
        report.Append("Sell Items:\n");
        lines = 0;
        foreach (VendorSellCandidate sell in SellCandidates(owned, profile)
            .OrderBy(static s => s.Item.ContainerSlot))
        {
            long proceeds = VendorTradePlanner.PayoutPerUnit(sell.Item, profile)
                * Math.Max(1, sell.Item.StackSize);
            report.Append(string.Create(
                CultureInfo.InvariantCulture,
                $"  {sell.Item.Name} x{Math.Max(1, sell.Item.StackSize)} for {proceeds:n0} - {sell.RuleName}\n"));
            lines++;
        }
        if (lines == 0)
            report.Append("  (Nothing)\n");
        Write(report.ToString().TrimEnd());
    }

    private string BeginOpen(string name, bool partial)
    {
        if (!_host.Automation.IsAvailable)
            return "Vendor open needs a live session.";
        if (_routeOwnsVendorOpen())
        {
            return "The route owns vendor opening while the macro runs; "
                + "use an open-vendor waypoint or stop the macro first.";
        }
        string wanted = name.Trim();
        PluginWorldObject vendor = _host.Automation.Objects.CaptureObjects()
            .Where(static obj => obj.ObjectClass == PluginObjectClass.Vendor)
            .Where(obj => partial
                ? obj.Name.Contains(wanted, StringComparison.OrdinalIgnoreCase)
                : obj.Name.Equals(wanted, StringComparison.OrdinalIgnoreCase))
            .OrderBy(static obj => obj.ObjectId)
            .FirstOrDefault();
        if (vendor.ObjectId == 0u)
        {
            Think("Vendor open failed: no vendor "
                + (partial ? "partially " : string.Empty) + $"named '{wanted}' in sight.");
            return Status = $"Vendor not found: {wanted}.";
        }
        if (_host.Automation.Items.ActiveVendorObjectId == vendor.ObjectId)
            return $"Vendor is already open: {vendor.Name}.";
        _openTarget = vendor.ObjectId;
        _openName = vendor.Name;
        _openTries = 0;
        // The first try goes out on the next tick, not after a full wait.
        _sinceOpenTry = double.MaxValue;
        return Status = $"Opening vendor: {vendor.Name}.";
    }

    private bool TickOpen(double step, bool canAct)
    {
        if (_openTarget == 0u)
            return false;
        if (_host.Automation.Vendor.IsOpen
            && _host.Automation.Vendor.VendorObjectId == _openTarget)
        {
            _openTarget = 0u;
            _openTries = 0;
            Status = $"Vendor opened: {_openName}.";
            return false;
        }
        if (!canAct)
            return true;
        if (_sinceOpenTry < double.MaxValue)
            _sinceOpenTry += step;
        double wait = Math.Max(0, _settings.TriesTimeMilliseconds()) / 1000d;
        if (_sinceOpenTry < wait)
            return true;
        if (_openTries >= Math.Max(1, _settings.Tries()))
        {
            Think($"Vendor open failed: {_openName} did not open after {_openTries} tries.");
            _openTarget = 0u;
            _openTries = 0;
            return false;
        }
        _openTries++;
        _sinceOpenTry = 0d;
        PluginItemCommandResult result = _host.Automation.Items.Use(_openTarget);
        Status = result.Accepted
            ? $"Using vendor {_openName} (try {_openTries})."
            : $"Waiting to use vendor {_openName}: {result.Status}.";
        return true;
    }

    private string AddBuyByName(string arguments, bool partial)
    {
        IVendorAutomation vendor = _host.Automation.Vendor;
        if (!vendor.IsOpen)
            return "addbuy: No vendor open.";
        (int count, string name) = SplitCount(arguments);
        if (name.Length == 0)
            return "Syntax: /vt vendor addbuy[p] [count] <item>";
        foreach (PluginVendorItem listing in vendor.Items)
        {
            if (!NameMatches(listing.Name, name, partial))
                continue;
            PluginVendorCommandResult result = vendor.AddToBuyList(listing.TemplateObjectId, count);
            return result.Accepted
                ? $"Added item to buy list: {listing.Name} * {count}"
                : $"addbuy: {result.Status}{Notice(result.Notice)}";
        }
        return $"addbuy: Unable to find item {(partial ? "partially " : string.Empty)}named '{name}' in the vendor's list.";
    }

    /// <summary>
    /// Stages whole stacks until the count is met; when the count runs out
    /// part way through the only stack left, that stack is split first and
    /// the piece is staged once it exists.
    /// </summary>
    private string AddSellByName(string arguments, bool partial)
    {
        if (!_host.Automation.Vendor.IsOpen)
            return "addsell: No vendor open.";
        (int count, string name) = SplitCount(arguments);
        if (name.Length == 0)
            return "Syntax: /vt vendor addsell[p] [count] <item>";
        uint player = _host.Automation.Character.ObjectId;
        IReadOnlySet<uint> protectedItems = _protectedItems();
        var matches = new List<PluginInventoryItem>();
        foreach (PluginInventoryItem item in _host.Automation.Items.CaptureOwnedItems())
        {
            if (!NameMatches(item.Name, name, partial))
                continue;
            PluginItemProperties properties = _host.Automation.Items
                .TryCaptureProperties(item.ObjectId, out PluginItemProperties value)
                    ? value
                    : EmptyProperties;
            if (VendorSellGuard.IsSafeToSell(item, properties, player, false, protectedItems))
                matches.Add(item);
        }
        if (matches.Count == 0)
            return $"addsell: Unable to find item {(partial ? "partially " : string.Empty)}named '{name}' in inventory.";

        int needed = count;
        PluginInventoryItem? oversized = null;
        foreach (PluginInventoryItem item in matches)
        {
            int stack = Math.Max(1, item.StackSize);
            if (stack <= needed)
            {
                _host.Automation.Vendor.AddToSellList(item.ObjectId);
                needed -= stack;
            }
            else if (oversized is null || Math.Max(1, oversized.Value.StackSize) > stack)
            {
                oversized = item;
            }
            if (needed == 0)
                break;
        }
        string shown = matches[0].Name;
        if (needed == 0)
            return $"Added item to sell list: {shown} * {count}";
        if (oversized is not { } source)
            return $"Added item to sell list: {shown} * {count - needed}, but was missing {needed} item(s).";

        long revision = _host.Automation.Items.LastInventoryCompletion.Revision;
        PluginItemCommandResult split = _host.Automation.Items.MoveToContainer(
            source.ObjectId, player, (uint)needed, 0);
        if (!split.Accepted)
            return $"addsell: could not split {source.Name}: {split.Status}{Notice(split.Notice)}";
        _stageSplitObjectId = source.ObjectId;
        _stageSplitName = source.Name;
        _stageSplitNeeded = needed;
        _stageSplitAsked = count;
        _stageSplitRevision = revision;
        return $"Splitting {needed} off {source.Name} to complete the sell list.";
    }

    private void ObserveStageSplit()
    {
        if (_stageSplitObjectId == 0u)
            return;
        PluginInventoryCompletion completion = _host.Automation.Items.LastInventoryCompletion;
        if (completion.Revision <= _stageSplitRevision || completion.SourceObjectId != _stageSplitObjectId)
            return;
        _stageSplitObjectId = 0u;
        if (!completion.IsSuccess)
        {
            Write($"addsell: the split of {_stageSplitName} failed (0x{completion.WeenieError:X}).");
            return;
        }
        PluginInventoryItem piece = _host.Automation.Items.CaptureOwnedItems()
            .FirstOrDefault(item => item.StackSize == _stageSplitNeeded
                && string.Equals(item.Name, _stageSplitName, StringComparison.OrdinalIgnoreCase));
        if (piece.ObjectId == 0u)
        {
            Write($"Added item to sell list: {_stageSplitName} * {_stageSplitAsked - _stageSplitNeeded}, "
                + $"but was missing {_stageSplitNeeded} item(s).");
            return;
        }
        _host.Automation.Vendor.AddToSellList(piece.ObjectId);
        Write($"Added item to sell list: {_stageSplitName} * {_stageSplitAsked}");
    }

    private static (int Count, string Name) SplitCount(string arguments)
    {
        string text = arguments.Trim();
        int space = text.IndexOf(' ');
        if (space > 0
            && int.TryParse(text[..space], NumberStyles.Integer, CultureInfo.InvariantCulture, out int count)
            && count > 0)
        {
            return (count, text[(space + 1)..].Trim());
        }
        return (1, text);
    }

    private static bool NameMatches(string name, string wanted, bool partial) => partial
        ? name.Contains(wanted, StringComparison.OrdinalIgnoreCase)
        : name.Equals(wanted, StringComparison.OrdinalIgnoreCase);

    private static string Describe(string what, in PluginVendorCommandResult result) =>
        result.Accepted ? $"{what}: sent." : $"{what}: {result.Status}{Notice(result.Notice)}";

    private static string Notice(string? notice) =>
        string.IsNullOrWhiteSpace(notice) ? string.Empty : ", " + notice;

    private void Stop(string? status, bool think = false)
    {
        IsRunning = false;
        _pendingSell.Clear();
        _completed.Clear();
        _phase = Phase.StackCram;
        _locks?.Release(ActionLockKind.Navigation);
        _locks?.Release(ActionLockKind.ItemUse);
        if (status is null)
            return;
        Status = status;
        if (think)
            Think(status);
        else
            Write(status);
    }

    private void Think(string text)
    {
        // The client's own "You think" line is what a meta's chat trigger
        // reads, so a run that is asked to think says it that way.
        Write(_settings.Think() ? $"You think, \"{text}\"" : text);
    }

    private void Write(string text) =>
        _host.Automation.Chat.PostSystemMessage(text.StartsWith("You think", StringComparison.Ordinal)
            ? text
            : "[MossTank] " + text);
}
