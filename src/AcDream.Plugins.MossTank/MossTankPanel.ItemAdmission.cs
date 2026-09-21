using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private sealed record PendingProfileAddition(
        uint ObjectId, bool Consumable, bool NoBuffs, string Profile)
    {
        public double Elapsed { get; set; }
    }

    private PendingProfileAddition? _pendingProfileAddition;
    private readonly Dictionary<uint, double> _assessmentRetries = new();
    private double _assessmentTime;
    private double _nextAssessment;
    private double _nextAssessmentScan;

    private bool EnsureItemAssessed(uint objectId)
    {
        IAutomationSurface automation = _host.Automation;
        if (automation.Objects.TryGet(objectId, out PluginWorldObject world)
            && world.LastIdTime != 0)
        {
            _assessmentRetries.Remove(objectId);
            return true;
        }
        if (_assessmentRetries.TryGetValue(objectId, out double retryAt)
            && _assessmentTime < retryAt)
            return false;
        if (_assessmentTime < _nextAssessment || automation.Items.IsBusy)
            return false;
        PluginItemCommandResult result = automation.Objects.Identify(objectId);
        NoteAssessmentRequest(objectId, result);
        if (result.Accepted)
        {
            // One appraisal in flight at a time; its answer is what ends the
            // wait, the ten seconds only bound a lost one.
            _nextAssessment = _assessmentTime + 2d;
            _assessmentRetries[objectId] = _assessmentTime + 10d;
        }
        else
        {
            // A request the client did not send costs nothing and must not
            // hold the others: the item that cannot be assessed backs off by
            // itself, and the scan goes on to the next one. Otherwise the
            // first refused item in the bag kept every item after it
            // unassessed for the whole session.
            _assessmentRetries[objectId] = _assessmentTime
                + (result.Status == PluginItemCommandStatus.Busy ? 1d : 10d);
        }
        return false;
    }

    private readonly Dictionary<uint, string> _assessmentNotes = new();

    // Said once per item and outcome: a profile item the client never gets
    // to assess is a silent failure otherwise, and every rule that reads
    // the item waits on it.
    private void NoteAssessmentRequest(uint objectId, PluginItemCommandResult result)
    {
        string note = result.Status.ToString();
        if (_assessmentNotes.TryGetValue(objectId, out string? previous)
            && string.Equals(previous, note, StringComparison.Ordinal))
            return;
        _assessmentNotes[objectId] = note;
        string name = _host.Automation.Items.CaptureOwnedItems()
            .FirstOrDefault(item => item.ObjectId == objectId).Name ?? string.Empty;
        EmitMacroLog(MacroLogChannel.Misc, $"Assessing {name} (0x{objectId:X8}): {note}");
    }

    private void TickConfiguredItemAssessment(double elapsed)
    {
        _assessmentTime += Math.Max(0d, elapsed);
        if (_assessmentTime < _nextAssessmentScan)
            return;
        _nextAssessmentScan = _assessmentTime + 0.5d;
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        foreach (uint missing in _assessmentRetries.Keys
            .Where(id => !owned.Any(item => item.ObjectId == id)).ToArray())
            _assessmentRetries.Remove(missing);
        foreach (PluginInventoryItem item in owned)
        {
            // Worn gear is not in this scan: the worn-mana rule asks about it
            // on its own clock, and it has to keep asking, because gear that
            // has been appraised once never looks any emptier. Two owners
            // asking would only crowd the one appraisal the client sends at
            // a time.
            if (!_combatSettings.ConsumableNames.Contains(item.Name)
                && !_combatSettings.CombatItemNames.Contains(item.Name)
                && !_combatSettings.CombatItemObjectIds.Contains(item.ObjectId))
                continue;
            if (!EnsureItemAssessed(item.ObjectId))
                continue;
            // What the bag says about an item only fills in a kind nobody
            // stated. A profile that came in saying what this consumable is
            // for has said it, and looking at the item again must not
            // quietly overrule that.
            if (_combatSettings.ConsumableNames.Contains(item.Name)
                && !_combatSettings.ImportedAssistItems.Any(loaded =>
                    string.Equals(loaded.Name, item.Name, StringComparison.Ordinal))
                && _host.Automation.Items.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties)
                && ProfileItemAdmission.TryConsumable(item, properties, _host.Automation,
                    out ConsumableCategory category))
                _combatSettings.ConsumableCategories[item.Name] = category;
        }
    }

    private void BeginProfileItemAddition(bool consumable, bool noBuffs)
    {
        if (!TryGetSelectedInventoryItem(out PluginInventoryItem item))
        {
            _profileNotice = "Select an owned inventory item first.";
            return;
        }
        if (consumable ? _combatSettings.ConsumableNames.Contains(item.Name)
            : _combatSettings.CombatItemObjectIds.Contains(item.ObjectId))
        {
            _profileNotice = $"{item.Name} is already in this list.";
            return;
        }
        _pendingProfileAddition = new(item.ObjectId, consumable, noBuffs, _profiles.Selected);
        TickProfileItemAddition(0d);
    }

    private void TickProfileItemAddition(double elapsed)
    {
        if (_pendingProfileAddition is not { } pending)
            return;
        pending.Elapsed += Math.Max(0d, elapsed);
        if (pending.Profile != _profiles.Selected)
        {
            _pendingProfileAddition = null;
            return;
        }
        PluginInventoryItem item = _host.Automation.Items.CaptureOwnedItems()
            .FirstOrDefault(value => value.ObjectId == pending.ObjectId);
        if (item.ObjectId == 0u || pending.Elapsed >= 30d)
        {
            _pendingProfileAddition = null;
            _profileNotice = item.ObjectId == 0u ? "The selected item is no longer owned."
                : "Assessment did not complete. Select the item and try again.";
            return;
        }
        if (!EnsureItemAssessed(item.ObjectId))
        {
            _profileNotice = $"Assessing {item.Name} before adding it…";
            return;
        }
        _pendingProfileAddition = null;
        if (!_host.Automation.Items.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties))
        {
            _profileNotice = "Item properties are unavailable. Try again.";
            return;
        }
        if (pending.Consumable)
        {
            if (ProfileItemAdmission.TryConsumable(item, properties, _host.Automation,
                out ConsumableCategory category))
                CommitConsumable(item, category);
            else
                _profileNotice = $"{item.Name} is not a supported consumable.";
        }
        else if (!ItemEnchantDefaults.IsProfileEligible(item)
            || item.ValidLocations == ItemEnchantDefaults.MissileWeapon && item.AmmoType == 0u)
        {
            _profileNotice = $"{item.Name} is not supported in Items.";
        }
        else
        {
            CommitProfileItem(item, pending.NoBuffs || item.IsPetDevice
                || properties.Ints.ContainsKey(36u));
            if (item.IsPetDevice && item.SummoningMastery != 0
                && item.SummoningMastery != _host.Automation.Character.SummoningMastery)
                _profileNotice = $"Added {item.Name} without buffs. Warning: different summoning mastery.";
        }
    }
}

internal static class ProfileItemAdmission
{
    public static bool TryConsumable(PluginInventoryItem item, PluginItemProperties properties,
        IAutomationSurface automation, out ConsumableCategory category)
    {
        category = ConsumableCategory.Other;
        PluginObjectClass kind = item.ObjectClass;
        if (kind is PluginObjectClass.Food or PluginObjectClass.Gem
            or PluginObjectClass.HealingKit or PluginObjectClass.ManaStone)
        {
            if (kind == PluginObjectClass.ManaStone)
            {
                if (properties.Floats.GetValueOrDefault(137u) != 1d)
                    category = ConsumableCategory.ManaStone;
                else if (properties.Ints.GetValueOrDefault(108u, -1) <= 0
                    && properties.Ints.GetValueOrDefault(107u) > 0)
                    category = ConsumableCategory.ManaSource;
                return category != ConsumableCategory.Other;
            }
            if (kind == PluginObjectClass.HealingKit)
            {
                category = ConsumableClassifier.Classify(item);
                return true;
            }
            category = item.BoosterVital switch
            {
                2 => ConsumableCategory.HealthFood,
                4 => ConsumableCategory.StaminaFood,
                6 => ConsumableCategory.ManaFood,
                _ => ConsumableCategory.Other,
            };
            if (category != ConsumableCategory.Other)
                return true;
            return TryBuff(item, automation, out category);
        }
        if (kind == PluginObjectClass.MissileWeapon && item.AmmoType == 0u
            && item.Name.Contains("Phial", StringComparison.Ordinal))
            category = ConsumableCategory.Grenade;
        else if (kind == PluginObjectClass.SpellComponent
            && !item.Name.EndsWith(" Pea", StringComparison.Ordinal))
            category = ConsumableCategory.SplitComponent;
        else if (kind == PluginObjectClass.Lockpick)
            category = ConsumableCategory.Lockpick;
        else if (kind == PluginObjectClass.Misc)
            return TryBuff(item, automation, out category);
        return category != ConsumableCategory.Other;
    }

    private static bool TryBuff(PluginInventoryItem item, IAutomationSurface automation,
        out ConsumableCategory category)
    {
        category = ConsumableCategory.Other;
        if (item.AppraisedSpellIds.Count == 0
            || !automation.Spells.TryGet(item.AppraisedSpellIds[0], out PluginSpellInfo spell)
            || spell.School == 32u || spell.DurationSeconds < 300f)
            return false;
        category = ConsumableCategory.BuffConsumable;
        return true;
    }
}
