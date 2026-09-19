using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class ItemManaRechargeTests
{
    [Fact]
    public void PlannerUsesProfiledManaChargeOnMostDepletedWornItem()
    {
        var names = Kinds(("Mana Charge", ConsumableCategory.ManaSource));
        PluginInventoryItem planTarget = Item(20, "Low Wand") with
        {
            EquippedLocation = 0x01000000u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        ItemManaRechargePlan plan = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                [
                    Item(10, "Mana Charge", 0x00080000u) with
                    {
                        ItemCurrentMana = 100,
                        Effects = 0x00000001u,
                    },
                    planTarget,
                    Item(21, "Other Wand") with
                    {
                        EquippedLocation = 0x01000000u,
                        ItemCurrentMana = 20,
                        ItemMaximumMana = 100,
                    },
                ],
                names,
                thresholdPercent: 33,
                sourceMissing: out _));

        Assert.Equal(10u, plan.ChargeObjectId);
        Assert.Equal(20u, plan.TargetObjectId);
        Assert.Equal(10, plan.CurrentMana);
    }

    [Fact]
    public void TheOldestQueuedWornItemIsChargedFirstNotTheMostDepleted()
    {
        var names = Kinds(("Mana Charge", ConsumableCategory.ManaSource));
        PluginInventoryItem[] inventory =
        [
            Item(10, "Mana Charge", 0x00080000u) with
            {
                ItemCurrentMana = 100,
                Effects = 0x00000001u,
            },
            Item(20, "Low Wand") with
            {
                EquippedLocation = 0x01000000u,
                ItemCurrentMana = 10,
                ItemMaximumMana = 100,
            },
            Item(21, "Older Wand") with
            {
                EquippedLocation = 0x02000000u,
                ItemCurrentMana = 20,
                ItemMaximumMana = 100,
            },
        ];

        ItemManaRechargePlan queued = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                inventory,
                names,
                thresholdPercent: 33,
                sourceMissing: out _,
                wieldOrder: [21u, 20u]));
        Assert.Equal(21u, queued.TargetObjectId);

        ItemManaRechargePlan depleted = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                inventory, names, thresholdPercent: 33, sourceMissing: out _));
        Assert.Equal(20u, depleted.TargetObjectId);
    }

    [Fact]
    public void PlannerRequiresProfileMembershipAndBelowThreshold()
    {
        PluginInventoryItem charge = Item(10, "Mana Charge", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0x00000001u,
        };
        PluginInventoryItem wand = Item(20, "Wand") with
        {
            EquippedLocation = 0x01000000u,
            ItemCurrentMana = 34,
            ItemMaximumMana = 100,
        };

        // Nothing in the profile names the charge: there is a want and no
        // source.
        Assert.Null(ItemManaRechargePlanner.Plan(
            [charge, wand with { ItemCurrentMana = 10 }],
            Kinds(),
            33,
            out bool missing));
        Assert.True(missing);
        // The charge is named, but the wand is above the threshold, so
        // nothing is wanted and no source is looked for.
        Assert.Null(ItemManaRechargePlanner.Plan(
            [charge, wand],
            Kinds(("Mana Charge", ConsumableCategory.ManaSource)),
            33,
            out missing));
        Assert.False(missing);
    }

    /// <summary>
    /// A worn-mana refill applies the charge to the player, because that is the
    /// command that distributes mana across equipped items. After a successful
    /// receipt, the old positive source appraisal must not schedule another
    /// action, even when a new appraisal stamp arrives while its omitted
    /// current-mana property remains merged in the local object.
    /// Mutation: target the selected threshold item again, or remove either the
    /// used-charge quarantine or the current-mana-change check, and this test
    /// observes an armor target or a second application.
    /// </summary>
    [Fact]
    public void SuccessfulPlayerRefillCannotReuseStalePositiveCharge()
    {
        PluginInventoryItem charge = Item(10, "Mana Charge", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0x00000001u,
        };
        PluginInventoryItem missingCurrentMana = Item(19, "Unready Helm") with
        {
            EquippedLocation = 0x00000001u,
            ItemCurrentMana = 0,
            ItemMaximumMana = 100,
        };
        PluginInventoryItem thresholdItem = Item(20, "Low Gloves") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var surface = new Surface
        {
            Inventory = [charge, missingCurrentMana, thresholdItem],
        };
        surface.Assess(charge, 100, (107u, 100));
        surface.Assess(missingCurrentMana, 100, (108u, 100));
        surface.Assess(thresholdItem, 100, (107u, 10), (108u, 100));
        var profiles = new CombatSettings();
        profiles.ConsumableNames.Add(charge.Name);
        profiles.ConsumableCategories[charge.Name] = ConsumableCategory.ManaSource;
        var host = new Host(surface);
        var controller = new ItemManaRechargeController(
            host,
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            profiles);

        // Worn gear is asked about first: what the client already holds about
        // an item this owner never had appraised does not date its mana.
        // One question at a time, and the gear takes its turn.
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal("Waiting for item assessment", controller.Status);
        Assert.Equal([19u], surface.IdentifyRequests);
        Assert.Empty(surface.ApplyCalls);
        surface.ReplaceAssessmentVersion(19u, 101);
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([19u, 20u], surface.IdentifyRequests);
        surface.ReplaceAssessmentVersion(20u, 101);
        surface.IdentifyRequests.Clear();

        Assert.True(controller.Tick(canAct: true));

        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        Assert.Contains(host.Logger.Infos, line =>
            line.Contains("thresholdItem=Low Gloves (0x00000014)",
                StringComparison.Ordinal));
        Assert.Contains(19u, surface.PropertyCaptures);
        Assert.Contains(20u, surface.PropertyCaptures);

        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        surface.LastItemCompletion = new PluginItemUseCompletion(
            1L,
            999u,
            surface.ObjectId,
            0u);
        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        surface.LastItemCompletion = new PluginItemUseCompletion(
            2L,
            charge.ObjectId,
            surface.ObjectId,
            0u);
        // Completion can arrive before the live charged-effect update. The
        // accepted source must remain blocked even while every cached value
        // still looks like the previous charged stone.
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        surface.ReplaceAssessmentVersion(charge.ObjectId, 101);
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        surface.Inventory =
        [
            charge with { Effects = 0u },
            missingCurrentMana,
            thresholdItem,
        ];

        Assert.False(controller.Tick(canAct: true));
        // The spent charge, and the worn gear the refill has just made a
        // stranger of: a refill ends the life of every reading.
        Assert.Equal([10u, 19u], surface.IdentifyRequests);
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        // A newer stamp alone is insufficient because appraisal application
        // merges keys and an empty stone's omitted 107 can remain stale.
        surface.ReplaceAssessmentVersion(charge.ObjectId, 101);

        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        Assert.Contains(host.Logger.Infos, line =>
            line.Contains("success=True", StringComparison.Ordinal)
                && line.Contains("expected=0x00000001, actual=0x00000001",
                    StringComparison.Ordinal));

        // A real empty-to-charged transition may refill to exactly the old amount.
        surface.Inventory = [charge, missingCurrentMana, thresholdItem];
        Assert.False(controller.Tick(canAct: true)); // Still the appraisal from before charging.
        // The worn gear answers the questions the refill made it owe, so it
        // can be spent on again; without them nothing more would be spent.
        surface.ReplaceAssessmentVersion(19u, 102);
        Assert.False(controller.Tick(canAct: true));
        surface.ReplaceAssessmentVersion(20u, 102);
        Assert.False(controller.Tick(canAct: true));
        surface.ReplaceAssessmentVersion(charge.ObjectId, 102);
        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u), (10u, 1u)], surface.ApplyCalls);
        surface.LastItemCompletion = new PluginItemUseCompletion(3, 10, 999, 0);
        Assert.True(controller.Tick(canAct: true));
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 15d));
        Assert.Contains("unconfirmed", controller.Status);
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u), (10u, 1u)], surface.ApplyCalls);
    }

    [Fact]
    public void EmptyStoneWithStalePositiveManaIsNeverARefillCandidate()
    {
        PluginInventoryItem emptyStone = Item(10, "Mana Stone", 0x00080000u) with
        {
            ObjectClass = PluginObjectClass.ManaStone,
            ItemCurrentMana = 100,
            Effects = 0u,
        };
        PluginInventoryItem armor = Item(20, "Worn Armor") with
        {
            EquippedLocation = 1u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        Assert.Null(ItemManaRechargePlanner.Plan(
            [emptyStone, armor],
            Kinds((emptyStone.Name, ConsumableCategory.ManaStone)),
            33,
            out bool missing));
        Assert.True(missing);
    }

    /// <summary>
    /// A charged stone is spent before a one-shot charge, and a charge is
    /// whatever the profile files under that kind -- a gem, a nut, anything.
    /// The old planner asked for the mana-stone item type and a charged
    /// effect, which no mana charge in the game carries, so a character with
    /// nothing but charges could never top its gear up.
    ///
    /// Mutation: look for the charge before the stone, or narrow the charge
    /// to the stone item type, and one of these two answers changes.
    /// </summary>
    [Fact]
    public void AChargedStoneOutranksACharge_AndACharge_IsAnyKindSixItem()
    {
        PluginInventoryItem gemCharge = Item(11, "Great Mana Charge") with
        {
            ObjectClass = PluginObjectClass.Gem,
        };
        PluginInventoryItem chargedStone = Item(12, "Major Mana Stone") with
        {
            ObjectClass = PluginObjectClass.ManaStone,
            Effects = 0x00000001u,
        };
        PluginInventoryItem wand = Item(20, "Low Wand") with
        {
            EquippedLocation = 1u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var kinds = Kinds(
            ("Great Mana Charge", ConsumableCategory.ManaSource),
            ("Major Mana Stone", ConsumableCategory.ManaStone));

        ItemManaRechargePlan both = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                [gemCharge, chargedStone, wand], kinds, 33, out _));
        Assert.Equal(12u, both.ChargeObjectId);

        ItemManaRechargePlan chargeOnly = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                [gemCharge, wand], kinds, 33, out _));
        Assert.Equal(11u, chargeOnly.ChargeObjectId);
    }

    /// <summary>
    /// The same order through the controller, with the real apply: a charged
    /// stone is used when there is one, and a plain charge -- a gem, here --
    /// when there is not.
    ///
    /// Mutation: swap the two lookups in the planner and the first case uses
    /// the gem.
    /// </summary>
    [Theory]
    [InlineData(true, 12u)]
    [InlineData(false, 11u)]
    public void TheControllerSpendsAStoneWhenThereIsOneAndACharge_Otherwise(
        bool holdingAStone,
        uint expected)
    {
        PluginInventoryItem gemCharge = Item(11, "Great Mana Charge") with
        {
            ObjectClass = PluginObjectClass.Gem,
        };
        PluginInventoryItem stone = Item(12, "Major Mana Stone") with
        {
            ObjectClass = PluginObjectClass.ManaStone,
            Effects = 0x00000001u,
        };
        PluginInventoryItem wand = Item(20, "Low Wand") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var surface = new Surface
        {
            Inventory = holdingAStone
                ? [gemCharge, stone, wand]
                : [gemCharge, wand],
        };
        surface.Assess(wand, 100, (107u, 10), (108u, 100));
        var profiles = new CombatSettings();
        profiles.ConsumableCategories["Great Mana Charge"] =
            ConsumableCategory.ManaSource;
        profiles.ConsumableCategories["Major Mana Stone"] =
            ConsumableCategory.ManaStone;
        var host = new Host(surface);
        var controller = new ItemManaRechargeController(
            host,
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            profiles);

        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([20u], surface.IdentifyRequests);
        surface.ReplaceAssessmentVersion(20u, 101);

        Assert.True(controller.Tick(canAct: true));

        Assert.Equal([(expected, 1u)], surface.ApplyCalls);
        Assert.Contains(
            host.Logger.Infos,
            line => line == "Item Low Wand low on mana, 10%, 10/100");
    }

    /// <summary>
    /// Gear that wants mana with nothing in the pack to give it says so once
    /// a run, not three times a second.
    ///
    /// Mutation: drop the once-a-run guard and the warning is counted many
    /// times over; drop the "wanted but had nothing" answer from the planner
    /// and it is never said at all.
    /// </summary>
    [Fact]
    public void NothingToSpendOnLowGearIsSaidOnceARun()
    {
        PluginInventoryItem wand = Item(20, "Low Wand") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var surface = new Surface { Inventory = [wand] };
        surface.Assess(wand, 100, (107u, 10), (108u, 100));
        var host = new Host(surface);
        var controller = new ItemManaRechargeController(
            host,
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            new CombatSettings());

        Assert.False(controller.Tick(canAct: true));
        surface.ReplaceAssessmentVersion(20u, 101);
        for (int pass = 0; pass < 5; pass++)
            Assert.False(controller.Tick(canAct: true, elapsedSeconds: 0.3d));

        Assert.Equal("No mana charge or stone to spend", controller.Status);
        Assert.Equal(
            1,
            host.Logger.Infos.Count(line => line.StartsWith(
                "Warning: No mana charges/stones available",
                StringComparison.Ordinal)));
        // The item's own line is said once too, while it says the same thing.
        Assert.Equal(
            1,
            host.Logger.Infos.Count(
                line => line == "Item Low Wand low on mana, 10%, 10/100"));

        controller.ResetOncePerRunWarnings();
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 0.3d));
        Assert.Equal(
            2,
            host.Logger.Infos.Count(line => line.StartsWith(
                "Warning: No mana charges/stones available",
                StringComparison.Ordinal)));
    }

    /// <summary>
    /// A refill moves mana into every worn item at once and the server says
    /// nothing further about any of them: the only word of it is the line it
    /// prints. So a refill ends the life of every reading this owner holds,
    /// and nothing more is spent until fresh ones arrive. Without that, gear
    /// went on reading low for two to six minutes and every charge in the
    /// pack was poured into kit that was already full.
    ///
    /// Mutation: leave the readings standing after a refill and all three
    /// charges are spent on the one item.
    /// </summary>
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ARefillEndsEveryReadingOfWornGear(bool throughTheSpokenLine)
    {
        PluginInventoryItem[] charges =
        [
            Item(10, "Mana Charge", 0x00080000u) with
            {
                ItemCurrentMana = 100,
                Effects = 0x00000001u,
            },
            Item(11, "Mana Charge", 0x00080000u) with
            {
                ItemCurrentMana = 100,
                Effects = 0x00000001u,
            },
            Item(12, "Mana Charge", 0x00080000u) with
            {
                ItemCurrentMana = 100,
                Effects = 0x00000001u,
            },
        ];
        PluginInventoryItem worn = Item(20, "Low Gauntlets") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var surface = new Surface { Inventory = [.. charges, worn] };
        foreach (PluginInventoryItem charge in charges)
            surface.Assess(charge, 100, (107u, 100));
        surface.Assess(worn, 100, (107u, 10), (108u, 100));
        var profiles = new CombatSettings();
        profiles.ConsumableNames.Add("Mana Charge");
        profiles.ConsumableCategories["Mana Charge"] =
            ConsumableCategory.ManaSource;
        var controller = new ItemManaRechargeController(
            new Host(surface),
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            profiles);

        Assert.False(controller.Tick(canAct: true));
        surface.ReplaceAssessmentVersion(20u, 101);
        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        if (throughTheSpokenLine)
        {
            // A receipt that says nothing good, so the spoken line is the
            // only thing here that can end the readings -- and the wait on
            // the receipt is over either way.
            surface.LastItemCompletion = new PluginItemUseCompletion(
                1L, 10u, surface.ObjectId, 0x1Du);
            surface.ChatMessages.Add(new PluginChatMessage(
                1u,
                0u,
                0,
                string.Empty,
                "The Mana Stone gives 1,200 points of mana to the "
                    + "following items: Low Gauntlets",
                string.Empty));
        }
        else
        {
            surface.LastItemCompletion = new PluginItemUseCompletion(
                1L, 10u, surface.ObjectId, 0u);
        }

        for (int pass = 0; pass < 10; pass++)
            controller.Tick(canAct: true, elapsedSeconds: 0.3d);

        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
    }

    /// <summary>
    /// A charge is in the character's hands from the moment it is used until
    /// the server answers for it, so it takes the one item slot every rule
    /// that consumes an item asks for first -- and the answer is read on the
    /// host frame, because the pass that would otherwise read it is the very
    /// thing the use is holding.
    ///
    /// Mutation: take the slot away and the first hold reads free; stop
    /// reading the answer on the frame and the slot is still held after it
    /// has landed, for the whole fifteen seconds of the watchdog.
    /// </summary>
    [Fact]
    public void AChargeInFlightHoldsTheItemSlotUntilTheFrameReadsTheAnswer()
    {
        PluginInventoryItem charge = Item(10, "Mana Charge", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0x00000001u,
        };
        PluginInventoryItem worn = Item(20, "Low Gauntlets") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var surface = new Surface { Inventory = [charge, worn] };
        surface.Assess(charge, 100, (107u, 100));
        surface.Assess(worn, 100, (107u, 10), (108u, 100));
        var profiles = new CombatSettings();
        profiles.ConsumableNames.Add(charge.Name);
        profiles.ConsumableCategories[charge.Name] = ConsumableCategory.ManaSource;
        var locks = new ActionLockTable();
        var controller = new ItemManaRechargeController(
            new Host(surface),
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            profiles);
        controller.BindActionLocks(locks);

        Assert.False(controller.Tick(canAct: true));
        surface.ReplaceAssessmentVersion(20u, 101);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));

        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        // No turn from here: only frames run while the pass is held.
        surface.LastItemCompletion = new PluginItemUseCompletion(
            1L, 10u, surface.ObjectId, 0u);
        controller.ObservePendingReceipt(0.05d);

        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
    }

    /// <summary>
    /// Worn gear is asked about one piece at a time, and in turn. The client
    /// answers one appraisal at a time and the looter asks for its own
    /// through the same slot, so a kit asked all at once queued up behind
    /// itself; and since a question nobody answers is let go rather than
    /// stamped, asking in list order would leave the first item asking for
    /// ever while the rest were never asked at all.
    ///
    /// Mutation: ask every due item on the pass and the first count is three;
    /// drop the turn-taking and the unanswered first item is the only one
    /// ever asked.
    /// </summary>
    [Fact]
    public void WornGearIsAskedAboutOneAtATimeAndInTurn()
    {
        PluginInventoryItem[] worn =
        [
            Item(20, "Gauntlets") with
            {
                EquippedLocation = 0x00000002u,
                ItemCurrentMana = 90,
                ItemMaximumMana = 100,
            },
            Item(21, "Helm") with
            {
                EquippedLocation = 0x00000004u,
                ItemCurrentMana = 90,
                ItemMaximumMana = 100,
            },
            Item(22, "Girth") with
            {
                EquippedLocation = 0x00000008u,
                ItemCurrentMana = 90,
                ItemMaximumMana = 100,
            },
        ];
        var surface = new Surface { Inventory = worn };
        foreach (PluginInventoryItem item in worn)
            surface.Assess(item, 100, (107u, 90), (108u, 100));
        var controller = new ItemManaRechargeController(
            new Host(surface),
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            new CombatSettings());

        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 0.3d));
        Assert.Equal([20u], surface.IdentifyRequests);

        // Nobody answers for the first one, and the question is let go after
        // ten seconds; the turn then passes to the next piece, not back to
        // the one that went unanswered.
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 11d));
        Assert.Equal([20u, 21u], surface.IdentifyRequests);
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 11d));
        Assert.Equal([20u, 21u, 22u], surface.IdentifyRequests);
    }

    private static IReadOnlyDictionary<string, ConsumableCategory> Kinds(
        params (string Name, ConsumableCategory Kind)[] rows) =>
        rows.ToDictionary(
            static row => row.Name,
            static row => row.Kind,
            StringComparer.Ordinal);

    /// <summary>
    /// Worn gear is appraised once and then let alone for two to six minutes
    /// before being looked at again. Without the second look the mana numbers
    /// the first one gave never change, so an item that drains after it was
    /// appraised is never seen to need anything.
    ///
    /// Mutation: stamp the appraisal with no expiry (or none at all) and the
    /// item is either never looked at again or looked at every pass.
    /// </summary>
    [Fact]
    public void WornGearIsAppraisedAgainOnlyOnceItsNumbersHaveGoneStale()
    {
        PluginInventoryItem charge = Item(10, "Mana Charge", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0x00000001u,
        };
        // Comfortably above the threshold, so nothing is ever applied and the
        // passes measure the appraisal cadence alone.
        PluginInventoryItem worn = Item(20, "Full Gauntlets") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 90,
            ItemMaximumMana = 100,
        };
        var surface = new Surface { Inventory = [charge, worn] };
        surface.Assess(charge, 100, (107u, 100));
        surface.Assess(worn, 100, (107u, 90), (108u, 100));
        var profiles = new CombatSettings();
        profiles.ConsumableNames.Add(charge.Name);
        profiles.ConsumableCategories[charge.Name] = ConsumableCategory.ManaSource;
        var controller = new ItemManaRechargeController(
            new Host(surface),
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            profiles);

        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([20u], surface.IdentifyRequests);
        surface.ReplaceAssessmentVersion(20u, 101);
        surface.IdentifyRequests.Clear();

        // The pass that takes the answer in: the wait starts here, at 0.1 s.
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 0.1d));
        Assert.Empty(surface.IdentifyRequests);
        Assert.Equal("Worn mana ready", controller.Status);

        // Two minutes on is inside every draw of the band.
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 119.9d));
        Assert.Empty(surface.IdentifyRequests);

        // Six minutes on is past every draw of it.
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 240.2d));
        Assert.Equal([20u], surface.IdentifyRequests);
        Assert.Empty(surface.ApplyCalls);
    }

    private static PluginInventoryItem Item(
        uint id,
        string name,
        uint itemType = 0x80u) => new(
            id, 0u, name, itemType, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);

    private sealed class Host(Surface surface) : IPluginHost
    {
        public bool HasUi => false;
        public TestLogger Logger { get; } = new();
        public IPluginLogger Log => Logger;
        public IGameState State => null!;
        public IEvents Events => null!;
        public ISelectionService Selection => null!;
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation => surface;
    }

    private sealed class TestLogger : IPluginLogger
    {
        public List<string> Infos { get; } = [];
        public void Info(string message) => Infos.Add(message);
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class Surface :
        IAutomationSurface,
        ICharacterInfo,
        IItemAutomation,
        IPluginChat,
        IWorldObjectAutomation
    {
        public List<PluginChatMessage> ChatMessages { get; } = [];
        public IReadOnlyList<PluginChatMessage> CaptureMessages(
            ulong afterSequence) => ChatMessages
                .Where(message => message.Sequence > afterSequence)
                .ToArray();
        public void PostSystemMessage(string text) { }
        private readonly Dictionary<uint, PluginWorldObject> _objects = [];
        private readonly Dictionary<uint, PluginItemProperties> _properties = [];

        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public IItemAutomation Items => this;
        public IWorldObjectAutomation Objects => this;
        public bool IsInWorld => true;
        public uint ObjectId => 1u;
        public uint CurrentHealth => 100u;
        public uint MaxHealth => 100u;
        public uint CurrentStamina => 100u;
        public uint MaxStamina => 100u;
        public uint CurrentMana => 100u;
        public uint MaxMana => 100u;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginInventoryItem> Inventory { get; set; } = [];
        public PluginItemUseCompletion LastItemCompletion { get; set; }
        PluginItemUseCompletion IItemAutomation.LastCompletion =>
            LastItemCompletion;
        public List<(uint Source, uint Target)> ApplyCalls { get; } = [];
        public List<uint> IdentifyRequests { get; } = [];
        public List<uint> PropertyCaptures { get; } = [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }

        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() =>
            Inventory;

        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            PropertyCaptures.Add(objectId);
            return _properties.TryGetValue(objectId, out properties);
        }

        public PluginItemCommandResult Apply(
            uint objectId,
            uint targetObjectId)
        {
            ApplyCalls.Add((objectId, targetObjectId));
            return new(PluginItemCommandStatus.Started);
        }

        public bool TryGet(uint objectId, out PluginWorldObject value) =>
            _objects.TryGetValue(objectId, out value);

        public PluginItemCommandResult Identify(uint objectId)
        {
            IdentifyRequests.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }

        public void Assess(
            PluginInventoryItem item,
            int version,
            params (uint Key, int Value)[] ints)
        {
            _objects[item.ObjectId] = new PluginWorldObject(
                item.ObjectId,
                item.WeenieClassId,
                item.Name,
                item.ObjectClass,
                item.ItemType,
                item.ContainerObjectId,
                item.WielderObjectId)
            {
                IsOwned = true,
                HasAppraisalData = true,
                LastIdTime = version,
            };
            _properties[item.ObjectId] = new PluginItemProperties(
                ints.ToDictionary(static pair => pair.Key, static pair => pair.Value),
                new Dictionary<uint, long>(),
                new Dictionary<uint, bool>(),
                new Dictionary<uint, double>(),
                new Dictionary<uint, string>(),
                new Dictionary<uint, uint>(),
                new Dictionary<uint, uint>());
        }

        public void ReplaceAssessmentVersion(uint objectId, int version)
        {
            _objects[objectId] = _objects[objectId] with
            {
                LastIdTime = version,
            };
        }
    }
}
