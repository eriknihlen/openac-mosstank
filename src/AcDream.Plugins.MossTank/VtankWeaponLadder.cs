using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Which weapon a rule that names none reaches for. Not "the biggest number":
/// a slayer for what is being fought wins outright, then a weapon whose
/// element the monster is already vulnerable to, then one whose imbue or
/// cleave gives it the right element, then the best-rated one, and only at the
/// very end anything at all that can be wielded.
/// </summary>
internal static class VtankWeaponLadder
{
    /// <summary>Only these slots hold something to fight with.</summary>
    private const uint WeaponReadyMask = 0x03500000u;

    /// <summary>What each rating is worth when weapons are compared by rating.</summary>
    private const int ArmorRendScore = 4000;
    private const int CriticalStrikeScore = 2000;
    private const int CripplingBlowScore = 1000;
    private const int ArmorCleavingScore = 400;
    private const int CrushingBlowScore = 200;
    private const int BitingStrikeScore = 100;

    private const int ArmorRendBit = 0x0004;
    private const int CriticalStrikeBit = 0x0001;
    private const int CripplingBlowBit = 0x0002;

    /// <summary>
    /// The rating a weapon's imbue and quest bonuses add up to. Zero means it
    /// brings nothing beyond its element.
    /// </summary>
    public static int RatingOf(in PluginEquipmentItem item)
    {
        int score = 0;
        if ((item.ImbuedEffect & ArmorRendBit) != 0)
            score += ArmorRendScore;
        if ((item.ImbuedEffect & CriticalStrikeBit) != 0)
            score += CriticalStrikeScore;
        if ((item.ImbuedEffect & CripplingBlowBit) != 0)
            score += CripplingBlowScore;
        if (item.ArmorCleaving)
            score += ArmorCleavingScore;
        if (item.CrushingBlow)
            score += CrushingBlowScore;
        if (item.BitingStrike)
            score += BitingStrikeScore;
        return score;
    }

    /// <summary>
    /// Picks a weapon out of the profiled ones.
    /// </summary>
    /// <param name="items">Everything the character owns.</param>
    /// <param name="isProfiled">Whether an item is on the Items page.</param>
    /// <param name="wanted">
    /// The elements the fight wants, best first: the monster's weaknesses for
    /// an automatic rule, or the single element the rule spells out.
    /// </param>
    /// <param name="species">The monster's species, or -1 when unknown.</param>
    /// <param name="canDeliver">
    /// Whether a weapon of this kind can put this element on the monster —
    /// only a launcher can answer no, and only for want of ammunition.
    /// </param>
    /// <param name="alreadyVulnerable">
    /// Whether the monster is already carrying that element's vulnerability.
    /// </param>
    public static uint Select(
        IReadOnlyList<PluginEquipmentItem> items,
        Func<PluginEquipmentItem, bool> isProfiled,
        IReadOnlyList<MonsterDamageType> wanted,
        int species,
        Func<PluginEquipmentItem, MonsterDamageType, bool> canDeliver,
        Func<MonsterDamageType, bool> alreadyVulnerable)
    {
        ArgumentNullException.ThrowIfNull(items);
        ArgumentNullException.ThrowIfNull(isProfiled);
        ArgumentNullException.ThrowIfNull(wanted);
        ArgumentNullException.ThrowIfNull(canDeliver);
        ArgumentNullException.ThrowIfNull(alreadyVulnerable);

        // Weapons whose element comes from an imbue or a cleave, and nothing
        // else. This map is asked first because the element is deliberate.
        var byImbue = new Dictionary<MonsterDamageType, uint>();
        var imbueOrder = new List<MonsterDamageType>();

        // The best-rated weapon per element.
        var byRating = new Dictionary<MonsterDamageType, uint>();
        var ratingOf = new Dictionary<MonsterDamageType, int>();
        var ratingOrder = new List<MonsterDamageType>();

        // Every weapon by whatever element it ends up striking with.
        var byElement = new Dictionary<MonsterDamageType, uint>();
        var elementOrder = new List<MonsterDamageType>();

        uint slayer = 0u;
        uint casterFallback = 0u;
        uint anyLauncher = 0u;
        uint last = 0u;

        foreach (PluginEquipmentItem item in items)
        {
            if (!isProfiled(item)
                || (item.ValidLocations & WeaponReadyMask) == 0u)
            {
                continue;
            }
            PluginCombatMode stance = CombatModeGate.ModeFor(in item);
            bool launcher =
                VtankAmmunitionDatabase.LauncherType(item.AmmoType) != 0;
            bool caster = stance == PluginCombatMode.Magic;

            if (caster)
            {
                last = item.ObjectId;
                if (species >= 0 && item.SlayerCreatureType == species)
                    slayer = item.ObjectId;
            }
            if (launcher)
                anyLauncher = item.ObjectId;
            if (species >= 0 && item.SlayerCreatureType == species)
                slayer = item.ObjectId;

            MonsterDamageType imbued = VtankWeaponElement.Resolve(
                item.ImbuedEffect,
                item.ResistanceCleaving,
                damageType: 0);
            if (imbued != MonsterDamageType.None && canDeliver(item, imbued))
                Record(byImbue, imbueOrder, imbued, item.ObjectId);

            MonsterDamageType element = VtankWeaponElement.Resolve(
                item.ImbuedEffect,
                item.ResistanceCleaving,
                item.DamageType);
            bool elementless = element == MonsterDamageType.None;
            if (elementless && launcher)
                element = MonsterDamageType.Pierce;
            if (element != MonsterDamageType.None && canDeliver(item, element))
                Record(byElement, elementOrder, element, item.ObjectId);

            int rating = RatingOf(in item);
            if (rating > 0)
            {
                if (element != MonsterDamageType.None
                    && canDeliver(item, element)
                    && rating > ratingOf.GetValueOrDefault(element))
                {
                    byRating[element] = item.ObjectId;
                    ratingOf[element] = rating;
                    Record(byRating, ratingOrder, element, item.ObjectId);
                }
                if (launcher && elementless)
                {
                    // A plain bow shoots whatever the quiver holds, so a
                    // well-rated one stands in for every element it can feed.
                    foreach (MonsterDamageType candidate in wanted)
                    {
                        if (candidate == element
                            || !canDeliver(item, candidate)
                            || rating <= ratingOf.GetValueOrDefault(candidate))
                        {
                            continue;
                        }
                        byRating[candidate] = item.ObjectId;
                        ratingOf[candidate] = rating;
                        Record(byRating, ratingOrder, candidate, item.ObjectId);
                    }
                }
                if (caster && elementless)
                    casterFallback = item.ObjectId;
            }
            last = item.ObjectId;
        }

        if (slayer != 0u)
            return slayer;
        foreach (MonsterDamageType candidate in wanted)
        {
            if (alreadyVulnerable(candidate)
                && byRating.TryGetValue(candidate, out uint vulnerable))
            {
                return vulnerable;
            }
        }
        if (FirstCommon(wanted, imbueOrder) is { } imbueHit)
            return byImbue[imbueHit];
        if (byImbue.TryGetValue(MonsterDamageType.VoidBasic, out uint voidWeapon))
            return voidWeapon;
        if (FirstCommon(wanted, ratingOrder) is { } ratingHit)
            return byRating[ratingHit];
        if (casterFallback != 0u)
            return casterFallback;
        if (FirstCommon(wanted, elementOrder) is { } elementHit)
            return byElement[elementHit];
        return anyLauncher != 0u ? anyLauncher : last;
    }

    private static void Record(
        Dictionary<MonsterDamageType, uint> map,
        List<MonsterDamageType> order,
        MonsterDamageType element,
        uint objectId)
    {
        if (!map.ContainsKey(element))
            order.Add(element);
        map[element] = objectId;
    }

    /// <summary>
    /// The first wanted element the map also holds. Order follows what the
    /// fight wants, not what the pack happens to contain.
    /// </summary>
    private static MonsterDamageType? FirstCommon(
        IReadOnlyList<MonsterDamageType> wanted,
        List<MonsterDamageType> available)
    {
        foreach (MonsterDamageType candidate in wanted)
        {
            if (available.Contains(candidate))
                return candidate;
        }
        return null;
    }
}
