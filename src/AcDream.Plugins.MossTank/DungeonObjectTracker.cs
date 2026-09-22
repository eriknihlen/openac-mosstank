using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// One thing placed on the dungeon map: where it is in landblock-local
/// metres, which storey that puts it on, and how it is to be marked.
/// </summary>
internal sealed class DungeonTrackedObject
{
    internal DungeonTrackedObject(
        uint id,
        string name,
        DungeonMarkerKind kind,
        PluginObjectClass objectClass,
        uint iconId,
        bool isMover)
    {
        Id = id;
        Name = name;
        Kind = kind;
        ObjectClass = objectClass;
        IconId = iconId;
        IsMover = isMover;
    }

    /// <summary>The object's id.</summary>
    public uint Id { get; }

    /// <summary>The label, already trimmed for the map.</summary>
    public string Name { get; }

    /// <summary>Which settings rows draw it.</summary>
    public DungeonMarkerKind Kind { get; }

    /// <summary>The object's own class.</summary>
    public PluginObjectClass ObjectClass { get; }

    /// <summary>The icon drawn when the rows ask for one.</summary>
    public uint IconId { get; }

    /// <summary>Whether it is placed again as it moves.</summary>
    public bool IsMover { get; }

    /// <summary>Where it was last placed, in landblock-local metres.</summary>
    public Vector3 Position { get; private set; }

    /// <summary>The storey band its height falls in.</summary>
    public float LayerZ { get; private set; }

    /// <summary>Whether it has been placed at all yet.</summary>
    public bool IsPlaced { get; private set; }

    /// <summary>
    /// Places the object, or moves it if it has gone further than
    /// <paramref name="threshold"/> metres from where it was; returns
    /// whether anything changed. A thing that does not move keeps its
    /// first place whatever a later capture says.
    /// </summary>
    internal bool Place(Vector3 position, double threshold)
    {
        if (IsPlaced && (!IsMover || Vector3.Distance(Position, position) <= threshold))
            return false;
        Position = position;
        LayerZ = DungeonLayerShade.BandOf(position.Z);
        IsPlaced = true;
        return true;
    }
}

/// <summary>
/// The objects on the map for one landblock, kept up to date from the
/// world-object capture and the player's own snapshot, and saying whether
/// anything about their placement changed so the map is repainted only
/// then. The player is placed from the navigation snapshot, which is the
/// client's own position, rather than from the capture.
/// </summary>
internal sealed class DungeonObjectTracker
{
    /// <summary>
    /// How far a mover must go before it is placed again. The reference's
    /// threshold is twenty-five of the units its distance helper returns,
    /// which are metres times two hundred and forty: just over a tenth of a
    /// metre, so a mover is repainted only once it has visibly moved.
    /// </summary>
    public const double UpdateDistanceMeters = 25d / 240d;

    private readonly Dictionary<uint, DungeonTrackedObject> _objects = [];
    private readonly HashSet<uint> _seen = [];
    private uint _landblockId;

    /// <summary>Everything placed on the map, in no particular order.</summary>
    public IReadOnlyCollection<DungeonTrackedObject> Objects => _objects.Values;

    /// <summary>Forgets everything, as when the map moves to another landblock.</summary>
    public void Clear()
    {
        _objects.Clear();
        _landblockId = 0u;
    }

    /// <summary>
    /// Brings the placements up to date with a capture; returns whether
    /// any placement changed. Objects carried, wielded, without a position
    /// or in another landblock are not on this map.
    /// </summary>
    public bool Update(
        IReadOnlyList<PluginWorldObject> capture,
        PluginDungeonFloorplan plan,
        PluginNavigationPosition self,
        uint selfId,
        string selfName)
    {
        ArgumentNullException.ThrowIfNull(capture);
        ArgumentNullException.ThrowIfNull(plan);

        bool changed = false;
        if (_landblockId != plan.LandblockId)
        {
            changed = _objects.Count > 0;
            _objects.Clear();
            _landblockId = plan.LandblockId;
        }
        _seen.Clear();

        if (selfId != 0u && (self.CellId & 0xFFFF0000u) == plan.LandblockId)
        {
            _seen.Add(selfId);
            if (!_objects.TryGetValue(selfId, out DungeonTrackedObject? you))
            {
                you = new DungeonTrackedObject(
                    selfId, selfName, DungeonMarkerKind.You, PluginObjectClass.Player, 0u, isMover: true);
                _objects[selfId] = you;
                changed = true;
            }
            changed |= you.Place(PluginDungeonFloorplan.ToLandblockLocal(in self), UpdateDistanceMeters);
        }

        for (int i = 0; i < capture.Count; i++)
        {
            PluginWorldObject subject = capture[i];
            if (subject.ObjectId == selfId
                || !subject.HasPosition
                || subject.WielderObjectId != 0u
                || subject.ContainerObjectId != 0u
                || (subject.Position.CellId & 0xFFFF0000u) != plan.LandblockId)
            {
                continue;
            }
            _seen.Add(subject.ObjectId);
            if (!_objects.TryGetValue(subject.ObjectId, out DungeonTrackedObject? tracked))
            {
                tracked = new DungeonTrackedObject(
                    subject.ObjectId,
                    DungeonMarkers.LabelOf(in subject),
                    DungeonMarkers.Classify(in subject, selfId, selfName),
                    subject.ObjectClass,
                    subject.IconId,
                    DungeonMarkers.IsMover(subject.ObjectClass));
                _objects[subject.ObjectId] = tracked;
                changed = true;
            }
            PluginNavigationPosition position = subject.Position;
            changed |= tracked.Place(PluginDungeonFloorplan.ToLandblockLocal(in position), UpdateDistanceMeters);
        }

        if (_objects.Count != _seen.Count)
        {
            foreach (uint id in _objects.Keys.Where(id => !_seen.Contains(id)).ToArray())
            {
                _objects.Remove(id);
                changed = true;
            }
        }
        return changed;
    }
}
