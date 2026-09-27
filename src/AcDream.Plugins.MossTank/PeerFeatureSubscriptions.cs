using AcDream.Plugin.Abstractions;
namespace AcDream.Plugins.MossTank;

internal sealed class PeerFeatureSubscriptions(INetworkAutomation network) : IDisposable
{
    private IDisposable? _lease;
    private PluginPeerCapabilities _current;
    internal void Update(PluginPeerCapabilities requested)
    {
        if (requested == _current) return;
        IDisposable? next = requested == PluginPeerCapabilities.None ? null : network.Subscribe(requested);
        _lease?.Dispose(); _lease = next; _current = requested;
    }
    public void Dispose() { _lease?.Dispose(); _lease = null; _current = PluginPeerCapabilities.None; }
}

internal static class MetaPeerRequirements
{
    internal static PluginPeerCapabilities Detect(MetaProfile profile)
    {
        var result = PluginPeerCapabilities.None;
        foreach (var rule in profile.Rules.Where(r => r.Enabled))
        { result |= Condition(rule.Condition); result |= Action(rule.Action); }
        return result;
    }
    private static PluginPeerCapabilities Text(string text)
    {
        var flags = PluginPeerCapabilities.None;
        if (System.Text.RegularExpressions.Regex.IsMatch(text, @"\bnetclients\s*\[", System.Text.RegularExpressions.RegexOptions.IgnoreCase | System.Text.RegularExpressions.RegexOptions.CultureInvariant)) flags |= PluginPeerCapabilities.ClientState;
        if (System.Text.RegularExpressions.Regex.IsMatch(text, @"\bnetcasts\s*\[", System.Text.RegularExpressions.RegexOptions.IgnoreCase | System.Text.RegularExpressions.RegexOptions.CultureInvariant)) flags |= PluginPeerCapabilities.Casts;
        return flags;
    }
    private static PluginPeerCapabilities Condition(MetaCondition condition) =>
        condition.Children.Aggregate(condition.Kind == MetaConditionKind.Expression ? Text(condition.Text) : PluginPeerCapabilities.None, (a, b) => a | Condition(b));
    private static PluginPeerCapabilities Action(MetaAction action) =>
        action.Children.Aggregate(action.Kind is MetaActionKind.ExpressionAction or MetaActionKind.ChatExpression ? Text(action.Text) : PluginPeerCapabilities.None, (a, b) => a | Action(b));
}
