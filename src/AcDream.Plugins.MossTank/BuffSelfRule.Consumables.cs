using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class BuffSelfRule
{
    private CombatSettings? _consumableSettings;
    private ActionLockTable _consumableLocks = new();
    private readonly Dictionary<uint, double> _consumableRetryAt = new();
    private uint _pendingConsumable;
    private uint _pendingConsumableSpell;
    private uint _pendingConsumableFamily;
    private readonly Dictionary<uint, double> _consumableFamilyRetryAt = new();
    private long _consumableCompletion;
    private double _consumableDeadline;

    internal void BindConsumables(CombatSettings settings, ActionLockTable locks)
    {
        _consumableSettings = settings;
        _consumableLocks = locks;
    }

    private bool TryPickConsumable(IAutomationSurface automation, double threshold,
        out BuffPick pick)
    {
        pick = default;
        if (_consumableSettings is null || !automation.Items.IsAvailable
            || _consumableLocks.IsLocked(ActionLockKind.ItemUse))
            return false;
        foreach (PluginInventoryItem item in automation.Items.CaptureOwnedItems())
        {
            if (!_consumableSettings.ConsumableNames.Contains(item.Name)
                || !_consumableSettings.ConsumableCategories.TryGetValue(item.Name, out var category)
                || category != ConsumableCategory.BuffConsumable
                || _consumableRetryAt.TryGetValue(item.ObjectId, out double retryAt)
                    && retryAt > _nowSeconds
                || !automation.Objects.TryGet(item.ObjectId, out PluginWorldObject world)
                || world.LastIdTime == 0
                || item.AppraisedSpellIds.Count == 0
                || !automation.Spells.TryGet(item.AppraisedSpellIds[0], out PluginSpellInfo spell)
                || _consumableFamilyRetryAt.TryGetValue(spell.Family, out double familyRetryAt)
                    && familyRetryAt > _nowSeconds
                || spell.IsFellowship && !automation.Fellowship.IsInFellowship
                || !automation.Items.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties))
                continue;
            if (item.UseRequiresSkill > 0
                && (!automation.Character.TryGetSkill((uint)item.UseRequiresSkill, out PluginSkillInfo skill)
                    || skill.Training is PluginSkillTraining.Unknown or PluginSkillTraining.Untrained
                    || skill.Current < item.UseRequiresSkillLevel
                    || item.UseRequiresSkillSpecialized != 0
                        && skill.Training != PluginSkillTraining.Specialized))
                continue;
            if (properties.Ints.TryGetValue(280u, out int cooldown)
                && cooldown > 0 && automation.Spells.GetCooldownRemaining((uint)cooldown) > 0d)
                continue;
            if (automation.Character.TimedEnchantments.Any(effect =>
                effect.Family == spell.Family && effect.Tier >= spell.Tier
                && !_buffDue.ForcedSpellIds.Contains(effect.SpellId)
                && effect.SecondsRemaining >= threshold))
                continue;
            pick = new BuffPick(spell, automation.Character.ObjectId, item.Name, false, item.ObjectId);
            return true;
        }
        return false;
    }

    private bool UseBuffConsumable(IAutomationSurface automation, BuffPick pick)
    {
        if (_consumableLocks.IsLocked(ActionLockKind.ItemUse))
            return true;
        long revision = automation.Items.LastCompletion.Revision;
        PluginItemCommandResult result = automation.Items.Use(pick.ConsumableObjectId);
        _owner.SetStatus($"Using {pick.TargetName} for {pick.Spell.Name}");
        if (!result.Accepted)
        {
            _consumableRetryAt[pick.ConsumableObjectId] = _nowSeconds + 5d;
            return result.Status == PluginItemCommandStatus.Busy;
        }
        _pendingConsumable = pick.ConsumableObjectId;
        _pendingConsumableSpell = pick.Spell.SpellId;
        _pendingConsumableFamily = pick.Spell.Family;
        _consumableCompletion = revision;
        _consumableDeadline = _nowSeconds + 15d;
        _consumableLocks.Arm(ActionLockKind.ItemUse, ItemUseLock.TransactionSeconds);
        return true;
    }

    private void ObserveConsumableUse()
    {
        if (_pendingConsumable == 0u)
            return;
        PluginItemUseCompletion completion = _host.Automation.Items.LastCompletion;
        bool completed = completion.Revision > _consumableCompletion
            && completion.SourceObjectId == _pendingConsumable;
        if (!completed && _nowSeconds < _consumableDeadline)
        {
            _consumableLocks.Arm(ActionLockKind.ItemUse, ItemUseLock.TransactionSeconds);
            return;
        }
        // Allow the effect update to arrive after the item-use acknowledgement.
        _consumableRetryAt[_pendingConsumable] = _nowSeconds
            + (completed && completion.IsSuccess ? 5d : 30d);
        _consumableFamilyRetryAt[_pendingConsumableFamily] = _consumableRetryAt[_pendingConsumable];
        if (completed && completion.IsSuccess)
            _buffDue.NoteRecast(_pendingConsumableSpell);
        if (!completed || !completion.IsSuccess)
            _owner.Log(MacroLogChannel.Misc, "Buff consumable failed or timed out; delaying retry.");
        ClearConsumableUse();
    }

    private void ClearConsumableUse()
    {
        if (_pendingConsumable != 0u)
            _consumableLocks.Release(ActionLockKind.ItemUse);
        _pendingConsumable = 0u;
        _pendingConsumableSpell = 0u;
        _pendingConsumableFamily = 0u;
        _consumableCompletion = 0;
        _consumableDeadline = 0d;
    }
}
