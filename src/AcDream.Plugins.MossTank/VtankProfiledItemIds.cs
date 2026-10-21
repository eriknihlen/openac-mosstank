namespace AcDream.Plugins.MossTank;

internal static class VtankProfiledItemIds
{
    private const string TableName = "BuffedItems";

    public static void Read(VtankDatabase database, CombatSettings settings,
        BuffSettings buffs, uint playerObjectId)
    {
        settings.CombatItemOrderIds.Clear();
        settings.CombatItemObjectIds.Clear();
        settings.RemovedCombatItemObjectIds.Clear();
        for (int index = buffs.ItemEnchantRows.Count - 1; index >= 0; index--)
        {
            if (buffs.ItemEnchantRows[index].IsProfiledItemRow)
                buffs.ItemEnchantRows.RemoveAt(index);
        }

        VtankTable? table = database.Find(TableName);
        int objectColumn = table?.ColumnIndex("Object") ?? -1;
        int spellColumn = table?.ColumnIndex("Spell") ?? -1;
        if (table is null || objectColumn < 0 || spellColumn < 0)
            return;

        foreach (VtankRow row in table.Rows)
        {
            if (!TryReadRow(row, objectColumn, spellColumn, out int rawId, out int rawSpell))
                continue;
            if (rawId == -1)
            {
                if (rawSpell > 0 || rawSpell == -1)
                    buffs.ItemEnchantRows.Add(new BuffItemEnchantRow(
                        string.Empty, string.Empty, uint.MaxValue, unchecked((uint)rawSpell)));
                continue;
            }
            uint id = unchecked((uint)rawId);
            if (id != playerObjectId && settings.CombatItemObjectIds.Add(id))
                settings.CombatItemOrderIds.Add(id);
            if (rawSpell > 0 || rawSpell == -1)
                buffs.ItemEnchantRows.Add(new BuffItemEnchantRow(
                    string.Empty, string.Empty, id, unchecked((uint)rawSpell)));
        }
    }

    private static bool TryReadRow(
        VtankRow row, int objectColumn, int spellColumn, out int objectId, out int spellId)
    {
        objectId = 0;
        spellId = 0;
        if ((uint)objectColumn >= (uint)row.Cells.Count
            || (uint)spellColumn >= (uint)row.Cells.Count)
            return false;
        try
        {
            objectId = row.Cells[objectColumn].AsInt();
            spellId = row.Cells[spellColumn].AsInt();
            return true;
        }
        catch (FormatException) { return false; }
        catch (OverflowException) { return false; }
    }

    private static bool TryReadObjectId(VtankRow row, int objectColumn, out uint id)
    {
        id = 0u;
        if ((uint)objectColumn >= (uint)row.Cells.Count)
            return false;
        try
        {
            id = unchecked((uint)row.Cells[objectColumn].AsInt());
            return true;
        }
        catch (FormatException) { return false; }
        catch (OverflowException) { return false; }
    }

    /// <summary>
    /// Save the listed items. An item added since the profile was read is
    /// written with the spells it is buffed with, one row each, or as the
    /// item with no spell when it carries none, as VTank writes an item.
    /// </summary>
    public static void Write(VtankDatabase database, CombatSettings settings, BuffSettings buffs)
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
            table.Rows.RemoveAll(row => TryReadObjectId(row, objectColumn, out uint id)
                && settings.RemovedCombatItemObjectIds.Contains(id));
            settings.RemovedCombatItemObjectIds.Clear();
        }

        var present = new HashSet<uint>();
        foreach (VtankRow row in table.Rows)
        {
            if (TryReadObjectId(row, objectColumn, out uint id))
                present.Add(id);
        }
        foreach (uint id in settings.CombatItemOrderIds)
        {
            if (id is 0u or uint.MaxValue || !present.Add(id))
                continue;
            bool wroteSpell = false;
            foreach (BuffItemEnchantRow spellRow in buffs.ItemEnchantRows)
            {
                if (!spellRow.IsProfiledItemRow || spellRow.ObjectId != id)
                    continue;
                AddRow(table, objectColumn, spellColumn, id, unchecked((int)spellRow.SpellId!.Value));
                wroteSpell = true;
            }
            if (!wroteSpell)
                AddRow(table, objectColumn, spellColumn, id, -1);
        }
    }

    private static void AddRow(
        VtankTable table, int objectColumn, int spellColumn, uint objectId, int spellId)
    {
        var row = new VtankRow();
        for (int column = 0; column < table.ColumnNames.Count; column++)
            row.Cells.Add(VtankCell.Int(-1));
        row.Cells[objectColumn] = VtankCell.Int(unchecked((int)objectId));
        row.Cells[spellColumn] = VtankCell.Int(spellId);
        table.Rows.Add(row);
    }
}
