using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class ProfileGiveControllerTests
{
    [Fact]
    public void NamedProfileGivesOnlyKeepMatchesAndWaitsForCompletion()
    {
        FakeAutomation automation = Nearby("Mule", 1d);
        automation.ItemsValue =
        [
            Item(10u, "Trade Pyreal"),
            Item(11u, "Personal Note"),
        ];
        var storage = new MemoryStorage();
        var host = new FakeHost(automation, storage);
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        Assert.True(profiles.Create("Mule Items", false, [], out _));
        profiles.SaveCurrent(
        [
            new LootRule
            {
                Expression = "name ~= trade",
                Action = LootAction.Keep,
            },
            new LootRule
            {
                Expression = "*",
                Action = LootAction.NoLoot,
            },
        ]);
        var controller = new ProfileGiveController(
            host,
            profiles,
            new InventorySettings());

        Assert.True(controller.TryStart("Mule Items.utl", "Mule"));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([(10u, 100u, 0u)], automation.Gives);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.Gives);
        automation.Completion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Give,
            10u,
            0u);
        // The answer and the end of the run arrive on the same step: with the
        // queue empty there is nothing left to ask for.
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Contains("1 item(s)", controller.Status, StringComparison.Ordinal);
    }

    [Fact]
    public void StartRejectsBusyMissingProfileAndMissingTarget()
    {
        FakeAutomation automation = Nearby("Mule", 1d);
        automation.ObjectsValue =
            [Standing("Mule", 100u, 1d / 240d, 0d) with { ObjectClass = PluginObjectClass.Npc }];
        var storage = new MemoryStorage();
        var host = new FakeHost(automation, storage);
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        var controller = new ProfileGiveController(
            host,
            profiles,
            new InventorySettings());

        Assert.False(controller.TryStart("Missing", "Mule"));
        Assert.True(profiles.Create("Empty", false, [], out _));
        Assert.False(controller.TryStart("Empty", "Missing"));
        Assert.True(controller.TryStart("Empty", "Mule"));
        Assert.False(controller.TryStart("Empty", "Mule"));
    }

    /// <summary>
    /// The name mode picks items three ways, and each way picks a different
    /// set out of the same packs. Mutation: collapsing the exact match onto a
    /// partial one makes the first run take the sealed note as well.
    /// </summary>
    [Fact]
    public void NameGiveTakesTheWholeNameAPartOfItOrAPattern()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue =
        [
            Item(10u, "Personal Note"),
            Item(11u, "Personal Note, Sealed"),
            Item(12u, "Prismatic Taper"),
        ];
        ProfileGiveController controller = Controller(automation);

        Assert.True(controller.TryStartByName(
            "personal note", GiveNameMatch.Exact, 0, "Mule"));
        Assert.Equal([10u], Queued(controller, automation));

        Assert.True(controller.TryStartByName(
            "personal", GiveNameMatch.Partial, 0, "Mule"));
        Assert.Equal([10u, 11u], Queued(controller, automation));

        Assert.True(controller.TryStartByName(
            "^pri.*taper$", GiveNameMatch.Pattern, 0, "Mule"));
        Assert.Equal([12u], Queued(controller, automation));

        Assert.False(controller.TryStartByName(
            "(unclosed", GiveNameMatch.Pattern, 0, "Mule"));
        Assert.Contains("pattern", controller.Status, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// A count is a number of units, not of items: whole stacks go until the
    /// count cannot cover the next one, and that one is split. Mutation:
    /// passing zero as the amount hands over the whole last stack.
    /// </summary>
    [Fact]
    public void NameGiveSplitsTheStackThatTheCountRunsOutIn()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue =
        [
            Stack(10u, "Prismatic Taper", 10),
            Stack(11u, "Prismatic Taper", 10),
        ];
        ProfileGiveController controller = Controller(automation);

        Assert.True(controller.TryStartByName(
            "Prismatic Taper", GiveNameMatch.Exact, 14, "Mule"));

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal((10u, 100u, 0u), automation.Gives[0]);
        automation.ItemsValue = [automation.ItemsValue[1]];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal((11u, 100u, 4u), automation.Gives[1]);
    }

    /// <summary>
    /// The range is measured flat along the ground. Mutation: taking height
    /// into the measure refuses the target on the floor above, which stands
    /// one meter away on the map.
    /// </summary>
    [Fact]
    public void NameGiveRefusesATargetBeyondTheRangeMeasuredFlat()
    {
        var automation = Nearby("Mule", 20d);
        automation.ItemsValue = [Item(10u, "Trade Pyreal")];
        ProfileGiveController controller = Controller(automation);

        Assert.False(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        Assert.Contains("range", controller.Status, StringComparison.OrdinalIgnoreCase);

        // The same target one meter off but fifty meters overhead is in range,
        // because height is not part of the measure.
        automation.ObjectsValue =
        [
            Standing("Mule", 100u, eastWest: 1d / 240d, elevation: 50d),
        ];
        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
    }

    /// <summary>
    /// A profile hand-over is range-checked as a give by name is, each
    /// refusal in the reference's words for its command and naming the
    /// target as it was typed. Mutation: skipping the check for the profile
    /// hand-over starts it; naming the found object prints "Mule".
    /// </summary>
    [Fact]
    public void BothHandOversRefuseATargetOutOfRangeInTheReferencesWords()
    {
        FakeAutomation automation = Nearby("Mule", 20d);
        automation.ItemsValue = [Item(10u, "Trade Pyreal")];
        var host = new FakeHost(automation, new MemoryStorage());
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        host.VtankProfiles.WriteText(
            StorageLayout.ItemGiverProfileKey("rares"),
            MossTankLootProfileStore.SerializeRules(
                [new LootRule { Expression = "*", Action = LootAction.Keep }]));
        var controller = new ProfileGiveController(host, profiles, new InventorySettings());

        Assert.False(controller.TryStart("rares", "mul", partialTargetMatch: true));
        Assert.Equal(
            "ItemGiver mul is 20.00 meters away. IGRange is set to 15",
            controller.Refusal);

        Assert.False(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "mul", partialTargetMatch: true));
        Assert.Equal(
            "mul is 20.00 meters away, IGRange is set to 15. bailing.",
            controller.Refusal);
        Assert.Empty(automation.Gives);
    }

    /// <summary>
    /// The end line is the reference's: a name give is named in lower case,
    /// a profile hand-over by its file with the extension, the target as it
    /// was typed. Mutation: the old line kept the typed case, dropped the
    /// extension and named the found object.
    /// </summary>
    [Fact]
    public void TheFinishedLineNamesTheRunAsTheReferenceDoes()
    {
        FakeAutomation automation = Nearby("Mule", 1d);
        var host = new FakeHost(automation, new MemoryStorage());
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        host.VtankProfiles.WriteText(
            StorageLayout.ItemGiverProfileKey("rares"),
            MossTankLootProfileStore.SerializeRules(
                [new LootRule { Expression = "*", Action = LootAction.Keep }]));
        var controller = new ProfileGiveController(host, profiles, new InventorySettings());

        Assert.True(controller.TryStart("rares", "mul", partialTargetMatch: true));
        controller.StopRequested();
        Assert.Equal(
            "ItemGiver finished: rares.utl to mul. took 0s to give 0 item(s). 0",
            controller.FinishedLine);

        Assert.True(controller.TryStartByName(
            "Trade PYREAL", GiveNameMatch.Partial, 0, "Mule"));
        controller.StopRequested();
        Assert.Equal(
            "ItemGiver finished: trade pyreal to Mule. took 0s to give 0 item(s). 0",
            controller.FinishedLine);
    }

    /// <summary>
    /// The last number of the end line is how many more times the client was
    /// asked than items went, as the reference counts it: an item the client
    /// was too busy for three times, then given, is one item and three.
    /// Mutation: printing the items written off prints 0.
    /// </summary>
    [Fact]
    public void TheFinishedLineCountsTheAsksThatGaveNothing()
    {
        FakeAutomation automation = Nearby("Mule", 1d);
        automation.ItemsValue = [Item(10u, "Trade Pyreal")];
        automation.GiveStatus = PluginItemCommandStatus.Busy;
        ProfileGiveController controller = Controller(automation);

        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        for (int tick = 0; tick < 3; tick++)
            controller.Tick(0.1d, canAct: true);
        automation.GiveStatus = PluginItemCommandStatus.Started;
        controller.Tick(0.1d, canAct: true);
        automation.ItemsValue = [];
        for (int tick = 0; tick < 3 && controller.IsRunning; tick++)
            controller.Tick(0.1d, canAct: true);

        Assert.False(controller.IsRunning);
        Assert.Equal(4, automation.Gives.Count);
        Assert.EndsWith("to give 1 item(s). 3", controller.FinishedLine, StringComparison.Ordinal);
    }

    /// <summary>
    /// An item is asked for once more than the busy count allows, then
    /// written off, and the run stops once more items are written off than
    /// the failure count allows -- the reference's ladder, which counts every
    /// ask, answered or not. Mutation: writing an item off on its first
    /// refusal gives each item once and stops after two asks.
    /// </summary>
    [Fact]
    public void AnItemIsAskedForOnceMoreThanTheBusyCountThenWrittenOff()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue =
        [
            Item(10u, "Trade Pyreal"),
            Item(11u, "Trade Pyreal"),
            Item(12u, "Trade Pyreal"),
            Item(13u, "Trade Pyreal"),
            Item(14u, "Trade Pyreal"),
        ];
        automation.GiveStatus = PluginItemCommandStatus.Refused;
        ProfileGiveController controller = Controller(
            automation,
            new InventorySettings { GiveBusyRetryLimit = 2, GiveFailureLimit = 1 });

        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        for (int tick = 0; tick < 20 && controller.IsRunning; tick++)
            controller.Tick(0.1d, canAct: true);

        Assert.False(controller.IsRunning);
        Assert.Equal(
            [10u, 10u, 10u, 11u, 11u, 11u],
            automation.Gives.Select(static give => give.Item));
        Assert.Contains("gave up", controller.Status, StringComparison.OrdinalIgnoreCase);
        Assert.EndsWith("to give 0 item(s). 6", controller.FinishedLine, StringComparison.Ordinal);
    }

    /// <summary>
    /// A give the server answers with an error leaves the item in the packs,
    /// and the reference asks for it again rather than moving on. Mutation:
    /// dropping the item on the error ends the run with one ask and nothing
    /// given.
    /// </summary>
    [Fact]
    public void AGiveTheServerRefusesIsAskedForAgain()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue = [Item(10u, "Trade Pyreal")];
        ProfileGiveController controller = Controller(automation);

        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        controller.Tick(0.1d, canAct: true);
        automation.Completion = new PluginInventoryCompletion(
            1, PluginInventoryCommandKind.Give, 10u, 0x2Bu);
        controller.Tick(0.1d, canAct: true);
        controller.Tick(0.1d, canAct: true);

        Assert.True(controller.IsRunning);
        Assert.Equal([10u, 10u], automation.Gives.Select(static give => give.Item));
    }

    /// <summary>
    /// The ten-second clock restarts when a new item is first asked for, and
    /// the pause between gives does not run it down: a run with a longer
    /// pause than the clock still reaches its second item. Mutation: a clock
    /// that runs through the pause bails before the second give.
    /// </summary>
    [Fact]
    public void AGiveDelayLongerThanTheBailClockDoesNotBail()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue =
        [
            Item(10u, "Trade Pyreal"),
            Item(11u, "Trade Pyreal"),
        ];
        ProfileGiveController controller = Controller(
            automation,
            new InventorySettings { GiveDelaySeconds = 15d });

        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        controller.Tick(0.1d, canAct: true);
        automation.ItemsValue = [automation.ItemsValue[1]];
        for (int second = 0; second < 20 && automation.Gives.Count < 2; second++)
            controller.Tick(1d, canAct: true);

        Assert.True(controller.IsRunning);
        Assert.Equal(string.Empty, controller.EndError);
        Assert.Equal([10u, 11u], automation.Gives.Select(static give => give.Item));
    }

    /// <summary>
    /// The player can call a run off, and calling one off when none is running
    /// says so rather than pretending.
    /// </summary>
    [Fact]
    public void StopRequestedEndsARunAndReportsWhenThereIsNoneToEnd()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue = [Item(10u, "Trade Pyreal")];
        ProfileGiveController controller = Controller(automation);

        Assert.False(controller.StopRequested());
        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        Assert.True(controller.StopRequested());
        Assert.False(controller.IsRunning);
        Assert.False(controller.StopRequested());
    }

    /// <summary>
    /// A run with a delay set leaves that long between one hand-over and the
    /// next, and none at all before the first.
    /// </summary>
    [Fact]
    public void AGiveDelayWaitsBetweenHandOversAndNotBeforeTheFirst()
    {
        var automation = Nearby("Mule", 1d);
        automation.ItemsValue =
        [
            Item(10u, "Trade Pyreal"),
            Item(11u, "Trade Pyreal"),
        ];
        ProfileGiveController controller = Controller(
            automation,
            new InventorySettings { GiveDelaySeconds = 1d });

        Assert.True(controller.TryStartByName(
            "Trade Pyreal", GiveNameMatch.Exact, 0, "Mule"));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.Gives);

        automation.ItemsValue = [automation.ItemsValue[1]];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.5d, canAct: true));
        Assert.Single(automation.Gives);
        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.Equal(2, automation.Gives.Count);
    }

    /// <summary>
    /// No target named is not a target that matches everybody. Part of a name
    /// is enough for the partial flag, and the empty string is part of every
    /// name, so a line whose target went missing would otherwise hand the
    /// packs to whoever happened to be standing nearest.
    /// Mutation: drop the refusal and the run starts against the bystander.
    /// </summary>
    [Fact]
    public void AGiveWithNoTargetNamedIsRefusedRatherThanMatchingEverybody()
    {
        FakeAutomation automation = Nearby("Mule", 1d);
        automation.ItemsValue = [Item(10u, "Prismatic Taper")];
        var host = new FakeHost(automation, new MemoryStorage());
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        Assert.True(profiles.Create("Mule Items", false, [], out _));
        profiles.SaveCurrent([new LootRule { Expression = "*", Action = LootAction.Keep }]);
        var controller = new ProfileGiveController(
            host,
            profiles,
            new InventorySettings());

        Assert.False(controller.TryStart("Mule Items.utl", string.Empty, true));
        Assert.Contains("Syntax:", controller.Status, StringComparison.Ordinal);
        Assert.Contains("/ub ig", controller.Status, StringComparison.Ordinal);
        Assert.False(controller.IsRunning);

        Assert.False(controller.TryStartByName(
            "Prismatic Taper", GiveNameMatch.Exact, 0, "   "));
        Assert.Contains("Syntax:", controller.Status, StringComparison.Ordinal);
        Assert.Contains("/ub give", controller.Status, StringComparison.Ordinal);
        Assert.False(controller.IsRunning);
    }

    /// <summary>
    /// A profile dropped into the shared itemgiver folder is what <c>/ub ig</c>
    /// finds, with or without its extension, and it wins over a loot profile
    /// of the same name. Mutation: reading the itemgiver folder removed turns
    /// the first assert red with "Item giver profile not found".
    /// </summary>
    [Fact]
    public void AProfileInTheItemGiverFolderIsFoundByName()
    {
        FakeAutomation automation = Nearby("Town Crier", 1d);
        automation.ItemsValue = [Item(10u, "Prismatic Taper")];
        var host = new FakeHost(automation, new MemoryStorage());
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        host.VtankProfiles.WriteText(
            StorageLayout.ItemGiverProfileKey("rares"),
            MossTankLootProfileStore.SerializeRules(
                [new LootRule { Expression = "*", Action = LootAction.Keep }]));
        var controller = new ProfileGiveController(
            host, profiles, new InventorySettings());

        Assert.True(controller.TryStart("rares", "Town Crier", false), controller.Status);
        Assert.True(controller.IsRunning);
        controller.StopRequested();
        Assert.True(controller.TryStart("rares.utl", "Town Crier", false), controller.Status);
        Assert.Equal(
            "mosstank/ub/itemgiver/rares.utl", StorageLayout.ItemGiverProfileKey("rares.utl"));
    }

    /// <summary>
    /// The target is found as the reference finds one: a decimal id, a hex
    /// id or "selected" names that object outright; a name picks the nearest
    /// object of that name; and a special name that lands on the character
    /// itself is refused in the reference's words. Mutation: the old
    /// name-only search finds nobody for an id, a hex id or "selected".
    /// </summary>
    [Theory]
    [InlineData("100", 100u)]
    [InlineData("0x64", 100u)]
    [InlineData("64", 100u)]
    [InlineData("selected", 100u)]
    [InlineData("Mule", 90u)]
    public void TheTargetIsFoundByIdHexIdSelectedOrNearestName(string typed, uint expected)
    {
        FakeAutomation automation = Nearby("Mule", 5d);
        automation.ObjectsValue =
        [
            Standing("Mule", 100u, 5d / 240d, 0d),
            Standing("Mule", 90u, 2d / 240d, 0d),
            Standing("Tester", 1u, 0d, 0d),
        ];
        automation.ItemsValue = [Item(10u, "Prismatic Taper")];
        automation.SelectedObjectId = 100u;
        ProfileGiveController controller = Controller(automation);

        Assert.True(
            controller.TryStartByName("Prismatic Taper", GiveNameMatch.Exact, 0, typed),
            controller.Refusal);
        controller.Tick(0.1d, canAct: true);

        Assert.Equal(expected, Assert.Single(automation.Gives).Target);
    }

    /// <summary>
    /// "selected" with the character itself selected names the character,
    /// which cannot be given to. Mutation: dropping the check hands the
    /// packs to yourself.
    /// </summary>
    [Fact]
    public void GivingToYourselfIsRefusedInTheReferencesWords()
    {
        FakeAutomation automation = Nearby("Mule", 5d);
        automation.ObjectsValue = [.. automation.ObjectsValue, Standing("Tester", 1u, 0d, 0d)];
        automation.ItemsValue = [Item(10u, "Prismatic Taper")];
        automation.SelectedObjectId = 1u;
        ProfileGiveController controller = Controller(automation);

        Assert.False(controller.TryStartByName(
            "Prismatic Taper", GiveNameMatch.Exact, 0, "selected"));
        Assert.Equal("You can't give to yourself", controller.Refusal);
        Assert.Empty(automation.Gives);
    }

    /// <summary>
    /// Every refused start says its own reason: a start refused outside the
    /// world does not repeat the reason the start before it was refused for.
    /// Mutation: returning before the refusal is cleared reports the earlier
    /// "player Nobody not found" for the second start.
    /// </summary>
    [Fact]
    public void ARefusedStartNeverReportsTheReasonBeforeIt()
    {
        FakeAutomation automation = Nearby("Mule", 1d);
        automation.ItemsValue = [Item(10u, "Prismatic Taper")];
        ProfileGiveController controller = Controller(automation);

        Assert.False(controller.TryStartByName(
            "Prismatic Taper", GiveNameMatch.Exact, 0, "Nobody"));
        Assert.Equal("player Nobody not found", controller.Refusal);

        automation.IsAvailable = false;
        Assert.False(controller.TryStartByName(
            "Prismatic Taper", GiveNameMatch.Exact, 0, "Mule"));
        Assert.Equal("Item giver refused: not in the world.", controller.Refusal);
        Assert.False(controller.TryStart("rares", "Mule"));
        Assert.Equal("Item giver refused: not in the world.", controller.Refusal);
    }

    private static ProfileGiveController Controller(
        FakeAutomation automation,
        InventorySettings? settings = null)
    {
        var host = new FakeHost(automation, new MemoryStorage());
        var profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        return new ProfileGiveController(
            host,
            profiles,
            settings ?? new InventorySettings());
    }

    /// <summary>
    /// The object ids a start queued, read out by running the queue dry with
    /// every give answered at once.
    /// </summary>
    private static List<uint> Queued(
        ProfileGiveController controller,
        FakeAutomation automation)
    {
        automation.Gives.Clear();
        IReadOnlyList<PluginInventoryItem> owned = automation.ItemsValue;
        for (int tick = 0; tick < 20 && controller.IsRunning; tick++)
        {
            controller.Tick(0.1d, canAct: true);
            if (automation.Gives.Count > 0)
            {
                uint last = automation.Gives[^1].Item;
                automation.ItemsValue =
                    [.. automation.ItemsValue.Where(item => item.ObjectId != last)];
            }
        }
        automation.ItemsValue = owned;
        return [.. automation.Gives.Select(static give => give.Item)];
    }

    /// <summary>A fake with one target of that name, so many meters off.</summary>
    private static FakeAutomation Nearby(string name, double meters) => new()
    {
        NavigationSnapshot = new PluginNavigationSnapshot(
            IsAvailable: true,
            IsPortalSpace: false,
            LocalObjectId: 1u,
            Position: new PluginNavigationPosition(
                0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
            IsMoving: false,
            IsAirborne: false),
        ObjectsValue = [Standing(name, 100u, meters / 240d, 0d)],
    };

    private static PluginWorldObject Standing(
        string name,
        uint objectId,
        double eastWest,
        double elevation) => new(
            objectId, 0u, name, PluginObjectClass.Player, 0u, 0u, 0u)
    {
        HasPosition = true,
        Position = new PluginNavigationPosition(
            0x00010001u, eastWest, 0d, elevation, 0f, IsOutdoor: true),
    };

    private static PluginInventoryItem Item(uint id, string name) => new(
        id, 0u, name, 0u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
        1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);

    private static PluginInventoryItem Stack(uint id, string name, int size) =>
        Item(id, name) with { StackSize = size };

    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, IItemAutomation,
          IWorldObjectAutomation, INavigationAutomation
    {
        public bool IsAvailable { get; set; } = true;
        public ICharacterInfo Character => this;
        public INavigationAutomation Navigation => this;

        /// <summary>Where the character stands; unavailable until a test sets it.</summary>
        public PluginNavigationSnapshot NavigationSnapshot { get; set; }

        public PluginNavigationSnapshot Snapshot => NavigationSnapshot;

        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }

        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent) =>
            PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Unavailable;

        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => NoOpAutomationSurface.Instance;
        public IItemAutomation Items => this;
        public IWorldObjectAutomation Objects => this;
        public bool IsInWorld => IsAvailable;
        public string Name => "Tester";
        public uint ObjectId => 1u;
        public uint CurrentHealth => 0u;
        public uint MaxHealth => 0u;
        public uint CurrentStamina => 0u;
        public uint MaxStamina => 0u;
        public uint CurrentMana => 0u;
        public uint MaxMana => 0u;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginInventoryItem> ItemsValue { get; set; } = [];
        public IReadOnlyList<PluginWorldObject> ObjectsValue { get; set; } = [];
        public PluginInventoryCompletion Completion { get; set; }
        public List<(uint Item, uint Target, uint Amount)> Gives { get; } = [];

        /// <summary>What the client has selected, if anything.</summary>
        public uint? SelectedObjectId { get; set; }
        bool IItemAutomation.IsAvailable => true;
        bool IItemAutomation.IsBusy => false;
        bool IWorldObjectAutomation.IsAvailable => true;
        public PluginInventoryCompletion LastInventoryCompletion => Completion;
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => ItemsValue;
        public IReadOnlyList<PluginWorldObject> CaptureObjects() => ObjectsValue;
        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginWorldObject item in ObjectsValue)
            {
                if (item.ObjectId == objectId)
                {
                    value = item;
                    return true;
                }
            }
            value = default;
            return false;
        }
        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            properties = new PluginItemProperties(
                new Dictionary<uint, int>(),
                new Dictionary<uint, long>(),
                new Dictionary<uint, bool>(),
                new Dictionary<uint, double>(),
                new Dictionary<uint, string>(),
                new Dictionary<uint, uint>(),
                new Dictionary<uint, uint>());
            return true;
        }
        /// <summary>What the client answers a give with.</summary>
        public PluginItemCommandStatus GiveStatus { get; set; } =
            PluginItemCommandStatus.Started;

        public PluginItemCommandResult Give(
            uint objectId,
            uint targetObjectId,
            uint amount = 0u)
        {
            Gives.Add((objectId, targetObjectId, amount));
            return new PluginItemCommandResult(GiveStatus);
        }
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }
    }

    private sealed class FakeHost(
        FakeAutomation automation,
        IPluginStorage storage) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new FakeLogger();
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection(automation);
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => storage;
        public IAutomationSurface Automation => automation;
        public IPluginStorage VtankProfiles => storage;
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text =
            new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            _text.TryGetValue(key, out string? value) ? value : null;
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
        public event Action<WorldEntitySnapshot> EntitySpawned
        {
            add { }
            remove { }
        }
        public event Action<double> Tick
        {
            add { }
            remove { }
        }
    }

    private sealed class FakeSelection(FakeAutomation automation) : ISelectionService
    {
        public uint? SelectedObjectId => automation.SelectedObjectId;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }
        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }
}
