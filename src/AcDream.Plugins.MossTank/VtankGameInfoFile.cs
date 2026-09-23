using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// How a <c>gameinfodb.ugd</c> is read, once, for both the database a
/// session starts with and the one its update check starts from.
/// </summary>
/// <remarks>
/// The reference client reads each value as it meets it. A value that does
/// not read (text where a number belongs) ends that table at the row before
/// it, and the tables after it in the file are lost; a layout that does not
/// read at all, or a database of another version than its own built-in one,
/// is dropped for the built-in database. This reader lands on the same
/// database in each of those cases.
/// </remarks>
internal static class VtankGameInfoFile
{
    /// <summary>The version the built-in database carries, and the only one a download may have.</summary>
    public const int BuiltInVersion = 9;

    /// <summary>
    /// Every table a game database has: its name, its columns and which
    /// column it is looked up by. The built-in database has exactly these,
    /// empty.
    /// </summary>
    internal static readonly (string Name, string[] Columns, int IndexColumn)[] Tables =
    [
        ("AmmunitionOptions",
            ["AmmoName", "LauncherType", "WieldReq", "Element", "Quality", "Special",
                "WieldReq2Skill", "WieldReq2Value"], 0),
        ("CooldownIDs", ["Itemname", "CooldownID"], 0),
        ("CraftInteractions",
            ["UseItem1", "UseItem2", "ResultItem", "ResultCount", "SuccessMsg", "FailMsg",
                "ReqSkill", "ReqDiff", "ID"], 8),
        ("DBLastUpdateTime", ["Zero", "Time"], -1),
        ("DBVersion", ["VersionInt"], -1),
        ("DrainSpellOptions",
            ["SpellID", "CastTimeMilliseconds", "EnemyDrainFactor", "EnemyDrainMaximumPoints",
                "ResultMultiplier"], 0),
        ("GrenadeOptions",
            ["GrenName", "WieldReqType", "WieldReqAttribute", "WieldReqValue", "Spell",
                "Spellcraft"], 0),
        ("HealKits", ["KitName", "RestoreBonus", "SkillBonus", "WhichVital"], 0),
        ("MartyrSpellOptions",
            ["SpellID", "CastTimeMilliseconds", "SelfDrainFactor", "ResultMultiplier"], 0),
        ("MonsterDamageOverrides", ["Monster", "DamageString"], 0),
        ("MonsterImmunities", ["Monster", "ImmunityMask"], 0),
        ("SpeciesDamages", ["Species", "DamageString"], 0),
        ("SpeciesMembers", ["Monster", "Species", "MaximumHealth"], 0),
        ("SpellQualityOverrides", ["SpellID", "Valid", "NewQuality", "NewFamily"], 0),
    ];

    /// <summary>
    /// The database a session has before anything is downloaded: every
    /// table, empty, at the built-in version and with no update time, so the
    /// first check asks for everything. MossTank builds it here rather than
    /// shipping a file.
    /// </summary>
    public static VtankDatabase BuiltIn()
    {
        var database = new VtankDatabase();
        foreach ((string name, string[] columns, int indexColumn) in Tables)
        {
            var table = new VtankTable();
            table.ColumnNames.AddRange(columns);
            for (int column = 0; column < columns.Length; column++)
                table.IndexFlags.Add(column == indexColumn);
            database.Tables.Add((name, table));
        }
        AddRow(database, "DBLastUpdateTime", VtankCell.Int(0), VtankCell.Int(0));
        AddRow(database, "DBVersion", VtankCell.Int(BuiltInVersion));
        return database;
    }

    private static void AddRow(VtankDatabase database, string tableName, params VtankCell[] cells)
    {
        var row = new VtankRow();
        row.Cells.AddRange(cells);
        database.Find(tableName)!.Rows.Add(row);
    }

    /// <summary>
    /// The profile folder's database when it reads and carries the built-in
    /// database's version; the built-in one otherwise.
    /// </summary>
    public static VtankDatabase LoadBase(IPluginStorage profiles)
    {
        ArgumentNullException.ThrowIfNull(profiles);
        VtankDatabase builtIn = BuiltIn();
        string? text = profiles.ReadText(VtankGameInfoDatabase.FileName);
        if (string.IsNullOrWhiteSpace(text))
            return builtIn;
        VtankDatabase? file = Read(text);
        return file is not null && Version(file) == Version(builtIn) ? file : builtIn;
    }

    /// <summary>
    /// The text read the way the reference client reads it, or null when its
    /// layout does not read at all.
    /// </summary>
    public static VtankDatabase? Read(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        VtankDatabase database;
        try
        {
            database = VtankDatabase.Parse(text);
        }
        catch (Exception error) when (
            error is FormatException or OverflowException or InvalidOperationException)
        {
            return null;
        }
        EndAtFirstUnreadableValue(database);
        return database;
    }

    /// <summary>Does every value in every table read as the kind its tag says?</summary>
    public static bool EveryValueReads(VtankDatabase database)
    {
        ArgumentNullException.ThrowIfNull(database);
        foreach ((_, VtankTable table) in database.Tables)
        {
            if (FirstUnreadableRow(table) >= 0)
                return false;
        }
        return true;
    }

    /// <summary>The <c>DBVersion</c> row's number; 1 when there is none to read.</summary>
    public static int Version(VtankDatabase database)
    {
        ArgumentNullException.ThrowIfNull(database);
        VtankTable? table = database.Find("DBVersion");
        if (table is not { Rows.Count: > 0 } || table.Rows[0].Cells.Count < 1)
            return 1;
        return int.TryParse(
            table.Rows[0].Cells[0].ScalarText,
            NumberStyles.Integer,
            CultureInfo.InvariantCulture,
            out int version)
            ? version
            : 1;
    }

    private static void EndAtFirstUnreadableValue(VtankDatabase database)
    {
        for (int index = 0; index < database.Tables.Count; index++)
        {
            VtankTable table = database.Tables[index].Table;
            int row = FirstUnreadableRow(table);
            if (row < 0)
                continue;
            table.Rows.RemoveRange(row, table.Rows.Count - row);
            database.Tables.RemoveRange(index + 1, database.Tables.Count - index - 1);
            return;
        }
    }

    private static int FirstUnreadableRow(VtankTable table)
    {
        for (int row = 0; row < table.Rows.Count; row++)
        {
            foreach (VtankCell cell in table.Rows[row].Cells)
            {
                if (!Reads(cell))
                    return row;
            }
        }
        return -1;
    }

    /// <summary>
    /// A value reads when its text is what its tag promises, by the same
    /// rules the reference client converts it with.
    /// </summary>
    private static bool Reads(VtankCell cell)
    {
        string text = cell.ScalarText ?? string.Empty;
        return cell.Tag switch
        {
            "d" => double.TryParse(
                text, NumberStyles.Float | NumberStyles.AllowThousands,
                CultureInfo.InvariantCulture, out _),
            "f" => float.TryParse(
                text, NumberStyles.Float | NumberStyles.AllowThousands,
                CultureInfo.InvariantCulture, out _),
            "i" => int.TryParse(
                text, NumberStyles.Integer, CultureInfo.InvariantCulture, out _),
            "u" => uint.TryParse(
                text, NumberStyles.Integer, CultureInfo.InvariantCulture, out _),
            "b" => bool.TryParse(text, out _),
            _ => true,
        };
    }
}
