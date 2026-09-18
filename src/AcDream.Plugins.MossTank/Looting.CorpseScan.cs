using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The looter's corpse half: which corpses the client knows of, which of them
/// may be looted at all, which one the next pass picks, and how a corpse is
/// remembered — refused, unopenable, finished — until it is forgotten again.
/// The item half of the same controller lives in <c>Looting.cs</c>.
/// </summary>
internal sealed partial class LootController
{
    /// <summary>
    /// A finished corpse is not just walked away from: the corpse object is
    /// used a second time, which is what shuts the container view. The use is
    /// an item action like any other, so it waits for the item channel to be
    /// free, and a refused one is simply tried again on the next pass — the
    /// corpse is not let go of until the container is really closed.
    /// </summary>
    private bool CloseFinishedCorpse(uint corpseId, bool canAct)
    {
        if (corpseId == 0u)
            return false;
        IItemAutomation items = _host.Automation.Items;
        ILootAutomation loot = _host.Automation.Loot;
        if (!canAct
            || !items.IsAvailable
            || !loot.IsAvailable
            || ItemSlotHeld(items))
        {
            Status = "Waiting to close corpse…";
            return true;
        }
        Status = loot.Close(corpseId).Accepted
            ? "Corpse complete."
            : "Waiting to close corpse…";
        return true;
    }

    /// <summary>
    /// Drops the three slots an open took, the moment the container it was
    /// waiting for is actually open.
    /// <para>
    /// The corpse-open slot is what says the three belong to an open rather
    /// than to anything else, so it is both the test and the first thing
    /// released. Without this the open's own window has to run out before the
    /// looter may read the corpse it just opened — and the item slot it holds
    /// is the very thing the loot rule is gated on, so the wait would be a
    /// stall rather than a pause. This runs on the host's frame, beside the
    /// slot clock, for the same reason: a rule pass cannot release the slot
    /// that stops it running.
    /// </para>
    /// </summary>
    /// <returns>True when the three slots were released this frame.</returns>
    internal bool ObserveCorpseOpened()
    {
        if (_actionLocks is not { } locks
            || !locks.IsLocked(ActionLockKind.CorpseOpenAttempt)
            || _activeCorpse == 0u
            || !_host.Automation.IsAvailable)
        {
            return false;
        }
        ILootAutomation loot = _host.Automation.Loot;
        if (!loot.IsAvailable || loot.CurrentContainerId != _activeCorpse)
            return false;

        locks.Release(ActionLockKind.CorpseOpenAttempt);
        locks.Release(ActionLockKind.Navigation);
        locks.Release(ActionLockKind.ItemUse);
        return true;
    }

    private bool IsOwnDeathCorpse(in PluginLootContainer corpse)
    {
        string character = _host.Automation.Character.Name;
        return character.Length != 0
            && string.Equals(
                corpse.Name,
                "Corpse of " + character,
                StringComparison.Ordinal);
    }

    /// <summary>
    /// The radius the open step picks within — arm's reach, a little over five
    /// metres. Outside it the corpse is the approach step's business.
    /// </summary>
    private const double CorpseOpenRangeMeters = 240d / 48d;

    /// <summary>
    /// The corpse the approach step walks at: the nearest one it may loot
    /// inside the approach range, over the whole set the client is reporting.
    /// It is the SAME pick the open step makes, with the other metric and the
    /// other radius — one place decides what "a corpse worth having" is, so a
    /// corpse the looter would refuse is never walked to either.
    /// </summary>
    /// <param name="rangeMeters">The approach range, as the profile sets it.</param>
    internal bool TrySelectApproachCorpse(
        double rangeMeters,
        out PluginLootContainer corpse)
    {
        corpse = default;
        if (!_settings.ProfileActive
            || !_settings.Enabled
            || !_host.Automation.IsAvailable)
            return false;
        ILootAutomation loot = _host.Automation.Loot;
        if (!loot.IsAvailable)
            return false;
        if (HasStationaryCorpseWork())
            return false;
        if (_settings.Rules.Count == 0
            && string.IsNullOrWhiteSpace(_settings.ExternalClassifierId))
        {
            return false;
        }
        if (SelectCorpse(
                loot.CaptureCorpses(float.MaxValue),
                rangeMeters,
                byHeading: false)
            is not { } picked)
        {
            return false;
        }
        corpse = picked;
        return true;
    }

    /// <summary>
    /// Answers whether the open step still has known work while its paced scan
    /// is waiting. Holding that rule's place keeps route movement from taking
    /// one turn between adjacent corpses.
    /// </summary>
    internal bool HasEligibleCorpseInOpenRange()
    {
        if (!_settings.ProfileActive
            || !_settings.Enabled
            || !_host.Automation.IsAvailable)
        {
            return false;
        }

        ILootAutomation loot = _host.Automation.Loot;
        return loot.IsAvailable
            && SelectCorpse(
                loot.CaptureCorpses(float.MaxValue),
                CorpseOpenRangeMeters,
                byHeading: true) is not null;
    }

    internal bool HasStationaryCorpseWork() =>
        HasPendingCorpseWork(CorpseOpenRangeMeters);

    internal bool HasPendingRouteLoot() =>
        HasPendingCorpseWork(Math.Max(CorpseOpenRangeMeters, _settings.CorpseApproachRange));

    // A cooldown or an outstanding description does not mean looting is done.
    // Keep movement out until nearby corpses can be judged and processed.
    private bool HasPendingCorpseWork(double rangeMeters)
    {
        if (!_settings.ProfileActive || !_settings.Enabled
            || !_host.Automation.IsAvailable
            || (_settings.Rules.Count == 0
                && string.IsNullOrWhiteSpace(_settings.ExternalClassifierId)))
            return false;
        ILootAutomation loot = _host.Automation.Loot;
        if (!loot.IsAvailable)
            return false;
        if (_activeCorpse != 0u || loot.CurrentContainerId != 0u)
            return true;

        IReadOnlyList<PluginLootContainer> corpses = loot.CaptureCorpses(float.MaxValue);
        if (SelectCorpse(corpses, rangeMeters, byHeading: false) is not null)
            return true;
        foreach (PluginLootContainer corpse in corpses)
        {
            if (!corpse.IsIdentified && corpse.Distance <= rangeMeters
                && !_completedCorpses.ContainsKey(corpse.ObjectId)
                && !IsCorpseDenied(corpse.ObjectId)
                && !IsCorpseBlacklisted(corpse.ObjectId))
                return true;
        }
        return false;
    }

    /// <summary>
    /// The corpse pick. Any rare corpse beats any non-rare one whatever the
    /// running best: the first rare found becomes the pick outright, after
    /// which only a better rare can replace it, and a non-rare can only win
    /// while no rare has been picked at all.
    /// <para>
    /// Two callers, two metrics. The step that walks to a corpse ranks by
    /// distance; the step that opens one ranks by how far round the character
    /// would have to turn to face it, so among corpses already in reach the
    /// one being looked at wins. An unplaceable corpse ranks last.
    /// </para>
    /// </summary>
    private PluginLootContainer? SelectCorpse(
        IEnumerable<PluginLootContainer> corpses,
        double rangeMeters,
        bool byHeading)
    {
        PluginNavigationPosition self =
            _host.Automation.Navigation.Snapshot.Position;
        PluginLootContainer? selected = null;
        double best = double.MaxValue;
        bool rarePicked = false;
        foreach (PluginLootContainer corpse in corpses)
        {
            if (corpse.Distance > rangeMeters
                || _completedCorpses.ContainsKey(corpse.ObjectId)
                || IsCorpseDenied(corpse.ObjectId)
                || IsCorpseBlacklisted(corpse.ObjectId)
                || !corpse.IsIdentified
                || !CanLoot(corpse))
            {
                continue;
            }
            double metric = byHeading
                ? HeadingOffsetTo(self, corpse)
                : corpse.Distance;
            bool rare = IsRare(corpse);
            if (rare && !rarePicked)
            {
                rarePicked = true;
                best = metric;
                selected = corpse;
            }
            else if ((rare || !rarePicked) && metric < best)
            {
                best = metric;
                selected = corpse;
            }
        }
        return selected;
    }

    /// <summary>
    /// How far round, in degrees, the character would have to turn to face the
    /// corpse — zero when it is dead ahead, 180 when it is directly behind. A
    /// corpse or a character with no place in the world answers a number no
    /// real corpse can beat.
    /// </summary>
    private static double HeadingOffsetTo(
        in PluginNavigationPosition self,
        in PluginLootContainer corpse)
    {
        if (!corpse.HasPosition)
            return 99999d;
        return Math.Abs(NavigationController.SignedHeadingDelta(
            self.HeadingDegrees,
            NavigationController.DesiredHeading(self, corpse.Position)));
    }

    /// <summary>
    /// The server refuses a corpse someone else owns or has open with a plain
    /// text line. That line is listened for, and the corpse skipped outright for ten
    /// seconds instead of grinding through thirty failed open attempts.
    /// </summary>
    private void ObserveOwnershipDenials()
    {
        IReadOnlyList<PluginChatMessage> messages =
            _host.Automation.Chat.CaptureMessages(_chatSequence);
        if (messages.Count == 0)
            return;
        // Always the corpse the last pick chose, never the one that happens to
        // be open — that is what the refusal is an answer to.
        uint denied = _selectedCorpse;
        foreach (PluginChatMessage message in messages)
        {
            if (message.Sequence > _chatSequence)
                _chatSequence = message.Sequence;
            if (denied == 0u
                || (!CorpseAlreadyInUse().IsMatch(message.Text)
                    && !NoRightToLoot().IsMatch(message.Text)))
            {
                continue;
            }
            _corpseDeniedAt[denied] = _lifetime;
            Log?.Invoke(
                MacroLogChannel.Loot,
                $"LootCorpse: 0x{denied:X8} refused, skipping it for "
                    + $"{DenialSkipSeconds:0} seconds");
            if (_activeCorpse == denied)
            {
                _activeCorpse = 0u;
                _activeCorpseSawContents = false;
                _activeCorpseIsOwnDeath = false;
                _stateAge = 0d;
            }
        }
    }

    private bool IsCorpseDenied(uint corpseId) =>
        _corpseDeniedAt.TryGetValue(corpseId, out double at)
        && _lifetime - at < DenialSkipSeconds;

    [GeneratedRegex(
        @"The (Corpse)|(Treasure) of ([a-zA-Z\ \-\']*) is already in use by someone else!")]
    private static partial Regex CorpseAlreadyInUse();

    [GeneratedRegex(
        @"You do not yet have the right to loot the (Corpse)|(Treasure) of .*")]
    private static partial Regex NoRightToLoot();

    private bool CanLoot(in PluginLootContainer corpse)
    {
        if (_settings.LootOnlyRareCorpses && !IsRare(corpse))
            return false;
        string killer = KillerName(corpse.LongDescription);
        string character = _host.Automation.Character.Name;
        if (killer.Length != 0
            && character.Length != 0
            && string.Equals(killer, character, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        if (IsRare(corpse))
            return false;

        double firstSeen = _corpseFirstSeen.TryGetValue(
            corpse.ObjectId,
            out double value) ? value : _lifetime;
        double age = _lifetime - firstSeen;
        PluginFellowMember? fellow = _host.Automation.Fellowship
            .CaptureMembers()
            .FirstOrDefault(member => string.Equals(
                member.Name,
                killer,
                StringComparison.OrdinalIgnoreCase));
        if (fellow is { ObjectId: not 0u } member)
        {
            if (!_settings.LootFellowCorpses)
                return false;
            return member.ShareLoot || age >= 100d;
        }

        return _settings.LootAllCorpses && age >= 100d;
    }

    private void BlacklistFailedCorpse(uint corpseId)
    {
        if (corpseId == 0u)
            return;
        _corpseOpenAttempts.TryGetValue(corpseId, out int attempts);
        attempts++;
        int threshold = Math.Clamp(
            _settings.BlacklistCorpseOpenAttemptCount,
            1,
            1000);
        if (attempts < threshold)
        {
            _corpseOpenAttempts[corpseId] = attempts;
            Status = $"Retrying corpse ({attempts}/{threshold})…";
            return;
        }
        _corpseOpenAttempts.Remove(corpseId);
        _corpseBlacklistedAt[corpseId] = _lifetime;
        Status = $"Blacklisted unopenable corpse for "
            + $"{Math.Clamp(_settings.BlacklistCorpseOpenTimeoutSeconds, 1d, 3600d):0} seconds.";
    }

    private bool IsCorpseBlacklisted(uint corpseId)
    {
        if (!_corpseBlacklistedAt.TryGetValue(corpseId, out double since))
            return false;
        double timeout = Math.Clamp(
            _settings.BlacklistCorpseOpenTimeoutSeconds,
            1d,
            3600d);
        if (_lifetime - since < timeout)
            return true;
        _corpseBlacklistedAt.Remove(corpseId);
        return false;
    }

    private void MarkCorpseComplete(uint corpseId)
    {
        if (corpseId == 0u)
            return;
        _completedCorpses[corpseId] = _lifetime;
        _corpseOpenAttempts.Remove(corpseId);
        _corpseBlacklistedAt.Remove(corpseId);
    }

    /// <summary>
    /// Stamps the corpses the client is reporting and drops the records of the
    /// ones it has stopped reporting. A record only goes when both are true:
    /// the corpse is gone from the client's known set, and the cache timeout
    /// has run since it last came into it. Both together, so a corpse still in
    /// sight never loses the fact that it has already been looted, and a
    /// corpse seen once from across a field does not sit in memory for the
    /// life of the session.
    /// </summary>
    private void PruneCorpseCache(IReadOnlyList<PluginLootContainer> known)
    {
        double expiry = Math.Clamp(
            _settings.CorpseCacheTimeoutMinutes,
            1d,
            1440d) * 60d;

        _presentCorpses.Clear();
        foreach (PluginLootContainer seen in known)
        {
            _presentCorpses.Add(seen.ObjectId);
            // Newly known, or known again after having gone: either way this
            // is when its eviction clock restarts. The first-sighting stamp
            // the ownership timers measure against is never restarted.
            if (_corpseFirstSeen.TryAdd(seen.ObjectId, _lifetime)
                || _releasedCorpses.Remove(seen.ObjectId))
            {
                _corpseLastSeen[seen.ObjectId] = _lifetime;
            }
        }
        foreach (uint id in _corpseFirstSeen.Keys)
        {
            if (!_presentCorpses.Contains(id))
                _releasedCorpses.Add(id);
        }

        _evictedCorpses.Clear();
        foreach (uint id in _releasedCorpses)
        {
            if (_corpseLastSeen.TryGetValue(id, out double last)
                && _lifetime - last >= expiry)
            {
                _evictedCorpses.Add(id);
            }
        }
        foreach (uint id in _evictedCorpses)
        {
            _corpseFirstSeen.Remove(id);
            _corpseLastSeen.Remove(id);
            _releasedCorpses.Remove(id);
            _completedCorpses.Remove(id);
            _corpseDeniedAt.Remove(id);
            _corpseBlacklistedAt.Remove(id);
            _corpseOpenAttempts.Remove(id);
        }
    }

    internal static string KillerName(string description)
    {
        if (string.IsNullOrWhiteSpace(description))
            return string.Empty;
        Match match = KilledByDescription().Match(description);
        return match.Success && match.Index == 0
            ? match.Groups[1].Value
            : string.Empty;
    }

    /// <summary>
    /// A corpse counts as rare either because the treasure it holds was
    /// flagged generated-rare, or because its description is not a kill
    /// description at all. The second case is the one that keeps a corpse
    /// nobody is recorded as having killed out of the ordinary ownership
    /// rules — such a corpse always sorts first and is then never looted,
    /// because the killer it names is nobody.
    /// </summary>
    private static bool IsRare(in PluginLootContainer corpse) =>
        corpse.IsGeneratedRare
        || KilledByDescription().Match(corpse.LongDescription ?? string.Empty)
            is not { Success: true, Index: 0 };

    [GeneratedRegex(@"(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)")]
    private static partial Regex KilledByDescription();
}
