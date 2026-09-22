using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class SummonPetRule : IMacroRule
{
    private const uint SummoningSkillId = 54u;

    private const uint SummonCooldownSpellId = unchecked((uint)(-32555));

    private readonly IPluginHost _host;
    private readonly CombatSettings _settings;
    private readonly PetAutomation _pets = new();

    private double _now;
    private PetAutomationChoice _choice;
    private bool _running;

    public SummonPetRule(
        IPluginHost host,
        CombatSettings settings)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
    }

    public string Name => "SummonPet";

    public string Status { get; private set; } = string.Empty;

    public bool ValidNow(in MacroPassContext context)
    {
        _now += Math.Max(0d, context.ElapsedSeconds);
        IAutomationSurface automation = _host.Automation;
        _choice = PetAutomationChoice.None;
        if (!automation.IsAvailable)
            return false;
        _pets.Observe(automation.Items, _now);
        if (!_settings.Enabled)          // EnableCombat
            return false;
        if (!_settings.SummonPets)       // SummonPets
            return false;
        if (!IsSummoningTrained(automation.Character))
            return false;
        if (HasSummonCooldown(automation.Character))
            return false;
        float range = (float)(_settings.PetRangeMode == PetRangeMode.Custom
            ? _settings.PetCustomRange
            : _settings.MaximumRange);
        IReadOnlyList<PluginCombatTarget> targets =
            automation.Combat.CaptureHostileTargets(range);
        // The predicate caches its pick and issues nothing: the reference's
        // rule chooses the device here and uses it only when it runs. It
        // asks nothing about a selected target.
        if (!_pets.TrySelectSummon(
                automation.Items,
                automation.Character,
                targets,
                _settings,
                _now,
                out _choice))
        {
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
            if (_choice.Kind == PetAutomationActionKind.Summon)
                Status = _pets.IssueSummon(_host.Automation.Items, _choice, _now);
        }
    }

    public void Reset()
    {
        _now = 0d;
        Status = string.Empty;
        _running = false;
    }

    private static bool IsSummoningTrained(ICharacterInfo character) =>
        character.TryGetSkill(SummoningSkillId, out PluginSkillInfo skill)
        && skill.Training is PluginSkillTraining.Trained
            or PluginSkillTraining.Specialized;

    private static bool HasSummonCooldown(ICharacterInfo character)
    {
        foreach (PluginActiveEnchantment enchantment in character.ActiveEnchantments)
        {
            if (enchantment.SpellId == SummonCooldownSpellId)
                return true;
        }
        return false;
    }
}
