using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankAmmunitionDatabaseTests
{
    /// <summary>
    /// The fixture game database's AmmunitionOptions rows. The four bow rows
    /// were written for these tests; none is anyone's real data.
    /// </summary>
    private static readonly IReadOnlyList<VtankAmmunitionOption> FixtureOptions =
        VtankGameInfoDatabase.Parse(File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "gameinfodb-excerpt.ugd")))
            .AmmunitionOptions;

    [Fact]
    public void LauncherTypeFollowsTheAmmoType()
    {
        Assert.Equal(5, VtankAmmunitionDatabase.LauncherType(Launcher(1u)));
        Assert.Equal(6, VtankAmmunitionDatabase.LauncherType(Launcher(2u)));
        Assert.Equal(7, VtankAmmunitionDatabase.LauncherType(Launcher(4u)));
    }

    [Fact]
    public void SpecialMaskSelectsRaiderOnlyWhenEnabled()
    {
        var character = new Character(
        [
            new PluginSkillInfo(
                47u,
                "Missile Weapons",
                PluginSkillTraining.Trained,
                300u)
            {
                Base = 300u,
            },
        ]);

        VtankAmmunitionOption regular = Assert.IsType<VtankAmmunitionOption>(
            VtankAmmunitionDatabase.Select(
                FixtureOptions,
                5,
                MonsterDamageType.Electric,
                VtankPrismaticAmmoPolicy.NoPrismatic,
                enabledSpecialMask: 0,
                character,
                static _ => true));
        VtankAmmunitionOption raider = Assert.IsType<VtankAmmunitionOption>(
            VtankAmmunitionDatabase.Select(
                FixtureOptions,
                5,
                MonsterDamageType.Electric,
                VtankPrismaticAmmoPolicy.NoPrismatic,
                enabledSpecialMask: 1,
                character,
                static _ => true));

        Assert.Equal("Fixture Lightning Arrow", regular.Name);
        Assert.Equal("Fixture Raider Lightning Arrow", raider.Name);
    }

    [Fact]
    public void PrismaticAmmoRequiresBothOfficialSkills()
    {
        var missileOnly = new Character(
        [
            new PluginSkillInfo(
                47u,
                "Missile Weapons",
                PluginSkillTraining.Trained,
                400u)
            {
                Base = 400u,
            },
        ]);
        var both = new Character(
        [
            new PluginSkillInfo(
                47u,
                "Missile Weapons",
                PluginSkillTraining.Trained,
                400u)
            {
                Base = 400u,
            },
            new PluginSkillInfo(
                37u,
                "Fletching",
                PluginSkillTraining.Trained,
                400u),
        ]);

        VtankAmmunitionOption withoutFletching =
            Assert.IsType<VtankAmmunitionOption>(
                VtankAmmunitionDatabase.Select(
                    FixtureOptions,
                    5,
                    MonsterDamageType.Fire,
                    VtankPrismaticAmmoPolicy.ForcePrismatic,
                    0,
                    missileOnly,
                    static _ => true));
        VtankAmmunitionOption withFletching =
            Assert.IsType<VtankAmmunitionOption>(
                VtankAmmunitionDatabase.Select(
                    FixtureOptions,
                    5,
                    MonsterDamageType.Fire,
                    VtankPrismaticAmmoPolicy.ForcePrismatic,
                    0,
                    both,
                    static _ => true));

        Assert.Equal("Fixture Fire Arrow", withoutFletching.Name);
        Assert.Equal("Fixture Prismatic Arrow", withFletching.Name);
    }

    /// <summary>
    /// Mutation: classify launchers from AmmoType alone; the melee item
    /// incorrectly becomes a bow, and the extended bit becomes a crossbow.
    /// </summary>
    [Fact]
    public void LauncherKindRequiresMissileClassAndAnExactAmmoType()
    {
        Assert.Equal(0, VtankAmmunitionDatabase.LauncherType(
            Launcher(1u) with { ObjectClass = PluginObjectClass.MeleeWeapon }));
        Assert.Equal(0, VtankAmmunitionDatabase.LauncherType(Launcher(0x80u)));
        Assert.Equal(VtankAmmunitionDatabase.MissileKind.Thrown,
            VtankAmmunitionDatabase.Kind(Launcher(0u)));
    }

    /// <summary>
    /// Mutation: recognize only element 100 as prismatic; the current
    /// element-11 row ceases to be selected for a non-element request.
    /// </summary>
    [Fact]
    public void CurrentAndLegacyPrismaticRowsRemainCandidatesForHarm()
    {
        VtankAmmunitionOption[] options =
        [
            new("Current Prism", 5, 0, 11, 300, 0, 0u, 0),
            new("Legacy Prism", 5, 0, 100, 301, 0, 0u, 0),
        ];

        VtankAmmunitionOption current = Assert.IsType<VtankAmmunitionOption>(
            VtankAmmunitionDatabase.Select(options[..1], 5, MonsterDamageType.Harm,
                VtankPrismaticAmmoPolicy.Any, 0, new Character([]),
                static _ => true));
        Assert.Equal("Current Prism", current.Name);
        VtankAmmunitionOption chosen = Assert.IsType<VtankAmmunitionOption>(
            VtankAmmunitionDatabase.Select(options, 5, MonsterDamageType.Harm,
                VtankPrismaticAmmoPolicy.Any, 0, new Character([]),
                static _ => true));
        Assert.Equal("Legacy Prism", chosen.Name);
        Assert.Equal(100, options[1].Element);
    }

    /// <summary>
    /// Mutation: permit Unknown primary training as though it were trained;
    /// the requirement-bearing option becomes available again.
    /// </summary>
    [Fact]
    public void PrimaryRequirementRejectsUnknownTraining()
    {
        var character = new Character(
        [
            new PluginSkillInfo(47u, "Missile Weapons",
                PluginSkillTraining.Unknown, 500u) { Base = 500u },
        ]);
        VtankAmmunitionOption[] options =
        [
            new("Trained Arrow", 5, 300, 6, 100, 0, 0u, 0),
        ];

        Assert.Null(VtankAmmunitionDatabase.Select(options, 5,
            MonsterDamageType.Fire, VtankPrismaticAmmoPolicy.NoPrismatic,
            0, character, static _ => true));
    }

    /// <summary>
    /// Mutation: return Pierce for a prismatic request; the Pierce row
    /// outranks the requested prismatic row.
    /// </summary>
    [Fact]
    public void RequestedPrismaticElementIsDistinctFromItsPolicy()
    {
        VtankAmmunitionOption[] options =
        [
            new("Prism", 5, 0, 11, 200, 0, 0u, 0),
            new("Pierce", 5, 0, 0, 2000, 0, 0u, 0),
        ];
        VtankAmmunitionOption chosen = Assert.IsType<VtankAmmunitionOption>(
            VtankAmmunitionDatabase.Select(options, 5,
                MonsterDamageType.Prismatic,
                VtankPrismaticAmmoPolicy.ForcePrismatic, 0,
                new Character([]), static _ => true));
        Assert.Equal("Prism", chosen.Name);
    }

    private static PluginEquipmentItem Launcher(uint ammoType) => new(
        1u, "Launcher", 0x100u, 0x00100000u, 0u, 1u, 0u, 2,
        0, 47, 1, 0.1)
    {
        ObjectClass = PluginObjectClass.MissileWeapon,
        AmmoType = ammoType,
    };

    private sealed class Character(IReadOnlyList<PluginSkillInfo> skills)
        : ICharacterInfo
    {
        public bool IsInWorld => true;
        public uint ObjectId => 1u;
        public uint CurrentHealth => 100u;
        public uint MaxHealth => 100u;
        public uint CurrentStamina => 100u;
        public uint MaxStamina => 100u;
        public uint CurrentMana => 100u;
        public uint MaxMana => 100u;
        public IReadOnlyList<PluginSkillInfo> Skills => skills;
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo value in skills)
            {
                if (value.SkillId != skillId)
                    continue;
                skill = value;
                return true;
            }
            skill = default;
            return false;
        }
    }
}
