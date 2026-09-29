namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void OptionsTabSwitchesCanBeListedReadSetToggledAndReloadedThroughVtankCommands()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Prover" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "opt list");
        Assert.Contains(automation.Messages, line => line.Contains(
            "WalkToOwnRareCorpses", StringComparison.Ordinal));
        Assert.Contains(automation.Messages, line => line.Contains(
            "ShowNavLines", StringComparison.Ordinal));

        Command(panel, "opt set walktoowncorpses true");
        Command(panel, "opt set shownavlines true");
        Assert.True(panel.WalkToOwnRareCorpsesEnabled);
        Assert.True(panel.ShowNavLinesEnabled);

        Command(panel, "opt toggle WalkToOwnRareCorpses");
        Assert.False(panel.WalkToOwnRareCorpsesEnabled);
        Command(panel, "opt toggle WalkToOwnRareCorpses");
        Command(panel, "opt get WalkToOwnCorpses");
        Command(panel, "opt get ShowNavLines");
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option WalkToOwnRareCorpses = True", StringComparison.Ordinal));
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option ShowNavLines = True", StringComparison.Ordinal));

        var restarted = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));
        Assert.True(restarted.WalkToOwnRareCorpsesEnabled);
        Assert.True(restarted.ShowNavLinesEnabled);
    }

    [Fact]
    public void NumericOptionsTabValueSetThroughChatSurvivesProfileReload()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Prover" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "opt set PetMonsterDensity 7");
        Command(panel, "opt get PetMonsterDensity");
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option PetMonsterDensity = 7", StringComparison.Ordinal));

        var restartedAutomation = new FakeAutomation { Name = "Prover" };
        var restarted = new MossTankPanel(new FakeHost(restartedAutomation, storage));
        Command(restarted, "opt get PetMonsterDensity");
        Assert.Contains(restartedAutomation.Messages, line => line.Contains(
            "Option PetMonsterDensity = 7", StringComparison.Ordinal));
    }
}
