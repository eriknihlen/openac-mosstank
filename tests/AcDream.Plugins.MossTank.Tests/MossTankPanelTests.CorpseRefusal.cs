using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The server refuses a corpse someone else has open with a status-line
/// notice, which the client hands the plugin as its own event and never as a
/// chat line. The corpse is then left for ten seconds instead of being asked
/// for again on every pass, as VTank leaves it.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const uint CorpseRefusalCorpse = 0x70000F01u;

    /// <summary>
    /// Mutation: leave the panel unsubscribed from the status notices and the
    /// corpse is asked for again on every pass.
    /// </summary>
    [Fact]
    public void ACorpseTheServerSaysIsInUseIsLeftForTenSeconds()
    {
        var loot = new FrameLootSurface();
        var automation = new FakeAutomation
        {
            LootSurface = loot,
            NavigationSnapshot = RareWalkSnapshot(0d),
        };
        var host = new FakeHost(automation);
        var events = (FakeEvents)host.Events;
        var panel = new MossTankPanel(host);
        panel.AddLootRule();
        if (!panel.LootEnabled)
            panel.ToggleLooting();
        RareWalkOption(panel, "LootOnlyRareCorpses false");
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log Loot on", "/vt log Loot on"));
        loot.Corpses =
        [
            RareWalkCorpse(CorpseRefusalCorpse, 3f, $"Killed by {automation.Name}."),
        ];
        panel.ToggleCombat();
        for (int i = 0; i < 10 && loot.OpenCount == 0; i++)
            panel.OnTick(0.3d);
        Assert.Equal(1, loot.OpenCount);

        events.RaiseStatusText(
            "The Corpse of Drudge Bloodletter is already in use by Unkle Leo!");
        for (int i = 0; i < 30; i++)
            panel.OnTick(0.3d);

        Assert.True(loot.OpenCount == 1, string.Join("\n", automation.Messages));
        Assert.Contains(
            automation.Messages,
            line => line.Contains("refused, skipping it for 10 seconds", StringComparison.Ordinal));
    }

    /// <summary>
    /// Mutation: keep the panel subscribed after it is disposed and a notice
    /// still reaches a looter that is gone.
    /// </summary>
    [Fact]
    public void DisposingThePanelStopsListeningForStatusNotices()
    {
        var host = new FakeHost(new FakeAutomation());
        var events = (FakeEvents)host.Events;
        var panel = new MossTankPanel(host);
        Assert.Equal(1, events.StatusTextListenerCount);

        panel.Dispose();

        Assert.Equal(0, events.StatusTextListenerCount);
    }
}
