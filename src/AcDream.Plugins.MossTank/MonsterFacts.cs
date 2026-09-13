namespace AcDream.Plugins.MossTank;

/// <summary>
/// The two monster facts a rule expression reads from the profile's own game
/// info database rather than from the live object: the species and the
/// maximum health. Neither is something the server tells a client, which is
/// why the reference macro ships a database of them; a monster the database
/// does not list has no species word and no maximum health, and rules written
/// against either simply do not match it.
/// </summary>
internal sealed class MonsterFactTable
{
    private readonly Dictionary<int, string> _speciesWords = [];

    public MonsterFactTable(VtankGameInfoDatabase database) =>
        Database = database ?? throw new ArgumentNullException(nameof(database));

    /// <summary>No database: every lookup answers the unlisted answer.</summary>
    public MonsterFactTable()
        : this(VtankGameInfoDatabase.Empty)
    {
    }

    public VtankGameInfoDatabase Database { get; }

    /// <summary>
    /// Records the word this client uses for a species it has met. The
    /// database stores a species as a number; turning that number back into
    /// the word a rule compares against needs the client's own species table,
    /// and the only place it surfaces is beside a live monster of that
    /// species. A species the client has never met therefore has no word.
    /// </summary>
    public void Learn(int speciesId, string? word)
    {
        if (speciesId <= 0 || string.IsNullOrWhiteSpace(word))
            return;
        _speciesWords[speciesId] = word;
    }

    /// <summary>
    /// The word for the monster's species, or the empty string when the
    /// database does not list it or the word is not known here.
    /// </summary>
    public string Species(string? monsterName)
    {
        int species = Database.SpeciesOf(monsterName);
        return species > 0
            && _speciesWords.TryGetValue(species, out string? word)
                ? word
                : string.Empty;
    }

    /// <summary>The monster's maximum health, or -1 when it is not listed.</summary>
    public int MaximumHealth(string? monsterName) =>
        Database.MaximumHealthOf(monsterName);

    /// <summary>
    /// Is there a row for this monster at all? The maximum-health column can
    /// itself hold -1 for "known monster, unknown ceiling", so a reader that
    /// has to tell the two apart cannot go by the value.
    /// </summary>
    public bool IsListed(string? monsterName) =>
        Database.IsLoaded
        && !string.IsNullOrWhiteSpace(monsterName)
        && Database.SpeciesMembers.ContainsKey(monsterName);

    /// <summary>Is the monster listed as unaffectable by magic?</summary>
    public bool IsImmuneToMagic(string? monsterName) =>
        Database.IsImmuneToMagic(monsterName);
}
