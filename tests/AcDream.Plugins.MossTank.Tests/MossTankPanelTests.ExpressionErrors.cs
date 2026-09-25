using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// A code string whose run failed is reported as two UB error lines in
    /// the error text class. With Plugin.Debug on the second line is the
    /// whole exception rather than its message, and with
    /// Plugin.ErrorMessageDisplay.Enabled off (it ships on) neither line is
    /// printed. Mutation: ignoring either setting fails its block.
    /// </summary>
    [Fact]
    public void AFailedCodeStringRunFollowsTheDebugAndErrorDisplaySettings()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.EvaluateExpression("exec[`listcount[5]`]");
        Assert.Equal(
            [
                ("[UB] Error: Error running string expression: listcount[5]<EOF>", UbChat.ErrorChatType),
                ("[UB] Error: listcount[List] expects argument #1/1 to be a List but a number was passed instead. Passed value: 5", UbChat.ErrorChatType),
            ],
            automation.Posted);

        UbCommand(panel, "opt set Plugin.Debug true");
        automation.Posted.Clear();
        panel.EvaluateExpression("exec[`listcount[5]`]");
        Assert.Equal(2, automation.Posted.Count);
        (string text, int kind) = automation.Posted[1];
        Assert.Equal(UbChat.ErrorChatType, kind);
        Assert.StartsWith("[UB] Error: ", text, StringComparison.Ordinal);
        Assert.Contains("listcount[List] expects argument #1/1", text, StringComparison.Ordinal);
        Assert.Contains("\n   at ", text, StringComparison.Ordinal);

        UbCommand(panel, "opt set Plugin.ErrorMessageDisplay.Enabled false");
        automation.Posted.Clear();
        panel.EvaluateExpression("exec[`listcount[5]`]");
        Assert.Empty(automation.Posted);
    }

    /// <summary>
    /// The four UB message displays, each an on/off switch and a text class,
    /// named and defaulted as the reference ships them: generic lines on in
    /// System, debug lines on in Abuse, expression lines (what /ub mexec
    /// prints) on in System, error lines on in Help. A display that is off
    /// keeps its lines out of chat; its colour is the class its lines are
    /// printed in. Mutation: ignoring any row fails its block.
    /// </summary>
    [Fact]
    public void TheUbMessageDisplaysShowAndColourTheirLines()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        foreach ((string name, string value) in new[]
        {
            ("Plugin.GenericMessageDisplay.Enabled", "True"),
            ("Plugin.GenericMessageDisplay.Color", "5"),
            ("Plugin.DebugMessageDisplay.Enabled", "True"),
            ("Plugin.DebugMessageDisplay.Color", "14"),
            ("Plugin.ExpressionMessageDisplay.Enabled", "True"),
            ("Plugin.ExpressionMessageDisplay.Color", "5"),
            ("Plugin.ErrorMessageDisplay.Enabled", "True"),
            ("Plugin.ErrorMessageDisplay.Color", "15"),
        })
        {
            Assert.Equal(value, UbSettingDefinitions.All.Single(row => row.Name == name)
                .Default.ToStorageString());
        }

        UbCommand(panel, "opt set Plugin.ErrorMessageDisplay.Color 3");
        UbCommand(panel, "opt set Plugin.ExpressionMessageDisplay.Color 7");
        automation.Posted.Clear();
        UbCommand(panel, "opt set Nope.Missing 1");
        UbCommand(panel, "mexec 1+1");
        Assert.Equal(("[UB] Error: Invalid option: nope.missing", 3), automation.Posted[0]);
        Assert.Contains(("[UB] Evaluating expression: \"1+1\"", 7), automation.Posted);

        UbCommand(panel, "opt set Plugin.GenericMessageDisplay.Enabled false");
        automation.Posted.Clear();
        UbCommand(panel, "opt set Jumper.Attempts 4");
        Assert.Empty(automation.Posted);

        UbCommand(panel, "opt set Plugin.Debug true");
        UbCommand(panel, "opt set Plugin.DebugMessageDisplay.Color 6");
        automation.Posted.Clear();
        UbCommand(panel, "opt set Jumper.Attempts 5");
        Assert.Equal([("[UB] Jumper.Attempts (Profile) = 5", 6)], automation.Posted);
        UbCommand(panel, "opt set Plugin.DebugMessageDisplay.Enabled false");
        automation.Posted.Clear();
        UbCommand(panel, "opt set Jumper.Attempts 6");
        Assert.Empty(automation.Posted);
    }
}
