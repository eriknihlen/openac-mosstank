namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void AdvancedSearchMatchesAllWordsAcrossNamesAndDescriptions()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        string[] originalNames = panel.AdvancedOptionNames.ToArray();
        panel.SetAdvancedOptionSearchText("  DOHELP   fellowship  ");
        Assert.Equal(["DoHelp"], panel.AdvancedOptionNames);
        panel.SetAdvancedOptionSearchText("DoHelp fellowship nonexistent-term");
        Assert.Empty(panel.AdvancedOptionNames);
        panel.ClearAdvancedOptionSearch();
        Assert.Equal(originalNames, panel.AdvancedOptionNames);
    }

    [Fact]
    public void AdvancedSearchPreservesSelectionByNameAndNeverChangesValues()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat"));
        bool before = panel.CombatEnabled;
        panel.SetAdvancedOptionSearchText("combat");
        Assert.Equal("EnableCombat", panel.AdvancedOptionName);
        panel.SetAdvancedOptionSearchText("DoHelp");
        Assert.False(panel.AdvancedOptionEditorVisible);
        Assert.Equal(-1, panel.SelectedAdvancedOptionIndex);
        panel.SubmitAdvancedOption(before ? "false" : "true");
        Assert.Equal(before, panel.CombatEnabled);
        panel.ClearAdvancedOptionSearch();
        Assert.Equal("EnableCombat", panel.AdvancedOptionName);
        Assert.Equal(before, panel.CombatEnabled);
    }

    [Fact]
    public void AdvancedCategoryAndTextFiltersIntersectAndRestoreSelection()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf("EnableNav"));
        panel.SetAdvancedOptionSearchText("EnableNav");
        for (int i = 0; i < panel.AdvancedOptionCategoryEnabled.Count; i++)
            panel.ToggleAdvancedOptionCategoryAt(i);
        Assert.Empty(panel.AdvancedOptionNames);
        Assert.False(panel.AdvancedOptionEditorVisible);
        for (int i = 0; i < panel.AdvancedOptionCategoryEnabled.Count; i++)
            panel.ToggleAdvancedOptionCategoryAt(i);
        Assert.Equal(["EnableNav"], panel.AdvancedOptionNames);
        Assert.Equal("EnableNav", panel.AdvancedOptionName);
    }

    [Fact]
    public void AdvancedTypedEditorsApplyOnlyTheSelectedOriginalSetting()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat"));
        Assert.True(panel.AdvancedOptionBooleanVisible);
        Assert.False(panel.AdvancedOptionChoiceVisible);
        bool before = panel.CombatEnabled;
        panel.ToggleAdvancedOptionBoolean();
        Assert.Equal(!before, panel.CombatEnabled);

        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf("UseArcs"));
        Assert.True(panel.AdvancedOptionChoiceVisible);
        Assert.Equal(["No", "At Range", "Yes"], panel.AdvancedOptionChoices);
        panel.SelectAdvancedOptionChoiceText("Yes");
        Assert.Equal("3", panel.AdvancedOptionValueDraft);
        Assert.Equal(2, panel.SelectedAdvancedOptionChoice);
        Assert.Equal("Yes", panel.AdvancedOptionChoiceText);
        panel.SelectAdvancedOptionChoiceText("not a choice");
        panel.SelectAdvancedOptionChoice(-1);
        Assert.Equal(2, panel.SelectedAdvancedOptionChoice);
        panel.ToggleAdvancedOptionBoolean();
        Assert.Equal(2, panel.SelectedAdvancedOptionChoice);

        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf("SpellDiffExcessThreshold-Hunt"));
        Assert.True(panel.AdvancedOptionNumberVisible);
        panel.SetAdvancedOptionValueDraft("37");
        panel.ApplyAdvancedOption();
        Assert.Equal("37", panel.AdvancedOptionValueDraft);
    }
}
