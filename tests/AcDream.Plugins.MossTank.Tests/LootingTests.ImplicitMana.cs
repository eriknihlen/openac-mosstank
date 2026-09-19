using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// What the looter does with the items no profile rule claims: the three
/// separate reasons it can leave one behind, and the two mana jobs it takes
/// on by itself. See the research note on worn-mana upkeep.
/// </summary>
public sealed partial class LootingTests
{
    /// <summary>
    /// Leaving an item behind has three causes and the log must tell them
    /// apart: nothing matched, a rule matched and said no, and a rule matched
    /// but the character already holds its limit. A single line for all three
    /// is what made a profile quietly sitting at its keep limit look like one
    /// whose rule never fired.
    ///
    /// Mutation: give <c>LootRefusal.Describe</c> one text for every kind and
    /// two of these three assertions fail.
    /// </summary>
    [Fact]
    public void ThreeReasonsForLeavingAnItemBehindReadDifferently()
    {
        const uint corpse = 0x70009001u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
            Owned =
            [
                Item(0x70009010u, "Treated Healing Kit", 44u) with
                {
                    StackSize = 7,
                },
            ],
        };
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(VtankRule(
            "Leave junk", LootAction.NoLoot, Requirement(1, "Junk", "1")));
        LootRule kits = VtankRule(
            "Kits", LootAction.KeepUpTo, Requirement(1, "Healing Kit", "1"));
        kits.KeepCount = 5;
        settings.Rules.Add(kits);
        var logged = new List<string>();
        var controller = new LootController(new Host(automation), settings)
        {
            Log = (_, line) => logged.Add(line),
        };
        controller.BindActionLocks(new ActionLockTable());

        Assert.True(controller.Tick(1d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents =
        [
            Item(0x70009011u, "Rusty Junk", 45u),
            Item(0x70009012u, "Treated Healing Kit", 44u),
            Item(0x70009013u, "Mystery Trinket", 46u),
        ];
        Assert.True(controller.ObserveCorpseOpened());
        controller.TickIdentification(0.5d);

        Assert.Contains(
            "LootDecision: Rusty Junk -> Leave junk says leave it",
            logged);
        Assert.Contains(
            "LootDecision: Treated Healing Kit -> Kits keeps up to 5 (holding 7)",
            logged);
        Assert.Contains(
            "LootDecision: Mystery Trinket -> no rule matched",
            logged);
    }
}
