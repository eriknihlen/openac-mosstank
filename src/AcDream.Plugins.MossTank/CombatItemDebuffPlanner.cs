using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum CombatDebuffSourceKind
{
    LearnedSpell,
    CasterItem,
    ProcWeapon,
    Grenade,
}

internal readonly record struct CombatDebuffSource(
    DebuffIdentity Identity,
    PluginSpellInfo Spell,
    CombatDebuffSourceKind Kind,
    uint ItemObjectId,
    int SourceSkill,
    int ActionOrder)
{
    public bool UsesItem => Kind != CombatDebuffSourceKind.LearnedSpell;

    /// <summary>
    /// The flight shape to test the way to the monster with, or null when this
    /// source has no flight at all (a melee proc, which lands or does not land
    /// with the swing). The shape belongs to the SOURCE, not to the spell: a
    /// launcher throws its debuff on a missile path whatever the spell is,
    /// while a wand's item spell keeps the spell's own shape.
    /// </summary>
    public PluginProjectilePathKind? PathKind { get; init; }
}

internal static class CombatItemDebuffPlanner
{
    private const uint MeleeWeapon = 0x00000001u;
    private const uint MissileWeapon = 0x00000100u;
    private const uint Caster = 0x00008000u;
    private const uint WarMagicSkill = 34u;
    private const uint VoidMagicSkill = 43u;
    private const uint AlchemySkill = 38u;

    public static IReadOnlyList<CombatDebuffSource> Candidates(
        MonsterRuleActions actions,
        CombatSettings settings,
        ICharacterInfo character,
        ISpellCatalog spells,
        IReadOnlyList<PluginInventoryItem> items,
        Func<DebuffIdentity, PluginSpellInfo, bool> isDue,
        double targetDistance = 0d)
    {
        ArgumentNullException.ThrowIfNull(actions);
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(spells);
        ArgumentNullException.ThrowIfNull(items);
        ArgumentNullException.ThrowIfNull(isDue);

        HashSet<DebuffIdentity> required = DebuffSpellCatalog.Required(actions);
        if (required.Count == 0)
            return Array.Empty<CombatDebuffSource>();
        return Collect(
            required,
            settings,
            character,
            spells,
            items,
            isDue,
            targetDistance);
    }

    public static IReadOnlyList<CombatDebuffSource> Sources(
        DebuffIdentity identity,
        CombatSettings settings,
        ICharacterInfo character,
        ISpellCatalog spells,
        IReadOnlyList<PluginInventoryItem> items,
        double targetDistance = 0d,
        Action<string>? log = null)
    {
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(spells);
        ArgumentNullException.ThrowIfNull(items);
        return Collect(
            new HashSet<DebuffIdentity> { identity },
            settings,
            character,
            spells,
            items,
            isDue: null,
            targetDistance,
            log);
    }

    private static IReadOnlyList<CombatDebuffSource> Collect(
        IReadOnlySet<DebuffIdentity> required,
        CombatSettings settings,
        ICharacterInfo character,
        ISpellCatalog spells,
        IReadOnlyList<PluginInventoryItem> items,
        Func<DebuffIdentity, PluginSpellInfo, bool>? isDue,
        double targetDistance,
        Action<string>? log = null)
    {
        var result = new List<CombatDebuffSource>();
        foreach (PluginSpellInfo spell in spells.KnownCombatSpells)
        {
            // A debuff the character cannot reach the monster with is not a
            // choice at all. This is the ONE place the reference macro asks
            // a spell how far it goes.
            if (targetDistance > 0d
                && AttackSpellCatalog.RangeMeters(
                    spell,
                    character,
                    settings.SpellRangeFudge) < targetDistance)
            {
                continue;
            }
            AddIfRequired(
                result,
                required,
                spell,
                CombatDebuffSourceKind.LearnedSpell,
                0u,
                CurrentSkill(character, spell.School),
                isDue,
                AttackSpellCatalog.ProjectileShapeFor(spell.Family));
        }

        foreach (PluginInventoryItem item in items)
        {
            bool profiled = settings.CombatItemObjectIds.Contains(item.ObjectId)
                || settings.CombatItemNames.Contains(item.Name);
            bool consumable = settings.ConsumableNames.Contains(item.Name);
            if (!profiled && !consumable)
                continue;
            string text = $"{item.Name} [{item.ObjectId}]";
            log?.Invoke($"Find debuff choice ({text}): Begin");
            int before = result.Count;
            if (profiled)
                AddProfileItem(result, required, item, spells, isDue);
            if (consumable)
                AddGrenade(result, required, item, character, spells, isDue);
            if (result.Count == before)
            {
                // dz.cs:303 — the item is not one of the object types this
                // debuff can be applied from.
                log?.Invoke($"Find debuff choice ({text}): Stop, wrong object type");
                continue;
            }
            log?.Invoke(
                "Find debuff choice ("
                + text
                + "): Item set to be used, quality "
                + result[^1].Spell.Quality.ToString(
                    System.Globalization.CultureInfo.InvariantCulture)
                + ".");
            log?.Invoke($"Find debuff choice ({text}): Item tests done.");
        }

        result.Sort((left, right) => Compare(
            left,
            right,
            settings.DebuffSelectionMethod));
        return result;
    }

    private static void AddProfileItem(
        ICollection<CombatDebuffSource> result,
        IReadOnlySet<DebuffIdentity> required,
        PluginInventoryItem item,
        ISpellCatalog spells,
        Func<DebuffIdentity, PluginSpellInfo, bool>? isDue)
    {
        if ((item.ItemType & Caster) != 0u
            && item.SpellId != 0u
            && spells.TryGet(item.SpellId, out PluginSpellInfo casterSpell))
        {
            AddIfRequired(
                result,
                required,
                casterSpell,
                CombatDebuffSourceKind.CasterItem,
                item.ObjectId,
                item.ItemSpellcraft,
                isDue,
                // A wand throws its own spell, so the spell's shape stands.
                AttackSpellCatalog.ProjectileShapeFor(casterSpell.Family));
            return;
        }

        if ((item.ItemType & (MeleeWeapon | MissileWeapon)) == 0u)
            return;
        foreach (uint spellId in item.AppraisedSpellIds)
        {
            if (!spells.TryGet(spellId, out PluginSpellInfo proc)
                || !proc.IsOffensive
                || proc.IsUntargeted
                || proc.School is WarMagicSkill or VoidMagicSkill)
            {
                continue;
            }
            AddIfRequired(
                result,
                required,
                proc,
                CombatDebuffSourceKind.ProcWeapon,
                item.ObjectId,
                item.ItemSpellcraft,
                isDue,
                // A launcher's proc flies the way its ammunition flies; a
                // melee proc has no flight, so nothing is tested.
                (item.ItemType & MissileWeapon) != 0u
                    ? PluginProjectilePathKind.Missile
                    : null);
            // ga.a uses the first qualifying item spell.
            return;
        }
    }

    private static void AddGrenade(
        ICollection<CombatDebuffSource> result,
        IReadOnlySet<DebuffIdentity> required,
        PluginInventoryItem item,
        ICharacterInfo character,
        ISpellCatalog spells,
        Func<DebuffIdentity, PluginSpellInfo, bool>? isDue)
    {
        if ((item.ItemType & MissileWeapon) == 0u
            || item.CombatUse != 0
            || !GrenadeCatalog.TryGet(item.Name, out GrenadeDefinition grenade)
            || CurrentSkill(character, AlchemySkill) < grenade.RequiredAlchemy
            || !spells.TryGet(grenade.SpellId, out PluginSpellInfo spell))
        {
            return;
        }
        AddIfRequired(
            result,
            required,
            spell,
            CombatDebuffSourceKind.Grenade,
            item.ObjectId,
            grenade.Spellcraft,
            isDue,
            PluginProjectilePathKind.Missile);
    }

    private static void AddIfRequired(
        ICollection<CombatDebuffSource> result,
        IReadOnlySet<DebuffIdentity> required,
        PluginSpellInfo spell,
        CombatDebuffSourceKind kind,
        uint itemObjectId,
        int sourceSkill,
        Func<DebuffIdentity, PluginSpellInfo, bool>? isDue,
        PluginProjectilePathKind? pathKind = null)
    {
        if (!DebuffSpellCatalog.TryClassify(
                spell,
                out DebuffIdentity identity,
                out int actionOrder)
            || !required.Contains(identity)
            || isDue?.Invoke(identity, spell) == false)
        {
            return;
        }
        result.Add(new CombatDebuffSource(
            identity,
            spell,
            kind,
            itemObjectId,
            sourceSkill,
            actionOrder)
        {
            PathKind = pathKind,
        });
    }

    private static int Compare(
        CombatDebuffSource left,
        CombatDebuffSource right,
        DebuffSelectionMethod selection)
    {
        if (selection == DebuffSelectionMethod.Skill)
        {
            int skill = right.SourceSkill.CompareTo(left.SourceSkill);
            if (skill != 0)
                return skill;
            int quality = right.Spell.Quality.CompareTo(left.Spell.Quality);
            if (quality != 0)
                return quality;
        }
        else
        {
            int quality = right.Spell.Quality.CompareTo(left.Spell.Quality);
            if (quality != 0)
                return quality;
            int skill = right.SourceSkill.CompareTo(left.SourceSkill);
            if (skill != 0)
                return skill;
        }

        bool leftDirect = left.Kind == CombatDebuffSourceKind.LearnedSpell;
        bool rightDirect = right.Kind == CombatDebuffSourceKind.LearnedSpell;
        if (leftDirect != rightDirect)
            return leftDirect ? -1 : 1;
        int action = left.ActionOrder.CompareTo(right.ActionOrder);
        return action != 0
            ? action
            : left.ItemObjectId.CompareTo(right.ItemObjectId);
    }

    private static int CurrentSkill(ICharacterInfo character, uint skillId) =>
        character.TryGetSkill(skillId, out PluginSkillInfo skill)
            ? checked((int)skill.Current)
            : 0;
}
