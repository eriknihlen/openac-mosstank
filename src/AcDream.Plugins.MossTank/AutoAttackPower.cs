using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal static class AutoAttackPower
{
    private const uint MeleeWeapon = 0x00000001u;
    private const uint MissileWeapon = 0x00000100u;

    /// <summary>The one equip slot a shield can go in.</summary>
    private const uint ShieldLocation = 0x00200000u;

    private const int SlashDamage = 0x0001;
    private const int PierceDamage = 0x0002;

    /// <summary>Both of the two physical damage types at once.</summary>
    private const int SlashAndPierce = SlashDamage | PierceDamage;

    /// <summary>The triple-slash bit of a weapon's attack types.</summary>
    private const int TripleSlashAttack = 0x0040;

    /// <summary>The weapon-type value an unarmed weapon carries.</summary>
    private const int UnarmedWeaponType = 1;

    private const uint RecklessnessSkill = 50u;

    /// <summary>
    /// How hard to charge the bar for one swing, or null to leave the bar
    /// wherever the player put it — which is what happens when the automatic
    /// power is switched off, because then nothing writes to the bar at all.
    /// </summary>
    public static float? Resolve(
        MonsterRuleActions actions,
        MonsterDamageType element,
        CombatSettings settings,
        ICharacterInfo character,
        IReadOnlyList<PluginInventoryItem> inventory)
    {
        ArgumentNullException.ThrowIfNull(actions);
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(inventory);
        if (!settings.AutoAttackPower)
            return null;

        PluginInventoryItem? weapon = FindWeapon(actions, inventory);
        float power = 1f;
        if (weapon is { } selected && (selected.ItemType & MeleeWeapon) != 0u)
        {
            PluginInventoryItem? offhand = FindOffhand(actions, selected, inventory);
            bool offhandMelee = offhand is { } held
                && (held.ItemType & MeleeWeapon) != 0u;
            bool offhandShield = offhand is { } shield
                && shield.ValidLocations == ShieldLocation;

            // The weapon strikes with slash AND pierce, so the swing's element
            // is the one the player chooses with the power bar.
            bool dualEdged = (selected.DamageType & SlashAndPierce)
                == SlashAndPierce;
            bool tripleSlash = (selected.AttackType & TripleSlashAttack) != 0;

            power = selected.WeaponType == UnarmedWeaponType && !offhandMelee
                ? element != MonsterDamageType.Slash || !dualEdged ? 0f : 0.5f
                : element == MonsterDamageType.Pierce && dualEdged && !tripleSlash
                    ? 0.2f
                    : element == MonsterDamageType.Pierce
                        && dualEdged
                        && tripleSlash
                        && offhandMelee
                        ? 0.49f
                        : element != MonsterDamageType.Pierce
                            || !dualEdged
                            || !tripleSlash
                            || offhandShield
                            ? 1f
                            : 0.2f;
        }

        return ClampForRecklessness(power, settings, character);
    }

    private static PluginInventoryItem? FindWeapon(
        MonsterRuleActions actions,
        IReadOnlyList<PluginInventoryItem> inventory)
    {
        PluginInventoryItem? equipped = null;
        PluginInventoryItem? named = null;
        foreach (PluginInventoryItem item in inventory)
        {
            if (actions.WeaponObjectId != 0u
                && item.ObjectId == actions.WeaponObjectId)
            {
                return item;
            }
            if (!string.IsNullOrWhiteSpace(actions.WeaponName)
                && item.Name.Equals(actions.WeaponName, StringComparison.Ordinal))
            {
                named ??= item;
            }
            if (item.IsEquipped
                && (item.ItemType & (MeleeWeapon | MissileWeapon)) != 0u)
            {
                equipped ??= item;
            }
        }
        return named ?? equipped;
    }

    private static PluginInventoryItem? FindOffhand(
        MonsterRuleActions actions,
        PluginInventoryItem weapon,
        IReadOnlyList<PluginInventoryItem> inventory)
    {
        PluginInventoryItem? equipped = null;
        PluginInventoryItem? named = null;
        foreach (PluginInventoryItem item in inventory)
        {
            if (item.ObjectId == weapon.ObjectId)
                continue;
            if (actions.OffhandObjectId != 0u
                && item.ObjectId == actions.OffhandObjectId)
            {
                return item;
            }
            if (!string.IsNullOrWhiteSpace(actions.OffhandName)
                && item.Name.Equals(actions.OffhandName, StringComparison.Ordinal))
            {
                named ??= item;
            }
            if (item.IsEquipped
                && ((item.ItemType & MeleeWeapon) != 0u
                    || item.ValidLocations == ShieldLocation))
            {
                equipped ??= item;
            }
        }
        return named ?? equipped;
    }

    private static float ClampForRecklessness(
        float power,
        CombatSettings settings,
        ICharacterInfo character)
    {
        if (!settings.UseRecklessness
            || !character.TryGetSkill(
                RecklessnessSkill,
                out PluginSkillInfo recklessness)
            || recklessness.Training is not (
                PluginSkillTraining.Trained or PluginSkillTraining.Specialized))
        {
            return power;
        }
        return Math.Clamp(power, 0.11f, 0.9f);
    }
}
