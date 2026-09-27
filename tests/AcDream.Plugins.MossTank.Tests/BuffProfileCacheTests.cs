using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

public class BuffProfileCacheTests
{
    private static PluginSpellInfo Spell(uint id, uint family = 1) =>
        new(id, $"Spell {id}", family, 1, 50, 10, 1800, 33,
            "Reduces damage the caster takes from Fire by 9%.", true, true);

    [Fact]
    public void ReusesEquivalentContentsWithoutAllocatingOnRepeatedPasses()
    {
        var cache = new BuffProfileCache();
        PluginSpellInfo[] spells = [Spell(1), Spell(2, 2)];
        var first = cache.Capture(spells);
        Assert.Same(first, cache.Capture(spells.ToArray()));
        for (int index = 0; index < 100; index++)
            cache.Capture(spells);
        long before = GC.GetAllocatedBytesForCurrentThread();
        for (int index = 0; index < 1000; index++)
            cache.Capture(spells);
        long allocated = GC.GetAllocatedBytesForCurrentThread() - before;
        Assert.True(allocated < 1024, $"Unchanged grouping allocated {allocated} bytes.");
    }

    [Fact]
    public void RebuildsAfterInPlaceReplacementAdditionRemovalAndReordering()
    {
        var cache = new BuffProfileCache();
        var spells = new List<PluginSpellInfo> { Spell(1), Spell(2) with { Tier = 2 } };
        var original = cache.Capture(spells);
        spells[0] = spells[0] with
        {
            Family = 3, Tier = 4, QualityOverride = 100,
            Description = "Increases the caster's Strength by 10 points."
        };
        Check();
        Assert.Equal(1u, original[0].Family);
        Assert.Equal(BuffTargetKind.Protection, original[0].Kind);
        spells.Add(Spell(3, 4));
        Check();
        spells.Reverse();
        Check();
        spells.RemoveAt(1);
        Check();
        spells.Clear();
        Check();
        spells.Add(Spell(4));
        Check();

        void Check()
        {
            var expected = BuffProfile.Build(spells);
            var actual = cache.Capture(spells);
            Assert.Equal(expected.Count, actual.Count);
            for (int index = 0; index < expected.Count; index++)
            {
                Assert.Equal(expected[index].Family, actual[index].Family);
                Assert.Equal(expected[index].Kind, actual[index].Kind);
                Assert.Equal(expected[index].TargetName, actual[index].TargetName);
                Assert.Equal(expected[index].Reference, actual[index].Reference);
                Assert.Equal(expected[index].Tiers, actual[index].Tiers);
            }
        }
    }

    [Fact]
    public void CachedGroupingStillUsesCurrentBuffSettings()
    {
        var cache = new BuffProfileCache();
        PluginSpellInfo[] spells = [Spell(1)];
        var settings = new BuffSettings();
        var lines = cache.Capture(spells);
        Assert.Single(BuffPlan.Build(lines, [], [], [], settings));
        settings.BuffProtections = false;
        Assert.Same(lines, cache.Capture(spells));
        Assert.Empty(BuffPlan.Build(lines, [], [], [], settings));
        settings.BuffProtections = true;
        Assert.Single(BuffPlan.Build(lines, [], [], [], settings));
    }

    [Fact]
    public void ReusedLinesStillObserveSkillsEnchantmentsAndFormulaEdits()
    {
        var cache = new BuffProfileCache();
        var components = new List<uint> { 1 };
        PluginSpellInfo[] spells = [Spell(1) with { FormulaComponentIds = components }];
        var lines = cache.Capture(spells);
        var settings = new BuffSettings();
        var castability = new ComponentProbe();
        PluginSkillInfo[] lowSkill = [new(33, "Life Magic", PluginSkillTraining.Trained, 50)];
        PluginSkillInfo[] highSkill = [new(33, "Life Magic", PluginSkillTraining.Trained, 100)];
        Assert.Empty(BuffPlan.Build(lines, lowSkill, [], [], settings, castability: castability));
        Assert.Single(BuffPlan.Build(lines, highSkill, [], [], settings, castability: castability));
        Assert.Empty(BuffPlan.Build(lines, highSkill, [], [new(1, 1, 1, 1800)], settings, castability: castability));
        Assert.Single(BuffPlan.Build(lines, highSkill, [], [new(1, 1, 1, 299)], settings, castability: castability));

        components[0] = 2;
        Assert.Same(lines, cache.Capture(spells));
        Assert.Empty(BuffPlan.Build(lines, highSkill, [], [], settings, castability: castability));
        spells[0] = spells[0] with { FormulaComponentIds = new uint[] { 1 } };
        var replaced = cache.Capture(spells);
        Assert.NotSame(lines, replaced);
        Assert.Single(BuffPlan.Build(replaced, highSkill, [], [], settings, castability: castability));
    }

    private sealed class ComponentProbe : IBuffCastability
    {
        public bool IsCastable(PluginSpellInfo spell) => spell.FormulaComponentIds[0] == 1;
        public void NoteNoCastableTier(BuffLine line) { }
    }
}
