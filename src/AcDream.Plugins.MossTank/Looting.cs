using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum LootAction
{
    NoLoot,
    Keep,
    Salvage,
    Sell,
    Read,
    User1,
    User2,
    User3,
    User4,
    User5,
    KeepUpTo,
    ManaStone,
    ManaTank,
}

internal sealed class LootRule
{
    private string _expression = "*";
    private string? _compiledSource;
    private LootRuleExpression? _compiled;

    public string Name { get; set; } = "Rule";
    public string Expression
    {
        get => _expression;
        set => _expression = string.IsNullOrWhiteSpace(value) ? "*" : value.Trim();
    }
    public LootAction Action { get; set; } = LootAction.Keep;
    public int KeepCount { get; set; } = 1;
    public int Priority { get; set; }
    public string CustomExpression { get; set; } = string.Empty;
    public List<VtankLootRequirement> VtankRequirements { get; set; } = [];
    public bool HasImportedRequirements { get; set; }

    public bool IsMatch(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IPluginHost? host,
        out string? error)
    {
        if (VtankRequirements.Count > 0)
        {
            return VtankLootRequirementEvaluator.IsMatch(
                VtankRequirements,
                item,
                properties,
                host,
                out error);
        }
        try
        {
            if (_compiled is null
                || !string.Equals(
                    _compiledSource,
                    Expression,
                    StringComparison.Ordinal))
            {
                _compiled = LootRuleExpression.Compile(Expression);
                _compiledSource = Expression;
            }
            error = null;
            return _compiled.IsMatch(item, properties);
        }
        catch (FormatException failure)
        {
            error = failure.Message;
            return false;
        }
    }

    /// <summary>
    /// Answers "can this rule decide the item without appraisal data?" and,
    /// when it can, whether it matches. A rule is only a decided match when
    /// every requirement decided; a single definite non-match ends it, and a
    /// single undecidable requirement leaves the whole rule open.
    /// </summary>
    public void EarlyMatch(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IPluginHost? host,
        out bool hasDecision,
        out bool isMatch)
    {
        if (VtankRequirements.Count == 0)
        {
            // A catch-all needs nothing to decide. Anything written in
            // MossTank's own expression language stays open: that language
            // has no per-clause appraisal-dependency model.
            hasDecision = IsCatchAll;
            isMatch = hasDecision;
            return;
        }

        bool anyOpen = false;
        foreach (VtankLootRequirement requirement in VtankRequirements)
        {
            VtankLootRequirementEvaluator.EarlyMatch(
                requirement,
                item,
                properties,
                host,
                out bool requirementDecided,
                out bool requirementMatched);
            if (requirementDecided && !requirementMatched)
            {
                hasDecision = true;
                isMatch = false;
                return;
            }
            if (!requirementDecided)
                anyOpen = true;
        }
        hasDecision = !anyOpen;
        isMatch = !anyOpen;
    }

    private bool IsCatchAll => Expression is "*"
        || Expression.Equals("DEFAULT", StringComparison.OrdinalIgnoreCase);
}

internal sealed class LootSettings
{
    // The shipped VTank default settings profile's own values.
    public bool ProfileActive { get; set; } = true;
    public bool Enabled { get; set; }
    public string ExternalClassifierId { get; set; } = string.Empty;
    public bool PriorityBoost { get; set; }
    public bool LootAllCorpses { get; set; }
    public bool LootFellowCorpses { get; set; }
    public bool LootOnlyRareCorpses { get; set; }
    public bool ReadUnknownScrolls { get; set; } = true;
    public bool CombineSalvage { get; set; } = true;
    public int ManaStoneLootCount { get; set; } = 4;
    public int ManaTankMinimumMana { get; set; } = 1000;
    public double CorpseApproachRange { get; set; } = 40d;
    public double CorpseMinimumApproachRange { get; set; } = 3.36d;
    public double CorpseOpenTimeoutSeconds { get; set; } = 1.5d;
    public double CorpseItemAppearanceTimeoutSeconds { get; set; } = 6d;
    public double CorpseItemIdentifyTimeoutSeconds { get; set; } = 60d;
    public int BlacklistCorpseOpenAttemptCount { get; set; } = 30;
    public double BlacklistCorpseOpenTimeoutSeconds { get; set; } = 200d;
    public double CorpseCacheTimeoutMinutes { get; set; } = 60d;
    public int CorpseLootItemMaxAttempts { get; set; } = 20;
    public double ScanIntervalSeconds { get; set; } = 0.25d;
    public List<LootRule> Rules { get; } = [];
    public VtankSalvageCombineSettings SalvageCombine { get; set; } = new();
}

internal readonly record struct LootDecision(
    LootAction Action,
    int Priority,
    int RuleIndex,
    string RuleName,
    string ClassifierId = "");

internal readonly record struct ManaStoneTransferPlan(
    uint StoneObjectId,
    uint TankObjectId,
    string StoneName,
    string TankName);

internal static class ManaStoneTransferPlanner
{
    private const uint MagicalEffect = 0x00000001u;
    private const uint InscriptionProperty = 7u;
    private const uint ScribeProperty = 8u;
    private const uint TinkerCountProperty = 171u;

    /// <summary>
    /// An item worth emptying into a stone: something of the character's own
    /// making that still holds mana. Four things decide it -- it carries its
    /// own charge, it holds at least the floor the profile set, it has a
    /// workmanship of its own, and nobody has tinkered it. A tinkered item is
    /// somebody's work and draining it would be the last thing they wanted.
    /// </summary>
    internal static bool IsDonor(PluginInventoryItem item, int minimumTankMana) =>
        !item.IsEquipped
        && item.WielderObjectId == 0u
        && (item.Effects & MagicalEffect) != 0u
        && item.ItemCurrentMana >= minimumTankMana
        && item.Workmanship > 0f
        && item.NumTimesTinkered <= 0;

    /// <summary>
    /// Whether nobody has written on the item. An inscribed item carries both
    /// the text and the name of whoever wrote it, and either one is enough to
    /// say that somebody meant to keep it. Checked where the character
    /// already holds the item, because that is where the writing is known.
    /// </summary>
    internal static bool IsUninscribed(in PluginItemProperties properties) =>
        !HasText(properties, InscriptionProperty)
        && !HasText(properties, ScribeProperty);

    /// <summary>
    /// The second look, taken once the character holds the item: nobody has
    /// written on it and nobody has tinkered it. Both are somebody's work on
    /// an item they meant to keep, and both are only properly readable in
    /// hand, so an item that fails here is neither drained nor counted
    /// against the stone that was waiting for it.
    /// </summary>
    internal static bool IsDrainableInHand(in PluginItemProperties properties) =>
        IsUninscribed(properties)
        && (properties.Ints is not { } ints
            || !ints.TryGetValue(TinkerCountProperty, out int tinkers)
            || tinkers <= 0);

    private static bool HasText(in PluginItemProperties properties, uint key) =>
        properties.Strings is { } strings
        && strings.TryGetValue(key, out string? text)
        && !string.IsNullOrEmpty(text);

    /// <param name="canUseStone">
    /// Whether a stone may be spent on this pass. Nothing here asks whether it
    /// has been appraised: empty or charged is something the client says about
    /// the object itself, and a stone nobody has ever looked at is still
    /// somewhere to put mana.
    /// </param>
    /// <param name="canDrainTank">
    /// Whether an item may be emptied. This one does wait on an appraisal --
    /// how much mana the item holds is only known from one.
    /// </param>
    public static ManaStoneTransferPlan? Plan(
        IReadOnlyList<PluginInventoryItem> owned,
        IReadOnlyDictionary<uint, LootAction> classified,
        int minimumTankMana,
        Func<PluginInventoryItem, bool>? isProfiledManaStone = null,
        Func<uint, bool>? canUseStone = null,
        Func<uint, bool>? canDrainTank = null,
        Func<uint, PluginItemProperties?>? capture = null)
    {
        PluginInventoryItem stone = owned
            .Where(item => !item.IsEquipped
                && item.WielderObjectId == 0u
                && (canUseStone?.Invoke(item.ObjectId) ?? true)
                && (item.Effects & MagicalEffect) == 0u
                && isProfiledManaStone?.Invoke(item) == true)
            .OrderBy(static item => item.ObjectId)
            .FirstOrDefault();
        if (stone.ObjectId == 0u)
            return null;
        PluginInventoryItem tank = owned
            .Where(item => classified.TryGetValue(
                    item.ObjectId,
                    out LootAction action)
                && action == LootAction.ManaTank
                && IsDonor(item, minimumTankMana)
                && (canDrainTank?.Invoke(item.ObjectId) ?? true)
                && (capture is null
                    || (capture(item.ObjectId) is { } written
                        && IsUninscribed(written))))
            .OrderByDescending(static item => item.ItemCurrentMana)
            .ThenBy(static item => item.ObjectId)
            .FirstOrDefault();
        return tank.ObjectId == 0u
            ? null
            : new ManaStoneTransferPlan(
                stone.ObjectId,
                tank.ObjectId,
                stone.Name,
                tank.Name);
    }
}

internal sealed record SalvageBagCombinePlan(
    IReadOnlyList<uint> ObjectIds,
    uint MaterialType,
    IReadOnlyList<string> Names)
{
    public uint FirstObjectId => ObjectIds.Count > 0 ? ObjectIds[0] : 0u;
    public uint SecondObjectId => ObjectIds.Count > 1 ? ObjectIds[1] : 0u;
    public string FirstName => Names.Count > 0 ? Names[0] : string.Empty;
    public string SecondName => Names.Count > 1 ? Names[1] : string.Empty;
}

internal static partial class SalvageBagCombinePlanner
{
    public static SalvageBagCombinePlan? Plan(
        IReadOnlyList<PluginInventoryItem> owned,
        ISet<uint>? abandoned = null,
        VtankSalvageCombineSettings? settings = null)
    {
        settings ??= new VtankSalvageCombineSettings();
        PluginInventoryItem[] bags = owned
            .Where(item => item.MaterialType != 0u
                && SalvageBagName().IsMatch(item.Name)
                && (abandoned is null || !abandoned.Contains(item.ObjectId)))
            .OrderBy(static item => item.MaterialType)
            .ThenBy(static item => item.Workmanship)
            .ThenBy(static item => item.ObjectId)
            .ToArray();
        foreach (IGrouping<uint, PluginInventoryItem> materialGroup in
            bags.GroupBy(static item => item.MaterialType))
        {
            string combine = settings.MaterialCombineStrings.TryGetValue(
                checked((int)materialGroup.Key),
                out string? materialCombine)
                    ? materialCombine
                    : settings.DefaultCombineString;
            IReadOnlyList<(double Minimum, double Maximum)> ranges =
                ParseCombineString(combine);
            foreach (IGrouping<int, PluginInventoryItem> bin in materialGroup
                .GroupBy(item => RangeIndex(ranges, item.Workmanship))
                .OrderBy(static group => group.Key))
            {
                PluginInventoryItem[] candidates = bin.ToArray();
                if (candidates.Length < 2)
                    continue;
                IReadOnlyList<PluginInventoryItem> selected;
                if (settings.MaterialValueModeValues.TryGetValue(
                    checked((int)materialGroup.Key),
                    out int targetValue))
                {
                    if (candidates.Sum(static item => item.Value) >= targetValue)
                    {
                        selected = candidates;
                    }
                    else
                    {
                        selected = FindSubHundredPair(candidates);
                        if (selected.Count == 0)
                            continue;
                    }
                }
                else
                {
                    var maximumBags = new List<PluginInventoryItem>();
                    int units = 0;
                    foreach (PluginInventoryItem candidate in candidates)
                    {
                        maximumBags.Add(candidate);
                        units += Math.Max(0, candidate.Structure);
                        if (units >= 100)
                            break;
                    }
                    selected = maximumBags;
                }
                return new SalvageBagCombinePlan(
                    selected.Select(static item => item.ObjectId).ToArray(),
                    materialGroup.Key,
                    selected.Select(static item => item.Name).ToArray());
            }
        }
        return null;
    }

    internal static bool SameVtankWorkmanshipBand(float left, float right) =>
        (left < 7f && right < 7f)
        || (left >= 7f && left < 9f && right >= 7f && right < 9f)
        || (left >= 9f && left < 10f && right >= 9f && right < 10f)
        || (left == 10f && right == 10f);

    internal static bool SameCombineBand(
        float left,
        float right,
        string combineString)
    {
        IReadOnlyList<(double Minimum, double Maximum)> ranges =
            ParseCombineString(combineString);
        return RangeIndex(ranges, left) == RangeIndex(ranges, right);
    }

    private static IReadOnlyList<PluginInventoryItem> FindSubHundredPair(
        IReadOnlyList<PluginInventoryItem> candidates)
    {
        for (int left = 0; left < candidates.Count - 1; left++)
        {
            for (int right = left + 1; right < candidates.Count; right++)
            {
                if (Math.Max(0, candidates[left].Structure)
                    + Math.Max(0, candidates[right].Structure) < 100)
                {
                    return [candidates[left], candidates[right]];
                }
            }
        }
        return [];
    }

    private static IReadOnlyList<(double Minimum, double Maximum)>
        ParseCombineString(string? source)
    {
        var result = new List<(double, double)>();
        foreach (string token in (source ?? string.Empty).Split(
            [',', ';'],
            StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            string[] bounds = token.Split(
                '-',
                StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            if (bounds.Length == 0
                || !double.TryParse(bounds[0], NumberStyles.Float,
                    CultureInfo.InvariantCulture, out double minimum))
            {
                continue;
            }
            double maximum = minimum;
            if (bounds.Length > 1)
            {
                _ = double.TryParse(bounds[1], NumberStyles.Float,
                    CultureInfo.InvariantCulture, out maximum);
            }
            result.Add((minimum, maximum));
        }
        return result;
    }

    private static int RangeIndex(
        IReadOnlyList<(double Minimum, double Maximum)> ranges,
        double value)
    {
        for (int index = 0; index < ranges.Count; index++)
        {
            if (ranges[index].Minimum > value)
                return index - 1;
            if (ranges[index].Minimum <= value
                && ranges[index].Maximum >= value)
            {
                return index;
            }
        }
        return ranges.Count;
    }

    [GeneratedRegex(@"^Salvage(?:d)?.* \([0-9]{1,2}\)$", RegexOptions.IgnoreCase)]
    private static partial Regex SalvageBagName();
}

/// <summary>Why the rule list chose nothing for an item.</summary>
internal enum LootRefusalKind
{
    /// <summary>No rule in the list matched the item at all.</summary>
    NoRule,

    /// <summary>A rule matched, and it says the item stays where it is.</summary>
    RuleDeclined,

    /// <summary>A rule matched, and the character already holds its limit.</summary>
    RuleAtCap,
}

/// <summary>
/// The rule list's answer when it chose nothing. All three reasons look the
/// same from outside — the item is left behind — and one log line covering
/// all three is what makes a profile quietly sitting at its keep limit
/// indistinguishable from one whose rule never fired at all.
/// </summary>
internal readonly record struct LootRefusal(
    LootRefusalKind Kind,
    string RuleName = "",
    int Limit = 0,
    int Held = 0)
{
    public static LootRefusal NoRule => new(LootRefusalKind.NoRule);

    public string Describe() => Kind switch
    {
        LootRefusalKind.RuleDeclined => $"{RuleName} says leave it",
        LootRefusalKind.RuleAtCap =>
            $"{RuleName} keeps up to {Limit} (holding {Held})",
        _ => "no rule matched",
    };
}

internal static class LootRuleEngine
{
    public static LootDecision? Decide(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<LootRule> rules,
        IReadOnlyList<PluginInventoryItem> ownedItems,
        IReadOnlyDictionary<string, int>? pendingByName = null,
        IPluginHost? host = null) =>
        Decide(
            item,
            properties,
            rules,
            ownedItems,
            out _,
            pendingByName,
            host);

    public static LootDecision? Decide(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<LootRule> rules,
        IReadOnlyList<PluginInventoryItem> ownedItems,
        out LootRefusal refusal,
        IReadOnlyDictionary<string, int>? pendingByName = null,
        IPluginHost? host = null)
    {
        ArgumentNullException.ThrowIfNull(rules);
        ArgumentNullException.ThrowIfNull(ownedItems);

        refusal = LootRefusal.NoRule;
        for (int index = 0; index < rules.Count; index++)
        {
            LootRule rule = rules[index];
            if (!rule.IsMatch(item, properties, host, out _))
                continue;
            string ruleName = string.IsNullOrWhiteSpace(rule.Name)
                ? $"Rule {index + 1}"
                : rule.Name;
            if (rule.Action == LootAction.NoLoot)
            {
                refusal = new LootRefusal(LootRefusalKind.RuleDeclined, ruleName);
                return null;
            }
            if (rule.Action == LootAction.KeepUpTo)
            {
                int limit = Math.Max(0, rule.KeepCount);
                string itemName = item.Name;
                int held = ownedItems
                    .Where(owned => string.Equals(
                        owned.Name,
                        itemName,
                        StringComparison.OrdinalIgnoreCase))
                    .Sum(static owned => Math.Max(1, owned.StackSize));
                if (pendingByName is not null
                    && pendingByName.TryGetValue(item.Name, out int pending))
                {
                    held += pending;
                }
                if (held >= limit)
                {
                    refusal = new LootRefusal(
                        LootRefusalKind.RuleAtCap, ruleName, limit, held);
                    return null;
                }
            }
            return new LootDecision(
                rule.Action,
                rule.Priority,
                index,
                ruleName);
        }
        return null;
    }

    /// <summary>
    /// One forward pass that answers "would appraising this item change which
    /// rule wins?". A rule that can already decide short-circuits; so does a
    /// later rule carrying the same action as an earlier still-open one,
    /// because the outcome is the same either way. Only a later rule with a
    /// different action than a still-open one forces the appraisal.
    /// </summary>
    public static bool NeedsIdentify(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<LootRule> rules,
        IPluginHost? host = null)
    {
        ArgumentNullException.ThrowIfNull(rules);

        bool open = false;
        LootAction openAction = LootAction.NoLoot;
        foreach (LootRule rule in rules)
        {
            if (open && rule.Action != openAction)
                return true;
            rule.EarlyMatch(
                item,
                properties,
                host,
                out bool hasDecision,
                out bool isMatch);
            if (hasDecision && isMatch)
                return false;
            if (hasDecision)
                continue;
            open = true;
            openAction = rule.Action;
        }
        return open;
    }
}

internal sealed partial class LootController
{
    /// <summary>How long the salvage, sell, mana-fill and combine steps wait for their answer.</summary>
    private const double PickupTimeoutSeconds = 4d;

    /// <summary>
    /// How long one pull holds the item slot and navigation. It is the pace of
    /// emptying a corpse, and it is what keeps the walk rule above this one
    /// off the character between pulls.
    /// </summary>
    private const double PickupHoldSeconds = 0.75d;

    /// <summary>
    /// How long a corpse the server refused stays skipped. This one is fixed;
    /// it is not one of the corpse-blacklist settings.
    /// </summary>
    private const double DenialSkipSeconds = 10d;

    private readonly IPluginHost _host;
    private readonly LootSettings _settings;
    private readonly ISet<string> _configuredConsumableNames;
    private readonly IDictionary<string, ConsumableCategory>
        _configuredConsumableKinds;
    private readonly Dictionary<uint, double> _completedCorpses = [];
    private readonly Dictionary<uint, int> _corpseOpenAttempts = [];
    private readonly Dictionary<uint, double> _corpseBlacklistedAt = [];
    private readonly Dictionary<uint, int> _itemAttempts = [];
    private readonly Dictionary<string, int> _pendingByName =
        new(StringComparer.OrdinalIgnoreCase);
    private readonly Dictionary<uint, LootAction> _classifiedOwnedItems = [];
    private readonly Dictionary<uint, string> _externalClassifierByItem = [];
    private readonly Dictionary<uint, LootDecision?> _decisions = [];
    private readonly Dictionary<uint, double> _corpseFirstSeen = [];

    /// <summary>
    /// When each corpse last came into the client's known set. The eviction
    /// clock runs from there, not from the last time it was looked at.
    /// </summary>
    private readonly Dictionary<uint, double> _corpseLastSeen = [];

    /// <summary>The corpses the client has stopped reporting.</summary>
    private readonly HashSet<uint> _releasedCorpses = [];
    private readonly HashSet<uint> _presentCorpses = [];
    private readonly List<uint> _evictedCorpses = [];
    private readonly Dictionary<uint, double> _corpseDeniedAt = [];

    /// <summary>
    /// How many times each corpse has been asked to describe itself without
    /// an answer coming back. Counted so a corpse that never answers is let
    /// go of instead of holding the character still for ever.
    /// </summary>
    private readonly Dictionary<uint, int> _corpseDescriptionAttempts = [];

    /// <summary>
    /// Items already reported as dropped from the pass for spending their
    /// attempts, so the line is written once rather than on every turn.
    /// </summary>
    private readonly HashSet<uint> _abandonedItems = [];

    /// <summary>
    /// The corpse the pass gave up on, still open and waiting for its closing
    /// use. It is deliberately NOT a completed corpse: nothing was taken from
    /// it, and once the packs have room again it is worth another visit.
    /// </summary>
    private uint _abandonedCorpse;
    private uint _selectedCorpse;
    private ulong _chatSequence;
    private double _stateAge;
    private uint _activeCorpse;
    private bool _activeCorpseSawContents;
    private bool _activeCorpseIsOwnDeath;
    private uint _waitingItem;
    private string _waitingName = string.Empty;
    private LootAction _waitingAction;
    private int _waitingQuantity;
    private PluginInventoryItem _waitingItemSnapshot;
    private string _waitingClassifierId = string.Empty;
    private bool _waitingPickupAccepted;
    private long _waitingPickupCompletionRevision;
    private uint _awaitingAppraisal;
    private uint _awaitingCorpseAppraisal;
    private uint _lastCorpseDescriptionRequest;
    private double _lifetime;
    private readonly Dictionary<uint, uint> _pendingScrollReads = [];
    private uint _salvagePendingItem;
    private string _salvagePendingName = string.Empty;
    private int _salvageAttempts;
    private uint _sellPendingItem;
    private string _sellPendingName = string.Empty;
    private ManaStoneTransferPlan? _manaTransfer;
    private long _manaTransferRevision;

    /// <summary>
    /// When the item-slot window the outstanding drain armed runs out. Past
    /// it the slot may be somebody else's, and giving it back then would pull
    /// the floor out from under whoever is standing on it.
    /// </summary>
    private double _manaTransferHoldUntil;
    private readonly HashSet<uint> _uncertainManaItems = [];
    private SalvageBagCombinePlan? _combinePending;
    private readonly Dictionary<uint, int> _combineAttempts = [];
    private readonly HashSet<uint> _abandonedCombineBags = [];

    private ActionLockTable? _actionLocks;

    public LootController(
        IPluginHost host,
        LootSettings settings,
        ISet<string>? configuredConsumableNames = null,
        IDictionary<string, ConsumableCategory>? configuredConsumableKinds = null)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _configuredConsumableNames = configuredConsumableNames
            ?? new HashSet<string>(StringComparer.Ordinal);
        _configuredConsumableKinds = configuredConsumableKinds
            ?? new Dictionary<string, ConsumableCategory>(StringComparer.Ordinal);
    }

    /// <summary>
    /// A mana stone the profile knows by that name: it is an item the client
    /// classes as a mana stone AND the profile's helper list names as one.
    /// The kind matters, not merely the presence of the name: the same list
    /// carries mana charges, healing kits and food under their own kinds, and
    /// treating any of them as a stone would have the looter fill up on the
    /// wrong thing.
    /// </summary>
    private bool IsProfiledManaStone(in PluginInventoryItem item) =>
        item.ObjectClass == PluginObjectClass.ManaStone
        && _configuredConsumableKinds.TryGetValue(
            item.Name,
            out ConsumableCategory kind)
        && kind == ConsumableCategory.ManaStone;

    /// <summary>
    /// Whether the profile asks for mana stones at all. Without one helper
    /// row of that kind the wanted count is zero whatever the stone-count
    /// setting says, because nothing has told the macro which item is a
    /// stone it may pick up.
    /// </summary>
    private bool ProfileNamesAManaStone =>
        _configuredConsumableKinds.Values.Any(
            static kind => kind == ConsumableCategory.ManaStone);

    /// <summary>
    /// The shared action-lock table. A corpse is closed with an item use,
    /// and that use waits its turn behind whatever else is holding the
    /// item slot, the same way every other item rule does.
    /// </summary>
    internal void BindActionLocks(ActionLockTable locks) =>
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));

    /// <summary>
    /// Whether the item slot is held. With the lock table bound that is the
    /// table's answer; without one, the host's own busy flag stands in.
    /// </summary>
    private bool ItemSlotHeld(IItemAutomation items) =>
        _actionLocks?.IsLocked(ActionLockKind.ItemUse) ?? items.IsBusy;

    public string Status { get; private set; } = "Looting disabled.";

    /// <summary>
    /// The macro log sink. Looting is otherwise silent apart from a status
    /// string nobody can read from outside the panel, which makes a looting
    /// pass impossible to follow in a log — so what it opened, what it
    /// decided and what it took go to the Loot channel.
    /// </summary>
    public Action<MacroLogChannel, string>? Log { get; set; }

    public IReadOnlyDictionary<uint, LootAction> ClassifiedOwnedItems =>
        _classifiedOwnedItems;

    /// <summary>
    /// The scrolls picked up for reading, spell id to item id. Reading them is
    /// a separate rule's job, not a continuation of the pickup, so this is
    /// what the two sides share.
    /// </summary>
    public IReadOnlyDictionary<uint, uint> PendingScrollReads =>
        _pendingScrollReads;

    /// <summary>
    /// Drops the queued scrolls whose item has left the character's hands —
    /// read, dropped, sold or given away. A scroll that is still held stays
    /// queued however many times reading it has failed.
    /// </summary>
    public void ForgetUnownedScrollReads(
        IReadOnlyList<PluginInventoryItem> owned)
    {
        ArgumentNullException.ThrowIfNull(owned);
        if (_pendingScrollReads.Count == 0)
            return;
        foreach ((uint spellId, uint itemId) in _pendingScrollReads.ToArray())
        {
            if (!owned.Any(item => item.ObjectId == itemId))
                _pendingScrollReads.Remove(spellId);
        }
    }

    /// <summary>Drops the queued scrolls whose spell the character now knows.</summary>
    public void ForgetKnownScrollReads()
    {
        if (_pendingScrollReads.Count == 0)
            return;
        ISpellCatalog spells = _host.Automation.Spells;
        foreach (uint spellId in _pendingScrollReads.Keys.ToArray())
        {
            if (spells.IsKnown(spellId))
                _pendingScrollReads.Remove(spellId);
        }
    }

    public bool Tick(double elapsedSeconds, bool canAct)
    {
        ILootAutomation loot = _host.Automation.Loot;
        if (!_settings.ProfileActive)
        {
            ResetTransient();
            Status = "No loot profile is active.";
            return false;
        }
        if (!_settings.Enabled)
        {
            ResetTransient();
            Status = "Looting disabled.";
            return false;
        }
        if (!_host.Automation.IsAvailable || !loot.IsAvailable)
        {
            Reset();
            Status = "Looting unavailable.";
            return false;
        }
        if (_settings.Rules.Count == 0
            && string.IsNullOrWhiteSpace(_settings.ExternalClassifierId))
        {
            ResetTransient();
            Status = "Loot profile has no rules.";
            return false;
        }

        // "Is a corpse open?" is re-answered from the client before anything
        // else this pass asks it, because the rest of the loot work — salvage,
        // combining, selling — is barred while one is. The corpse stops being
        // this controller's business the moment the client reports the
        // container is no longer open, which is also what ends the closing-use
        // retry below.
        uint current = loot.CurrentContainerId;
        if (_activeCorpse != 0u
            && current != _activeCorpse
            && _waitingItem != 0u)
        {
            if (_host.Automation.Items.CaptureOwnedItems().Any(
                    item => item.ObjectId == _waitingItem))
            {
                CompleteWaitingPickup();
            }
            else
            {
                AbandonWaitingItem("the corpse closed before it arrived");
            }
        }
        if (_activeCorpse != 0u
            && current != _activeCorpse
            && IsCorpseFinished(_activeCorpse))
        {
            _abandonedCorpse = 0u;
            _activeCorpse = 0u;
            _activeCorpseSawContents = false;
            _activeCorpseIsOwnDeath = false;
            _stateAge = 0d;
        }

        if (_activeCorpse == 0u && _waitingItem == 0u)
            PruneRemovedExternalItems();

        double elapsed = Math.Max(0d, elapsedSeconds);
        // An open still waiting for its container, held up behind somebody
        // else's item use, is not an open that is failing: the wait belongs
        // to the holder. Charging it against the open timeout wrote off
        // perfectly good corpses -- the attempt count climbed and the corpse
        // was blacklisted -- whenever another rule happened to be mid-use.
        // Our own open holds the corpse-open slot as well, which is what
        // tells the two apart.
        bool delayedByAnotherHolder = _activeCorpse != 0u
            && current != _activeCorpse
            && _actionLocks is { } holdTable
            && holdTable.IsLocked(ActionLockKind.ItemUse)
            && !holdTable.IsLocked(ActionLockKind.CorpseOpenAttempt);
        if (!delayedByAnotherHolder)
            _stateAge += elapsed;
        _lifetime += elapsed;
        ObserveOwnershipDenials();

        // The reference's open rule is valid — and holds the pass doing
        // nothing — for as long as the item slot is held: an open attempt,
        // a pull, any item use of ours is answered before the next loot
        // step is taken. The slot comes down on the frame the container
        // opens, or when its own window runs out.
        // A drain this controller has already issued is the one thing read
        // through a held slot: the slot it is waiting behind is its own, and
        // a pass that cannot look at its own answer would only ever end the
        // wait on the watchdog.
        if (_manaTransfer is null
            && _actionLocks is { } locks
            && locks.IsLocked(ActionLockKind.ItemUse))
        {
            Status = _waitingItem != 0u
                ? $"Waiting for {_waitingName}…"
                : _activeCorpse != 0u && current != _activeCorpse
                    ? "Waiting for corpse contents…"
                    : "Waiting for the item slot…";
            return true;
        }

        if ((_manaTransfer is not null
                || HasManaStoneTransfer())
            && ContinueManaStoneTransfer(canAct))
        {
            return true;
        }
        if (_activeCorpse == 0u
            && (_salvagePendingItem != 0u
                || _classifiedOwnedItems.Values.Contains(LootAction.Salvage))
            && ContinueSalvage(canAct))
        {
            return true;
        }
        if (_activeCorpse == 0u
            && _settings.CombineSalvage
            && (_combinePending is not null || HasSalvageBagCombine())
            && ContinueSalvageBagCombine(canAct))
        {
            return true;
        }
        if (_activeCorpse == 0u
            && (_sellPendingItem != 0u
                || _classifiedOwnedItems.Values.Contains(LootAction.Sell))
            && ContinueSell(canAct))
        {
            return true;
        }

        if (_activeCorpse != 0u && current == _activeCorpse)
            return ContinueCurrentCorpse(loot, canAct);

        if (_activeCorpse != 0u
            && loot.RequestedContainerId == _activeCorpse)
        {
            if (_stateAge <= Math.Max(0.25d, _settings.CorpseOpenTimeoutSeconds))
            {
                Status = "Waiting for corpse contents…";
                return true;
            }
            uint failedCorpse = _activeCorpse;
            BlacklistFailedCorpse(failedCorpse);
            _activeCorpse = 0u;
            _activeCorpseSawContents = false;
            _activeCorpseIsOwnDeath = false;
            _stateAge = 0d;
            if (IsCorpseBlacklisted(failedCorpse))
                return false;
        }

        if (!canAct)
            return false;

        // Every pass, not on a pacing clock: the reference selects afresh
        // each time it is asked. The age clock the public and fellow timers
        // measure against starts when a corpse first streams into the
        // client's known set, which is a far wider radius than any of the
        // loot ranges — a corpse watched from across a field is already old
        // enough by the time the player walks up to it.
        IReadOnlyList<PluginLootContainer> known =
            loot.CaptureCorpses(float.MaxValue);
        PruneCorpseCache(known);

        // Descriptions are the frame's business (TickIdentification), asked
        // of every corpse the client reports as it appears; the rule's own
        // turn never spends itself on one. Only a described corpse within
        // arm's reach is ready work here; the walk is its own rule, one
        // position above this one.
        _selectedCorpse = 0u;
        if (SelectCorpse(known, CorpseOpenRangeMeters, byHeading: true)
            is { } corpse)
        {
            _selectedCorpse = corpse.ObjectId;

            PluginItemCommandResult opened = loot.Open(corpse.ObjectId);
            if (!opened.Accepted)
            {
                if (opened.Status == PluginItemCommandStatus.Busy)
                {
                    Status = "Waiting to open corpse…";
                    return true;
                }
                // The reference counts every open attempt, refused or not,
                // and blacklists the corpse at the profile's attempt count.
                // It also arms the same three slots after every attempt,
                // refused or not -- which is what paces the retries. Without
                // the arms the attempt ceiling is spent in a couple of
                // seconds at the heartbeat and the corpse is written off.
                Status = $"Could not open {corpse.Name} ({opened.Status}).";
                Log?.Invoke(
                    MacroLogChannel.Loot,
                    $"LootCorpse: open of {corpse.Name} (0x{corpse.ObjectId:X8}) refused: {opened.Status}");
                BlacklistFailedCorpse(corpse.ObjectId);
                ArmCorpseOpenSlots();
                return false;
            }
            _activeCorpse = corpse.ObjectId;
            _activeCorpseSawContents = false;
            _activeCorpseIsOwnDeath = IsOwnDeathCorpse(corpse);
            _stateAge = 0d;
            // Opening a corpse is not instant and it is not always local: a corpse
            // out of arm's reach is opened by walking to it first, and the walk is
            // the client's, not this controller's. So the open holds three slots —
            // the item slot, the corpse-open slot, and navigation, that last one so
            // the route rule does not steer against the walk the open just started.
            // The three are held only until the container actually opens; the
            // timeout is the ceiling for an open that never lands, not the wait.
            // See ObserveCorpseOpened, which is what gives them back and what the
            // corpse-open slot exists to mark.
            ArmCorpseOpenSlots();
            Status = $"Opening {corpse.Name}…";
            Log?.Invoke(
                MacroLogChannel.Loot,
                $"LootCorpse: opening {corpse.Name} (0x{corpse.ObjectId:X8})");
            return true;
        }

        // A sale still queued for a vendor keeps its own status line; the
        // corpse scan has nothing to add to it.
        if (_sellPendingItem == 0u
            && !_classifiedOwnedItems.Values.Contains(LootAction.Sell))
        {
            Status = "No nearby corpses.";
        }
        return false;
    }

    // Advance from the last accepted request so one corpse that never
    // answers cannot monopolize every later request. True when a request
    // went out or the client is busy with one.
    private bool TryRequestNextCorpseDescription(
        ILootAutomation loot,
        IReadOnlyList<PluginLootContainer> known)
    {
        int lastRequestIndex = -1;
        for (int index = 0; index < known.Count; index++)
        {
            if (known[index].ObjectId == _lastCorpseDescriptionRequest)
            {
                lastRequestIndex = index;
                break;
            }
        }
        for (int offset = 1; offset <= known.Count; offset++)
        {
            int index = (lastRequestIndex + offset) % known.Count;
            PluginLootContainer candidateCorpse = known[index];
            if (candidateCorpse.IsIdentified
                || _completedCorpses.ContainsKey(candidateCorpse.ObjectId)
                || IsCorpseDenied(candidateCorpse.ObjectId)
                || IsCorpseBlacklisted(candidateCorpse.ObjectId))
            {
                continue;
            }
            PluginItemCommandResult identify = loot.Identify(
                candidateCorpse.ObjectId);
            if (identify.Accepted)
            {
                _awaitingCorpseAppraisal = candidateCorpse.ObjectId;
                _lastCorpseDescriptionRequest = candidateCorpse.ObjectId;
                _stateAge = 0d;
                _identifyAge = 0d;
                Status = $"Identifying {candidateCorpse.Name}…";
                Log?.Invoke(
                    MacroLogChannel.Loot,
                    $"Identifying corpse {candidateCorpse.Name} (0x{candidateCorpse.ObjectId:X8}) at {candidateCorpse.Distance:0.0}m");
                return true;
            }
            if (identify.Status == PluginItemCommandStatus.Busy)
                return true;
        }
        return false;
    }

    private double _identifyAge;

    /// <summary>
    /// The reference's id queue sends one request every 499 ms, round robin
    /// over everything waiting for an id.
    /// </summary>
    private const double IdentifyRequestIntervalSeconds = 0.499d;
    private double _sinceIdentifyRequest = IdentifyRequestIntervalSeconds;

    /// <summary>
    /// Asks for descriptions and item ids on the host's frame rather than on
    /// the loot rule's turn: the reference queues every corpse on its radar
    /// as it appears and every item of an opened corpse as the contents
    /// arrive, and sends the requests from a timer of its own. So by the
    /// time the fight is over the corpse is already known and the open
    /// follows at once, and while a corpse is open its items are described
    /// beside the pass, never at the cost of one. Left to the rule, each
    /// request could only be sent on a turn the rule won, a whole pass late
    /// every time — and a request is an item transaction, which would have
    /// stood the whole pass still.
    /// </summary>
    internal void TickIdentification(double elapsedSeconds)
    {
        if (!_settings.ProfileActive
            || !_settings.Enabled
            || !_host.Automation.IsAvailable)
        {
            return;
        }
        ILootAutomation loot = _host.Automation.Loot;
        if (!loot.IsAvailable)
            return;
        double elapsed = Math.Max(0d, elapsedSeconds);
        _identifyAge += elapsed;
        _sinceIdentifyRequest += elapsed;
        if (_activeCorpse != 0u && loot.CurrentContainerId == _activeCorpse)
        {
            TickCorpseItemIdentification(loot);
            return;
        }
        // The item latch belongs to the corpse that is open: with none open
        // there is no item left to be waiting for, and the one description
        // outstanding when a corpse shut used to be held for ever right
        // here, which stopped every later corpse from being described at
        // all.
        _awaitingAppraisal = 0u;
        if (_awaitingCorpseAppraisal != 0u)
        {
            PluginAppraisalState appraisal = loot.Appraisal;
            bool answered = appraisal.CurrentObjectId == _awaitingCorpseAppraisal
                && appraisal.AwaitingObjectId != _awaitingCorpseAppraisal;
            // The client says outright when it has given up on a question,
            // which is a failure to count now rather than a wait to sit out.
            bool dropped =
                appraisal.LastAbandonedObjectId == _awaitingCorpseAppraisal;
            bool expired = _identifyAge >= Math.Max(
                1d,
                _settings.CorpseOpenTimeoutSeconds * 2d);
            if (!answered && !dropped && !expired)
                return;
            if (!answered)
                NoteCorpseDescriptionFailed(_awaitingCorpseAppraisal);
            _awaitingCorpseAppraisal = 0u;
            _identifyAge = 0d;
        }
        // Every corpse the client reports, as the reference's identify queue
        // does: a corpse watched from across the field is described long
        // before the character walks up to it.
        IReadOnlyList<PluginLootContainer> known = loot.CaptureCorpses(float.MaxValue);
        _ = TryRequestNextCorpseDescription(loot, known);
    }

    /// <summary>
    /// The item half of the frame's identification: while a corpse is open,
    /// each item that needs an id is asked for one, one request at a time on
    /// the reference's cadence, and each answer becomes a decision the rule's
    /// next turn can act on. Items that need no id are decided as they are
    /// seen.
    /// </summary>
    private void TickCorpseItemIdentification(ILootAutomation loot)
    {
        if (!loot.CurrentContentsReady)
            return;
        IReadOnlyList<PluginInventoryItem> contents = loot.CaptureCurrentContents();
        IReadOnlyList<PluginInventoryItem> owned =
            _host.Automation.Items.CaptureOwnedItems();
        if (_awaitingAppraisal != 0u)
        {
            PluginAppraisalState appraisal = loot.Appraisal;
            if (appraisal.CurrentObjectId == _awaitingAppraisal
                && appraisal.AwaitingObjectId != _awaitingAppraisal)
            {
                if (contents.FirstOrDefault(
                        item => item.ObjectId == _awaitingAppraisal) is { } answered
                    && answered.ObjectId != 0u)
                {
                    PluginItemProperties identified = default;
                    _ = loot.TryCaptureProperties(answered.ObjectId, out identified);
                    RecordDecision(
                        answered,
                        DecideItem(
                            answered,
                            identified,
                            owned,
                            _pendingByName,
                            out LootRefusal answeredRefusal),
                        answeredRefusal);
                }
                _awaitingAppraisal = 0u;
                _identifyAge = 0d;
            }
            // A question the client has already given up on is a failure
            // now, not a wait to sit out: without this the item pass stood
            // still for the whole identify timeout over an answer that was
            // never coming, and the corpse behind it with it.
            else if (appraisal.LastAbandonedObjectId != _awaitingAppraisal
                && _identifyAge < Math.Clamp(
                    _settings.CorpseItemIdentifyTimeoutSeconds,
                    1d,
                    600d))
            {
                return;
            }
            else
            {
                IncrementAttempt(_awaitingAppraisal);
                _awaitingAppraisal = 0u;
                _identifyAge = 0d;
            }
        }

        foreach (PluginInventoryItem item in contents)
        {
            if (_decisions.ContainsKey(item.ObjectId))
                continue;
            PluginItemProperties properties = default;
            _ = loot.TryCaptureProperties(item.ObjectId, out properties);
            if (!NeedsIdentify(item, properties, owned))
            {
                RecordDecision(
                    item,
                    DecideItem(
                        item,
                        properties,
                        owned,
                        _pendingByName,
                        out LootRefusal refusal),
                    refusal);
                continue;
            }
            if (_sinceIdentifyRequest < IdentifyRequestIntervalSeconds)
                return;
            PluginItemCommandResult identify = loot.Identify(item.ObjectId);
            if (identify.Accepted)
            {
                _awaitingAppraisal = item.ObjectId;
                _identifyAge = 0d;
                _sinceIdentifyRequest = 0d;
            }
            // Busy or refused: asked again on a later frame.
            return;
        }
    }

    public void Reset()
    {
        foreach ((uint objectId, string classifierId) in
                 _externalClassifierByItem.ToArray())
        {
            _host.LootClassifiers.TryNotifyItemRemoved(classifierId, objectId);
        }
        ResetTransient();
        _completedCorpses.Clear();
        _corpseOpenAttempts.Clear();
        _corpseDescriptionAttempts.Clear();
        _corpseBlacklistedAt.Clear();
        _itemAttempts.Clear();
        _abandonedItems.Clear();
        _abandonedCorpse = 0u;
        _pendingByName.Clear();
        _classifiedOwnedItems.Clear();
        _externalClassifierByItem.Clear();
        _decisions.Clear();
        _corpseFirstSeen.Clear();
        _corpseLastSeen.Clear();
        _releasedCorpses.Clear();
        _presentCorpses.Clear();
        _evictedCorpses.Clear();
        _corpseDeniedAt.Clear();
        _pendingScrollReads.Clear();
        _selectedCorpse = 0u;
        _chatSequence = 0uL;
        _lifetime = 0d;
        _salvagePendingItem = 0u;
        _salvagePendingName = string.Empty;
        _salvageAttempts = 0;
        _sellPendingItem = 0u;
        _sellPendingName = string.Empty;
        _manaTransfer = null;
        _manaTransferRevision = 0L;
        _manaTransferHoldUntil = 0d;
        _combinePending = null;
        _combineAttempts.Clear();
        _abandonedCombineBags.Clear();
        _uncertainManaItems.Clear();
        Status = _settings.Enabled ? "Idle." : "Looting disabled.";
    }

    private bool ContinueCurrentCorpse(ILootAutomation loot, bool canAct)
    {
        // Once the item pass is done the corpse is marked looted and the only
        // thing left is the closing use. That use is retried every pass until
        // the container actually shuts — nothing re-reads the contents in
        // between, and the corpse stays this pass's business meanwhile.
        if (IsCorpseFinished(_activeCorpse))
            return CloseFinishedCorpse(_activeCorpse, canAct);

        // No settling pause before the contents are read: whether they have
        // arrived is a question the client answers, and a fixed wait on top
        // of it only cost a heartbeat per corpse.
        if (!loot.CurrentContentsReady)
        {
            Status = "Waiting for corpse item data…";
            return true;
        }

        IReadOnlyList<PluginInventoryItem> contents =
            loot.CaptureCurrentContents();
        if (contents.Count > 0)
            _activeCorpseSawContents = true;
        if (contents.Count == 0
            && !_activeCorpseSawContents
            && _stateAge < Math.Clamp(
                _settings.CorpseItemAppearanceTimeoutSeconds,
                0d,
                300d))
        {
            Status = "Waiting for corpse items to appear…";
            return true;
        }

        ObservePickup(loot, contents);

        if (_waitingItem != 0u
            && _waitingPickupAccepted
            && contents.Any(item => item.ObjectId == _waitingItem)
            && _host.Automation.Items.IsBusy)
        {
            Status = $"Waiting for {_waitingName}…";
            return true;
        }

        // A receipt at its bounded retry ceiling is a definitive failed
        // pickup. Reconcile it before choosing another item, otherwise the
        // next accepted pull would inherit the old receipt and lose its own
        // transfer classification.
        int maximumAttempts = Math.Clamp(
            _settings.CorpseLootItemMaxAttempts,
            1,
            100);
        if (_waitingItem != 0u
            && !_waitingPickupAccepted
            && _itemAttempts.TryGetValue(_waitingItem, out int waitingAttempts)
            && waitingAttempts >= maximumAttempts
            && contents.Any(item => item.ObjectId == _waitingItem))
        {
            AbandonWaitingItem($"{waitingAttempts} attempts spent");
        }

        // Items still waiting on their description are the frame's business
        // (TickIdentification); this turn works from the decisions already
        // made, the way the reference's pull step works from its queue.
        bool identifying = false;
        var candidates = new List<(PluginInventoryItem Item, LootDecision Decision)>();
        foreach (PluginInventoryItem item in contents)
        {
            if (!_decisions.TryGetValue(item.ObjectId, out LootDecision? cached))
            {
                identifying = true;
                continue;
            }
            if (_itemAttempts.TryGetValue(item.ObjectId, out int attempts)
                && attempts >= maximumAttempts)
            {
                // Dropping an item out of the pass is a decision, and a
                // silent one left a run with no record of why an item was
                // still sitting in the corpse when it was closed.
                if (_abandonedItems.Add(item.ObjectId))
                {
                    Log?.Invoke(
                        MacroLogChannel.Loot,
                        $"LootPickup: abandoned {item.Name}: {attempts} attempts spent");
                }
                continue;
            }
            if (cached is { } decision)
                candidates.Add((item, decision));
        }

        if (candidates.Count == 0)
        {
            if (identifying)
            {
                if (_stateAge < Math.Clamp(
                    _settings.CorpseItemIdentifyTimeoutSeconds,
                    1d,
                    600d))
                {
                    Status = "Identifying corpse items…";
                    return true;
                }
                Log?.Invoke(
                    MacroLogChannel.Loot,
                    $"CorpseWait: abandoned 0x{_activeCorpse:X8}, unable to receive an id for every item");
            }
            foreach (PluginInventoryItem item in contents)
                _decisions.Remove(item.ObjectId);
            uint finished = _activeCorpse;
            MarkCorpseComplete(finished);
            _stateAge = 0d;
            Log?.Invoke(
                MacroLogChannel.Loot,
                $"CorpseWait: closing 0x{finished:X8}");
            return CloseFinishedCorpse(finished, canAct);
        }
        if (!canAct)
            return true;

        if (_waitingItem != 0u
            && _waitingPickupAccepted
            && contents.Any(item => item.ObjectId == _waitingItem)
            && _decisions.TryGetValue(_waitingItem, out LootDecision? waitingDecision)
            && waitingDecision is { } pendingDecision)
        {
            IssuePickup(loot, new(
                contents.First(item => item.ObjectId == _waitingItem), pendingDecision));
            return true;
        }

        (PluginInventoryItem Item, LootDecision Decision) chosen = candidates
            .OrderByDescending(static candidate => candidate.Decision.Priority)
            .ThenBy(static candidate => candidate.Decision.RuleIndex)
            .ThenBy(static candidate => candidate.Item.ContainerSlot)
            .ThenBy(static candidate => candidate.Item.ObjectId)
            .First();
        return IssuePickup(loot, chosen);
    }

    private bool IssuePickup(
        ILootAutomation loot,
        (PluginInventoryItem Item, LootDecision Decision) chosen)
    {
        PluginItemCommandResult pickup = loot.Pickup(chosen.Item.ObjectId);

        // Every pull holds the item slot and navigation for three quarters of
        // a second, whether or not it actually went out, the way the
        // reference's pickup step does. The item slot is what paces the pulls
        // — and what this rule holds the pass on meanwhile; navigation is what
        // stops the walk-to-a-corpse rule — which outranks this one — from
        // steering the character away from the corpse it is standing over, one
        // item into emptying it. A pull the client would not send needs the
        // pace most of all: without the hold it is re-issued on every
        // heartbeat, the whole attempt ceiling is spent in a fraction of a
        // second, and the client's own notice is re-printed each time.
        _actionLocks?.Arm(ActionLockKind.ItemUse, PickupHoldSeconds);
        _actionLocks?.Arm(ActionLockKind.Navigation, PickupHoldSeconds);

        if (pickup.Status == PluginItemCommandStatus.Busy)
        {
            // Busy is the client's own one-request-at-a-time gate: it never
            // looked at this item, so this is not an attempt on it. Sitting
            // out the hold and asking again is the whole answer.
            if (_waitingItem == chosen.Item.ObjectId)
                _waitingPickupAccepted = false;
            Status = $"Waiting to take {chosen.Item.Name}…";
            return true;
        }

        // The pull is counted when it is issued, as the reference counts it:
        // an item that is still in the corpse on the next turn is pulled
        // again, until the profile's attempt ceiling drops it.
        IncrementAttempt(chosen.Item.ObjectId);

        if (!pickup.Accepted)
        {
            // The client decided against sending this one, and nothing this
            // rule does on the next heartbeat changes that — a pack with no
            // room is still full a tenth of a second later. So the corpse is
            // let go of instead of asked again, and set aside rather than
            // recorded as looted: nothing was taken from it.
            if (_waitingItem == chosen.Item.ObjectId)
                _waitingPickupAccepted = false;
            Status = $"Pickup refused: {chosen.Item.Name} ({pickup.Status}).";
            GiveUpOnCorpse(
                _activeCorpse,
                string.IsNullOrWhiteSpace(pickup.Notice)
                    ? $"{chosen.Item.Name} was refused ({pickup.Status})"
                    : $"{chosen.Item.Name} was refused: {pickup.Notice}");
            return true;
        }

        if (_waitingItem == 0u)
        {
            _waitingItem = chosen.Item.ObjectId;
            _waitingName = chosen.Item.Name;
            _waitingAction = chosen.Decision.Action;
            _waitingQuantity = Math.Max(1, chosen.Item.StackSize);
            _waitingItemSnapshot = chosen.Item;
            _waitingClassifierId = chosen.Decision.ClassifierId;
            if (chosen.Decision.Action == LootAction.KeepUpTo)
            {
                _pendingByName.TryGetValue(chosen.Item.Name, out int pending);
                _pendingByName[chosen.Item.Name] =
                    pending + _waitingQuantity;
            }
        }
        _waitingPickupAccepted = true;
        _waitingPickupCompletionRevision = loot.LastInventoryCompletion.Revision;
        Status = $"Looting {chosen.Item.Name} ({chosen.Decision.RuleName})…";
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"LootPickup: taking {chosen.Item.Name} x{_waitingQuantity} "
                + $"({chosen.Decision.Action}, {chosen.Decision.RuleName})");
        return true;
    }

    /// <summary>Store a decision and say what it was, or why there was none.</summary>
    private void RecordDecision(
        in PluginInventoryItem item,
        LootDecision? decision,
        in LootRefusal refusal)
    {
        _decisions[item.ObjectId] = decision;
        Log?.Invoke(
            MacroLogChannel.Loot,
            decision is { } chosen
                ? $"LootDecision: {item.Name} -> {chosen.Action} ({chosen.RuleName})"
                : $"LootDecision: {item.Name} -> {refusal.Describe()}");
    }

    /// <summary>
    /// The pull issued last turn, judged by the corpse: an item that is no
    /// longer in the container was taken, and an item that still is will be
    /// pulled again by the turn that follows. The reference keeps no
    /// completion of its own for a pull; the container is the answer.
    /// </summary>
    private void ObservePickup(
        ILootAutomation loot,
        IReadOnlyList<PluginInventoryItem> contents)
    {
        if (_waitingItem == 0u)
            return;
        PluginInventoryCompletion completion = loot.LastInventoryCompletion;
        if (_waitingPickupAccepted
            && completion.Revision > _waitingPickupCompletionRevision
            && completion.Kind == PluginInventoryCommandKind.Pickup
            && completion.SourceObjectId == _waitingItem
            && completion.WeenieError != 0u)
        {
            _waitingPickupAccepted = false;
        }
        bool stillInCorpse = contents.Any(item => item.ObjectId == _waitingItem);
        if (!stillInCorpse)
        {
            CompleteWaitingPickup();
            return;
        }
        else
        {
            Status = $"Retrying {_waitingName}.";
            return;
        }
    }

    private void CompleteWaitingPickup()
    {
        // An item taken for its mana keeps what it was taken as unless the
        // second look, now that it is in hand, says otherwise: somebody wrote
        // on it or tinkered it, and it is left alone. A description that
        // cannot be read yet is not that verdict — the item arrives before
        // its description does — and treating it as one dropped the item's
        // whole classification for good, so that donor was never drained.
        // The stone it reserves is released where the stones are counted,
        // once the writing on it is known.
        if (_waitingAction != LootAction.ManaTank
            || CaptureOwnedProperties(_waitingItem) is not { } written
            || ManaStoneTransferPlanner.IsDrainableInHand(written))
        {
            _classifiedOwnedItems[_waitingItem] = _waitingAction;
        }
        if (_waitingClassifierId.Length != 0)
        {
            _externalClassifierByItem[_waitingItem] = _waitingClassifierId;
            _host.LootClassifiers.TryNotifyLooted(
                _waitingClassifierId,
                new PluginLootedItem(
                    _waitingItemSnapshot,
                    (PluginLootAction)(int)_waitingAction));
        }
        _decisions.Remove(_waitingItem);
        Status = $"Looted {_waitingName}.";
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"LootPickup: took {_waitingName} ({_waitingAction})");
        _itemAttempts.Remove(_waitingItem);
        _abandonedItems.Remove(_waitingItem);
        if (_waitingAction == LootAction.Read
            && _waitingItemSnapshot.SpellId != 0u)
        {
            _pendingScrollReads.TryAdd(
                _waitingItemSnapshot.SpellId,
                _waitingItem);
        }
        ReleaseWaitingReservation();
        ClearWaitingItem();
    }

    /// <summary>
    /// Lets go of the item the pass was waiting on and says why. Every way an
    /// item leaves the pass without arriving goes through here, so a run
    /// always has exactly one line for it.
    /// </summary>
    private void AbandonWaitingItem(string reason)
    {
        if (_waitingItem != 0u)
        {
            Log?.Invoke(
                MacroLogChannel.Loot,
                $"LootPickup: abandoned {_waitingName}: {reason}");
        }
        ReleaseWaitingReservation();
        ClearWaitingItem();
    }

    private void ReleaseWaitingReservation()
    {
        if (_waitingAction != LootAction.KeepUpTo
            || !_pendingByName.TryGetValue(_waitingName, out int pending))
        {
            return;
        }
        if (pending <= _waitingQuantity)
            _pendingByName.Remove(_waitingName);
        else
            _pendingByName[_waitingName] = pending - _waitingQuantity;
    }

    private void ClearWaitingItem()
    {
        _waitingItem = 0u;
        _waitingName = string.Empty;
        _waitingAction = LootAction.NoLoot;
        _waitingQuantity = 0;
        _waitingItemSnapshot = default;
        _waitingClassifierId = string.Empty;
        _waitingPickupAccepted = false;
        _waitingPickupCompletionRevision = 0L;
    }

    private bool ContinueSalvage(bool canAct)
    {
        IItemAutomation items = _host.Automation.Items;
        IReadOnlyList<PluginInventoryItem> owned = items.CaptureOwnedItems();
        if (_salvagePendingItem != 0u)
        {
            if (!owned.Any(item => item.ObjectId == _salvagePendingItem))
            {
                RemoveClassifiedOwned(_salvagePendingItem);
                Status = $"Salvaged {_salvagePendingName}.";
                _salvagePendingItem = 0u;
                _salvagePendingName = string.Empty;
                _salvageAttempts = 0;
                _stateAge = 0d;
                return true;
            }
            if (_stateAge < PickupTimeoutSeconds)
            {
                Status = $"Salvaging {_salvagePendingName}…";
                return true;
            }
            _salvagePendingItem = 0u;
            _salvagePendingName = string.Empty;
            _stateAge = 0d;
            _salvageAttempts++;
        }

        var ownedById = owned.ToDictionary(static item => item.ObjectId);
        foreach (uint stale in _classifiedOwnedItems
            .Where(entry => entry.Value == LootAction.Salvage
                && !ownedById.ContainsKey(entry.Key))
            .Select(static entry => entry.Key)
            .ToArray())
        {
            RemoveClassifiedOwned(stale);
        }
        uint sourceId = _classifiedOwnedItems
            .Where(static entry => entry.Value == LootAction.Salvage)
            .Select(static entry => entry.Key)
            .FirstOrDefault(ownedById.ContainsKey);
        if (sourceId == 0u)
            return false;
        if (!canAct || !items.IsAvailable || items.IsBusy)
            return true;

        const uint tinkeringTool = 0x20000000u;
        PluginInventoryItem tool = owned.FirstOrDefault(
            item => (item.ItemType & tinkeringTool) != 0u);
        if (tool.ObjectId == 0u)
        {
            Status = "Salvage action is waiting for a salvage tool.";
            return false;
        }

        PluginInventoryItem source = ownedById[sourceId];
        PluginItemCommandResult result = items.Salvage(tool.ObjectId, [sourceId]);
        if (!result.Accepted)
        {
            Status = result.Status == PluginItemCommandStatus.Busy
                ? "Waiting to salvage…"
                : $"Could not salvage {source.Name}.";
            return result.Status == PluginItemCommandStatus.Busy;
        }
        _salvagePendingItem = sourceId;
        _salvagePendingName = source.Name;
        _stateAge = 0d;
        Status = $"Salvaging {source.Name}…";
        return true;
    }

    private bool ContinueSell(bool canAct)
    {
        IItemAutomation items = _host.Automation.Items;
        IReadOnlyList<PluginInventoryItem> owned = items.CaptureOwnedItems();
        if (_sellPendingItem != 0u)
        {
            if (!owned.Any(item => item.ObjectId == _sellPendingItem))
            {
                RemoveClassifiedOwned(_sellPendingItem);
                Status = $"Sold {_sellPendingName}.";
                _sellPendingItem = 0u;
                _sellPendingName = string.Empty;
                _stateAge = 0d;
                return true;
            }
            if (_stateAge < PickupTimeoutSeconds)
            {
                Status = $"Selling {_sellPendingName}…";
                return true;
            }
            _sellPendingItem = 0u;
            _sellPendingName = string.Empty;
            _stateAge = 0d;
        }

        var ownedById = owned.ToDictionary(static item => item.ObjectId);
        foreach (uint stale in _classifiedOwnedItems
            .Where(entry => entry.Value == LootAction.Sell
                && !ownedById.ContainsKey(entry.Key))
            .Select(static entry => entry.Key)
            .ToArray())
        {
            RemoveClassifiedOwned(stale);
        }
        uint sourceId = _classifiedOwnedItems
            .Where(static entry => entry.Value == LootAction.Sell)
            .Select(static entry => entry.Key)
            .FirstOrDefault(ownedById.ContainsKey);
        if (sourceId == 0u)
            return false;
        if (items.ActiveVendorObjectId == 0u)
        {
            Status = "Sell loot is queued until a vendor is open.";
            return false;
        }
        if (!canAct || !items.IsAvailable || items.IsBusy)
            return true;

        PluginInventoryItem source = ownedById[sourceId];
        PluginItemCommandResult result = items.Sell(sourceId);
        if (!result.Accepted)
        {
            Status = result.Status == PluginItemCommandStatus.Busy
                ? "Waiting to sell…"
                : result.Notice ?? $"Could not sell {source.Name}.";
            return result.Status == PluginItemCommandStatus.Busy;
        }
        _sellPendingItem = sourceId;
        _sellPendingName = source.Name;
        _stateAge = 0d;
        Status = $"Selling {source.Name}…";
        return true;
    }

    private bool HasManaStoneTransfer()
    {
        if (!_classifiedOwnedItems.Values.Contains(LootAction.ManaTank))
            return false;
        return ManaStoneTransferPlanner.Plan(
            _host.Automation.Items.CaptureOwnedItems(),
            _classifiedOwnedItems,
            _settings.ManaTankMinimumMana,
            item => IsProfiledManaStone(item),
            CanUseManaStone,
            CanDrainManaItem,
            CaptureOwnedProperties) is not null;
    }

    /// <summary>
    /// Whether a stone may be filled. Empty or charged comes with the object
    /// itself, so no appraisal is waited for: asking for one benched every
    /// stone a character simply carries -- nothing appraises those -- and a
    /// run's worth of items taken for their mana sat in the pack undrained.
    /// A pair whose last use could not be accounted for is left alone.
    /// </summary>
    private bool CanUseManaStone(uint objectId) =>
        !_uncertainManaItems.Contains(objectId);

    /// <summary>
    /// Whether an item may be emptied. How much mana it holds is known only
    /// from an appraisal, so this one waits for one.
    /// </summary>
    private bool CanDrainManaItem(uint objectId) =>
        !_uncertainManaItems.Contains(objectId)
        && ConfiguredSupplyReadiness.IsAssessed(_host.Automation, objectId);

    private PluginItemProperties? CaptureOwnedProperties(uint objectId) =>
        _host.Automation.Items.TryCaptureProperties(
            objectId,
            out PluginItemProperties properties)
            ? properties
            : null;

    private bool ContinueManaStoneTransfer(bool canAct)
    {
        IItemAutomation items = _host.Automation.Items;
        if (_manaTransfer is { } pending)
        {
            ManaFillOutcome outcome = ManaStoneFillConfirmation.Observe(
                pending, items.CaptureOwnedItems(), items.LastCompletion,
                _manaTransferRevision, _stateAge >= PickupTimeoutSeconds);
            if (outcome == ManaFillOutcome.Waiting)
            {
                Status = $"Waiting for {pending.StoneName} charge confirmation…";
                return true;
            }
            if (outcome == ManaFillOutcome.Confirmed)
            {
                RemoveClassifiedOwned(pending.TankObjectId);
                Status = $"Filled {pending.StoneName}.";
                Log?.Invoke(
                    MacroLogChannel.Loot,
                    $"ManaDrain: filled {pending.StoneName} from {pending.TankName}");
                _host.Automation.Objects.Identify(pending.StoneObjectId);
            }
            else
            {
                // Keep the donor reserved, but do not repeat an uncertain destructive use.
                _uncertainManaItems.Add(pending.StoneObjectId);
                _uncertainManaItems.Add(pending.TankObjectId);
                Status = $"Mana fill unconfirmed; holding {pending.StoneName} and {pending.TankName}.";
                Log?.Invoke(
                    MacroLogChannel.Loot,
                    $"ManaDrain: no answer for {pending.StoneName} on " +
                    $"{pending.TankName}; leaving both alone");
            }
            _host.Log.Info($"Mana stone fill: {outcome}, source={pending.StoneName} " +
                $"(0x{pending.StoneObjectId:X8}), donor={pending.TankName} " +
                $"(0x{pending.TankObjectId:X8}), error=0x{items.LastCompletion.WeenieError:X8}");
            ReleaseManaTransferHold();
            _manaTransfer = null;
            _manaTransferRevision = 0L;
            _stateAge = 0d;
            return true;
        }
        if (!canAct || !items.IsAvailable || items.IsBusy)
            return true;

        ManaStoneTransferPlan? plan = ManaStoneTransferPlanner.Plan(
            items.CaptureOwnedItems(),
            _classifiedOwnedItems,
            _settings.ManaTankMinimumMana,
            item => IsProfiledManaStone(item),
            CanUseManaStone,
            CanDrainManaItem,
            CaptureOwnedProperties);
        if (plan is not { } next)
            return false;
        PluginItemCommandResult result = items.Apply(
            next.StoneObjectId,
            next.TankObjectId);
        if (!result.Accepted)
        {
            if (result.Status == PluginItemCommandStatus.Busy)
            {
                Status = "Waiting to fill mana stone…";
                return true;
            }
            // Refused outright: nothing was sent and nothing changed, so the
            // answer is about this item and asking again would only produce
            // it again. It stops being one this run means to empty, which
            // ends the retry and hands the stone it was holding back to the
            // next item worth draining.
            _uncertainManaItems.Add(next.TankObjectId);
            RemoveClassifiedOwned(next.TankObjectId);
            Status = $"Could not use {next.StoneName} on {next.TankName}.";
            Log?.Invoke(
                MacroLogChannel.Loot,
                $"ManaDrain: could not empty {next.TankName} into " +
                $"{next.StoneName} ({result.Status}); leaving it alone");
            return false;
        }
        _host.Log.Info($"Mana stone fill request: source={next.StoneName} (0x{next.StoneObjectId:X8}), donor={next.TankName} (0x{next.TankObjectId:X8})");
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"ManaDrain: emptying {next.TankName} into {next.StoneName}");
        _manaTransfer = next;
        _manaTransferRevision = items.LastCompletion.Revision;
        _stateAge = 0d;
        // The drain is an item use like any other and the character's hands
        // are full until the server answers for it, so the item slot is held
        // the same way every other rule that consumes an item holds it.
        _actionLocks?.Arm(ActionLockKind.ItemUse, ItemUseLock.TransactionSeconds);
        _manaTransferHoldUntil =
            (_actionLocks?.Now ?? 0d) + ItemUseLock.TransactionSeconds;
        Status = $"Filling {next.StoneName} from {next.TankName}…";
        return true;
    }

    /// <summary>
    /// Gives back the item slot the outstanding drain took, but only while
    /// the window it armed is still running.
    /// </summary>
    private void ReleaseManaTransferHold()
    {
        if (_actionLocks is { } locks && locks.Now < _manaTransferHoldUntil)
            locks.Release(ActionLockKind.ItemUse);
        _manaTransferHoldUntil = 0d;
    }

    private bool HasSalvageBagCombine() => SalvageBagCombinePlanner.Plan(
        _host.Automation.Items.CaptureOwnedItems(),
        _abandonedCombineBags,
        _settings.SalvageCombine) is not null;

    private bool ContinueSalvageBagCombine(bool canAct)
    {
        IItemAutomation items = _host.Automation.Items;
        IReadOnlyList<PluginInventoryItem> owned = items.CaptureOwnedItems();
        if (_combinePending is { } pending)
        {
            bool allExist = pending.ObjectIds.All(
                id => owned.Any(item => item.ObjectId == id));
            if (!allExist)
            {
                foreach (uint id in pending.ObjectIds)
                    _combineAttempts.Remove(id);
                _combinePending = null;
                _stateAge = 0d;
                Status = "Combined salvage bags.";
                return true;
            }
            if (_stateAge < PickupTimeoutSeconds)
            {
                Status = "Combining salvage bags…";
                return true;
            }

            _combineAttempts.TryGetValue(pending.FirstObjectId, out int attempts);
            attempts++;
            _combineAttempts[pending.FirstObjectId] = attempts;
            if (attempts > 40)
            {
                _abandonedCombineBags.Add(pending.FirstObjectId);
                _combineAttempts.Remove(pending.FirstObjectId);
                Status = $"Abandoned bugged salvage bag {pending.FirstName}.";
            }
            else
            {
                Status = $"Retrying salvage combine ({attempts}/40)…";
            }
            _combinePending = null;
            _stateAge = 0d;
            return attempts <= 40;
        }
        if (!canAct || !items.IsAvailable || items.IsBusy)
            return true;

        SalvageBagCombinePlan? plan = SalvageBagCombinePlanner.Plan(
            owned,
            _abandonedCombineBags,
            _settings.SalvageCombine);
        if (plan is not { } next)
            return false;
        const uint tinkeringTool = 0x20000000u;
        PluginInventoryItem tool = owned.FirstOrDefault(
            item => (item.ItemType & tinkeringTool) != 0u);
        if (tool.ObjectId == 0u)
        {
            Status = "Salvage combine is waiting for a salvage tool.";
            return false;
        }
        PluginItemCommandResult result = items.Salvage(
            tool.ObjectId,
            next.ObjectIds);
        if (!result.Accepted)
        {
            Status = result.Status == PluginItemCommandStatus.Busy
                ? "Waiting to combine salvage…"
                : "Could not combine salvage bags.";
            return result.Status == PluginItemCommandStatus.Busy;
        }
        _combinePending = next;
        _stateAge = 0d;
        Status = "Combining salvage bags…";
        return true;
    }

    /// <summary>
    /// A corpse item is appraised only when the answer depends on it.
    /// Three cases always appraise: an external classifier (which has no
    /// early-decision contract), the player's own death corpse, and a magical
    /// candidate while a spare mana stone is held, because only an appraisal
    /// can say whether that candidate is a tank worth draining.
    /// </summary>
    private bool NeedsIdentify(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<PluginInventoryItem> owned)
    {
        if (!string.IsNullOrWhiteSpace(_settings.ExternalClassifierId))
            return true;
        if (_activeCorpseIsOwnDeath)
            return true;
        if (VtankLootRequirementEvaluator.IsMagical(item, properties)
            && SpareManaStoneCount(owned) > 0)
        {
            return true;
        }
        if (LootRuleEngine.NeedsIdentify(
            item,
            properties,
            _settings.Rules,
            _host))
        {
            return true;
        }
        return false;
    }

    private int SpareManaStoneCount(IReadOnlyList<PluginInventoryItem> owned)
    {
        const uint magicalEffect = 0x00000001u;
        int stones = owned.Count(item =>
            !item.IsEquipped
            && (item.Effects & magicalEffect) == 0u
            && IsProfiledManaStone(item));
        HashSet<uint> ownedIds = owned
            .Select(static item => item.ObjectId)
            .ToHashSet();
        int queued = _classifiedOwnedItems.Count(entry =>
            entry.Value == LootAction.ManaTank
            && ownedIds.Contains(entry.Key)
            && ReservesAManaStone(entry.Key));
        queued += PendingDecisionCount(LootAction.ManaTank);
        return Math.Max(0, stones - queued);
    }

    /// <summary>
    /// Whether a tank the character is carrying still holds a stone back. An
    /// item whose description has since arrived and says somebody wrote on it
    /// or tinkered it is left alone, so it stops reserving anything. One
    /// whose description still cannot be read keeps its reservation: it is
    /// what the stone is being saved for.
    /// </summary>
    private bool ReservesAManaStone(uint objectId) =>
        CaptureOwnedProperties(objectId) is not { } written
        || ManaStoneTransferPlanner.IsDrainableInHand(written);

    private void IncrementAttempt(uint objectId)
    {
        _itemAttempts.TryGetValue(objectId, out int attempts);
        _itemAttempts[objectId] = attempts + 1;
    }

    private LootDecision? DecideItem(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<PluginInventoryItem> owned,
        IReadOnlyDictionary<string, int> pending,
        out LootRefusal refusal)
    {
        LootDecision? decision = string.IsNullOrWhiteSpace(
            _settings.ExternalClassifierId)
            ? LootRuleEngine.Decide(
                item,
                properties,
                _settings.Rules,
                owned,
                out refusal,
                pending,
                _host)
            : DecideWithExternalClassifier(
                item, properties, owned, pending, out refusal);
        if (decision is not null || !IsReadableUnknownScroll(item))
        {
            if (decision is not null)
                return decision;
        }
        else
        {
            return new LootDecision(
                LootAction.Read,
                Priority: 0,
                RuleIndex: int.MaxValue,
                RuleName: "Unknown Scroll");
        }

        LootAction? manaAction = AutomaticManaAction(item, owned);
        return manaAction is { } action
            ? new LootDecision(
                action,
                Priority: 0,
                RuleIndex: int.MaxValue,
                RuleName: action.ToString())
            : null;
    }

    private LootDecision? DecideWithExternalClassifier(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<PluginInventoryItem> owned,
        IReadOnlyDictionary<string, int> pending,
        out LootRefusal refusal)
    {
        refusal = LootRefusal.NoRule;
        var context = new PluginLootClassificationContext(
            item,
            properties,
            owned);
        if (!_host.LootClassifiers.TryClassify(
                _settings.ExternalClassifierId,
                context,
                out PluginLootClassification classification)
            || !classification.Matched
            || !Enum.IsDefined(classification.Action))
        {
            return null;
        }

        string ruleName = string.IsNullOrWhiteSpace(classification.RuleName)
            ? _settings.ExternalClassifierId
            : classification.RuleName.Trim();
        LootAction action = (LootAction)(int)classification.Action;
        if (action == LootAction.NoLoot)
        {
            refusal = new LootRefusal(LootRefusalKind.RuleDeclined, ruleName);
            return null;
        }
        if (action == LootAction.KeepUpTo)
        {
            int limit = Math.Max(0, classification.KeepCount);
            string itemName = item.Name;
            int held = owned
                .Where(ownedItem => string.Equals(
                    ownedItem.Name,
                    itemName,
                    StringComparison.OrdinalIgnoreCase))
                .Sum(static ownedItem => Math.Max(1, ownedItem.StackSize));
            if (pending.TryGetValue(itemName, out int pendingCount))
                held += pendingCount;
            if (held >= limit)
            {
                refusal = new LootRefusal(
                    LootRefusalKind.RuleAtCap, ruleName, limit, held);
                return null;
            }
        }

        return new LootDecision(
            action,
            classification.Priority,
            RuleIndex: -1,
            RuleName: ruleName,
            ClassifierId: _settings.ExternalClassifierId);
    }

    private void RemoveClassifiedOwned(uint objectId)
    {
        _classifiedOwnedItems.Remove(objectId);
        if (!_externalClassifierByItem.Remove(objectId, out string? classifierId))
            return;
        _host.LootClassifiers.TryNotifyItemRemoved(classifierId, objectId);
    }

    private void PruneRemovedExternalItems()
    {
        if (_externalClassifierByItem.Count == 0)
            return;
        HashSet<uint> owned = _host.Automation.Items.CaptureOwnedItems()
            .Select(static item => item.ObjectId)
            .ToHashSet();
        foreach (uint removed in _externalClassifierByItem.Keys
            .Where(objectId => !owned.Contains(objectId))
            .ToArray())
        {
            RemoveClassifiedOwned(removed);
        }
    }

    private LootAction? AutomaticManaAction(
        in PluginInventoryItem item,
        IReadOnlyList<PluginInventoryItem> owned)
    {
        int desired = ProfileNamesAManaStone
            ? Math.Clamp(_settings.ManaStoneLootCount, 0, 100)
            : 0;
        int stones = owned.Count(ownedItem => IsProfiledManaStone(ownedItem));
        stones += PendingDecisionCount(LootAction.ManaStone);
        if (IsProfiledManaStone(item) && stones < desired)
            return LootAction.ManaStone;
        if (SpareManaStoneCount(owned) > 0
            && ManaStoneTransferPlanner.IsDonor(item, _settings.ManaTankMinimumMana))
        {
            return LootAction.ManaTank;
        }
        return null;
    }

    private int PendingDecisionCount(LootAction action) =>
        _decisions.Values.Count(decision => decision?.Action == action);

    private bool IsReadableUnknownScroll(in PluginInventoryItem item) =>
        ScrollReading.IsEligible(
            _host,
            _settings,
            item,
            _pendingScrollReads,
            commit: true);

    private void ResetTransient()
    {
        if (_manaTransfer is { } pendingFill)
        {
            _uncertainManaItems.Add(pendingFill.StoneObjectId);
            _uncertainManaItems.Add(pendingFill.TankObjectId);
        }
        _activeCorpse = 0u;
        _activeCorpseSawContents = false;
        _activeCorpseIsOwnDeath = false;
        _abandonedCorpse = 0u;
        AbandonWaitingItem("the pass stopped");
        _awaitingAppraisal = 0u;
        _awaitingCorpseAppraisal = 0u;
        _lastCorpseDescriptionRequest = 0u;
        _salvagePendingItem = 0u;
        _salvagePendingName = string.Empty;
        _salvageAttempts = 0;
        _sellPendingItem = 0u;
        _sellPendingName = string.Empty;
        _manaTransfer = null;
        _manaTransferRevision = 0L;
        _combinePending = null;
        _stateAge = 0d;
    }
}

internal sealed class LootRuleExpression
{
    private readonly Clause[][] _groups;

    private LootRuleExpression(Clause[][] groups) => _groups = groups;

    public static LootRuleExpression Compile(string source)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(source);
        string normalized = source.Trim();
        if (normalized is "*" || normalized.Equals(
                "DEFAULT",
                StringComparison.OrdinalIgnoreCase))
        {
            return new LootRuleExpression([[]]);
        }

        Clause[][] groups = Split(normalized, "||")
            .Select(group => Split(group, "&&")
                .Select(ParseClause)
                .ToArray())
            .ToArray();
        if (groups.Length == 0 || groups.Any(static group => group.Length == 0))
            throw new FormatException("Loot expression contains an empty condition.");
        return new LootRuleExpression(groups);
    }

    public bool IsMatch(
        in PluginInventoryItem item,
        in PluginItemProperties properties)
    {
        if (_groups.Length == 1 && _groups[0].Length == 0)
            return true;
        foreach (Clause[] group in _groups)
        {
            bool all = true;
            foreach (Clause clause in group)
            {
                if (!clause.IsMatch(item, properties))
                {
                    all = false;
                    break;
                }
            }
            if (all)
                return true;
        }
        return false;
    }

    private static Clause ParseClause(string text)
    {
        foreach (string operation in new[] { ">=", "<=", "!=", "==", "~=", ">", "<" })
        {
            int offset = FindOutsideQuotes(text, operation);
            if (offset < 0)
                continue;
            string field = text[..offset].Trim();
            string expected = Unquote(text[(offset + operation.Length)..].Trim());
            if (field.Length == 0 || expected.Length == 0)
                throw new FormatException($"Invalid loot condition '{text.Trim()}'.");
            return new Clause(field, operation, expected);
        }
        throw new FormatException(
            $"Loot condition '{text.Trim()}' needs a comparison operator.");
    }

    private static string[] Split(string source, string delimiter)
    {
        var result = new List<string>();
        int start = 0;
        char quote = '\0';
        for (int index = 0; index <= source.Length - delimiter.Length; index++)
        {
            char current = source[index];
            if (current is '\'' or '"')
                quote = quote == '\0' ? current : quote == current ? '\0' : quote;
            if (quote != '\0'
                || !source.AsSpan(index).StartsWith(
                    delimiter,
                    StringComparison.Ordinal))
            {
                continue;
            }
            result.Add(source[start..index].Trim());
            start = index + delimiter.Length;
            index += delimiter.Length - 1;
        }
        result.Add(source[start..].Trim());
        return result.ToArray();
    }

    private static int FindOutsideQuotes(string source, string operation)
    {
        char quote = '\0';
        for (int index = 0; index <= source.Length - operation.Length; index++)
        {
            char current = source[index];
            if (current is '\'' or '"')
                quote = quote == '\0' ? current : quote == current ? '\0' : quote;
            if (quote == '\0'
                && source.AsSpan(index).StartsWith(
                    operation,
                    StringComparison.Ordinal))
            {
                return index;
            }
        }
        return -1;
    }

    private static string Unquote(string value) => value.Length >= 2
        && ((value[0] == '"' && value[^1] == '"')
            || (value[0] == '\'' && value[^1] == '\''))
            ? value[1..^1]
            : value;

    private readonly record struct Clause(
        string Field,
        string Operation,
        string Expected)
    {
        public bool IsMatch(
            in PluginInventoryItem item,
            in PluginItemProperties properties)
        {
            Value actual = Resolve(item, properties, Field);
            if (Operation == "~=")
            {
                if (actual.Kind != ValueKind.Text)
                    throw new FormatException("~= is only valid for text fields.");
                return actual.Text.Contains(Expected, StringComparison.OrdinalIgnoreCase);
            }
            int comparison = actual.Kind switch
            {
                ValueKind.Number => actual.Number.CompareTo(ParseNumber(Expected)),
                ValueKind.Boolean => actual.Boolean.CompareTo(ParseBoolean(Expected)),
                _ => string.Compare(
                    actual.Text,
                    Expected,
                    StringComparison.OrdinalIgnoreCase),
            };
            return Operation switch
            {
                "==" => comparison == 0,
                "!=" => comparison != 0,
                ">" => comparison > 0,
                "<" => comparison < 0,
                ">=" => comparison >= 0,
                "<=" => comparison <= 0,
                _ => false,
            };
        }

        private static Value Resolve(
            in PluginInventoryItem item,
            in PluginItemProperties properties,
            string field)
        {
            string key = field.Trim().ToLowerInvariant();
            return key switch
            {
                "name" => Value.FromText(item.Name),
                "wcid" or "typeid" => Value.FromNumber(item.WeenieClassId),
                "itemtype" or "type" => Value.FromNumber(item.ItemType),
                "stack" or "stacksize" => Value.FromNumber(item.StackSize),
                "maxstack" => Value.FromNumber(item.MaximumStackSize),
                "value" => Value.FromNumber(item.Value),
                "burden" => Value.FromNumber(item.Burden),
                "workmanship" => Value.FromNumber(item.Workmanship),
                "material" => Value.FromNumber(item.MaterialType),
                _ => ResolveRaw(key, properties),
            };
        }

        private static Value ResolveRaw(
            string field,
            in PluginItemProperties properties)
        {
            if (!TryRawKey(field, out string table, out uint key))
                throw new FormatException($"Unknown loot field '{field}'.");
            return table switch
            {
                "int" => Value.FromNumber(
                    properties.Ints?.TryGetValue(key, out int value) == true
                        ? value : 0),
                "int64" => Value.FromNumber(
                    properties.Int64s?.TryGetValue(key, out long value) == true
                        ? value : 0),
                "bool" => Value.FromBoolean(
                    properties.Bools?.TryGetValue(key, out bool value) == true
                        && value),
                "float" => Value.FromNumber(
                    properties.Floats?.TryGetValue(key, out double value) == true
                        ? value : 0d),
                "string" => Value.FromText(
                    properties.Strings?.TryGetValue(key, out string? value) == true
                        ? value : string.Empty),
                "did" => Value.FromNumber(
                    properties.DataIds?.TryGetValue(key, out uint value) == true
                        ? value : 0u),
                "iid" => Value.FromNumber(
                    properties.InstanceIds?.TryGetValue(key, out uint value) == true
                        ? value : 0u),
                _ => throw new FormatException($"Unknown raw table '{table}'."),
            };
        }

        private static bool TryRawKey(
            string field,
            out string table,
            out uint key)
        {
            int open = field.IndexOf('[', StringComparison.Ordinal);
            int close = field.LastIndexOf(']');
            table = open > 0 ? field[..open] : string.Empty;
            key = 0u;
            return open > 0 && close == field.Length - 1
                && uint.TryParse(
                    field.AsSpan(open + 1, close - open - 1),
                    NumberStyles.Integer,
                    CultureInfo.InvariantCulture,
                    out key);
        }

        private static double ParseNumber(string value) =>
            double.TryParse(
                value,
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out double parsed)
                ? parsed
                : throw new FormatException($"'{value}' is not a number.");

        private static bool ParseBoolean(string value) =>
            bool.TryParse(value, out bool parsed)
                ? parsed
                : throw new FormatException($"'{value}' is not true or false.");
    }

    private enum ValueKind
    {
        Number,
        Text,
        Boolean,
    }

    private readonly record struct Value(
        ValueKind Kind,
        double Number,
        string Text,
        bool Boolean)
    {
        public static Value FromNumber(double value) =>
            new(ValueKind.Number, value, string.Empty, false);
        public static Value FromText(string value) =>
            new(ValueKind.Text, 0d, value ?? string.Empty, false);
        public static Value FromBoolean(bool value) =>
            new(ValueKind.Boolean, 0d, string.Empty, value);
    }
}
