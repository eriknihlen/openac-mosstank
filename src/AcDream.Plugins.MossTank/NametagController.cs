using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Hangs a name over the players, pets, portals, non-player characters,
/// vendors and monsters around the character, with a second line for a
/// player's allegiance or a portal's destination.
/// </summary>
/// <remarks>
/// <para>
/// The host owns the drawing: it takes the whole label set at once and
/// draws each label at a constant screen size wherever its object goes. So
/// this class keeps one tag per object, rebuilds the set every frame from
/// those tags, and hands it over only when it differs from the set last
/// handed over. A frame in which nothing changed costs one comparison.
/// </para>
/// <para>
/// A tag's text is refreshed at most once a second, and an appraisal is
/// asked for on a backoff: only when the object is closer than it was the
/// last time, at most every five seconds, and not past a fixed number of
/// tries. A creature that never answers is left with its name alone rather
/// than asked forever.
/// </para>
/// <para>
/// Objects the client drops while the tags are being walked are queued and
/// removed after the walk, so the walk never edits the table under itself.
/// </para>
/// </remarks>
internal sealed class NametagController : IDisposable
{
    /// <summary>How often one tag's text is refreshed from the object table.</summary>
    internal const double TextUpdateIntervalSeconds = 1d;

    /// <summary>The least time between two appraisal requests for one object.</summary>
    internal const double AssessRetrySeconds = 5d;

    /// <summary>
    /// The count past which no further appraisal is asked for. The check is
    /// "more than this many already sent", so one more than this number is
    /// sent before an object is given up on.
    /// </summary>
    internal const int AssessAttemptLimit = 10;

    /// <summary>How often the page's rows are re-read for a change.</summary>
    internal const double SettingsPollSeconds = 0.25d;

    private const uint LevelProperty = 25u;
    private const uint MonarchProperty = 26u;
    private const uint PetOwnerProperty = 44u;
    private const uint PortalDestinationProperty = 38u;
    private const uint AllegianceNameProperty = 47u;

    private readonly IWorldObjectAutomation _objects;
    private readonly IWorldLabelAutomation _labels;
    private readonly Func<uint> _selfObjectId;
    private readonly Func<PluginNavigationPosition?> _selfPosition;
    private readonly Func<uint> _selfMonarch;
    private readonly Func<NametagSettings> _readSettings;
    private readonly Dictionary<uint, Nametag> _tags = [];
    private readonly List<uint> _destructionQueue = [];
    private readonly List<PluginWorldLabel> _labelSet = [];
    private PluginWorldLabel[] _lastPushed = [];
    private NametagSettings _settings;
    private double _now;
    private double _settingsPollRemaining;
    private bool _needsScan = true;
    private bool _walking;
    private IEvents? _events;
    private Action<PluginObjectChange>? _objectChanged;

    public NametagController(
        IWorldObjectAutomation objects,
        IWorldLabelAutomation labels,
        Func<uint> selfObjectId,
        Func<PluginNavigationPosition?> selfPosition,
        Func<uint> selfMonarch,
        Func<NametagSettings> readSettings)
    {
        _objects = objects;
        _labels = labels;
        _selfObjectId = selfObjectId;
        _selfPosition = selfPosition;
        _selfMonarch = selfMonarch;
        _readSettings = readSettings;
        _settings = readSettings();
    }

    /// <summary>How many label sets have been handed to the host.</summary>
    internal int PushCount { get; private set; }

    /// <summary>The objects currently carrying a tag.</summary>
    internal IReadOnlyCollection<uint> TaggedObjectIds => _tags.Keys;

    /// <summary>Listens for objects arriving, being appraised and leaving.</summary>
    public void Attach(IEvents events)
    {
        Detach();
        _events = events;
        _objectChanged = OnObjectChanged;
        events.ObjectChanged += _objectChanged;
    }

    private void Detach()
    {
        if (_events is not null && _objectChanged is not null)
            _events.ObjectChanged -= _objectChanged;
        _events = null;
        _objectChanged = null;
    }

    /// <summary>
    /// One object arrived, was appraised, or left. Arrival adds a tag when
    /// its kind has a group switched on; an appraisal refreshes its text at
    /// once rather than on the next second; leaving drops it.
    /// </summary>
    public void OnObjectChanged(PluginObjectChange change)
    {
        switch (change.Kind)
        {
            case PluginObjectChangeKind.Created:
                if (_settings.Enabled)
                    AddTag(change.ObjectId);
                break;
            case PluginObjectChangeKind.IdentReceived:
                if (_tags.TryGetValue(change.ObjectId, out Nametag? tag))
                    UpdateData(tag, force: true);
                break;
            case PluginObjectChangeKind.Released:
                RemoveTag(change.ObjectId);
                break;
        }
    }

    /// <summary>
    /// The frame: re-read the page now and then, refresh every tag on its
    /// own second, drop what the client dropped, and hand the host the set
    /// when it changed.
    /// </summary>
    public void OnTick(double elapsedSeconds)
    {
        _now += Math.Max(0d, elapsedSeconds);
        _settingsPollRemaining -= Math.Max(0d, elapsedSeconds);
        if (_settingsPollRemaining <= 0d)
        {
            _settingsPollRemaining = SettingsPollSeconds;
            ApplySettings(_readSettings());
        }
        if (!_settings.Enabled)
        {
            PushIfChanged();
            return;
        }
        if (_needsScan)
        {
            _needsScan = false;
            ScanWorld();
        }
        _walking = true;
        try
        {
            foreach (Nametag tag in _tags.Values)
                UpdateData(tag, force: false);
        }
        finally
        {
            _walking = false;
        }
        FlushDestructionQueue();
        PushIfChanged();
    }

    /// <summary>
    /// The session ended: every tag belongs to a world that is gone. The
    /// host's set is cleared so nothing lingers into the next login, and the
    /// world is scanned afresh when one comes.
    /// </summary>
    public void Reset()
    {
        _tags.Clear();
        _destructionQueue.Clear();
        _needsScan = true;
        PushIfChanged();
    }

    public void Dispose()
    {
        Detach();
        _tags.Clear();
        _destructionQueue.Clear();
        PushIfChanged();
    }

    private void ApplySettings(NametagSettings next)
    {
        if (next == _settings)
            return;
        bool wasEnabled = _settings.Enabled;
        _settings = next;
        if (!next.Enabled)
        {
            _tags.Clear();
            _destructionQueue.Clear();
            _needsScan = true;
            return;
        }
        // A group switched off loses its tags; a group switched on gets
        // them on the scan that follows, which is also what a fresh enable
        // needs.
        foreach (Nametag tag in _tags.Values)
            if (!next.For(tag.Group).Enabled)
                _destructionQueue.Add(tag.ObjectId);
        FlushDestructionQueue();
        _needsScan = _needsScan || !wasEnabled || GroupsSwitchedOn(next);
    }

    private static bool GroupsSwitchedOn(NametagSettings next) =>
        next.Player.Enabled || next.Pet.Enabled || next.AllegiancePlayer.Enabled
        || next.Portal.Enabled || next.Npc.Enabled || next.Vendor.Enabled
        || next.Monster.Enabled;

    private void ScanWorld()
    {
        foreach (PluginWorldObject world in _objects.CaptureObjects())
            if (world.IsLandscape)
                AddTag(world.ObjectId);
    }

    private void AddTag(uint objectId)
    {
        if (objectId == 0u || objectId == _selfObjectId() || _tags.ContainsKey(objectId))
            return;
        if (!_objects.TryGet(objectId, out PluginWorldObject world))
            return;
        if (Classify(world) is not { } group || !_settings.For(group).Enabled)
            return;
        var tag = new Nametag(objectId, group, world.Name);
        _tags.Add(objectId, tag);
        UpdateData(tag, force: false);
    }

    private void RemoveTag(uint objectId)
    {
        if (_walking)
            _destructionQueue.Add(objectId);
        else
            _tags.Remove(objectId);
    }

    private void FlushDestructionQueue()
    {
        if (_destructionQueue.Count == 0)
            return;
        foreach (uint objectId in _destructionQueue)
            _tags.Remove(objectId);
        _destructionQueue.Clear();
    }

    private static NametagGroup? Classify(PluginWorldObject world) => world.ObjectClass switch
    {
        PluginObjectClass.Player => NametagGroup.Player,
        PluginObjectClass.CombatPet => NametagGroup.Pet,
        PluginObjectClass.Monster => NametagGroup.Monster,
        PluginObjectClass.Portal => NametagGroup.Portal,
        PluginObjectClass.Npc => NametagGroup.Npc,
        PluginObjectClass.Vendor => NametagGroup.Vendor,
        _ => null,
    };

    /// <summary>
    /// Refreshes one tag from the object table, at most once a second unless
    /// forced by an appraisal arriving. The text is worked out only while the
    /// object is in range and still has something to learn, then left alone.
    /// </summary>
    private void UpdateData(Nametag tag, bool force)
    {
        if (!force && _now - tag.LastTextUpdate < TextUpdateIntervalSeconds && tag.LastTextUpdate >= 0d)
            return;
        tag.LastTextUpdate = _now;
        if (!_objects.TryGet(tag.ObjectId, out PluginWorldObject world))
        {
            RemoveTag(tag.ObjectId);
            return;
        }
        tag.Name = world.Name;
        float range = RangeTo(world);
        tag.Range = range;
        bool outOfRange = range > _settings.MaxRange;
        if (force || (tag.NeedsProcess && !outOfRange))
        {
            switch (tag.Group)
            {
                case NametagGroup.Player:
                case NametagGroup.AllegiancePlayer:
                    ProcessPlayer(tag, world, range);
                    break;
                case NametagGroup.Portal:
                    ProcessPortal(tag, world, range);
                    break;
                case NametagGroup.Monster:
                    ProcessMonster(tag, world, range);
                    break;
                default:
                    tag.NeedsProcess = false;
                    break;
            }
        }
        tag.OutOfRange = outOfRange;
    }

    private void ProcessPlayer(Nametag tag, PluginWorldObject world, float range)
    {
        PluginItemProperties? properties = Capture(tag.ObjectId);
        int level = LevelOf(properties);
        if (level <= 0)
        {
            TryAssess(tag, range);
            return;
        }
        tag.Level = level;
        uint monarch = 0u;
        properties?.InstanceIds?.TryGetValue(MonarchProperty, out monarch);
        if (monarch != tag.LastMonarch)
        {
            tag.LastMonarch = monarch;
            if (monarch == 0u)
            {
                tag.ShowTicker = false;
            }
            else
            {
                if (SharesAllegiance(tag.ObjectId, monarch))
                    tag.Group = NametagGroup.AllegiancePlayer;
                tag.ShowTicker = true;
                string? allegiance = null;
                properties?.Strings?.TryGetValue(AllegianceNameProperty, out allegiance);
                tag.Ticker = monarch == tag.ObjectId
                    ? $"<{world.Name}>"
                    : $"<{allegiance ?? string.Empty}>";
            }
        }
        tag.NeedsProcess = false;
    }

    private void ProcessPortal(Nametag tag, PluginWorldObject world, float range)
    {
        if (!world.HasAppraisalData)
        {
            TryAssess(tag, range);
            return;
        }
        PluginItemProperties? properties = Capture(tag.ObjectId);
        string? destination = null;
        properties?.Strings?.TryGetValue(PortalDestinationProperty, out destination);
        tag.NeedsProcess = false;
        tag.ShowTicker = true;
        tag.Ticker = $"<{destination ?? string.Empty}>";
    }

    private void ProcessMonster(Nametag tag, PluginWorldObject world, float range)
    {
        PluginItemProperties? properties = Capture(tag.ObjectId);
        uint owner = 0u;
        properties?.InstanceIds?.TryGetValue(PetOwnerProperty, out owner);
        if (owner != 0u)
        {
            tag.Group = NametagGroup.Pet;
            tag.NeedsProcess = false;
            return;
        }
        int level = LevelOf(properties);
        if (level > 0)
        {
            tag.NeedsProcess = false;
            tag.Level = level;
        }
        TryAssess(tag, range);
    }

    private PluginItemProperties? Capture(uint objectId) =>
        _objects.TryCaptureProperties(objectId, out PluginItemProperties properties)
            ? properties
            : null;

    private static int LevelOf(PluginItemProperties? properties)
    {
        int level = 0;
        properties?.Ints?.TryGetValue(LevelProperty, out level);
        return level;
    }

    private bool SharesAllegiance(uint objectId, uint monarch)
    {
        uint mine = _selfMonarch();
        return mine != 0u && (monarch == mine || objectId == mine);
    }

    /// <summary>
    /// The appraisal backoff: closer than last time, at most every five
    /// seconds, and not past the try limit. The first request always goes,
    /// because nothing has been closer than "never measured".
    /// </summary>
    private void TryAssess(Nametag tag, float range)
    {
        if (tag.AssessCount > AssessAttemptLimit)
        {
            tag.NeedsProcess = false;
            return;
        }
        if (tag.NextAssessAt <= _now && tag.LastAssessRange > range)
        {
            tag.LastAssessRange = range;
            tag.NextAssessAt = _now + AssessRetrySeconds;
            tag.AssessCount++;
            _objects.Identify(tag.ObjectId);
        }
    }

    private float RangeTo(PluginWorldObject world)
    {
        if (!world.HasPosition || _selfPosition() is not { } self)
            return float.MaxValue;
        return (float)self.HorizontalDistanceMeters(world.Position);
    }

    /// <summary>
    /// Rebuilds the set the host should show and hands it over only when it
    /// differs from the last one. A set past the host's cap is trimmed to
    /// the nearest objects rather than refused whole.
    /// </summary>
    private void PushIfChanged()
    {
        _labelSet.Clear();
        if (_settings.Enabled)
        {
            IEnumerable<Nametag> shown = _tags.Values
                .Where(tag => !tag.OutOfRange && _settings.For(tag.Group).Enabled);
            int shownCount = shown.Count();
            int labelsPerTag = 2;
            if (shownCount * labelsPerTag > IWorldLabelAutomation.MaximumLabels)
                shown = shown.OrderBy(static tag => tag.Range).ThenBy(static tag => tag.ObjectId);
            else
                shown = shown.OrderBy(static tag => tag.ObjectId);
            foreach (Nametag tag in shown)
            {
                NametagGroupSettings group = _settings.For(tag.Group);
                int needed = tag.ShowTicker ? 2 : 1;
                if (_labelSet.Count + needed > IWorldLabelAutomation.MaximumLabels)
                    break;
                _labelSet.Add(new PluginWorldLabel(
                    tag.ObjectId,
                    tag.Text,
                    ToColor(group.TagColor),
                    0f,
                    tag.ShowTicker ? 1 : 0,
                    _settings.MaxRange,
                    true));
                if (tag.ShowTicker)
                    _labelSet.Add(new PluginWorldLabel(
                        tag.ObjectId,
                        tag.Ticker,
                        ToColor(group.TickerColor),
                        0f,
                        0,
                        _settings.MaxRange,
                        true));
            }
        }
        if (_labelSet.SequenceEqual(_lastPushed))
            return;
        _lastPushed = [.. _labelSet];
        PushCount++;
        _labels.ShowLabels(_lastPushed);
    }

    private static Vector4 ToColor(uint argb) => new(
        ((argb >> 16) & 0xFFu) / 255f,
        ((argb >> 8) & 0xFFu) / 255f,
        (argb & 0xFFu) / 255f,
        ((argb >> 24) & 0xFFu) / 255f);

    /// <summary>One object's tag: what it says and what it is still waiting to learn.</summary>
    private sealed class Nametag(uint objectId, NametagGroup group, string name)
    {
        public uint ObjectId { get; } = objectId;
        public NametagGroup Group { get; set; } = group;
        public string Name { get; set; } = name;
        public int Level { get; set; }
        public string Ticker { get; set; } = string.Empty;
        public bool ShowTicker { get; set; }
        public bool NeedsProcess { get; set; } = true;
        public bool OutOfRange { get; set; } = true;
        public float Range { get; set; } = float.MaxValue;
        public uint LastMonarch { get; set; } = uint.MaxValue;
        public double LastTextUpdate { get; set; } = -1d;
        public float LastAssessRange { get; set; } = float.MaxValue;
        public double NextAssessAt { get; set; } = double.MinValue;
        public int AssessCount { get; set; }

        public string Text => Level > 0 ? $"{Name} [{Level}]" : Name;
    }
}
