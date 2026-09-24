namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankLootProfileSerializerTests
{
    [Fact]
    public void ReadsAndWritesExactUtlOneRecordsAndUnknownBlocks()
    {
        const string source = "UTL\r\n1\r\n1\r\n"
            + "Epic loot\r\nLegacy editor text\r\n42;10;1;3;9999\r\n7\r\n"
            + "12\r\n^Epic.*\r\n1\r\n11\r\n25000\r\n19\r\n"
            + "7\r\nfalse\r\n"
            + "SalvageCombine\r\n36\r\n1\r\n1-6, 7-8, 9, 10\r\n1\r\n61\r\n1-10\r\n0\r\n"
            + "FutureBlock\r\n7\r\nhello\r\n";

        Assert.True(VtankLootProfileSerializer.TryRead(
            source,
            out VtankLootProfile profile,
            out string error), error);
        LootRule rule = Assert.Single(profile.Rules);
        Assert.Equal("Epic loot", rule.Name);
        Assert.Equal("Legacy editor text", rule.CustomExpression);
        Assert.Equal(LootAction.KeepUpTo, rule.Action);
        Assert.Equal(7, rule.KeepCount);
        Assert.Equal(42, rule.Priority);
        Assert.Equal([1, 3, 9999],
            rule.VtankRequirements.Select(static requirement => requirement.Type));
        Assert.Equal("1-10", profile.SalvageCombine.MaterialCombineStrings[61]);
        Assert.Equal("hello\r\n", Assert.Single(profile.UnknownBlocks).Payload);

        string canonical = VtankLootProfileSerializer.Write(profile);
        Assert.True(VtankLootProfileSerializer.TryRead(
            canonical,
            out VtankLootProfile second,
            out error), error);
        Assert.Equal(
            canonical,
            VtankLootProfileSerializer.Write(second));
    }

    [Fact]
    public void RoundTripsEveryVtankRequirementTypeWithoutLosingPayload()
    {
        int[] types =
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13,
            14, 15, 16, 17,
            1000, 1001, 1002, 1003, 1004,
            2000, 2001, 2003, 2005, 2006, 2007, 2008,
            9999,
        ];
        var profile = new VtankLootProfile();
        LootRule rule = new()
        {
            Name = "All vocabulary",
            Action = LootAction.Keep,
            Priority = -19,
        };
        foreach (int type in types)
        {
            rule.VtankRequirements.Add(new VtankLootRequirement
            {
                Type = type,
                Payload = ValidPayload(type),
            });
        }
        profile.Rules.Add(rule);

        string source = VtankLootProfileSerializer.Write(profile);
        Assert.True(VtankLootProfileSerializer.TryRead(
            source,
            out VtankLootProfile loaded,
            out string error), error);
        LootRule roundTrip = Assert.Single(loaded.Rules);
        Assert.Equal(types,
            roundTrip.VtankRequirements.Select(static requirement => requirement.Type));
        Assert.Equal(
            rule.VtankRequirements.Select(static requirement => requirement.Payload),
            roundTrip.VtankRequirements.Select(static requirement => requirement.Payload));
        Assert.Equal(source, VtankLootProfileSerializer.Write(loaded));
    }

    [Fact]
    public void KnownRecordsUseTheirTypedLineShapeAndKnownBlocksIgnoreLengthMetadata()
    {
        const string source = "UTL\n1\n2\n"
            + "First\neditor expression\n0;1;1\n999\nSword\n1\n"
            + "Second\n\n0;1;7\n1\n2\n"
            + "SalvageCombine\n1\n1\n1-6, 7-8, 9, 10\n1\n61\n1-10\n0\n"
            + "FutureBlock\n6\nhello\n";

        Assert.True(VtankLootProfileSerializer.TryRead(
            source,
            out VtankLootProfile profile,
            out string error), error);

        Assert.Equal(2, profile.Rules.Count);
        Assert.Equal("Sword\r\n1\r\n", profile.Rules[0].VtankRequirements[0].Payload);
        Assert.Equal("2\r\n", profile.Rules[1].VtankRequirements[0].Payload);
        Assert.Equal("1-10", profile.SalvageCombine.MaterialCombineStrings[61]);
        Assert.Equal("hello\n", Assert.Single(profile.UnknownBlocks).Payload);
    }

    [Fact]
    public void UnknownRequirementStillUsesAndValidatesItsDeclaredLength()
    {
        const string valid = "UTL\r\n1\r\n1\r\nUnknown\r\n\r\n0;1;4242\r\n7\r\nhello\r\n";
        Assert.True(VtankLootProfileSerializer.TryRead(
            valid,
            out VtankLootProfile profile,
            out string error), error);
        Assert.Equal("hello\r\n", Assert.Single(profile.Rules[0].VtankRequirements).Payload);

        const string truncated = "UTL\r\n1\r\n1\r\nUnknown\r\n\r\n0;1;4242\r\n99\r\nhello\r\n";
        Assert.False(VtankLootProfileSerializer.TryRead(
            truncated,
            out _,
            out error));
        Assert.Contains("truncated", error, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// Mutation <c>ExportRequirementsWithoutImportedProvenance</c>: omit the
    /// imported-requirements branch in <c>ExportRequirements</c>; the saved
    /// header gains requirement 9999 and the reloaded rule stops matching.
    /// </summary>
    [Fact]
    public void ImportedEmptyRequirementSetRoundTripsAsAnUnconditionalKeep()
    {
        const string source = "UTL\r\n1\r\n1\r\nAlways keep\r\n\r\n0;1\r\n";

        Assert.True(VtankLootProfileSerializer.TryRead(
            source,
            out VtankLootProfile profile,
            out string error), error);
        LootRule imported = Assert.Single(profile.Rules);
        Assert.Empty(imported.VtankRequirements);
        Assert.True(imported.HasImportedRequirements);

        string written = VtankLootProfileSerializer.Write(profile);
        Assert.DoesNotContain(";9999\r\n", written, StringComparison.Ordinal);
        Assert.True(VtankLootProfileSerializer.TryRead(
            written,
            out VtankLootProfile reloaded,
            out error), error);
        LootRule roundTrip = Assert.Single(reloaded.Rules);
        Assert.Empty(roundTrip.VtankRequirements);
        Assert.True(roundTrip.HasImportedRequirements);

        LootDecision? decision = LootRuleEngine.Decide(default, default, [roundTrip], []);
        Assert.Equal(LootAction.Keep, decision?.Action);
    }
    [Fact]
    public void NativeExpressionsExportDisabledInsteadOfAccidentalMatchAll()
    {
        var profile = new VtankLootProfile
        {
            Rules =
            [
                new LootRule
                {
                    Name = "Native only",
                    Expression = "name ~= Sword && value > 100",
                    Action = LootAction.Keep,
                },
            ],
        };

        string source = VtankLootProfileSerializer.Write(profile);
        Assert.Contains("0;1;9999\r\n", source, StringComparison.Ordinal);
        Assert.Contains("6\r\ntrue\r\n", source, StringComparison.Ordinal);
    }

    [Fact]
    public void ReadsLegacyVersionZeroKnownRequirements()
    {
        const string source = "1\r\nLegacy\r\n7;1;1;8\r\n^Sword$\r\n1\r\n3\r\n";

        Assert.True(VtankLootProfileSerializer.TryRead(
            source,
            out VtankLootProfile profile,
            out string error), error);
        LootRule rule = Assert.Single(profile.Rules);
        Assert.Equal(0, profile.SourceVersion);
        Assert.Equal(2, rule.VtankRequirements.Count);
        Assert.Equal("^Sword$\r\n1\r\n", rule.VtankRequirements[0].Payload);
        Assert.Equal("3\r\n", rule.VtankRequirements[1].Payload);
    }

    /// <summary>
    /// Each rule's expression rides in a block whose length prefixes count
    /// the characters as written. An expression written with bare LF or CR
    /// line breaks must be measured after its line endings are normalized,
    /// or the reader stops mid-text and loses every later rule's expression.
    /// </summary>
    [Fact]
    public void RuleExpressionsWithCrlfLfAndCrLineBreaksRoundTripThroughTheFile()
    {
        LootRule[] rules =
        [
            new() { Name = "LF", Expression = "first\nsecond" },
            new() { Name = "CRLF and CR", Expression = "one\r\ntwo\rthree" },
            new() { Name = "Single line", Expression = "ObjectClass = 5" },
        ];

        string text = MossTankLootProfileStore.SerializeRules(rules);
        Assert.True(VtankLootProfileSerializer.TryRead(text, out _, out string error), error);

        var loaded = new List<LootRule>();
        Assert.True(MossTankLootProfileStore.TryParseRules(text, loaded));
        Assert.Equal(
            ["first\r\nsecond", "one\r\ntwo\r\nthree", "ObjectClass = 5"],
            loaded.Select(static rule => rule.Expression));

        // The file read back writes out unchanged.
        Assert.Equal(text, MossTankLootProfileStore.SerializeRules(loaded));
    }

    private static string ValidPayload(int type)
    {
        int lines = type switch
        {
            0 => 1,
            1 => 2,
            2 or 3 or 4 or 5 or 11 or 12 or 13 or 17 or 1000 or 2003 or 2005 => 2,
            6 or 7 or 8 or 10 or 1001 or 1002 or 1003 or 2000 or 2001
                or 2006 or 2007 or 9999 => 1,
            9 or 1004 or 2008 => 3,
            14 => 5,
            15 or 16 => 6,
            _ => throw new ArgumentOutOfRangeException(nameof(type)),
        };
        return string.Concat(Enumerable.Range(1, lines).Select(index => $"{index}\r\n"));
    }
}
