using System.Reflection;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>One <c>SpeciesMembers</c> row: monster name -&gt; species + max HP.</summary>
internal readonly record struct VtankSpeciesMember(int Species, int MaximumHealth);

/// <summary>One <c>HealKits</c> row.</summary>
internal readonly record struct VtankHealKit(
    string Name,
    double RestoreBonus,
    int SkillBonus,
    int Vital);

/// <summary>One <c>GrenadeOptions</c> row.</summary>
internal readonly record struct VtankGrenadeOption(
    string Name,
    int WieldRequirementType,
    int WieldRequirementAttribute,
    int WieldRequirementValue,
    uint SpellId,
    int Spellcraft);

/// <summary>One <c>DrainSpellOptions</c> row.</summary>
internal readonly record struct VtankDrainSpellOption(
    uint SpellId,
    int CastTimeMilliseconds,
    double EnemyDrainFactor,
    int EnemyDrainMaximumPoints,
    double ResultMultiplier);

/// <summary>One <c>MartyrSpellOptions</c> row.</summary>
internal readonly record struct VtankMartyrSpellOption(
    uint SpellId,
    int CastTimeMilliseconds,
    double SelfDrainFactor,
    double ResultMultiplier);

internal sealed class VtankGameInfoDatabase
{
    public const string FileName = "gameinfodb.ugd";

    private static readonly MonsterDamageType[] NoElements = [];

    /// <summary>
    /// No database. Every lookup answers the way the reference client answers
    /// with an unloaded one.
    /// </summary>
    public static VtankGameInfoDatabase Empty { get; } = new();

    private VtankGameInfoDatabase()
    {
        MonsterDamageOverrides =
            new Dictionary<string, IReadOnlyList<MonsterDamageType>>(
                StringComparer.OrdinalIgnoreCase);
        SpeciesMembers = new Dictionary<string, VtankSpeciesMember>(
            StringComparer.OrdinalIgnoreCase);
        SpeciesDamages = new Dictionary<int, IReadOnlyList<MonsterDamageType>>();
        AmmunitionOptions = [];
        HealKits = [];
        GrenadeOptions = [];
        DrainSpellOptions = [];
        MartyrSpellOptions = [];
    }

    /// <summary>Is there a database at all?</summary>
    public bool IsLoaded { get; private init; }

    public IReadOnlyDictionary<string, IReadOnlyList<MonsterDamageType>>
        MonsterDamageOverrides { get; private init; }

    public IReadOnlyDictionary<string, VtankSpeciesMember> SpeciesMembers
    { get; private init; }

    /// <summary>Monster name -&gt; immunity mask.</summary>
    public IReadOnlyDictionary<string, int> MonsterImmunities { get; private init; }
        = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);

    /// <summary>The bit that says a monster cannot be affected by magic.</summary>
    public const int ImmuneToMagicMask = 2;

    /// <summary>
    /// The species column of the monster's <c>SpeciesMembers</c> row, or
    /// <c>-1</c> when there is no database or no row for the name.
    /// </summary>
    public int SpeciesOf(string? monsterName) =>
        Member(monsterName) is { } member ? member.Species : -1;

    /// <summary>
    /// The maximum-health column of the monster's <c>SpeciesMembers</c> row,
    /// or <c>-1</c> when there is no database or no row for the name. This is
    /// the only source a monster's maximum health has: the client is never
    /// told it.
    /// </summary>
    public int MaximumHealthOf(string? monsterName) =>
        Member(monsterName) is { } member ? member.MaximumHealth : -1;

    /// <summary>Is the monster listed as unaffectable by magic?</summary>
    public bool IsImmuneToMagic(string? monsterName) =>
        IsLoaded
        && !string.IsNullOrWhiteSpace(monsterName)
        && MonsterImmunities.TryGetValue(monsterName, out int mask)
        && (mask & ImmuneToMagicMask) != 0;

    private VtankSpeciesMember? Member(string? monsterName) =>
        IsLoaded
        && !string.IsNullOrWhiteSpace(monsterName)
        && SpeciesMembers.TryGetValue(monsterName, out VtankSpeciesMember member)
            ? member
            : null;

    public IReadOnlyDictionary<int, IReadOnlyList<MonsterDamageType>> SpeciesDamages
    { get; private init; }

    public IReadOnlyList<VtankAmmunitionOption> AmmunitionOptions { get; private init; }

    public IReadOnlyList<VtankHealKit> HealKits { get; private init; }

    public IReadOnlyList<VtankGrenadeOption> GrenadeOptions { get; private init; }

    public IReadOnlyList<VtankDrainSpellOption> DrainSpellOptions { get; private init; }

    public IReadOnlyList<VtankMartyrSpellOption> MartyrSpellOptions { get; private init; }

    public static VtankGameInfoDatabase Load(IPluginStorage storage)
    {
        ArgumentNullException.ThrowIfNull(storage);
        if (!storage.IsAvailable)
            return Empty;
        string? text = storage.ReadText(FileName);
        if (string.IsNullOrWhiteSpace(text))
            return LoadDefault();
        try
        {
            return Parse(text);
        }
        catch (FormatException)
        {
            return LoadDefault();
        }
        catch (InvalidOperationException)
        {
            return LoadDefault();
        }
    }

    private const string DefaultResourceSuffix = ".VtankDefaultGameInfo.ugd";

    /// <summary>
    /// The database the reference client ships inside itself and reads when
    /// the profile directory has none, or an unreadable one. Every table it
    /// ships is empty (its content came from the reference client's online
    /// service and from monsters met in play); a profile-directory file with
    /// content takes precedence.
    /// </summary>
    public static VtankGameInfoDatabase LoadDefault()
    {
        Assembly assembly = typeof(VtankGameInfoDatabase).Assembly;
        string resource = assembly.GetManifestResourceNames().Single(
            static name => name.EndsWith(DefaultResourceSuffix, StringComparison.Ordinal));
        using Stream stream = assembly.GetManifestResourceStream(resource)
            ?? throw new InvalidOperationException(
                "The embedded default game information database is missing.");
        using var reader = new StreamReader(stream);
        return Parse(reader.ReadToEnd());
    }

    public static VtankGameInfoDatabase Parse(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        VtankDatabase database = VtankDatabase.Parse(text);
        return new VtankGameInfoDatabase
        {
            IsLoaded = true,
            MonsterDamageOverrides = ReadNamedElements(database, "MonsterDamageOverrides"),
            SpeciesMembers = ReadSpeciesMembers(database),
            MonsterImmunities = ReadMonsterImmunities(database),
            SpeciesDamages = ReadSpeciesDamages(database),
            AmmunitionOptions = ReadAmmunitionOptions(database),
            HealKits = ReadHealKits(database),
            GrenadeOptions = ReadGrenadeOptions(database),
            DrainSpellOptions = ReadDrainSpellOptions(database),
            MartyrSpellOptions = ReadMartyrSpellOptions(database),
        };
    }

    public IReadOnlyList<MonsterDamageType> DamagePreferences(string? monsterName)
    {
        if (!IsLoaded || string.IsNullOrWhiteSpace(monsterName))
            return NoElements;
        if (MonsterDamageOverrides.TryGetValue(
                monsterName,
                out IReadOnlyList<MonsterDamageType>? exact))
        {
            return exact;
        }
        if (SpeciesMembers.TryGetValue(monsterName, out VtankSpeciesMember member)
            && SpeciesDamages.TryGetValue(
                member.Species,
                out IReadOnlyList<MonsterDamageType>? species))
        {
            return species;
        }
        return NoElements;
    }

    private static Dictionary<string, IReadOnlyList<MonsterDamageType>> ReadNamedElements(
        VtankDatabase database,
        string tableName)
    {
        var result = new Dictionary<string, IReadOnlyList<MonsterDamageType>>(
            StringComparer.OrdinalIgnoreCase);
        foreach (VtankRow row in Rows(database, tableName, 2))
            result[row.Cells[0].AsString()] = VtankDamageElements.Parse(row.Cells[1].AsString());
        return result;
    }

    private static Dictionary<string, VtankSpeciesMember> ReadSpeciesMembers(
        VtankDatabase database)
    {
        var result = new Dictionary<string, VtankSpeciesMember>(
            StringComparer.OrdinalIgnoreCase);
        foreach (VtankRow row in Rows(database, "SpeciesMembers", 3))
        {
            result[row.Cells[0].AsString()] = new VtankSpeciesMember(
                row.Cells[1].AsInt(),
                row.Cells[2].AsInt());
        }
        return result;
    }

    private static Dictionary<string, int> ReadMonsterImmunities(
        VtankDatabase database)
    {
        var result = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        foreach (VtankRow row in Rows(database, "MonsterImmunities", 2))
            result[row.Cells[0].AsString()] = row.Cells[1].AsInt();
        return result;
    }

    private static Dictionary<int, IReadOnlyList<MonsterDamageType>> ReadSpeciesDamages(
        VtankDatabase database)
    {
        var result = new Dictionary<int, IReadOnlyList<MonsterDamageType>>();
        foreach (VtankRow row in Rows(database, "SpeciesDamages", 2))
            result[row.Cells[0].AsInt()] = VtankDamageElements.Parse(row.Cells[1].AsString());
        return result;
    }

    private static List<VtankAmmunitionOption> ReadAmmunitionOptions(VtankDatabase database)
    {
        var result = new List<VtankAmmunitionOption>();
        foreach (VtankRow row in Rows(database, "AmmunitionOptions", 8))
        {
            result.Add(new VtankAmmunitionOption(
                row.Cells[0].AsString(),
                row.Cells[1].AsInt(),
                row.Cells[2].AsInt(),
                row.Cells[3].AsInt(),
                row.Cells[4].AsInt(),
                row.Cells[5].AsInt(),
                unchecked((uint)row.Cells[6].AsInt()),
                row.Cells[7].AsInt()));
        }
        return result;
    }

    private static List<VtankHealKit> ReadHealKits(VtankDatabase database)
    {
        var result = new List<VtankHealKit>();
        foreach (VtankRow row in Rows(database, "HealKits", 4))
        {
            result.Add(new VtankHealKit(
                row.Cells[0].AsString(),
                row.Cells[1].AsDouble(),
                row.Cells[2].AsInt(),
                row.Cells[3].AsInt()));
        }
        return result;
    }

    private static List<VtankGrenadeOption> ReadGrenadeOptions(VtankDatabase database)
    {
        var result = new List<VtankGrenadeOption>();
        foreach (VtankRow row in Rows(database, "GrenadeOptions", 6))
        {
            result.Add(new VtankGrenadeOption(
                row.Cells[0].AsString(),
                row.Cells[1].AsInt(),
                row.Cells[2].AsInt(),
                row.Cells[3].AsInt(),
                unchecked((uint)row.Cells[4].AsInt()),
                row.Cells[5].AsInt()));
        }
        return result;
    }

    private static List<VtankDrainSpellOption> ReadDrainSpellOptions(VtankDatabase database)
    {
        var result = new List<VtankDrainSpellOption>();
        foreach (VtankRow row in Rows(database, "DrainSpellOptions", 5))
        {
            result.Add(new VtankDrainSpellOption(
                unchecked((uint)row.Cells[0].AsInt()),
                row.Cells[1].AsInt(),
                row.Cells[2].AsDouble(),
                row.Cells[3].AsInt(),
                row.Cells[4].AsDouble()));
        }
        return result;
    }

    private static List<VtankMartyrSpellOption> ReadMartyrSpellOptions(VtankDatabase database)
    {
        var result = new List<VtankMartyrSpellOption>();
        foreach (VtankRow row in Rows(database, "MartyrSpellOptions", 4))
        {
            result.Add(new VtankMartyrSpellOption(
                unchecked((uint)row.Cells[0].AsInt()),
                row.Cells[1].AsInt(),
                row.Cells[2].AsDouble(),
                row.Cells[3].AsDouble()));
        }
        return result;
    }

    private static IEnumerable<VtankRow> Rows(
        VtankDatabase database,
        string tableName,
        int columns)
    {
        VtankTable? table = database.Find(tableName);
        if (table is null)
            yield break;
        foreach (VtankRow row in table.Rows)
        {
            if (row.Cells.Count >= columns)
                yield return row;
        }
    }
}

internal static class VtankDamageElements
{
    public static IReadOnlyList<MonsterDamageType> Parse(string text)
    {
        if (string.IsNullOrWhiteSpace(text))
            return [];
        var result = new List<MonsterDamageType>(7);
        foreach (Range range in text.AsSpan().Split(';'))
        {
            if (!int.TryParse(text.AsSpan()[range], out int raw))
                continue;
            MonsterDamageType mapped = raw switch
            {
                0 => MonsterDamageType.Pierce,
                1 => MonsterDamageType.Bludgeon,
                2 => MonsterDamageType.Slash,
                3 => MonsterDamageType.Acid,
                4 => MonsterDamageType.Electric,
                5 => MonsterDamageType.Cold,
                6 => MonsterDamageType.Fire,
                _ => MonsterDamageType.None,
            };
            if (mapped != MonsterDamageType.None && !result.Contains(mapped))
                result.Add(mapped);
        }
        return result;
    }
}
