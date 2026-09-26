using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    public Action ShowRemote => () => SwitchPanel("main", "remote");
    public Action ShowMainWindow => () => SwitchPanel("remote", "main");
    public Action RemoteFollowSelected => FollowSelectedPlayer;

    private void SwitchPanel(string source, string destination)
    {
        // Keep the current controls available if the destination cannot open.
        if (_host.Ui.ShowPanel(destination))
            _host.Ui.HidePanel(source);
    }

    private void FollowSelectedPlayer()
    {
        uint selected = _host.Selection.SelectedObjectId ?? 0u;
        PluginWorldObject? player = _host.Automation.Objects.CaptureObjects()
            .Where(candidate => candidate.ObjectId == selected
                && candidate.ObjectId != _host.Automation.Character.ObjectId
                && PlayerClasses.Contains(candidate.ObjectClass))
            .Cast<PluginWorldObject?>().FirstOrDefault();
        if (player is not { } target)
        {
            WriteUbError("Select a live player to follow first.");
            return;
        }
        if (!FollowOnFollowRoute(target.ObjectId, target.Name))
        {
            WriteUbError($"Failed to follow {target.Name}[0x{target.ObjectId:X8}]");
            return;
        }
        WriteUb($"Following {target.Name}[0x{target.ObjectId:X8}]");
        if (!_navigationSettings.Enabled)
            WriteUb("Turn Enable Navigation on to start.");
    }
}
