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

    /// <summary>
    /// A mana stone is picked up outside the rule file only when the
    /// profile's helper list names it as a mana stone. The same name filed
    /// under any other kind — a mana charge, food, a kit — wants no stone at
    /// all, however high the stone count is set, because nothing has told the
    /// macro that this item is a stone it should carry.
    ///
    /// Mutation: make the membership test read the helper names without their
    /// kinds and the charge row is looted as a stone.
    /// </summary>
    [Theory]
    [InlineData(true, "ManaStone (ManaStone)")]
    [InlineData(false, "no rule matched")]
    public void OnlyAHelperRowOfTheStoneKindMakesTheLooterTakeAStone(
        bool filedAsAStone,
        string expected)
    {
        ConsumableCategory kind = filedAsAStone
            ? ConsumableCategory.ManaStone
            : ConsumableCategory.ManaFood;
        const uint corpse = 0x70009101u;
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
        };
        var settings = new LootSettings
        {
            Enabled = true,
            ManaStoneLootCount = 4,
        };
        // A rule file that says nothing about stones: the stone job is the
        // one the macro takes on by itself, outside the rules.
        settings.Rules.Add(VtankRule(
            "Leave junk", LootAction.NoLoot, Requirement(1, "Junk", "1")));
        var logged = new List<string>();
        var controller = new LootController(
            new Host(automation),
            settings,
            new HashSet<string>(StringComparer.Ordinal) { "Major Mana Stone" },
            new Dictionary<string, ConsumableCategory>(StringComparer.Ordinal)
            {
                ["Major Mana Stone"] = kind,
            })
        {
            Log = (_, line) => logged.Add(line),
        };
        controller.BindActionLocks(new ActionLockTable());

        Assert.True(controller.Tick(1d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents =
        [
            Item(0x70009110u, "Major Mana Stone", 47u) with
            {
                ObjectClass = PluginObjectClass.ManaStone,
            },
        ];
        Assert.True(controller.ObserveCorpseOpened());
        controller.TickIdentification(0.5d);

        Assert.Contains($"LootDecision: Major Mana Stone -> {expected}", logged);
    }

    /// <summary>
    /// The whole drain, through the looter: a charged item found on a corpse
    /// while a spare stone is held is taken for its mana and emptied into the
    /// stone -- unless somebody has written on it, in which case it is taken
    /// and left alone.
    ///
    /// Mutation: drop the inscription check from the drain and the written
    /// case issues the same use as the plain one.
    /// </summary>
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ACorpseDonorIsDrainedIntoASpareStoneUnlessSomebodyWroteOnIt(
        bool inscribed)
    {
        const uint corpse = 0x70009201u;
        const uint stone = 0x70009210u;
        const uint donor = 0x70009211u;
        PluginInventoryItem spare = Item(stone, "Major Mana Stone", 47u) with
        {
            ObjectClass = PluginObjectClass.ManaStone,
        };
        PluginInventoryItem wand = Item(donor, "Sturdy Wand", 48u) with
        {
            Effects = 1u,
            ItemCurrentMana = 1500,
            Workmanship = 5f,
        };
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
            Owned = [spare],
        };
        if (inscribed)
        {
            automation.ItemProperties[donor] = new PluginItemProperties(
                new Dictionary<uint, int>(),
                new Dictionary<uint, long>(),
                new Dictionary<uint, bool>(),
                new Dictionary<uint, double>(),
                new Dictionary<uint, string> { [7u] = "For Horan" },
                new Dictionary<uint, uint>(),
                new Dictionary<uint, uint>());
        }
        var settings = new LootSettings
        {
            Enabled = true,
            ManaStoneLootCount = 4,
            ManaTankMinimumMana = 1000,
        };
        settings.Rules.Add(VtankRule(
            "Leave junk", LootAction.NoLoot, Requirement(1, "Junk", "1")));
        var controller = new LootController(
            new Host(automation),
            settings,
            new HashSet<string>(StringComparer.Ordinal) { "Major Mana Stone" },
            new Dictionary<string, ConsumableCategory>(StringComparer.Ordinal)
            {
                ["Major Mana Stone"] = ConsumableCategory.ManaStone,
            });

        Assert.True(controller.Tick(1d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [wand];
        controller.TickIdentification(0.5d);
        Assert.Equal([donor], automation.Identified);

        automation.AppraisalState = new PluginAppraisalState(1, 0u, donor);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([donor], automation.Picked);

        automation.Contents = [];
        automation.Owned = [spare, wand];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1, PluginInventoryCommandKind.Pickup, donor, 0u);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(LootAction.ManaTank, controller.ClassifiedOwnedItems[donor]);

        // The corpse is done; the drain is what is left to do.
        for (int pass = 0; pass < 4 && automation.Applied.Count == 0; pass++)
            controller.Tick(0.2d, canAct: true);

        Assert.Equal(
            inscribed ? [] : new[] { (stone, donor) },
            automation.Applied);
    }
}
