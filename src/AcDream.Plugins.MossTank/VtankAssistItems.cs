namespace AcDream.Plugins.MossTank;

internal sealed record AssistItem(string Name, ConsumableCategory Category);

internal static class VtankAssistItems
{
    public static void Read(VtankDatabase database, CombatSettings settings)
    {
        settings.ImportedAssistItems.Clear();
        VtankTable? table = database.Find("AssistItems");
        int name = table?.ColumnIndex("Object") ?? -1;
        int type = table?.ColumnIndex("Type") ?? -1;
        if (table is null || name < 0 || type < 0)
            return;
        foreach (VtankRow row in table.Rows)
        {
            try
            {
                string itemName = row.Cells[name].AsString();
                if (itemName.Length == 0 || !TryMap(row.Cells[type].AsInt(), out ConsumableCategory category))
                    continue;
                settings.ImportedAssistItems.Add(new AssistItem(itemName, category));
            }
            catch (FormatException) { }
            catch (OverflowException) { }
        }
    }

    public static void Apply(CombatSettings settings)
    {
        foreach (AssistItem item in settings.ImportedAssistItems)
        {
            settings.ConsumableNames.Add(item.Name);
            settings.ConsumableCategories[item.Name] = item.Category;
        }
    }

    private static bool TryMap(int type, out ConsumableCategory category)
    {
        category = type switch
        {
            0 => ConsumableCategory.HealthKit, 1 => ConsumableCategory.HealthFood,
            2 => ConsumableCategory.StaminaKit, 3 => ConsumableCategory.StaminaFood,
            4 => ConsumableCategory.ManaKit, 5 => ConsumableCategory.ManaFood,
            6 => ConsumableCategory.ManaSource, 7 => ConsumableCategory.Grenade,
            8 => ConsumableCategory.ManaStone, 9 => ConsumableCategory.Pea,
            10 => ConsumableCategory.Lockpick, 11 => ConsumableCategory.AllPeas,
            _ => ConsumableCategory.Other,
        };
        return type is >= 0 and <= 11;
    }
}
