using AcDream.Plugin.Abstractions;
namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private PeerFeatureSubscriptions? _peerSubscriptions;
    private PluginPeerCapabilities _metaPeerRequirements;
    private PluginPeerCapabilities _dynamicPeerRequirements;
    private MetaProfile? _peerRequirementsProfile;

    private DateTimeOffset _expressionPeersUntil;
    private bool MetaPeersReady => !_host.Automation.Network.SupportsSubscriptions
        || (_metaPeerRequirements | _dynamicPeerRequirements) == PluginPeerCapabilities.None
        || _host.Automation.Network.IsConnected;
    private void UpdatePeerSubscriptions()
    {
        _peerSubscriptions ??= new PeerFeatureSubscriptions(_host.Automation.Network);
        if (!_host.Automation.IsAvailable)
        { _peerSubscriptions.Dispose(); _dynamicPeerRequirements = PluginPeerCapabilities.None; return; }
        if (!ReferenceEquals(_peerRequirementsProfile, _metaProfile))
        {
            _peerRequirementsProfile = _metaProfile;
            _metaPeerRequirements = MetaPeerRequirements.Detect(_metaProfile);
        }
        var requested = PluginPeerCapabilities.None;
        if (_ubCatalog.Require("Sharing.Vitals").Get().Boolean) requested |= PluginPeerCapabilities.ClientState | PluginPeerCapabilities.Casts;
        if (_ubCatalog.Require("Networking.ReceiveCommands").Get().Boolean) requested |= PluginPeerCapabilities.Commands;
        if ((_host.HasUi && _networkHud.ShouldShow) || (_combat.Enabled && _vitalSettings.Enabled && _vitalSettings.HelpOthers && _vitalSettings.HelpNetworkPeers))
            requested |= PluginPeerCapabilities.ClientState;
        if (!_meta.Enabled && DateTimeOffset.UtcNow >= _expressionPeersUntil)
            _dynamicPeerRequirements = PluginPeerCapabilities.None;
        if (_meta.Enabled) requested |= _metaPeerRequirements;
        requested |= _dynamicPeerRequirements;
        _peerSubscriptions.Update(requested);
    }
    private void RequireExpressionPeers(PluginPeerCapabilities capabilities)
    {
        _dynamicPeerRequirements |= capabilities;
        _expressionPeersUntil = DateTimeOffset.UtcNow.AddSeconds(5);
        _peerSubscriptions ??= new PeerFeatureSubscriptions(_host.Automation.Network);
        UpdatePeerSubscriptions();
    }
}
