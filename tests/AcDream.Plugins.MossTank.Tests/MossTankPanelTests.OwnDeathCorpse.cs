namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void LootOwnDeathCorpseIsOnByDefaultSetThroughCommandsAndSurvivesARestart()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Prover" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Assert.True(panel.LootOwnDeathCorpseEnabled);

        Command(panel, "opt set LootOwnDeathCorpse false");
        Assert.False(panel.LootOwnDeathCorpseEnabled);
        Command(panel, "opt get lootowndeathcorpse");
        Assert.Contains(automation.Messages, line => line.Contains(
            "Option LootOwnDeathCorpse = False", StringComparison.Ordinal));

        var restarted = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));
        Assert.False(restarted.LootOwnDeathCorpseEnabled);

        restarted.ToggleLootOwnDeathCorpse();
        Assert.True(restarted.LootOwnDeathCorpseEnabled);
    }

    [Fact]
    public void LootOwnDeathCorpseAppearsInLootingAdvancedOptions()
    {
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, new MemoryStorage()));

        Assert.DoesNotContain(VtankOptionCatalog.LootOwnDeathCorpse, VtankOptionCatalog.Names);
        panel.ShowAdvancedOptions();
        panel.SetAdvancedOptionSearchText("LootOwnDeathCorpse");
        Assert.Equal([VtankOptionCatalog.LootOwnDeathCorpse], panel.AdvancedOptionNames);
        panel.SelectAdvancedOption(0);
        Assert.Contains("death corpse", panel.AdvancedOptionDescription, StringComparison.Ordinal);

        Assert.Equal("True", panel.AdvancedOptionValueColumn[0]);
        panel.ClickAdvancedOptionValue(0);
        Assert.False(panel.LootOwnDeathCorpseEnabled);
    }
}
