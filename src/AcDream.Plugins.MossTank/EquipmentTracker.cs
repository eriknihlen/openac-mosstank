using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class EquipmentTracker : IDisposable
{
    private const double SettlementSeconds = 0.1d;
    private const double SwapCooldownSeconds = 0.8d;

    private readonly IAutomationSurface _automation;
    private readonly Action _poke;
    private readonly List<uint> _equippedIds = [];
    private readonly List<double> _settlements = [];
    private uint _weaponId;
    private uint _ammoId;
    private uint _shieldId;
    private double _now;
    private double _swapCooldownUntil;
    private bool _recentlySwapped;
    private uint _swapTargetId;
    private bool _wasAvailable;
    private bool _disposed;

    public EquipmentTracker(IAutomationSurface automation, Action poke)
    {
        _automation = automation;
        _poke = poke;
        _wasAvailable = automation.Equipment.IsAvailable;
        if (_wasAvailable)
            SeedFromWorld();
        automation.Equipment.PlacementObserved += OnPlacementObserved;
    }

    public uint WeaponId => ValidOrZero(_weaponId);
    public uint AmmoId => ValidOrZero(_ammoId);
    public uint ShieldId => ValidOrZero(_shieldId);
    public IReadOnlyList<uint> EquippedIds => _equippedIds;
    public uint SwapTargetId => _swapTargetId;

    public bool RecentlySwapped
    {
        get
        {
            if (_swapCooldownUntil < _now)
                _recentlySwapped = false;
            return _recentlySwapped;
        }
    }

    public void Advance(double elapsedSeconds)
    {
        bool available = _automation.Equipment.IsAvailable;
        if (available != _wasAvailable)
        {
            _wasAvailable = available;
            Clear();
            if (available)
                SeedFromWorld();
        }
        if (double.IsFinite(elapsedSeconds) && elapsedSeconds > 0d)
            _now += elapsedSeconds;
        for (int index = 0; index < _settlements.Count;)
        {
            if (_settlements[index] > _now)
            {
                index++;
                continue;
            }
            _settlements.RemoveAt(index);
            _recentlySwapped = false;
            _poke();
        }
    }

    public bool TryArmSwap(uint objectId, bool requirePeace,
        PluginCombatMode effectiveMode)
    {
        if (RecentlySwapped
            || requirePeace && effectiveMode != PluginCombatMode.Peace)
            return false;
        _swapCooldownUntil = _now + SwapCooldownSeconds;
        _swapTargetId = objectId;
        _recentlySwapped = true;
        return true;
    }

    private void SeedFromWorld()
    {
        foreach (PluginEquipmentPlacement placement in
            _automation.Equipment.CaptureWorldPlacementsInOrder())
        {
            TrackEquipped(placement.ObjectId, placement.EquippedLocation,
                scheduleSettlement: false);
        }
    }

    private void OnPlacementObserved(PluginEquipmentObservation observation)
    {
        if (observation.IsRemoval)
            TrackRemoved(observation.ObjectId);
        else
            TrackEquipped(observation.ObjectId,
                observation.EquippedLocation, scheduleSettlement: true);
    }

    private void TrackEquipped(
        uint objectId, uint location, bool scheduleSettlement)
    {
        if (location is 0x01000000u or 0x00100000u
            or 0x00400000u or 0x02000000u)
            _weaponId = objectId;
        if (location == 0x00800000u)
            _ammoId = objectId;
        if (location == 0x00200000u)
            _shieldId = objectId;
        if (location != 0u && !_equippedIds.Contains(objectId))
            _equippedIds.Add(objectId);
        if (scheduleSettlement)
            _settlements.Add(_now + SettlementSeconds);
    }

    private void TrackRemoved(uint objectId)
    {
        if (_weaponId == objectId)
            _weaponId = 0u;
        if (_ammoId == objectId)
            _ammoId = 0u;
        if (_shieldId == objectId)
            _shieldId = 0u;
        _equippedIds.Remove(objectId);
        _settlements.Add(_now + SettlementSeconds);
    }

    private uint ValidOrZero(uint objectId)
    {
        if (objectId == 0u)
            return 0u;
        if (_automation.Objects.IsAvailable)
            return _automation.Objects.TryGet(objectId, out _)
                ? objectId : 0u;
        foreach (PluginEquipmentItem item in
            _automation.Equipment.CaptureOwnedEquipment())
        {
            if (item.ObjectId == objectId)
                return objectId;
        }
        return 0u;
    }

    private void Clear()
    {
        _weaponId = 0u;
        _ammoId = 0u;
        _shieldId = 0u;
        _equippedIds.Clear();
        _settlements.Clear();
        _recentlySwapped = false;
        _swapTargetId = 0u;
    }

    public void Dispose()
    {
        if (_disposed)
            return;
        _disposed = true;
        _automation.Equipment.PlacementObserved -= OnPlacementObserved;
    }
}
