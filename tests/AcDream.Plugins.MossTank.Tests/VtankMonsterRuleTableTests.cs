namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankMonsterRuleTableTests
{
    [Fact]
    public void ShippedDefaultRowParsesToTheShippedValues()
    {
        // The shipped default settings row: <DEFAULT>, priority 1,
        // DamageType 8 (Auto), WeaponToUse -1, Attack and Streak set,
        // SecondaryVuln 98 (None), SecondaryEquip 0 (Auto),
        // PetDamageType 101 (PAuto).
        List<MonsterRule>? rules = VtankMonsterRuleTable.TryRead(
            VtankDefaultSettingsDatabase.Parse());

        MonsterRule rule = Assert.Single(rules!);
        Assert.True(rule.IsDefault);
        Assert.Equal(VtankMonsterRuleTable.DefaultRowName, rule.Expression);
        Assert.Equal(1, rule.Priority);
        Assert.Equal(MonsterDamageType.Auto, rule.Actions.DamageType);
        Assert.Equal(
            MonsterActionFlags.Attack | MonsterActionFlags.Streak,
            rule.Actions.Flags);
        Assert.Equal(MonsterDamageType.None, rule.Actions.ExtraVulnerability);
        Assert.Equal(MonsterDamageType.PlayerAuto, rule.Actions.PetDamageType);
        Assert.Equal(0u, rule.Actions.WeaponObjectId);
        Assert.Equal(0u, rule.Actions.OffhandObjectId);
    }

    /// <summary>
    /// The extra vulnerability column's file codes: 98 is "none", 8 is
    /// "automatic", 3 is acid. Every one of them survives a read and a write
    /// unchanged, byte for byte.
    /// </summary>
    [Fact]
    public void TheExtraVulnerabilityColumnRoundTripsItsFileCodes()
    {
        const int extraVulnerabilityColumn = 18;
        foreach ((int code, MonsterDamageType expected) in new[]
        {
            (98, MonsterDamageType.None),
            (8, MonsterDamageType.Auto),
            (3, MonsterDamageType.Acid),
        })
        {
            VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
            seed.Find(VtankMonsterRuleTable.TableName)!
                .Rows[0].Cells[extraVulnerabilityColumn] = VtankCell.Int(code);
            string original = seed.Render();

            VtankDatabase database = VtankDatabase.Parse(original);
            List<MonsterRule> rules = VtankMonsterRuleTable.TryRead(database)!;
            Assert.Equal(expected, rules[0].Actions.ExtraVulnerability);

            VtankMonsterRuleTable.Write(database, rules);

            Assert.Equal(original, database.Render());
        }
    }

    /// <summary>
    /// A row nobody has edited asks for no extra vulnerability, and saves as
    /// the "none" code.
    /// Mutation: default <c>MonsterRuleActions.ExtraVulnerability</c> to
    /// automatic again and this writes 8 - the spelling that made a profile
    /// with the vulnerability column unticked debuff every monster it met.
    /// </summary>
    [Fact]
    public void AFreshRowWritesTheExtraVulnerabilityColumnOff()
    {
        const int extraVulnerabilityColumn = 18;
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();

        VtankMonsterRuleTable.Write(
            database,
            [new MonsterRule("DEFAULT", new MonsterRuleActions())]);

        Assert.Equal(
            98,
            database.Find(VtankMonsterRuleTable.TableName)!
                .Rows[0].Cells[extraVulnerabilityColumn].AsInt());
    }

    [Fact]
    public void UntouchedRowsRoundTripByteForByte()
    {
        // Save() rewrites the table from the live rules, so an untouched
        // profile must still render exactly what it parsed.
        string original = VtankDefaultSettingsDatabase.Parse().Render();
        VtankDatabase database = VtankDatabase.Parse(original);
        List<MonsterRule> rules = VtankMonsterRuleTable.TryRead(database)!;

        VtankMonsterRuleTable.Write(database, rules);

        Assert.Equal(original, database.Render());
    }

    [Fact]
    public void PriorityWeaponAndSecondaryEquipShapesRoundTripByteForByte()
    {
        VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
        VtankRow row = seed.Find(VtankMonsterRuleTable.TableName)!.Rows[0];
        row.Cells[1] = VtankCell.Int(7);
        row.Cells[3] = VtankCell.Int(0);
        row.Cells[19] = VtankCell.Int((int)VtankSecondaryEquip.AutoShield);
        string original = seed.Render();

        VtankDatabase database = VtankDatabase.Parse(original);
        List<MonsterRule> rules = VtankMonsterRuleTable.TryRead(database)!;
        Assert.Equal(7, rules[0].Priority);

        VtankMonsterRuleTable.Write(database, rules);

        Assert.Equal(original, database.Render());
    }

    /// <summary>
    /// Mutation pin: keep the signed bits of direct primary and secondary
    /// object ids instead of treating a negative signed spelling as a mode.
    /// Mutation executed: <c>WeaponObjectId used weapon &gt; 0 and
    /// OffhandObjectId used offhand &gt;= ListedTypesEnd</c>.
    /// </summary>
    [Fact]
    public void SignedDirectEquipmentIdsSurviveParseAndRoundTrip()
    {
        const uint primaryId = 0x8000_DEB2u;
        const uint secondaryId = 0x8001_AC87u;
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankTable monsters = database.Find(VtankMonsterRuleTable.TableName)!;
        monsters.Rows[0].Cells[3] = VtankCell.Int(unchecked((int)primaryId));
        monsters.Rows[0].Cells[19] = VtankCell.Int(unchecked((int)secondaryId));

        MonsterRuleActions actions = Assert.Single(
            VtankMonsterRuleTable.TryRead(database)!).Actions;

        Assert.Equal(primaryId, actions.WeaponObjectId);
        Assert.Equal(secondaryId, actions.OffhandObjectId);
        VtankMonsterRuleTable.Write(database, [new MonsterRule("DEFAULT", actions)]);
        Assert.Equal(unchecked((int)primaryId), monsters.Rows[0].Cells[3].AsInt());
        Assert.Equal(unchecked((int)secondaryId), monsters.Rows[0].Cells[19].AsInt());
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    public void EverySecondaryEquipModeSurvivesASave(int mode)
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankTable monsters = database.Find(VtankMonsterRuleTable.TableName)!;
        monsters.Rows[0].Cells[19] = VtankCell.Int(mode);

        List<MonsterRule> rules = VtankMonsterRuleTable.TryRead(database)!;
        VtankMonsterRuleTable.Write(database, rules);

        Assert.Equal(mode, monsters.Rows[0].Cells[19].AsInt());
    }

    [Theory]
    [InlineData(0, 0)]
    [InlineData(-1, -1)]
    [InlineData(0x4000_0011, 0x4000_0011)]
    public void WeaponToUseKeepsTheSpellingTheFileUsed(int stored, int expected)
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankTable monsters = database.Find(VtankMonsterRuleTable.TableName)!;
        monsters.Rows[0].Cells[3] = VtankCell.Int(stored);

        List<MonsterRule> rules = VtankMonsterRuleTable.TryRead(database)!;
        VtankMonsterRuleTable.Write(database, rules);

        Assert.Equal(expected, monsters.Rows[0].Cells[3].AsInt());
    }

    [Fact]
    public void AnUnparseableSpecIsLoggedAndIgnoredInsteadOfLosingTheProfile()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankTable monsters = database.Find(VtankMonsterRuleTable.TableName)!;
        monsters.Rows[0].Cells[0] = VtankCell.String("(((");
        var warnings = new List<string>();

        List<MonsterRule>? rules = VtankMonsterRuleTable.TryRead(
            database,
            warnings.Add);

        MonsterRule rule = Assert.Single(rules!);
        Assert.Equal("(((", rule.Expression);
        Assert.True(rule.IsIgnoredSpec);
        Assert.False(
            rule.Matches(
                new MonsterExpressionContext(
                    "Drudge",
                    TypeId: 1u,
                    Species: "Drudge",
                    MaximumHealth: 100,
                    Range: 5f,
                    HasShield: false,
                    MetaState: string.Empty,
                    Setting: null),
                out _));
        Assert.Contains(
            warnings,
            warning => warning.StartsWith(
                "Parse error in monster spec: \"(((\", ignoring entry (",
                StringComparison.Ordinal));

        // And the row is still the row: it round-trips like any other.
        VtankMonsterRuleTable.Write(database, rules!);
        Assert.Equal("(((", monsters.Rows[0].Cells[0].AsString());
    }

    [Fact]
    public void LoadKeepsEverySettingWhenOneMonsterSpecIsUnparseable()
    {
        VtankDatabase seed = VtankDefaultSettingsDatabase.Parse();
        seed.Find(VtankMonsterRuleTable.TableName)!.Rows[0].Cells[0] =
            VtankCell.String("(((");
        var combat = new CombatSettings();
        var warnings = new List<string>();

        VtankSettingsProfileSerializer.Load(
            seed.Render(),
            new VtankSettingsProfileSerializer.AllSettings
            {
                Combat = combat,
                Buffs = new BuffSettings(),
                Vitals = new VitalSettings(),
                Inventory = new InventorySettings(),
                Navigation = new NavigationSettings(),
            },
            warnings.Add);

        Assert.NotEmpty(warnings);
        MonsterRule rule = Assert.Single(combat.Rules);
        Assert.Equal("(((", rule.Expression);
    }

    [Fact]
    public void EveryColumnSurvivesAWriteThenReadCycle()
    {
        var rules = new List<MonsterRule>
        {
            new("DEFAULT", new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Attack,
                Priority = 1,
            }),
            new("species==drudge", new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Imperil
                    | MonsterActionFlags.Vulnerability
                    | MonsterActionFlags.Yield
                    | MonsterActionFlags.GravityWell
                    | MonsterActionFlags.Ring
                    | MonsterActionFlags.Broadside
                    | MonsterActionFlags.Fester
                    | MonsterActionFlags.WeakeningCurse
                    | MonsterActionFlags.FesteringCurse
                    | MonsterActionFlags.Corruption
                    | MonsterActionFlags.DestructiveCurse
                    | MonsterActionFlags.Corrosion
                    | MonsterActionFlags.Streak,
                Priority = 4,
                DamageType = MonsterDamageType.Electric,
                ExtraVulnerability = MonsterDamageType.Acid,
                PetDamageType = MonsterDamageType.Cold,
                WeaponObjectId = 0x4000_0011u,
                OffhandObjectId = 0x4000_0022u,
            }),
            new("name#^Olthoi", new MonsterRuleActions
            {
                Flags = MonsterActionFlags.Imperil,
                Priority = -1,
                DamageType = MonsterDamageType.VoidBasic,
            }),
        };
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();

        VtankMonsterRuleTable.Write(database, rules);
        List<MonsterRule> read = VtankMonsterRuleTable.TryRead(database)!;

        Assert.Equal(3, read.Count);
        for (int i = 0; i < rules.Count; i++)
        {
            Assert.Equal(rules[i].Actions.Flags, read[i].Actions.Flags);
            Assert.Equal(rules[i].Priority, read[i].Priority);
            Assert.Equal(rules[i].Actions.DamageType, read[i].Actions.DamageType);
            Assert.Equal(
                rules[i].Actions.ExtraVulnerability,
                read[i].Actions.ExtraVulnerability);
            Assert.Equal(
                rules[i].Actions.PetDamageType,
                read[i].Actions.PetDamageType);
            Assert.Equal(
                rules[i].Actions.WeaponObjectId,
                read[i].Actions.WeaponObjectId);
            Assert.Equal(
                rules[i].Actions.OffhandObjectId,
                read[i].Actions.OffhandObjectId);
        }
        Assert.Equal(VtankMonsterRuleTable.DefaultRowName, read[0].Expression);
        Assert.Equal("species==drudge", read[1].Expression);
        Assert.Equal("name#^Olthoi", read[2].Expression);
    }

    /// <summary>
    /// The attack column is stored exactly the way the row's tick box shows
    /// it: a row that attacks stores True, a row that does not stores False.
    /// This earns a pin of its own because the field the column is built from
    /// is negated twice - once on the way into the file and once on the way
    /// into the tick box - so the two negations cancel and the stored column
    /// is straight. Reading it inverted would make every authored profile
    /// fight exactly the monsters it was told to leave alone.
    /// Mutation: negate either side and
    /// <see cref="ShippedDefaultRowParsesToTheShippedValues"/> reads the
    /// shipped "attack anything" default as a row that never attacks.
    /// </summary>
    [Fact]
    public void TheAttackColumnIsStoredTheSameWayTheTickBoxShowsIt()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankMonsterRuleTable.Write(
            database,
            [
                new MonsterRule("DEFAULT", new MonsterRuleActions
                {
                    Flags = MonsterActionFlags.Attack,
                }),
                new MonsterRule("nope", new MonsterRuleActions
                {
                    Flags = MonsterActionFlags.None,
                }),
            ]);

        VtankTable table = database.Find(VtankMonsterRuleTable.TableName)!;
        Assert.True(table.Rows[0].Cells[8].AsBool());
        Assert.False(table.Rows[1].Cells[8].AsBool());

        // And the two rows read back to the flags they were written from.
        List<MonsterRule> read = VtankMonsterRuleTable.TryRead(database)!;
        Assert.True(read[0].Actions.UsesPrimaryAttack);
        Assert.False(read[1].Actions.UsesPrimaryAttack);
    }

    /// <summary>
    /// The shape authored profiles really use: a blanket row that attacks
    /// everything it meets, and beneath it a range-keyed row that leaves
    /// distant monsters alone while still debuffing them. Read the attack
    /// column inverted and those two swap, which is the profile inside out.
    /// </summary>
    [Fact]
    public void ARangeKeyedRowThatOnlyDebuffsDoesNotAttack()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();
        VtankTable table = database.Find(VtankMonsterRuleTable.TableName)!;
        var distant = new VtankRow();
        foreach (VtankCell cell in table.Rows[0].Cells)
            distant.Cells.Add(cell);
        distant.Cells[0] = VtankCell.String("range>5");
        distant.Cells[4] = VtankCell.Bool(true);
        distant.Cells[8] = VtankCell.Bool(false);
        table.Rows.Add(distant);

        List<MonsterRule> rules = VtankMonsterRuleTable.TryRead(database)!;

        Assert.True(rules[0].IsDefault);
        Assert.True(rules[0].Actions.UsesPrimaryAttack);
        Assert.Equal("range>5", rules[1].Expression);
        Assert.False(rules[1].Actions.UsesPrimaryAttack);
        Assert.Equal(
            MonsterActionFlags.Imperil,
            rules[1].Actions.Flags & MonsterActionFlags.Imperil);
    }

    /// <summary>
    /// A brand-new monster row, column for column: priority one, automatic
    /// damage, automatic weapon, attack and streak ticked, every other tick
    /// off, no extra vulnerability, automatic off-hand and an automatic pet
    /// element. A fresh row that does not match this writes a profile the
    /// editor would show differently from the one the player just made, and a
    /// row added beside authored rows would behave differently from its
    /// neighbours for no reason the player can see.
    /// Mutation: drop the streak tick, or put priority back to zero, and this
    /// fails on exactly that column.
    /// </summary>
    [Fact]
    public void AFreshMonsterRowIsWrittenColumnForColumn()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Parse();

        VtankMonsterRuleTable.Write(database, [MonsterRule.Fresh("DEFAULT")]);

        VtankRow row = database.Find(VtankMonsterRuleTable.TableName)!.Rows[0];
        Assert.Equal(VtankMonsterRuleTable.DefaultRowName, row.Cells[0].AsString());
        Assert.Equal(1, row.Cells[1].AsInt());
        Assert.Equal(8, row.Cells[2].AsInt());
        Assert.Equal(-1, row.Cells[3].AsInt());
        Assert.True(row.Cells[8].AsBool());
        Assert.True(row.Cells[17].AsBool());
        foreach (int column in new[] { 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16 })
            Assert.False(row.Cells[column].AsBool());
        Assert.Equal(98, row.Cells[18].AsInt());
        Assert.Equal(0, row.Cells[19].AsInt());
        Assert.Equal(101, row.Cells[20].AsInt());
    }

    /// <summary>
    /// A profile whose only rule is an untouched fresh row still needs no
    /// monster table written into a file that never had one.
    /// </summary>
    [Fact]
    public void AnUntouchedFreshRowDoesNotFabricateAMonsterTable()
    {
        var database = new VtankDatabase();

        VtankMonsterRuleTable.Write(database, [MonsterRule.Fresh("DEFAULT")]);

        Assert.Null(database.Find(VtankMonsterRuleTable.TableName));
    }

    /// <summary>
    /// But a row the player did touch is written even when it is the only
    /// one, because dropping it would silently lose the edit.
    /// </summary>
    [Fact]
    public void AnEditedLoneDefaultRowIsWrittenEvenWithoutAnExistingTable()
    {
        var database = new VtankDatabase();

        VtankMonsterRuleTable.Write(
            database,
            [
                new MonsterRule(
                    "DEFAULT",
                    MonsterRuleActions.FreshRow with
                    {
                        DamageType = MonsterDamageType.Fire,
                    }),
            ]);

        VtankTable table = database.Find(VtankMonsterRuleTable.TableName)!;
        Assert.Equal(6, table.Rows[0].Cells[2].AsInt());
    }

    [Fact]
    public void DamageElementNumbersAreVtanksNotMossTanks()
    {
        Assert.Equal(4, VtankDamageElement.FromMonsterDamageType(
            MonsterDamageType.Electric));
        Assert.Equal(9, VtankDamageElement.FromMonsterDamageType(
            MonsterDamageType.VoidBasic));
        Assert.Equal(98, VtankDamageElement.FromMonsterDamageType(
            MonsterDamageType.None));
        Assert.Equal(101, VtankDamageElement.FromMonsterDamageType(
            MonsterDamageType.PlayerAuto));
        Assert.Equal(
            MonsterDamageType.Electric,
            VtankDamageElement.ToMonsterDamageType(4));
        Assert.Equal(
            MonsterDamageType.PlayerAuto,
            VtankDamageElement.ToMonsterDamageType(101));
        // The legacy prismatic database id folds onto Prismatic.
        Assert.Equal(
            MonsterDamageType.Prismatic,
            VtankDamageElement.ToMonsterDamageType(100));
    }

    [Fact]
    public void ADropInVtankProfilesRulesBecomeLive()
    {
        VtankDatabase source = VtankDefaultSettingsDatabase.Parse();
        VtankMonsterRuleTable.Write(
            source,
            [
                new MonsterRule("DEFAULT", new MonsterRuleActions
                {
                    Flags = MonsterActionFlags.Attack,
                }),
                new MonsterRule("name#^Olthoi", new MonsterRuleActions
                {
                    Flags = MonsterActionFlags.Attack | MonsterActionFlags.Imperil,
                    Priority = 4,
                    DamageType = MonsterDamageType.Acid,
                }),
            ]);
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };

        VtankSettingsProfileSerializer.Load(source.Render(), settings);

        Assert.Equal(2, settings.Combat.Rules.Count);
        Assert.Equal("name#^Olthoi", settings.Combat.Rules[1].Expression);
        Assert.Equal(4, settings.Combat.Rules[1].Priority);
        Assert.Equal(
            MonsterDamageType.Acid,
            settings.Combat.Rules[1].Actions.DamageType);
    }
}
