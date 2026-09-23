using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// How a <c>gameinfodb.ugd</c> in the profile folder is read at start and by
/// the update check: one reader, the reference client's rules.
/// </summary>
public sealed class VtankGameInfoFileTests
{
    /// <summary>
    /// A file of another version is dropped for the built-in database when
    /// the session starts, as it is by the update check. Mutation: take the
    /// file whatever its version, and its ammunition is loaded.
    /// </summary>
    [Fact]
    public void AFileOfAnotherVersionLoadsTheBuiltInDatabase()
    {
        VtankDatabase file = VtankDatabase.Parse(VtankGameInfoUpdaterTests.ExcerptText);
        file.Find("DBVersion")!.Rows[0].Cells[0] = VtankCell.Int(4);

        VtankGameInfoDatabase database = VtankGameInfoDatabase.Load(new Storage(file.Render()));

        Assert.True(database.IsLoaded);
        Assert.Equal(9, database.Version);
        Assert.Empty(database.AmmunitionOptions);
    }

    /// <summary>
    /// A value that does not read as its kind ends its table at the row
    /// before it, and the tables after it in the file are lost, as they are
    /// to the reference client's reader; everything before it stays.
    /// Mutation: skip only the bad row, and the heal kit after it and the
    /// later tables survive.
    /// </summary>
    [Fact]
    public void AnUnreadableValueEndsItsTableAndTheTablesAfterIt()
    {
        VtankDatabase file = VtankDatabase.Parse(VtankGameInfoUpdaterTests.ExcerptText);
        VtankTable kits = file.Find("HealKits")!;
        kits.Rows[1].Cells[2] = new VtankCell { Tag = "i", ScalarText = "lots" };

        VtankGameInfoDatabase database = VtankGameInfoDatabase.Load(new Storage(file.Render()));

        Assert.Equal("Fixture Healing Kit", Assert.Single(database.HealKits).Name);
        Assert.Equal(7, database.AmmunitionOptions.Count);
        Assert.Equal(3, database.GrenadeOptions.Count);
        Assert.Empty(database.MartyrSpellOptions);
        Assert.Empty(database.SpeciesMembers);
    }

    /// <summary>
    /// A row that reads but not as the kind this client needs (text in a
    /// number column) is passed over on its own; the rest of the table and
    /// the file are kept. Mutation: let the row's failure escape, and reading
    /// the file fails outright.
    /// </summary>
    [Fact]
    public void ARowOfTheWrongKindIsPassedOverAlone()
    {
        VtankDatabase file = VtankDatabase.Parse(VtankGameInfoUpdaterTests.ExcerptText);
        file.Find("GrenadeOptions")!.Rows[1].Cells[4] = VtankCell.String("a spell");

        VtankGameInfoDatabase database = VtankGameInfoDatabase.Load(new Storage(file.Render()));

        Assert.Equal(
            ["Iron Phial of Imperil", "Fixture Phial of Nothing"],
            database.GrenadeOptions.Select(static grenade => grenade.Name));
        Assert.Equal(3, database.MartyrSpellOptions.Count);
    }

    /// <summary>
    /// The built-in database is built in code, not shipped, and it has to
    /// take every table the service sends, in the service's shape, or the
    /// merge drops that table. One row per table, merged into it, lands in
    /// every table; the craft table is looked up by its ID column, and the
    /// database asks at version 9 from time zero. Mutations: leave a table
    /// out of the built-in list, or give one a different column count (its
    /// row is dropped); key the craft table on column 0 (the second row with
    /// the same ID is added instead of replacing the first).
    /// </summary>
    [Fact]
    public void TheBuiltInDatabaseTakesEveryTableTheServiceSends()
    {
        (string Name, int Columns)[] sent =
        [
            ("DBLastUpdateTime", 2), ("MonsterDamageOverrides", 2), ("SpeciesDamages", 2),
            ("SpeciesMembers", 3), ("CraftInteractions", 9), ("CooldownIDs", 2),
            ("AmmunitionOptions", 8), ("GrenadeOptions", 6), ("SpellQualityOverrides", 4),
            ("HealKits", 4), ("DrainSpellOptions", 5), ("MartyrSpellOptions", 4),
            ("MonsterImmunities", 2),
        ];
        var answer = new VtankDatabase();
        foreach ((string name, int columns) in sent)
        {
            var table = new VtankTable();
            for (int column = 0; column < columns; column++)
            {
                table.ColumnNames.Add("C" + column);
                table.IndexFlags.Add(false);
            }
            // The service's time row is keyed on its zero column, as the built-in one is.
            table.Rows.Add(Row(columns, first: name == "DBLastUpdateTime" ? 0 : 1, last: 7));
            if (name == "CraftInteractions")
                table.Rows.Add(Row(columns, first: 2, last: 7));
            answer.Tables.Add((name, table));
        }
        VtankDatabase database = VtankGameInfoFile.BuiltIn();

        VtankGameInfoUpdater.Merge(database, answer);

        Assert.Equal(9, VtankGameInfoFile.Version(database));
        foreach ((string name, _) in sent)
            Assert.Single(database.Find(name)!.Rows);
        Assert.Equal(2, database.Find("CraftInteractions")!.Rows[0].Cells[0].AsInt());
        Assert.Equal(7, VtankGameInfoUpdater.LastUpdateTime(database));

        static VtankRow Row(int columns, int first, int last)
        {
            var row = new VtankRow();
            row.Cells.Add(VtankCell.Int(first));
            for (int column = 1; column < columns - 1; column++)
                row.Cells.Add(VtankCell.Int(column));
            if (columns > 1)
                row.Cells.Add(VtankCell.Int(last));
            return row;
        }
    }

    [Fact]
    public void TheBuiltInDatabaseStartsAtVersion9WithNoUpdateTime()
    {
        VtankGameInfoDatabase database = VtankGameInfoDatabase.LoadDefault();

        Assert.Equal(9, database.Version);
        Assert.Equal(0, database.LastUpdateTime);
        Assert.Empty(database.AmmunitionOptions);
    }

    private sealed class Storage(string text) : IPluginStorage
    {
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            key == VtankGameInfoDatabase.FileName ? text : null;
    }
}
