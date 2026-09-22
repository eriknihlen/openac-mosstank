namespace AcDream.Plugins.MossTank;

/// <summary>
/// Which of the three files a setting is written to.
/// </summary>
/// <remarks>
/// The rule is one sentence: how this client looks is one choice for the
/// installation, how the character behaves belongs in a profile so it can be
/// handed to someone else, and who the character is belongs to that
/// character alone. Reading always walks character, then profile, then
/// global, then the default, so a value placed in a wider file still
/// applies; the scope only decides where an edit lands.
/// </remarks>
internal enum UbSettingScope
{
    /// <summary>One value for this installation, whoever is logged in.</summary>
    Global,

    /// <summary>A named, shareable set someone can hand to a friend.</summary>
    Profile,

    /// <summary>One value for this character on this server.</summary>
    Character,
}

/// <summary>
/// One row of the catalogue as authored: everything about a setting that
/// does not depend on where its value is kept.
/// </summary>
/// <param name="Name">
/// The full dotted name, which is also the storage key. The three-level
/// nesting of the display tools is flattened into it, because the page has
/// one filter field where a tree would not fit.
/// </param>
/// <param name="Summary">One line of prose, shown under the list.</param>
/// <param name="Kind">The value shape.</param>
/// <param name="Default">The value used until someone writes one.</param>
/// <param name="Scope">Which file an edit lands in.</param>
internal sealed record UbSettingDefinition(
    string Name,
    string Summary,
    UbSettingKind Kind,
    UbSettingValue Default,
    UbSettingScope Scope)
{
    /// <summary>The choices, for an <see cref="UbSettingKind.Enum"/> row.</summary>
    public IReadOnlyList<VtankEnumValue> Choices { get; init; } = [];

    /// <summary>
    /// The tool this setting belongs to: the first part of the dotted name,
    /// and the entry the category list filters on.
    /// </summary>
    public string Category { get; } =
        Name.IndexOf('.', StringComparison.Ordinal) is var dot and > 0
            ? Name[..dot]
            : Name;

    /// <summary>
    /// The short form shown in the value column: a choice reads as its
    /// label, a list as its lines run together, everything else as the text
    /// it is stored as.
    /// </summary>
    public string Display(UbSettingValue value)
    {
        if (value.Kind == UbSettingKind.Enum)
            foreach (VtankEnumValue choice in Choices)
                if (choice.Value == value.AsInt32())
                    return choice.Label;
        if (value.Kind == UbSettingKind.Collection)
            return value.Items.Count == 0 ? "(empty)" : string.Join(", ", value.Items);
        return value.ToStorageString();
    }
}

/// <summary>
/// Where one setting's value actually lives: the pair the page calls rather
/// than reaching into a store itself.
/// </summary>
/// <param name="Get">Reads the value in force.</param>
/// <param name="Set">Writes a new one.</param>
internal sealed record UbSettingBinding(
    Func<UbSettingValue> Get,
    Action<UbSettingValue> Set)
{
    /// <summary>
    /// Whether what <see cref="Set"/> writes outlives the session. A value
    /// kept somewhere that cannot be saved still changes, so the page asks
    /// this to know which of the two it has just done.
    /// </summary>
    public Func<bool> CanSave { get; init; } = static () => true;
}

/// <summary>One catalogue row, bound to where its value is kept.</summary>
internal sealed class UbSetting
{
    private readonly UbSettingBinding _binding;

    internal UbSetting(
        UbSettingDefinition definition,
        UbSettingBinding binding,
        bool hasLiveOwner = false)
    {
        Definition = definition;
        _binding = binding;
        HasLiveOwner = hasLiveOwner;
    }

    /// <summary>What this setting is.</summary>
    internal UbSettingDefinition Definition { get; }

    /// <summary>
    /// Whether the value belongs to something running rather than to the
    /// settings files, which decides what putting it back to its default
    /// has to do.
    /// </summary>
    internal bool HasLiveOwner { get; }

    /// <summary>The full dotted name.</summary>
    internal string Name => Definition.Name;

    /// <summary>The value shape.</summary>
    internal UbSettingKind Kind => Definition.Kind;

    /// <summary>Which file an edit lands in.</summary>
    internal UbSettingScope Scope => Definition.Scope;

    /// <summary>Reads the value in force.</summary>
    internal UbSettingValue Get() => _binding.Get();

    /// <summary>Writes a new value.</summary>
    internal void Set(UbSettingValue value) => _binding.Set(value);

    /// <summary>Whether a write to this row survives the session.</summary>
    internal bool CanSave => _binding.CanSave();

    /// <summary>The short form for the value column.</summary>
    internal string Display() => Definition.Display(Get());
}

/// <summary>
/// Somewhere a scoped value can be kept. The catalogue reads through all
/// three tiers and writes to the one the setting declares.
/// </summary>
internal interface IUbSettingValues
{
    /// <summary>The value written at one tier, or null when none was.</summary>
    UbSettingValue? Read(UbSettingScope scope, string name);

    /// <summary>Writes one value at one tier.</summary>
    void Write(UbSettingScope scope, string name, UbSettingValue value);

    /// <summary>Removes one value from one tier, so the wider tier shows through.</summary>
    bool Clear(UbSettingScope scope, string name);

    /// <summary>
    /// Whether a write to one tier survives the session. A tier whose file
    /// would not read is not written over, so the value applies now and is
    /// gone on the next login; whoever made the edit has to be told which
    /// of the two happened.
    /// </summary>
    bool IsWritable(UbSettingScope scope);
}

/// <summary>
/// The three tiers held in memory. This is what a session uses before it
/// knows where it may write, and what the tests drive.
/// </summary>
internal sealed class UbSettingValueBag : IUbSettingValues
{
    private readonly Dictionary<UbSettingScope, Dictionary<string, UbSettingValue>> _tiers =
        new()
        {
            [UbSettingScope.Global] = new(StringComparer.OrdinalIgnoreCase),
            [UbSettingScope.Profile] = new(StringComparer.OrdinalIgnoreCase),
            [UbSettingScope.Character] = new(StringComparer.OrdinalIgnoreCase),
        };

    /// <inheritdoc />
    public UbSettingValue? Read(UbSettingScope scope, string name) =>
        _tiers[scope].TryGetValue(name, out UbSettingValue value) ? value : null;

    /// <inheritdoc />
    public void Write(UbSettingScope scope, string name, UbSettingValue value) =>
        _tiers[scope][name] = value;

    /// <inheritdoc />
    public bool Clear(UbSettingScope scope, string name) => _tiers[scope].Remove(name);

    /// <inheritdoc />
    /// <remarks>
    /// Nothing here goes to a file at all, but nothing here can lose an
    /// edit either: this bag is the session.
    /// </remarks>
    public bool IsWritable(UbSettingScope scope) => true;
}

/// <summary>
/// Every UB setting, bound to where its value is kept. Rows whose value
/// already has a live owner elsewhere in the plugin are bound to that owner
/// instead of to the value store, so one number never exists twice.
/// </summary>
internal sealed class UbSettingCatalog
{
    private readonly Dictionary<string, UbSetting> _byName;
    private readonly IUbSettingValues _values;

    internal UbSettingCatalog(
        IUbSettingValues values,
        IReadOnlyDictionary<string, UbSettingBinding>? liveOwners = null)
    {
        ArgumentNullException.ThrowIfNull(values);
        _values = values;
        var rows = new List<UbSetting>(UbSettingDefinitions.All.Count);
        foreach (UbSettingDefinition definition in UbSettingDefinitions.All)
        {
            bool live = liveOwners is not null
                && liveOwners.TryGetValue(definition.Name, out UbSettingBinding? owner);
            UbSettingBinding binding = live
                ? liveOwners![definition.Name]
                : StoredBinding(values, definition);
            rows.Add(new UbSetting(definition, binding, live));
        }
        Settings = rows;
        _byName = rows.ToDictionary(
            static row => row.Name,
            StringComparer.OrdinalIgnoreCase);
        Categories = rows
            .Select(static row => row.Definition.Category)
            .Distinct(StringComparer.Ordinal)
            .OrderBy(static category => category, StringComparer.Ordinal)
            .ToArray();
    }

    /// <summary>Every row, in the order the catalogue declares them.</summary>
    internal IReadOnlyList<UbSetting> Settings { get; }

    /// <summary>The tools, in name order, for the category filter.</summary>
    internal IReadOnlyList<string> Categories { get; }

    /// <summary>The row of one name, if the catalogue has it.</summary>
    internal bool TryGet(string name, out UbSetting setting) =>
        _byName.TryGetValue(name, out setting!);

    /// <summary>The row of one name; throws when the catalogue has no such row.</summary>
    internal UbSetting Require(string name) =>
        _byName.TryGetValue(name, out UbSetting? setting)
            ? setting
            : throw new KeyNotFoundException($"No UB setting is named '{name}'.");

    /// <summary>
    /// Puts one row back to what the catalogue says. A stored row has its
    /// own value taken out of its own file rather than the default written
    /// into it, so a value set at a wider tier shows through again instead
    /// of being shadowed by a copy of the default. A row with a live owner
    /// has nowhere to take a value out of, so it is set.
    /// </summary>
    internal void ResetToDefault(UbSetting row)
    {
        if (row.HasLiveOwner)
            row.Set(row.Definition.Default);
        else
            _values.Clear(row.Scope, row.Name);
    }

    /// <summary>
    /// The reading order: the character's own value, then the profile's,
    /// then the installation's, then what the catalogue says it should be.
    /// A write goes to the one tier the setting declares.
    /// </summary>
    private static UbSettingBinding StoredBinding(
        IUbSettingValues values,
        UbSettingDefinition definition) =>
        new(
            () => values.Read(UbSettingScope.Character, definition.Name)
                ?? values.Read(UbSettingScope.Profile, definition.Name)
                ?? values.Read(UbSettingScope.Global, definition.Name)
                ?? definition.Default,
            value => values.Write(definition.Scope, definition.Name, value))
        {
            CanSave = () => values.IsWritable(definition.Scope),
        };
}
