using System.Globalization;
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

    /// <summary>The <c>CraftInteractions</c> table: every recipe the crafter knows.</summary>
    public VtankCraftDatabase Crafts { get; private init; } = VtankCraftDatabase.Empty;

    /// <summary>
    /// The profile folder's database, read the way the reference client
    /// reads it (see <see cref="VtankGameInfoFile.LoadBase"/>): the same
    /// database the update check starts from.
    /// </summary>
    public static VtankGameInfoDatabase Load(IPluginStorage storage)
    {
        ArgumentNullException.ThrowIfNull(storage);
        if (!storage.IsAvailable)
            return Empty;
        return From(VtankGameInfoFile.LoadBase(storage));
    }

    private const string DefaultResourceSuffix = ".VtankDefaultGameInfo.ugd";

    /// <summary>
    /// The database the reference client ships inside itself and reads when
    /// the profile directory has none, or an unreadable one. Every table it
    /// ships is empty (its content came from the reference client's online
    /// service and from monsters met in play); a profile-directory file with
    /// content takes precedence.
    /// </summary>
    public static VtankGameInfoDatabase LoadDefault() => From(VtankGameInfoFile.BuiltIn());

    /// <summary>The built-in database's text, as the reference client ships it.</summary>
    internal static string DefaultText()
    {
        Assembly assembly = typeof(VtankGameInfoDatabase).Assembly;
        string resource = assembly.GetManifestResourceNames().Single(
            static name => name.EndsWith(DefaultResourceSuffix, StringComparison.Ordinal));
        using Stream stream = assembly.GetManifestResourceStream(resource)
            ?? throw new InvalidOperationException(
                "The embedded default game information database is missing.");
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    /// <summary>
    /// When the service last changed what this database holds, in seconds
    /// since 1970, or null when it does not say.
    /// </summary>
    public int? LastUpdateTime { get; private init; }

    /// <summary>The database's own version number, or null when it does not say.</summary>
    public int? Version { get; private init; }

    /// <summary>Reads a whole database text; throws <see cref="FormatException"/> when its layout does not read.</summary>
    public static VtankGameInfoDatabase Parse(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        return From(VtankDatabase.Parse(text));
    }

    internal static VtankGameInfoDatabase From(VtankDatabase database)
    {
        ArgumentNullException.ThrowIfNull(database);
        return new VtankGameInfoDatabase
        {
            IsLoaded = true,
            LastUpdateTime = ReadCellInt(database, "DBLastUpdateTime", 1),
            Version = ReadCellInt(database, "DBVersion", 0),
            MonsterDamageOverrides = ReadNamedElements(database, "MonsterDamageOverrides"),
            SpeciesMembers = ReadSpeciesMembers(database),
            MonsterImmunities = ReadMonsterImmunities(database),
            SpeciesDamages = ReadSpeciesDamages(database),
            AmmunitionOptions = ReadAmmunitionOptions(database),
            HealKits = ReadHealKits(database),
            GrenadeOptions = ReadGrenadeOptions(database),
            DrainSpellOptions = ReadDrainSpellOptions(database),
            MartyrSpellOptions = ReadMartyrSpellOptions(database),
            Crafts = ReadCraftInteractions(database),
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

    private static int? ReadCellInt(VtankDatabase database, string tableName, int column)
    {
        VtankTable? table = database.Find(tableName);
        return table is { Rows.Count: > 0 } && table.Rows[0].Cells.Count > column
            && int.TryParse(
                table.Rows[0].Cells[column].ScalarText,
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out int value)
                ? value
                : null;
    }

    private static Dictionary<string, IReadOnlyList<MonsterDamageType>> ReadNamedElements(
        VtankDatabase database,
        string tableName)
    {
        var result = new Dictionary<string, IReadOnlyList<MonsterDamageType>>(
            StringComparer.OrdinalIgnoreCase);
        foreach ((string name, IReadOnlyList<MonsterDamageType> elements) in ReadRows(
            database, tableName, 2,
            static cells => (cells[0].AsString(), VtankDamageElements.Parse(cells[1].AsString()))))
        {
            result[name] = elements;
        }
        return result;
    }

    private static Dictionary<string, VtankSpeciesMember> ReadSpeciesMembers(
        VtankDatabase database)
    {
        var result = new Dictionary<string, VtankSpeciesMember>(
            StringComparer.OrdinalIgnoreCase);
        foreach ((string name, VtankSpeciesMember member) in ReadRows(
            database, "SpeciesMembers", 3,
            static cells => (
                cells[0].AsString(),
                new VtankSpeciesMember(cells[1].AsInt(), cells[2].AsInt()))))
        {
            result[name] = member;
        }
        return result;
    }

    private static Dictionary<string, int> ReadMonsterImmunities(
        VtankDatabase database)
    {
        var result = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        foreach ((string name, int mask) in ReadRows(
            database, "MonsterImmunities", 2,
            static cells => (cells[0].AsString(), cells[1].AsInt())))
        {
            result[name] = mask;
        }
        return result;
    }

    private static Dictionary<int, IReadOnlyList<MonsterDamageType>> ReadSpeciesDamages(
        VtankDatabase database)
    {
        var result = new Dictionary<int, IReadOnlyList<MonsterDamageType>>();
        foreach ((int species, IReadOnlyList<MonsterDamageType> elements) in ReadRows(
            database, "SpeciesDamages", 2,
            static cells => (cells[0].AsInt(), VtankDamageElements.Parse(cells[1].AsString()))))
        {
            result[species] = elements;
        }
        return result;
    }

    private static List<VtankAmmunitionOption> ReadAmmunitionOptions(VtankDatabase database) =>
        ReadRows(database, "AmmunitionOptions", 8, static cells => new VtankAmmunitionOption(
            cells[0].AsString(),
            cells[1].AsInt(),
            cells[2].AsInt(),
            cells[3].AsInt(),
            cells[4].AsInt(),
            cells[5].AsInt(),
            unchecked((uint)cells[6].AsInt()),
            cells[7].AsInt()));

    private static List<VtankHealKit> ReadHealKits(VtankDatabase database) =>
        ReadRows(database, "HealKits", 4, static cells => new VtankHealKit(
            cells[0].AsString(),
            cells[1].AsDouble(),
            cells[2].AsInt(),
            cells[3].AsInt()));

    private static List<VtankGrenadeOption> ReadGrenadeOptions(VtankDatabase database) =>
        ReadRows(database, "GrenadeOptions", 6, static cells => new VtankGrenadeOption(
            cells[0].AsString(),
            cells[1].AsInt(),
            cells[2].AsInt(),
            cells[3].AsInt(),
            unchecked((uint)cells[4].AsInt()),
            cells[5].AsInt()));

    private static List<VtankDrainSpellOption> ReadDrainSpellOptions(VtankDatabase database) =>
        ReadRows(database, "DrainSpellOptions", 5, static cells => new VtankDrainSpellOption(
            unchecked((uint)cells[0].AsInt()),
            cells[1].AsInt(),
            cells[2].AsDouble(),
            cells[3].AsInt(),
            cells[4].AsDouble()));

    private static List<VtankMartyrSpellOption> ReadMartyrSpellOptions(VtankDatabase database) =>
        ReadRows(database, "MartyrSpellOptions", 4, static cells => new VtankMartyrSpellOption(
            unchecked((uint)cells[0].AsInt()),
            cells[1].AsInt(),
            cells[2].AsDouble(),
            cells[3].AsDouble()));

    /// <summary>
    /// The columns the reference client reads by position: the two items,
    /// the result, how many it makes, the skill it needs (column 6), the
    /// difficulty and the row's id. The two message columns are the game's
    /// own and are not needed.
    /// </summary>
    private static VtankCraftDatabase ReadCraftInteractions(VtankDatabase database)
    {
        List<VtankCraftRecipe> result = ReadRows(
            database, "CraftInteractions", 9, static cells => new VtankCraftRecipe(
                cells[0].AsString(),
                cells[1].AsString(),
                cells[2].AsString(),
                cells[3].AsInt(),
                unchecked((uint)cells[6].AsInt()),
                cells[7].AsInt(),
                cells[8].AsInt()));
        return result.Count == 0 ? VtankCraftDatabase.Empty : new VtankCraftDatabase(result);
    }

    /// <summary>
    /// Every row of a table that reads as the kind this client needs. A row
    /// with too few columns, or a column that does not hold the number it
    /// should, is passed over on its own: the reference client only finds
    /// out when it looks that row up, and nothing else in the table is
    /// affected by it.
    /// </summary>
    private static List<T> ReadRows<T>(
        VtankDatabase database,
        string tableName,
        int columns,
        Func<IReadOnlyList<VtankCell>, T> read)
    {
        var result = new List<T>();
        VtankTable? table = database.Find(tableName);
        if (table is null)
            return result;
        foreach (VtankRow row in table.Rows)
        {
            if (row.Cells.Count < columns)
                continue;
            try
            {
                result.Add(read(row.Cells));
            }
            catch (Exception error) when (error is FormatException or OverflowException)
            {
                // This row only; see the summary.
            }
        }
        return result;
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
