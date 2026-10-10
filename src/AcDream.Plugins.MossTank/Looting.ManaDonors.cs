using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class LootController
{
    private readonly ManaDonorJournal _manaDonors;

    internal void ObserveManaDonors()
    {
        IAutomationSurface automation = _host.Automation;
        if (!automation.IsAvailable || !automation.Character.IsInWorld || !automation.Items.IsAvailable)
            return;
        _manaDonors.Bind(automation.Character.WorldName, automation.Character.Name);
        if (!_manaDonors.CanOperate)
            return;
        if (!_manaDonors.Entries.Any() && !_classifiedOwnedItems.Values.Contains(LootAction.ManaTank))
            return;
        IReadOnlyList<PluginInventoryItem> owned = automation.Items.CaptureOwnedItems();
        _manaDonors.Reconcile(owned, automation.Items.IsOwnedInventoryComplete);
        foreach (uint id in _classifiedOwnedItems.Where(pair => pair.Value == LootAction.ManaTank)
                     .Select(pair => pair.Key).ToArray())
            _classifiedOwnedItems.Remove(id);
        foreach (ManaDonorJournal.Entry donor in _manaDonors.Entries)
        {
            if (!donor.Removed && owned.Any(item => item.ObjectId == donor.ObjectId))
                _classifiedOwnedItems[donor.ObjectId] = LootAction.ManaTank;
        }
    }

    private void RememberManaDonor(PluginInventoryItem item)
    {
        _manaDonors.Bind(_host.Automation.Character.WorldName, _host.Automation.Character.Name);
        if (!_manaDonors.Remember(item))
            Status = "Mana donor history unavailable; draining paused.";
    }
}
