using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankWeaponLadderTests
{
    /// <summary>
    /// Mutation: score the weapons by the damage they deal again and this
    /// fails — the plain sword hits harder, but the imbued one is the one the
    /// fight wants.
    /// </summary>
    [Fact]
    public void AnImbuedWeaponOfTheWantedElementBeatsAPlainerHarderOne()
    {
        PluginEquipmentItem plain = Weapon(1, "Heavy Sword", damageType: 0x0001)
            with
        { Damage = 90 };
        PluginEquipmentItem imbued = Weapon(2, "Fire Rending Dagger", damageType: 0x0002)
            with
        { Damage = 20, ImbuedEffect = 0x0200 };

        // The plain sword is listed last on purpose: it is what a ladder that
        // could not see the imbue would fall back to.
        Assert.Equal(2u, Pick([imbued, plain], MonsterDamageType.Fire));
    }

    /// <summary>
    /// Mutation: read only the weapon's plain damage and this fails — the
    /// element this weapon brings comes from what it cleaves, nothing else.
    /// </summary>
    [Fact]
    public void AWeaponWhoseElementComesOnlyFromACleaveStillQualifies()
    {
        PluginEquipmentItem plain = Weapon(1, "Heavy Sword", damageType: 0x0001)
            with
        { Damage = 90 };
        PluginEquipmentItem cleaver = Weapon(2, "Acid Cleaver", damageType: 0x0001)
            with
        { Damage = 20, ResistanceCleaving = 0x0020 };

        Assert.Equal(2u, Pick([cleaver, plain], MonsterDamageType.Acid));
    }

    /// <summary>
    /// Mutation: drop the slayer rung and this fails — a slayer for what is
    /// being fought outranks the weapon with the wanted element.
    /// </summary>
    [Fact]
    public void ASlayerForTheSpeciesBeingFoughtWinsOutright()
    {
        PluginEquipmentItem fire = Weapon(1, "Fire Rending Dagger", damageType: 0x0010)
            with
        { ImbuedEffect = 0x0200 };
        PluginEquipmentItem slayer = Weapon(2, "Olthoi Slayer", damageType: 0x0001)
            with
        { SlayerCreatureType = 12 };

        Assert.Equal(
            2u,
            VtankWeaponLadder.Select(
                [fire, slayer],
                static _ => true,
                [MonsterDamageType.Fire],
                species: 12,
                static (_, _) => true,
                static _ => false));
        // Fighting something else, the slayer means nothing.
        Assert.Equal(
            1u,
            VtankWeaponLadder.Select(
                [fire, slayer],
                static _ => true,
                [MonsterDamageType.Fire],
                species: 25,
                static (_, _) => true,
                static _ => false));
    }

    /// <summary>
    /// Mutation: change any of the six rating weights and this fails. Armour
    /// rending is worth more than everything else put together.
    /// </summary>
    [Fact]
    public void TheRatingWeightsAreTheOnesTheReferenceMacroUses()
    {
        Assert.Equal(
            0,
            VtankWeaponLadder.RatingOf(Weapon(1, "Plain", damageType: 0x0001)));
        Assert.Equal(
            4000,
            VtankWeaponLadder.RatingOf(
                Weapon(1, "Rending", damageType: 0x0001) with
                { ImbuedEffect = 0x0004 }));
        Assert.Equal(
            2000,
            VtankWeaponLadder.RatingOf(
                Weapon(1, "Critical", damageType: 0x0001) with
                { ImbuedEffect = 0x0001 }));
        Assert.Equal(
            1000,
            VtankWeaponLadder.RatingOf(
                Weapon(1, "Crippling", damageType: 0x0001) with
                { ImbuedEffect = 0x0002 }));
        Assert.Equal(
            700,
            VtankWeaponLadder.RatingOf(
                Weapon(1, "Quest", damageType: 0x0001) with
                {
                    ArmorCleaving = true,
                    CrushingBlow = true,
                    BitingStrike = true,
                }));
    }

    /// <summary>
    /// Mutation: drop the "already vulnerable" rung and this fails — with the
    /// monster already carrying the fire vulnerability, the best-rated fire
    /// weapon is taken ahead of the one whose imbue merely names fire.
    /// </summary>
    [Fact]
    public void AnElementTheMonsterAlreadyCarriesTakesTheBestRatedWeapon()
    {
        PluginEquipmentItem imbued = Weapon(1, "Fire Rending Dagger", damageType: 0x0002)
            with
        { ImbuedEffect = 0x0200 };
        PluginEquipmentItem rated = Weapon(2, "Rending Axe", damageType: 0x0010)
            with
        { ImbuedEffect = 0x0004 };

        Assert.Equal(
            2u,
            VtankWeaponLadder.Select(
                [imbued, rated],
                static _ => true,
                [MonsterDamageType.Fire],
                species: -1,
                static (_, _) => true,
                static element => element == MonsterDamageType.Fire));
        Assert.Equal(
            1u,
            VtankWeaponLadder.Select(
                [imbued, rated],
                static _ => true,
                [MonsterDamageType.Fire],
                species: -1,
                static (_, _) => true,
                static _ => false));
    }

    /// <summary>
    /// Mutation: drop the profile filter and this fails — only what is on the
    /// Items page may be reached for.
    /// </summary>
    [Fact]
    public void OnlyProfiledWeaponsAreConsidered()
    {
        PluginEquipmentItem profiled = Weapon(1, "Listed", damageType: 0x0010);
        PluginEquipmentItem stranger = Weapon(2, "Unlisted", damageType: 0x0010)
            with
        { ImbuedEffect = 0x0200 };

        Assert.Equal(
            1u,
            VtankWeaponLadder.Select(
                [profiled, stranger],
                static item => item.Name == "Listed",
                [MonsterDamageType.Fire],
                species: -1,
                static (_, _) => true,
                static _ => false));
    }

    private static uint Pick(
        IReadOnlyList<PluginEquipmentItem> items,
        MonsterDamageType wanted) => VtankWeaponLadder.Select(
            items,
            static _ => true,
            [wanted],
            species: -1,
            static (_, _) => true,
            static _ => false);

    private static PluginEquipmentItem Weapon(
        uint id,
        string name,
        int damageType) => new(
            id,
            name,
            ItemType: 0x1,
            ValidLocations: 0x00100000,
            EquippedLocation: 0,
            ContainerObjectId: 1,
            WielderObjectId: 0,
            CombatUse: 1,
            DamageType: damageType,
            WeaponSkill: 44,
            Damage: 20,
            DamageVariance: 0.25);
}
