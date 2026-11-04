using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// <see cref="LootSettings.LootOwnDeathCorpse"/>: this character's own death
/// corpse is looted at once, whoever killed the character, and everything in
/// it is taken whatever the loot rules say, rare-only looting or not. The
/// switch is on by default; off, the corpse is a monster's kill like any other.
/// </summary>
public sealed partial class LootingTests
{
    private const uint OwnDeathCorpseId = 0x70000F01u;
    private const uint OwnDeathCorpseItem = 0x70000F02u;

    private static LootSettings RubyOnlySettings(bool lootOwnDeathCorpse)
    {
        var settings = new LootSettings
        {
            Enabled = true,
            LootOwnDeathCorpse = lootOwnDeathCorpse,
        };
        settings.Rules.Add(new LootRule
        {
            Name = "Rubies",
            Expression = "Ruby",
            Action = LootAction.Keep,
        });
        return settings;
    }

    private static Automation DeathCorpseAutomation(string corpseName) => new()
    {
        Corpses =
        [
            new PluginLootContainer(
                OwnDeathCorpseId, 1u, corpseName, 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Drudge Slinker.",
            },
        ],
    };

    /// <summary>
    /// Mutation: drop the own-death check from the corpse pick and the corpse
    /// (a monster's kill, seen for no time at all) is never opened; drop the
    /// keep-everything decision and the pyreal no rule keeps stays behind.
    /// </summary>
    [Fact]
    public void WithTheOptionOnYourOwnDeathCorpseIsOpenedAtOnceAndEmptied()
    {
        Automation automation = DeathCorpseAutomation("Corpse of Tester");
        var controller = new LootController(
            new Host(automation),
            RubyOnlySettings(lootOwnDeathCorpse: true));

        Assert.True(controller.Tick(0.25d, canAct: true));
        Assert.Equal([OwnDeathCorpseId], automation.Opened);
        automation.Current = OwnDeathCorpseId;
        automation.Contents = [Item(OwnDeathCorpseItem, "Pyreal", 273u)];
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));

        // Nothing is appraised: everything is taken, so nothing needs judging.
        Assert.Empty(automation.Identified);
        Assert.Equal([OwnDeathCorpseItem], automation.Picked);
    }

    [Fact]
    public void WithTheOptionOffYourOwnDeathCorpseIsAnotherKillersCorpse()
    {
        Automation automation = DeathCorpseAutomation("Corpse of Tester");
        var controller = new LootController(
            new Host(automation),
            RubyOnlySettings(lootOwnDeathCorpse: false));

        controller.Tick(0.25d, canAct: true);

        Assert.Empty(automation.Opened);
    }

    [Fact]
    public void TheOptionLeavesAnotherPlayersDeathCorpseAlone()
    {
        Automation automation = DeathCorpseAutomation("Corpse of Someone Else");
        var controller = new LootController(
            new Host(automation),
            RubyOnlySettings(lootOwnDeathCorpse: true));

        controller.Tick(0.25d, canAct: true);

        Assert.Empty(automation.Opened);
    }

    [Fact]
    public void TheOptionIsOnByDefault() =>
        Assert.True(new LootSettings().LootOwnDeathCorpse);

    [Fact]
    public void WithRareOnlyLootingOnYourOwnDeathCorpseIsStillOpened()
    {
        Automation automation = DeathCorpseAutomation("Corpse of Tester");
        LootSettings settings = RubyOnlySettings(lootOwnDeathCorpse: true);
        settings.LootOnlyRareCorpses = true;
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));

        Assert.Equal([OwnDeathCorpseId], automation.Opened);
    }

    /// <summary>
    /// Rare-only looting describes nothing until a rare is announced; the
    /// own death corpse is described regardless, and nothing else is.
    /// Mutation: keep the rare-only gate on the description pass and the
    /// undescribed death corpse is never asked about, so never looted.
    /// </summary>
    [Fact]
    public void WithRareOnlyLootingOnlyYourOwnDeathCorpseIsDescribed()
    {
        const uint monsterCorpse = 0x70000F03u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    monsterCorpse, 1u, "Corpse of Drudge", 3f, false, false, false),
                new PluginLootContainer(
                    OwnDeathCorpseId, 1u, "Corpse of Tester", 3f, false, false, false),
            ],
        };
        LootSettings settings = RubyOnlySettings(lootOwnDeathCorpse: true);
        settings.LootOnlyRareCorpses = true;
        var controller = new LootController(new Host(automation), settings);

        // The corpses are listed every couple of seconds while no rare is
        // announced, so the death corpse is asked about within that.
        for (int frame = 0; frame < 5; frame++)
            controller.TickIdentification(0.5d);

        Assert.Equal([OwnDeathCorpseId], automation.Identified);
    }
}
