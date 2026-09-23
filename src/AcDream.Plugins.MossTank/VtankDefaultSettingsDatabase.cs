namespace AcDream.Plugins.MossTank;

/// <summary>
/// The settings document a new profile starts from, built from MossTank's
/// own option catalogue (<see cref="VtankOptionCatalog"/>) and option details
/// (<see cref="VtankOptionDetails"/>) in the settings-profile format, so a
/// profile made here is one any reader of that format can open. Nothing of
/// it is shipped as a file.
/// </summary>
internal static class VtankDefaultSettingsDatabase
{
    private const string MonsterTable = "MyMonsters";

    private static readonly IReadOnlyDictionary<string, int> CategoryBitmasksByName =
        VtankOptionDetails.Pages.ToDictionary(
            static pair => pair.Key,
            static pair => (int)pair.Value,
            StringComparer.OrdinalIgnoreCase);

    private static readonly IReadOnlyDictionary<string, IReadOnlyList<VtankEnumValue>> EnumValuesByName =
        VtankOptionDetails.ChoiceLabels
            .GroupBy(static entry => entry.Setting, StringComparer.OrdinalIgnoreCase)
            .ToDictionary(
                static group => group.Key,
                static group => (IReadOnlyList<VtankEnumValue>)group
                    .Select(static entry => new VtankEnumValue(entry.Value, entry.Label))
                    .ToArray(),
                StringComparer.OrdinalIgnoreCase);

    /// <summary>A fresh copy of the default document, for the caller to change.</summary>
    public static VtankDatabase Create()
    {
        var database = new VtankDatabase();
        AddTable(database, "AntiExtraBuffSpells", ["ExemplarId"], indexColumn: -1);
        AddTable(database, "AssistItems", ["Object", "Type"], indexColumn: -1);
        AddTable(database, "BuffedItems", ["Object", "Spell"], indexColumn: -1);
        AddTable(database, "ExtraBuffSpells", ["ExemplarId"], indexColumn: -1);
        VtankTable gems = AddTable(database, "GemFoodItems", ["Name", "Spell"], indexColumn: -1);
        foreach (GemFoodItem gem in VtankOptionDetails.DefaultGemFoodItems)
            AddRow(gems, VtankCell.String(gem.Name), VtankCell.Int(unchecked((int)gem.SpellId)));
        AddTable(database, "ItemUseSpecifiers", ["Object", "Uses"], indexColumn: 0);
        AddTable(database, MonsterTable, VtankMonsterRuleTable.Columns, indexColumn: 0);
        VtankMonsterRuleTable.Write(database, [MonsterRule.AuthenticDefault()]);

        VtankTable settings = AddTable(
            database, "Settings", ["Setting", "Value", "Description", "SettingType"], indexColumn: 0);
        VtankTable categories = AddTable(
            database, "SettingsCategories", ["Setting", "Categories"], indexColumn: 0);
        foreach (string name in VtankOptionCatalog.Names)
        {
            VtankSettingValueType type = VtankOptionCatalog.DeclaredType(name);
            AddRow(
                settings,
                VtankCell.String(name),
                DefaultValue(name, type),
                VtankCell.String(VtankOptionDetails.Descriptions.GetValueOrDefault(name, string.Empty)),
                VtankCell.Int((int)type));
            if (VtankOptionDetails.Pages.TryGetValue(name, out VtankOptionPage pages))
                AddRow(categories, VtankCell.String(name), VtankCell.Int((int)pages));
        }

        VtankTable choices = AddTable(
            database, "SettingsEnumInfo", ["Setting", "Value", "EnumValue"], indexColumn: -1);
        foreach ((string setting, int value, string label) in VtankOptionDetails.ChoiceLabels)
            AddRow(choices, VtankCell.String(setting), VtankCell.Int(value), VtankCell.String(label));
        return database;
    }

    /// <summary>The recharge plan a new profile starts with.</summary>
    public static IReadOnlyList<RechargeHandlerRow> DefaultRechargeHandlerRows =>
        VtankOptionDetails.DefaultRechargeHandlers;

    public static IReadOnlyDictionary<string, int> SettingCategoryBitmasks => CategoryBitmasksByName;

    public static IReadOnlyDictionary<string, string> SettingDescriptions =>
        VtankOptionDetails.Descriptions;

    public static IReadOnlyDictionary<string, IReadOnlyList<VtankEnumValue>> SettingEnumValues =>
        EnumValuesByName;

    private static VtankCell DefaultValue(string name, VtankSettingValueType type)
    {
        MonsterValue value = VtankOptionCatalog.Default(name);
        return type switch
        {
            VtankSettingValueType.Bool => VtankCell.Bool(value.Boolean),
            VtankSettingValueType.Int or VtankSettingValueType.Enum =>
                VtankCell.Int((int)Math.Round(value.Number, MidpointRounding.AwayFromZero)),
            VtankSettingValueType.Single => VtankCell.Float((float)value.Number),
            VtankSettingValueType.String => VtankCell.String(value.Text),
            VtankSettingValueType.Custom => VtankCell.NestedTable(RechargeHandlerTable()),
            _ => VtankCell.Double(value.Number),
        };
    }

    /// <summary>The one table-valued setting: the recharge plan, one row per step.</summary>
    private static VtankTable RechargeHandlerTable()
    {
        var table = new VtankTable();
        table.ColumnNames.AddRange(["Vital", "HandlerString", "MinPercent", "MaxPercent", "Stance"]);
        table.IndexFlags.AddRange([false, false, false, false, false]);
        foreach (RechargeHandlerRow row in VtankOptionDetails.DefaultRechargeHandlers)
        {
            AddRow(
                table,
                VtankCell.Int(row.Vital),
                VtankCell.String(row.HandlerString),
                VtankCell.Int(row.MinPercent),
                VtankCell.Int(row.MaxPercent),
                VtankCell.Int(row.Stance));
        }
        return table;
    }

    private static VtankTable AddTable(
        VtankDatabase database,
        string name,
        IReadOnlyList<string> columns,
        int indexColumn)
    {
        var table = new VtankTable();
        table.ColumnNames.AddRange(columns);
        for (int column = 0; column < columns.Count; column++)
            table.IndexFlags.Add(column == indexColumn);
        database.Tables.Add((name, table));
        return table;
    }

    private static void AddRow(VtankTable table, params VtankCell[] cells)
    {
        var row = new VtankRow();
        row.Cells.AddRange(cells);
        table.Rows.Add(row);
    }
}

internal readonly record struct VtankEnumValue(int Value, string Label);
