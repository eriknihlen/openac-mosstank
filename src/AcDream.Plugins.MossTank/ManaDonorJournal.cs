using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Permission to drain explicitly collected donors, never inferred from pack contents.</summary>
internal sealed class ManaDonorJournal(IPluginStorage storage, IPluginLogger log)
{
    internal sealed record Entry(uint ObjectId, uint ClassId, string Name)
    {
        public uint StoneId { get; set; }
        public bool Removed { get; set; }
        public string? BlockedReason { get; set; }
    }

    private readonly Dictionary<uint, Entry> _entries = [];
    private string? _scope;
    private string? _key;
    private bool _failed;
    private bool _identityReady;
    internal Action<string>? Changed { get; set; }
    internal IEnumerable<Entry> Entries => _entries.Values;
    internal bool CanOperate => !_failed && (!storage.IsAvailable || _identityReady);

    internal void Bind(string server, string character)
    {
        _identityReady = !string.IsNullOrWhiteSpace(server) && !string.IsNullOrWhiteSpace(character);
        if (!_identityReady)
            return;
        string scope = server.Trim().ToUpperInvariant() + "\0" + character.Trim().ToUpperInvariant();
        if (_scope == scope)
            return;
        _scope = scope;
        _key = "mana-donors/" + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(scope))) + ".json";
        _entries.Clear();
        _failed = false;
        if (!storage.IsAvailable)
            return;
        try
        {
            string? json = storage.ReadText(_key);
            if (json is null)
                return;
            Entry[] entries = JsonSerializer.Deserialize<Entry[]>(json)
                ?? throw new JsonException("Missing donor list.");
            foreach (Entry entry in entries)
            {
                if (entry.ObjectId == 0 || entry.ClassId == 0 || string.IsNullOrWhiteSpace(entry.Name)
                    || !_entries.TryAdd(entry.ObjectId, entry))
                    throw new JsonException("Invalid or duplicate donor identity.");
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or JsonException)
        {
            _entries.Clear();
            Fail("Could not read mana donor history; draining is disabled.", error);
        }
    }


    internal bool Remember(PluginInventoryItem item)
    {
        if (_failed || item.ObjectId == 0)
            return false;
        _entries[item.ObjectId] = new(item.ObjectId, item.WeenieClassId, item.Name);
        return Save();
    }

    internal bool Begin(ManaStoneTransferPlan plan)
    {
        if (!DonorReady(plan.TankObjectId) || !_entries.TryGetValue(plan.TankObjectId, out Entry? entry))
            return false;
        // Persist before sending: interruption between send and acknowledgement
        // must never make the next session repeat a destructive request.
        entry.StoneId = plan.StoneObjectId;
        entry.BlockedReason = "Awaiting inventory confirmation";
        return Save();
    }

    internal void NotSent(uint donor, string? reason)
    {
        if (!_entries.TryGetValue(donor, out Entry? entry))
            return;
        entry.StoneId = 0;
        entry.BlockedReason = reason;
        Save();
    }

    internal void Forget(uint donor)
    {
        if (_entries.Remove(donor))
            Save();
    }

    internal bool DonorReady(uint id) => CanOperate
        && _entries.TryGetValue(id, out Entry? entry)
        && !entry.Removed && entry.StoneId == 0 && entry.BlockedReason is null;

    internal bool StoneHeld(uint id) => _entries.Values.Any(entry => entry.StoneId == id);

    internal void Reconcile(IReadOnlyList<PluginInventoryItem> owned, bool complete = false)
    {
        if (_failed)
            return;
        var inventory = owned.ToDictionary(item => item.ObjectId);
        bool changed = false;
        foreach (Entry entry in _entries.Values.ToArray())
        {
            bool present = inventory.TryGetValue(entry.ObjectId, out PluginInventoryItem donor);
            if (!entry.Removed && present && donor.WeenieClassId != 0 && donor.Name.Length != 0
                && (donor.WeenieClassId != entry.ClassId || donor.Name != entry.Name))
            {
                entry.Removed = true;
                changed = true;
            }
            else if (!present && complete && !entry.Removed)
            {
                entry.Removed = true;
                changed = true;
                Report($"ManaDrain: {entry.Name} is no longer owned; removed donor permission.");
            }

            if (entry.StoneId != 0)
            {
                bool stonePresent = inventory.TryGetValue(entry.StoneId, out PluginInventoryItem stone);
                // Late charge updates settle the held stone without resending.
                // A missing object proves nothing until all packs are listed.
                if ((stonePresent && (stone.Effects & 1u) != 0u)
                    || (!stonePresent && complete))
                {
                    entry.StoneId = 0;
                    entry.BlockedReason = "Previous drain resolved; donor requires review if still owned";
                    changed = true;
                    Report($"ManaDrain: reconciled the held stone for {entry.Name}.");
                }
            }
            if (entry.Removed && entry.StoneId == 0)
            {
                _entries.Remove(entry.ObjectId);
                changed = true;
            }
        }
        if (changed)
            Save();
    }

    private bool Save()
    {
        if (_failed)
            return false;
        if (!storage.IsAvailable)
            return true;
        if (!_identityReady || _key is null)
            return false;
        try
        {
            storage.WriteText(_key, JsonSerializer.Serialize(_entries.Values.ToArray()));
            return true;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            Fail("Could not save mana donor history; draining is disabled.", error);
            return false;
        }
    }

    private void Report(string message)
    {
        log.Info(message);
        Changed?.Invoke(message);
    }

    private void Fail(string message, Exception error)
    {
        _failed = true;
        log.Error(message, error);
    }
}
