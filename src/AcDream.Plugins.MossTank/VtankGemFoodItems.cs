namespace AcDream.Plugins.MossTank;

internal static class VtankGemFoodItems
{
    private const string TableName = "GemFoodItems";

    public static void Read(VtankDatabase database, BuffSettings settings)
    {
        settings.GemFoodItems.Clear();
        VtankTable? table = database.Find(TableName);
        int name = table?.ColumnIndex("Name") ?? -1;
        int spell = table?.ColumnIndex("Spell") ?? -1;
        if (table is null || name < 0 || spell < 0)
            return;
        foreach (VtankRow row in table.Rows)
        {
            if (!TryReadItem(row, name, spell, out GemFoodItem item))
                continue;
            settings.GemFoodItems.Add(item);
        }
    }

    public static void Write(VtankDatabase database, BuffSettings settings)
    {
        VtankTable? table = database.Find(TableName);
        if (table is null)
        {
            if (settings.GemFoodItems.Count == 0)
                return;
            table = new VtankTable();
            table.ColumnNames.AddRange(["Name", "Spell"]);
            table.IndexFlags.AddRange([false, false]);
            database.Tables.Add((TableName, table));
        }
        int name = table.ColumnIndex("Name");
        int spell = table.ColumnIndex("Spell");
        if (name < 0 || spell < 0)
            throw new FormatException("'GemFoodItems' table is missing Name/Spell columns.");
        var remaining = new List<GemFoodItem>(settings.GemFoodItems);
        var matches = new GemFoodItem?[table.Rows.Count];
        for (int rowIndex = 0; rowIndex < table.Rows.Count; rowIndex++)
        {
            VtankRow row = table.Rows[rowIndex];
            if (!TryReadItem(row, name, spell, out GemFoodItem source))
                continue;
            int entryIndex = remaining.FindIndex(entry =>
                entry.Name.Equals(source.Name, StringComparison.Ordinal)
                && entry.SpellId == source.SpellId);
            if (entryIndex < 0)
                continue;
            matches[rowIndex] = remaining[entryIndex];
            remaining.RemoveAt(entryIndex);
        }
        for (int rowIndex = 0; rowIndex < table.Rows.Count; rowIndex++)
        {
            if (matches[rowIndex] is not null
                || !TryReadItem(table.Rows[rowIndex], name, spell, out GemFoodItem source))
            {
                continue;
            }
            int entryIndex = remaining.FindIndex(entry =>
                entry.Name.Equals(source.Name, StringComparison.Ordinal));
            if (entryIndex < 0)
                continue;
            matches[rowIndex] = remaining[entryIndex];
            remaining.RemoveAt(entryIndex);
        }
        for (int rowIndex = table.Rows.Count - 1; rowIndex >= 0; rowIndex--)
        {
            VtankRow row = table.Rows[rowIndex];
            if (!TryReadItem(row, name, spell, out GemFoodItem source))
                continue;
            GemFoodItem? entry = matches[rowIndex];
            if (entry is null)
            {
                table.Rows.RemoveAt(rowIndex);
                continue;
            }
            if (entry.SpellId != source.SpellId)
                row.Cells[spell] = VtankCell.Int(unchecked((int)entry.SpellId));
        }

        foreach (GemFoodItem entry in remaining)
        {
            var row = new VtankRow();
            for (int column = 0; column < table.ColumnNames.Count; column++)
                row.Cells.Add(VtankCell.Int(0));
            row.Cells[name] = VtankCell.String(entry.Name);
            row.Cells[spell] = VtankCell.Int(unchecked((int)entry.SpellId));
            table.Rows.Add(row);
        }
    }

    private static bool TryReadItem(
        VtankRow row, int nameColumn, int spellColumn, out GemFoodItem item)
    {
        item = null!;
        if ((uint)nameColumn >= (uint)row.Cells.Count
            || (uint)spellColumn >= (uint)row.Cells.Count)
        {
            return false;
        }
        try
        {
            string name = row.Cells[nameColumn].AsString();
            int spell = row.Cells[spellColumn].AsInt();
            if (name.Length == 0 || spell <= 0)
                return false;
            item = new GemFoodItem(name, unchecked((uint)spell));
            return true;
        }
        catch (FormatException)
        {
            return false;
        }
        catch (OverflowException)
        {
            return false;
        }
    }
}
