using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The experience spender: it reads the character's ranks, training and
/// banked experience off the host, plans, and walks the plan one request
/// per interval, stopping on any refusal. It is not a macro rule: it takes
/// no lock and simply refuses to run while the macro is enabled.
/// </summary>
public sealed class ExperienceSpendControllerTests
{
    private static readonly string[] Policy = ["Strength = 1", "Health = 1.4", "WarMagic = 10"];

    /// <summary>
    /// Batch mode sends one chunk per target: war magic's six affordable
    /// ranks (282 of 300) go out as one request, named by the skill's id,
    /// and the run reports how many targets it levelled. Mutation: sending
    /// the next step without waiting the interval fires two requests on one
    /// tick in slow mode.
    /// </summary>
    [Fact]
    public void LevelSendsChunksAndSlowSendsOneRankPerInterval()
    {
        var automation = new FakeAutomation { Unassigned = 300 };
        ExperienceSpendController controller = Controller(automation, macroEnabled: () => false);

        Assert.Contains("1 step", controller.Command("level")![0], StringComparison.Ordinal);
        Assert.True(controller.IsRunning);
        controller.Tick(0d);
        Assert.Equal([(PluginAdvancementKind.Skill, 34u, 282UL)], automation.Requests);
        controller.Tick(0.1d);
        Assert.False(controller.IsRunning);
        Assert.Contains("1 target", controller.Status, StringComparison.Ordinal);

        automation.Requests.Clear();
        Assert.Contains("6 step", controller.Command("slow")![0], StringComparison.Ordinal);
        controller.Tick(0d);
        Assert.Equal([(PluginAdvancementKind.Skill, 34u, 23UL)], automation.Requests);
        controller.Tick(0.1d);
        Assert.Single(automation.Requests);
        controller.Tick(0.25d);
        Assert.Equal(33UL, automation.Requests[1].Cost);
        for (int i = 0; i < 4; i++)
            controller.Tick(0.3d);
        Assert.Equal([23UL, 33UL, 41UL, 52UL, 62UL, 71UL], automation.Requests.Select(r => r.Cost).ToArray());
        controller.Tick(0.3d);
        Assert.False(controller.IsRunning);
    }

    /// <summary>
    /// The plan is built from banked experience, not luminance: the two sit
    /// in different rows of the character's 64-bit table. Mutation: reading
    /// the luminance row tells a character with 300 experience and no
    /// luminance that there is nothing to level, and builds a plan the
    /// server refuses for one with luminance and no experience.
    /// </summary>
    [Fact]
    public void PlanReadsExperienceNotLuminance()
    {
        var automation = new FakeAutomation { Unassigned = 300, Luminance = 0 };
        ExperienceSpendController controller = Controller(automation, macroEnabled: () => false);
        Assert.Contains("1 step", controller.Command("level")![0], StringComparison.Ordinal);
        controller.Tick(0d);
        Assert.Equal([(PluginAdvancementKind.Skill, 34u, 282UL)], automation.Requests);

        var rich = new FakeAutomation { Unassigned = 0, Luminance = 5_000 };
        ExperienceSpendController idle = Controller(rich, macroEnabled: () => false);
        idle.Command("level");
        idle.Tick(0d);
        Assert.Empty(rich.Requests);
    }

    /// <summary>
    /// Attributes and pools are named by the id the host's own record
    /// carries, with the catalogue's id only when the record has none.
    /// Mutation: always using the catalogue's id names the wrong pool on a
    /// host that numbers them differently.
    /// </summary>
    [Fact]
    public void StatIdsComeFromTheHostRecordWithTheCatalogueAsFallback()
    {
        var automation = new FakeAutomation
        {
            Unassigned = 200,
            HealthStatId = 42u,
            StrengthStatId = 0u,
            WarMagicTraining = PluginSkillTraining.Untrained,
        };
        ExperienceSpendController controller = Controller(
            automation,
            macroEnabled: () => false,
            policy: ["Strength = 1", "Health = 1"]);

        controller.Command("slow");
        for (int i = 0; i < 3; i++)
            controller.Tick(0.3d);
        Assert.Equal(
            [(PluginAdvancementKind.Vital, 42u, 73UL), (PluginAdvancementKind.Attribute, 1u, 110UL)],
            automation.Requests);
    }

    /// <summary>
    /// The spender refuses to start while the macro is enabled, says so,
    /// and stops if the macro comes on mid-run.
    /// </summary>
    [Fact]
    public void RefusesWhileTheMacroIsEnabled()
    {
        var automation = new FakeAutomation { Unassigned = 300 };
        bool macro = true;
        ExperienceSpendController controller = Controller(automation, macroEnabled: () => macro);

        Assert.Contains("macro", controller.Command("level")![0], StringComparison.OrdinalIgnoreCase);
        Assert.False(controller.IsRunning);
        Assert.Empty(automation.Requests);

        macro = false;
        controller.Command("slow");
        controller.Tick(0d);
        Assert.Single(automation.Requests);
        macro = true;
        controller.Tick(0.3d);
        Assert.False(controller.IsRunning);
        Assert.Single(automation.Requests);
        Assert.Contains("macro", controller.Status, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// A refusal from the client ends the run with its reason; a second
    /// level command while running halts the run.
    /// </summary>
    [Fact]
    public void ARefusalStopsTheRunAndASecondCommandHaltsIt()
    {
        var automation = new FakeAutomation
        {
            Unassigned = 300,
            Answer = new PluginAdvancementResult(PluginAdvancementStatus.Refused, "not now"),
        };
        ExperienceSpendController controller = Controller(automation, macroEnabled: () => false);

        controller.Command("slow");
        controller.Tick(0d);
        Assert.Single(automation.Requests);
        Assert.False(controller.IsRunning);
        Assert.Contains("not now", controller.Status, StringComparison.Ordinal);

        automation.Answer = new PluginAdvancementResult(PluginAdvancementStatus.Sent, null!);
        controller.Command("slow");
        Assert.True(controller.IsRunning);
        automation.Messages.Clear();
        Assert.Empty(controller.Command("level")!);
        Assert.False(controller.IsRunning);
        // The reference's stop says it is stopping, then how far it got.
        Assert.Equal(
            ["[UB] Stopping AutoXp.", "[UB] Finished leveling 0 targets."],
            automation.Messages);
    }

    /// <summary>
    /// The read-only verbs: no verb lists the weights in use, test prints
    /// the plan per target, export writes the policy in the paste-able
    /// form, and import rewrites the policy row and reports what it could
    /// not read.
    /// </summary>
    [Fact]
    public void TestExportAndImportSpeakTheReferenceFormat()
    {
        var automation = new FakeAutomation { Unassigned = 300 };
        var lines = new List<string>(Policy);
        ExperienceSpendController controller = Controller(
            automation,
            macroEnabled: () => false,
            policy: lines);

        IReadOnlyList<string> weights = controller.Command("")!;
        Assert.Contains(weights, l => l.Contains("WarMagic: 10", StringComparison.Ordinal));
        Assert.Contains(controller.Command("test")!, l => l.Contains("WarMagic: 6 levels for 282 xp", StringComparison.Ordinal));
        Assert.Equal("[UB] Strength=1;Health=1.4;WarMagic=10", controller.Command("export")![0]);

        IReadOnlyList<string> imported = controller.Command("import Alchemy=2;Bogus=1;WarMagic=0")!;
        Assert.Contains(imported, l => l.Contains("Bogus", StringComparison.Ordinal));
        Assert.Equal(["Strength = 1", "Health = 1.4", "WarMagic = 0", "Alchemy = 2"], lines);
        Assert.Equal("[UB] Strength=1;Health=1.4;WarMagic=0;Alchemy=2", controller.Command("export")![0]);
    }

    private static ExperienceSpendController Controller(
        FakeAutomation automation,
        Func<bool> macroEnabled,
        IList<string>? policy = null)
    {
        IList<string> lines = policy ?? new List<string>(Policy);
        var controller = new ExperienceSpendController(new FakeHost(automation), macroEnabled);
        controller.BindSettings(new ExperienceSpendSettings
        {
            StopBeforeMax = () => 10,
            TriesTimeMilliseconds = () => 300,
            MaxXpChunk = () => 1_000_000_000L,
            PolicyLines = () => lines.ToArray(),
            SetPolicyLines = updated =>
            {
                lines.Clear();
                foreach (string line in updated)
                    lines.Add(line);
            },
        });
        return controller;
    }

    private sealed class FakeAutomation : IAutomationSurface, ICharacterInfo, IWorldObjectAutomation, IPluginChat
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public IWorldObjectAutomation Objects => this;
        public IPluginChat Chat => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IItemAutomation Items => NoOpAutomationSurface.Instance;

        public long Unassigned { get; set; }
        public long Luminance { get; set; }
        public uint HealthStatId { get; set; } = 1u;
        public uint StrengthStatId { get; set; } = 1u;
        public PluginSkillTraining WarMagicTraining { get; set; } = PluginSkillTraining.Specialized;
        public PluginAdvancementResult Answer { get; set; } = new(PluginAdvancementStatus.Sent, null!);
        public List<(PluginAdvancementKind Kind, uint StatId, ulong Cost)> Requests { get; } = [];
        public List<string> Messages { get; } = [];

        public bool IsInWorld => true;
        public string Name => "Acdream";
        public string WorldName => "Coldeve";
        public uint ObjectId => 1u;
        public uint CurrentHealth => 0u;
        public uint MaxHealth => 0u;
        public uint CurrentStamina => 0u;
        public uint MaxStamina => 0u;
        public uint CurrentMana => 0u;
        public uint MaxMana => 0u;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes =>
        [
            new(0, "Strength", 100u) { StatId = StrengthStatId, Ranks = 0u },
            new(1, "Endurance", 100u) { StatId = 2u, Ranks = 5u },
        ];
        public IReadOnlyList<PluginVitalInfo> Vitals => [Health()];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];

        private PluginVitalInfo Health() =>
            new(0, "Health", 100u, 100u) { StatId = HealthStatId, Ranks = 0u };

        public bool TryGetVital(int kind, out PluginVitalInfo vital)
        {
            vital = kind == 0 ? Health() : default;
            return kind == 0;
        }

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            if (skillId == 34u)
            {
                skill = new PluginSkillInfo(34u, "War Magic", WarMagicTraining, 100u) { Ranks = 0u };
                return true;
            }
            skill = default;
            return false;
        }

        public PluginAdvancementResult RequestAdvancement(PluginAdvancementKind kind, uint statId, ulong cost)
        {
            Requests.Add((kind, statId, cost));
            return Answer;
        }

        bool IWorldObjectAutomation.IsAvailable => true;
        public IReadOnlyList<PluginWorldObject> CaptureObjects() => [];
        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            value = default;
            return false;
        }
        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            properties = new PluginItemProperties(
                new Dictionary<uint, int>(),
                new Dictionary<uint, long> { [2u] = Unassigned, [6u] = Luminance },
                new Dictionary<uint, bool>(),
                new Dictionary<uint, double>(),
                new Dictionary<uint, string>(),
                new Dictionary<uint, uint>(),
                new Dictionary<uint, uint>());
            return objectId == ObjectId;
        }

        public void PostSystemMessage(string text) => Messages.Add(text);
        public bool Submit(string text) => true;
    }

    private sealed class FakeHost(FakeAutomation automation) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new FakeLogger();
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage { get; } = new MemoryStorage();
        public IAutomationSurface Automation => automation;
        public IPluginStorage VtankProfiles => Storage;
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? ReadText(string key) => _text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content) => _text[key] = content;
        public bool Delete(string key) => _text.Remove(key);
    }

    private sealed class FakeLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class FakeState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class FakeEvents : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned { add { } remove { } }
        public event Action<double> Tick { add { } remove { } }
    }

    private sealed class FakeSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed { add { } remove { } }
        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }
}
