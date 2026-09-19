using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal interface IBuffRuleHost
{
    bool MacroEnabled { get; }

    bool HasTarget { get; }

    CombatModeGate Gate { get; }

    SpellCastTracker CastTracker { get; }

    void SetStatus(string status);

    void Announce(string text);

    /// <summary><c>ga.a(string, eLogState)</c>, gated by <c>/vt log</c>.</summary>
    void Log(MacroLogChannel channel, string message);

    void MirrorToLog(MacroLogChannel channel, string message);

    /// <summary>Remember the selection a burst is about to borrow.</summary>
    void CaptureSelection();

    /// <summary>Put the borrowed selection back.</summary>
    void RestoreSelection();

    /// <summary>The panel's own full stop, used when the session is lost.</summary>
    void StopFromBuffRule(string status);

    /// <summary>FastCastBuffs' forward hold, which the panel owns.</summary>
    void BeginFastCast(IAutomationSurface automation, in PluginSpellInfo spell);
}

internal sealed partial class BuffSelfRule
{
    private readonly IPluginHost _host;
    private readonly BuffSettings _settings;
    private readonly IBuffRuleHost _owner;

    private readonly BuffDueTracker _buffDue = new();

    private readonly ItemEnchantLedger _itemLedger = new();

    private readonly Dictionary<uint, string> _tierTraceSignatures = [];

    private double _nowSeconds;

    private bool _bursting;

    /// <summary>The buff-cast-recast cooldown slot.</summary>
    private double _buffCastRecastRemaining;

    /// <summary>The spell this rule has in flight, 0 when it has none.</summary>
    private uint _castAwaitingSpellId;

    private uint _castAwaitingItemId;
    private uint _castAwaitingItemFamily;
    private uint _castOutcomeItemId;
    private uint _castOutcomeItemFamily;

    private int _castAwaitingItemQuality;
    private double _castAwaitingItemDuration;
    private string _castAwaitingItemSpellName = string.Empty;
    private int _castOutcomeItemQuality;
    private double _castOutcomeItemDuration;
    private string _castOutcomeItemSpellName = string.Empty;

    private CastAttemptOutcome _castOutcome;
    private uint _castOutcomeSpellId;

    /// <summary>
    /// AC's Item Enchantment skill id, which the plugin surface projects as a
    /// spell's school. The reference client asks the same question by school
    /// name.
    /// </summary>
    private const uint ItemEnchantmentSchool = 32u;

    public BuffSelfRule(
        IPluginHost host, BuffSettings settings, IBuffRuleHost owner)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _owner = owner ?? throw new ArgumentNullException(nameof(owner));
    }

    public bool IsBursting => _bursting;

    /// <summary>The rule body, for both list positions.</summary>
    public bool Tick(MacroPassContext context, bool idle) =>
        TickBuffRule(context, idle);

    /// <summary>
    /// Why the last pass did not want to buff. The rule has five ways to say
    /// no and all five are silent, so a run where nothing is ever buffed
    /// looks exactly like a run with nothing to buff.
    /// </summary>
    public string DeclineReason { get; private set; } = string.Empty;

    public void StartForce() => StartForceBuff();

    public void CancelForce() => CancelForceBuffCore();

    public void Advance(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        _buffCastRecastRemaining = Math.Max(
            0d, _buffCastRecastRemaining - elapsed);
        _nowSeconds += elapsed;
        ObserveConsumableUse();
        _itemLedger.Expire(_nowSeconds);
    }

    public void ClearCastAttempt()
    {
        _owner.CastTracker.Reset();
        _castAwaitingSpellId = 0u;
        _castAwaitingItemId = 0u;
        _castAwaitingItemFamily = 0u;
        _castAwaitingItemQuality = 0;
        _castAwaitingItemDuration = 0d;
        _castAwaitingItemSpellName = string.Empty;
        _castOutcome = CastAttemptOutcome.None;
        _castOutcomeSpellId = 0u;
        _castOutcomeItemId = 0u;
        _castOutcomeItemFamily = 0u;
        _castOutcomeItemQuality = 0;
        _castOutcomeItemDuration = 0d;
        _castOutcomeItemSpellName = string.Empty;
    }

    public void Stop()
    {
        ClearCastAttempt();
        _bursting = false;
        _buffDue.CancelForce();
        _itemLedger.CancelForce();
    }

    public void StopBurstOnly()
    {
        ClearCastAttempt();
        _bursting = false;
    }

    /// <summary>Session teardown: every scrap of state, the table included.</summary>
    public void Reset()
    {
        ResetTimerScope();
        ClearConsumableUse();
        _consumableRetryAt.Clear();
        _consumableFamilyRetryAt.Clear();
        _bursting = false;
        _buffCastRecastRemaining = 0d;
        _nowSeconds = 0d;
        _buffDue.Reset();
        _itemLedger.Reset();
        _itemMissingWarnings.Clear();
        _tierTraceSignatures.Clear();
        _lastCastRefusal = null;
        _castAwaitingSpellId = 0u;
        _castAwaitingItemId = 0u;
        _castAwaitingItemFamily = 0u;
        _castAwaitingItemQuality = 0;
        _castAwaitingItemDuration = 0d;
        _castAwaitingItemSpellName = string.Empty;
        _castOutcome = CastAttemptOutcome.None;
        _castOutcomeSpellId = 0u;
        _castOutcomeItemId = 0u;
        _castOutcomeItemFamily = 0u;
        _castOutcomeItemQuality = 0;
        _castOutcomeItemDuration = 0d;
        _castOutcomeItemSpellName = string.Empty;
    }

    public void ResetOncePerRunWarnings() => _itemMissingWarnings.Clear();

    /// <summary>The Meta's <c>needsbuff</c> question, asked against live state.</summary>
    public bool HasAnythingDue() => BuildPlan(_host.Automation).Count != 0;

    public void ObserveCastOutcome(SpellCastOutcomeInfo info) =>
        OnCastTrackerOutcome(info);

    /// <summary>Drop the BuffCastRecast lock outright.</summary>
    public void ClearRecastLock() => _buffCastRecastRemaining = 0d;

    internal static bool ResolveBestKnown(
        IAutomationSurface automation,
        string tierOneName,
        BuffSettings settings,
        IBuffCastability? castability,
        out PluginSpellInfo spell) =>
        TryResolveBestKnown(
            automation, tierOneName, settings, castability, out spell);

    private void StartForceBuff()
    {
        IAutomationSurface automation = _host.Automation;
        if (!automation.IsAvailable)
        {
            _owner.SetStatus("Not in world.");
            return;
        }

        _buffDue.Observe(automation.Character.TimedEnchantments);
        _buffDue.ForceAll();
        _buffDue.ForceItems(CaptureForcedItemRows(automation));
        _itemLedger.ForceAll(_nowSeconds);
        // The reference client's own wording for this.
        _owner.Announce("Force buff enabled.");
    }

    private List<(uint ItemObjectId, uint Family)> CaptureForcedItemRows(
        IAutomationSurface automation)
    {
        var rows = new List<(uint, uint)>();
        IReadOnlyList<PluginInventoryItem> owned = automation.Items.IsAvailable
            ? automation.Items.CaptureOwnedItems()
            : [];
        BuffCastability castability = BuildCastability(automation, owned);

        uint characterId = automation.Character.ObjectId;
        if (characterId != 0u)
        {
            foreach (string tierOneName in CharacterEnchantNames())
            {
                if (TryResolveBestKnown(
                        automation,
                        tierOneName,
                        _settings,
                        castability,
                        out PluginSpellInfo bane))
                {
                    rows.Add((characterId, bane.Family));
                }
            }
        }

        if (!automation.Items.IsAvailable)
            return rows;
        foreach (BuffItemEnchantRow row in _settings.ItemEnchantRows)
        {
            if (row.CastsNothing
                || !TryResolveItemTarget(owned, row, out PluginInventoryItem item))
            {
                continue;
            }
            PluginSpellInfo spell = default;
            bool resolved = row.SpellId is uint spellId
                ? automation.Spells.TryGet(spellId, out PluginSpellInfo exemplar)
                    && TryResolveBestKnown(
                        automation, exemplar, _settings, castability, out spell)
                : TryResolveBestKnown(
                    automation, row.SpellName, _settings, castability, out spell);
            if (resolved && !spell.IsUntargeted)
                rows.Add((item.ObjectId, spell.Family));
        }
        return rows;
    }

    private void CancelForceBuffCore()
    {
        _buffDue.CancelForce();
        // Cancelling a force pass restores the item ledger, as finishing one
        // does.
        _itemLedger.CancelForce();
        _owner.Announce("Force buff canceled.");
    }


    private List<PluginSpellInfo> BuildPlan(
        IAutomationSurface automation,
        double? rebuffWhenUnderSeconds = null)
    {
        uint[] untargetedProfileSpells = _settings.ItemEnchantRows
            .Where(static row => row.IsProfiledItemRow && !row.CastsNothing)
            .Select(static row => row.SpellId!.Value)
            .Where(spellId => automation.Spells.TryGet(spellId, out PluginSpellInfo spell)
                && spell.IsUntargeted)
            .ToArray();
        return BuffPlan.Build(
            BuffProfile.Build(automation.Spells.KnownSelfBuffs),
            automation.Character.Skills,
            automation.Character.Attributes,
            automation.Character.TimedEnchantments,
            _settings,
            force: false,
            rebuffWhenUnderSeconds,
            automation.Character.Level,
            _buffDue.ForcedSpellIds,
            BuildCastability(automation),
            automation.Spells,
            untargetedProfileSpells);
    }

    private BuffCastability BuildCastability(IAutomationSurface automation) =>
        BuildCastability(
            automation,
            automation.Items.IsAvailable
                ? automation.Items.CaptureOwnedItems()
                : []);

    private BuffCastability BuildCastability(
        IAutomationSurface automation,
        IReadOnlyList<PluginInventoryItem> owned) =>
        new(automation.Spells,
            automation.Magic,
            owned,
            _settings.BlacklistedSpellComponents,
            WarnOnce,
            NoteTierPick);

    private void WarnOnce(string text)
    {
        if (!_itemMissingWarnings.Add(text))
            return;
        _owner.Announce(text);
        _owner.MirrorToLog(MacroLogChannel.Misc, text);
    }

    private void OnCastTrackerOutcome(SpellCastOutcomeInfo info)
    {
        if (_castAwaitingSpellId == 0u || info.SpellId != _castAwaitingSpellId)
            return;
        _castAwaitingSpellId = 0u;
        _castOutcomeSpellId = info.SpellId;
        _castOutcomeItemId = _castAwaitingItemId;
        _castOutcomeItemFamily = _castAwaitingItemFamily;
        _castOutcomeItemQuality = _castAwaitingItemQuality;
        _castOutcomeItemDuration = _castAwaitingItemDuration;
        _castOutcomeItemSpellName = _castAwaitingItemSpellName;
        _castAwaitingItemId = 0u;
        _castAwaitingItemFamily = 0u;
        _castAwaitingItemQuality = 0;
        _castAwaitingItemDuration = 0d;
        _castAwaitingItemSpellName = string.Empty;
        _castOutcome = info.Outcome switch
        {
            SpellCastOutcome.Success or SpellCastOutcome.Kill =>
                CastAttemptOutcome.Success,
            SpellCastOutcome.Rejected => CastAttemptOutcome.Rejected,
            SpellCastOutcome.Fail or SpellCastOutcome.PermanentFail =>
                CastAttemptOutcome.Failed,
            _ => CastAttemptOutcome.Timeout,
        };
    }

    private void ConsumeCastOutcome()
    {
        CastAttemptOutcome outcome = _castOutcome;
        uint spellId = _castOutcomeSpellId;
        uint itemId = _castOutcomeItemId;
        uint itemFamily = _castOutcomeItemFamily;
        int itemQuality = _castOutcomeItemQuality;
        double itemDuration = _castOutcomeItemDuration;
        string itemSpellName = _castOutcomeItemSpellName;
        _castOutcome = CastAttemptOutcome.None;
        _castOutcomeSpellId = 0u;
        _castOutcomeItemId = 0u;
        _castOutcomeItemFamily = 0u;
        _castOutcomeItemQuality = 0;
        _castOutcomeItemDuration = 0d;
        _castOutcomeItemSpellName = string.Empty;
        if (outcome != CastAttemptOutcome.Success)
            return;
        _buffDue.NoteRecast(spellId);
        if (itemId == 0u)
            return;

        _buffDue.NoteItemRecast(itemId, itemFamily);
        PluginInventoryItem ownedItem = _host.Automation.Items.CaptureOwnedItems()
            .FirstOrDefault(item => item.ObjectId == itemId);
        if (ownedItem.ObjectId != 0u)
        {
            _itemLedger.NoteCast(ownedItem, itemFamily, spellId, itemQuality,
                itemDuration, _nowSeconds, itemSpellName,
                text => _owner.Log(MacroLogChannel.Misc, text));
            return;
        }
        _itemLedger.NoteCast(
            itemId,
            itemFamily,
            spellId,
            itemQuality,
            itemDuration,
            _nowSeconds,
            itemSpellName,
            text => _owner.Log(MacroLogChannel.Misc, text));
    }

    private bool TickBuffRule(MacroPassContext context, bool idle)
    {
        IAutomationSurface automation = _host.Automation;
        if (!automation.IsAvailable)
        {
            if (_bursting)
                _owner.StopFromBuffRule("Lost the session.");
            DeclineReason = "there is no session to buff in";
            return false;
        }

        // Fold the world into the tracked table before anything reads it.
        _buffDue.Observe(automation.Character.TimedEnchantments);
        ConsumeCastOutcome();

        if (_pendingConsumable != 0u)
            return true;

        // The reference gates a buff on the item-use slot, never on the
        // host's inventory transaction state. The two are not the same: a
        // cast raises that transaction count on its way out, so gating on
        // it made this rule decline for several passes after each of its
        // OWN casts, and the pass fell through to whatever wanted it next.
        if (_consumableLocks.IsLocked(ActionLockKind.ItemUse))
        {
            DeclineReason = "the item slot is held";
            return PauseBurst();
        }

        // EnableBuffing, checked inside the rule.
        if (!_settings.Enabled)
        {
            DeclineReason = "EnableBuffing is off";
            return PauseBurst();
        }

        if (!_owner.MacroEnabled)
        {
            DeclineReason = "the macro is not running";
            return EndBurstAt(idle);
        }

        double threshold = idle
            ? _settings.IdleBuffTopoffSeconds
            : _settings.RebuffWhenUnderSeconds;

        if (_buffCastRecastRemaining > 0d)
            threshold += _settings.BuffCastRecastSeconds;

        // The reference client's floor on the rebuff window.
        if (threshold < 10d)
            threshold = 10d;

        // The whole rule comes down to whether a buff is due within the
        // window.
        if (!TryPickBuff(automation, threshold, out BuffPick pick))
        {
            string within = threshold.ToString(
                "0.#", System.Globalization.CultureInfo.InvariantCulture);
            string known = automation.Spells.KnownSelfBuffs.Count.ToString(
                System.Globalization.CultureInfo.InvariantCulture);
            string carried = automation.Items.IsAvailable
                ? automation.Items.CaptureOwnedItems().Count.ToString(
                      System.Globalization.CultureInfo.InvariantCulture)
                      + " items carried"
                : "the item surface is unavailable, so no component or "
                      + "consumable can be counted";
            DeclineReason =
                $"nothing is due within {within}s that this character can "
                + $"cast ({known} self buffs known, {carried})";
            return EndBurstAt(idle);
        }

        DeclineReason = string.Empty;

        if (!_bursting)
        {
            _bursting = true;
            _owner.CaptureSelection();
        }

        if (!context.CanAct)
            return true;

        // Only this macro's own cast holds the rule here. The host's casting
        // flag is its inventory transaction count under another name, and a
        // request that never completes leaves that count raised for good --
        // which stalled the rule on every pass, claiming and casting
        // nothing, with every rule below it starved.
        if (_owner.CastTracker.IsBusy)
            return true;

        if (pick.ConsumableObjectId != 0u)
            return UseBuffConsumable(automation, pick);

        if (!_owner.Gate.TryPrepare(PluginCombatMode.Magic))
        {
            _owner.SetStatus(_owner.Gate.Status);
            return true;
        }

        // Arm the buff-cast-recast cooldown for its reset window.
        _buffCastRecastRemaining = Math.Max(
            0d,
            _settings.BuffCastRecastResetSeconds);

        TryCast(automation, in pick, "Buffing");
        return true;
    }

    private readonly record struct BuffPick(
        PluginSpellInfo Spell,
        uint TargetObjectId,
        string TargetName,
        bool IsItemEnchant,
        uint ConsumableObjectId = 0u);

    private bool TryPickBuff(
        IAutomationSurface automation,
        double threshold,
        out BuffPick pick)
    {
        List<PluginSpellInfo> due = BuildPlan(automation, threshold);
        if (due.Count > 0)
        {
            pick = new BuffPick(
                due[0], automation.Character.ObjectId, "yourself", false);
            return true;
        }
        return TryPickItemEnchant(automation, threshold, out pick)
            || TryPickConsumable(automation, threshold, out pick);
    }

    private bool TryPickItemEnchant(
        IAutomationSurface automation,
        double threshold,
        out BuffPick pick)
    {
        pick = default;

        if (!BuffPlan.IsSchoolAvailable(
                ItemEnchantmentSchool,
                automation.Character.Skills,
                _settings,
                automation.Character.Level))
        {
            return false;
        }

        IReadOnlyList<PluginInventoryItem> owned = automation.Items.IsAvailable
            ? automation.Items.CaptureOwnedItems()
            : [];
        BuffCastability castability = BuildCastability(automation, owned);

        if (TryPickCharacterEnchant(
                automation, castability, threshold, out pick))
        {
            return true;
        }

        if (!_settings.BuffAuras)
            return false;
        IList<BuffItemEnchantRow> rows = _settings.ItemEnchantRows;
        if (rows.Count == 0 || owned.Count == 0)
            return false;

        foreach (BuffItemEnchantRow row in rows)
        {
            if (row.CastsNothing)
                continue;
            if (!TryResolveItemTarget(owned, row, out PluginInventoryItem item))
            {
                PostItemMissingWarningOnce(row.ObjectId.HasValue
                    ? row.ObjectId.Value == uint.MaxValue ? "equipped weapon" : row.ObjectId.Value.ToString()
                    : row.ItemName);
                continue;
            }
            PluginSpellInfo spell = default;
            bool resolved;
            if (row.SpellId.HasValue)
            {
                resolved = automation.Spells.TryGet(row.SpellId.Value, out PluginSpellInfo exemplar)
                    && TryResolveBestKnown(automation, exemplar, _settings, castability, out spell);
            }
            else
            {
                resolved = TryResolveBestKnown(
                    automation, row.SpellName, _settings, castability, out spell);
            }
            if (!resolved || spell.IsUntargeted)
            {
                continue;
            }
            // A force pass zeroes every item entry's stamp too, so a forced
            // row reads as having nothing left.
            double remaining = _buffDue.IsItemForced(item.ObjectId, spell.Family)
                ? 0d
                : ItemEnchantRemainingSeconds(automation, item.ObjectId, in spell);
            if (remaining >= threshold)
                continue;
            pick = new BuffPick(spell, item.ObjectId, item.Name, true);

            LogRowPick(
                "item row", item.Name, item.ObjectId, in spell, remaining, threshold);
            return true;
        }
        return false;
    }

    private static bool TryResolveItemTarget(
        IReadOnlyList<PluginInventoryItem> owned,
        in BuffItemEnchantRow row,
        out PluginInventoryItem item)
    {
        if (row.ObjectId is uint objectId)
        {
            if (objectId == uint.MaxValue)
            {
                foreach (PluginInventoryItem candidate in owned)
                {
                    if (CombatModeGate.IsWeaponSlot(candidate.EquippedLocation))
                    {
                        item = candidate;
                        return true;
                    }
                }
                item = default;
                return false;
            }
            foreach (PluginInventoryItem candidate in owned)
            {
                if (candidate.ObjectId == objectId)
                {
                    item = candidate;
                    return true;
                }
            }
            item = default;
            return false;
        }
        return TryFindOwned(owned, row.ItemName, out item);
    }

    private bool TryPickCharacterEnchant(
        IAutomationSurface automation,
        BuffCastability castability,
        double threshold,
        out BuffPick pick)
    {
        pick = default;
        uint characterId = automation.Character.ObjectId;
        if (characterId == 0u)
            return false;

        if (!_settings.BuffBanes)
            return false;

        foreach (string tierOneName in CharacterEnchantNames())
        {
            // The same family resolve every other row uses.
            if (!TryResolveBestKnown(
                    automation,
                    tierOneName,
                    _settings,
                    castability,
                    out PluginSpellInfo spell))
            {
                continue;
            }
            double remaining =
                _buffDue.IsItemForced(characterId, spell.Family)
                    ? 0d
                    : ItemEnchantRemainingSeconds(
                        automation, characterId, in spell);
            // Due when what is left on the enchantment is under the window.
            if (remaining >= threshold)
                continue;
            pick = new BuffPick(spell, characterId, "yourself", true);
            LogRowPick(
                "character row",
                automation.Character.Name,
                characterId,
                in spell,
                remaining,
                threshold);
            return true;
        }
        return false;
    }

    private IEnumerable<string> CharacterEnchantNames()
    {
        yield return BuffElementProfile.BaneSpellName(
            BuffElementProfile.Element.Physical);
        foreach (BuffElementProfile.Element element in BuffElementProfile.Banes(
            _settings.BaneProfileMode, _settings.BaneElements))
        {
            yield return BuffElementProfile.BaneSpellName(element);
        }
    }

    private void NoteTierPick(
        BuffLine line, PluginSpellInfo? pick, IReadOnlyList<BuffTierRejection> rejections)
    {
        if (line.Tiers.Count == 0)
            return;

        int highestKnownTier = line.Tiers[0].Tier;
        if (pick is { } picked && picked.Tier >= highestKnownTier)
            return;

        int floor = pick?.Tier ?? int.MinValue;
        var higher = new List<BuffTierRejection>();
        foreach (BuffTierRejection rejection in rejections)
        {
            if (rejection.Spell.Tier > floor)
                higher.Add(rejection);
        }

        string signature = BuildTierTraceSignature(pick, higher);
        if (_tierTraceSignatures.TryGetValue(line.Family, out string? lastSignature)
            && lastSignature == signature)
        {
            return;   // same pick, same rejections as the last trace of this family
        }
        _tierTraceSignatures[line.Family] = signature;

        string pickedText = pick is { } spell
            ? $"picked {spell.Name} (gen {spell.Tier})"
            : "picked nothing";
        string rejectedText;
        if (higher.Count == 0)
        {
            rejectedText = "none";
        }
        else
        {
            var fragments = new List<string>(higher.Count);
            foreach (BuffTierRejection rejection in higher)
                fragments.Add($"{rejection.Spell.Name} {rejection.Reason}");
            rejectedText = string.Join(", ", fragments);
        }

        _owner.Log(
            MacroLogChannel.Timers,
            $"Buffing: {line.Reference.Name} — {pickedText}; rejected: {rejectedText}");
    }

    private static string BuildTierTraceSignature(
        PluginSpellInfo? pick, IReadOnlyList<BuffTierRejection> higher)
    {
        string signature = pick?.SpellId.ToString() ?? "none";
        foreach (BuffTierRejection rejection in higher)
            signature += "|" + rejection.Spell.SpellId + ":" + rejection.Reason;
        return signature;
    }

    private void LogRowPick(
        string kind,
        string targetName,
        uint targetObjectId,
        in PluginSpellInfo spell,
        double remaining,
        double threshold) =>
        _owner.Log(
            MacroLogChannel.Misc,
            $"Buffing: {kind} {targetName} ({targetObjectId}) \u2192 "
                + $"{spell.Name} [family {spell.Family}], covered for "
                + $"{remaining:0.#}s of {threshold:0.#}s");

    private static bool TryFindOwned(
        IReadOnlyList<PluginInventoryItem> owned,
        string name,
        out PluginInventoryItem found)
    {
        foreach (PluginInventoryItem candidate in owned)
        {
            if (string.Equals(candidate.Name, name, StringComparison.Ordinal))
            {
                found = candidate;
                return true;
            }
        }
        found = default;
        return false;
    }

    private static bool TryResolveBestKnown(
        IAutomationSurface automation,
        string tierOneName,
        BuffSettings settings,
        IBuffCastability? castability,
        out PluginSpellInfo spell)
    {
        string stem = StemOf(tierOneName);
        spell = default;
        IReadOnlyList<PluginSpellInfo> known = automation.Spells.KnownSelfBuffs;

        // A_0: the row's own tier-I spell, by its exact name.
        PluginSpellInfo reference = default;
        bool haveReference = false;
        foreach (PluginSpellInfo candidate in known)
        {
            if (candidate.Name.Equals(tierOneName, StringComparison.OrdinalIgnoreCase))
            {
                reference = candidate;
                haveReference = true;
                break;
            }
        }
        if (!haveReference)
        {
            foreach (PluginSpellInfo candidate in known)
            {
                if (!candidate.Name.StartsWith(stem, StringComparison.OrdinalIgnoreCase))
                    continue;
                if (!haveReference
                    || candidate.Tier < reference.Tier
                    || (candidate.Tier == reference.Tier
                        && candidate.SpellId < reference.SpellId))
                {
                    reference = candidate;
                    haveReference = true;
                }
            }
        }
        return haveReference && TryResolveBestKnown(
            automation, reference, settings, castability, out spell);
    }

    private static bool TryResolveBestKnown(
        IAutomationSurface automation,
        in PluginSpellInfo reference,
        BuffSettings settings,
        IBuffCastability? castability,
        out PluginSpellInfo spell)
    {
        spell = default;
        IReadOnlyList<PluginSpellInfo> known = automation.Spells.KnownSelfBuffs;
        var tiers = new List<PluginSpellInfo>();
        foreach (PluginSpellInfo candidate in known)
        {
            if (candidate.Family == reference.Family)
                tiers.Add(candidate);
        }
        tiers.Sort(static (a, b) =>
        {
            if (a.Tier != b.Tier)
                return b.Tier.CompareTo(a.Tier);
            if (a.Quality != b.Quality)
                return b.Quality.CompareTo(a.Quality);
            return a.SpellId.CompareTo(b.SpellId);
        });

        var skillLevels = new Dictionary<uint, uint>();
        foreach (PluginSkillInfo skill in automation.Character.Skills)
            skillLevels[skill.SkillId] = skill.Current;

        var line = new BuffLine(
            reference.Family, BuffTargetKind.Other, StemOf(reference.Name), tiers)
        {
            ReferenceOverride = reference,
        };
        return BuffPlan.TryPickTier(
            line, skillLevels, settings, castability, out spell);
    }

    /// <summary>Drop the trailing tier numeral from a tier-I name.</summary>
    private static string StemOf(string tierOneName) =>
        tierOneName.EndsWith(" I", StringComparison.Ordinal)
            ? tierOneName[..^2]
            : tierOneName;

    private double ItemEnchantRemainingSeconds(
        IAutomationSurface automation,
        uint itemObjectId,
        in PluginSpellInfo spell)
    {
        double longest = 0d;
        // Self-targeted auras are carried by the character even when their
        // profile row belongs to a weapon. The server restores these timers
        // at login, independently of casts observed during this session.
        if (spell.IsSelfTargeted)
        {
            foreach (PluginActiveEnchantment held in automation.Character.TimedEnchantments)
            {
                if (held.Family == spell.Family && held.Tier >= spell.Tier)
                    longest = Math.Max(longest, held.SecondsRemaining);
            }
            return longest;
        }
        PluginInventoryItem ownedItem = automation.Items.CaptureOwnedItems()
            .FirstOrDefault(item => item.ObjectId == itemObjectId);
        longest = ownedItem.ObjectId != 0u
            ? _itemLedger.RemainingSeconds(ownedItem, spell.Family, spell.Tier, _nowSeconds)
            : itemObjectId == automation.Character.ObjectId
                ? _itemLedger.RemainingSeconds(itemObjectId, spell.Family, spell.Tier, _nowSeconds)
                : 0d;
        foreach (PluginTrackedEnchantment held in
            automation.Enchantments.Capture(itemObjectId))
        {
            if (held.Family != spell.Family || held.Quality < spell.Difficulty)
                continue;
            if (held.SecondsRemaining > longest)
                longest = held.SecondsRemaining;
        }
        return longest;
    }

    private void PostItemMissingWarningOnce(string itemName) =>
        WarnOnce($"Warning: item {itemName} is in profile but not found "
            + "in inventory. Skipping buffs for item.");

    private readonly HashSet<string> _itemMissingWarnings =
        new(StringComparer.Ordinal);

    private bool EndBurstAt(bool idle) =>
        idle || !_settings.IdleBuffTopoff ? EndBurst() : PauseBurst();

    private bool PauseBurst()
    {
        if (!_bursting)
            return false;
        _bursting = false;
        _owner.RestoreSelection();
        return false;
    }

    private bool EndBurst()
    {
        if (!_bursting)
            return false;

        _bursting = false;
        _owner.RestoreSelection();
        _owner.SetStatus("Idle.");
        return false;
    }

    private void NoteCastRefused(in PluginSpellInfo spell, string reason)
    {
        string text = $"SpellCaster: {spell.Name} not issued — {reason}";
        if (string.Equals(_lastCastRefusal, text, StringComparison.Ordinal))
            return;
        _lastCastRefusal = text;
        _owner.Log(MacroLogChannel.CastInfo, text);
    }

    private string? _lastCastRefusal;

    private bool TryCast(
        IAutomationSurface automation, in BuffPick pick, string label)
    {
        PluginSpellInfo spell = pick.Spell;
        if (pick.IsItemEnchant || !spell.IsSelfTargeted)
        {
            uint target = pick.TargetObjectId;
            if (target == 0)
            {
                _owner.SetStatus($"{spell.Name}: no target");
                NoteCastRefused(spell, "no target");
                return false;
            }
            if (_host.Selection.SelectedObjectId != target)
                _host.Selection.Select(target);
        }

        PluginCastGate gate = automation.Magic.EvaluateGate(spell.SpellId);
        if (gate != PluginCastGate.Ready)
        {
            _owner.SetStatus($"{spell.Name}: {gate}");
            NoteCastRefused(spell, $"{gate} on {pick.TargetName}");
            return false;
        }

        long issueRevision = automation.Magic.LastCompletion.Revision;
        PluginCastRequestResult request =
            automation.Magic.RequestCast(spell.SpellId);
        if (request != PluginCastRequestResult.Sent)
        {
            _owner.SetStatus($"Refused {spell.Name}.");
            NoteCastRefused(spell, request.ToString());
            return false;
        }
        _lastCastRefusal = null;

        _owner.Log(
            MacroLogChannel.SpellCast,
            $"Casting: {spell.Name} on {pick.TargetObjectId} ({pick.TargetName})");

        _owner.BeginFastCast(automation, spell);
        _owner.CastTracker.Begin(
            spell.SpellId,
            spell.Name,
            pick.TargetObjectId,
            spell.School == ItemEnchantmentSchool ? string.Empty : pick.TargetName,
            SpellCastTracker.HitsMultipleTargetsFor(spell),
            issueRevision,
            spell.Saying);
        _owner.Log(MacroLogChannel.CastInfo, "SpellCaster: Begin");
        _castAwaitingSpellId = spell.SpellId;
        _castAwaitingItemId = pick.IsItemEnchant ? pick.TargetObjectId : 0u;
        _castAwaitingItemFamily = pick.IsItemEnchant ? spell.Family : 0u;
        // What the item ledger will need when the RESULT arrives: the spell's
        // quality, its duration and its name.
        _castAwaitingItemQuality = pick.IsItemEnchant ? spell.Tier : 0;
        _castAwaitingItemDuration =
            pick.IsItemEnchant ? spell.DurationSeconds : 0d;
        _castAwaitingItemSpellName = pick.IsItemEnchant ? spell.Name : string.Empty;
        _castOutcome = CastAttemptOutcome.None;
        _owner.SetStatus($"{label}: {spell.Name}");
        _host.Log.Info($"MossTank: casting {spell.Name} (0x{spell.SpellId:X4})");
        return true;
    }
}

/// <summary>What the buff rule made of the tracker outcome it was waiting on.</summary>
internal enum CastAttemptOutcome
{
    None,
    Success,

    /// <summary>A server refusal — the host answered with a weenie error.</summary>
    Rejected,

    Failed,

    Timeout,
}
