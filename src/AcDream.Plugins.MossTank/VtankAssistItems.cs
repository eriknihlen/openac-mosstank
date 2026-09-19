namespace AcDream.Plugins.MossTank;

internal sealed record AssistItem(string Name, ConsumableCategory Category);

internal static class VtankAssistItems
{
    private const string TableName = "AssistItems";

    public static void Read(VtankDatabase database, CombatSettings settings)
    {
        settings.ImportedAssistItems.Clear();
        VtankTable? table = database.Find(TableName);
        int name = table?.ColumnIndex("Object") ?? -1;
        int type = table?.ColumnIndex("Type") ?? -1;
        if (table is null || name < 0 || type < 0)
            return;
        foreach (VtankRow row in table.Rows)
        {
            if (!TryRead(row, name, type, out AssistItem item))
                continue;
            settings.ImportedAssistItems.Add(item);
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

    /// <summary>
    /// Update only rows whose reference kind is understood. Invalid and
    /// extension rows remain byte-for-byte represented by their original
    /// cells, while exact selected rows retain their custom columns.
    /// </summary>
    public static void Write(VtankDatabase database, CombatSettings settings)
    {
        ArgumentNullException.ThrowIfNull(database);
        ArgumentNullException.ThrowIfNull(settings);
        VtankTable? table = database.Find(TableName);
        var desired = settings.ImportedAssistItems
            .Where(item => !settings.ConsumableCategories.ContainsKey(item.Name))
            .Concat(settings.ConsumableNames
                .Where(name => settings.ConsumableCategories.TryGetValue(name, out _))
                .Select(name => new AssistItem(name, settings.ConsumableCategories[name])))
            .Where(static item => TryUnmap(item.Category, out _))
            .ToHashSet();
        if (table is null)
        {
            if (desired.Count == 0)
                return;
            table = new VtankTable();
            table.ColumnNames.AddRange(["Object", "Type"]);
            table.IndexFlags.AddRange([false, false]);
            database.Tables.Add((TableName, table));
        }
        int name = table.ColumnIndex("Object");
        int type = table.ColumnIndex("Type");
        if (name < 0 || type < 0)
            throw new FormatException("'AssistItems' table is missing Object/Type columns.");

        var exact = new HashSet<AssistItem>();
        foreach (VtankRow row in table.Rows)
        {
            if (TryRead(row, name, type, out AssistItem item) && desired.Contains(item))
                exact.Add(item);
        }
        foreach (AssistItem item in desired.Where(item => !exact.Contains(item)))
        {
            VtankRow? fallback = table.Rows.FirstOrDefault(row =>
                TryRead(row, name, type, out AssistItem existing)
                && string.Equals(existing.Name, item.Name, StringComparison.Ordinal));
            if (fallback is null)
            {
                fallback = new VtankRow();
                for (int column = 0; column < table.ColumnNames.Count; column++)
                    fallback.Cells.Add(VtankCell.Int(-1));
                fallback.Cells[name] = VtankCell.String(item.Name);
                table.Rows.Add(fallback);
            }
            fallback.Cells[type] = VtankCell.Int(Unmap(item.Category));
        }
        table.Rows.RemoveAll(row => TryRead(row, name, type, out AssistItem item)
            && !desired.Contains(item));
    }

    internal static bool TryMap(int type, out ConsumableCategory category)
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

    private static bool TryRead(VtankRow row, int name, int type, out AssistItem item)
    {
        item = default!;
        if ((uint)name >= (uint)row.Cells.Count || (uint)type >= (uint)row.Cells.Count)
            return false;
        try
        {
            string itemName = row.Cells[name].AsString();
            if (itemName.Length == 0 || !TryMap(row.Cells[type].AsInt(), out ConsumableCategory category))
                return false;
            item = new AssistItem(itemName, category);
            return true;
        }
        catch (FormatException) { return false; }
        catch (OverflowException) { return false; }
    }

    private static bool TryUnmap(ConsumableCategory category, out int type)
    {
        type = category switch
        {
            ConsumableCategory.HealthKit => 0, ConsumableCategory.HealthFood => 1,
            ConsumableCategory.StaminaKit => 2, ConsumableCategory.StaminaFood => 3,
            ConsumableCategory.ManaKit => 4, ConsumableCategory.ManaFood => 5,
            ConsumableCategory.ManaSource => 6, ConsumableCategory.Grenade => 7,
            ConsumableCategory.ManaStone => 8, ConsumableCategory.Pea => 9,
            ConsumableCategory.Lockpick => 10, ConsumableCategory.AllPeas => 11,
            _ => -1,
        };
        return type >= 0;
    }

    private static int Unmap(ConsumableCategory category)
    {
        _ = TryUnmap(category, out int type);
        return type;
    }
}
