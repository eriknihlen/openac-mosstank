using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// A meta's own <c>/vt</c> and <c>/ub</c> lines run inside its pass: the
/// client's submit runs the plugin's command before it returns, so the next
/// rule already sees what the line did (VTank dispatches a meta's chat
/// command the same way). These pin that order; the Rust line ran the lines
/// after the pass, and <c>neftet.met</c> skipped its quest turn-ins.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const string MetaLinesQuestsRoute = "NAV: nav0 circular ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n~~ }\r\n";

    private const string MetaLinesEndStates =
        "STATE: {Loaded} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\n"
        + "STATE: {Skipped} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\n"
        + "STATE: {Other} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\n";

    /// <summary>
    /// The meta loaded and switched on, the macro started, the client's
    /// submit routing <c>/vt</c> and <c>/ub</c> lines to the plugin at once,
    /// and one pass.
    /// </summary>
    private static (MossTankPanel Panel, FakeAutomation Automation, MemoryStorage Storage) RunOneMetaLinesPass(
        string defaultRules)
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Lines.af"] =
            "STATE: {Default} ~~ {\r\n" + defaultRules + "~~ }\r\n" + MetaLinesEndStates;
        storage.Text["mosstank/navs/Quests.af"] = MetaLinesQuestsRoute;
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        automation.OnSubmit = text =>
        {
            if (text.StartsWith("/vt ", StringComparison.Ordinal))
                Command(panel, text[4..]);
            else if (text.StartsWith("/ub ", StringComparison.Ordinal))
                panel.ExecuteUbCommand(new PluginCommand("ub", text[4..], text));
        };
        Command(panel, "meta load Lines");
        panel.ToggleMeta();
        panel.ToggleCombat();
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        return (panel, automation, storage);
    }

    [Fact]
    public void ARouteAMetaLineLoadsIsThereForTheNextRule()
    {
        var (panel, automation, _) = RunOneMetaLinesPass(
            "\tIF:\tAlways\r\n\t\tDO:\tChat {/vt nav load Quests}\r\n"
            + "\tIF:\tNavEmpty\r\n\t\tDO:\tSetState {Skipped}\r\n"
            + "\tIF:\tAlways\r\n\t\tDO:\tSetState {Loaded}\r\n");

        Assert.True(panel.MetaState == "Loaded", string.Join("\n", automation.Messages));
        Assert.Single(panel.RouteRows);
    }

    [Fact]
    public void ARouteAChatboxLineLoadsIsThereForTheNextRule()
    {
        var (panel, automation, _) = RunOneMetaLinesPass(
            "\tIF:\tAlways\r\n\t\tDO:\tDoExpr {chatbox[`/vt nav load Quests`]}\r\n"
            + "\tIF:\tNavEmpty\r\n\t\tDO:\tSetState {Skipped}\r\n"
            + "\tIF:\tAlways\r\n\t\tDO:\tSetState {Loaded}\r\n");

        Assert.True(panel.MetaState == "Loaded", string.Join("\n", automation.Messages));
    }

    [Fact]
    public void AStateAMetaLineSetsEndsThePass()
    {
        var (panel, automation, _) = RunOneMetaLinesPass(
            "\tIF:\tAlways\r\n\t\tDO:\tChat {/vt setmetastate Other}\r\n"
            + "\tIF:\tAlways\r\n\t\tDO:\tSetState {Skipped}\r\n");

        Assert.Equal("Other", panel.MetaState);
        Assert.Equal(
            ["[MossTank] Meta state: Default -> Other."],
            automation.Messages.Where(line => line.Contains("Meta state:", StringComparison.Ordinal)));
    }

    [Fact]
    public void AVariableAUbLineSetsIsThereForTheNextRule()
    {
        var (panel, automation, _) = RunOneMetaLinesPass(
            "\tIF:\tAlways\r\n\t\tDO:\tChat {/ub mexec setvar[seen, 1]}\r\n"
            + "\tIF:\tExpr {getvar[seen]==1}\r\n\t\tDO:\tSetState {Loaded}\r\n"
            + "\tIF:\tAlways\r\n\t\tDO:\tSetState {Skipped}\r\n");

        Assert.True(panel.MetaState == "Loaded", string.Join("\n", automation.Messages));
    }
}
