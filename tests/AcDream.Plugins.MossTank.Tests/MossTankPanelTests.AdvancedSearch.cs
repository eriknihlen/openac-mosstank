namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    private static string ReadCustomBuffValue(MossTankPanel panel, string name) =>
        ((AcDream.Plugins.MossTank.Expressions.ExpressionValue)typeof(MossTankPanel)
            .GetMethod("GetMetaOption", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)!
            .Invoke(panel, [name])!).ToDisplayString();

    [Theory]
    [InlineData("BuffProfile_Banes", "BuffProfile-Banes", "BuffProfile-Prots")]
    [InlineData("BuffProfile_Prots", "BuffProfile-Prots", "BuffProfile-Banes")]
    public void AdvancedCustomBuffEditorAppliesTheRelatedOriginalStringSetting(
        string modeName, string customName, string otherName)
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SetAdvancedOptionSearchText(modeName);
        panel.SelectAdvancedOption(0);
        Assert.False(panel.AdvancedCustomBuffVisible);
        panel.SelectAdvancedOptionChoiceText("Custom");
        Assert.True(panel.AdvancedCustomBuffVisible);
        Assert.Equal(customName, panel.AdvancedCustomBuffName);
        Assert.Equal("ALFCBPS", panel.AdvancedCustomBuffDraft);

        panel.SubmitAdvancedCustomBuff("bpa");
        Assert.Equal("ABP", ReadCustomBuffValue(panel, customName));
        Assert.Equal("ABP", panel.AdvancedCustomBuffDraft);
        Assert.Equal("ALFCBPS", ReadCustomBuffValue(panel, otherName));
        Assert.Equal("Custom", panel.AdvancedOptionChoiceText);

        panel.SubmitAdvancedCustomBuff("123");
        Assert.Equal("ABP", ReadCustomBuffValue(panel, customName));
        Assert.Contains("Use only", panel.AdvancedOptionNotice);
        panel.SubmitAdvancedCustomBuff("");
        Assert.Equal("", ReadCustomBuffValue(panel, customName));

        panel.SelectAdvancedOptionChoiceText("All");
        Assert.False(panel.AdvancedCustomBuffVisible);
        panel.SubmitAdvancedCustomBuff("L");
        Assert.Equal("", ReadCustomBuffValue(panel, customName));
        panel.SelectAdvancedOptionChoiceText("Custom");
        panel.SubmitAdvancedCustomBuff("C");
        panel.SetAdvancedOptionSearchText("no-such-option");
        Assert.False(panel.AdvancedCustomBuffVisible);
        panel.SubmitAdvancedCustomBuff("F");
        Assert.Equal("C", ReadCustomBuffValue(panel, customName));
        panel.ClearAdvancedOptionSearch();
        Assert.Equal(modeName, panel.AdvancedOptionName);
        Assert.Equal("C", panel.AdvancedCustomBuffDraft);
    }

    [Theory]
    [InlineData("BuffProfile_Banes", "BPSAC", 7)]
    [InlineData("BuffProfile_Prots", "BPSAC", 8)]
    public void AdvancedBuffProfileChoicesKeepTheirOriginalLabelsAndValues(
        string name, string label, int expectedValue)
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat"));
        Assert.Empty(panel.AdvancedOptionChoices);

        panel.SetAdvancedOptionSearchText(name);
        panel.SelectAdvancedOption(panel.AdvancedOptionNames.ToList().IndexOf(name));
        Assert.Equal(name, panel.AdvancedOptionName);
        Assert.True(panel.AdvancedOptionChoiceVisible);
        Assert.Equal(8, panel.AdvancedOptionChoices.Count);
        Assert.Contains("Custom", panel.AdvancedOptionChoices);
        Assert.Contains("All", panel.AdvancedOptionChoices);
        Assert.Contains("None", panel.AdvancedOptionChoices);

        panel.SelectAdvancedOptionChoiceText(label);
        Assert.Equal(label, panel.AdvancedOptionChoiceText);
        Assert.Equal(expectedValue.ToString(), panel.AdvancedOptionValueDraft);
        Assert.Equal(expectedValue - 1, panel.SelectedAdvancedOptionChoice);

        panel.SelectAdvancedOptionChoiceText("None");
        Assert.Equal("None", panel.AdvancedOptionChoiceText);
        Assert.Equal("3", panel.AdvancedOptionValueDraft);
        panel.ClearAdvancedOptionSearch();
        Assert.Equal(name, panel.AdvancedOptionName);
        Assert.Equal("None", panel.AdvancedOptionChoiceText);
    }

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
