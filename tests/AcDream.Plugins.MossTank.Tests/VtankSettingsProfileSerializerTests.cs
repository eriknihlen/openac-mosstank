using System.Globalization;
using System.Linq;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankSettingsProfileSerializerTests
{
    private static readonly string FixturesRoot = Path.Combine(
        AppContext.BaseDirectory, "Fixtures", "vtank");

    public static TheoryData<string> UsdFixtureData()
    {
        var data = new TheoryData<string>();
        foreach (string name in new[] { "defaultsettings", "owner-a", "owner-b", "owner-c" })
            data.Add(Path.Combine(FixturesRoot, name + ".usd"));
        return data;
    }

    private static VtankSettingsProfileSerializer.AllSettings NewSettings() => new()
    {
        Combat = new CombatSettings(),
        Buffs = new BuffSettings(),
        Vitals = new VitalSettings(),
        Inventory = new InventorySettings(),
        Navigation = new NavigationSettings(),
    };

    /// <summary>
    /// Mutation pin: skip reading BuffedItems identities; the ordered-ID
    /// assertion fails even though the raw spell rows remain in the document.
    /// </summary>
    [Fact]
    public void BuffedItemsSupplyDistinctOrderedIdsAndKeepSpellRows()
    {
        VtankDatabase document = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = document.Find("BuffedItems")!;
        table.Rows.Add(BuffedItem(802, 17));
        table.Rows.Add(BuffedItem(801, 18));
        table.Rows.Add(BuffedItem(802, 19));
        table.Rows.Add(BuffedItem(802, -1));
        table.Rows.Add(BuffedItem(900, 20));
        table.Rows.Add(BuffedItem(0, 22));
        table.Rows.Add(BuffedItem(-1, 21));
        string original = document.Render();
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new(), Buffs = new(), Vitals = new(),
            Inventory = new(), Navigation = new(),
            PlayerObjectId = static () => 900u,
        };

        VtankDatabase loaded = VtankSettingsProfileSerializer.Load(original, settings);

        Assert.Equal([802u, 801u, 0u], settings.Combat.CombatItemOrderIds);
        Assert.Equal([0u, 801u, 802u], settings.Combat.CombatItemObjectIds.Order());
        Assert.Contains(settings.Buffs.ItemEnchantRows, row =>
            row.ObjectId == 802u && row.SpellId == 17u);
        Assert.Contains(settings.Buffs.ItemEnchantRows, row =>
            row.ObjectId == uint.MaxValue && row.SpellId == 21u);
        Assert.Contains(settings.Buffs.ItemEnchantRows, row =>
            row.ObjectId == 802u && row.CastsNothing);
        Assert.Equal(original, VtankSettingsProfileSerializer.Save(loaded, settings));
    }

    /// <summary>Mutation <c>UnsafeBuffedItemsWrite</c>: throw while inspecting a malformed raw object cell.</summary>
    [Fact]
    public void BuffedItemsKeepMalformedDuplicateAndCustomRowsOnRoundTrip()
    {
        VtankDatabase document = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = document.Find("BuffedItems")!;
        table.ColumnNames.Add("Custom");
        table.IndexFlags.Add(false);
        table.Rows.Add(BuffedItem(801, 17, "first"));
        table.Rows.Add(BuffedItem(801, 18, "second"));
        table.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.String("not-an-id"), VtankCell.Int(19), VtankCell.String("raw") },
        });
        table.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.UInt(uint.MaxValue), VtankCell.Int(20), VtankCell.String("overflow") },
        });
        string original = document.Render();
        var settings = NewSettings();

        VtankDatabase loaded = VtankSettingsProfileSerializer.Load(original, settings);

        Assert.Equal([801u], settings.Combat.CombatItemOrderIds);
        Assert.Equal(original, VtankSettingsProfileSerializer.Save(loaded, settings));
    }

    [Fact]
    public void LoadingAProfileWithoutBuffedItemsClearsEarlierProfiledRows()
    {
        VtankDatabase first = VtankDefaultSettingsDatabase.Parse();
        first.Find("BuffedItems")!.Rows.Add(BuffedItem(801, 17));
        VtankDatabase second = VtankDefaultSettingsDatabase.Parse();
        second.Tables.RemoveAll(static entry => entry.Name == "BuffedItems");
        var settings = NewSettings();

        VtankSettingsProfileSerializer.Load(first.Render(), settings);
        VtankSettingsProfileSerializer.Load(second.Render(), settings);

        Assert.Empty(settings.Combat.CombatItemOrderIds);
        Assert.DoesNotContain(settings.Buffs.ItemEnchantRows,
            static row => row.IsProfiledItemRow);
    }

    private static VtankRow BuffedItem(int objectId, int spellId, string? custom = null)
    {
        var row = new VtankRow();
        row.Cells.Add(VtankCell.Int(objectId));
        row.Cells.Add(VtankCell.Int(spellId));
        if (custom is not null)
            row.Cells.Add(VtankCell.String(custom));
        return row;
    }

    [Theory]
    [MemberData(nameof(UsdFixtureData))]
    public void EveryUsdFixtureLoads(string path)
    {
        string text = File.ReadAllText(path);
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(text, settings);
        Assert.NotNull(document.Find("Settings"));
    }

    [Theory]
    [MemberData(nameof(UsdFixtureData))]
    public void UntouchedRoundTripIsByteIdentical(string path)
    {
        string original = File.ReadAllText(path);
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(original, settings);
        string rewritten = VtankSettingsProfileSerializer.Save(document, settings);
        Assert.Equal(original, rewritten);
    }

    [Fact]
    public void DefaultSettingsUsdMatchesEveryCatalogDefault()
    {
        string text = File.ReadAllText(Path.Combine(FixturesRoot, "defaultsettings.usd"));
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankSettingsProfileSerializer.Load(text, settings);

        Assert.Equal(25, settings.Combat.HuntSkillExcessOverDifficulty);
        Assert.True(settings.Combat.Enabled);
        Assert.True(settings.Buffs.Enabled);
        Assert.False(settings.Inventory.Loot.Enabled);
        Assert.False(settings.Navigation.Enabled);
        Assert.Equal(0.75, settings.Vitals.NormalHealth, 6);
        Assert.Equal(0.50, settings.Vitals.NormalStamina, 6);
        Assert.Equal(0.50, settings.Vitals.NormalMana, 6);
        Assert.Equal(5f, settings.Combat.MaximumRange, 3);
        Assert.Equal(2, (int)settings.Combat.AttackHeight);
        Assert.Equal(TargetSelectionMethod.Both, settings.Combat.SelectionMethod);
        Assert.Equal(1.9, settings.Vitals.StaminaToHealthMultiplier, 6);
        Assert.Equal(2.8, settings.Vitals.ManaToHealthMultiplier, 6);
        Assert.Equal("ALFCBPS", settings.Buffs.ProtectionElements);
        Assert.Equal(2, settings.Buffs.ProtectionProfileMode);
        Assert.Equal(30d, settings.Buffs.BuffCastRecastSeconds, 6);
        Assert.Equal(80, settings.Buffs.BuffWithUntrainedItemSkill);
        Assert.Equal(-50, settings.Navigation.DoorLockpickExcessThreshold);
        Assert.Equal(34, settings.Vitals.DropToPeaceModeRetryCount);

        Assert.Equal(26, settings.Vitals.RechargeHandlerRows.Count);
        RechargeHandlerRow first = settings.Vitals.RechargeHandlerRows[0];
        Assert.Equal(1, first.Vital); // Health
        Assert.Equal("Stamina to Health", first.HandlerString);
        Assert.Equal(0, first.MinPercent);
        Assert.Equal(15, first.MaxPercent);
        Assert.Equal(1, first.Stance); // magic
    }

    [Fact]
    public void Every137CatalogNameMapsToExactlyOneField()
    {
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        var unmapped = new List<string>();
        foreach (string name in VtankOptionCatalog.Names)
        {
            if (VtankSettingsProfileSerializer.Capture(name, settings) is null)
                unmapped.Add(name);
        }
        // RechargeHandlerSet is the only one left with no write path — the
        // reference client has none for it either.
        Assert.Equal(["RechargeHandlerSet"], unmapped);
    }

    public static TheoryData<string> CatalogNamesWithLiveWritePath()
    {
        var data = new TheoryData<string>();
        foreach (string name in VtankOptionCatalog.Names)
        {
            if (name is "RechargeHandlerSet")
                continue; // no live write path (Every137CatalogNameMapsToExactlyOneField).
            data.Add(name);
        }
        return data;
    }

    [Theory]
    [MemberData(nameof(CatalogNamesWithLiveWritePath))]
    public void CaptureMatchesDeclaredSettingTypeAndValue(string name)
    {
        string text = File.ReadAllText(Path.Combine(FixturesRoot, "defaultsettings.usd"));
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(text, settings);
        VtankTable table = document.Find("Settings")!;
        int nameColumn = table.ColumnIndex("Setting");
        int valueColumn = table.ColumnIndex("Value");
        VtankRow row = table.Rows.Single(
            r => r.Cells[nameColumn].AsString().Equals(name, StringComparison.Ordinal));
        VtankCell fixtureCell = row.Cells[valueColumn];

        VtankCell? captured = VtankSettingsProfileSerializer.Capture(name, settings);

        Assert.NotNull(captured);
        Assert.Equal(fixtureCell.Tag, captured!.Tag);
        switch (fixtureCell.Tag)
        {
            case "b":
                Assert.Equal(fixtureCell.AsBool(), captured.AsBool());
                break;
            case "s":
                Assert.Equal(fixtureCell.AsString(), captured.AsString());
                break;
            case "i":
                Assert.Equal(fixtureCell.AsInt(), captured.AsInt());
                break;
            case "u":
                Assert.Equal(fixtureCell.AsUInt(), captured.AsUInt());
                break;
            case "d":
            case "f":
                Assert.Equal(
                    fixtureCell.AsDouble(),
                    captured.AsDouble());
                break;
            default:
                Assert.Fail($"{name}: unexpected tag '{fixtureCell.Tag}'.");
                break;
        }
    }

    [Fact]
    public void RechargeHandlerSetRoundTripsFromOwnerFixture()
    {
        string text = File.ReadAllText(Path.Combine(FixturesRoot, "owner-a.usd"));
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankSettingsProfileSerializer.Load(text, settings);
        Assert.NotEmpty(settings.Vitals.RechargeHandlerRows);
        Assert.Contains(
            settings.Vitals.RechargeHandlerRows,
            static row => row.HandlerString == "Kit Recharge");
    }

    [Fact]
    public void CreateNewHasTheSameTableSetAsTheDefaultFixture()
    {
        string fixtureText = File.ReadAllText(
            Path.Combine(FixturesRoot, "defaultsettings.usd"));
        VtankDatabase fixture = VtankDatabase.Parse(fixtureText);

        VtankDatabase created = VtankSettingsProfileSerializer.CreateNew(NewSettings());

        string[] expectedTables = [.. fixture.Tables
            .Select(static entry => entry.Name)
            .OrderBy(static name => name, StringComparer.Ordinal)];
        string[] actualTables = [.. created.Tables
            .Select(static entry => entry.Name)
            .OrderBy(static name => name, StringComparer.Ordinal)];
        Assert.Equal(expectedTables, actualTables);
        Assert.True(expectedTables.Length > 1, "the fixture itself must carry more than one table.");

        // Every row's Description/SettingType survive from the embedded
        // document (not the prior blanket Description="" / SettingType=1).
        VtankTable? settings = created.Find("Settings");
        Assert.NotNull(settings);
        int descriptionColumn = settings!.ColumnIndex("Description");
        int typeColumn = settings.ColumnIndex("SettingType");
        Assert.True(descriptionColumn >= 0);
        Assert.True(typeColumn >= 0);
        Assert.Contains(
            settings.Rows,
            row => row.Cells[descriptionColumn].AsString().Length > 0);
        Assert.Contains(
            settings.Rows,
            row => row.Cells[typeColumn].AsInt() != (int)VtankSettingValueType.Bool);
    }

    [Fact]
    public void UnknownSettingsRowIsPreservedVerbatim()
    {
        string usd = SingleSettingUsd("SomeFutureSetting", "s", "future-value");
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(usd, settings);
        string rewritten = VtankSettingsProfileSerializer.Save(document, settings);
        Assert.Equal(usd, rewritten);
    }

    [Fact]
    public void GemFoodRowsLoadSaveAndReplaceThePriorProfileRows()
    {
        string first = File.ReadAllText(Path.Combine(FixturesRoot, "owner-c.usd"));
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(first, settings);

        Assert.Contains(settings.Buffs.GemFoodItems,
            entry => entry.Name == "Asheron's Benediction" && entry.SpellId == 3810u);
        settings.Buffs.GemFoodItems.Clear();
        settings.Buffs.GemFoodItems.Add(new GemFoodItem("Unknown Gem", 999999u));
        string saved = VtankSettingsProfileSerializer.Save(document, settings);

        VtankSettingsProfileSerializer.AllSettings reloaded = NewSettings();
        VtankSettingsProfileSerializer.Load(saved, reloaded);
        GemFoodItem item = Assert.Single(reloaded.Buffs.GemFoodItems);
        Assert.Equal("Unknown Gem", item.Name);
        Assert.Equal(999999u, item.SpellId);
    }

    /// <summary>
    /// Mutation pin: rebuilding the GemFood table with <c>Rows.Clear()</c>
    /// loses the extension cell and invalid source row before any user edit.
    /// </summary>
    [Fact]
    public void GemFoodWritesKeepCustomCellsInvalidRowsAndUnchangedRowsVerbatim()
    {
        VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = seed.Find("GemFoodItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Clear();
        table.Rows.Add(GemFoodRow(table, "Blackmoor's Favor", 3811, "keep-this"));
        table.Rows.Add(GemFoodRow(table, "Broken entry", 0, "invalid-row"));
        string original = seed.Render();

        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(original, settings);
        Assert.Equal(original, VtankSettingsProfileSerializer.Save(document, settings));

        settings.Buffs.GemFoodItems.Clear();
        settings.Buffs.GemFoodItems.Add(new GemFoodItem("Blackmoor's Favor", 3812u));
        string edited = VtankSettingsProfileSerializer.Save(document, settings);
        VtankTable rewritten = VtankDatabase.Parse(edited).Find("GemFoodItems")!;
        int extension = rewritten.ColumnIndex("Extension");
        Assert.Contains(rewritten.Rows, row =>
            row.Cells[extension].AsString() == "keep-this");
        Assert.Contains(rewritten.Rows, row =>
            row.Cells[extension].AsString() == "invalid-row");
        Assert.Contains(rewritten.Rows, row => row.Cells[rewritten.ColumnIndex("Spell")].AsInt() == 3812);

        settings.Buffs.GemFoodItems.Clear();
        VtankSettingsProfileSerializer.Save(document, settings);
        VtankTable afterRemoval = document.Find("GemFoodItems")!;
        Assert.Single(afterRemoval.Rows);
        Assert.Equal("Broken entry", afterRemoval.Rows[0].Cells[afterRemoval.ColumnIndex("Name")].AsString());
    }

    [Fact]
    public void GemFoodInvalidNumericCellsStayInertAndRoundTripVerbatim()
    {
        VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = seed.Find("GemFoodItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Clear();
        VtankRow nonNumeric = GemFoodRow(table, "Broken text", 1, string.Empty);
        nonNumeric.Cells[table.ColumnIndex("Spell")] = new VtankCell
        {
            Tag = "i", ScalarText = "not-an-id",
        };
        VtankRow overflow = GemFoodRow(table, "Broken overflow", 1, string.Empty);
        overflow.Cells[table.ColumnIndex("Spell")] = new VtankCell
        {
            Tag = "i", ScalarText = "999999999999999999999",
        };
        table.Rows.Add(nonNumeric);
        table.Rows.Add(overflow);
        string original = seed.Render();

        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(original, settings);

        Assert.Empty(settings.Buffs.GemFoodItems);
        Assert.Equal(original, VtankSettingsProfileSerializer.Save(document, settings));
    }

    /// <summary>
    /// The exact source row reserves its custom cells before a same-name
    /// fallback is considered for the changed spell row.
    /// </summary>
    [Fact]
    public void GemFoodRemovalKeepsTheExactSameNameSourceRow()
    {
        VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = seed.Find("GemFoodItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Clear();
        table.Rows.Add(GemFoodRow(table, "Gem", 1, "custom-a"));
        table.Rows.Add(GemFoodRow(table, "Gem", 2, "custom-b"));
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(seed.Render(), settings);
        settings.Buffs.GemFoodItems.Clear();
        settings.Buffs.GemFoodItems.Add(new GemFoodItem("Gem", 1u));

        VtankTable saved = VtankDatabase.Parse(
            VtankSettingsProfileSerializer.Save(document, settings)).Find("GemFoodItems")!;

        VtankRow row = Assert.Single(saved.Rows);
        Assert.Equal(1, row.Cells[saved.ColumnIndex("Spell")].AsInt());
        Assert.Equal("custom-a", row.Cells[saved.ColumnIndex("Extension")].AsString());
    }

    [Fact]
    public void GemFoodRowsClearWhenTheNextProfileHasNoTable()
    {
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankSettingsProfileSerializer.Load(
            File.ReadAllText(Path.Combine(FixturesRoot, "owner-c.usd")), settings);
        Assert.NotEmpty(settings.Buffs.GemFoodItems);

        VtankSettingsProfileSerializer.Load(
            SingleSettingUsd("EnableBuffing", "b", "True"), settings);

        Assert.Empty(settings.Buffs.GemFoodItems);
    }

    [Fact]
    public void ExtraBuffExemplarsLoadRoundTripAndPreserveUnknownCells()
    {
        VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
        VtankTable extra = seed.Find("ExtraBuffSpells")!;
        extra.ColumnNames.Add("Extension");
        extra.IndexFlags.Add(false);
        extra.Rows.Add(ExemplarRow(extra, 999999, "keep-extra"));
        extra.Rows.Add(ExemplarRow(extra, 999999, "duplicate-extra"));
        extra.Rows.Add(ExemplarRow(extra, 0, "invalid-extra"));
        VtankRow overflow = ExemplarRow(extra, 1, "overflow-extra");
        overflow.Cells[extra.ColumnIndex("ExemplarId")] = new VtankCell
        {
            Tag = "i", ScalarText = "999999999999999999999",
        };
        extra.Rows.Add(overflow);
        VtankTable anti = seed.Find("AntiExtraBuffSpells")!;
        anti.Rows.Add(ExemplarRow(anti, 999998, null));
        string original = seed.Render();

        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankDatabase document = VtankSettingsProfileSerializer.Load(original, settings);
        Assert.Equal([999999u], settings.Buffs.ExtraBuffSpellIds);
        Assert.Equal([999998u], settings.Buffs.AntiExtraBuffSpellIds);
        Assert.Equal(original, VtankSettingsProfileSerializer.Save(document, settings));

        settings.Buffs.ExtraBuffSpellIds.Clear();
        settings.Buffs.ExtraBuffSpellIds.Add(999997u);
        string saved = VtankSettingsProfileSerializer.Save(document, settings);
        VtankTable rewritten = VtankDatabase.Parse(saved).Find("ExtraBuffSpells")!;
        int extension = rewritten.ColumnIndex("Extension");
        Assert.Contains(rewritten.Rows, row => row.Cells[extension].AsString() == "invalid-extra");
        Assert.Contains(rewritten.Rows, row => row.Cells[extension].AsString() == "overflow-extra");
    }

    [Fact]
    public void ExtraBuffExemplarsClearWhenTheNextProfileOmitsTheirTables()
    {
        VtankSettingsProfileSerializer.AllSettings settings = NewSettings();
        VtankSettingsProfileSerializer.Load(
            File.ReadAllText(Path.Combine(FixturesRoot, "owner-c.usd")), settings);
        settings.Buffs.ExtraBuffSpellIds.Add(100u);
        settings.Buffs.AntiExtraBuffSpellIds.Add(200u);

        VtankDatabase withoutTables = VtankDefaultSettingsDatabase.Parse();
        withoutTables.Tables.RemoveAll(static entry => entry.Name is "ExtraBuffSpells" or "AntiExtraBuffSpells");
        VtankSettingsProfileSerializer.Load(withoutTables.Render(), settings);

        Assert.Empty(settings.Buffs.ExtraBuffSpellIds);
        Assert.Empty(settings.Buffs.AntiExtraBuffSpellIds);
    }

    private static VtankRow ExemplarRow(VtankTable table, int exemplarId, string? extension)
    {
        var row = new VtankRow();
        for (int column = 0; column < table.ColumnNames.Count; column++)
            row.Cells.Add(VtankCell.Int(0));
        row.Cells[table.ColumnIndex("ExemplarId")] = VtankCell.Int(exemplarId);
        if (extension is not null)
            row.Cells[table.ColumnIndex("Extension")] = VtankCell.String(extension);
        return row;
    }

    private static VtankRow GemFoodRow(
        VtankTable table, string name, int spell, string extension)
    {
        var row = new VtankRow();
        for (int column = 0; column < table.ColumnNames.Count; column++)
            row.Cells.Add(VtankCell.Int(0));
        row.Cells[table.ColumnIndex("Name")] = VtankCell.String(name);
        row.Cells[table.ColumnIndex("Spell")] = VtankCell.Int(spell);
        row.Cells[table.ColumnIndex("Extension")] = VtankCell.String(extension);
        return row;
    }

    private static string SingleSettingUsd(string name, string valueTag, string valueText)
    {
        var lines = new List<string>
        {
            "1", "Settings", "4", "Setting", "Value", "Description", "SettingType",
            "y", "n", "n", "n", "1",
            "s", name, valueTag, valueText, "s", string.Empty, "i", "1",
        };
        return string.Join("\r\n", lines) + "\r\n";
    }
}
