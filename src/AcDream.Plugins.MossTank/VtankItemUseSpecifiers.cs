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
