using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The rare this character just found is never walked away from. The corpse
/// the server says generated this character's rare is walked to up to the
/// hundred metres MossTank ever walks to a corpse, even when the profile's
/// corpse approach range is shorter, and the route waits while it is.
/// This is MossTank's own rule, beside its rare announcement handling, and
/// it sits behind an option that is off unless turned on; the reference
/// drops every corpse beyond the approach range, and so does MossTank with
/// the option off.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const uint RareWalkOrdinaryCorpse = 0x70000E01u;
    private const uint RareWalkRareCorpse = 0x70000E02u;

    /// <summary>
    /// The live case with the option off, its default: the rare just past
    /// the approach range is left as the reference leaves it, and the route
    /// walks on.
    /// Mutation: walk to the rare whatever the option says and the corpse
    /// walk wins the pass.
    /// </summary>
    [Fact]
    public void WithTheOptionOffAnAnnouncedRarePastTheRangeIsLeftAndTheRouteWalksOn()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: true, navPriorityBoost: false, walkToOwnRares: false);
        Assert.False(panel.WalkToOwnRareCorpsesEnabled);
        panel.OnTick(0.3d);

        loot.Corpses = [RareWalkCorpse(RareWalkRareCorpse, 15.3f, description: null)];
        panel.OnTick(0.3d);
        automation.PostChat($"{automation.Name} has discovered the Warrior's Crystal!");
        panel.OnTick(0.3d);
        loot.Corpses =
        [
            RareWalkCorpse(
                RareWalkRareCorpse,
                15.3f,
                $"Killed by {automation.Name}. This corpse generated a rare item!"),
        ];
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateCorpsePriority", StringComparison.Ordinal));
    }

    /// <summary>
    /// With the option off, a route set to outrank looting outranks this
    /// character's rare too, even inside the approach range: nothing holds
    /// the route for it.
    /// Mutation: hold the route for the rare whatever the option says and
    /// the priority route loses the pass.
    /// </summary>
    [Fact]
    public void WithTheOptionOffARouteThatOutranksLootingDoesNotWaitForTheRare()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: false, navPriorityBoost: true, walkToOwnRares: false);
        panel.OnTick(0.3d);
        loot.Corpses =
        [
            RareWalkCorpse(
                RareWalkRareCorpse,
                10f,
                $"Killed by {automation.Name}. This corpse generated a rare item!"),
        ];
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateRoutePriority", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Holding the route", StringComparison.Ordinal));
    }

    /// <summary>
    /// With the option off, an undescribed corpse that appeared with this
    /// character's rare announcement holds the walks only inside the usual
    /// reach (the approach range plus ten metres), as the reference has it;
    /// forty metres back it does not.
    /// Mutation: widen the corpse-id hold whatever the option says and the
    /// route is held for the corpse forty metres back.
    /// </summary>
    [Fact]
    public void WithTheOptionOffAnUndescribedCorpseFarBackDoesNotHoldTheRoute()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: true, navPriorityBoost: false, walkToOwnRares: false);
        panel.OnTick(0.3d);

        loot.Corpses = [RareWalkCorpse(RareWalkRareCorpse, 40f, description: null)];
        panel.OnTick(0.3d);
        automation.PostChat($"{automation.Name} has discovered the Warrior's Crystal!");
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.DoesNotContain(
            "[MossTank] Holding this tick until the corpse id arrives.",
            automation.Messages);
        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));
    }

    /// <summary>
    /// With the option off and rare-only looting off, nothing uses this
    /// character's rare announcement, so it is not listened for, as before
    /// the option existed.
    /// Mutation: listen for the announcement whatever the settings say and
    /// the loot log reports it.
    /// </summary>
    [Fact]
    public void WithTheOptionAndRareOnlyLootingOffTheAnnouncementIsNotListenedFor()
    {
        (MossTankPanel panel, FakeAutomation automation, _) =
            RareWalkPanel(
                lootPriorityBoost: true,
                navPriorityBoost: false,
                rareOnly: false,
                walkToOwnRares: false);
        automation.PostChat($"{automation.Name} has discovered the Warrior's Crystal!");
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("rare announced", StringComparison.Ordinal));
    }

    /// <summary>
    /// The option is off in a new profile, and turning it on is written to
    /// the profile's MossTank side file, where a restart finds it.
    /// Mutation: leave the option out of the side file and the restarted
    /// panel has it off again.
    /// </summary>
    [Fact]
    public void TheOwnRareWalkOptionIsOffByDefaultAndSurvivesARestart()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Prover" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Assert.False(panel.WalkToOwnRareCorpsesEnabled);

        panel.ToggleWalkToOwnRareCorpses();

        Assert.True(panel.WalkToOwnRareCorpsesEnabled);
        Assert.Contains(
            storage.Text,
            pair => pair.Key.StartsWith("profiles/macro/sidecar/", StringComparison.Ordinal)
                && pair.Value.Contains(
                    "\"InventoryLootWalkToOwnRareCorpses\": true",
                    StringComparison.Ordinal));
        var restarted = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));
        Assert.True(restarted.WalkToOwnRareCorpsesEnabled);
    }

    /// <summary>
    /// The live loss, step for step: two kills beside a waypoint, the meta
    /// turns combat off and the route walks on; the corpses land behind the
    /// character, the server announces this character's rare, the two are
    /// described — an ordinary kill at 10.4 metres and the rare's at 15.3,
    /// just past the profile's fifteen-metre approach range. The rare's
    /// corpse must be walked to and opened; the route must not win a pass
    /// in between.
    /// Mutation: take the rare reach out of the approach pick and the walk
    /// declines with "No corpse to walk to." while the route carries the
    /// character off.
    /// </summary>
    [Fact]
    public void AnAnnouncedRareJustPastTheApproachRangeIsWalkedToAndOpened()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: true, navPriorityBoost: false);

        // The fight is over and the route walks on.
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);
        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));

        // The two corpses land behind the character, and half a second later
        // the server announces the rare.
        loot.Corpses =
        [
            RareWalkCorpse(RareWalkOrdinaryCorpse, 10.4f, description: null),
            RareWalkCorpse(RareWalkRareCorpse, 15.3f, description: null),
        ];
        panel.OnTick(0.3d);
        automation.PostChat($"{automation.Name} has discovered the Warrior's Crystal!");
        panel.OnTick(0.3d);
        Assert.Contains(
            automation.Messages,
            line => line.Contains("rare announced (Warrior's Crystal)", StringComparison.Ordinal));

        // Both descriptions arrive.
        loot.Corpses =
        [
            RareWalkCorpse(RareWalkOrdinaryCorpse, 10.4f, $"Killed by {automation.Name}."),
            RareWalkCorpse(
                RareWalkRareCorpse,
                15.3f,
                $"Killed by {automation.Name}. This corpse generated a rare item!"),
        ];
        automation.Messages.Clear();
        automation.MovementIntents.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateCorpsePriority", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));
        Assert.Contains(automation.MovementIntents, intent => intent.Forward);

        // Arrived: the rare's corpse is in arm's reach, and it is opened.
        loot.Corpses =
        [
            RareWalkCorpse(RareWalkOrdinaryCorpse, 12f, $"Killed by {automation.Name}."),
            RareWalkCorpse(
                RareWalkRareCorpse,
                2f,
                $"Killed by {automation.Name}. This corpse generated a rare item!"),
        ];
        for (int tick = 0; tick < 12 && loot.Opened == 0u; tick++)
            panel.OnTick(0.3d);

        Assert.Equal(RareWalkRareCorpse, loot.Opened);
    }

    /// <summary>
    /// A corpse that appeared with this character's rare announcement, still
    /// waiting for its description, holds every walk off out to the rare
    /// reach, not only out to the approach range plus ten: it may be the
    /// rare's, and the route must not carry the character off before the
    /// answer arrives.
    /// Mutation: leave the post-announcement corpses out of the corpse-id
    /// hold and the route wins the pass while the rare's corpse is still
    /// undescribed forty metres back.
    /// </summary>
    [Fact]
    public void AnUndescribedCorpseFromTheAnnouncementHoldsTheRouteOutToTheRareReach()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: true, navPriorityBoost: false);
        panel.OnTick(0.3d);

        loot.Corpses = [RareWalkCorpse(RareWalkRareCorpse, 40f, description: null)];
        panel.OnTick(0.3d);
        automation.PostChat($"{automation.Name} has discovered the Warrior's Crystal!");
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            "[MossTank] Holding this tick until the corpse id arrives.",
            automation.Messages);
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));

        loot.Corpses =
        [
            RareWalkCorpse(
                RareWalkRareCorpse,
                40f,
                $"Killed by {automation.Name}. This corpse generated a rare item!"),
        ];
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateCorpsePriority", StringComparison.Ordinal));
    }

    /// <summary>
    /// With the route set to outrank looting, the idle corpse walk sits below
    /// the route. A rare of this character's own is the exception: while its
    /// corpse is in the rare reach and unopened, the route stands down, so
    /// the corpse walk gets its turn.
    /// Mutation: drop the rare hold from the route rules and the priority
    /// route wins every pass.
    /// </summary>
    [Fact]
    public void ARouteThatOutranksLootingStillWaitsForThisCharactersRare()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: false, navPriorityBoost: true);
        panel.OnTick(0.3d);

        loot.Corpses = [RareWalkCorpse(RareWalkRareCorpse, 30f, description: null)];
        panel.OnTick(0.3d);
        automation.PostChat($"{automation.Name} has discovered the Warrior's Crystal!");
        panel.OnTick(0.3d);
        loot.Corpses =
        [
            RareWalkCorpse(
                RareWalkRareCorpse,
                30f,
                $"Killed by {automation.Name}. This corpse generated a rare item!"),
        ];
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateRoutePriority", StringComparison.Ordinal));
        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateCorpseIdle", StringComparison.Ordinal));
    }

    /// <summary>
    /// The exception is this character's rare only. Somebody else's
    /// announcement changes nothing, and an ordinary kill of this
    /// character's own just past the approach range is left, as the
    /// reference leaves it: the route walks on.
    /// Mutation: widen the reach for every corpse rather than for this
    /// character's rare and the ordinary kill is walked to.
    /// </summary>
    [Fact]
    public void AnOrdinaryKillPastTheApproachRangeIsStillLeftBehind()
    {
        (MossTankPanel panel, FakeAutomation automation, FrameLootSurface loot) =
            RareWalkPanel(lootPriorityBoost: true, navPriorityBoost: false, rareOnly: false);
        loot.Corpses =
        [
            RareWalkCorpse(RareWalkOrdinaryCorpse, 20f, $"Killed by {automation.Name}."),
        ];
        automation.PostChat("Someone Else has discovered the Warrior's Crystal!");
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateCorpsePriority", StringComparison.Ordinal));
    }

    /// <summary>
    /// The meta's looting setup on Coldeve: rare-only looting, a fifteen
    /// metre corpse approach range, looting outranking the route (or the
    /// route outranking looting), and one route point sixty-six metres
    /// south for the route to walk to.
    /// </summary>
    private static (MossTankPanel Panel, FakeAutomation Automation, FrameLootSurface Loot)
        RareWalkPanel(
            bool lootPriorityBoost,
            bool navPriorityBoost,
            bool rareOnly = true,
            bool walkToOwnRares = true)
    {
        var loot = new FrameLootSurface();
        var automation = new FakeAutomation
        {
            LootSurface = loot,
            NavigationSnapshot = RareWalkSnapshot(0d),
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.AddLootRule();
        if (!panel.LootEnabled)
            panel.ToggleLooting();
        if (panel.WalkToOwnRareCorpsesEnabled != walkToOwnRares)
            panel.ToggleWalkToOwnRareCorpses();
        RareWalkOption(panel, $"LootOnlyRareCorpses {(rareOnly ? "true" : "false")}");
        RareWalkOption(panel, "CorpseApproachRange-Max 0.0625");
        RareWalkOption(panel, $"LootPriorityBoost {(lootPriorityBoost ? "true" : "false")}");
        RareWalkOption(panel, $"NavPriorityBoost {(navPriorityBoost ? "true" : "false")}");
        automation.NavigationSnapshot = RareWalkSnapshot(-66d / 240d);
        panel.AddRoutePoint();
        automation.NavigationSnapshot = RareWalkSnapshot(0d);
        panel.ToggleNavigation();
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log Loot on", "/vt log Loot on"));
        automation.Messages.Clear();
        panel.ToggleCombat();
        return (panel, automation, loot);
    }

    private static void RareWalkOption(MossTankPanel panel, string setting) =>
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "opt set " + setting, "/vt opt set " + setting));

    private static PluginNavigationSnapshot RareWalkSnapshot(double northSouth) =>
        new(
            true,
            false,
            1u,
            new PluginNavigationPosition(
                0x00010001u, 0d, northSouth, 0d, 0f, IsOutdoor: true),
            false,
            false);

    /// <summary>A corpse due north of the character, behind the route.</summary>
    private static PluginLootContainer RareWalkCorpse(
        uint objectId,
        float distance,
        string? description) =>
        new(objectId, 1u, "Corpse of Drudge Bloodletter", distance, false, false, false)
        {
            IsIdentified = description is not null,
            LongDescription = description ?? string.Empty,
            HasPosition = true,
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, distance / 240d, 0d, 0f, IsOutdoor: true),
        };
}
