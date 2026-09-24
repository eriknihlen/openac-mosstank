using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankLootRequirementEvaluatorTests
{
    // Spell 2604 grants +20 to IntValueKey 28 (VtankLootRequirementEvaluator's
    // IntSpellBonuses table) — key 28 is not one of the named
    // PluginInventoryItem fields, so it only "exists" via the raw property
    // bag.
    private const uint BonusIntKey = 28u;
    private const uint BonusIntSpellId = 2604u;

    // The "(T) Rare!" rule as Loot5.utl writes it: type 12 (an int key equals
    // a value) on IntValueKey.IconUnderlay (218103850) with 23308, which is
    // the rare backdrop 0x06005B0C without the icon-id prefix.
    private static VtankLootRequirement RareUnderlayRule() => new()
    {
        Type = 12,
        Payload = "23308\r\n218103850\r\n",
    };

    [Fact]
    public void TheRareRuleMatchesAnItemWithTheRareBackdrop()
    {
        PluginInventoryItem rare = Item() with { IconUnderlayId = 0x06005B0Cu };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [RareUnderlayRule()], rare, EmptyProperties(), host: null, out string? error));
        Assert.Null(error);
    }

    [Fact]
    public void TheRareRuleDoesNotMatchAnItemWithoutTheBackdrop()
    {
        PluginInventoryItem ordinary = Item() with { IconUnderlayId = 0u };
        PluginInventoryItem otherBackdrop = Item() with { IconUnderlayId = 0x06006C0Bu };

        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [RareUnderlayRule()], ordinary, EmptyProperties(), host: null, out _));
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [RareUnderlayRule()], otherBackdrop, EmptyProperties(), host: null, out _));
    }

    // The coverage requirement of Loot5.utl's "(C) Multi-Legendary (Shirt)"
    // rules: IntValueKey.Coverage (218103821) equals 104, the chest and both
    // arm sections a shirt covers. Coverage arrives with the object itself,
    // so the rule decides before any appraisal.
    private static VtankLootRequirement ShirtCoverageRule() => new()
    {
        Type = 12,
        Payload = "104\r\n218103821\r\n",
    };

    [Fact]
    public void TheShirtCoverageRuleMatchesAShirtBeforeAppraisal()
    {
        PluginInventoryItem shirt = Item() with { CoverageMask = 104u };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [ShirtCoverageRule()], shirt, EmptyProperties(), host: null, out string? error));
        Assert.Null(error);
        VtankLootRequirementEvaluator.EarlyMatch(
            ShirtCoverageRule(), shirt, EmptyProperties(), host: null,
            out bool hasDecision, out bool isMatch);
        Assert.True(hasDecision);
        Assert.True(isMatch);
    }

    [Fact]
    public void TheShirtCoverageRuleDoesNotMatchPants()
    {
        PluginInventoryItem pants = Item() with { CoverageMask = 22u };

        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [ShirtCoverageRule()], pants, EmptyProperties(), host: null, out _));
    }

    // Loot5.utl's "(H) 430 Insane": a heavy melee weapon that can reach
    // tinked damage 109 with a melee defense and an attack bonus of 1.17.
    private static VtankLootRequirement InsaneMeleeRule() => new()
    {
        Type = 2008,
        Payload = "109\r\n1.17\r\n1.17\r\n",
    };

    // An untinkerable weapon (no material), so no tinks are counted and the
    // numbers stand as they are: 114 max after the rule's +24 with variance
    // 0.3 gives 110 damage over time, and defense 1.18 clears 1.17.
    private static PluginInventoryItem UntinkerableSword() => Item() with
    {
        ObjectClass = PluginObjectClass.MeleeWeapon,
        Damage = 90,
        DamageVariance = 0.3d,
    };

    private static PluginItemProperties MeleeProperties(double weaponOffense) =>
        EmptyProperties() with
        {
            Floats = new Dictionary<uint, double> { [29u] = 1.18d },
            WeaponProfile = Weapon() with { WeaponOffense = weaponOffense },
        };

    [Fact]
    public void TheInsaneMeleeRuleReadsTheAttackBonusFromTheAppraisedWeapon()
    {
        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [InsaneMeleeRule()], UntinkerableSword(), MeleeProperties(1.18d),
            host: null, out string? error));
        Assert.Null(error);
    }

    [Fact]
    public void TheInsaneMeleeRuleRejectsAWeaponWhoseAttackBonusFallsShort()
    {
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [InsaneMeleeRule()], UntinkerableSword(), MeleeProperties(1.10d),
            host: null, out _));
    }

    [Fact]
    public void ABuffedAttackBonusRuleAddsTheWeaponsOwnAttackCantrip()
    {
        // Type 2005 on DoubleValueKey AttackBonus (167772172): the appraised
        // offense 1.15 plus spell 2591's +0.05 reaches 1.20, over 1.19.
        var rule = new VtankLootRequirement
        {
            Type = 2005,
            Payload = "1.19\r\n167772172\r\n",
        };
        PluginInventoryItem sword = Item() with { AppraisedSpellIds = [2591u] };
        PluginItemProperties properties = EmptyProperties() with
        {
            WeaponProfile = Weapon() with { WeaponOffense = 1.15d },
        };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [rule], sword, properties, host: null, out string? error));
        Assert.Null(error);
    }

    // Loot5.utl's "(M) Bow (OD +10)": buffed missile damage of 78.6, which is
    // damage + (damage modifier - 1) * 100 / 3 + elemental damage bonus.
    private static VtankLootRequirement BowDamageRule() => new()
    {
        Type = 2001,
        Payload = "78.6\r\n",
    };

    private static PluginItemProperties BowProperties(double damageMod) =>
        EmptyProperties() with
        {
            Ints = new Dictionary<uint, int> { [204u] = 22 },
            WeaponProfile = Weapon() with { Damage = 0, DamageMod = damageMod },
        };

    [Fact]
    public void TheBowDamageRuleReadsTheDamageModifierFromTheAppraisedWeapon()
    {
        // (2.70 - 1) * 100 / 3 = 56.7, plus 22 elemental: 78.7.
        PluginInventoryItem bow = Item() with { ObjectClass = PluginObjectClass.MissileWeapon };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [BowDamageRule()], bow, BowProperties(2.70d), host: null, out string? error));
        Assert.Null(error);
    }

    [Fact]
    public void TheBowDamageRuleRejectsABowWithTooSmallAModifier()
    {
        // (2.60 - 1) * 100 / 3 = 53.3, plus 22: 75.3.
        PluginInventoryItem bow = Item() with { ObjectClass = PluginObjectClass.MissileWeapon };

        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [BowDamageRule()], bow, BowProperties(2.60d), host: null, out _));
    }

    [Fact]
    public void RangeIsTheLaunchSpeedTheAppraisedWeaponReports()
    {
        // Type 5 (a double at least) on DoubleValueKey Range (167772173).
        var rule = new VtankLootRequirement { Type = 5, Payload = "25\r\n167772173\r\n" };
        PluginItemProperties fast = EmptyProperties() with
        {
            WeaponProfile = Weapon() with { MaxVelocity = 27.3d },
        };
        PluginItemProperties slow = EmptyProperties() with
        {
            WeaponProfile = Weapon() with { MaxVelocity = 18d },
        };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [rule], Item(), fast, host: null, out _));
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [rule], Item(), slow, host: null, out _));
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [rule], Item(), EmptyProperties(), host: null, out _));
    }

    [Fact]
    public void WeaponSpeedIsTheAppraisedSpeedRatingNotTheCombatRole()
    {
        // Type 12 (an int equals) on IntValueKey WeapSpeed (218103839): a
        // weapon of speed 25. An item with a combat role but no
        // appraised weapon has no speed at all.
        var rule = new VtankLootRequirement { Type = 12, Payload = "25\r\n218103839\r\n" };
        PluginInventoryItem sword = Item() with { CombatUse = 1 };
        PluginItemProperties appraised = EmptyProperties() with
        {
            WeaponProfile = Weapon() with { WeaponTime = 25 },
        };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [rule], sword, appraised, host: null, out _));
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [rule], sword with { CombatUse = 25 }, EmptyProperties(), host: null, out _));
    }

    // Every protection distinct, so a key read from the wrong one fails.
    private static PluginArmorProfile Armor() => new(
        ArmorLevel: 500,
        SlashMod: 0.1f,
        PierceMod: 0.2f,
        BludgeonMod: 0.3f,
        ColdMod: 0.4f,
        FireMod: 0.5f,
        AcidMod: 0.6f,
        NetherMod: 0.7f,
        ElectricMod: 0.8f);

    [Theory]
    [InlineData(167772160u, "0.1")] // SlashProt
    [InlineData(167772161u, "0.2")] // PierceProt
    [InlineData(167772162u, "0.3")] // BludgeonProt
    [InlineData(167772163u, "0.6")] // AcidProt
    [InlineData(167772164u, "0.8")] // LightningProt
    [InlineData(167772165u, "0.5")] // FireProt
    [InlineData(167772166u, "0.4")] // ColdProt
    public void EachProtectionKeyReadsItsOwnAppraisedArmorNumber(uint key, string expected)
    {
        // Types 5 and 4 together (at least and at most) pin the exact value.
        VtankLootRequirement[] rules =
        [
            new() { Type = 5, Payload = $"{expected}\r\n{key}\r\n" },
            new() { Type = 4, Payload = $"{expected}\r\n{key}\r\n" },
        ];
        PluginItemProperties armor = EmptyProperties() with { ArmorProfile = Armor() };

        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            rules, Item(), armor, host: null, out string? error));
        Assert.Null(error);
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            rules, Item(), EmptyProperties(), host: null, out _));
    }

    private static PluginWeaponProfile Weapon() => new(
        DamageType: 1,
        WeaponTime: 40,
        WeaponSkill: 44u,
        Damage: 50,
        DamageVariance: 0.5d,
        DamageMod: 1d,
        WeaponLength: 1d,
        MaxVelocity: 1d,
        WeaponOffense: 1d,
        MaxVelocityEstimated: 0);

    private static PluginItemProperties EmptyProperties() => new(
        Ints: new Dictionary<uint, int>(),
        Int64s: new Dictionary<uint, long>(),
        Bools: new Dictionary<uint, bool>(),
        Floats: new Dictionary<uint, double>(),
        Strings: new Dictionary<uint, string>(),
        DataIds: new Dictionary<uint, uint>(),
        InstanceIds: new Dictionary<uint, uint>());

    [Fact]
    public void BuffedIntRequirementDoesNotApplyBonusWhenBaseKeyIsAbsent()
    {
        PluginInventoryItem item = Item() with
        {
            AppraisedSpellIds = [BonusIntSpellId],
        };
        var properties = new PluginItemProperties(
            Ints: new Dictionary<uint, int>(), // key 28 absent entirely
            Int64s: new Dictionary<uint, long>(),
            Bools: new Dictionary<uint, bool>(),
            Floats: new Dictionary<uint, double>(),
            Strings: new Dictionary<uint, string>(),
            DataIds: new Dictionary<uint, uint>(),
            InstanceIds: new Dictionary<uint, uint>());

        // Type 2003 = BuffedLongValKeyGE: values[0]=threshold, values[1]=key.
        // Pre-fix this would read base=0, add the +20 bonus unconditionally
        // (20 >= 10 => true); the gate must keep it at the un-buffed base
        // value (0 >= 10 => false) since the key never existed at all.
        var requirement = new VtankLootRequirement
        {
            Type = 2003,
            Payload = $"10\r\n{BonusIntKey}\r\n",
        };
        bool matched = VtankLootRequirementEvaluator.IsMatch(
            [requirement], item, properties, host: null, out string? error);
        Assert.Null(error);
        Assert.False(matched);
    }

    [Fact]
    public void BuffedIntRequirementAppliesBonusWhenBaseKeyExists()
    {
        PluginInventoryItem item = Item() with
        {
            AppraisedSpellIds = [BonusIntSpellId],
        };
        var properties = new PluginItemProperties(
            Ints: new Dictionary<uint, int> { [BonusIntKey] = 0 }, // key exists, base 0
            Int64s: new Dictionary<uint, long>(),
            Bools: new Dictionary<uint, bool>(),
            Floats: new Dictionary<uint, double>(),
            Strings: new Dictionary<uint, string>(),
            DataIds: new Dictionary<uint, uint>(),
            InstanceIds: new Dictionary<uint, uint>());

        var requirement = new VtankLootRequirement
        {
            Type = 2003,
            Payload = $"10\r\n{BonusIntKey}\r\n",
        };
        // Base key exists (value 0) so the +20 bonus applies: 20 >= 10.
        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [requirement], item, properties, host: null, out string? error));
        Assert.Null(error);
    }

    private const uint BonusDoubleKey = 29u;
    private const uint BonusDoubleAdditiveSpellId = 2600u;

    [Fact]
    public void BuffedDoubleRequirementDoesNotApplyBonusWhenBaseKeyIsAbsent()
    {
        PluginInventoryItem item = Item() with
        {
            AppraisedSpellIds = [BonusDoubleAdditiveSpellId],
        };
        var properties = new PluginItemProperties(
            Ints: new Dictionary<uint, int>(),
            Int64s: new Dictionary<uint, long>(),
            Bools: new Dictionary<uint, bool>(),
            Floats: new Dictionary<uint, double>(), // key 29 absent entirely
            Strings: new Dictionary<uint, string>(),
            DataIds: new Dictionary<uint, uint>(),
            InstanceIds: new Dictionary<uint, uint>());

        // Type 2005 = BuffedDoubleValKeyGE: values[0]=threshold, values[1]=key.
        // Pre-fix (no double-side gate) this would read base=0, add the
        // +.03 bonus unconditionally (0.03 >= 0.02 => true); the gate must
        // keep it at the un-buffed base value (0 >= 0.02 => false) since the
        // key never existed at all.
        var requirement = new VtankLootRequirement
        {
            Type = 2005,
            Payload = $"0.02\r\n{BonusDoubleKey}\r\n",
        };
        bool matched = VtankLootRequirementEvaluator.IsMatch(
            [requirement], item, properties, host: null, out string? error);
        Assert.Null(error);
        Assert.False(matched);
    }

    [Fact]
    public void BuffedDoubleRequirementAppliesAdditiveBonusWhenBaseKeyExists()
    {
        PluginInventoryItem item = Item() with
        {
            AppraisedSpellIds = [BonusDoubleAdditiveSpellId],
        };
        var properties = new PluginItemProperties(
            Ints: new Dictionary<uint, int>(),
            Int64s: new Dictionary<uint, long>(),
            Bools: new Dictionary<uint, bool>(),
            Floats: new Dictionary<uint, double> { [BonusDoubleKey] = 0d }, // key exists, base 0
            Strings: new Dictionary<uint, string>(),
            DataIds: new Dictionary<uint, uint>(),
            InstanceIds: new Dictionary<uint, uint>());

        var requirement = new VtankLootRequirement
        {
            Type = 2005,
            Payload = $"0.02\r\n{BonusDoubleKey}\r\n",
        };
        // Base key exists (value 0) so the +.03 additive bonus applies:
        // 0.03 >= 0.02.
        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            [requirement], item, properties, host: null, out string? error));
        Assert.Null(error);
    }

    [Fact]
    public void BuffedDoubleRequirementAppliesMultiplicativeBonusWhenChangeIsSet()
    {
        const uint MultiplicativeKey = 144u;
        const uint MultiplicativeSpellId = 3201u;
        PluginInventoryItem item = Item() with
        {
            AppraisedSpellIds = [MultiplicativeSpellId],
        };
        var properties = new PluginItemProperties(
            Ints: new Dictionary<uint, int>(),
            Int64s: new Dictionary<uint, long>(),
            Bools: new Dictionary<uint, bool>(),
            Floats: new Dictionary<uint, double> { [MultiplicativeKey] = 10d }, // base 10
            Strings: new Dictionary<uint, string>(),
            DataIds: new Dictionary<uint, uint>(),
            InstanceIds: new Dictionary<uint, uint>());

        var additiveWouldPass = new VtankLootRequirement
        {
            Type = 2005,
            Payload = $"10.6\r\n{MultiplicativeKey}\r\n",
        };
        // Additive would give 11.05 (>= 10.6 => true); multiplicative gives
        // 10.5 (>= 10.6 => false). Observing "false" here proves the branch
        // took the multiplicative path.
        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            [additiveWouldPass], item, properties, host: null, out string? error));
        Assert.Null(error);
    }

    private static PluginInventoryItem Item() => new(
        0u, 0u, "Test Item", 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u,
        1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);
}
