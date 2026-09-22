using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class DebuffSchedulerTests
{
    [Theory]
    [InlineData("Fester Other VII", 1 << 0, 0)]
    [InlineData("Broadside of a Barn", 1 << 1, 0)]
    [InlineData("Gravity Well", 1 << 2, 0)]
    [InlineData("Incantation of Imperil Other", 1 << 3, 0)]
    [InlineData("Magic Yield Other VI", 1 << 4, 0)]
    [InlineData("Incantation of Fire Vulnerability Other", 1 << 5, 5)]
    [InlineData("Acid Vulnerability Other VI", 1 << 5, 6)]
    [InlineData("Blade Vulnerability Other VI", 1 << 5, 1)]
    [InlineData("Bludgeoning Vulnerability Other VI", 1 << 5, 3)]
    [InlineData("Cold Vulnerability Other VI", 1 << 5, 4)]
    [InlineData("Lightning Vulnerability Other VI", 1 << 5, 7)]
    [InlineData("Piercing Vulnerability Other VI", 1 << 5, 2)]
    [InlineData("Weakening Curse VII", 1 << 9, 0)]
    [InlineData("Festering Curse VII", 1 << 10, 0)]
    [InlineData("Corruption VII", 1 << 11, 0)]
    [InlineData("Destructive Curse VII", 1 << 12, 0)]
    [InlineData("Corrosion VII", 1 << 13, 0)]
    public void ClassifierMapsEveryVtankMonsterDebuffColumn(
        string name,
        int expectedFlag,
        int expectedDamage)
    {
        Assert.True(DebuffSpellCatalog.TryClassify(
            Spell(1, name),
            out DebuffIdentity identity,
            out _));
        Assert.Equal((MonsterActionFlags)expectedFlag, identity.Flag);
        Assert.Equal((MonsterDamageType)expectedDamage, identity.DamageType);
    }

    /// <summary>
    /// No Lure is a creature vulnerability, whatever its element and whatever
    /// its spelling. A Lure enchants an item, so the server refuses every one
    /// aimed at a monster; treating one as a vulnerability spends a cast and
    /// a spell component on a refusal, and leaves the monster undebuffed.
    /// Mutation: accept a Lure as a vulnerability again and every case here
    /// fails.
    /// </summary>
    [Theory]
    [InlineData("Incantation of Lure Blade")]
    [InlineData("Acid Lure VI")]
    [InlineData("Blade Lure III")]
    [InlineData("Bludgeon Lure VI")]
    [InlineData("Flame Lure III")]
    [InlineData("Frost Lure VI")]
    [InlineData("Lightning Lure III")]
    [InlineData("Piercing Lure VI")]
    [InlineData("Incantation of Flame Lure")]
    public void NoLureIsACreatureVulnerability(string name)
    {
        Assert.False(DebuffSpellCatalog.TryClassify(
            Spell(1, name),
            out _,
            out _));
    }


    [Fact]
    public void TrackerWaitsForMatchingSuccessfulServerReceipt()
    {
        var tracker = new DebuffTracker();
        var identity = new DebuffIdentity(
            MonsterActionFlags.Imperil,
            MonsterDamageType.Auto);
        PluginSpellInfo spell = Spell(10, "Imperil Other VII", duration: 60);
        tracker.Begin(99, identity, spell, now: 10, completionRevision: 4);

        Assert.False(tracker.Observe(
            new PluginCastCompletion(5, 11, 99, 0), 12).Completed);
        Assert.True(tracker.HasPending);

        DebuffCompletion failed = tracker.Observe(
            new PluginCastCompletion(6, 10, 99, 0x0402), 13);
        Assert.True(failed.Completed);
        Assert.False(failed.Succeeded);
        Assert.True(tracker.IsDue(99, identity, spell, 13, 5));

        tracker.Begin(99, identity, spell, 14, completionRevision: 6);
        DebuffCompletion succeeded = tracker.Observe(
            new PluginCastCompletion(7, 10, 99, 0), 15);
        Assert.True(succeeded.Succeeded);
        Assert.False(tracker.IsDue(99, identity, spell, 69, 5));
        Assert.True(tracker.IsDue(99, identity, spell, 70, 5));
    }

    [Fact]
    public void ZeroToleranceStepsGetNoPrecastWindow()
    {
        var tracker = new DebuffTracker();
        var identity = new DebuffIdentity(
            MonsterActionFlags.Corrosion,
            MonsterDamageType.Auto);
        PluginSpellInfo spell = Spell(
            10,
            "Corrosion VII",
            duration: 60) with { IsDamageOverTime = true };
        tracker.Begin(99, identity, spell, 0, 0);
        tracker.Observe(new PluginCastCompletion(1, 10, 99, 0), 1);

        Assert.False(tracker.IsDue(99, identity, spell, 60, 0d));
        Assert.True(tracker.IsDue(99, identity, spell, 61, 0d));

        // ...and with the ordinary precast window the same timer IS due
        // early, which is exactly what the nine non-zero-tolerance steps get.
        Assert.True(tracker.IsDue(99, identity, spell, 45, 20));
    }

    [Fact]
    public void FakeImperilCreatesVtankThreeThousandSecondLocalMarker()
    {
        var tracker = new DebuffTracker();
        var identity = new DebuffIdentity(
            MonsterActionFlags.Imperil,
            MonsterDamageType.Auto);
        PluginSpellInfo learned = Spell(
            10,
            "Imperil Other VII",
            tier: 7,
            duration: 60);

        tracker.RecordFakeImperil(99u, now: 10d);

        Assert.False(tracker.IsDue(99u, identity, learned, 3009d, 0d));
        Assert.True(tracker.IsDue(99u, identity, learned, 3010d, 0d));
    }

    /// <summary>
    /// A cast another client reported is stamped onto THIS tracker's clock:
    /// now plus what is left of it, never the other client's wall time. From
    /// then on it reads like a cast this client saw land itself, so the
    /// chain does not re-cast it and the element choice sees it.
    /// Mutation: stamp the remote record with its seconds remaining alone
    /// (no <c>now</c>) and the first two assertions fail.
    /// </summary>
    [Fact]
    public void ARemoteCastIsAppliedForItsRemainingSecondsOnTheLocalClock()
    {
        var tracker = new DebuffTracker();
        var identity = new DebuffIdentity(
            MonsterActionFlags.Vulnerability,
            MonsterDamageType.Fire);
        PluginSpellInfo spell = Spell(10, "Fire Vulnerability Other VII", tier: 7);

        Assert.True(tracker.RecordRemote(
            99u, identity, spell, now: 100d, secondsRemaining: 30d));

        Assert.True(tracker.IsApplied(99u, identity, spell, 129.9d));
        Assert.False(tracker.IsApplied(99u, identity, spell, 130d));
        Assert.False(tracker.IsDue(99u, identity, spell, 129.9d, 0d));
        Assert.True(tracker.IsDue(99u, identity, spell, 130d, 0d));
    }

    /// <summary>
    /// A report with nothing left of the effect is not a record: the host
    /// has already aged it to zero, so writing it would only overwrite a
    /// live local record with a lapsed one.
    /// </summary>
    [Theory]
    [InlineData(0d)]
    [InlineData(-5d)]
    [InlineData(double.NaN)]
    public void ARemoteCastWithNothingLeftIsNotRecorded(double secondsRemaining)
    {
        var tracker = new DebuffTracker();
        var identity = new DebuffIdentity(
            MonsterActionFlags.Imperil,
            MonsterDamageType.Auto);
        PluginSpellInfo spell = Spell(10, "Imperil Other VII", tier: 7);

        Assert.False(tracker.RecordRemote(99u, identity, spell, 10d, secondsRemaining));

        Assert.False(tracker.IsApplied(99u, identity, spell, 10d));
    }

    /// <summary>
    /// A remote report merges into what is already known rather than
    /// replacing it: the tier can only rise and the expiry can only move
    /// later. A weaker or shorter report from another client never shortens
    /// or weakens what this client saw land itself, and a stronger one is
    /// taken up without giving away the longer tail already known.
    /// Mutation: overwrite instead of merge and every "nothing changes" row
    /// fails; drop the tier comparison and the stronger-remote rows fail.
    /// </summary>
    [Theory]
    // Weaker and sooner: nothing changes.
    [InlineData(7, 60d, 5, 20d, 7, 60d, false)]
    // Weaker but longer: the tail is kept, the tier is not lowered.
    [InlineData(7, 60d, 5, 100d, 7, 110d, true)]
    // Stronger but sooner: the tier rises, the later expiry stays.
    [InlineData(5, 60d, 7, 20d, 7, 60d, true)]
    // Stronger and longer: both move.
    [InlineData(5, 60d, 7, 100d, 7, 110d, true)]
    // Same tier, sooner: nothing changes.
    [InlineData(7, 60d, 7, 20d, 7, 60d, false)]
    // Same tier, longer: the expiry moves out.
    [InlineData(7, 60d, 7, 100d, 7, 110d, true)]
    public void ARemoteRecordMergesByHigherTierAndLaterExpiry(
        int localTier,
        double localExpiresAt,
        int remoteTier,
        double remoteSecondsRemaining,
        int expectedTier,
        double expectedExpiresAt,
        bool expectedChanged)
    {
        var tracker = new DebuffTracker();
        var identity = new DebuffIdentity(
            MonsterActionFlags.Imperil,
            MonsterDamageType.Auto);
        PluginSpellInfo local = Spell(
            10, "Imperil Other (local)", tier: localTier, duration: (float)localExpiresAt);
        tracker.RecordApplied(99u, identity, local, now: 0d);
        PluginSpellInfo remote = Spell(20, "Imperil Other (remote)", tier: remoteTier);

        Assert.Equal(
            expectedChanged,
            tracker.RecordRemote(99u, identity, remote, now: 10d, remoteSecondsRemaining));

        // A spell of the expected tier under another id reads the record's
        // tier and expiry; one tier above it must not be satisfied.
        PluginSpellInfo probe = Spell(30, "Imperil Other (probe)", tier: expectedTier);
        PluginSpellInfo stronger = Spell(31, "Imperil Other (stronger)", tier: expectedTier + 1);
        Assert.True(tracker.IsApplied(99u, identity, probe, expectedExpiresAt - 0.5d));
        Assert.False(tracker.IsApplied(99u, identity, probe, expectedExpiresAt));
        Assert.False(tracker.IsApplied(99u, identity, stronger, expectedExpiresAt - 0.5d));
        Assert.False(tracker.IsDue(99u, identity, probe, expectedExpiresAt - 0.5d, 0d));
    }

    private static PluginSpellInfo Spell(
        uint id,
        string name,
        int tier = 8,
        uint school = 33,
        float duration = 30) => new(
            id,
            name,
            Family: id,
            Tier: tier,
            Difficulty: 300,
            ManaCost: 20,
            DurationSeconds: duration,
            School: school,
            Description: string.Empty,
            IsSelfTargeted: false,
            IsBeneficial: false)
        {
            IsDebuff = true,
            IsOffensive = true,
        };

    private sealed class Character(
        IReadOnlyList<PluginSkillInfo>? skills = null) : ICharacterInfo
    {
        public bool IsInWorld => true;
        public uint ObjectId => 1;
        public uint CurrentHealth => 100;
        public uint MaxHealth => 100;
        public uint CurrentStamina => 100;
        public uint MaxStamina => 100;
        public uint CurrentMana => 100;
        public uint MaxMana => 100;
        public IReadOnlyList<PluginSkillInfo> Skills { get; } = skills ?? [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in Skills)
            {
                if (candidate.SkillId == skillId)
                {
                    skill = candidate;
                    return true;
                }
            }
            skill = default;
            return false;
        }
    }
}
