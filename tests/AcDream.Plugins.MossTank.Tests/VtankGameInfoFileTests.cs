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
