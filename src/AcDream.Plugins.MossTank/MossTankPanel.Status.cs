using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    /// <summary>
    /// The status line other plugins read for the macro's current meta
    /// state, the value VTank's external interface reported as its current
    /// meta state.
    /// </summary>
    internal const string MetaStateStatusKey = "metaState";

    private string? _publishedMetaState;

    /// <summary>Publishes the meta state on the host's status board when it changes.</summary>
    private void PublishStatus()
    {
        string state = _meta.CurrentState;
        if (string.Equals(state, _publishedMetaState, StringComparison.Ordinal))
            return;
        IPluginStatusBoard board = _host.StatusBoard;
        if (!board.IsAvailable)
            return;
        if (board.Publish(MetaStateStatusKey, state))
            _publishedMetaState = state;
    }
}
