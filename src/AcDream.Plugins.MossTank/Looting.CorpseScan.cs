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
    internal const double CorpseOpenRangeMeters = 240d / 48d;

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

    /// <summary>
    /// The margin the reference adds to the approach range when it asks
    /// whether an undescribed corpse is close enough to hold the walks off:
    /// ten metres.
    /// </summary>
    internal const double CorpseIdWaitMarginMeters = 240d / 24d;

    /// <summary>
    /// Whether a corpse whose description has not arrived lies within
    /// <paramref name="rangeMeters"/>. This is the reference's "a corpse id
    /// request is pending" question: every corpse the client knows of is
    /// asked for its description as it appears, so a corpse the server has
    /// not answered yet is one whose answer is still on its way. A corpse
    /// answered without a description is not waited on.
    /// </summary>
    internal bool HasCorpseAwaitingDescriptionWithin(double rangeMeters)
    {
        if (!_host.Automation.IsAvailable || !RareWindowOpenOrNotRareOnly())
            return false;
        ILootAutomation loot = _host.Automation.Loot;
        if (!loot.IsAvailable)
            return false;
        foreach (PluginLootContainer corpse in loot.CaptureCorpses(float.MaxValue))
        {
            if (!IsDescriptionAnswered(corpse)
                && WantsDescription(corpse)
                && corpse.Distance <= rangeMeters
                && !_completedCorpses.ContainsKey(corpse.ObjectId)
                && !IsCorpseDenied(corpse.ObjectId)
                && !IsCorpseBlacklisted(corpse.ObjectId))
            {
                return true;
            }
        }
        return false;
    }

    /// <summary>
    /// Has the server answered this corpse's description request, with the
    /// description or without one? A corpse the server no longer has (it
    /// decayed while the character was elsewhere) is answered with nothing
    /// and never gains a description, but its answer is in: it is no longer
    /// waited on or asked about, as the reference stops waiting once a
    /// corpse's id request has left its queue. Without a description it is
    /// still never picked to open (the pick needs the killer's name), so it
    /// is simply passed over. The client's own flag says so; one already
    /// described has been answered either way.
    /// </summary>
    internal static bool IsDescriptionAnswered(in PluginLootContainer corpse) =>
        corpse.IsIdentified || corpse.IsAppraisalAnswered;

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
            ObserveRareAnnouncement(message.Text);
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

    /// <summary>
    /// How long before the announcement a corpse may have appeared and still
    /// be the one it is about. The server puts the rare's corpse in the world
    /// half a second before it announces the find.
    /// </summary>
    internal const double RareAnnouncementLeadSeconds = 5d;

    /// <summary>How long an announcement keeps new corpses worth describing.</summary>
    internal const double RareAnnouncementWindowSeconds = 30d;

    /// <summary>
    /// The server tells everyone nearby when a kill generates a rare:
    /// "Name has discovered the Item!". When the name is this character's,
    /// the corpses that appeared around then are the ones worth describing.
    /// </summary>
    private void ObserveRareAnnouncement(string text)
    {
        if (!_settings.LootOnlyRareCorpses
            || RareDiscovered().Match(text) is not { Success: true } match)
        {
            return;
        }
        string character = _host.Automation.Character.Name.TrimStart('+');
        if (character.Length == 0
            || !string.Equals(
                match.Groups["finder"].Value.TrimStart('+'),
                character,
                StringComparison.OrdinalIgnoreCase))
        {
            return;
        }
        _rareAnnouncedAt = _lifetime;
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"LootCorpse: rare announced ({match.Groups["item"].Value}); describing new corpses");
    }

    /// <summary>
    /// Whether this corpse is worth a description request. With rare-only
    /// looting on, an ordinary kill is never described: only corpses that
    /// appeared around this character's own rare announcement are, for a
    /// short while after it. Everything else is described as before.
    /// </summary>
    private bool WantsDescription(in PluginLootContainer corpse)
    {
        if (!_settings.LootOnlyRareCorpses)
            return true;
        if (!RareWindowOpenOrNotRareOnly())
            return false;
        return !_corpseFirstSeen.TryGetValue(corpse.ObjectId, out double firstSeen)
            || firstSeen >= _rareAnnouncedAt - RareAnnouncementLeadSeconds;
    }

    /// <summary>
    /// False only while rare-only looting waits for an announcement: then no
    /// corpse is worth looking at, and the corpse passes skip the whole list.
    /// </summary>
    private bool RareWindowOpenOrNotRareOnly() =>
        !_settings.LootOnlyRareCorpses
        || _lifetime - _rareAnnouncedAt <= RareAnnouncementWindowSeconds;

    [GeneratedRegex(@"^(?<finder>[^\r\n]+?) has discovered the (?<item>[^\r\n]+)!$")]
    private static partial Regex RareDiscovered();

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
        // The local server writes the killer's name without the plus sign an
        // admin character carries, so the character's own plus is not part
        // of the comparison. That is a deliberate deviation.
        string character = _host.Automation.Character.Name.TrimStart('+');
        if (killer.Length != 0
            && character.Length != 0
            && string.Equals(killer, character, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        // A corpse whose description names no killer is nobody's, and is
        // never looted, however old it gets.
        if (killer.Length == 0)
            return false;

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

    /// <summary>
    /// The three slots an open attempt holds, for the profile's open
    /// timeout. The reference arms them after every attempt, whether the
    /// use went out or was refused, so the retries are paced by the slot
    /// rather than by the heartbeat.
    /// </summary>
    private void ArmCorpseOpenSlots()
    {
        double openWindow = Math.Max(0.25d, _settings.CorpseOpenTimeoutSeconds);
        _actionLocks?.Arm(ActionLockKind.ItemUse, openWindow);
        _actionLocks?.Arm(ActionLockKind.Navigation, openWindow);
        _actionLocks?.Arm(ActionLockKind.CorpseOpenAttempt, openWindow);
    }

    /// <summary>
    /// How many times a corpse may fail to answer for itself before the
    /// looter stops asking. Three is enough to ride out a lost answer or a
    /// question crowded off the one asking slot, and short enough that a
    /// corpse that will never answer — the character's own old one, or
    /// anything else the server stays quiet about — stops holding the
    /// character still within seconds rather than for good.
    /// </summary>
    private const int CorpseDescriptionAttemptLimit = 3;

    /// <summary>
    /// Counts a description that never arrived, and after the third one
    /// skips the corpse for the profile's blacklist period. That is what
    /// takes it out of <see cref="HasCorpseAwaitingDescriptionWithin"/>, so
    /// a corpse nobody will ever describe stops holding every walk off.
    /// </summary>
    private void NoteCorpseDescriptionFailed(uint corpseId)
    {
        if (corpseId == 0u || IsCorpseBlacklisted(corpseId))
            return;
        _corpseDescriptionAttempts.TryGetValue(corpseId, out int attempts);
        attempts++;
        if (attempts < CorpseDescriptionAttemptLimit)
        {
            _corpseDescriptionAttempts[corpseId] = attempts;
            return;
        }
        _corpseDescriptionAttempts.Remove(corpseId);
        _corpseBlacklistedAt[corpseId] = _lifetime;
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"Skipping corpse 0x{corpseId:X8}: no description after "
                + $"{attempts} attempts, for "
                + $"{Math.Clamp(_settings.BlacklistCorpseOpenTimeoutSeconds, 1d, 3600d):0} seconds");
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

    /// <summary>
    /// A corpse this pass is done with, however it ended: emptied, or given
    /// up on because the client would not send a pull for it. Both endings
    /// close the container and let the corpse go; only the first records it
    /// as looted.
    /// </summary>
    private bool IsCorpseFinished(uint corpseId) =>
        corpseId != 0u
        && (_completedCorpses.ContainsKey(corpseId)
            || corpseId == _abandonedCorpse);

    /// <summary>
    /// Lets go of the corpse the pass is working when the client will not
    /// send a pull for what is in it. The corpse is skipped for the profile's
    /// blacklist period rather than recorded as looted — nothing was taken
    /// from it, and once the packs have room again it is worth another visit.
    /// </summary>
    private void GiveUpOnCorpse(uint corpseId, string reason)
    {
        if (corpseId == 0u)
            return;
        _abandonedCorpse = corpseId;
        _corpseBlacklistedAt[corpseId] = _lifetime;
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"LootCorpse: giving up on 0x{corpseId:X8}: {reason}");
    }

    private void MarkCorpseComplete(uint corpseId)
    {
        if (corpseId == 0u)
            return;
        _completedCorpses[corpseId] = _lifetime;
        _corpseOpenAttempts.Remove(corpseId);
        _corpseDescriptionAttempts.Remove(corpseId);
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
            _corpseDescriptionAttempts.Remove(id);
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
    /// A corpse is rare when the host flags it so, or when its description
    /// names its killer and then says it generated something, as the
    /// reference macro reads it.
    /// </summary>
    private static bool IsRare(in PluginLootContainer corpse)
    {
        if (corpse.IsGeneratedRare)
            return true;
        string description = corpse.LongDescription ?? string.Empty;
        return KilledByDescription().Match(description) is { Success: true, Index: 0 }
            && RareKillDescription().IsMatch(description);
    }

    [GeneratedRegex(@"(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)")]
    private static partial Regex KilledByDescription();

    /// <summary>
    /// A kill description that goes on to say the corpse generated something:
    /// "Killed by Name. This corpse generated a rare item!". The server does
    /// not send the corpse's rare flag with its description, so this sentence
    /// is how a rare corpse is known.
    /// </summary>
    [GeneratedRegex(@"(?:Killed by )([a-zA-Z\ \-\']*)(?:\..*)([gG]enerated)(?:.*)")]
    private static partial Regex RareKillDescription();
}
