namespace AcDream.Plugins.MossTank;

internal static class VtankBuffExemplars
{
    public static void Read(VtankDatabase database, BuffSettings settings)
    {
        ReadTable(database, "ExtraBuffSpells", settings.ExtraBuffSpellIds);
        ReadTable(database, "AntiExtraBuffSpells", settings.AntiExtraBuffSpellIds);
    }

    public static void Write(VtankDatabase database, BuffSettings settings)
    {
        WriteTable(database, "ExtraBuffSpells", settings.ExtraBuffSpellIds);
        WriteTable(database, "AntiExtraBuffSpells", settings.AntiExtraBuffSpellIds);
    }

    private static void ReadTable(VtankDatabase database, string tableName, IList<uint> ids)
    {
        ids.Clear();
        VtankTable? table = database.Find(tableName);
        int exemplar = table?.ColumnIndex("ExemplarId") ?? -1;
        if (table is null || exemplar < 0)
            return;
        foreach (VtankRow row in table.Rows)
        {
            if (!TryGetId(row, exemplar, out uint id) || ids.Contains(id))
                continue;
            ids.Add(id);
        }
    }

    private static void WriteTable(VtankDatabase database, string tableName, IList<uint> ids)
    {
        VtankTable? table = database.Find(tableName);
        if (table is null)
        {
            if (ids.Count == 0)
                return;
            table = new VtankTable();
            table.ColumnNames.Add("ExemplarId");
            table.IndexFlags.Add(false);
            database.Tables.Add((tableName, table));
        }
        int exemplar = table.ColumnIndex("ExemplarId");
        if (exemplar < 0)
            throw new FormatException($"'{tableName}' table is missing ExemplarId column.");

        var selected = new HashSet<uint>(ids);
        var present = new HashSet<uint>();
        for (int rowIndex = table.Rows.Count - 1; rowIndex >= 0; rowIndex--)
        {
            VtankRow row = table.Rows[rowIndex];
            if (!TryGetId(row, exemplar, out uint id))
                continue;
            if (!selected.Contains(id))
            {
                table.Rows.RemoveAt(rowIndex);
                continue;
            }
            present.Add(id);
        }
        foreach (uint id in ids)
        {
            if (!present.Add(id))
                continue;
            var row = new VtankRow();
            for (int column = 0; column < table.ColumnNames.Count; column++)
                row.Cells.Add(VtankCell.Int(0));
            row.Cells[exemplar] = VtankCell.Int(unchecked((int)id));
            table.Rows.Add(row);
        }
    }

    private static bool TryGetId(VtankRow row, int column, out uint id)
    {
        id = 0u;
        if ((uint)column >= (uint)row.Cells.Count)
            return false;
        try
        {
            if (row.Cells[column].Tag == "u")
            {
                id = row.Cells[column].AsUInt();
                return id != 0u;
            }
            int raw = row.Cells[column].AsInt();
            if (raw == 0)
                return false;
            id = unchecked((uint)raw);
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
