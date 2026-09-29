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
            "WalkToRareCorpse", StringComparison.Ordinal));
        Assert.Contains(automation.Messages, line => line.Contains(
            "ShowNavLines", StringComparison.Ordinal));

        Command(panel, "opt set WalkToRareCorpse true");
        Command(panel, "opt set shownavlines true");
        Assert.True(panel.WalkToOwnRareCorpsesEnabled);
        Assert.True(panel.ShowNavLinesEnabled);

        Command(panel, "opt get WalkToRareCorpse");
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option WalkToRareCorpse = True", StringComparison.Ordinal));
        Command(panel, "opt set WalkToRareCorpse false");
        Command(panel, "opt get WalkToRareCorpse");
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option WalkToRareCorpse = False", StringComparison.Ordinal));
        Assert.False(panel.WalkToOwnRareCorpsesEnabled);

        // Existing macros still read and write the same saved switch.
        Command(panel, "opt set walktoowncorpses true");
        Command(panel, "opt get WalkToOwnCorpses");
        Command(panel, "opt get WalkToOwnRareCorpses");
        Command(panel, "opt get ShowNavLines");
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option WalkToRareCorpse = True", StringComparison.Ordinal));
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option ShowNavLines = True", StringComparison.Ordinal));

        var restarted = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));
        Assert.True(restarted.WalkToOwnRareCorpsesEnabled);
        Assert.True(restarted.ShowNavLinesEnabled);
    }

    [Fact]
    public void RareCorpseWalkAppearsInLootingAdvancedOptionsAndEditsTheSavedSwitch()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));

        Assert.DoesNotContain(VtankOptionCatalog.WalkToRareCorpse, VtankOptionCatalog.Names);
        panel.ShowAdvancedOptions();
        panel.SetAdvancedOptionSearchText("WalkToRareCorpse");
        Assert.Equal([VtankOptionCatalog.WalkToRareCorpse], panel.AdvancedOptionNames);
        Assert.Equal("False", panel.AdvancedOptionValueColumn[0]);
        panel.SelectAdvancedOption(0);
        Assert.True(panel.AdvancedOptionBooleanVisible);
        Assert.Contains("own rare", panel.AdvancedOptionDescription, StringComparison.Ordinal);

        panel.ClickAdvancedOptionValue(0);
        Assert.True(panel.WalkToOwnRareCorpsesEnabled);
        Assert.Equal("True", panel.AdvancedOptionValueColumn[0]);

        var restarted = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));
        Assert.True(restarted.WalkToOwnRareCorpsesEnabled);

        restarted.SetAdvancedOptionSearchText(string.Empty);
        for (int index = 0; index < restarted.AdvancedOptionCategoryEnabled.Count; index++)
            if (VtankOptionCatalog.CategoryBits[index] != 0x100)
                restarted.ToggleAdvancedOptionCategoryAt(index);
        Assert.Contains(VtankOptionCatalog.WalkToRareCorpse, restarted.AdvancedOptionNames);
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
