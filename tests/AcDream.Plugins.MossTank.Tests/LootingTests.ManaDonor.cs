using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The mana donor job end to end: an item is only taken off a corpse for its
/// mana when there is an empty stone to put that mana in, and it is emptied
/// into that stone straight away -- the drain destroys it, which is what
/// gives the pack slot back. See the research note on worn-mana upkeep.
/// </summary>
public sealed partial class LootingTests
{
    private const uint DrainCorpse = 0x7000A001u;
    private const uint DrainStone = 0x7000A010u;
    private const uint DrainDonor = 0x7000A011u;

    /// <summary>An empty stone the profile knows by name.</summary>
    private static PluginInventoryItem EmptyStone(uint id = DrainStone) =>
        Item(id, "Major Mana Stone", 47u) with
        {
            ObjectClass = PluginObjectClass.ManaStone,
        };

    /// <summary>An item worth emptying: it holds mana and nobody made it theirs.</summary>
    private static PluginInventoryItem Donor(
        uint id = DrainDonor,
        string name = "Sturdy Wand",
        int mana = 1500) =>
        Item(id, name, 48u) with
        {
            Effects = 1u,
            ItemCurrentMana = mana,
            Workmanship = 5f,
        };

    private static Automation DrainWorld(
        IReadOnlyList<PluginInventoryItem> owned,
        IReadOnlyList<PluginInventoryItem> corpseContents) => new()
        {
            Corpses =
            [
                new PluginLootContainer(
                    DrainCorpse, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
            Owned = owned,
            Contents = corpseContents,
            ManaDrainConsumesDonor = true,
        };

    private static LootSettings DrainSettings()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ManaStoneLootCount = 4,
            ManaTankMinimumMana = 1000,
        };
        // A rule file that says nothing about wands or stones: the mana job is
        // the one the macro takes on by itself, outside the rules.
        settings.Rules.Add(VtankRule(
            "Leave junk", LootAction.NoLoot, Requirement(1, "Junk", "1")));
        return settings;
    }

    private static LootController DrainController(
        Automation automation,
        ActionLockTable locks,
        List<string>? logged = null)
    {
        var controller = new LootController(
            new Host(automation),
            DrainSettings(),
            new HashSet<string>(StringComparer.Ordinal) { "Major Mana Stone" },
            new Dictionary<string, ConsumableCategory>(StringComparer.Ordinal)
            {
                ["Major Mana Stone"] = ConsumableCategory.ManaStone,
            });
        controller.BindActionLocks(locks);
        if (logged is not null)
            controller.Log = (_, line) => logged.Add(line);
        return controller;
    }

    /// <summary>The client answers the one description it was asked for.</summary>
    private static void AnswerDescription(Automation automation)
    {
        uint asked = automation.AppraisalState.AwaitingObjectId;
        if (asked == 0u)
            return;
        automation.AppraisalState = automation.AppraisalState with
        {
            Revision = automation.AppraisalState.Revision + 1,
            AwaitingObjectId = 0u,
            CurrentObjectId = asked,
        };
    }

    /// <summary>
    /// One pass of everything around the looter: the slot clock moves on, the
    /// client answers the outstanding description, the rule takes its turn,
    /// and whatever was pulled out of the corpse turns up in the pack.
    /// </summary>
    private static void DrainPass(
        Automation automation,
        LootController controller,
        ActionLockTable locks,
        double seconds = 0.5d)
    {
        locks.Advance(seconds);
        controller.TickIdentification(seconds);
        AnswerDescription(automation);
        controller.TickIdentification(seconds);
        int before = automation.Picked.Count;
        controller.Tick(seconds, canAct: true);
        automation.SettleManaDrains();
        for (int index = before; index < automation.Picked.Count; index++)
        {
            uint pulled = automation.Picked[index];
            PluginInventoryItem taken = automation.Contents.FirstOrDefault(
                item => item.ObjectId == pulled);
            if (taken.ObjectId == 0u)
                continue;
            automation.Contents = automation.Contents
                .Where(item => item.ObjectId != pulled)
                .ToArray();
            automation.Owned = [.. automation.Owned, taken];
            automation.InventoryCompletion = new PluginInventoryCompletion(
                automation.InventoryCompletion.Revision + 1,
                PluginInventoryCommandKind.Pickup,
                pulled,
                0u);
        }
    }

    /// <summary>Opens the staged corpse and works it through.</summary>
    private static void WorkTheCorpse(
        Automation automation,
        LootController controller,
        ActionLockTable locks,
        int passes = 8)
    {
        Assert.True(controller.Tick(1d, canAct: true));
        automation.Requested = DrainCorpse;
        automation.Current = DrainCorpse;
        Assert.True(controller.ObserveCorpseOpened());
        for (int pass = 0; pass < passes; pass++)
            DrainPass(automation, controller, locks);
    }

    /// <summary>
    /// The precondition for taking an item for its mana is somewhere to put
    /// that mana: an empty stone in the pack. A stone that is already charged
    /// is no place to put anything, so the item is left where it lies rather
    /// than carried around in the hope a stone turns up.
    ///
    /// Mutation: count charged stones as spare and the donor is taken with
    /// nothing to drain it into.
    /// </summary>
    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void ADonorIsOnlyTakenWhenAnEmptyStoneIsCarried(bool stoneIsEmpty)
    {
        PluginInventoryItem stone = stoneIsEmpty
            ? EmptyStone()
            : EmptyStone() with { Effects = 1u };
        Automation automation = DrainWorld([stone], [Donor()]);
        var locks = new ActionLockTable();
        var logged = new List<string>();
        LootController controller = DrainController(automation, locks, logged);

        WorkTheCorpse(automation, controller, locks);

        if (stoneIsEmpty)
        {
            Assert.Equal([DrainDonor], automation.Picked);
            Assert.Contains(
                "LootDecision: Sturdy Wand -> ManaTank (ManaTank)",
                logged);
        }
        else
        {
            Assert.Empty(automation.Picked);
            Assert.Empty(automation.Applied);
        }
    }

    /// <summary>
    /// Whether a stone is empty is something the client says about the object
    /// itself, not something an appraisal has to establish, so a stone nobody
    /// has ever appraised is a perfectly good place to put mana. Requiring an
    /// appraisal of it is what left a run's worth of donors sitting in the
    /// pack undrained: the stones were never looked at, so the drain never
    /// found one.
    ///
    /// Mutation: require the stone to have been appraised and no drain is
    /// ever issued.
    /// </summary>
    [Fact]
    public void AStoneNobodyHasAppraisedIsStillSomewhereToPutTheMana()
    {
        Automation automation = DrainWorld([EmptyStone()], [Donor()]);
        automation.Unassessed.Add(DrainStone);
        var locks = new ActionLockTable();
        LootController controller = DrainController(automation, locks);

        WorkTheCorpse(automation, controller, locks);

        Assert.Equal([(DrainStone, DrainDonor)], automation.Applied);
        Assert.DoesNotContain(
            automation.Owned,
            item => item.ObjectId == DrainDonor);
    }

    /// <summary>
    /// The drain is an item use like any other: it waits while another rule
    /// holds the item slot, and it holds the slot itself while the server is
    /// answering for it, so nothing else puts an item in the character's
    /// hands inside that window.
    ///
    /// Mutation: leave the slot unarmed after the use and the second half of
    /// this fails -- the attack, the buff and the next pull all act inside
    /// the drain's own window.
    /// </summary>
    [Fact]
    public void TheDrainWaitsForTheItemSlotAndHoldsItWhileItRuns()
    {
        Automation automation = DrainWorld([EmptyStone()], [Donor()]);
        var locks = new ActionLockTable();
        LootController controller = DrainController(automation, locks);

        // The corpse is worked until the donor is in the pack, and no
        // further: the drain is what the rest of this test is about.
        Assert.True(controller.Tick(1d, canAct: true));
        automation.Requested = DrainCorpse;
        automation.Current = DrainCorpse;
        Assert.True(controller.ObserveCorpseOpened());
        for (int pass = 0;
            pass < 8 && !controller.ClassifiedOwnedItems.ContainsKey(DrainDonor);
            pass++)
        {
            DrainPass(automation, controller, locks);
        }
        Assert.Equal([DrainDonor], automation.Picked);

        // Somebody else is mid-use: the drain waits its turn.
        locks.Arm(ActionLockKind.ItemUse, 60d);
        for (int pass = 0; pass < 4; pass++)
            controller.Tick(0.2d, canAct: true);
        Assert.Empty(automation.Applied);

        locks.Release(ActionLockKind.ItemUse);
        controller.Tick(0.2d, canAct: true);

        Assert.Equal([(DrainStone, DrainDonor)], automation.Applied);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));
    }

    /// <summary>
    /// A use the client will not start is answered once. A refusal says this
    /// item cannot be drained -- it stops being one this run means to empty,
    /// which both ends the retry and gives the stone it was holding back to
    /// the next donor. Repeating it every pass is how a single bad item
    /// spent a whole run's passes.
    ///
    /// Mutation: leave the classification in place on a refusal and the same
    /// use goes out on every pass for ever.
    /// </summary>
    [Fact]
    public void ARefusedDrainIsNotRepeated()
    {
        Automation automation = DrainWorld([EmptyStone()], [Donor()]);
        automation.ApplyResults.Enqueue(
            new PluginItemCommandResult(PluginItemCommandStatus.InvalidItem));
        var locks = new ActionLockTable();
        var logged = new List<string>();
        LootController controller = DrainController(automation, locks, logged);

        WorkTheCorpse(automation, controller, locks);
        for (int pass = 0; pass < 6; pass++)
            DrainPass(automation, controller, locks);

        Assert.Single(automation.Applied);
        Assert.DoesNotContain(DrainDonor, controller.ClassifiedOwnedItems.Keys);
        Assert.Contains(logged, line => line.StartsWith(
            "ManaDrain: could not empty Sturdy Wand", StringComparison.Ordinal));
    }

    /// <summary>
    /// A client that is busy this instant has not refused anything: the use
    /// is asked again on the next pass and goes through.
    /// </summary>
    [Fact]
    public void ABusyClientOnlyDelaysTheDrain()
    {
        Automation automation = DrainWorld([EmptyStone()], [Donor()]);
        automation.ApplyResults.Enqueue(
            new PluginItemCommandResult(PluginItemCommandStatus.Busy));
        var locks = new ActionLockTable();
        LootController controller = DrainController(automation, locks);

        WorkTheCorpse(automation, controller, locks);
        for (int pass = 0; pass < 4; pass++)
            DrainPass(automation, controller, locks);

        Assert.Equal(
            [(DrainStone, DrainDonor), (DrainStone, DrainDonor)],
            automation.Applied);
        Assert.DoesNotContain(
            automation.Owned,
            item => item.ObjectId == DrainDonor);
    }

    /// <summary>
    /// One stone takes one donor. A second item worth draining, seen on the
    /// same corpse while the first is still on its way into the stone, is
    /// left behind: the stone is already spoken for, and taking both would
    /// fill the pack with mana nothing can drink.
    ///
    /// Mutation: count the stone as spare while a donor is already queued
    /// against it and both wands are taken.
    /// </summary>
    [Fact]
    public void OneStoneTakesOneDonorAndTheOtherIsLeft()
    {
        const uint second = 0x7000A012u;
        Automation automation = DrainWorld(
            [EmptyStone()],
            [Donor(), Donor(second, "Heavy Wand", 1800)]);
        var locks = new ActionLockTable();
        LootController controller = DrainController(automation, locks);

        WorkTheCorpse(automation, controller, locks);

        Assert.Single(automation.Picked);
        Assert.Equal([(DrainStone, DrainDonor)], automation.Applied);
    }

    /// <summary>
    /// The drain says what it did on the same channel as the rest of the
    /// looting. A destructive use with no line in the loot log is a run that
    /// cannot be read back: items vanish from the pack with nothing to say
    /// where they went.
    ///
    /// Mutation: log the drain to the plugin log only and neither line is on
    /// the channel.
    /// </summary>
    [Fact]
    public void TheDrainSpeaksOnTheLootChannel()
    {
        Automation automation = DrainWorld([EmptyStone()], [Donor()]);
        var locks = new ActionLockTable();
        var logged = new List<string>();
        LootController controller = DrainController(automation, locks, logged);

        WorkTheCorpse(automation, controller, locks);

        Assert.Contains(
            "ManaDrain: emptying Sturdy Wand into Major Mana Stone",
            logged);
        Assert.Contains(
            "ManaDrain: filled Major Mana Stone from Sturdy Wand",
            logged);
    }
}
