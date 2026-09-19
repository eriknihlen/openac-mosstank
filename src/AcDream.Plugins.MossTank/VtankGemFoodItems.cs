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
            string itemName = row.Cells[name].AsString();
            int rawSpell = row.Cells[spell].AsInt();
            if (itemName.Length == 0 || rawSpell <= 0)
                continue;
            settings.GemFoodItems.Add(new GemFoodItem(itemName, unchecked((uint)rawSpell)));
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
        table.Rows.Clear();
        foreach (GemFoodItem entry in settings.GemFoodItems)
        {
            var row = new VtankRow();
            for (int column = 0; column < table.ColumnNames.Count; column++)
                row.Cells.Add(VtankCell.Int(0));
            row.Cells[name] = VtankCell.String(entry.Name);
            row.Cells[spell] = VtankCell.Int(unchecked((int)entry.SpellId));
            table.Rows.Add(row);
        }
    }
}
