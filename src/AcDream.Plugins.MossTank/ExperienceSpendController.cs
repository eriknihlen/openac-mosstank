using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The rows the experience spender reads, live, and the one it writes back
/// when a policy is imported.
/// </summary>
internal sealed class ExperienceSpendSettings
{
    public Func<int> StopBeforeMax { get; init; } = static () => 10;
    public Func<int> TriesTimeMilliseconds { get; init; } = static () => 300;
    public Func<long> MaxXpChunk { get; init; } = static () => 1_000_000_000L;
    public Func<IReadOnlyList<string>> PolicyLines { get; init; } = static () => [];
    public Action<IReadOnlyList<string>> SetPolicyLines { get; init; } = static _ => { };
}

/// <summary>
/// Spends banked experience by the weight policy, one request per interval,
/// stopping on any refusal. It is not a macro rule and takes no lock: it
/// never acts mid-fight because it refuses to run at all while the macro is
/// enabled, and says so.
/// </summary>
internal sealed class ExperienceSpendController
{
    /// <summary>
    /// The character's own 64-bit property that holds the experience not yet
    /// spent. Row 2; row 6 beside it is unspent luminance, which the
    /// experience meter reads and this spender must not.
    /// </summary>
    private const uint AvailableExperienceProperty = 2u;

    private readonly IPluginHost _host;
    private readonly Func<bool> _macroEnabled;
    private ExperienceSpendSettings _settings = new();
    private IReadOnlyList<ExperienceStep> _plan = [];
    private int _index;
    private double _sinceSpend;

    public ExperienceSpendController(IPluginHost host, Func<bool> macroEnabled)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _macroEnabled = macroEnabled ?? throw new ArgumentNullException(nameof(macroEnabled));
    }

    public bool IsRunning { get; private set; }

    public string Status { get; private set; } = "Experience spending idle.";

    public void BindSettings(ExperienceSpendSettings settings) =>
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));

    /// <summary>
    /// <c>/vt xp [level|slow|test|export|import &lt;Target=Weight;...&gt;]</c>:
    /// no verb lists the weights in use; <c>level</c> spends in chunks and
    /// <c>slow</c> one rank at a time, either halting a run already going;
    /// <c>test</c> shows the plan; <c>export</c> writes the policy in the
    /// form <c>import</c> reads. Returns the lines to say back.
    /// </summary>
    public IReadOnlyList<string> Command(string arguments)
    {
        string text = (arguments ?? string.Empty).Trim();
        int space = text.IndexOf(' ');
        string verb = (space < 0 ? text : text[..space]).ToLowerInvariant();
        string rest = space < 0 ? string.Empty : text[(space + 1)..].Trim();
        switch (verb)
        {
            case "level":
                return [Start(batch: true)];
            case "slow":
                return [Start(batch: false)];
            case "test":
                return PrintPlan();
            case "export":
                return [ExperiencePolicy.Export(ExperiencePolicy.Parse(_settings.PolicyLines()))];
            case "import":
                var notices = new List<string>();
                IReadOnlyList<string> imported = ExperiencePolicy.Import(_settings.PolicyLines(), rest, notices);
                _settings.SetPolicyLines(imported);
                notices.Add($"Experience policy now holds {ExperiencePolicy.Parse(imported).Count} weight(s).");
                return notices;
            case "":
                return PrintPolicy();
            default:
                return ["Syntax: /vt xp [level|slow|test|export|import <Target=Weight;...>]"];
        }
    }

    /// <summary>Walks the plan: one request each interval. True while running.</summary>
    public bool Tick(double elapsedSeconds)
    {
        if (!IsRunning)
            return false;
        if (_macroEnabled())
        {
            Halt("Experience spending stopped: the macro was enabled.");
            return false;
        }
        if (_index >= _plan.Count)
        {
            Halt($"Finished leveling {_index} target(s).");
            return false;
        }
        _sinceSpend += Math.Max(0d, elapsedSeconds);
        if (_sinceSpend * 1000d < Math.Max(0, _settings.TriesTimeMilliseconds()))
            return true;
        _sinceSpend = 0d;

        ExperienceStep step = _plan[_index];
        if (!TryStatId(step.Target, out uint statId))
        {
            Halt($"Experience spending stopped: the client has no record of {step.Target.Name}.");
            return false;
        }
        PluginAdvancementResult result = _host.Automation.Character.RequestAdvancement(
            step.Target.AdvancementKind,
            statId,
            (ulong)step.Cost);
        if (!result.Accepted)
        {
            Halt($"Experience spending stopped at {step.Target.Name}: {result.Status}"
                + (string.IsNullOrWhiteSpace(result.Notice) ? "." : $", {result.Notice}"));
            return false;
        }
        _index++;
        Status = string.Create(
            CultureInfo.InvariantCulture,
            $"Spent {step.Cost:n0} on {step.Target.Name} ({_index}/{_plan.Count}).");
        return true;
    }

    public void Reset()
    {
        IsRunning = false;
        _plan = [];
        _index = 0;
        _sinceSpend = 0d;
        Status = "Experience spending idle.";
    }

    private string Start(bool batch)
    {
        if (IsRunning)
        {
            Halt("Stopping experience spending.");
            return Status;
        }
        if (!_host.Automation.IsAvailable)
            return Status = "Experience spending needs a live session.";
        if (_macroEnabled())
            return Status = "Experience spending refuses to run while the macro is enabled; stop it first.";

        long unassigned = UnassignedExperience();
        ExperiencePlan plan = Plan(unassigned, batch);
        if (plan.Steps.Count == 0)
            return Status = "Nothing to level: no affordable rank under the policy.";
        _plan = plan.Steps;
        _index = 0;
        // The first request goes out on the next tick, not an interval later.
        _sinceSpend = double.MaxValue / 2d;
        IsRunning = true;
        return Status = string.Create(
            CultureInfo.InvariantCulture,
            $"Spending on a plan consisting of {plan.Steps.Count} step(s) with {unassigned:n0} available exp.");
    }

    private void Halt(string status)
    {
        IsRunning = false;
        _plan = [];
        Status = status;
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + status);
    }

    private IReadOnlyList<string> PrintPlan()
    {
        long unassigned = UnassignedExperience();
        ExperiencePlan plan = Plan(unassigned, batch: false);
        var lines = new List<string>
        {
            string.Create(CultureInfo.InvariantCulture, $"Experience plan for {unassigned:n0} exp:"),
        };
        foreach (ExperienceTargetPlan target in plan.ByTarget)
        {
            lines.Add(string.Create(
                CultureInfo.InvariantCulture,
                $"{target.Target.Name}: {target.Costs.Count} levels for {target.Costs.Sum()} xp"));
        }
        if (plan.ByTarget.Count == 0)
            lines.Add("(nothing can be levelled under the policy)");
        return lines;
    }

    private IReadOnlyList<string> PrintPolicy()
    {
        var lines = new List<string> { "Current experience policy weights:" };
        foreach ((string name, double weight) in ExperiencePolicy.Parse(_settings.PolicyLines()))
        {
            if (weight > 0d)
                lines.Add($"{name}: {weight.ToString(CultureInfo.InvariantCulture)}");
        }
        return lines;
    }

    private ExperiencePlan Plan(long unassigned, bool batch) => ExperiencePlanner.Plan(
        ExperiencePolicy.Parse(_settings.PolicyLines()),
        States(),
        unassigned,
        _settings.StopBeforeMax(),
        batch,
        _settings.MaxXpChunk());

    /// <summary>
    /// Where every target stands, read off the character sheet: attributes
    /// by name, pools by kind, skills by id. A target the client has no
    /// record of is left out and so is never planned.
    /// </summary>
    private List<ExperienceTargetState> States()
    {
        ICharacterInfo character = _host.Automation.Character;
        var states = new List<ExperienceTargetState>();
        foreach (ExperienceTarget target in ExperienceTargets.All)
        {
            switch (target.Kind)
            {
                case ExperienceTargetKind.Attribute:
                    foreach (PluginAttributeInfo attribute in character.Attributes)
                    {
                        if (string.Equals(attribute.Name, target.Name, StringComparison.OrdinalIgnoreCase))
                        {
                            states.Add(new ExperienceTargetState(target, (int)attribute.Ranks, PluginSkillTraining.Unknown));
                            break;
                        }
                    }
                    break;
                case ExperienceTargetKind.Vital:
                    if (character.TryGetVital(VitalKind(target), out PluginVitalInfo vital))
                        states.Add(new ExperienceTargetState(target, (int)vital.Ranks, PluginSkillTraining.Unknown));
                    break;
                default:
                    if (character.TryGetSkill(target.StatId, out PluginSkillInfo skill))
                        states.Add(new ExperienceTargetState(target, (int)skill.Ranks, skill.Training));
                    break;
            }
        }
        return states;
    }

    /// <summary>
    /// The number a request names the target by: the host's own record
    /// first, the catalogue's when the record carries none.
    /// </summary>
    private bool TryStatId(in ExperienceTarget target, out uint statId)
    {
        ICharacterInfo character = _host.Automation.Character;
        statId = 0u;
        switch (target.Kind)
        {
            case ExperienceTargetKind.Attribute:
                foreach (PluginAttributeInfo attribute in character.Attributes)
                {
                    if (string.Equals(attribute.Name, target.Name, StringComparison.OrdinalIgnoreCase))
                    {
                        statId = attribute.StatId != 0u ? attribute.StatId : target.StatId;
                        return true;
                    }
                }
                return false;
            case ExperienceTargetKind.Vital:
                if (!character.TryGetVital(VitalKind(target), out PluginVitalInfo vital))
                    return false;
                statId = vital.StatId != 0u ? vital.StatId : target.StatId;
                return true;
            default:
                statId = target.StatId;
                return character.TryGetSkill(target.StatId, out _);
        }
    }

    /// <summary>Health, stamina and mana are pools 0, 1 and 2 to the lookup.</summary>
    private static int VitalKind(in ExperienceTarget target) => target.Name switch
    {
        "Health" => 0,
        "Stamina" => 1,
        _ => 2,
    };

    private long UnassignedExperience() =>
        _host.Automation.Objects.TryCaptureProperties(
            _host.Automation.Character.ObjectId,
            out PluginItemProperties properties)
        && properties.Int64s is { } quads
        && quads.TryGetValue(AvailableExperienceProperty, out long available)
            ? Math.Max(0L, available)
            : 0L;
}
