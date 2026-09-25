using System.Globalization;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class VtankMetaProfileSerializerTests
{
    [Fact]
    public void LoadsKnownTypedCondActRecord()
    {
        const string source = "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\n"
            + "CData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n1\r\n"
            + "i\r\n1\r\ni\r\n2\r\ni\r\n0\r\ns\r\n/say ready\r\n"
            + "s\r\nDefault\r\n";

        Assert.True(VtankMetaProfileSerializer.TryLoad(
            source, out MetaProfile profile, out string error), error);

        MetaRule rule = Assert.Single(profile.Rules);
        Assert.Equal(MetaConditionKind.Always, rule.Condition.Kind);
        Assert.Equal(MetaActionKind.ChatCommand, rule.Action.Kind);
        Assert.Equal("/say ready", rule.Action.Text);
        Assert.Equal("Default", rule.State);
    }

    /// <summary>
    /// An embedded route's stated length counts the characters of the whole
    /// serialized route, so a long route runs past any sensible bound on a
    /// number of rules or nodes: a 1,742-node route is over 111,000
    /// characters. Mutation: bounding the length like a collection count
    /// refuses the file with "Invalid VTank Meta collection count".
    /// </summary>
    [Fact]
    public void AnEmbeddedRouteLongerThanTheCollectionBoundLoads()
    {
        const int nodes = 2_000;
        var route = new System.Text.StringBuilder();
        route.Append("uTank2 NAV 1.2\r\n4\r\n")
            .Append(nodes.ToString(CultureInfo.InvariantCulture)).Append("\r\n");
        for (int index = 0; index < nodes; index++)
        {
            route.Append("0\r\n")
                .Append((44.0861417770386 + index / 1000d).ToString("R", CultureInfo.InvariantCulture)).Append("\r\n")
                .Append("-41.2450457890828\r\n0.483354187011719\r\n0\r\n");
        }
        string name = "long";
        int serializedCharacters = name.Length + 2
            + nodes.ToString(CultureInfo.InvariantCulture).Length + 2
            + route.Length;
        Assert.True(serializedCharacters > 100_000);
        string source = "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\n"
            + "CData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n1\r\n"
            + "i\r\n1\r\ni\r\n4\r\ni\r\n0\r\n"
            + "ba\r\n" + serializedCharacters.ToString(CultureInfo.InvariantCulture) + "\r\n"
            + name + "\r\n" + nodes.ToString(CultureInfo.InvariantCulture) + "\r\n"
            + route
            + "s\r\nDefault\r\n";

        Assert.True(VtankMetaProfileSerializer.TryLoad(
            source, out MetaProfile profile, out string error), error);

        MetaRule rule = Assert.Single(profile.Rules);
        Assert.Equal(MetaActionKind.LoadEmbeddedNavigationRoute, rule.Action.Kind);
        Assert.Equal(nodes, rule.Action.EmbeddedRoute!.Waypoints.Count);
    }

    [Fact]
    public void SignedHighBitLandblockIdRoundTripsExactly()
    {
        int highBitValue = unchecked((int)0x8B370000u);
        Assert.True(highBitValue < 0, "0x8B370000 must decode as negative once cast to Int32.");
        string source = "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\n"
            + "CData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n1\r\n"
            + "i\r\n17\r\ni\r\n0\r\n"
            + "i\r\n" + highBitValue.ToString(CultureInfo.InvariantCulture) + "\r\n"
            + "i\r\n0\r\n"
            + "s\r\nDefault\r\n";

        Assert.True(VtankMetaProfileSerializer.TryLoad(
            source, out MetaProfile profile, out string error), error);

        MetaRule rule = Assert.Single(profile.Rules);
        Assert.Equal(MetaConditionKind.LandblockEquals, rule.Condition.Kind);
        Assert.Equal((double)highBitValue, rule.Condition.Number);
    }
}
