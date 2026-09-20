using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// What the looter does when the client answers a pull with something other
/// than "sent": a pack with no room left, or the client's own
/// one-request-at-a-time gate. Both used to be read as "sent", and when the
/// client started answering honestly a refused pull was re-issued on every
/// heartbeat until the item's whole attempt budget was gone.
/// </summary>
public sealed partial class LootingTests
{
    /// <summary>A corpse open in front of the character with one item in it, ready to pull.</summary>
    private sealed record PullStage(
        Automation Automation,
        LootController Controller,
        ActionLockTable Locks,
        List<string> Log);

    private static PullStage StageOpenCorpse(
        uint corpseId,
        PluginInventoryItem item,
        LootSettings settings) =>
        StageOpenCorpse(corpseId, [item], settings, new Automation
        {
            Corpses = [ChainCorpse(corpseId)],
        });

    private static PullStage StageOpenCorpse(
        uint corpseId,
        IReadOnlyList<PluginInventoryItem> items,
        LootSettings settings,
        Automation automation)
    {
        var log = new List<string>();
        var locks = new ActionLockTable();
        var controller = new LootController(new Host(automation), settings)
        {
            Log = (channel, message) =>
            {
                Assert.Equal(MacroLogChannel.Loot, channel);
                log.Add(message);
            },
        };
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpseId], automation.Opened);
        automation.Requested = corpseId;
        automation.Current = corpseId;
        Assert.True(controller.ObserveCorpseOpened());
        automation.Contents = items;
        controller.TickIdentification(0.5d);
        return new PullStage(automation, controller, locks, log);
    }

    /// <summary>An ordinary item that takes a slot of its own.</summary>
    private static PluginInventoryItem Loose(
        uint id,
        string name,
        uint wcid,
        int slot) =>
        Item(id, name, wcid) with { ContainerSlot = slot };

    private static IEnumerable<string> Lines(PullStage stage, string fragment) =>
        stage.Log.Where(line =>
            line.Contains(fragment, StringComparison.Ordinal));

    /// <summary>
    /// A pull the client would not send paces exactly like one it did: the
    /// item slot and navigation are held for the same three quarters of a
    /// second. Mutation: return out of the refused branch before the two
    /// Arm calls and the pull goes out again on the very next heartbeat.
    /// </summary>
    [Fact]
    public void ARefusedPullHoldsTheItemSlotLikeAnAcceptedOne()
    {
        const uint corpse = 0x70001201u;
        const uint coin = 0x70001202u;
        PullStage stage = StageOpenCorpse(
            corpse,
            Item(coin, "Colosseum coin", 77u),
            ChainSettings());
        stage.Automation.PickupResults.Enqueue(new(
            PluginItemCommandStatus.Refused,
            "no pack the player has open has room for it"));

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));

        Assert.Equal([coin], stage.Automation.Picked);
        Assert.True(stage.Locks.IsLocked(ActionLockKind.ItemUse));
        Assert.True(stage.Locks.IsLocked(ActionLockKind.Navigation));

        // And the pass really is held by it: the very next turn takes nothing.
        Assert.True(stage.Controller.Tick(0.1d, canAct: true));
        Assert.Equal([coin], stage.Automation.Picked);
    }

    /// <summary>
    /// A pull the client refuses ends the corpse, with one line for it. The
    /// pack is no emptier a tenth of a second later, so asking again only
    /// spends the item's attempts and re-prints the client's own notice.
    /// Mutation: keep the old "return busy?" answer and the coin is pulled
    /// again every turn until its attempts run out.
    /// </summary>
    [Fact]
    public void APullTheClientRefusesEndsTheCorpseWithOneLine()
    {
        const uint corpse = 0x70001211u;
        const uint coin = 0x70001212u;
        LootSettings settings = ChainSettings();
        settings.BlacklistCorpseOpenTimeoutSeconds = 1d;
        PullStage stage = StageOpenCorpse(
            corpse,
            Item(coin, "Colosseum coin", 77u),
            settings);
        stage.Automation.PickupResults.Enqueue(new(
            PluginItemCommandStatus.Refused,
            "no pack the player has open has room for it"));

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], stage.Automation.Picked);
        string given = Assert.Single(Lines(stage, "giving up on"));
        Assert.Contains("no pack", given, StringComparison.Ordinal);

        // The corpse is closed rather than worked again.
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], stage.Automation.Closed);
        Assert.Equal([coin], stage.Automation.Picked);
        Assert.Single(Lines(stage, "giving up on"));
    }

    /// <summary>
    /// The corpse was set aside, not recorded as looted: nothing was taken
    /// from it, so once the skip period is over it is opened again. Mutation:
    /// mark the corpse complete instead of setting it aside and it is never
    /// opened a second time.
    /// </summary>
    [Fact]
    public void ACorpseGivenUpOnIsNotRecordedAsLooted()
    {
        const uint corpse = 0x70001221u;
        const uint coin = 0x70001222u;
        LootSettings settings = ChainSettings();
        settings.BlacklistCorpseOpenTimeoutSeconds = 1d;
        PullStage stage = StageOpenCorpse(
            corpse,
            Item(coin, "Colosseum coin", 77u),
            settings);
        stage.Automation.PickupResults.Enqueue(new(
            PluginItemCommandStatus.Refused,
            "no pack the player has open has room for it"));

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], stage.Automation.Closed);

        // The container is shut and the skip period runs out.
        stage.Automation.Requested = 0u;
        stage.Automation.Current = 0u;
        stage.Automation.Contents = [];
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(2d, canAct: true));

        Assert.Equal([corpse, corpse], stage.Automation.Opened);
    }

    /// <summary>
    /// Busy is the client's own one-request-at-a-time gate: it never looked
    /// at the item, so the item is charged nothing for it. Mutation: count
    /// the attempt before reading the status and the coin is dropped out of
    /// the pass after two busy answers.
    /// </summary>
    [Fact]
    public void ABusyPullCostsTheItemNoneOfItsAttempts()
    {
        const uint corpse = 0x70001231u;
        const uint coin = 0x70001232u;
        LootSettings settings = ChainSettings();
        settings.CorpseLootItemMaxAttempts = 2;
        PullStage stage = StageOpenCorpse(
            corpse,
            Item(coin, "Colosseum coin", 77u),
            settings);
        for (int busy = 0; busy < 3; busy++)
        {
            stage.Automation.PickupResults.Enqueue(
                new(PluginItemCommandStatus.Busy));
        }

        for (int turn = 0; turn < 3; turn++)
        {
            Assert.True(stage.Controller.Tick(0.3d, canAct: true));
            stage.Locks.Advance(1d);
        }
        Assert.Equal(3, stage.Automation.Picked.Count);

        // The fourth answer is "sent": the coin was still in the pass, three
        // busy answers later.
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal(4, stage.Automation.Picked.Count);
        Assert.Empty(Lines(stage, "abandoned"));
    }

    /// <summary>
    /// An item dropped out of the pass for spending its attempts says so,
    /// once. Mutation: delete the line from the attempt-ceiling skip in
    /// <c>ContinueCurrentCorpse</c> and the corpse closes with the item
    /// still in it and nothing said.
    /// </summary>
    [Fact]
    public void AnItemThatSpentItsAttemptsIsReportedOnce()
    {
        const uint corpse = 0x70001241u;
        const uint coin = 0x70001242u;
        LootSettings settings = ChainSettings();
        settings.CorpseLootItemMaxAttempts = 1;
        PullStage stage = StageOpenCorpse(
            corpse,
            Item(coin, "Colosseum coin", 77u),
            settings);

        // The pull goes out and the coin stays where it is.
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], stage.Automation.Picked);
        stage.Locks.Advance(1d);

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], stage.Automation.Picked);

        string dropped = Assert.Single(Lines(stage, "abandoned"));
        Assert.Contains("Colosseum coin", dropped, StringComparison.Ordinal);
    }

    /// <summary>
    /// An item still being waited on when the container shuts leaves a line
    /// too. Mutation: drop the reason from the corpse-closed branch and the
    /// run has no record of the coin at all.
    /// </summary>
    [Fact]
    public void AnItemLeftBehindWhenTheCorpseShutsIsReported()
    {
        const uint corpse = 0x70001251u;
        const uint coin = 0x70001252u;
        PullStage stage = StageOpenCorpse(
            corpse,
            Item(coin, "Colosseum coin", 77u),
            ChainSettings());

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], stage.Automation.Picked);

        // The container shuts with the coin still in it and nothing of it in
        // the character's own packs.
        stage.Automation.Requested = 0u;
        stage.Automation.Current = 0u;
        stage.Locks.Advance(1d);
        stage.Controller.Tick(0.3d, canAct: true);

        string left = Assert.Single(Lines(stage, "abandoned"));
        Assert.Contains("Colosseum coin", left, StringComparison.Ordinal);
        Assert.Contains("corpse closed", left, StringComparison.Ordinal);
    }

    /// <summary>
    /// One item the client will not take costs only that item: the pass
    /// carries on with what else is in the corpse, and the refused one is
    /// never asked for again. Mutation: give up on the whole corpse in the
    /// refused branch and the ring is left in it.
    /// </summary>
    [Fact]
    public void ARefusedItemIsRetiredAndTheCorpsesOtherItemsAreStillTaken()
    {
        const uint corpse = 0x70001261u;
        const uint anvil = 0x70001262u;
        const uint ring = 0x70001263u;
        PullStage stage = StageOpenCorpse(
            corpse,
            [Loose(anvil, "Anvil", 77u, 0), Loose(ring, "Gold Ring", 78u, 1)],
            ChainSettings(),
            new Automation { Corpses = [ChainCorpse(corpse)] });
        stage.Automation.PickupResults.Enqueue(new(
            PluginItemCommandStatus.Refused,
            "it is too heavy for you to carry"));

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil], stage.Automation.Picked);
        string line = Assert.Single(Lines(stage, "LootPickup: refused"));
        Assert.Contains("Anvil", line, StringComparison.Ordinal);
        Assert.Contains("too heavy", line, StringComparison.Ordinal);
        Assert.Empty(Lines(stage, "giving up on"));

        // The corpse's other item is worked next, and the anvil is never
        // asked for a second time however many turns the pass takes.
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil, ring], stage.Automation.Picked);
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil, ring, ring], stage.Automation.Picked);
        Assert.Single(Lines(stage, "LootPickup: refused"));
    }

    /// <summary>
    /// A corpse is given up on only once every one of its wanted items has
    /// been refused, with one line for it, and it is set aside rather than
    /// recorded as looted: nothing was taken from it. Mutation: mark it
    /// complete instead and it is never opened a second time.
    /// </summary>
    [Fact]
    public void ACorpseIsGivenUpOnOnlyWhenEveryItemHasBeenRefused()
    {
        const uint corpse = 0x70001271u;
        const uint anvil = 0x70001272u;
        const uint ring = 0x70001273u;
        LootSettings settings = ChainSettings();
        settings.BlacklistCorpseOpenTimeoutSeconds = 1d;
        PullStage stage = StageOpenCorpse(
            corpse,
            [Loose(anvil, "Anvil", 77u, 0), Loose(ring, "Gold Ring", 78u, 1)],
            settings,
            new Automation { Corpses = [ChainCorpse(corpse)] });
        for (int refusal = 0; refusal < 2; refusal++)
        {
            stage.Automation.PickupResults.Enqueue(new(
                PluginItemCommandStatus.Refused,
                "it is too heavy for you to carry"));
        }

        // The first refusal does not end the corpse; the second does.
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Empty(Lines(stage, "giving up on"));
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil, ring], stage.Automation.Picked);
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Single(Lines(stage, "giving up on"));
        Assert.Equal([corpse], stage.Automation.Closed);
        Assert.Equal([anvil, ring], stage.Automation.Picked);

        // The container shuts and the skip period runs out: the corpse was
        // set aside, not looted, so it is opened again.
        stage.Automation.Requested = 0u;
        stage.Automation.Current = 0u;
        stage.Automation.Contents = [];
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(2d, canAct: true));
        Assert.Equal([corpse, corpse], stage.Automation.Opened);
    }

    /// <summary>
    /// A pack with nowhere to put anything ends the corpse for everything
    /// that needs a slot -- but not for a stack that merges into one the
    /// character already holds, which needs none. Mutation: retire every
    /// remaining item and the pyreal is left in the corpse.
    /// </summary>
    [Fact]
    public void AFullPackStillTakesAStackThatMergesIntoOneAlreadyHeld()
    {
        const uint corpse = 0x70001281u;
        const uint anvil = 0x70001282u;
        const uint pyreal = 0x70001283u;
        const uint purse = 0x70001284u;
        var automation = new Automation
        {
            Corpses = [ChainCorpse(corpse)],
            Owned =
            [
                Item(purse, "Pyreal", 60u) with
                {
                    ContainerObjectId = Player,
                    StackSize = 5,
                    MaximumStackSize = 25,
                },
            ],
        };
        PullStage stage = StageOpenCorpse(
            corpse,
            [
                Loose(anvil, "Anvil", 77u, 0),
                Item(pyreal, "Pyreal", 60u) with
                {
                    ContainerSlot = 1,
                    StackSize = 3,
                    MaximumStackSize = 25,
                },
            ],
            ChainSettings(),
            automation);
        stage.Automation.PickupResults.Enqueue(new(
            PluginItemCommandStatus.Refused,
            "no pack the player has open has room for it"));

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil], stage.Automation.Picked);
        Assert.Empty(Lines(stage, "giving up on"));

        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil, pyreal], stage.Automation.Picked);
    }

    /// <summary>
    /// A pack with no free slot and nothing in the corpse that could merge
    /// ends the corpse on the spot rather than one refusal at a time: the
    /// ring is never asked for. The client wrote no notice here, so the
    /// free-slot count is what says the pack is full. Mutation: read only
    /// the notice and the ring is pulled on the next turn.
    /// </summary>
    [Fact]
    public void AFullPackEndsTheCorpseWithoutAskingForWhatCannotFit()
    {
        const uint corpse = 0x70001291u;
        const uint anvil = 0x70001292u;
        const uint ring = 0x70001293u;
        const uint held = 0x70001294u;
        var automation = new Automation
        {
            Corpses = [ChainCorpse(corpse)],
            ItemCapacity = 1,
            Owned = [Item(held, "Shield", 61u) with { ContainerObjectId = Player }],
        };
        PullStage stage = StageOpenCorpse(
            corpse,
            [Loose(anvil, "Anvil", 77u, 0), Loose(ring, "Gold Ring", 78u, 1)],
            ChainSettings(),
            automation);
        stage.Automation.PickupResults.Enqueue(
            new(PluginItemCommandStatus.Refused));

        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([anvil], stage.Automation.Picked);
        Assert.Single(Lines(stage, "giving up on"));

        // The corpse is closed, and the ring was never asked for.
        stage.Locks.Advance(1d);
        Assert.True(stage.Controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], stage.Automation.Closed);
        Assert.Equal([anvil], stage.Automation.Picked);
    }
}
