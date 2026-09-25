using System.Xml.Linq;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Tinkering;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The Tinker page and its three commands: that the controls on it reach the
/// run rather than a second copy of its state, and that the commands print
/// what a macro written against them expects.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const uint WeaponTinkeringSkill = 28u;

    /// <summary>
    /// The page is a tab like the others: it can be shown, it is enabled, and
    /// showing it takes the selection off whichever page was open.
    /// Mutation: bind TinkerVisible to a constant and the page never hides.
    /// </summary>
    [Fact]
    public void TheTinkerTabShowsAndHidesWithTheRest()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.True(panel.TinkerTabEnabled);
        Assert.False(panel.TinkerSelected);

        panel.ShowTinker();

        Assert.True(panel.TinkerSelected);
        Assert.True(panel.TinkerVisible);
        Assert.False(panel.UbVisible);
        Assert.False(panel.OptionsVisible);

        panel.ShowOptions();

        Assert.False(panel.TinkerVisible);
    }

    /// <summary>
    /// The Tinker page's markup is one group of its own, so a tab added to
    /// this window on another branch merges beside it instead of inside it.
    /// Mutation: fold the controls into an existing page's group and this fails.
    /// </summary>
    [Fact]
    public void TheTinkerPageIsOneSelfContainedGroupInTheMarkup()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        XElement group = Assert.Single(
            root.Elements("group"),
            static element => (string?)element.Attribute("visible") == "{TinkerVisible}");
        Assert.Equal("left top right bottom", (string?)group.Attribute("anchor"));
        Assert.Equal(2, group.Elements("list").Count());
        Assert.Equal(4, group.Elements("menu").Count());
        Assert.Contains(
            group.Elements("button"),
            static button => (string?)button.Attribute("text") == "Add Selected Item");
        Assert.Contains(
            group.Elements("button"),
            static button => (string?)button.Attribute("text") == "Rend All");
    }

    /// <summary>
    /// Add Selected Item takes what the client has selected, and narrows the
    /// salvage choice to what is worth putting on that kind of thing -- a
    /// long sword is offered granite and iron, a ring is not.
    /// Mutation: leave the choice list alone and the ring is still offered iron.
    /// </summary>
    [Fact]
    public void AddSelectedItemTakesTheSelectionAndNarrowsTheSalvageChoice()
    {
        var automation = new FakeAutomation();
        automation.ItemEntries = [TinkerWeapon(0x100u), TinkerRing(0x101u)];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);

        panel.AddSelectedTinkerItem();

        Assert.Equal("Iron Long Sword", panel.TinkerItemName);
        Assert.Contains(TinkerJobManager.GraniteIron, panel.TinkerSalvageNames);
        Assert.Contains("Iron", panel.TinkerSalvageNames);

        host.Selection.Select(0x101u);
        panel.AddSelectedTinkerItem();

        Assert.Equal("Gold Ring", panel.TinkerItemName);
        Assert.DoesNotContain("Iron", panel.TinkerSalvageNames);
        Assert.Equal(["Gold", "Moonstone", "Linen", "Pine"], panel.TinkerSalvageNames);
    }

    /// <summary>
    /// An item that cannot take a tinker is refused by name rather than
    /// silently accepted and then planned as an empty run.
    /// Mutation: drop the check and the page holds a food item.
    /// </summary>
    [Fact]
    public void AnItemThatCannotBeTinkeredIsRefused()
    {
        var automation = new FakeAutomation();
        automation.ItemEntries = [TinkerWeapon(0x100u) with
        {
            ObjectClass = PluginObjectClass.Food,
            Name = "Bread",
        }];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);

        panel.AddSelectedTinkerItem();

        Assert.Equal("[None]", panel.TinkerItemName);
        Assert.Equal("Item cannot be tinkered", panel.TinkerNotice);
    }

    /// <summary>
    /// Populate fills the list off the packs, and Start hands the first bag
    /// to the server. The list and the run are the same plan, not two.
    /// Mutation: rebuild the plan in Start and the row's bag is not the one applied.
    /// </summary>
    [Fact]
    public void PopulateFillsTheListAndStartAppliesItsFirstBag()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
            SalvageBag(0x202u, TinkerMaterial.Iron, 3d),
        ];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        panel.SelectTinkerMaxTinks("2");

        panel.PopulateTinkerList();

        Assert.Equal(["1", "2"], panel.TinkerRowNumbers);
        Assert.Equal(["Iron Long Sword", "Iron Long Sword"], panel.TinkerRowItems);
        Assert.Equal(["Iron(2) ", "Iron(3) "], panel.TinkerRowSalvage);
        Assert.All(panel.TinkerRowChances, static text => Assert.EndsWith("%", text.Trim()));

        panel.StartTinker();

        Assert.Equal([(0x201u, 0x100u)], automation.Applied);
    }

    /// <summary>
    /// The stop-at choice is the setting: changing it on the page writes the
    /// row the UB page edits, and drops the plan built against the old one.
    /// Mutation: keep the plan and a run stops at a number nobody chose.
    /// </summary>
    [Fact]
    public void TheStopAtChoiceWritesTheSettingAndDropsThePlanBuiltAgainstTheOldOne()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
            SalvageBag(0x202u, TinkerMaterial.Iron, 3d),
        ];
        var host = new FakeHost(automation, new MemoryStorage());
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        panel.SelectTinkerMaxTinks("2");
        panel.PopulateTinkerList();
        Assert.Equal(2, panel.TinkerRowNumbers.Count);

        panel.SelectTinkerMaxTinks("1");

        Assert.Equal("1", panel.SelectedTinkerMaxTinks);
        panel.SetUbFilterText("AutoTinker.MaxTinks");
        Assert.Equal("1", panel.UbSettingValues[0]);

        panel.PopulateTinkerList();
        Assert.Single(panel.TinkerRowNumbers);
    }

    /// <summary>
    /// The minimum-percent field is the setting itself, not a copy of it: what
    /// is typed on the Tinker page shows on the UB page.
    /// Mutation: keep the typed text in a field of its own and the setting
    /// still reads 99.5.
    /// </summary>
    [Fact]
    public void TheMinimumPercentFieldWritesTheSettingInTheInvariantForm()
    {
        var panel = new MossTankPanel(
            new FakeHost(new FakeAutomation(), new MemoryStorage()));

        Assert.Equal("99.5", panel.TinkerMinimumPercentText);

        panel.SetTinkerMinimumPercentText("42.5");

        panel.SetUbFilterText("AutoTinker.MinPercentage");
        Assert.Equal("42.5", panel.UbSettingValues[0]);
    }

    /// <summary>
    /// Choosing a damage type moves the salvage choice to the one that rends
    /// it, which is what makes the Imbue page usable without a lookup table.
    /// Mutation: leave the salvage alone and fire is rended with an emerald.
    /// </summary>
    [Fact]
    public void ChoosingADamageTypeMovesTheSalvageChoiceToTheOneThatRendsIt()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        panel.SelectImbueDamageType("Fire");
        Assert.Equal("Red Garnet", panel.SelectedImbueSalvage);

        panel.SelectImbueDamageType("Electric");
        Assert.Equal("Jet", panel.SelectedImbueSalvage);

        Assert.Equal(10, panel.ImbueSalvageNames.Count);
        Assert.DoesNotContain("Iron", panel.ImbueSalvageNames);
    }

    /// <summary>
    /// Rend All plans one rend per weapon, matched to what it strikes with.
    /// Mutation: plan with the page's chosen salvage and the cold weapon gets a garnet.
    /// </summary>
    [Fact]
    public void RendAllPlansOneRendPerWeaponMatchedToItsOwnElement()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 600u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u) with { DamageType = 16 },
            TinkerWeapon(0x101u) with { Name = "Iron Dagger", DamageType = 8 },
            SalvageBag(0x201u, TinkerMaterial.RedGarnet, 5d),
            SalvageBag(0x202u, TinkerMaterial.Aquamarine, 5d),
        ];
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.RendAllImbue();

        Assert.Equal(["Iron Long Sword", "Iron Dagger"], panel.ImbueRowItems);
        Assert.Equal(["Red Garnet(5) ", "Aquamarine(5) "], panel.ImbueRowSalvage);
    }

    /// <summary>
    /// A row that came off draws green and one that did not draws red, so a
    /// run that half worked is readable at a glance.
    /// Mutation: return one colour for every row and the failure looks fine.
    /// </summary>
    [Fact]
    public void ARowsColourFollowsWhetherTheAttemptCameOff()
    {
        var automation = new FakeAutomation();
        automation.Name = "Acdream";
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
            SalvageBag(0x202u, TinkerMaterial.Iron, 3d),
        ];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        panel.PopulateTinkerList();
        panel.StartTinker();

        Assert.Equal([(0x201u, 0x100u)], automation.Applied);
        IReadOnlyList<uint> before = panel.TinkerRowColors;
        Assert.Equal(before[0], before[1]);

        automation.PostChat(
            "Acdream fails to apply the Iron Salvage (100) "
            + "(workmanship 2.00) to the Iron Long Sword. It is destroyed.");
        panel.OnTick(0.1d);

        Assert.NotEqual(before[0], panel.TinkerRowColors[0]);
        Assert.Equal(before[1], panel.TinkerRowColors[1]);
    }

    /// <summary>
    /// Clicking a row selects its item in the client, which is how the page
    /// hands an item back to the player to look at.
    /// Mutation: select the bag instead and the wrong thing is highlighted.
    /// </summary>
    [Fact]
    public void ClickingARowSelectsItsItemInTheClient()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
        ];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        panel.PopulateTinkerList();
        host.Selection.Clear();

        panel.SelectTinkerRow(0);

        Assert.Equal(0x100u, host.Selection.SelectedObjectId);
        Assert.Equal(0, panel.SelectedTinkerRowIndex);
    }

    // ── the commands ────────────────────────────────────────────────────

    /// <summary>
    /// <c>/ub getjob</c> prints the queue, and says so when there is none.
    /// Mutation: print nothing when the queue is empty and a macro cannot tell.
    /// </summary>
    [Fact]
    public void GetJobPrintsTheQueueAndSaysSoWhenThereIsNone()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
        ];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        UbCommand(panel, "getjob");
        Assert.Equal(["[UB] i'm out of jobs"], automation.Messages);

        automation.Messages.Clear();
        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        panel.PopulateTinkerList();
        UbCommand(panel, "getjob");

        Assert.Equal("[UB] Target item: Iron Long Sword", automation.Messages[0]);
        Assert.StartsWith("[UB] salvage: Iron Salvage (100)", automation.Messages[1]);
    }

    /// <summary>
    /// <c>/ub autotinker</c> starts the run the page has planned; with no
    /// plan it says so rather than looking as though it started one.
    /// Mutation: start regardless and an empty run reports itself as running.
    /// </summary>
    [Fact]
    public void AutoTinkerStartsThePlannedRunAndSaysSoWhenThereIsNothingToRun()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
        ];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        UbCommand(panel, "autotinker");
        Assert.Equal(["[UB] i'm out of jobs"], automation.Messages);
        Assert.Empty(automation.Applied);

        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        panel.PopulateTinkerList();
        UbCommand(panel, "autotinker");

        Assert.Equal([(0x201u, 0x100u)], automation.Applied);
    }

    /// <summary>
    /// <c>/ub tinkcalc</c> prints what the chosen salvage is worth on the
    /// item, and for a melee weapon how the remaining attempts split between
    /// granite and iron.
    /// Mutation: print the odds without the attempt number and the line stops
    /// saying which tink it is about.
    /// </summary>
    [Fact]
    public void TinkCalcPrintsTheOddsAndTheGraniteIronSplit()
    {
        var automation = new FakeAutomation();
        automation.Skills = [Skill(WeaponTinkeringSkill, 500u)];
        automation.ItemEntries =
        [
            TinkerWeapon(0x100u),
            SalvageBag(0x201u, TinkerMaterial.Iron, 2d),
        ];
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x100u);
        panel.AddSelectedTinkerItem();
        panel.SelectTinkerSalvage("Iron");
        automation.Messages.Clear();

        UbCommand(panel, "tinkcalc");

        Assert.StartsWith("[UB] 1: Applying ws 2 Iron", automation.Messages[0]);
        Assert.Contains("with a successChance of", automation.Messages[0]);
        Assert.Contains(
            automation.Messages,
            static line => line.Contains("Final max damage:", StringComparison.Ordinal)
                && line.Contains("granite", StringComparison.Ordinal)
                && line.Contains("iron", StringComparison.Ordinal));
    }

    /// <summary>
    /// With nothing selected and nothing on the page, the calculation says so
    /// rather than throwing.
    /// Mutation: read the selection without a guard and this throws.
    /// </summary>
    [Fact]
    public void TinkCalcWithNothingSelectedSaysSo()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "tinkcalc");

        Assert.Equal(["[UB] Nothing selected"], automation.Messages);
    }

    /// <summary>
    /// The three commands are in the help catalogue with the usage lines a
    /// macro written against the original expects.
    /// Mutation: leave one out and /vt help stops listing it.
    /// </summary>
    [Fact]
    public void TheThreeTinkeringCommandsCarryTheirUsageLines()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "help autotinker");
        Assert.Equal("[UB] Usage: /ub autotinker", automation.Messages[0]);

        automation.Messages.Clear();
        UbCommand(panel, "help getjob");
        Assert.Equal("[UB] Usage: /ub getjob", automation.Messages[0]);

        automation.Messages.Clear();
        UbCommand(panel, "help tinkcalc");
        Assert.Equal("[UB] Usage: /ub tinkcalc", automation.Messages[0]);
    }

    private static PluginSkillInfo Skill(uint skillId, uint current) =>
        new(skillId, "Tinkering", PluginSkillTraining.Trained, current);

    private static PluginInventoryItem TinkerWeapon(uint objectId) =>
        BareItem(objectId, "Iron Long Sword") with
        {
            ObjectClass = PluginObjectClass.MeleeWeapon,
            MaterialType = (uint)TinkerMaterial.Iron,
            Workmanship = 4f,
            SalvageWorkmanship = 4d,
            MaxDamage = 20,
        };

    private static PluginInventoryItem TinkerRing(uint objectId) =>
        BareItem(objectId, "Gold Ring") with
        {
            ObjectClass = PluginObjectClass.Jewelry,
            MaterialType = (uint)TinkerMaterial.Gold,
            Workmanship = 4f,
            SalvageWorkmanship = 4d,
        };

    private static PluginInventoryItem SalvageBag(
        uint objectId,
        TinkerMaterial material,
        double workmanship) =>
        BareItem(objectId, TinkerMaterials.Name((int)material) + " Salvage (100)") with
        {
            ObjectClass = PluginObjectClass.Salvage,
            MaterialType = (uint)material,
            SalvageWorkmanship = workmanship,
            Structure = 100,
            MaximumStructure = 100,
        };

    private static PluginInventoryItem BareItem(uint objectId, string name) =>
        new(
            objectId, 0u, name, 0u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0.25d, 0, 0, 0);
}
