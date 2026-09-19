namespace AcDream.Plugins.MossTank;

internal static class VtankProfiledItemIds
{
    private const string TableName = "BuffedItems";

    public static void Read(VtankDatabase database, CombatSettings settings,
        uint playerObjectId)
    {
        settings.CombatItemOrderIds.Clear();
        settings.CombatItemObjectIds.Clear();
        settings.RemovedCombatItemObjectIds.Clear();

        VtankTable? table = database.Find(TableName);
        int objectColumn = table?.ColumnIndex("Object") ?? -1;
        if (table is null || objectColumn < 0)
            return;

        foreach (VtankRow row in table.Rows)
        {
            int rawId = row.Cells[objectColumn].AsInt();
            if (rawId == -1)
                continue;
            uint id = unchecked((uint)rawId);
            if (id == playerObjectId || !settings.CombatItemObjectIds.Add(id))
                continue;
            settings.CombatItemOrderIds.Add(id);
        }
    }

    public static void Write(VtankDatabase database, CombatSettings settings)
    {
        VtankTable? table = database.Find(TableName);
        if (table is null)
        {
            if (settings.CombatItemOrderIds.Count == 0)
                return;
            table = new VtankTable();
            table.ColumnNames.AddRange(["Object", "Spell"]);
            table.IndexFlags.AddRange([false, false]);
            database.Tables.Add((TableName, table));
        }
        int objectColumn = table.ColumnIndex("Object");
        int spellColumn = table.ColumnIndex("Spell");
        if (objectColumn < 0 || spellColumn < 0)
            throw new FormatException("'BuffedItems' table is missing Object/Spell columns.");

        if (settings.RemovedCombatItemObjectIds.Count != 0)
        {
            table.Rows.RemoveAll(row => settings.RemovedCombatItemObjectIds.Contains(
                unchecked((uint)row.Cells[objectColumn].AsInt())));
            settings.RemovedCombatItemObjectIds.Clear();
        }

        var present = new HashSet<uint>();
        foreach (VtankRow row in table.Rows)
            present.Add(unchecked((uint)row.Cells[objectColumn].AsInt()));
        foreach (uint id in settings.CombatItemOrderIds)
        {
            if (id is 0u or uint.MaxValue || !present.Add(id))
                continue;
            var row = new VtankRow();
            for (int column = 0; column < table.ColumnNames.Count; column++)
                row.Cells.Add(VtankCell.Int(-1));
            row.Cells[objectColumn] = VtankCell.Int(unchecked((int)id));
            table.Rows.Add(row);
        }
    }
}
