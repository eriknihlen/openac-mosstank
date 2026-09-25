using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The macro's options are exactly the names the reference's settings table
/// holds. Every path that writes or reads one by name refuses any other name
/// the way the reference does, and nothing is stored under it.
/// </summary>
public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// The one option write path refuses a name the reference does not hold
    /// and stores nothing under it; a real name in any case is accepted.
    /// Mutation: the old store-anything fallback answers true and keeps the
    /// name.
    /// </summary>
    [Fact]
    public void TheOptionWritePathRefusesANameTheReferenceDoesNotHold()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.False(panel.SetMetaOption("MonsterRange", ExpressionValue.Number(42d)));
        Assert.False(panel.SetMetaOption("MyOwnFlag", ExpressionValue.Boolean(true)));
        Assert.True(panel.SetMetaOption("enablecombat", ExpressionValue.Boolean(false)));
        Assert.False(panel.GetMetaOptionForTest("EnableCombat"));
    }

    /// <summary>
    /// <c>vtsetsetting</c> of a name the reference does not hold answers 1
    /// and changes nothing, as the reference's does (its failure is eaten);
    /// <c>vtgetsetting</c> of one fails the expression, because the
    /// reference's lookup throws. Mutation: storing the name makes the read
    /// answer what was written.
    /// </summary>
    [Fact]
    public void VtSettingFunctionsTreatAnUnknownNameAsTheReferenceDoes()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.Equal(1d, panel.EvaluateExpression("vtsetsetting[`MyOwnFlag`,`1`]").AsNumber());
        ExpressionEvaluationException error = Assert.Throws<ExpressionEvaluationException>(
            () => panel.EvaluateExpression("vtgetsetting[`MyOwnFlag`]"));
        Assert.Contains("Error getting setting from database", error.Message, StringComparison.Ordinal);
    }

    /// <summary>
    /// <c>/vt opt get</c> and <c>/vt opt set</c> answer an unknown name in
    /// the reference's words, and the settings table's one unused name is a
    /// real option like any other. Mutation: leaving that name out answers
    /// it as unknown.
    /// </summary>
    [Fact]
    public void VtOptAnswersUnknownNamesInTheReferenceWords()
    {
        (FakeAutomation automation, MossTankPanel panel) = NamespacePanel();

        Issue(panel, VtWord, "opt set MyOwnFlag true");
        Assert.Equal("Option set: Invalid option specified.", Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Issue(panel, VtWord, "opt get MyOwnFlag");
        Assert.Equal("Option get: Invalid option specified.", Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Issue(panel, VtWord, "opt get WhoYouGonnaCall");
        Assert.Equal("Option WhoYouGonnaCall = True", Assert.Single(automation.Messages));
    }

    /// <summary>
    /// <c>uboptget</c> and <c>uboptset</c> of a name that is neither a UB
    /// setting nor one of the macro's options is the reference's invalid
    /// option: an error line and 0. Mutation: the macro-option fallback
    /// reads 0 silently and stores the name.
    /// </summary>
    [Fact]
    public void UbOptOfANameNoOneHasIsAnInvalidOption()
    {
        (FakeAutomation automation, MossTankPanel panel) = NamespacePanel();

        Assert.Equal(0d, panel.EvaluateExpression("uboptset[`MyOwnFlag`,1]").AsNumber());
        Assert.Equal("[UB] Error: Invalid option: myownflag", Assert.Single(automation.Messages));

        automation.Messages.Clear();
        Assert.Equal(0d, panel.EvaluateExpression("uboptget[`MyOwnFlag`]").AsNumber());
        Assert.Equal("[UB] Error: Invalid option: myownflag", Assert.Single(automation.Messages));
    }
}
