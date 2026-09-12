namespace AcDream.Plugins.MossTank;

/// <summary>
/// Which element a weapon actually strikes with. Three things can decide it,
/// and they are asked in this order: the imbue burned into the weapon, the
/// resistance it cleaves, and finally the damage it plainly deals.
/// </summary>
internal static class VtankWeaponElement
{
    // The imbue bits that name an element, plus the two critical bonuses that
    // share the same field and must be considered alongside them.
    private const int CriticalStrike = 0x0001;
    private const int CripplingBlow = 0x0002;
    private const int ArmorRend = 0x0004;
    private const int SlashRend = 0x0008;
    private const int PierceRend = 0x0010;
    private const int BludgeonRend = 0x0020;
    private const int AcidRend = 0x0040;
    private const int ColdRend = 0x0080;
    private const int LightningRend = 0x0100;
    private const int FireRend = 0x0200;

    private const int RendMask = CriticalStrike | CripplingBlow | ArmorRend
        | SlashRend | PierceRend | BludgeonRend | AcidRend | ColdRend
        | LightningRend | FireRend;

    // Damage-type bits, shared by the cleaving field and the damage field.
    private const int Slash = 0x0001;
    private const int Pierce = 0x0002;
    private const int Bludgeon = 0x0004;
    private const int Cold = 0x0008;
    private const int Fire = 0x0010;
    private const int Acid = 0x0020;
    private const int Lightning = 0x0040;
    private const int Nether = 0x0400;

    /// <summary>
    /// The element, or <see cref="MonsterDamageType.None"/> when none of the
    /// three says anything.
    /// </summary>
    public static MonsterDamageType Resolve(
        int imbuedEffect,
        int resistanceCleaving,
        int damageType)
    {
        // An exact match: a weapon carrying two rends, or a rend alongside a
        // critical bonus, names no single element and falls through.
        switch (imbuedEffect & RendMask)
        {
            case AcidRend: return MonsterDamageType.Acid;
            case BludgeonRend: return MonsterDamageType.Bludgeon;
            case ColdRend: return MonsterDamageType.Cold;
            case FireRend: return MonsterDamageType.Fire;
            case LightningRend: return MonsterDamageType.Electric;
            case PierceRend: return MonsterDamageType.Pierce;
            case SlashRend: return MonsterDamageType.Slash;
            default: break;
        }

        if ((resistanceCleaving & Acid) != 0) return MonsterDamageType.Acid;
        if ((resistanceCleaving & Bludgeon) != 0) return MonsterDamageType.Bludgeon;
        if ((resistanceCleaving & Cold) != 0) return MonsterDamageType.Cold;
        if ((resistanceCleaving & Fire) != 0) return MonsterDamageType.Fire;
        if ((resistanceCleaving & Lightning) != 0) return MonsterDamageType.Electric;
        if ((resistanceCleaving & Pierce) != 0) return MonsterDamageType.Pierce;
        if ((resistanceCleaving & Slash) != 0) return MonsterDamageType.Slash;

        if ((damageType & Acid) != 0) return MonsterDamageType.Acid;
        if ((damageType & Bludgeon) != 0) return MonsterDamageType.Bludgeon;
        if ((damageType & Cold) != 0) return MonsterDamageType.Cold;
        if ((damageType & Fire) != 0) return MonsterDamageType.Fire;
        if ((damageType & Lightning) != 0) return MonsterDamageType.Electric;
        if ((damageType & Slash) != 0) return MonsterDamageType.Slash;
        if ((damageType & Pierce) != 0) return MonsterDamageType.Pierce;
        if ((damageType & Nether) != 0) return MonsterDamageType.VoidBasic;
        return MonsterDamageType.None;
    }
}
