using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class PeerFeatureSubscriptionsTests
{
    [Fact]
    public void ClientsOwnTheirRequestsAndDisabledFeaturesReleaseThem()
    {
        var first = new Network();
        var second = new Network();
        using var one = new PeerFeatureSubscriptions(first);
        using var two = new PeerFeatureSubscriptions(second);
        one.Update(PluginPeerCapabilities.None);
        Assert.Empty(first.Requests);
        one.Update(PluginPeerCapabilities.Casts);
        one.Update(PluginPeerCapabilities.Casts);
        Assert.Single(first.Requests);
        Assert.Empty(second.Requests);
        one.Update(PluginPeerCapabilities.Casts | PluginPeerCapabilities.Commands);
        Assert.Equal(1, first.Active);
        one.Update(PluginPeerCapabilities.None);
        Assert.Equal(0, first.Active);
    }

    [Fact]
    public void MetaRequirementsFollowEnabledNestedCallsRatherThanSimilarWords()
    {
        var profile = new MetaProfile
        {
            Rules =
            [
                new() { Condition = new() { Kind = MetaConditionKind.Expression, Text = "netclients []" }, Action = new() { Children = [new() { Kind = MetaActionKind.ExpressionAction, Text = "netcasts[tag]" }] } },
                new() { Enabled = false, Condition = new() { Kind = MetaConditionKind.Expression, Text = "netclients[]" } },
            ],
        };
        Assert.Equal(PluginPeerCapabilities.ClientState | PluginPeerCapabilities.Casts, MetaPeerRequirements.Detect(profile));
        profile.Rules[0].Enabled = false;
        Assert.Equal(PluginPeerCapabilities.None, MetaPeerRequirements.Detect(profile));
        profile.Rules.Add(new() { Condition = new() { Kind = MetaConditionKind.Expression, Text = "mynetclients[]" } });
        Assert.Equal(PluginPeerCapabilities.None, MetaPeerRequirements.Detect(profile));
    }

    private sealed class Network : INetworkAutomation
    {
        internal List<PluginPeerCapabilities> Requests { get; } = [];
        internal int Active;
        public IDisposable Subscribe(PluginPeerCapabilities capabilities)
        { Requests.Add(capabilities); Active++; return new Lease(this); }
        private sealed class Lease(Network owner) : IDisposable
        {
            private bool _disposed;
            public void Dispose() { if (_disposed) return; _disposed = true; owner.Active--; }
        }
    }
}
