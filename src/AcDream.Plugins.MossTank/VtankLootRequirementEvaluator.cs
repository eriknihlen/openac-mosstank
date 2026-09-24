using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Executes VTClassic's typed loot requirements as an AND set.</summary>
internal static class VtankLootRequirementEvaluator
{
    private const uint VtankIntBase = 218_103_808u;
    private const uint VtankDoubleBase = 167_772_160u;
    private const uint PluralNameKey = 184_549_376u;

    /// <summary>The item key whose bit 0 is the "magical" icon highlight.</summary>
    private const uint IconHighlightKey = VtankIntBase + 16;

    private static readonly IReadOnlyDictionary<uint, (uint Key, int Bonus)>
        IntSpellBonuses = new Dictionary<uint, (uint, int)>
        {
            [2598] = (VtankIntBase + 34, 2),
            [2586] = (VtankIntBase + 34, 4),
            [4661] = (VtankIntBase + 34, 7),
            [6089] = (VtankIntBase + 34, 10),
            [2604] = (28, 20),
            [2592] = (28, 40),
            [4667] = (28, 60),
            [6095] = (28, 80),
        };

    // Change and Bonus are two separate authored fields. Only Bonus is ever
    // applied; Change truncated to an int selects the operation, so anything
    // in [1, 2) multiplies and everything else adds.
    private static readonly IReadOnlyDictionary<uint, (uint Key, double Change, double Bonus)>
        DoubleSpellBonuses = new Dictionary<uint, (uint, double, double)>
        {
            [3251] = (152, .01, .01), [3250] = (152, .03, .03),
            [4670] = (152, .05, .05), [6098] = (152, .07, .07),
            [2603] = (VtankDoubleBase + 12, .03, .03),
            [2591] = (VtankDoubleBase + 12, .05, .05),
            [4666] = (VtankDoubleBase + 12, .07, .07),
            [6094] = (VtankDoubleBase + 12, .09, .09),
            [2600] = (29, .03, .03), [3985] = (29, .04, .04),
            [2588] = (29, .05, .05), [4663] = (29, .07, .07), [6091] = (29, .09, .09),
            [3201] = (144, 1.05, 1.05), [3199] = (144, 1.10, 1.10),
            [3202] = (144, 1.15, 1.15), [3200] = (144, 1.20, 1.20),
            [6086] = (144, 1.25, 1.25), [6087] = (144, 1.30, 1.30),
        };

    /// <summary>
    /// The int keys whose value the server sends with the object itself; every
    /// other key only arrives with an appraisal.
    /// </summary>
    private static readonly HashSet<uint> NonIdentifiedIntKeys =
    [
        10u, 19u, 91u, 92u, 131u,
        VtankIntBase + 0, VtankIntBase + 1, VtankIntBase + 2,
        VtankIntBase + 4, VtankIntBase + 5, VtankIntBase + 6,
        VtankIntBase + 7, VtankIntBase + 9, VtankIntBase + 12,
        VtankIntBase + 13, VtankIntBase + 14, VtankIntBase + 15,
        VtankIntBase + 16, VtankIntBase + 17, VtankIntBase + 18,
        VtankIntBase + 20, VtankIntBase + 24, VtankIntBase + 25,
        VtankIntBase + 26, VtankIntBase + 27, VtankIntBase + 35,
        VtankIntBase + 36, VtankIntBase + 37, VtankIntBase + 38,
        VtankIntBase + 41, VtankIntBase + 42,
    ];

    internal static bool IsIdentifiedIntKey(uint key) =>
        !NonIdentifiedIntKeys.Contains(key);

    internal static bool IsIdentifiedStringKey(uint key) =>
        key != 1u && key != PluralNameKey;

    internal static bool IsIdentifiedDoubleKey(uint key) =>
        key != VtankDoubleBase + 8 && key != VtankDoubleBase + 9;

    /// <summary>
    /// True when the server has flagged the item as magical, which is the only
    /// pre-appraisal signal that it is expected to carry spells.
    /// </summary>
    internal static bool IsMagical(
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        (IntValue(IconHighlightKey, item, properties) & 1) != 0;

    /// <summary>
    /// Answers "could this requirement decide, without appraisal data?" and,
    /// when it can, whether it matches. A requirement that cannot decide
    /// leaves <paramref name="hasDecision"/> false so the rule stays open.
    /// </summary>
    public static void EarlyMatch(
        VtankLootRequirement requirement,
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IPluginHost? host,
        out bool hasDecision,
        out bool isMatch)
    {
        hasDecision = true;
        isMatch = false;
        try
        {
            string[] values = Lines(requirement.Payload);
            switch (requirement.Type)
            {
                // Spell requirements: undecided only while the item claims to
                // be magical, because then spell data is still to come.
                case 0:
                case 8:
                case 9:
                    hasDecision = !IsMagical(item, properties);
                    return;
                // Weapon-damage requirements: undecided only for the weapon
                // class the requirement can apply to.
                case 10:
                case 2008:
                    hasDecision =
                        item.ObjectClass != PluginObjectClass.MeleeWeapon;
                    return;
                case 2000:
                case 2006:
                    hasDecision =
                        item.ObjectClass != PluginObjectClass.MeleeWeapon
                        && item.ObjectClass != PluginObjectClass.MissileWeapon;
                    return;
                case 2001:
                    hasDecision =
                        item.ObjectClass != PluginObjectClass.MissileWeapon;
                    return;
                // Buffed and rating requirements can never decide early.
                case 2003:
                case 2005:
                case 2007:
                    hasDecision = false;
                    return;
                // Retired, disabled, and preserved-unknown requirements are a
                // decided non-match whatever the item is.
                case 6:
                case 9999:
                case -1:
                    return;
                case 1:
                    hasDecision = !IsIdentifiedStringKey(U32(values, 1));
                    break;
                case 2:
                case 3:
                case 11:
                case 12:
                case 13:
                    hasDecision = !IsIdentifiedIntKey(U32(values, 1));
                    break;
                case 4:
                case 5:
                    hasDecision = !IsIdentifiedDoubleKey(U32(values, 1));
                    break;
                default:
                    break;
            }
            isMatch = hasDecision
                && IsMatch(requirement, item, properties, host);
        }
        catch (Exception failure) when (
            failure is FormatException or ArgumentException or OverflowException)
        {
            hasDecision = true;
            isMatch = false;
        }
    }

    private static readonly IReadOnlyDictionary<string, int[]> ArmorColorSlots =
        new Dictionary<string, int[]>(StringComparer.Ordinal)
        {
            ["Amuli Coat (Chest)"] = [0],
            ["Amuli Coat (Collar/Shoulder)"] = [1, 2],
            ["Amuli Coat (Arms/Trim)"] = [3, 4, 5, 6, 7],
            ["Amuli Legs (Base)"] = [0, 1],
            ["Amuli Legs (Trim)"] = [2, 3],
            ["Celdon (Base)"] = [0],
            ["Celdon (Veins)"] = [1, 2],
            ["Chiran Coat (Base/Arms)"] = [0, 1],
            ["Chiran Coat (Stripes)"] = [2, 3, 4],
            ["Chiran Legs (Girth)"] = [1],
            ["Chiran Legs (Legs)"] = [2, 3],
            ["Chiran Legs (Trim)"] = [0],
            ["Chiran Helm (Horns)"] = [0],
            ["Chiran Helm (Base)"] = [1],
            ["Haebrean BP (Chest) *"] = [0],
            ["Haebrean BP (Ornaments)"] = [1],
            ["Haebrean BP (Trim)"] = [2],
            ["Haebrean Girth (Base) *"] = [0],
            ["Haebrean Girth (Belt/Scales)"] = [1, 2],
            ["Haebrean Helm (Base)"] = [0],
            ["Haebrean Helm (Mask)"] = [1],
            ["Haebrean Pauldrons (Base) *"] = [0],
            ["Haebrean Pauldrons (Ornaments)"] = [1],
            ["Lorica BP (Veins)"] = [0, 1],
            ["Lorica BP (Base)"] = [2, 3],
            ["Lorica BP (Neck/Trim) *"] = [4],
            ["Lorica Legs (Base)"] = [0],
            ["Lorica Legs (Knees/Belt/Crotch) *"] = [1, 2],
            ["Lorica Legs (Legs) *"] = [3],
            ["Nariyid BP (Circle/Lines)"] = [0, 1],
            ["Nariyid BP (Base)"] = [2],
            ["Nariyid BP (Shoulders)"] = [3],
            ["Nariyid Girth (Base) *"] = [0],
            ["Nariyid Girth (Belt/Lines)"] = [2],
            ["Nariyid Girth (Ornaments)"] = [3],
            ["Nariyid Sleeves (Shoulders)"] = [0],
            ["Nariyid Sleeves (Upper Arm)"] = [1, 2],
            ["Nariyid Sleeves (Lower Arm)"] = [3],
            ["Olthoi BP (Base)"] = [0],
            ["Olthoi BP (Veins)"] = [1],
            ["Olthoi Alduressa Legs (Girth: Base)"] = [0, 1, 2],
            ["Olthoi Alduressa Legs (Girth: Lines)"] = [3],
            ["Olthoi Alduressa Legs (Legs: Lines)"] = [4, 5],
            ["Olthoi Amuli Coat (Base) *"] = [0, 1],
            ["Olthoi Amuli Coat (Trim)"] = [2],
            ["Olthoi Amuli Coat (Shoulders)"] = [3],
            ["Olthoi Amuli Legs (Trim)"] = [6, 7, 8],
            ["Olthoi Koujia Kabuton (Base)"] = [0],
            ["Olthoi Koujia Kabuton (Horns)"] = [1],
            ["Olthoi Koujia Legs (Base)"] = [0, 1, 2],
            ["Olthoi Koujia Legs (Sides/Shins)"] = [3, 4, 5],
            ["Scalemail Cuirass (Base)"] = [0],
            ["Scalemail Cuirass (Bumps)"] = [1],
            ["Scalemail Cuirass (Belt)"] = [2],
            ["Tenassa Legs (Line at Side)"] = [0],
            ["Tenassa Legs (Base)"] = [1],
            ["Tenassa Legs (Hilight)"] = [2],
            ["Tenassa BP (Shoulders)"] = [0],
            ["Tenassa BP (Base)"] = [1],
            ["Yoroi Cuirass (Base)"] = [0, 1],
            ["Yoroi Cuirass (Belt)"] = [2],
            ["Yoroi Girth (Base)"] = [0],
            ["Yoroi Girth (Belt)"] = [1],
        };

    public static bool IsMatch(
        IReadOnlyList<VtankLootRequirement> requirements,
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IPluginHost? host,
        out string? error)
    {
        try
        {
            foreach (VtankLootRequirement requirement in requirements)
            {
                if (!IsMatch(requirement, item, properties, host))
                {
                    error = null;
                    return false;
                }
            }
            error = null;
            return true;
        }
        catch (Exception failure) when (
            failure is FormatException or ArgumentException or OverflowException)
        {
            error = failure.Message;
            return false;
        }
    }

    private static bool IsMatch(
        VtankLootRequirement requirement,
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IPluginHost? host)
    {
        string[] values = Lines(requirement.Payload);
        return requirement.Type switch
        {
            0 => SpellNames(item, host).Any(name => Rx(values, 0).IsMatch(name)),
            1 => Rx(values, 0).IsMatch(StringValue(
                U32(values, 1), item, properties)),
            2 => IntValue(U32(values, 1), item, properties) <= I32(values, 0),
            3 => IntValue(U32(values, 1), item, properties) >= I32(values, 0),
            4 => (float)DoubleValue(U32(values, 1), item, properties)
                <= (float)F64(values, 0),
            5 => (float)DoubleValue(U32(values, 1), item, properties)
                >= (float)F64(values, 0),
            // VTClassic deliberately retired this requirement; its own Match
            // method always returns false.
            6 => false,
            7 => (int)item.ObjectClass == I32(values, 0),
            8 => item.AppraisedSpellIds.Count >= I32(values, 0),
            9 => SpellMatch(values, item, host),
            10 => MinimumDamage(item) >= F64(values, 0),
            11 => (IntValue(U32(values, 1), item, properties)
                & I32(values, 0)) > 0,
            12 => IntValue(U32(values, 1), item, properties) == I32(values, 0),
            13 => IntValue(U32(values, 1), item, properties) != I32(values, 0),
            14 => ColorMatch(values, item.Palettes),
            15 => ArmorColorMatch(values, item.Palettes),
            16 => SlotColorMatch(values, item.Palettes),
            17 => ExactPalette(values, item.Palettes),
            1000 => CharacterSkill(host, U32(values, 1), buffed: true)
                >= I32(values, 0),
            1001 => (host?.Automation.Character.MainPackFreeSlots ?? 0)
                >= I32(values, 0),
            1002 => (host?.Automation.Character.Level ?? 0) >= I32(values, 0),
            1003 => (host?.Automation.Character.Level ?? 0) <= I32(values, 0),
            1004 => CharacterBaseSkillRange(values, host),
            2000 => BuffedMedianDamage(item, properties) >= F64(values, 0),
            2001 => BuffedMissileDamage(item, properties) >= F64(values, 0),
            2003 => BuffedInt(
                U32(values, 1), item, properties) >= F64(values, 0),
            2005 => (float)BuffedDouble(
                U32(values, 1), item, properties) >= (float)F64(values, 0),
            2006 => BuffedTinkedDamage(item, properties) >= F64(values, 0),
            2007 => TotalRatings(item, properties) >= F64(values, 0),
            2008 => CanReachTarget(values, item, properties),
            9999 => !Bool(values, 0),
            _ => false,
        };
    }

    private static bool SpellMatch(
        string[] values,
        in PluginInventoryItem item,
        IPluginHost? host)
    {
        Regex include = Rx(values, 0);
        Regex exclude = Rx(values, 1);
        bool excludeEmpty = Value(values, 1).Trim().Length == 0;
        int required = I32(values, 2);
        int count = 0;
        foreach (string name in SpellNames(item, host))
        {
            if (include.IsMatch(name)
                && (excludeEmpty || !exclude.IsMatch(name))
                && ++count >= required)
            {
                return true;
            }
        }
        return false;
    }

    private static bool ColorMatch(
        string[] values,
        IReadOnlyList<PluginPaletteInfo> palettes)
    {
        for (int index = 0; index < palettes.Count; index++)
        {
            if (SimilarColor(values, palettes[index]))
                return true;
        }
        return false;
    }

    private static bool ArmorColorMatch(
        string[] values,
        IReadOnlyList<PluginPaletteInfo> palettes)
    {
        if (!ArmorColorSlots.TryGetValue(Value(values, 5), out int[]? slots))
            return false;
        foreach (int slot in slots)
        {
            if (slot >= 0 && slot < palettes.Count
                && SimilarColor(values, palettes[slot]))
            {
                return true;
            }
        }
        return false;
    }

    private static bool SlotColorMatch(
        string[] values,
        IReadOnlyList<PluginPaletteInfo> palettes)
    {
        int slot = I32(values, 5);
        return slot >= 0 && slot < palettes.Count
            && SimilarColor(values, palettes[slot]);
    }

    private static bool ExactPalette(
        string[] values,
        IReadOnlyList<PluginPaletteInfo> palettes)
    {
        int slot = I32(values, 0);
        uint expected = U32(values, 1) & 0x00FF_FFFFu;
        return slot >= 0 && slot < palettes.Count
            && (palettes[slot].PaletteId & 0x00FF_FFFFu) == expected;
    }

    private static bool SimilarColor(
        string[] values,
        in PluginPaletteInfo palette)
    {
        Hsv(
            checked((byte)I32(values, 0)),
            checked((byte)I32(values, 1)),
            checked((byte)I32(values, 2)),
            out double targetHue,
            out double targetSaturation,
            out double targetValue);
        Hsv(
            palette.Red,
            palette.Green,
            palette.Blue,
            out double hue,
            out double saturation,
            out double value);
        if (Math.Abs(hue - targetHue) > F64(values, 3))
            return false;
        double sd = saturation - targetSaturation;
        double vd = value - targetValue;
        return Math.Sqrt((sd * sd) + (vd * vd)) <= F64(values, 4);
    }

    private static void Hsv(
        byte red,
        byte green,
        byte blue,
        out double hue,
        out double saturation,
        out double value)
    {
        int maximum = Math.Max(red, Math.Max(green, blue));
        int minimum = Math.Min(red, Math.Min(green, blue));
        int delta = maximum - minimum;
        if (delta == 0)
        {
            hue = 0d;
        }
        else if (maximum == red)
        {
            hue = 60d * (green - blue) / delta;
            if (hue < 0d)
                hue += 360d;
        }
        else if (maximum == green)
        {
            hue = (60d * (blue - red) / delta) + 120d;
        }
        else
        {
            hue = (60d * (red - green) / delta) + 240d;
        }
        saturation = maximum == 0 ? 0d : 1d - ((double)minimum / maximum);
        value = maximum / 255d;
    }

    private static IEnumerable<string> SpellNames(
        PluginInventoryItem item,
        IPluginHost? host)
    {
        if (host is null)
            yield break;
        foreach (uint spellId in item.AppraisedSpellIds)
        {
            if (host.Automation.Spells.TryGet(spellId, out PluginSpellInfo spell))
                yield return spell.Name;
        }
    }

    private static int CharacterSkill(
        IPluginHost? host,
        uint skillId,
        bool buffed)
    {
        if (host?.Automation.Character.TryGetSkill(
            skillId,
            out PluginSkillInfo skill) != true)
        {
            return 0;
        }
        return checked((int)(buffed ? skill.Current : skill.Base));
    }

    private static bool CharacterBaseSkillRange(
        string[] values,
        IPluginHost? host)
    {
        int level = CharacterSkill(host, U32(values, 0), buffed: false);
        return level >= I32(values, 1) && level <= I32(values, 2);
    }

    private static string StringValue(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties) => key switch
    {
        1 => item.Name,
        // The name for more than one, which the object itself carries.
        PluralNameKey => item.PluralName ?? string.Empty,
        _ => properties.Strings?.TryGetValue(key, out string? value) == true
            ? value
            : string.Empty,
    };

    private static bool TryIntValue(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        out int value)
    {
        switch (key)
        {
            case 5: value = item.Burden; return true;
            // The slot a worn item is in, as the object itself reports it.
            case 10: return Present(unchecked((int)item.EquippedLocation), out value);
            case 19: value = item.Value; return true;
            // Uses left and the most uses, which the object itself carries.
            case 91: return TryObjectInt(91, item.MaximumStructure, properties, out value);
            case 92: return TryObjectInt(92, item.Structure, properties, out value);
            case 105: value = checked((int)item.Workmanship); return true;
            case 107: value = item.ItemCurrentMana; return true;
            case 108: value = item.ItemMaximumMana; return true;
            case 131: value = checked((int)item.MaterialType); return true;
            case VtankIntBase + 0: value = checked((int)item.WeenieClassId); return true;
            // The icon as the object itself sends it: without the
            // 0x06000000 every icon id carries, like the icon layers.
            case VtankIntBase + 1: return TryIconLayer(item.IconId, out value);
            // The container holding the item or, for an item someone
            // wields, the wielder.
            case VtankIntBase + 2:
                return item.ContainerObjectId != 0u
                    ? Present(unchecked((int)item.ContainerObjectId), out value)
                    : Present(unchecked((int)item.WielderObjectId), out value);
            case VtankIntBase + 4: value = item.ItemsCapacity; return true;
            case VtankIntBase + 5: value = item.ContainersCapacity; return true;
            case VtankIntBase + 6: value = item.StackSize; return true;
            case VtankIntBase + 7: value = item.MaximumStackSize; return true;
            case VtankIntBase + 8: value = checked((int)item.SpellId); return true;
            // Only ever -1, the mark of an item someone wields.
            case VtankIntBase + 9:
                value = item.WielderObjectId != 0u ? -1 : 0;
                return item.WielderObjectId != 0u;
            case VtankIntBase + 10: value = checked((int)item.WielderObjectId); return true;
            case VtankIntBase + 11: value = checked((int)item.EquippedLocation); return true;
            // The body parts a worn item covers, as the object itself sends
            // them: a shirt is 104 (chest and both arm sections), pants 22.
            case VtankIntBase + 13: return Present(unchecked((int)item.CoverageMask), out value);
            case VtankIntBase + 14: value = checked((int)item.ValidLocations); return true;
            // Melee weapon, missile weapon, ammunition, shield.
            case VtankIntBase + 15: return Present(item.CombatUse, out value);
            case IconHighlightKey: value = checked((int)item.Effects); return true;
            // The ammunition a launcher takes or an arrow or bolt is.
            case VtankIntBase + 17: return Present(unchecked((int)item.AmmoType), out value);
            // What kind of object a targeted-use item may be used on.
            case VtankIntBase + 18: return Present(unchecked((int)item.TargetType), out value);
            // The item-type bits (weapon, armor, food and so on) and the
            // object-description bits (door, corpse, vendor, ...) that head
            // every object the server creates.
            case VtankIntBase + 26: return Present(unchecked((int)item.ItemType), out value);
            case VtankIntBase + 27: return Present(unchecked((int)item.PublicFlags), out value);
            case VtankIntBase + 23: value = checked((int)item.PublicFlags); return true;
            // The weapon's speed rating from its appraisal; a weapon never
            // appraised, or anything else, has none.
            case VtankIntBase + 31:
                value = properties.WeaponProfile?.WeaponTime ?? 0;
                return properties.WeaponProfile is not null;
            case VtankIntBase + 32: value = item.WeaponSkill; return true;
            case VtankIntBase + 33: value = item.DamageType; return true;
            case VtankIntBase + 34: value = item.Damage; return true;
            // How the item may be used, including whether it needs a target.
            case VtankIntBase + 35: return Present(unchecked((int)item.Useability), out value);
            case VtankIntBase + 38: value = item.AppraisedSpellIds.Count; return true;
            case VtankIntBase + 41: return TryIconLayer(item.IconOverlayId, out value);
            case VtankIntBase + 42: return TryIconLayer(item.IconUnderlayId, out value);
            default:
                value = 0;
                return properties.Ints?.TryGetValue(key, out value) == true;
        }
    }

    /// <summary>
    /// An icon layer as the reference macro stores it: the id as the server
    /// sends it, without the <c>0x06000000</c> every icon id carries. A rare's
    /// backdrop is <c>0x06005B0C</c>, and loot profiles test for 23308
    /// (<c>0x5B0C</c>). An item without the layer has no value at all.
    /// </summary>
    private static bool TryIconLayer(uint iconId, out int value)
    {
        const uint IconIdPrefix = 0x0600_0000u;
        if (iconId == 0u)
        {
            value = 0;
            return false;
        }
        value = checked((int)(iconId >= IconIdPrefix ? iconId - IconIdPrefix : iconId));
        return true;
    }

    /// <summary>
    /// A value the server sends with the object only when the object has
    /// one. The item snapshot has no "absent" encoding, so zero stands for
    /// "the server sent none", which is what the reference macro sees too.
    /// </summary>
    private static bool Present(int field, out int value)
    {
        value = field;
        return field != 0;
    }

    /// <summary>
    /// A number the object itself carries and an appraisal may repeat: the
    /// appraised value when there is one, otherwise the object's own.
    /// </summary>
    private static bool TryObjectInt(
        uint key,
        int field,
        in PluginItemProperties properties,
        out int value)
    {
        if (properties.Ints?.TryGetValue(key, out value) == true)
            return true;
        return Present(field, out value);
    }

    private static int IntValue(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        TryIntValue(key, item, properties, out int value) ? value : 0;

    private static bool TryDoubleValue(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        out double value)
    {
        switch (key)
        {
            // The seven protections come from the armor numbers an appraisal
            // reports, as multipliers on incoming damage (1.2 = takes 20%
            // more). The reference macro's order is slash, pierce,
            // bludgeon, acid, lightning, fire, cold.
            case VtankDoubleBase + 0:
                return TryArmorNumber(properties, static armor => armor.SlashMod, out value);
            case VtankDoubleBase + 1:
                return TryArmorNumber(properties, static armor => armor.PierceMod, out value);
            case VtankDoubleBase + 2:
                return TryArmorNumber(properties, static armor => armor.BludgeonMod, out value);
            case VtankDoubleBase + 3:
                return TryArmorNumber(properties, static armor => armor.AcidMod, out value);
            case VtankDoubleBase + 4:
                return TryArmorNumber(properties, static armor => armor.ElectricMod, out value);
            case VtankDoubleBase + 5:
                return TryArmorNumber(properties, static armor => armor.FireMod, out value);
            case VtankDoubleBase + 6:
                return TryArmorNumber(properties, static armor => armor.ColdMod, out value);
            // How close, in metres, the character must be to use the item.
            case VtankDoubleBase + 8:
                value = item.UseRadius;
                return item.UseRadius != 0f;
            case VtankDoubleBase + 9: value = item.Workmanship; return true;
            case VtankDoubleBase + 11: value = item.DamageVariance; return true;
            // Attack bonus, range and damage bonus exist only as the weapon
            // numbers an appraisal reports, and only for a weapon: the
            // offense and damage multipliers (1.17 is "+17%") and the launch
            // speed a missile weapon gives its ammunition.
            case VtankDoubleBase + 12:
                return TryWeaponNumber(properties, static weapon => weapon.WeaponOffense, out value);
            case VtankDoubleBase + 13:
                return TryWeaponNumber(properties, static weapon => weapon.MaxVelocity, out value);
            case VtankDoubleBase + 14:
                return TryWeaponNumber(properties, static weapon => weapon.DamageMod, out value);
            default:
                return TryRawFloat(properties, key, out value);
        }
    }

    private static bool TryArmorNumber(
        in PluginItemProperties properties,
        Func<PluginArmorProfile, float> number,
        out double value)
    {
        if (properties.ArmorProfile is { } armor)
        {
            value = number(armor);
            return true;
        }
        value = 0d;
        return false;
    }

    private static bool TryWeaponNumber(
        in PluginItemProperties properties,
        Func<PluginWeaponProfile, double> number,
        out double value)
    {
        if (properties.WeaponProfile is { } weapon)
        {
            value = number(weapon);
            return true;
        }
        value = 0d;
        return false;
    }

    private static double DoubleValue(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        TryDoubleValue(key, item, properties, out double value) ? value : 0d;

    private static bool TryRawFloat(in PluginItemProperties properties, uint key, out double value)
    {
        value = 0d;
        return properties.Floats?.TryGetValue(key, out value) == true;
    }

    private static int BuffedInt(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties)
    {
        int value = IntValue(key, item, properties);
        if (!IntKeyExists(key, item, properties))
            return value;
        foreach (uint spellId in item.AppraisedSpellIds)
        {
            if (IntSpellBonuses.TryGetValue(spellId, out var bonus)
                && bonus.Key == key
                && bonus.Bonus != 0)
            {
                value += bonus.Bonus;
            }
        }
        return value;
    }

    private static double BuffedDouble(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties)
    {
        double value = DoubleValue(key, item, properties);
        if (!DoubleKeyExists(key, item, properties))
            return value;
        foreach (uint spellId in item.AppraisedSpellIds)
        {
            // A bonus of zero is inert: it is what marks a table row that
            // carries only an operation and no amount.
            if (!DoubleSpellBonuses.TryGetValue(spellId, out var bonus)
                || bonus.Key != key
                || bonus.Bonus == 0d)
            {
                continue;
            }
            value = (int)bonus.Change == 1
                ? value * bonus.Bonus
                : value + bonus.Bonus;
        }
        return value;
    }

    // The buffed getters only add a spell bonus when the item carries a base
    // value for that key at all; an item with no such property keeps the
    // caller's default. A key projected out of the item snapshot has no
    // "absent" encoding, so the default value is the only signal we have.
    private static bool IntKeyExists(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        properties.Ints?.ContainsKey(key) == true
        || (TryIntValue(key, item, properties, out int value) && value != 0);

    private static bool DoubleKeyExists(
        uint key,
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        properties.Floats?.ContainsKey(key) == true
        || (TryDoubleValue(key, item, properties, out double value)
            && value != 0d);

    private static double MinimumDamage(in PluginInventoryItem item) =>
        item.Damage - (item.DamageVariance * item.Damage);

    private static double BuffedMedianDamage(
        in PluginInventoryItem item,
        in PluginItemProperties properties)
    {
        int maximum = BuffedInt(VtankIntBase + 34, item, properties);
        double minimum = maximum - (item.DamageVariance * maximum);
        return (minimum + maximum) / 2d;
    }

    private static double BuffedMissileDamage(
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        BuffedInt(VtankIntBase + 34, item, properties)
        + (((BuffedDouble(VtankDoubleBase + 14, item, properties) - 1d)
            * 100d) / 3d)
        + BuffedInt(204, item, properties);

    private static double BuffedTinkedDamage(
        in PluginInventoryItem item,
        in PluginItemProperties properties)
    {
        double variance = item.DamageVariance;
        int maximum = BuffedInt(VtankIntBase + 34, item, properties);
        int tinks = Math.Max(10 - IntValue(171, item, properties), 0);
        if (IntValue(179, item, properties) == 0)
            tinks--;
        if (IntValue(131, item, properties) == 0)
            tinks = 0;
        for (int index = 1; index <= tinks; index++)
        {
            double iron = DamageOverTime(maximum + 25, variance);
            double granite = DamageOverTime(maximum + 24, variance * .8d);
            if (iron >= granite)
                maximum++;
            else
                variance *= .8d;
        }
        return DamageOverTime(maximum + 24, variance);
    }

    private static int TotalRatings(
        in PluginInventoryItem item,
        in PluginItemProperties properties) =>
        item.GearDamage + item.GearDamageResistance
        + item.GearCriticalChance + item.GearCriticalResistance
        + item.GearCriticalDamage + item.GearCriticalDamageResistance
        + IntValue(376, item, properties) + IntValue(379, item, properties);

    private static bool CanReachTarget(
        string[] values,
        in PluginInventoryItem item,
        in PluginItemProperties properties)
    {
        double targetDamage = F64(values, 0);
        double targetDefense = F64(values, 1);
        double targetAttack = F64(values, 2);
        double defense = BuffedDouble(29, item, properties);
        double attack = BuffedDouble(VtankDoubleBase + 12, item, properties);
        double variance = item.DamageVariance;
        int maximum = BuffedInt(VtankIntBase + 34, item, properties);
        int tinks = Math.Max(10 - IntValue(171, item, properties), 0);
        if (IntValue(179, item, properties) == 0)
            tinks--;
        if (IntValue(131, item, properties) == 0)
            tinks = 0;
        for (int index = 1; index <= tinks; index++)
        {
            if (defense < targetDefense)
                defense += .01d;
            else if (attack < targetAttack)
                attack += .01d;
            else if (DamageOverTime(maximum + 25, variance)
                >= DamageOverTime(maximum + 24, variance * .8d))
                maximum++;
            else
                variance *= .8d;
        }
        return DamageOverTime(maximum + 24, variance) >= targetDamage
            && defense >= targetDefense
            && attack >= targetAttack;
    }

    private static double DamageOverTime(int maximum, double variance) =>
        maximum * ((.9d * (2d - variance) / 2d) + .2d);

    private static string[] Lines(string? payload) =>
        (payload ?? string.Empty)
            .Replace("\r\n", "\n", StringComparison.Ordinal)
            .Replace('\r', '\n')
            .Split('\n');

    private static string Value(string[] values, int index) =>
        index >= 0 && index < values.Length
            ? values[index]
            : throw new FormatException("A VTClassic loot requirement is truncated.");

    private static Regex Rx(string[] values, int index) => new(
        Value(values, index),
        RegexOptions.CultureInvariant,
        TimeSpan.FromMilliseconds(100));

    private static int I32(string[] values, int index) =>
        int.TryParse(Value(values, index), NumberStyles.Integer,
            CultureInfo.InvariantCulture, out int parsed)
                ? parsed
                : throw new FormatException("A VTClassic integer is invalid.");

    private static uint U32(string[] values, int index) =>
        uint.TryParse(Value(values, index), NumberStyles.Integer,
            CultureInfo.InvariantCulture, out uint parsed)
                ? parsed
                : throw new FormatException("A VTClassic key is invalid.");

    private static double F64(string[] values, int index) =>
        double.TryParse(Value(values, index).Replace(',', '.'),
            NumberStyles.Float, CultureInfo.InvariantCulture, out double parsed)
                ? parsed
                : throw new FormatException("A VTClassic number is invalid.");

    private static bool Bool(string[] values, int index) =>
        bool.TryParse(Value(values, index), out bool parsed)
            ? parsed
            : throw new FormatException("A VTClassic boolean is invalid.");
}
