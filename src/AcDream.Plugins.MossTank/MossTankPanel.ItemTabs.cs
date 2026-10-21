using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The Items and Consumables pages, laid out the way VTank shows them.
/// </summary>
/// <remarks>
/// Items lists one row per item the profile buffs, by object id, in the order
/// the profile first names each one. The first spell the profile lists for an
/// item decides whether it is shown as carrying no buffs. The Hands column is
/// the profile's choice of which hand an item is used in, kept in the profile
/// and read by combat. Consumables lists the gems and food first, then every
/// other consumable.
/// </remarks>
internal sealed partial class MossTankPanel
{
    /// <summary>The colour an ordinary Items row draws in.</summary>
    private const uint ItemRowColor = 0xE8DEC3u;

    /// <summary>The colour an item carrying no buffs draws in.</summary>
    private const uint ItemNoBuffsColor = 0xFF0000u;

    /// <summary>The integer property holding the slots an item may be worn in.</summary>
    private const uint ValidLocationsProperty = 9u;

    private IReadOnlyList<string> _itemRows = Array.Empty<string>();
    private IReadOnlyList<string> _itemRowNames = Array.Empty<string>();
    private IReadOnlyList<uint?> _itemRowObjectIds = Array.Empty<uint?>();
    private IReadOnlyList<string> _itemHandsColumn = Array.Empty<string>();
    private IReadOnlyList<uint> _itemNameColors = Array.Empty<uint>();

    private Action<PluginObjectChange>? _itemListObjectChanged;
    private Action? _itemListLoginComplete;
    private bool _itemListStale;

    /// <summary>
    /// The Items page names an item the client knows and shows the id of
    /// one it does not, so the page is built again when a listed item
    /// arrives, changes or leaves, and on entering the world. The rebuild
    /// waits for the next tick, so a burst of changes costs one rebuild.
    /// </summary>
    private void AttachItemListRefresh(IEvents events)
    {
        _itemListObjectChanged = OnItemListObjectChanged;
        _itemListLoginComplete = MarkItemListStale;
        events.ObjectChanged += _itemListObjectChanged;
        events.LoginComplete += _itemListLoginComplete;
    }

    private void DetachItemListRefresh(IEvents events)
    {
        if (_itemListObjectChanged is not null)
            events.ObjectChanged -= _itemListObjectChanged;
        if (_itemListLoginComplete is not null)
            events.LoginComplete -= _itemListLoginComplete;
        _itemListObjectChanged = null;
        _itemListLoginComplete = null;
    }

    private void OnItemListObjectChanged(PluginObjectChange change)
    {
        if (change.Kind == PluginObjectChangeKind.Moved)
            return;
        if (_combatSettings.CombatItemObjectIds.Contains(change.ObjectId))
            _itemListStale = true;
    }

    private void MarkItemListStale() => _itemListStale = true;

    private void RebuildItemListIfStale()
    {
        if (!_itemListStale)
            return;
        _itemListStale = false;
        RebuildItemList();
    }

    private void RebuildItemList()
    {
        uint characterId = _host.Automation.Character.ObjectId;
        var ownedById = new Dictionary<uint, PluginInventoryItem>();
        foreach (PluginInventoryItem item in _host.Automation.Items.CaptureOwnedItems())
            ownedById.TryAdd(item.ObjectId, item);
        var firstSpellById = new Dictionary<uint, uint>();
        foreach (BuffItemEnchantRow row in _buffSettings.ItemEnchantRows)
        {
            if (row.IsProfiledItemRow)
                firstSpellById.TryAdd(row.ObjectId!.Value, row.SpellId!.Value);
        }

        var rows = new List<string>();
        var names = new List<string>();
        var ids = new List<uint?>();
        var hands = new List<string>();
        var colors = new List<uint>();
        var listed = new HashSet<uint>();
        foreach (uint objectId in _combatSettings.CombatItemOrderIds)
        {
            // The "any equipped weapon" entry and the character itself are
            // never rows.
            if (objectId == uint.MaxValue
                || (characterId != 0u && objectId == characterId)
                || !listed.Add(objectId))
            {
                continue;
            }
            string name;
            bool showHands;
            if (TryResolveListedItem(objectId, ownedById, out name, out uint validLocations))
            {
                _combatSettings.CombatItemNamesById[objectId] = name;
                showHands = (validLocations & ItemEnchantDefaults.MeleeWeapon) != 0u;
            }
            else
            {
                name = $"<INVALID {unchecked((int)objectId)}>";
                showHands = true;
            }
            bool noBuffs = firstSpellById.TryGetValue(objectId, out uint firstSpell)
                && firstSpell == uint.MaxValue;
            rows.Add(noBuffs ? name + " (NO BUFFS)" : name);
            names.Add(name);
            ids.Add(objectId);
            colors.Add(noBuffs ? ItemNoBuffsColor : ItemRowColor);
            hands.Add(showHands
                ? ItemUseLabel(VtankItemUseSpecifiers.UsesFor(_combatSettings, objectId))
                : string.Empty);
        }
        // A profile that lists its items by name alone has them used by
        // name, so they stay on the page where they can be removed.
        if (_combatSettings.CombatItemObjectIds.Count == 0)
        {
            foreach (string name in SortedCombatItemNames())
            {
                rows.Add(name);
                names.Add(name);
                ids.Add(null);
                colors.Add(ItemRowColor);
                hands.Add(string.Empty);
            }
        }
        _itemRows = rows;
        _itemRowNames = names;
        _itemRowObjectIds = ids;
        _itemHandsColumn = hands;
        _itemNameColors = colors;
        _selectedItemRow = ClampRow(_selectedItemRow, _itemRows.Count);
    }

    /// <summary>
    /// The name and wearable slots of an item the client knows: from the
    /// inventory when the character holds it, else from the object table.
    /// </summary>
    private bool TryResolveListedItem(
        uint objectId,
        IReadOnlyDictionary<uint, PluginInventoryItem> ownedById,
        out string name,
        out uint validLocations)
    {
        if (ownedById.TryGetValue(objectId, out PluginInventoryItem owned))
        {
            name = owned.Name;
            validLocations = owned.ValidLocations;
            return true;
        }
        IWorldObjectAutomation objects = _host.Automation.Objects;
        if (objects.TryGet(objectId, out PluginWorldObject world))
        {
            name = world.Name;
            validLocations = objects.TryGetIntProperty(
                objectId, ValidLocationsProperty, out int slots)
                ? unchecked((uint)slots)
                : 0u;
            return true;
        }
        name = string.Empty;
        validLocations = 0u;
        return false;
    }

    /// <summary>
    /// The Hands label for an item's hand choice: none, the right, the left
    /// or both. A value that is none of those shows nothing and cannot be
    /// stepped on from the page.
    /// </summary>
    internal static string ItemUseLabel(int uses) => uses switch
    {
        0 => "DONOTUSE",
        1 => "RightHand",
        2 => "LeftHand",
        3 => "BothHands",
        _ => string.Empty,
    };

    /// <summary>The next hand choice, in the order the page steps through them.</summary>
    internal static int NextItemUse(int uses) => uses switch
    {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => 0,
    };

    private void DeleteItemRowAtCore(int row)
    {
        if ((uint)row >= (uint)_itemRows.Count)
            return;
        string removed = _itemRowNames[row];
        if (_itemRowObjectIds[row] is uint objectId)
            RemoveListedItem(objectId);
        else
            RemoveNamedItem(removed);
        RefreshItemEditors();
        _profileNotice = SaveProfile()
            ? $"Removed {removed}."
            : UnsavedProfileNotice($"Removed {removed}");
    }

    /// <summary>
    /// Take one item off the page and out of the profile: every spell row it
    /// has, its hand choice, and what was kept about it by name beside the
    /// profile unless another listed item carries the same name.
    /// </summary>
    private void RemoveListedItem(uint objectId)
    {
        for (int index = _buffSettings.ItemEnchantRows.Count - 1; index >= 0; index--)
        {
            BuffItemEnchantRow candidate = _buffSettings.ItemEnchantRows[index];
            if (candidate.IsProfiledItemRow && candidate.ObjectId == objectId)
                _buffSettings.ItemEnchantRows.RemoveAt(index);
        }
        _combatSettings.CombatItemObjectIds.Remove(objectId);
        _combatSettings.CombatItemOrderIds.Remove(objectId);
        _combatSettings.RemovedCombatItemObjectIds.Add(objectId);
        _combatSettings.ItemUseSpecifiers.Remove(objectId);
        if (!_combatSettings.CombatItemNamesById.Remove(objectId, out string? name)
            || string.IsNullOrWhiteSpace(name)
            || _combatSettings.CombatItemNamesById.Values.Contains(name, StringComparer.Ordinal))
        {
            return;
        }
        RemoveNamedItem(name);
    }

    private void RemoveNamedItem(string name)
    {
        _combatSettings.CombatItemNames.Remove(name);
        _combatSettings.CombatItemOrder.Remove(name);
        _noBuffItemNames.Remove(name);
        ClearItemEnchantRows(name);
    }

    private void CycleItemHandsAtCore(int row)
    {
        if ((uint)row >= (uint)_itemRows.Count
            || _itemHandsColumn[row].Length == 0
            || _itemRowObjectIds[row] is not uint objectId)
        {
            return;
        }
        _combatSettings.ItemUseSpecifiers[objectId] =
            NextItemUse(VtankItemUseSpecifiers.UsesFor(_combatSettings, objectId));
        RebuildItemList();
        SaveProfile();
    }

    private void RemoveSelectedItemCore()
    {
        if (_itemRows.Count == 0)
        {
            AnnounceProfileNotice("The Items profile is empty.");
            return;
        }
        DeleteItemRowAtCore(ClampRow(_selectedItemRow, _itemRows.Count));
    }

    /// <summary>
    /// The gems and food first, then every other consumable, each in the
    /// order the profile lists it.
    /// </summary>
    private void RebuildConsumableList()
    {
        var rows = new List<string>();
        var gems = new HashSet<string>(StringComparer.Ordinal);
        foreach (GemFoodItem gem in _buffSettings.GemFoodItems)
        {
            if (gems.Add(gem.Name))
                rows.Add(gem.Name);
        }
        var others = new HashSet<string>(StringComparer.Ordinal);
        foreach (AssistItem item in _combatSettings.ImportedAssistItems)
        {
            if (_combatSettings.ConsumableNames.Contains(item.Name) && others.Add(item.Name))
                rows.Add(item.Name);
        }
        foreach (string name in _combatSettings.ConsumableNames)
        {
            if (others.Add(name))
                rows.Add(name);
        }
        _consumableRows = rows;
        _selectedConsumableRow = ClampRow(_selectedConsumableRow, _consumableRows.Count);
    }

    /// <summary>
    /// Remove the selected consumable: from the gems and food when the name
    /// is one of them, else from the other consumables.
    /// </summary>
    private void RemoveSelectedConsumableCore()
    {
        if (_consumableRows.Count == 0)
        {
            _profileNotice = "The Consumables profile is empty.";
            return;
        }
        string removed = _consumableRows[ClampRow(_selectedConsumableRow, _consumableRows.Count)];
        bool gemOrFood = false;
        for (int i = _buffSettings.GemFoodItems.Count - 1; i >= 0; i--)
        {
            if (!string.Equals(_buffSettings.GemFoodItems[i].Name, removed, StringComparison.Ordinal))
                continue;
            _buffSettings.GemFoodItems.RemoveAt(i);
            gemOrFood = true;
        }
        if (!gemOrFood)
        {
            _combatSettings.ConsumableNames.Remove(removed);
            _combatSettings.ConsumableCategories.Remove(removed);
            for (int i = _combatSettings.ImportedAssistItems.Count - 1; i >= 0; i--)
            {
                if (string.Equals(_combatSettings.ImportedAssistItems[i].Name, removed, StringComparison.Ordinal))
                    _combatSettings.ImportedAssistItems.RemoveAt(i);
            }
        }
        _profileNotice = $"Removed {removed}.";
        RefreshItemEditors();
        SaveProfile();
    }
}
