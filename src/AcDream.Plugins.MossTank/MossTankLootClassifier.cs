using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Registers MossTank's own loot-rule engine as a loot classifier so other
/// plugins can evaluate an item against MossTank's live rules, or against a
/// named stored profile, without depending on MossTank's UI surface.
/// </summary>
internal sealed class MossTankLootClassifier : IPluginLootClassifier
{
    /// <summary>
    /// The client's long-description string property. It is written only
    /// once an item has been identified, so its absence is the signal that
    /// an item is still unappraised.
    /// </summary>
    private const uint LongDescPropertyId = 16u;

    /// <summary>
    /// Requirement kinds that can only be evaluated with appraisal data:
    /// spell-name and appraised-spell-count checks, and every buffed or
    /// tinkered rating (they read properties the server sends only after
    /// identification).
    /// </summary>
    private static readonly HashSet<int> AppraisalDependentRequirementTypes =
    [
        0, 8, 9, 10, 2000, 2001, 2003, 2005, 2006, 2007,
    ];

    private readonly IPluginHost _host;
    private readonly Func<IReadOnlyList<LootRule>> _liveRules;
    private readonly Dictionary<string, VtankLootProfile?> _profileCache =
        new(StringComparer.OrdinalIgnoreCase);

    internal MossTankLootClassifier(
        IPluginHost host,
        Func<IReadOnlyList<LootRule>> liveRules)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _liveRules = liveRules ?? throw new ArgumentNullException(nameof(liveRules));
    }

    public PluginLootClassification Classify(
        in PluginLootClassificationContext context) =>
        DecideAgainst(_liveRules(), context);

    public bool NeedsIdentification(in PluginLootClassificationContext context)
    {
        if (context.Properties.Strings.ContainsKey(LongDescPropertyId))
            return false;

        foreach (LootRule rule in _liveRules())
        {
            foreach (VtankLootRequirement requirement in rule.VtankRequirements)
            {
                if (AppraisalDependentRequirementTypes.Contains(requirement.Type))
                    return true;
            }
        }
        return false;
    }

    public bool TryClassifyWithProfile(
        string profileName,
        in PluginLootClassificationContext context,
        out PluginLootClassification classification)
    {
        if (!TryLoadProfile(profileName, out VtankLootProfile? profile)
            || profile is null)
        {
            classification = default;
            return false;
        }
        classification = DecideAgainst(profile.Rules, context);
        return true;
    }

    private PluginLootClassification DecideAgainst(
        IReadOnlyList<LootRule> rules,
        in PluginLootClassificationContext context)
    {
        LootDecision? decision = LootRuleEngine.Decide(
            context.Item,
            context.Properties,
            rules,
            context.OwnedItems,
            pendingByName: null,
            _host);
        if (decision is not { } found)
            return default;

        // MossTank's own vocabulary has two mana-transfer actions the
        // public PluginLootAction enum does not model; a rule that resolves
        // to one of those cannot be reported through this surface.
        if (found.Action is LootAction.ManaStone or LootAction.ManaTank)
            return default;

        return new PluginLootClassification(
            Matched: true,
            Action: (PluginLootAction)(int)found.Action,
            RuleName: found.RuleName,
            Priority: found.Priority,
            KeepCount: found.RuleIndex >= 0 && found.RuleIndex < rules.Count
                ? rules[found.RuleIndex].KeepCount
                : 0);
    }

    private bool TryLoadProfile(string? profileName, out VtankLootProfile? profile)
    {
        string normalized = (profileName ?? string.Empty).Trim();
        if (normalized.EndsWith(".utl", StringComparison.OrdinalIgnoreCase))
            normalized = normalized[..^4];
        if (normalized.Length == 0)
        {
            profile = null;
            return false;
        }

        if (_profileCache.TryGetValue(normalized, out VtankLootProfile? cached))
        {
            profile = cached;
            return cached is not null;
        }

        string? text = _host.VtankProfiles.IsAvailable
            ? _host.VtankProfiles.ReadText(normalized + ".utl")
            : null;
        if (text is null
            || !VtankLootProfileSerializer.TryRead(
                text, out VtankLootProfile parsed, out _))
        {
            _profileCache[normalized] = null;
            profile = null;
            return false;
        }

        _profileCache[normalized] = parsed;
        profile = parsed;
        return true;
    }
}
