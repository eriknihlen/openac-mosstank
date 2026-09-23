namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The document a new settings profile starts from is built from MossTank's
/// own option catalogue and details, not shipped.
/// </summary>
public sealed class VtankDefaultSettingsDatabaseTests
{
    /// <summary>
    /// Every catalogued setting has one row, holding its catalogue default in
    /// the value kind its declared type calls for, its type number, and
    /// MossTank's description. Mutations: leave a setting out; write every
    /// value as a decimal (the switches stop being switches).
    /// </summary>
    [Fact]
    public void EverySettingHasARowWithItsCatalogueDefault()
    {
        VtankTable settings = VtankDefaultSettingsDatabase.Create().Find("Settings")!;

        Assert.Equal(
            VtankOptionCatalog.Names,
            settings.Rows.Select(static row => row.Cells[0].AsString()));
        foreach (VtankRow row in settings.Rows)
        {
            string name = row.Cells[0].AsString();
            VtankSettingValueType type = VtankOptionCatalog.DeclaredType(name);
            MonsterValue expected = VtankOptionCatalog.Default(name);
            VtankCell value = row.Cells[1];
            switch (type)
            {
                case VtankSettingValueType.Bool:
                    Assert.Equal(("b", expected.Boolean), (value.Tag, value.AsBool()));
                    break;
                case VtankSettingValueType.Int or VtankSettingValueType.Enum:
                    Assert.Equal(("i", (int)expected.Number), (value.Tag, value.AsInt()));
                    break;
                case VtankSettingValueType.Double:
                    Assert.Equal(("d", expected.Number), (value.Tag, value.AsDouble()));
                    break;
                case VtankSettingValueType.String:
                    Assert.Equal(("s", expected.Text), (value.Tag, value.AsString()));
                    break;
                case VtankSettingValueType.Custom:
                    Assert.Equal("TABLE", value.Tag);
                    break;
            }
            Assert.Equal((int)type, row.Cells[3].AsInt());
            Assert.Equal(
                VtankOptionDetails.Descriptions.GetValueOrDefault(name, string.Empty),
                row.Cells[2].AsString());
        }
    }

    /// <summary>
    /// Seeding live settings from a new profile gives the recharge plan, the
    /// two blessing gems and the fresh DEFAULT monster row MossTank starts
    /// with. Mutation: drop the recharge plan from the Settings row (a new
    /// profile falls back to no plan of its own).
    /// </summary>
    [Fact]
    public void ANewProfileSeedsTheDefaultListsAndRules()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        var target = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings { RechargeHandlerRows = [] },
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };

        VtankSettingsProfileSerializer.ApplySeedDatabase(database, target);

        Assert.Equal(VtankOptionDetails.DefaultRechargeHandlers, target.Vitals.RechargeHandlerRows);
        Assert.Equal(26, target.Vitals.RechargeHandlerRows.Count);
        Assert.Equal(VtankOptionDetails.DefaultGemFoodItems, target.Buffs.GemFoodItems);
        MonsterRule rule = Assert.Single(VtankMonsterRuleTable.TryRead(database)!);
        Assert.True(rule.IsDefault);
        Assert.Equal(MonsterRuleActions.FreshRow, rule.Actions);
    }

    /// <summary>
    /// The page table the profile carries is the one the advanced options
    /// filter by. Mutation: file a setting under another page in the table
    /// only, and the two disagree.
    /// </summary>
    [Fact]
    public void ThePageTableIsTheOneTheOptionsFilterUses()
    {
        VtankTable pages = VtankDefaultSettingsDatabase.Create().Find("SettingsCategories")!;

        Assert.Equal(
            VtankDefaultSettingsDatabase.SettingCategoryBitmasks.OrderBy(static pair => pair.Key),
            pages.Rows
                .Select(static row => KeyValuePair.Create(row.Cells[0].AsString(), row.Cells[1].AsInt()))
                .OrderBy(static pair => pair.Key));
    }

    /// <summary>
    /// The plugin ships no data file taken from another program: the only
    /// resource it carries is its own experience-cost table. Mutation: embed
    /// a file again, and this fails.
    /// </summary>
    [Fact]
    public void ThePluginEmbedsNoOtherProgramsFiles()
    {
        Assert.Equal(
            ["AcDream.Plugins.MossTank.ExperienceCosts.tsv"],
            typeof(VtankDefaultSettingsDatabase).Assembly.GetManifestResourceNames());
    }
}
