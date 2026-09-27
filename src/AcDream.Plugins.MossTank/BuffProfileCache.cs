using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Reuses spell grouping while each buff pass still evaluates live state.</summary>
internal sealed class BuffProfileCache
{
    private PluginSpellInfo[] _spells = [];
    private IReadOnlyList<BuffLine> _lines = Array.Empty<BuffLine>();

    internal IReadOnlyList<BuffLine> Capture(IReadOnlyList<PluginSpellInfo> spells)
    {
        // Compare values, not list identity: a host may replace an entry in place.
        bool unchanged = spells.Count == _spells.Length;
        for (int index = 0; unchanged && index < spells.Count; index++)
            unchanged = spells[index] == _spells[index];
        if (unchanged)
            return _lines;

        var snapshot = new PluginSpellInfo[spells.Count];
        for (int index = 0; index < snapshot.Length; index++)
            snapshot[index] = spells[index];
        IReadOnlyList<BuffLine> lines = BuffProfile.Build(snapshot);
        _spells = snapshot;
        _lines = lines;
        return lines;
    }
}
