using System.Globalization;
using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Golden data for the Rust conversion's route follower (rust/, branch
/// rust-port). Reads the scenarios in
/// rust/crates/mosstank-nav/tests/golden/routes.txt, runs each through this
/// plugin's <see cref="NavigationController"/> against a recording client,
/// and writes every call the controller made and every line it said, step
/// by step, to routes.out beside it, which the Rust test compares against.
/// It only runs when MOSSTANK_RUST_GOLDEN is set (to anything), so the
/// normal test run never rewrites the file:
///
///     MOSSTANK_RUST_GOLDEN=1 dotnet test --filter RustPortNavGolden
///
/// The scenario language and the transcript format are the Rust test's
/// (rust/crates/mosstank-nav/tests/golden.rs); change both together.
/// </summary>
public sealed class RustPortNavGolden
{
    private static string RepositoryRoot()
    {
        string? directory = AppContext.BaseDirectory;
        while (directory is not null && !File.Exists(Path.Combine(directory, "MossTank.slnx")))
            directory = Path.GetDirectoryName(directory);
        return directory ?? throw new InvalidOperationException("repository root not found");
    }

    [Fact]
    public void WriteNavGoldenData()
    {
        if (Environment.GetEnvironmentVariable("MOSSTANK_RUST_GOLDEN") is null)
            return;
        string golden = Path.Combine(RepositoryRoot(), "rust", "crates", "mosstank-nav", "tests", "golden");
        string[] lines = File.ReadAllLines(Path.Combine(golden, "routes.txt"));
        File.WriteAllText(Path.Combine(golden, "routes.out"), Run(lines), new UTF8Encoding(false));
    }

    internal static string Run(IEnumerable<string> lines)
    {
        var output = new StringBuilder();
        Scenario? scenario = null;
        foreach (string raw in lines)
        {
            string line = raw.Trim();
            if (line.Length == 0 || line.StartsWith('#'))
                continue;
            List<string> words = Words(line);
            if (words[0] == "case")
            {
                scenario = new Scenario();
                output.Append("##### ").Append(words[1]).Append('\n');
                continue;
            }
            if (scenario is null)
                throw new InvalidOperationException("a line outside a case: " + line);
            scenario.Apply(words, line, output);
        }
        return output.ToString();
    }

    /// <summary>
    /// Space-separated words; double quotes anywhere in a word keep the
    /// spaces between them and are dropped (<c>name="Town Portal"</c>).
    /// </summary>
    private static List<string> Words(string line)
    {
        var words = new List<string>();
        var word = new StringBuilder();
        bool quoted = false;
        bool started = false;
        foreach (char c in line)
        {
            if (c == '"')
            {
                quoted = !quoted;
                started = true;
            }
            else if (c == ' ' && !quoted)
            {
                if (started)
                    words.Add(word.ToString());
                word.Clear();
                started = false;
            }
            else
            {
                word.Append(c);
                started = true;
            }
        }
        if (started)
            words.Add(word.ToString());
        return words;
    }

    /// <summary>A number; with an "m" suffix, metres turned into map units.</summary>
    private static double Number(string text) => text.EndsWith('m')
        ? double.Parse(text[..^1], CultureInfo.InvariantCulture) / 240d
        : double.Parse(text, CultureInfo.InvariantCulture);

    private static PluginNavigationPosition At(string text, float heading = 0f)
    {
        string[] parts = text.Split(',');
        return new PluginNavigationPosition(
            0x7F7F0001u,
            Number(parts[0]),
            Number(parts[1]),
            parts.Length > 2 ? Number(parts[2]) : 0d,
            heading,
            IsOutdoor: true);
    }

    private static string F(double value) => value.ToString(CultureInfo.InvariantCulture);

    private static string F(float value) => value.ToString(CultureInfo.InvariantCulture);

    private static string B(bool value) => value ? "1" : "0";

    private static Dictionary<string, string> Keys(List<string> words, int from)
    {
        var keys = new Dictionary<string, string>(StringComparer.Ordinal);
        for (int index = from; index < words.Count; index++)
        {
            int equals = words[index].IndexOf('=');
            keys[equals < 0 ? words[index] : words[index][..equals]] =
                equals < 0 ? string.Empty : words[index][(equals + 1)..];
        }
        return keys;
    }

    private sealed class Scenario
    {
        private readonly GoldenAutomation _automation = new();
        private readonly NavigationSettings _settings = new() { Enabled = true };
        private NavigationController? _controller;
        private ActionLockTable? _locks;
        private readonly List<string> _log = [];

        private NavigationController Controller
        {
            get
            {
                if (_controller is null)
                {
                    _controller = new NavigationController(new GoldenHost(_automation), _settings);
                    _controller.Log = (_, line) => _log.Add(line);
                    if (_locks is not null)
                        _controller.BindActionLocks(_locks);
                }
                return _controller;
            }
        }

        public void Apply(List<string> words, string line, StringBuilder output)
        {
            Dictionary<string, string> keys;
            switch (words[0])
            {
                case "route":
                    _settings.Mode = Enum.Parse<RouteMode>(words[1]);
                    _settings.MinimumDistanceMeters = Number(words[2]);
                    keys = Keys(words, 3);
                    if (keys.TryGetValue("max", out string? max))
                        _settings.MaximumDistanceMeters = Number(max);
                    if (keys.TryGetValue("portal", out string? portal))
                        _settings.PortalUseDistanceMeters = Number(portal);
                    if (keys.TryGetValue("pathing", out string? pathing))
                        _settings.ClientPathing = Enum.Parse<ClientPathing>(pathing);
                    if (keys.TryGetValue("doors", out string? doors))
                        _settings.OpenDoors = doors == "1";
                    if (keys.TryGetValue("identify", out string? identify))
                        _settings.DoorIdentifyRangeMeters = Number(identify);
                    if (keys.TryGetValue("open", out string? open))
                        _settings.DoorOpenRangeMeters = Number(open);
                    if (keys.TryGetValue("follow", out string? follow))
                        _settings.FollowTargetObjectId = uint.Parse(follow, CultureInfo.InvariantCulture);
                    if (keys.TryGetValue("corners", out string? corners))
                        _settings.FollowAroundCorners = corners == "1";
                    if (keys.TryGetValue("enabled", out string? enabled))
                        _settings.Enabled = enabled == "1";
                    return;
                case "wp":
                    _settings.Waypoints.Add(Waypoint(words));
                    return;
                case "me":
                    _automation.NavigationSnapshot = Me(words);
                    return;
                case "obj":
                    PutObject(words);
                    return;
                case "unobj":
                {
                    uint id = uint.Parse(words[1], CultureInfo.InvariantCulture);
                    _automation.Objects.Remove(id);
                    _automation.WorldObjects.RemoveAll(value => value.ObjectId == id);
                    return;
                }
                case "find":
                    _automation.FoundObject = new PluginNavigationObject(
                        uint.Parse(words[1], CultureInfo.InvariantCulture),
                        words[2],
                        At(words[3]));
                    return;
                case "nofind":
                    _automation.FoundObject = null;
                    return;
                case "typing":
                    _automation.ChatInputActive = words[1] == "1";
                    return;
                case "vendor":
                    _automation.ActiveVendorObjectId = uint.Parse(words[1], CultureInfo.InvariantCulture);
                    return;
                case "appraisal":
                    _automation.WorldAppraisalAvailable = words[1] == "1";
                    return;
                case "magic":
                    keys = Keys(words, 1);
                    _automation.Magic ??= new GoldenMagic(_automation.Transcript);
                    if (keys.TryGetValue("answer", out string? answer))
                        _automation.Magic.RequestAnswer = Enum.Parse<PluginCastRequestResult>(answer);
                    if (keys.TryGetValue("casting", out string? casting))
                        _automation.Magic.IsCasting = casting == "1";
                    if (keys.TryGetValue("completion", out string? completion))
                    {
                        string[] parts = completion.Split(',');
                        _automation.Magic.LastCompletion = new PluginCastCompletion(
                            long.Parse(parts[0], CultureInfo.InvariantCulture),
                            uint.Parse(parts[1], CultureInfo.InvariantCulture),
                            0u,
                            uint.Parse(parts[2], CultureInfo.InvariantCulture));
                    }
                    return;
                case "usecompletion":
                    _automation.ItemCompletion = new PluginItemUseCompletion(
                        long.Parse(words[1], CultureInfo.InvariantCulture),
                        uint.Parse(words[2], CultureInfo.InvariantCulture),
                        0u,
                        uint.Parse(words[3], CultureInfo.InvariantCulture));
                    return;
                case "goanswer":
                    _automation.GoToAnswer = Enum.Parse<PluginNavigationCommandStatus>(words[1]);
                    return;
                case "goend":
                    _automation.GoToReport = _automation.GoToReport with
                    {
                        State = Enum.Parse<PluginGoToState>(words[1]),
                        Reason = words[2],
                    };
                    return;
                case "chat":
                    _automation.ChatMessages.Add(new PluginChatMessage(
                        ulong.Parse(words[1], CultureInfo.InvariantCulture),
                        uint.Parse(words[3], CultureInfo.InvariantCulture),
                        int.Parse(words[4], CultureInfo.InvariantCulture),
                        words[5],
                        words[6],
                        string.Empty)
                    {
                        LogTextType = int.Parse(words[2], CultureInfo.InvariantCulture),
                        DisplayText = words[7],
                    });
                    return;
                case "locks":
                    _locks = new ActionLockTable();
                    _controller?.BindActionLocks(_locks);
                    return;
                case "lockadvance":
                    _locks!.Advance(Number(words[1]));
                    return;
            }

            output.Append("> ").Append(line).Append('\n');
            NavigationController controller = Controller;
            string result = "-";
            switch (words[0])
            {
                case "tick":
                    result = B(controller.Tick(Number(words[1]), words.Count < 3));
                    break;
                case "frame":
                    controller.AdvanceClock(Number(words[1]));
                    result = B(controller.Tick(Number(words[1]), canAct: true));
                    break;
                case "claim":
                    result = B(controller.ClaimFromRulePass(words.Count < 2));
                    break;
                case "step":
                    controller.StepArmedMover(Number(words[1]), words.Count < 3);
                    break;
                case "steps":
                {
                    double seconds = Number(words[1]);
                    for (double elapsed = 0d; elapsed < seconds; elapsed += 0.05d)
                        controller.StepArmedMover(0.05d, navigationSlotsAreClear: true);
                    break;
                }
                case "door":
                    result = B(controller.TickDoorRule(Number(words[1]), words.Count < 3));
                    break;
                case "macrostop":
                    controller.StopForMacroStop();
                    break;
                case "lostturn":
                    controller.StopForLostTurn();
                    break;
                case "reset":
                    controller.Reset();
                    break;
                case "anchor":
                    controller.AnchorRoundToStart();
                    break;
                case "skip":
                    result = controller.SkipWaypoints(int.Parse(words[1], CultureInfo.InvariantCulture))
                        .ToString(CultureInfo.InvariantCulture);
                    break;
                case "back":
                    result = controller.StepBackWaypoints(int.Parse(words[1], CultureInfo.InvariantCulture))
                        .ToString(CultureInfo.InvariantCulture);
                    break;
                case "reverse":
                    controller.ToggleReverse();
                    break;
                case "clearlocks":
                    controller.ClearActionLocks();
                    break;
                case "pathing":
                    _settings.ClientPathing = Enum.Parse<ClientPathing>(words[1]);
                    controller.ClientPathingChanged();
                    break;
                case "warnreset":
                    controller.ResetOncePerRunWarnings();
                    break;
                case "query":
                    result = string.Create(
                        CultureInfo.InvariantCulture,
                        $"detail={controller.RunningDetail} holds={B(controller.HoldsPass)} normal={B(controller.AllowsNormalMovement())} nothing={B(controller.HasNothingLeftToWalk)} block={controller.StepBackBlock} rev={B(controller.Reversing)} walking={B(controller.IsClientWalking)} action={B(controller.HasActiveAction)} consumed0={B(controller.IsWaypointConsumed(0))}");
                    break;
                default:
                    throw new InvalidOperationException("unknown line: " + line);
            }

            foreach (string call in _automation.Transcript)
                output.Append("  ").Append(call).Append('\n');
            _automation.Transcript.Clear();
            foreach (string logged in _log)
                output.Append("  log ").Append(logged).Append('\n');
            _log.Clear();
            output.Append("  = ").Append(result)
                .Append(" | ").Append(controller.Status)
                .Append(" | door: ").Append(controller.DoorStatus)
                .Append(" | idx ").Append(controller.CurrentWaypointIndex.ToString(CultureInfo.InvariantCulture));
            if (_locks is not null)
            {
                output.Append(" | locks ")
                    .Append(B(_locks.IsLocked(ActionLockKind.Navigation)))
                    .Append(B(_locks.IsLocked(ActionLockKind.ItemUse)))
                    .Append(B(_locks.IsLocked(ActionLockKind.DoorOpening)));
            }
            output.Append('\n');
        }

        private static RouteWaypoint Waypoint(List<string> words)
        {
            var waypoint = new RouteWaypoint { Type = Enum.Parse<RouteWaypointType>(words[1]) };
            Dictionary<string, string> keys = Keys(words, 2);
            if (keys.TryGetValue("at", out string? at))
                waypoint.Position = At(at);
            if (keys.TryGetValue("ref", out string? reference))
                waypoint.ReferencePosition = At(reference);
            if (keys.TryGetValue("id", out string? id))
                waypoint.ObjectId = uint.Parse(id, CultureInfo.InvariantCulture);
            if (keys.TryGetValue("name", out string? name))
                waypoint.ObjectName = name;
            if (keys.TryGetValue("text", out string? text))
                waypoint.Text = text;
            if (keys.TryGetValue("dur", out string? duration))
                waypoint.DurationMilliseconds = int.Parse(duration, CultureInfo.InvariantCulture);
            if (keys.TryGetValue("recall", out string? recall))
                waypoint.Recall = Enum.Parse<RouteRecallKind>(recall);
            if (keys.TryGetValue("spell", out string? spell))
                waypoint.RecallSpellId = uint.Parse(spell, CultureInfo.InvariantCulture);
            if (keys.TryGetValue("spellname", out string? spellName))
                waypoint.RecallSpellName = spellName;
            if (keys.TryGetValue("heading", out string? heading))
                waypoint.JumpHeadingDegrees = Number(heading);
            if (keys.TryGetValue("shift", out string? shift))
                waypoint.JumpHoldShift = shift == "1";
            if (keys.TryGetValue("charge", out string? charge))
                waypoint.JumpChargeMilliseconds = int.Parse(charge, CultureInfo.InvariantCulture);
            if (keys.TryGetValue("dir", out string? direction))
                waypoint.JumpDirection = Enum.Parse<RouteJumpDirection>(direction);
            return waypoint;
        }

        private static PluginNavigationSnapshot Me(List<string> words)
        {
            float heading = (float)Number(words[3]);
            PluginNavigationPosition position = At(words[1] + "," + words[2], heading);
            var snapshot = new PluginNavigationSnapshot(
                IsAvailable: !words.Contains("gone"),
                IsPortalSpace: words.Contains("portal"),
                LocalObjectId: 1u,
                position,
                IsMoving: false,
                IsAirborne: words.Contains("air"));
            string? confirmed = words.FirstOrDefault(static word => word.StartsWith("confirmed=", StringComparison.Ordinal));
            if (confirmed is not null)
            {
                string[] parts = confirmed["confirmed=".Length..].Split(',');
                snapshot = snapshot with
                {
                    ConfirmedPosition = At(parts[0] + "," + parts[1]),
                    ConfirmedPositionRevision = ulong.Parse(parts[2], CultureInfo.InvariantCulture),
                };
            }
            return snapshot;
        }

        private void PutObject(List<string> words)
        {
            uint id = uint.Parse(words[1], CultureInfo.InvariantCulture);
            Dictionary<string, string> keys = Keys(words, 4);
            var value = new PluginNavigationObject(id, words[2], At(words[3]))
            {
                IsDoor = keys.ContainsKey("door"),
                IsOpen = keys.ContainsKey("open"),
                IsLocked = keys.ContainsKey("locked"),
                HasLockState = keys.ContainsKey("lockstate"),
                LockDifficulty = keys.TryGetValue("diff", out string? difficulty)
                    ? int.Parse(difficulty, CultureInfo.InvariantCulture)
                    : 0,
            };
            _automation.Objects[id] = value;
            int index = _automation.WorldObjects.FindIndex(existing => existing.ObjectId == id);
            if (index < 0)
                _automation.WorldObjects.Add(value);
            else
                _automation.WorldObjects[index] = value;
        }
    }

    private sealed class GoldenHost(GoldenAutomation automation) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new QuietLogger();
        public IGameState State { get; } = new NoState();
        public IEvents Events { get; } = new NoEvents();
        public ISelectionService Selection { get; } = new NoSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => NoOpPluginStorage.Instance;
        public IAutomationSurface Automation { get; } = automation;
        public IPluginStorage VtankProfiles => NoOpPluginStorage.Instance;
    }

    private sealed class QuietLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class NoState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class NoEvents : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned { add { } remove { } }
        public event Action<double> Tick { add { } remove { } }
    }

    private sealed class NoSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed { add { } remove { } }
        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }

    private sealed class GoldenMagic(List<string> transcript) : IMagicCommands
    {
        public PluginCastRequestResult RequestAnswer { get; set; } = PluginCastRequestResult.Sent;
        public bool IsCasting { get; set; }
        public PluginCastCompletion LastCompletion { get; set; }
        public PluginCastGate EvaluateGate(uint spellId) => PluginCastGate.Refused;
        public bool Cast(uint spellId) => false;

        public PluginCastRequestResult RequestCast(uint spellId)
        {
            transcript.Add($"cast {spellId.ToString(CultureInfo.InvariantCulture)} -> {RequestAnswer}");
            return RequestAnswer;
        }
    }

    /// <summary>The recording client: what <c>NavigationTests.FakeAutomation</c> answers.</summary>
    private sealed class GoldenAutomation
        : IAutomationSurface, INavigationAutomation, IPluginChat, IItemAutomation,
          IWorldObjectAutomation
    {
        public List<string> Transcript { get; } = [];
        public bool IsAvailable => true;
        public bool WorldAppraisalAvailable { get; set; }
        IWorldObjectAutomation IAutomationSurface.Objects => WorldAppraisalAvailable ? this : NoOpAutomationSurface.Instance;

        PluginItemCommandResult IWorldObjectAutomation.Identify(uint objectId)
        {
            Transcript.Add($"identify {objectId.ToString(CultureInfo.InvariantCulture)}");
            return new(PluginItemCommandStatus.Started);
        }

        public ICharacterInfo Character => NoOpAutomationSurface.Instance;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public GoldenMagic? Magic { get; set; }
        IMagicCommands IAutomationSurface.Magic => Magic ?? (IMagicCommands)NoOpAutomationSurface.Instance;
        public bool ChatInputActive { get; set; }
        bool IPluginChat.IsInputActive => ChatInputActive;
        public IPluginChat Chat => this;
        public IItemAutomation Items => this;
        public INavigationAutomation Navigation => this;
        public PluginNavigationSnapshot NavigationSnapshot { get; set; }
        PluginNavigationSnapshot INavigationAutomation.Snapshot => NavigationSnapshot;
        public Dictionary<uint, PluginNavigationObject> Objects { get; } = [];
        public List<PluginNavigationObject> WorldObjects { get; } = [];
        public List<PluginChatMessage> ChatMessages { get; } = [];
        public PluginItemUseCompletion ItemCompletion { get; set; }
        public PluginItemUseCompletion LastCompletion => ItemCompletion;
        public uint ActiveVendorObjectId { get; set; }
        public PluginNavigationObject? FoundObject { get; set; }
        public PluginNavigationCommandStatus GoToAnswer { get; set; } = PluginNavigationCommandStatus.Accepted;
        public PluginGoToReport GoToReport { get; set; }

        public bool TryGetObject(uint objectId, out PluginNavigationObject value) =>
            Objects.TryGetValue(objectId, out value);

        public bool TryFindObject(
            string name,
            in PluginNavigationPosition near,
            double maximumDistanceMeters,
            out PluginNavigationObject value)
        {
            Transcript.Add($"find {name} near {F(near.EastWest)},{F(near.NorthSouth)} within {F(maximumDistanceMeters)}");
            value = FoundObject ?? default;
            return FoundObject.HasValue;
        }

        public IReadOnlyList<PluginNavigationObject> CaptureObjects() => WorldObjects.ToArray();

        public PluginNavigationCommandStatus SetMovementIntent(in PluginMovementIntent intent)
        {
            Transcript.Add(
                $"intent F={B(intent.Forward)} B={B(intent.Backward)} SL={B(intent.StrafeLeft)} SR={B(intent.StrafeRight)} TL={B(intent.TurnLeft)} TR={B(intent.TurnRight)} run={B(intent.Run)} jump={B(intent.Jump)}");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus ClearMovementIntent()
        {
            Transcript.Add("clear");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus GoTo(PluginNavigationPosition position, float arrivalMeters)
        {
            Transcript.Add($"goto {F(position.EastWest)},{F(position.NorthSouth)} within {F(arrivalMeters)} -> {GoToAnswer}");
            if (GoToAnswer != PluginNavigationCommandStatus.Accepted)
                return GoToAnswer;
            GoToReport = new PluginGoToReport(GoToReport.Sequence + 1, PluginGoToState.Walking, 0u, 0f, 0, "walking");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus StopGoTo()
        {
            Transcript.Add("stopgoto");
            GoToReport = GoToReport with { State = PluginGoToState.Stopped, Reason = "stopped" };
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus FaceHeading(float headingDegrees)
        {
            Transcript.Add($"face {F(headingDegrees)}");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus Move(
            PluginMoveDirection direction,
            PluginMovePace pace,
            float amount,
            PluginMoveUnit unit = PluginMoveUnit.MetersOrDegrees)
        {
            Transcript.Add($"move {direction} {pace} {F(amount)} {unit}");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus StopMoving(PluginMoveChannel channel)
        {
            Transcript.Add($"stopmove {channel}");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus Jump(float power)
        {
            Transcript.Add($"jump {F(power)}");
            return PluginNavigationCommandStatus.Accepted;
        }

        public bool Submit(string text)
        {
            Transcript.Add($"submit {text}");
            return true;
        }

        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            ChatMessages.Where(message => message.Sequence > afterSequence).ToArray();

        public void PostSystemMessage(string text) => Transcript.Add($"post {text}");

        public PluginItemCommandResult Use(uint objectId)
        {
            Transcript.Add($"use {objectId.ToString(CultureInfo.InvariantCulture)}");
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }
    }
}
