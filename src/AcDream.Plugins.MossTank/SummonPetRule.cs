using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Summons a combat pet. It runs on every pass beside the main list, not in
/// it: valid whenever combat and summoning are on, Summoning is trained, the
/// shared summoning cooldown is not running, an essence suits a monster in
/// pet range and there is room ahead for the pet to appear. The cooldown is
/// what stops it re-using an essence: a pet outlives nothing, it expires a
/// little before the cooldown does.
/// <para>
/// One short retry clock sits beside the cooldown, keyed on the server's
/// answer to the use: once a summon has gone out the rule waits for that
/// answer (by the item-use completion count moving past where it stood when
/// the use went out), and a use the client refused, or one the server
/// answered with an error — which starts no cooldown — is not tried again
/// for a second. Without it a summon that fails without starting the
/// cooldown was re-sent on every pass.
/// </para>
/// </summary>
internal sealed class SummonPetRule : IMacroRule
{
    private const uint SummoningSkillId = 54u;

    /// <summary>How far ahead of the character a summoned pet appears, in metres.</summary>
    public const float PetRoomAheadMeters = 3f;

    private readonly IPluginHost _host;
    private readonly CombatSettings _settings;
    private readonly Func<PluginCombatTarget, MonsterDamageType> _attackElement;
    private readonly Func<string, IReadOnlyList<MonsterDamageType>> _damagePreferences;
    private readonly Action<string>? _warn;

    private PetSummonChoice _choice;
    private bool _running;

    /// <summary>How long a refused or failed summon waits before it is tried again.</summary>
    internal const double RefusalRetrySeconds = 1d;

    /// <summary>
    /// How long a summon that went out waits for the server's answer before
    /// the rule stops waiting for it; a lost answer must not stop summoning.
    /// </summary>
    internal const double AnswerWaitSeconds = 5d;

    private double _now;
    private double _retryAt = double.NegativeInfinity;
    private uint _awaitingDevice;
    private long _issueRevision;
    private double _issuedAt;

    public SummonPetRule(
        IPluginHost host,
        CombatSettings settings,
        Func<PluginCombatTarget, MonsterDamageType> attackElement,
        Func<string, IReadOnlyList<MonsterDamageType>> damagePreferences,
        Action<string>? warn = null)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _attackElement = attackElement ?? throw new ArgumentNullException(nameof(attackElement));
        _damagePreferences = damagePreferences
            ?? throw new ArgumentNullException(nameof(damagePreferences));
        _warn = warn;
    }

    public string Name => "SummonPet";

    public string Status { get; private set; } = string.Empty;

    /// <summary>Why the rule last declined, when it is its own clock that held it.</summary>
    public string? DeclineReason { get; private set; }

    public bool ValidNow(in MacroPassContext context)
    {
        _now += Math.Max(0d, context.ElapsedSeconds);
        DeclineReason = null;
        IAutomationSurface automation = _host.Automation;
        _choice = PetSummonChoice.None;
        if (!automation.IsAvailable || !automation.Items.IsAvailable)
            return false;
        ObserveAnswer(automation.Items.LastCompletion);
        if (!_settings.Enabled)          // EnableCombat
            return false;
        if (!_settings.SummonPets)       // SummonPets
            return false;
        if (!IsSummoningTrained(automation.Character))
            return false;
        if (HasSummonCooldown(automation.Spells))
            return false;
        if (_awaitingDevice != 0u)
        {
            DeclineReason = "waiting for the server to answer the last summon";
            return false;
        }
        if (_now < _retryAt)
        {
            DeclineReason = "a refused summon is waiting to be tried again";
            return false;
        }
        // The choice is made here and nothing is issued: the rule uses the
        // essence only on its turn.
        _choice = PetAutomation.SelectPet(
            automation.Items,
            automation.Items.CaptureOwnedItems(),
            automation.Combat.CaptureHostileTargets(PetAutomation.PetRange(_settings)),
            automation.Character,
            _settings,
            _attackElement,
            _damagePreferences,
            _warn);
        if (_choice.IsNone)
            return false;
        // Last, as in the reference: is there room for the pet to appear?
        // A client that cannot tell counts as room, the way the reference
        // takes a check that failed as a clear spot.
        if (automation.Navigation.CheckRoomAhead(PetRoomAheadMeters).Status
            == PluginRoomAheadStatus.Blocked)
        {
            _choice = PetSummonChoice.None;
            return false;
        }
        return true;
    }

    public bool Running
    {
        get => _running;
        set
        {
            _running = value;
            if (!value)
            {
                Status = string.Empty;
                return;
            }
            if (_choice.IsNone)
                return;
            IItemAutomation items = _host.Automation.Items;
            long revision = items.LastCompletion.Revision;
            PluginItemCommandResult result = items.Use(_choice.Device.ObjectId);
            if (result.Status == PluginItemCommandStatus.Started)
            {
                _awaitingDevice = _choice.Device.ObjectId;
                _issueRevision = revision;
                _issuedAt = _now;
                Status = $"Summoning {_choice.Device.Name} for {_choice.Target.Name}";
                return;
            }
            _retryAt = _now + RefusalRetrySeconds;
            Status = result.Notice ?? $"Combat pet summon refused: {result.Status}";
        }
    }

    public void Reset()
    {
        _choice = PetSummonChoice.None;
        _now = 0d;
        _retryAt = double.NegativeInfinity;
        _awaitingDevice = 0u;
        _issueRevision = 0L;
        _issuedAt = 0d;
        DeclineReason = null;
        Status = string.Empty;
        _running = false;
    }

    /// <summary>
    /// Folds the server's answer to the summon that went out into the retry
    /// clock: the answer for that essence ends the wait, and an answer with
    /// an error starts the retry clock. No answer in time ends the wait too.
    /// </summary>
    private void ObserveAnswer(PluginItemUseCompletion completion)
    {
        if (_awaitingDevice == 0u)
            return;
        if (completion.Revision > _issueRevision
            && completion.SourceObjectId == _awaitingDevice)
        {
            _awaitingDevice = 0u;
            if (!completion.IsSuccess)
                _retryAt = _now + RefusalRetrySeconds;
            return;
        }
        if (_now - _issuedAt >= AnswerWaitSeconds)
            _awaitingDevice = 0u;
    }

    private static bool IsSummoningTrained(ICharacterInfo character) =>
        character.TryGetSkill(SummoningSkillId, out PluginSkillInfo skill)
        && skill.Training is PluginSkillTraining.Trained
            or PluginSkillTraining.Specialized;

    /// <summary>
    /// Whether the cooldown every essence shares is still running. The server
    /// keeps it with the cooldowns, not with the enchantments in force, so the
    /// enchantment list never shows it.
    /// </summary>
    private static bool HasSummonCooldown(ISpellCatalog spells) =>
        spells.GetCooldownRemaining(PluginInventoryItem.SummoningCooldownId) > 0d;
}
