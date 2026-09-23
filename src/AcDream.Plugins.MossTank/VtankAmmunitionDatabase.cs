using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum VtankPrismaticAmmoPolicy
{
    Any,
    NoPrismatic,
    ForcePrismatic,
}

internal readonly record struct VtankAmmunitionOption(
    string Name,
    int LauncherType,
    int WieldRequirement,
    int Element,
    int Quality,
    int SpecialMask,
    uint SecondarySkill,
    int SecondaryRequirement);

internal static class VtankAmmunitionDatabase
{
    internal enum MissileKind
    {
        None = 0,
        Bow = 5,
        Crossbow = 6,
        Atlatl = 7,
        Thrown = 8,
    }

    public static MissileKind Kind(in PluginEquipmentItem item) =>
        item.ObjectClass != PluginObjectClass.MissileWeapon
            ? MissileKind.None
            : item.AmmoType switch
            {
                1u => MissileKind.Bow,
                2u => MissileKind.Crossbow,
                4u => MissileKind.Atlatl,
                0u => MissileKind.Thrown,
                _ => MissileKind.None,
            };

    public static int LauncherType(in PluginEquipmentItem item) =>
        Kind(in item) is MissileKind.Bow or MissileKind.Crossbow
            or MissileKind.Atlatl ? (int)Kind(in item) : 0;

    /// <summary>
    /// The best row of the game database's AmmunitionOptions table for this
    /// launcher and element that the character can wield and has (or can
    /// make). With no table there is no choice.
    /// </summary>
    public static VtankAmmunitionOption? Select(
        IReadOnlyList<VtankAmmunitionOption> options,
        int launcherType,
        MonsterDamageType damage,
        VtankPrismaticAmmoPolicy prismatic,
        int enabledSpecialMask,
        ICharacterInfo character,
        Func<string, bool> isAvailable)
    {
        ArgumentNullException.ThrowIfNull(options);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(isAvailable);
        int desiredElement = Element(damage);
        if (launcherType == 0)
            return null;

        VtankAmmunitionOption? best = null;
        int bestQuality = int.MinValue;
        foreach (VtankAmmunitionOption option in options)
        {
            if (option.LauncherType != launcherType)
                continue;
            int optionElement = option.Element == 100 ? 11 : option.Element;
            int quality = option.Quality;
            if (prismatic == VtankPrismaticAmmoPolicy.ForcePrismatic
                && optionElement != 11)
            {
                quality -= 1000;
            }
            if (optionElement != desiredElement)
            {
                if (optionElement != 11)
                    continue;
                if (prismatic == VtankPrismaticAmmoPolicy.NoPrismatic)
                    quality -= 1000;
            }
            if (quality < bestQuality
                || !MeetsRequirements(option, character)
                || (option.SpecialMask != 0
                    && (option.SpecialMask & enabledSpecialMask) == 0)
                || !isAvailable(option.Name))
            {
                continue;
            }
            bestQuality = quality;
            best = option;
        }
        return best;
    }

    private static bool MeetsRequirements(
        in VtankAmmunitionOption option,
        ICharacterInfo character)
    {
        if (option.WieldRequirement > 0)
        {
            if (!character.TryGetSkill(47u, out PluginSkillInfo missile)
                || missile.Training is PluginSkillTraining.Unknown
                    or PluginSkillTraining.Untrained
                || missile.Base < option.WieldRequirement)
            {
                return false;
            }
        }
        if (option.SecondarySkill == 0u || option.SecondaryRequirement == 0)
            return true;
        return character.TryGetSkill(
                option.SecondarySkill,
                out PluginSkillInfo secondary)
            && secondary.Training != PluginSkillTraining.Untrained
            && secondary.Current >= option.SecondaryRequirement;
    }

    private static int Element(MonsterDamageType damage) => damage switch
    {
        MonsterDamageType.Pierce => 0,
        MonsterDamageType.Bludgeon => 1,
        MonsterDamageType.Slash => 2,
        MonsterDamageType.Acid => 3,
        MonsterDamageType.Electric => 4,
        MonsterDamageType.Cold => 5,
        MonsterDamageType.Fire => 6,
        MonsterDamageType.Harm => 7,
        MonsterDamageType.Auto => 8,
        MonsterDamageType.VoidBasic => 9,
        MonsterDamageType.DrainAuto => 10,
        MonsterDamageType.Prismatic => 11,
        MonsterDamageType.Random => 12,
        MonsterDamageType.Fists => 13,
        MonsterDamageType.None => 98,
        MonsterDamageType.Physical => 99,
        MonsterDamageType.PlayerAuto => 101,
        _ => -1,
    };
}
