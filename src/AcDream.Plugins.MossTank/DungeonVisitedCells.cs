using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The cells of one landblock the player has stood in, kept in the
/// settings tree so the next visit to the same dungeon starts with them
/// shaded. One file per landblock, one cell id per line in hexadecimal:
/// small, readable, and never worth a format of its own. A line that is
/// not a cell id is skipped rather than failing the file.
/// </summary>
internal sealed class DungeonVisitedCells(IPluginStorage storage)
{
    private readonly IPluginStorage _storage = storage ?? throw new ArgumentNullException(nameof(storage));
    private readonly HashSet<uint> _cells = [];
    private uint _landblockId;
    private string? _key;

    /// <summary>The landblock the set is of, with a zero low half; zero before any bind.</summary>
    public uint LandblockId => _landblockId;

    /// <summary>How many cells of this landblock have been walked.</summary>
    public int Count => _cells.Count;

    /// <summary>The file a landblock's cells are kept in.</summary>
    public static string KeyFor(uint landblockId) =>
        $"{UbSettingStore.Root}dungeonmaps/visited/{(landblockId >> 16) & 0xFFFFu:X4}.txt";

    /// <summary>
    /// Opens the set for a landblock, reading what an earlier session left.
    /// Binding the same landblock again is no change.
    /// </summary>
    public void Bind(uint landblockId)
    {
        landblockId &= 0xFFFF0000u;
        if (landblockId == _landblockId && _key is not null)
            return;
        _landblockId = landblockId;
        _key = KeyFor(landblockId);
        _cells.Clear();
        if (!_storage.IsAvailable || _storage.ReadText(_key) is not { } text)
            return;
        foreach (string line in text.Split('\n'))
        {
            string trimmed = line.Trim();
            if (trimmed.Length > 0
                && uint.TryParse(trimmed, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out uint cellId)
                && (cellId & 0xFFFF0000u) == landblockId)
            {
                _cells.Add(cellId);
            }
        }
    }

    /// <summary>Whether the player has stood in a cell.</summary>
    public bool Contains(uint cellId) => _cells.Contains(cellId);

    /// <summary>
    /// Records that the player stands in a cell; returns whether that is
    /// new. A cell of another landblock is not this set's and is refused.
    /// </summary>
    public bool Mark(uint cellId)
    {
        if (_key is null || (cellId & 0xFFFF0000u) != _landblockId || !_cells.Add(cellId))
            return false;
        Save();
        return true;
    }

    private void Save()
    {
        if (_key is null || !_storage.IsAvailable)
            return;
        var text = new StringBuilder(_cells.Count * 9);
        foreach (uint cellId in _cells.Order())
            text.Append(cellId.ToString("X8", CultureInfo.InvariantCulture)).Append('\n');
        _storage.WriteText(_key, text.ToString());
    }
}
