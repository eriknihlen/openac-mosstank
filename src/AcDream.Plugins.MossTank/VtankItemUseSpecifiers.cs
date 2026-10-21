namespace AcDream.Plugins.MossTank;

internal static class VtankItemUseSpecifiers
{
    private const string TableName = "ItemUseSpecifiers";

    public static void Read(VtankDatabase database, CombatSettings settings)
    {
        settings.ItemUseSpecifiers.Clear();
        VtankTable? table = database.Find(TableName);
        int objectColumn = table?.ColumnIndex("Object") ?? -1;
        int usesColumn = table?.ColumnIndex("Uses") ?? -1;
        if (table is null || objectColumn < 0 || usesColumn < 0)
            return;
        foreach (VtankRow row in table.Rows)
        {
            if (!TryRead(row, objectColumn, usesColumn, out uint id, out int uses))
                continue;
            settings.ItemUseSpecifiers[id] = uses;
        }
    }

    public static int UsesFor(CombatSettings settings, uint objectId) =>
        settings.ItemUseSpecifiers.TryGetValue(objectId, out int uses) ? uses : 3;

    /// <summary>
    /// Save each item's hand choice: a changed choice is rewritten where it
    /// stands, one no longer held is dropped, a new one is added. A row whose
    /// cells cannot be read is left as it is.
    /// </summary>
    public static void Write(VtankDatabase database, CombatSettings settings)
    {
        ArgumentNullException.ThrowIfNull(database);
        ArgumentNullException.ThrowIfNull(settings);
        VtankTable? table = database.Find(TableName);
        if (table is null)
        {
            if (settings.ItemUseSpecifiers.Count == 0)
                return;
            table = new VtankTable();
            table.ColumnNames.AddRange(["Object", "Uses"]);
            table.IndexFlags.AddRange([true, false]);
            database.Tables.Add((TableName, table));
        }
        int objectColumn = table.ColumnIndex("Object");
        int usesColumn = table.ColumnIndex("Uses");
        if (objectColumn < 0 || usesColumn < 0)
            throw new FormatException("'ItemUseSpecifiers' table is missing Object/Uses columns.");

        var present = new HashSet<uint>();
        for (int index = table.Rows.Count - 1; index >= 0; index--)
        {
            VtankRow row = table.Rows[index];
            if (!TryRead(row, objectColumn, usesColumn, out uint id, out int uses))
                continue;
            if (!settings.ItemUseSpecifiers.TryGetValue(id, out int wanted))
            {
                table.Rows.RemoveAt(index);
                continue;
            }
            present.Add(id);
            if (uses != wanted)
                row.Cells[usesColumn] = VtankCell.Int(wanted);
        }
        foreach ((uint id, int uses) in settings.ItemUseSpecifiers)
        {
            if (present.Contains(id))
                continue;
            var row = new VtankRow();
            for (int column = 0; column < table.ColumnNames.Count; column++)
                row.Cells.Add(VtankCell.Int(0));
            row.Cells[objectColumn] = VtankCell.Int(unchecked((int)id));
            row.Cells[usesColumn] = VtankCell.Int(uses);
            table.Rows.Add(row);
        }
    }

    private static bool TryRead(VtankRow row, int objectColumn, int usesColumn,
        out uint objectId, out int uses)
    {
        objectId = 0u;
        uses = 0;
        if ((uint)objectColumn >= (uint)row.Cells.Count || (uint)usesColumn >= (uint)row.Cells.Count)
            return false;
        try
        {
            objectId = unchecked((uint)row.Cells[objectColumn].AsInt());
            uses = row.Cells[usesColumn].AsInt();
            return true;
        }
        catch (FormatException) { return false; }
        catch (OverflowException) { return false; }
    }
}
