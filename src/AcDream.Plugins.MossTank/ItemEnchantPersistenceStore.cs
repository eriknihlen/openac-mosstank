using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal readonly record struct ItemEnchantPersistenceScope(
    string ServerName,
    string CharacterName)
{
    public bool IsValid => ServerName.Length > 0 && CharacterName.Length > 0;

    public static ItemEnchantPersistenceScope Create(
        string? serverName,
        string? characterName) =>
        new(serverName?.Trim() ?? string.Empty, characterName?.Trim() ?? string.Empty);

    public bool Matches(in ItemEnchantPersistenceScope other) =>
        string.Equals(ServerName, other.ServerName, StringComparison.OrdinalIgnoreCase)
        && string.Equals(CharacterName, other.CharacterName, StringComparison.OrdinalIgnoreCase);
}

internal readonly record struct ItemEnchantPersistenceRecord(
    uint ItemObjectId,
    uint WeenieClassId,
    string ItemName,
    uint Family,
    uint SpellId,
    int Quality,
    DateTimeOffset RecordedAtUtc,
    DateTimeOffset ExpiresAtUtc);

internal enum ItemEnchantPersistenceLoadStatus
{
    Unavailable,
    Missing,
    Loaded,
    ClockRollback,
    Invalid,
}

internal readonly record struct ItemEnchantPersistenceLoadResult(
    ItemEnchantPersistenceLoadStatus Status,
    IReadOnlyList<ItemEnchantPersistenceRecord> Records,
    bool RequiresRewrite,
    string? Error = null);

/// <summary>
/// Durable storage for locally observed item-enchantment casts. Each file is
/// scoped to one server and character and repeats that identity in its body.
/// </summary>
internal sealed class ItemEnchantPersistenceStore
{
    private const int CurrentVersion = 1;
    private const string Folder = "item-enchant-timers";

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
    };

    private readonly IPluginStorage _storage;

    public ItemEnchantPersistenceStore(IPluginStorage storage)
    {
        _storage = storage ?? throw new ArgumentNullException(nameof(storage));
    }

    public ItemEnchantPersistenceLoadResult Load(
        in ItemEnchantPersistenceScope scope,
        DateTimeOffset utcNow)
    {
        if (!IsStorageAvailable() || !scope.IsValid)
        {
            return new ItemEnchantPersistenceLoadResult(
                ItemEnchantPersistenceLoadStatus.Unavailable, [], false);
        }

        try
        {
            string key = Key(scope);
            string? text = _storage.ReadText(key);
            if (string.IsNullOrWhiteSpace(text))
            {
                return new ItemEnchantPersistenceLoadResult(
                    ItemEnchantPersistenceLoadStatus.Missing, [], false);
            }

            Document? document = JsonSerializer.Deserialize<Document>(text, JsonOptions);
            if (document is null
                || document.Version != CurrentVersion
                || document.Entries is null
                || !scope.Matches(ItemEnchantPersistenceScope.Create(
                    document.ServerName, document.CharacterName)))
            {
                _storage.Delete(key);
                return new ItemEnchantPersistenceLoadResult(
                    ItemEnchantPersistenceLoadStatus.Invalid,
                    [],
                    false,
                    "The stored timer scope or schema did not match.");
            }

            DateTimeOffset savedAt = DateTimeOffset.FromUnixTimeMilliseconds(
                document.SavedAtUnixMilliseconds);
            if (savedAt > utcNow)
                return RejectRollback(scope);

            bool rewrite = false;
            var records = new Dictionary<(uint Item, uint Family, uint Spell),
                ItemEnchantPersistenceRecord>();
            foreach (Row row in document.Entries)
            {
                DateTimeOffset recordedAt = DateTimeOffset.FromUnixTimeMilliseconds(
                    row.RecordedAtUnixMilliseconds);
                DateTimeOffset expiresAt = DateTimeOffset.FromUnixTimeMilliseconds(
                    row.ExpiresAtUnixMilliseconds);

                // A future observation means wall time moved behind a prior cast.
                // Keeping only some rows would make the same rollback look valid.
                if (recordedAt > utcNow)
                    return RejectRollback(scope);

                if (row.ItemObjectId == 0u
                    || row.WeenieClassId == 0u
                    || string.IsNullOrWhiteSpace(row.ItemName)
                    || row.Family == 0u
                    || row.SpellId == 0u
                    || expiresAt <= recordedAt
                    || expiresAt <= utcNow)
                {
                    rewrite = true;
                    continue;
                }

                var record = new ItemEnchantPersistenceRecord(
                    row.ItemObjectId,
                    row.WeenieClassId,
                    row.ItemName,
                    row.Family,
                    row.SpellId,
                    row.Quality,
                    recordedAt,
                    expiresAt);
                var recordKey = (row.ItemObjectId, row.Family, row.SpellId);
                if (records.TryGetValue(recordKey, out ItemEnchantPersistenceRecord held))
                {
                    rewrite = true;
                    if (held.ExpiresAtUtc >= expiresAt)
                        continue;
                }
                records[recordKey] = record;
            }

            return new ItemEnchantPersistenceLoadResult(
                ItemEnchantPersistenceLoadStatus.Loaded,
                records.Values.ToArray(),
                rewrite);
        }
        catch (Exception exception)
        {
            Delete(scope);
            return new ItemEnchantPersistenceLoadResult(
                ItemEnchantPersistenceLoadStatus.Invalid,
                [],
                false,
                exception.Message);
        }
    }

    public bool Save(
        in ItemEnchantPersistenceScope scope,
        IReadOnlyCollection<ItemEnchantPersistenceRecord> records,
        DateTimeOffset utcNow)
    {
        if (!IsStorageAvailable() || !scope.IsValid)
            return false;

        try
        {
            string key = Key(scope);
            if (records.Count == 0)
            {
                _storage.Delete(key);
                return true;
            }

            var document = new Document
            {
                Version = CurrentVersion,
                ServerName = scope.ServerName,
                CharacterName = scope.CharacterName,
                SavedAtUnixMilliseconds = utcNow.ToUnixTimeMilliseconds(),
                Entries = records
                    .OrderBy(static record => record.ItemObjectId)
                    .ThenBy(static record => record.Family)
                    .ThenBy(static record => record.SpellId)
                    .Select(static record => new Row
                    {
                        ItemObjectId = record.ItemObjectId,
                        WeenieClassId = record.WeenieClassId,
                        ItemName = record.ItemName,
                        Family = record.Family,
                        SpellId = record.SpellId,
                        Quality = record.Quality,
                        RecordedAtUnixMilliseconds =
                            record.RecordedAtUtc.ToUnixTimeMilliseconds(),
                        ExpiresAtUnixMilliseconds =
                            record.ExpiresAtUtc.ToUnixTimeMilliseconds(),
                    })
                    .ToList(),
            };
            _storage.WriteText(key, JsonSerializer.Serialize(document, JsonOptions));
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    public bool Delete(in ItemEnchantPersistenceScope scope)
    {
        if (!IsStorageAvailable() || !scope.IsValid)
            return false;
        try
        {
            return _storage.Delete(Key(scope));
        }
        catch (Exception)
        {
            return false;
        }
    }

    internal static string Key(in ItemEnchantPersistenceScope scope)
    {
        string normalized = scope.ServerName.ToUpperInvariant()
            + "\0"
            + scope.CharacterName.ToUpperInvariant();
        string digest = Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(normalized)));
        return $"{Folder}/{digest}.json";
    }

    private ItemEnchantPersistenceLoadResult RejectRollback(
        in ItemEnchantPersistenceScope scope)
    {
        Delete(scope);
        return new ItemEnchantPersistenceLoadResult(
            ItemEnchantPersistenceLoadStatus.ClockRollback,
            [],
            false,
            "The system clock is earlier than the stored timer observations.");
    }

    private bool IsStorageAvailable()
    {
        try
        {
            return _storage.IsAvailable;
        }
        catch (Exception)
        {
            return false;
        }
    }

    private sealed class Document
    {
        public int Version { get; set; }
        public string ServerName { get; set; } = string.Empty;
        public string CharacterName { get; set; } = string.Empty;
        public long SavedAtUnixMilliseconds { get; set; }
        public List<Row> Entries { get; set; } = [];
    }

    private sealed class Row
    {
        public uint ItemObjectId { get; set; }
        public uint WeenieClassId { get; set; }
        public string ItemName { get; set; } = string.Empty;
        public uint Family { get; set; }
        public uint SpellId { get; set; }
        public int Quality { get; set; }
        public long RecordedAtUnixMilliseconds { get; set; }
        public long ExpiresAtUnixMilliseconds { get; set; }
    }
}
