using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class MonsterFactTableTests
{
    private static VtankGameInfoDatabase Fixture() =>
        VtankGameInfoDatabase.Parse(File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory,
            "Fixtures",
            "vtank",
            "gameinfodb-excerpt.ugd")));

    private static PluginCombatTarget Target(
        string name,
        int speciesId,
        string speciesWord) => new(
            0x50000001u,
            name,
            WeenieClassId: 42u,
            Distance: 3f,
            RelativeAngleDegrees: 0f,
            IsHealthKnown: true,
            HealthFraction: 1f)
        {
            SpeciesId = speciesId,
            SpeciesName = speciesWord,
        };

    [Fact]
    public void MaximumHealthComesFromTheDatabaseNotTheLiveObject()
    {
        var settings = new CombatSettings
        {
            MonsterFacts = new MonsterFactTable(Fixture()),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule("maxhp>500", 4));
        settings.Rules.Add(new MonsterRule("DEFAULT", 1));

        Assert.Equal(
            4,
            settings.ResolveRule(Target("Olthoi Slasher", 1, "Olthoi")).Priority);
        Assert.Equal(
            1,
            settings.ResolveRule(Target("Black Rabbit", 25, "Rabbit")).Priority);
        Assert.Equal(
            1,
            settings.ResolveRule(Target("Drudge Prowler", 4, "Drudge")).Priority);
    }

    [Fact]
    public void AnUnlistedMonsterHasNoMaximumHealthAtAll()
    {
        var settings = new CombatSettings
        {
            MonsterFacts = new MonsterFactTable(Fixture()),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule("maxhp<0", 4));
        settings.Rules.Add(new MonsterRule("DEFAULT", 1));

        // An unlisted row answers -1, which the host figure never can.
        Assert.Equal(
            4,
            settings.ResolveRule(Target("Drudge Prowler", 4, "Drudge")).Priority);
    }

    [Fact]
    public void SpeciesComesFromTheDatabaseNotTheLiveProperty()
    {
        var settings = new CombatSettings
        {
            MonsterFacts = new MonsterFactTable(Fixture()),
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule("species==Olthoi", 4));
        settings.Rules.Add(new MonsterRule("DEFAULT", 1));

        // The client's word for species 1 is learned from a live monster.
        Assert.Equal(
            4,
            settings.ResolveRule(Target("Olthoi Slasher", 1, "Olthoi")).Priority);

        // Same live species, but the database does not list this monster, so
        // it has no species at all and the row cannot match it.
        Assert.Equal(
            1,
            settings.ResolveRule(Target("Olthoi Eviscerator", 1, "Olthoi"))
                .Priority);
    }

    [Fact]
    public void WithoutADatabaseNothingHasASpeciesOrAMaximumHealth()
    {
        var settings = new CombatSettings();
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule("species==Olthoi", 4));
        settings.Rules.Add(new MonsterRule("maxhp>=0", 3));
        settings.Rules.Add(new MonsterRule("DEFAULT", 1));

        Assert.Equal(
            1,
            settings.ResolveRule(Target("Olthoi Slasher", 1, "Olthoi")).Priority);
    }

    [Fact]
    public void TheImmunityMaskIsRead()
    {
        var facts = new MonsterFactTable(Fixture());

        Assert.True(facts.IsImmuneToMagic("Olthoi Slasher"));
        Assert.False(facts.IsImmuneToMagic("Black Rabbit"));
        Assert.False(new MonsterFactTable().IsImmuneToMagic("Olthoi Slasher"));
    }
}
