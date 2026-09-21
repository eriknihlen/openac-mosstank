using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class BuffSelfRule
{
    private string _timerWorld = string.Empty;
    private string _timerCharacter = string.Empty;

    internal void EnsureTimerPersistence()
    {
        ICharacterInfo character = _host.Automation.Character;
        string world = character.WorldName.Trim();
        string name = character.Name.Trim();
        if (!_host.Automation.IsAvailable || world.Length == 0 || name.Length == 0
            || !_host.Storage.IsAvailable
            || world.Equals(_timerWorld, StringComparison.OrdinalIgnoreCase)
                && name.Equals(_timerCharacter, StringComparison.OrdinalIgnoreCase))
            return;
        _itemLedger.BindPersistence(_host.Storage, world, name, _nowSeconds,
            message => _owner.Log(MacroLogChannel.Misc, message));
        _timerWorld = world;
        _timerCharacter = name;
    }

    internal void InvalidateItemTimers()
    {
        ClearCastAttempt();
        ClearConsumableUse();
        _itemLedger.InvalidateAll();
        _host.Automation.Enchantments.ForgetReported();
        _buffDue.CancelForce();
    }

    private void ResetTimerScope()
    {
        _timerWorld = string.Empty;
        _timerCharacter = string.Empty;
    }
}
