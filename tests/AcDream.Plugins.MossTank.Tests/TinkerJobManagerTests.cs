using System.Globalization;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Tinkering;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The tinkering run as a state machine: what it plans off the packs, what
/// it hands the server, what it says yes to, and what it does with the
/// answer. Every pin here is on behaviour the player would notice -- a bag
/// spent twice, an attempt made after a failure, a prompt left unanswered.
/// </summary>
public sealed class TinkerJobManagerTests
{
    private const uint WeaponTinkering = 28u;

    /// <summary>
    /// The plan takes the cheapest bag first and never spends one twice: the
    /// whole point of ordering by workmanship is that the good salvage is
    /// left for the attempts that need it.
    /// Mutation: stop marking a bag as spent and the same bag fills every row.
    /// </summary>
    [Fact]
    public void ThePlanSpendsTheCheapestBagsFirstAndNeverOneTwice()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 400u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 9.4d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Iron, workmanship: 2.1d));
        host.Items.Add(Bag(0x203u, TinkerMaterial.Iron, workmanship: 5.5d));
        TinkerJobManager tinker = Manager(host);

        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 3, minimumPercent: 0d);

        Assert.Equal(
            [0x202u, 0x203u, 0x201u],
            tinker.TinkerRows.Select(static row => row.SalvageObjectId));
        Assert.Equal([1, 2, 3], tinker.TinkerRows.Select(static row => row.Number));
        TinkerJob job = Assert.Single(tinker.Jobs);
        Assert.Equal([0x202u, 0x203u, 0x201u], job.SalvageToApply);
    }

    /// <summary>
    /// The floor under the odds is a floor: a bag whose chance does not clear
    /// it is passed over, and when nothing clears it nothing is queued.
    /// Mutation: compare the chance against zero and the low-skill plan fills.
    /// </summary>
    [Fact]
    public void ABagThatDoesNotClearTheMinimumPercentIsNotQueued()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 100u;
        host.Items.Add(Weapon(0x100u, workmanship: 9f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);

        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 10, minimumPercent: 99.5d);

        Assert.Empty(tinker.TinkerRows);
        Assert.Empty(tinker.Jobs);
    }

    /// <summary>
    /// Stopping at four attempts on an item that has taken one plans three,
    /// not ten. The count is where the run stops, not how many it makes.
    /// Mutation: plan maxTinks attempts instead of the remainder and this is four.
    /// </summary>
    [Fact]
    public void TheStopAtCountIsWhereTheRunStopsNotHowManyAttemptsItMakes()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f, timesTinkered: 1));
        for (uint id = 0x201u; id <= 0x210u; id++)
            host.Items.Add(Bag(id, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);

        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 4, minimumPercent: 0d);

        Assert.Equal(3, tinker.TinkerRows.Count);
        Assert.Equal([2, 3, 4], tinker.TinkerRows.Select(static row => row.Number));
    }

    /// <summary>
    /// Starting the run selects the first bag and applies it to the item, in
    /// that order -- the server's crafting interaction is a use of a selected
    /// thing on another.
    /// Mutation: apply without selecting and the selection stays where it was.
    /// </summary>
    [Fact]
    public void StartingTheRunSelectsTheFirstBagAndAppliesItToTheItem()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 1, minimumPercent: 0d);

        tinker.Start();

        Assert.Equal([(0x201u, 0x100u)], host.Applied);
        Assert.Equal(0x201u, host.Selection.SelectedObjectId);
        Assert.True(tinker.IsRunning);
    }

    /// <summary>
    /// The client's craft prompt is answered yes while an attempt is in
    /// flight, and left alone when none is -- a prompt raised by something
    /// else is not this plugin's to answer.
    /// Mutation: answer every confirmation and the idle case answers one too.
    /// </summary>
    [Fact]
    public void TheCraftPromptIsAnsweredYesOnlyWhileAnAttemptIsInFlight()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);

        host.Events.RaiseConfirmation(new PluginConfirmation(7u, 5, "before"));
        Assert.Empty(host.Answered);

        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 1, minimumPercent: 0d);
        tinker.Start();
        host.Events.RaiseConfirmation(
            new PluginConfirmation(9u, 5, "You have a 98% chance of success."));

        Assert.Equal([(9u, true)], host.Answered);
        Assert.Contains(
            host.Written,
            static line => line.StartsWith("AutoTinker: Clicking Yes on", StringComparison.Ordinal));
    }

    /// <summary>
    /// A prompt that is not the crafting one is left for whoever raised it.
    /// Mutation: drop the type check and the trade prompt is answered too.
    /// </summary>
    [Fact]
    public void APromptThatIsNotTheCraftingOneIsLeftAlone()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 1, minimumPercent: 0d);
        tinker.Start();

        host.Events.RaiseConfirmation(new PluginConfirmation(11u, 2, "Swear allegiance?"));

        Assert.Empty(host.Answered);
    }

    /// <summary>
    /// The line that says the tink came off moves the run on to the next bag,
    /// and marks the row that spent the first one.
    /// Mutation: ignore the success line and the second bag is never applied.
    /// </summary>
    [Fact]
    public void TheSuccessLineMarksTheRowAndMovesOnToTheNextBag()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Iron, workmanship: 3d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 2, minimumPercent: 0d);
        tinker.Start();

        host.Post(
            "Acdream successfully applies the Iron Salvage (100) "
            + "(workmanship 2.00) to the Iron Long Sword.");
        tinker.OnTick(0.1d);

        Assert.Equal([(0x201u, 0x100u), (0x202u, 0x100u)], host.Applied);
        Assert.True(tinker.TinkerRows[0].Succeeded);
        Assert.Null(tinker.TinkerRows[1].Succeeded);
    }

    /// <summary>
    /// A failure ends the job on that item. The attempt is spent either way,
    /// and every bag still queued was chosen against the old attempt count,
    /// so carrying on would be spending good salvage at odds nobody agreed to.
    /// Mutation: carry on after a failure and the second bag is applied.
    /// </summary>
    [Fact]
    public void AFailureEndsTheJobOnThatItemRatherThanSpendingTheNextBag()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Iron, workmanship: 3d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 2, minimumPercent: 0d);
        tinker.Start();

        host.Post(
            "Acdream fails to apply the Iron Salvage (100) "
            + "(workmanship 2.00) to the Iron Long Sword. The salvage is destroyed.");
        tinker.OnTick(0.1d);

        Assert.Equal([(0x201u, 0x100u)], host.Applied);
        Assert.False(tinker.TinkerRows[0].Succeeded);
        Assert.False(tinker.IsRunning);
        Assert.Contains("Done tinkering", host.Written);
    }

    /// <summary>
    /// A line about somebody else's tinkering, or about a different bag, is
    /// not this run's answer. Without the match a passer-by's crafting line
    /// would step the run on and desynchronise it from the server.
    /// Mutation: drop the workmanship comparison and the foreign line counts.
    /// </summary>
    [Fact]
    public void ALineAboutSomebodyElsesTinkeringIsNotThisRunsAnswer()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Iron, workmanship: 3d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 2, minimumPercent: 0d);
        tinker.Start();

        host.Post(
            "Horan successfully applies the Iron Salvage (100) "
            + "(workmanship 2.00) to the Iron Long Sword.");
        host.Post(
            "Acdream successfully applies the Iron Salvage (100) "
            + "(workmanship 7.00) to the Iron Long Sword.");
        tinker.OnTick(0.1d);

        Assert.Equal([(0x201u, 0x100u)], host.Applied);
        Assert.Null(tinker.TinkerRows[0].Succeeded);
        Assert.True(tinker.IsRunning);
    }

    /// <summary>
    /// One line answers one attempt. When two lines arrive in the same batch
    /// -- the same words twice, which is what identical bags on the same item
    /// produce -- the first moves the run on to the next bag and the second
    /// must not be read as that new bag's answer: the run would be a whole
    /// attempt ahead of the server, applying salvage nobody has heard back
    /// about.
    /// Mutation: let the loop carry on past a recorded outcome and the third
    /// bag is applied off the second copy of the first bag's line.
    /// </summary>
    [Fact]
    public void ASecondCopyOfTheSameLineDoesNotAnswerTheAttemptItJustStarted()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Iron, workmanship: 2d));
        host.Items.Add(Bag(0x203u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 3, minimumPercent: 0d);
        tinker.Start();

        const string line =
            "Acdream successfully applies the Iron Salvage (100) "
            + "(workmanship 2.00) to the Iron Long Sword.";
        host.Post(line);
        host.Post(line);
        tinker.OnTick(0.1d);

        Assert.Equal([(0x201u, 0x100u), (0x202u, 0x100u)], host.Applied);
        Assert.True(tinker.TinkerRows[0].Succeeded);
        Assert.Null(tinker.TinkerRows[1].Succeeded);
        Assert.True(tinker.IsRunning);
    }

    /// <summary>
    /// An attempt nobody ever answers ends the run rather than leaving it
    /// looking busy forever. The reference waits for its line without a
    /// bound, which strands a run when the line never comes.
    /// Mutation: drop the timeout and the run is still going after a minute.
    /// </summary>
    [Fact]
    public void AnAttemptNobodyAnswersEndsTheRunRatherThanStrandingIt()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 1, minimumPercent: 0d);
        tinker.Start();

        for (int frame = 0; frame < 30; frame++)
            tinker.OnTick(1d);

        Assert.False(tinker.IsRunning);
        Assert.Contains(
            host.Written,
            static line => line.Contains("no word back", StringComparison.Ordinal));
    }

    /// <summary>
    /// A tinkering skill that has dropped since the plan was made stops the
    /// run: the odds every queued bag was chosen at no longer hold.
    /// Mutation: drop the guard and the second bag is applied at worse odds.
    /// </summary>
    [Fact]
    public void ASkillThatHasDroppedSinceThePlanStopsTheRun()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Iron, workmanship: 3d));
        TinkerJobManager tinker = Manager(host);
        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 2, minimumPercent: 0d);
        tinker.Start();

        host.Skills[WeaponTinkering] = 200u;
        host.Post(
            "Acdream successfully applies the Iron Salvage (100) "
            + "(workmanship 2.00) to the Iron Long Sword.");
        tinker.OnTick(0.1d);

        Assert.Equal([(0x201u, 0x100u)], host.Applied);
        Assert.Contains(
            "tinkering skill decreased... stopping tinkering.",
            host.Written);
    }

    /// <summary>
    /// What can be tinkered at all: the six kinds of thing, with a
    /// workmanship, with attempts left. Clothing has to be one of the three
    /// armoured slots and carry armour of its own -- tinkering a shirt does
    /// nothing.
    /// Mutation: drop the clothing rule and the shirt is offered.
    /// </summary>
    [Fact]
    public void OnlyTheRightKindOfThingWithAttemptsLeftCanBeTinkered()
    {
        Assert.True(TinkerJobManager.CanBeTinkered(Weapon(1u, 5f)));
        Assert.False(TinkerJobManager.CanBeTinkered(Weapon(2u, 5f, timesTinkered: 10)));
        Assert.False(TinkerJobManager.CanBeTinkered(Weapon(3u, 0f)));
        Assert.False(TinkerJobManager.CanBeTinkered(
            Weapon(4u, 5f) with { ObjectClass = PluginObjectClass.Food }));

        PluginInventoryItem cap = Weapon(5u, 5f) with
        {
            ObjectClass = PluginObjectClass.Clothing,
            ValidLocations = 1u,
            ArmorLevel = 40,
        };
        Assert.True(TinkerJobManager.CanBeTinkered(cap));
        Assert.False(TinkerJobManager.CanBeTinkered(cap with { ArmorLevel = 0 }));
        Assert.False(TinkerJobManager.CanBeTinkered(cap with { ValidLocations = 4u }));
    }

    /// <summary>
    /// What can still be imbued: a workmanship, no imbue already, attempts
    /// left. The imbue is the thing that cannot be done twice.
    /// Mutation: drop the imbue check and an already-rended weapon is offered.
    /// </summary>
    [Fact]
    public void AnAlreadyImbuedItemCannotBeImbuedAgain()
    {
        PluginInventoryItem weapon = Weapon(1u, 5f);
        Assert.True(TinkerJobManager.CanBeImbued(weapon));
        Assert.False(TinkerJobManager.CanBeImbued(weapon with { ImbuedEffect = 1 }));
        Assert.False(TinkerJobManager.CanBeImbued(weapon with { NumTimesTinkered = 10 }));
        Assert.False(TinkerJobManager.CanBeImbued(weapon with { Workmanship = 0f }));
    }

    /// <summary>
    /// A rend is planned only for weapons that strike with the chosen damage
    /// type, and only for the kinds of thing the chosen salvage can go into.
    /// Mutation: drop the damage-type check and the cold weapon is queued too.
    /// </summary>
    [Fact]
    public void ARendIsPlannedOnlyForWeaponsOfTheChosenDamageType()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 5f) with { DamageType = 1 });
        host.Items.Add(Weapon(0x101u, workmanship: 5f) with
        {
            Name = "Iron Dagger",
            DamageType = 8,
        });
        host.Items.Add(Bag(0x201u, TinkerMaterial.ImperialTopaz, workmanship: 6d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.ImperialTopaz, workmanship: 7d));
        TinkerJobManager tinker = Manager(host);

        tinker.PopulateImbueList("Slashing", "Imperial Topaz");

        TinkerListRow row = Assert.Single(tinker.ImbueRows);
        Assert.Equal(0x100u, row.ItemObjectId);
        Assert.Equal(0x201u, row.SalvageObjectId);
    }

    /// <summary>
    /// A rend is one attempt on the item, whatever has already gone into it.
    /// A weapon nine tinks deep is the ordinary case -- the imbue is normally
    /// the last thing put in -- and it has to be offered like any other.
    /// Mutation: plan the rend against the attempts left instead of one, and
    /// a weapon with any prior tink gets no rend at all.
    /// </summary>
    [Fact]
    public void ARendIsPlannedForAWeaponThatHasAlreadyBeenTinkeredNineTimes()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 5f, timesTinkered: 9) with
        {
            DamageType = 1,
        });
        host.Items.Add(Bag(0x201u, TinkerMaterial.ImperialTopaz, workmanship: 6d));
        TinkerJobManager tinker = Manager(host);

        tinker.PopulateImbueList("Slashing", "Imperial Topaz");

        TinkerListRow row = Assert.Single(tinker.ImbueRows);
        Assert.Equal(0x100u, row.ItemObjectId);
        Assert.Equal(0x201u, row.SalvageObjectId);
        TinkerJob job = Assert.Single(tinker.Jobs);
        Assert.Equal([0x201u], job.SalvageToApply);
    }

    /// <summary>
    /// Rend All picks the salvage that matches what each weapon already
    /// strikes with, and leaves out anything whose odds fall below thirty
    /// percent.
    /// Mutation: drop the thirty-percent floor and the hopeless weapon queues.
    /// </summary>
    [Fact]
    public void RendAllMatchesEachWeaponsElementAndSkipsTheHopelessOnes()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 560u;
        host.Items.Add(Weapon(0x100u, workmanship: 5f) with { DamageType = 16 });
        host.Items.Add(Weapon(0x101u, workmanship: 10f) with
        {
            Name = "Iron Dagger",
            DamageType = 8,
        });
        host.Items.Add(Bag(0x201u, TinkerMaterial.RedGarnet, workmanship: 6d));
        host.Items.Add(Bag(0x202u, TinkerMaterial.Aquamarine, workmanship: 1d));
        TinkerJobManager tinker = Manager(host);

        tinker.PopulateRendAll();

        TinkerListRow row = Assert.Single(tinker.ImbueRows);
        Assert.Equal(0x100u, row.ItemObjectId);
        Assert.Equal(0x201u, row.SalvageObjectId);
        Assert.InRange(row.SuccessChance, 0.30d, 1d);
    }

    /// <summary>
    /// The queue in words, as the job command prints it.
    /// Mutation: print the object ids alone and the names go missing.
    /// </summary>
    [Fact]
    public void TheQueueReadsBackAsOneLinePerItemAndOnePerBagWaitingOnIt()
    {
        var host = new TinkerHost();
        host.Skills[WeaponTinkering] = 500u;
        host.Items.Add(Weapon(0x100u, workmanship: 4f));
        host.Items.Add(Bag(0x201u, TinkerMaterial.Iron, workmanship: 2d));
        TinkerJobManager tinker = Manager(host);

        Assert.Equal(["i'm out of jobs"], tinker.DescribeJobs());

        tinker.PopulateTinkerList(0x100u, "Iron", maximumAttempts: 1, minimumPercent: 0d);

        Assert.Equal(
            ["Target item: Iron Long Sword", "salvage: Iron Salvage (100) 513"],
            tinker.DescribeJobs());
    }

    /// <summary>
    /// Granite and iron are decided attempt by attempt against a running
    /// estimate, because each choice changes what the next one is worth. The
    /// weapon here is worth more granite than iron, and a plan for it spends
    /// both.
    /// Mutation: decide once and hold it, and the plan is ten of one kind.
    /// </summary>
    [Fact]
    public void GraniteAndIronAreWeighedAfreshForEveryAttempt()
    {
        PluginInventoryItem weapon = Weapon(0x100u, workmanship: 5f) with
        {
            MaxDamage = 20,
            DamageVariance = 0.5d,
        };
        (int granite, int iron, double finalDamage) =
            TinkerJobManager.BestGraniteIron(weapon);

        Assert.Equal(10, granite + iron);
        Assert.True(granite > 0);
        Assert.True(iron > 0);
        Assert.True(finalDamage > TinkerCalc.DamagePerSecond(44d, 0.5d));
    }

    /// <summary>
    /// The damage estimate starts from the weapon's own ceiling, plus what
    /// its blood-thirst cantrips add, plus the flat allowance.
    /// Mutation: leave the cantrips out and the legendary one counts for nothing.
    /// </summary>
    [Fact]
    public void TheDamageEstimateCountsTheBloodThirstCantripsAndTheAllowance()
    {
        PluginInventoryItem plain = Weapon(1u, 5f) with { MaxDamage = 50 };
        Assert.Equal(74d, TinkerJobManager.DamageCeiling(plain));

        PluginInventoryItem enchanted = plain with
        {
            AppraisedSpellIds = new uint[] { 6089u, 2598u },
        };
        Assert.Equal(86d, TinkerJobManager.DamageCeiling(enchanted));
    }

    private static TinkerJobManager Manager(TinkerHost host) =>
        new(host, host.Written.Add, () => host.CharmedSmith);

    private static PluginInventoryItem Weapon(
        uint objectId,
        float workmanship,
        int timesTinkered = 0) =>
        new(
            objectId,
            0u,
            "Iron Long Sword",
            0u,
            1u,
            0u,
            0u,
            0u,
            0u,
            0u,
            0u,
            1,
            0,
            0,
            0u,
            0,
            0,
            0u,
            false,
            0d,
            0,
            1,
            10,
            0.25d,
            0,
            0,
            0)
        {
            ObjectClass = PluginObjectClass.MeleeWeapon,
            MaterialType = (uint)TinkerMaterial.Iron,
            Workmanship = workmanship,
            SalvageWorkmanship = workmanship,
            NumTimesTinkered = timesTinkered,
            MaxDamage = 20,
        };

    private static PluginInventoryItem Bag(
        uint objectId,
        TinkerMaterial material,
        double workmanship) =>
        new(
            objectId,
            0u,
            TinkerMaterials.Name((int)material) + " Salvage (100)",
            0u,
            1u,
            0u,
            0u,
            0u,
            0u,
            0u,
            0u,
            1,
            100,
            100,
            0u,
            0,
            0,
            0u,
            false,
            0d,
            0,
            0,
            0,
            0d,
            0,
            0,
            0)
        {
            ObjectClass = PluginObjectClass.Salvage,
            MaterialType = (uint)material,
            SalvageWorkmanship = workmanship,
        };

    /// <summary>
    /// The client as the tinkering run sees it: the packs, the skills, the
    /// selection, the chat log, and the two things it asks the client to do.
    /// </summary>
    private sealed class TinkerHost
        : IPluginHost, IAutomationSurface, ICharacterInfo, IPluginChat,
          IItemAutomation, IDialogAutomation
    {
        private ulong _chatSequence;

        public List<PluginInventoryItem> Items { get; } = [];
        public Dictionary<uint, uint> Skills { get; } = [];
        public List<(uint One, uint Two)> Applied { get; } = [];
        public List<(uint ContextId, bool Accept)> Answered { get; } = [];
        public List<string> Written { get; } = [];
        public List<PluginChatMessage> ChatLines { get; } = [];
        public bool CharmedSmith { get; set; }

        public void Post(string text) =>
            ChatLines.Add(new PluginChatMessage(
                ++_chatSequence, 0u, 0, string.Empty, text, string.Empty));

        // IPluginHost
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new SilentLogger();
        public IGameState State { get; } = new EmptyState();
        IEvents IPluginHost.Events => Events;
        public RaisingEvents Events { get; } = new();
        ISelectionService IPluginHost.Selection => Selection;
        public RecordingSelection Selection { get; } = new();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation => this;

        // IAutomationSurface
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        IItemAutomation IAutomationSurface.Items => this;
        IDialogAutomation IAutomationSurface.Dialogs => this;

        // ICharacterInfo
        public bool IsInWorld => true;
        public string Name => "Acdream";
        public uint ObjectId => 0x5000000Au;
        public uint CurrentHealth => 100u;
        public uint MaxHealth => 100u;
        public uint CurrentStamina => 100u;
        public uint MaxStamina => 100u;
        public uint CurrentMana => 100u;
        public uint MaxMana => 100u;

        IReadOnlyList<PluginSkillInfo> ICharacterInfo.Skills =>
            Skills
                .Select(pair => new PluginSkillInfo(
                    pair.Key,
                    pair.Key.ToString(CultureInfo.InvariantCulture),
                    PluginSkillTraining.Trained,
                    pair.Value))
                .ToArray();

        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            if (Skills.TryGetValue(skillId, out uint current))
            {
                skill = new PluginSkillInfo(
                    skillId,
                    skillId.ToString(CultureInfo.InvariantCulture),
                    PluginSkillTraining.Trained,
                    current);
                return true;
            }
            skill = default;
            return false;
        }

        // IPluginChat
        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            ChatLines.Where(message => message.Sequence > afterSequence).ToArray();

        public void PostSystemMessage(string text) => Written.Add(text);

        // IItemAutomation
        bool IItemAutomation.IsAvailable => true;

        IReadOnlyList<PluginInventoryItem> IItemAutomation.CaptureOwnedItems() =>
            Items;

        PluginItemCommandResult IItemAutomation.Apply(uint objectId, uint targetObjectId)
        {
            Applied.Add((objectId, targetObjectId));
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        // IDialogAutomation
        bool IDialogAutomation.Answer(uint contextId, bool accept)
        {
            Answered.Add((contextId, accept));
            return true;
        }
    }

    private sealed class RaisingEvents : IEvents
    {
        private Action<PluginConfirmation>? _confirmation;

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

        public event Action<PluginConfirmation> ConfirmationRequested
        {
            add => _confirmation += value;
            remove => _confirmation -= value;
        }

        public void RaiseConfirmation(PluginConfirmation confirmation) =>
            _confirmation?.Invoke(confirmation);
    }

    private sealed class RecordingSelection : ISelectionService
    {
        public uint? SelectedObjectId { get; private set; }
        public uint? PreviousObjectId { get; private set; }

        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId)
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = objectId == 0u ? null : objectId;
            return true;
        }

        public bool Clear()
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = null;
            return true;
        }
    }

    private sealed class SilentLogger : IPluginLogger
    {
        public void Info(string message)
        {
        }

        public void Warn(string message)
        {
        }

        public void Error(string message, Exception? exception = null)
        {
        }
    }

    private sealed class EmptyState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }
}
