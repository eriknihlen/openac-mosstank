using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The one folder the plugin keeps its files in, the copy that fills it from
/// the old flat layout, and the one canonical name a character's files are
/// filed under.
/// </summary>
public sealed class MossTankFileLayoutTests
{
    [Fact]
    public void CanonicalCharacterKeyDropsTheServersDisplayMarker()
    {
        Assert.Equal("Acdream", VtankProfileDirectory.CanonicalCharacterKey("+Acdream"));
        Assert.Equal("Acdream", VtankProfileDirectory.CanonicalCharacterKey("Acdream"));
        Assert.Equal("Acdream", VtankProfileDirectory.CanonicalCharacterKey("  +Acdream  "));
        Assert.Equal(string.Empty, VtankProfileDirectory.CanonicalCharacterKey(null));
        Assert.Equal(string.Empty, VtankProfileDirectory.CanonicalCharacterKey("+"));
    }

    internal sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text = new(StringComparer.Ordinal);

        public bool IsAvailable => true;

        public string? RootPath { get; set; }

        public IReadOnlyCollection<string> Keys => _text.Keys;

        public string? ReadText(string key) =>
            _text.TryGetValue(key, out string? value) ? value : null;

        public IReadOnlyList<string> List(string prefix) => _text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();

        public void WriteText(string key, string content) => _text[key] = content;

        public bool Delete(string key) => _text.Remove(key);
    }
}
